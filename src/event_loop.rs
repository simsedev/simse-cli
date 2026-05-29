//! CLI runtime — wires the async event loop to ACP, tools, and conversation.
//!
//! This module provides [`CliRuntime`], the high-level async runtime that
//! sits between the terminal event loop in `main.rs` and the ACP engine.
//! It manages the ACP client connection, conversation state, tool registry,
//! permission handling, and command dispatch.
//!
//! The actual terminal event loop (crossterm `read_event` + ratatui `draw`)
//! remains in `main.rs`. This module provides the runtime that `main.rs`
//! orchestrates.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

// simse-core types
use simse_core::acp::client::{
    AcpClient as AcpEngine, AcpConfig as AcpEngineConfig, McpServerEntry, ServerEntry,
};
use simse_core::acp::error::AcpError;
use simse_core::acp::permission::PermissionPolicy;
use simse_core::agentic_loop::{
    self, AgenticLoopOptions, CancellationToken, LoopCallbacks, Message, MessageRole,
};
use simse_core::remote::tunnel::TunnelClient;
use simse_core::tools::types::ToolContext;
use simse_core::tools::{ToolCallRequest, ToolRegistry, ToolRegistryOptions};

use crate::ui_core::state::conversation::{ConversationBuffer, ConversationOptions};
use crate::ui_core::state::permission_manager::PermissionManager;
use crate::ui_core::state::permissions::PermissionMode;

use crate::app::AppMessage;
use crate::commands::{BridgeAction, CommandContext};
use crate::config::{AcpServerConfig, LoadedConfig};
use crate::session_store::SessionStore;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors that can occur in the CLI runtime.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("Not connected to ACP server")]
    NotConnected,
    // Generic CLI-runtime error (login, tunnel, remote inference, sessions).
    // NOT ACP-specific — the message is self-describing, so no prefix.
    #[error("{0}")]
    Acp(String),
    #[error("No ACP servers configured")]
    NoServersConfigured,
    #[error("ACP server not found: {0}")]
    ServerNotFound(String),
    #[error("No active session")]
    NoSession,
    #[error("I/O error: {0}")]
    Io(String),
}

// ---------------------------------------------------------------------------
// CliRuntime
// ---------------------------------------------------------------------------

/// The high-level CLI runtime that wires ACP, tools, and conversation together.
///
/// This struct owns all the state needed to drive an agentic loop from the CLI.
/// The terminal event loop in `main.rs` calls methods on this struct to connect,
/// submit prompts, handle permissions, and abort.
pub struct CliRuntime {
    /// Loaded configuration (ACP servers, MCP servers, library, etc.).
    config: LoadedConfig,
    /// ACP engine connection (set by `connect()` for default simse inference).
    acp_engine: Option<Arc<AcpEngine>>,
    /// The model client used for generation. Set either by `connect()` (ACP engine)
    /// or by `set_model_client()` (--provider). Both paths produce a `dyn AcpClient`.
    model_client: Option<std::sync::Arc<dyn simse_core::agentic_loop::AcpClient>>,
    /// Conversation state buffer.
    conversation: ConversationBuffer,
    /// Tool registry with discovered tools.
    tool_registry: std::sync::Arc<ToolRegistry>,
    /// Permission manager for tool call authorization.
    permission_manager: PermissionManager,
    /// Active ACP session ID.
    session_id: Option<String>,
    /// Cancellation token shared with the agentic loop.
    cancel_token: CancellationToken,
    /// Whether verbose mode is enabled.
    pub verbose: bool,
    /// Session persistence store.
    session_store: SessionStore,
    /// gRPC tunnel to the API relay (None until login or startup auto-connect).
    tunnel: Option<Arc<TunnelClient>>,
    /// Background task handling incoming tunnel messages.
    tunnel_handler: Option<tokio::task::JoinHandle<()>>,
    /// Accumulated token count across agentic loop runs.
    total_tokens: u64,
    /// Host-backed sandbox context — passed to every tool execution so
    /// tools run through the sandbox pattern against the local machine.
    tool_context: Arc<ToolContext>,
    /// Deferred dependency cell shared with the `subagent_spawn` /
    /// `subagent_delegate` tool runners. Filled by `fill_subagent_deps`
    /// once the model client (and, if available, the ACP engine) is settled.
    subagent_deps: crate::tools::subagent_cli::DeferredCliSubagent,
}

impl CliRuntime {
    /// Create a new CLI runtime from a loaded configuration.
    pub fn new(config: LoadedConfig) -> Self {
        let session_store = SessionStore::new(&config.data_dir);
        let mut tool_registry = ToolRegistry::new(ToolRegistryOptions::default());

        // Register sandbox tools (same as managed/server.rs).
        let work_dir = config.work_dir.clone();
        simse_core::tools::sandboxed::bash::register_bash_tool(
            &mut tool_registry,
            simse_core::tools::sandboxed::bash::BashToolOptions {
                working_directory: work_dir.clone(),
                default_timeout_ms: Some(30_000),
                max_output_bytes: Some(100_000),
                shell: None,
            },
        );
        simse_core::tools::sandboxed::filesystem::register_filesystem_tools(
            &mut tool_registry,
            simse_core::tools::sandboxed::filesystem::FilesystemToolOptions {
                working_directory: work_dir,
                allowed_paths: None,
            },
        );
        simse_core::tools::sandboxed::network::register_network_tools(&mut tool_registry);
        simse_core::tools::sandboxed::git::register_git_tools(&mut tool_registry);
        // Register the wall-clock tool (`now`).
        simse_core::tools::sandboxed::time::register_time_tool(&mut tool_registry);
        crate::tools::memory::register_memory_tools(&mut tool_registry);

        // Register todo/task-tracking tools (task_create / task_get /
        // task_update / task_delete / task_list). Backed by an in-process
        // `TaskList` shared across handlers for the lifetime of the runtime.
        simse_core::tools::builtin::register_task_tools(
            &mut tool_registry,
            std::sync::Arc::new(std::sync::Mutex::new(simse_core::tasks::TaskList::new(
                None,
            ))),
        );

        // Register subagent_spawn / subagent_delegate. The runners hold their
        // dependencies in a deferred cell, filled later by `fill_subagent_deps`
        // once the model client exists (a tool handler cannot borrow `self`).
        let subagent_deps = crate::tools::subagent_cli::deferred_cli_subagent();
        simse_core::tools::subagent::register_subagent_tools(
            &mut tool_registry,
            &simse_core::tools::subagent::SubagentToolsOptions {
                loop_runner: std::sync::Arc::new(crate::tools::subagent_cli::CliSubagentRunner {
                    deps: subagent_deps.clone(),
                }),
                delegate_runner: std::sync::Arc::new(
                    crate::tools::subagent_cli::CliDelegateRunner {
                        deps: subagent_deps.clone(),
                    },
                ),
                callbacks: None,
                default_max_turns: 10,
                max_depth: 2,
                system_prompt: None,
            },
            0,
        );

        // Host-backed sandbox context. Falls back to an in-memory context
        // if the work dir is unusable, so the CLI still starts.
        let tool_context = Arc::new(
            ToolContext::host(config.work_dir.clone()).unwrap_or_else(|e| {
                // CLI doesn't initialize tracing_subscriber, so a tracing::warn
                // here is swallowed and the silent fallback to ToolContext::local
                // (in-memory VFS, no disk persistence) is invisible — every
                // fs_write reports `[ok]` but the file never lands on disk.
                // Promote to eprintln so the operator at least sees why their
                // workspace looks empty after fs_write.
                eprintln!(
                    "[simse-cli] WARN: host sandbox context unavailable, falling back to in-memory VFS: {e}"
                );
                ToolContext::local(config.work_dir.clone())
            }),
        );

        Self {
            config,
            acp_engine: None,
            model_client: None,
            conversation: ConversationBuffer::new(ConversationOptions::default()),
            tool_registry: std::sync::Arc::new(tool_registry),
            permission_manager: PermissionManager::new(PermissionMode::Default),
            session_id: None,
            cancel_token: CancellationToken::new(),
            verbose: false,
            session_store,
            tunnel: None,
            tunnel_handler: None,
            total_tokens: 0,
            tool_context,
            subagent_deps,
        }
    }

    /// Populate the sub-agent runners' deferred deps. Safe to call repeatedly;
    /// the `OnceCell` ignores writes after the first. No-op until the model
    /// client is set.
    ///
    /// Ordering note: `acp_engine` is set only by `init_tools` / `McpRestart`,
    /// neither of which runs in the startup path (`create_runtime` calls
    /// `init_plugins` then `init_model_client`, never `init_tools`). The model
    /// client is therefore set first at startup, so the cell normally captures
    /// `acp_engine: None` — the delegate runner falls back to single-shot
    /// generation, which is correct since no ACP engine exists yet. Calling
    /// this at the end of `init_tools` too lets a user who connects ACP
    /// *before* a model client still get the engine captured.
    fn fill_subagent_deps(&self) {
        if let Some(client) = &self.model_client {
            let registry: std::sync::Arc<dyn simse_core::agentic_loop::ToolExecutor> =
                std::sync::Arc::clone(&self.tool_registry) as _;
            let _ = self
                .subagent_deps
                .set(crate::tools::subagent_cli::CliSubagentDeps {
                    model_client: std::sync::Arc::clone(client),
                    tool_registry: registry,
                    tool_context: std::sync::Arc::clone(&self.tool_context),
                    acp_engine: self.acp_engine.clone(),
                    system_prompt: None,
                    cancel_token: self.cancel_token.clone(),
                });
        }
    }

    /// Set an external model client (from `--provider`).
    /// This bypasses ACP engine connect — the client is used directly.
    pub fn set_model_client(
        &mut self,
        client: std::sync::Arc<dyn simse_core::agentic_loop::AcpClient>,
    ) {
        self.model_client = Some(client);
        self.fill_subagent_deps();
    }

    /// Load and register all plugins (skills, hooks, ACP, MCP).
    ///
    /// Called during startup before the tool registry is shared with any
    /// sub-agent runners, so `Arc::get_mut` is guaranteed to succeed.
    pub async fn init_plugins(&mut self) {
        let plugins_dir = self.config.data_dir.join("plugins");
        let Some(registry) = std::sync::Arc::get_mut(&mut self.tool_registry) else {
            tracing::warn!("init_plugins: tool registry already shared, skipping plugin load");
            return;
        };
        crate::plugin_loader::load_all_plugins(registry, &plugins_dir).await;
    }

    /// API endpoint for the remote tunnel gRPC stream.
    const API_URL: &'static str = "https://api.simse.dev";

    /// Connect the gRPC tunnel to the API relay using stored credentials.
    pub async fn connect_tunnel(
        &mut self,
    ) -> Result<(String, tokio::sync::mpsc::UnboundedReceiver<String>), RuntimeError> {
        if self.tunnel.is_some() {
            self.disconnect_tunnel().await;
        }

        let auth =
            crate::auth::load_auth().ok_or_else(|| RuntimeError::Acp("Not logged in".into()))?;

        let relay_url = Self::API_URL;

        let tunnel = Arc::new(TunnelClient::new());
        let incoming_rx = tunnel.subscribe_messages().await;

        let tunnel_id = tunnel
            .connect(relay_url, &auth.access_token)
            .await
            .map_err(|e| RuntimeError::Acp(format!("Tunnel connect failed: {e}")))?;

        self.tunnel = Some(tunnel);
        Ok((tunnel_id, incoming_rx))
    }

    /// Store the message handler task handle so we can abort it on disconnect.
    pub fn set_tunnel_handler(&mut self, handle: tokio::task::JoinHandle<()>) {
        self.tunnel_handler = Some(handle);
    }

    /// Disconnect the tunnel and abort the message handler task.
    pub async fn disconnect_tunnel(&mut self) {
        if let Some(handle) = self.tunnel_handler.take() {
            handle.abort();
        }
        if let Some(ref tunnel) = self.tunnel {
            let _ = tunnel.disconnect().await;
        }
        self.tunnel = None;
    }

    /// Whether the tunnel is currently connected.
    pub fn tunnel_connected(&self) -> bool {
        self.tunnel
            .as_ref()
            .map(|t| t.is_connected())
            .unwrap_or(false)
    }

    /// Get a reference to the tunnel client, if connected.
    pub fn tunnel(&self) -> Option<&Arc<TunnelClient>> {
        self.tunnel.as_ref()
    }

    /// Connect to the remote inference service via the API gateway.
    ///
    /// Requires the user to be logged in (auth token must exist).
    pub async fn connect_inference(&mut self) -> Result<(), RuntimeError> {
        self.connect_inference_with_model(None).await
    }

    /// Connect to the native simse inference path. `model_override` (from the
    /// `--model` flag) wins over the config default_agent; both fall back to
    /// "" which tells the router to pick the default tier. Lets
    /// `simse --model zoysia` drive the large tier for harder tasks.
    pub async fn connect_inference_with_model(
        &mut self,
        model_override: Option<String>,
    ) -> Result<(), RuntimeError> {
        let auth = crate::auth::load_auth()
            .ok_or_else(|| RuntimeError::Acp("Not logged in. Run `simse login` first.".into()))?;

        // Default tier is zoysia (the capable model). rye (4B) is too weak
        // for agentic tool use, so an unspecified model must NOT fall through
        // to the router's default node — pin it to zoysia explicitly.
        let model = model_override
            .or_else(|| self.config.default_agent.clone())
            .unwrap_or_else(|| "zoysia".to_string());

        // Tier-based tool gating: rye is a chat-only tier. It may ONLY use the
        // read-only research tools (web_search, web_fetch); all
        // filesystem/shell/git/etc tools are removed so it cannot attempt
        // (and fail) agentic actions it is too weak to drive reliably.
        if simse_core::prompts::tier_prompt::tier_for_model(Some(&model)) == "rye" {
            // Fail CLOSED: rye must be chat-only. If the registry Arc is already
            // shared (get_mut → None) we cannot enforce the restriction, so
            // refuse to connect rather than silently serve rye with full tools.
            match std::sync::Arc::get_mut(&mut self.tool_registry) {
                Some(registry) => registry.restrict_to(&["web_search", "web_fetch"]),
                None => {
                    return Err(RuntimeError::Acp(
                        "cannot enforce rye chat-only tool restriction (tool registry already \
                         shared); refusing to connect rye un-gated"
                            .into(),
                    ));
                }
            }
        }

        let client =
            crate::inference_client::RemoteInferenceClient::connect(&auth.access_token, model)
                .await
                .map_err(RuntimeError::Acp)?;

        self.model_client = Some(std::sync::Arc::new(client));
        self.fill_subagent_deps();
        Ok(())
    }

    /// Discover and register tools from configured ACP servers.
    ///
    /// Does NOT set the model client — ACP is for tools only.
    pub async fn init_tools(&mut self) -> Result<(), RuntimeError> {
        if self.config.acp.servers.is_empty() {
            return Ok(());
        }

        let server_config = self.resolve_server(None)?;

        let acp_config = AcpEngineConfig {
            servers: vec![ServerEntry {
                name: server_config.name.clone(),
                command: server_config.command.clone(),
                args: server_config.args.clone(),
                cwd: server_config.cwd.clone(),
                env: server_config.env.clone(),
                default_agent: server_config.default_agent.clone(),
                timeout_ms: server_config.timeout_ms,
                permission_policy: Some(PermissionPolicy::AutoApprove),
            }],
            default_server: Some(server_config.name.clone()),
            default_agent: self.config.default_agent.clone(),
            mcp_servers: vec![],
        };

        let engine = AcpEngine::new(acp_config)
            .await
            .map_err(|e: AcpError| RuntimeError::Acp(e.to_string()))?;

        self.acp_engine = Some(Arc::new(engine));
        // ACP is for tools only — do NOT set model_client here.
        // Capture the freshly-set engine into the subagent deps. This is a
        // no-op if the model client is not yet set (cell stays empty) or if
        // the cell was already filled by an earlier model-client call.
        self.fill_subagent_deps();
        Ok(())
    }

    /// Build the CLIENT context system message: the workspace SIMSE.md (if
    /// present) plus this client's tool definitions. This is client-specific
    /// data — which tools this CLI offers — NOT behavioral rules. The engine
    /// (loom) owns behavioral rules via the tier system prompt (shared.md);
    /// it prepends them and merges this client context after. Returns `None`
    /// when there is nothing client-specific to send.
    fn build_system_prompt(&self) -> Option<String> {
        let mut prompt = String::new();
        if let Some(ws) = &self.config.workspace_prompt {
            prompt.push_str(ws);
        }
        let tools_section = self.tool_registry.format_for_system_prompt();
        if !tools_section.is_empty() {
            if !prompt.is_empty() {
                prompt.push_str("\n\n");
            }
            prompt.push_str(&tools_section);
        }
        if prompt.is_empty() {
            None
        } else {
            Some(prompt)
        }
    }

    /// Handle a user submission: run the agentic loop with the given input.
    ///
    /// The input is added to the conversation and the agentic loop is run.
    /// Returns the final text response from the loop.
    pub async fn handle_submit(
        &mut self,
        input: &str,
        callbacks: LoopCallbacks,
    ) -> Result<String, RuntimeError> {
        if self.model_client.is_none() {
            return Err(RuntimeError::NotConnected);
        }

        self.update_conversation(|c| c.add_user(input));
        self.cancel_token = CancellationToken::new();

        // Thin client: behavioral rules, doom-loop detection, and context
        // compaction are all owned by the inference engine (loom). The CLI
        // only orchestrates tool execution + streaming. system_prompt here
        // carries client context (tools/workspace), which the engine merges
        // after the tier prompt.
        let options = AgenticLoopOptions {
            max_turns: crate::constants::MAX_AGENTIC_TURNS,
            system_prompt: self.build_system_prompt(),
            agent_manages_tools: false,
            ..Default::default()
        };

        let conv_messages = self.conversation.to_messages();
        let mut loop_messages: Vec<Message> = conv_messages
            .iter()
            .map(|m| {
                let role = match m.role {
                    simse_core::conversation::Role::User => MessageRole::User,
                    simse_core::conversation::Role::Assistant => MessageRole::Assistant,
                    simse_core::conversation::Role::System => MessageRole::System,
                    simse_core::conversation::Role::ToolResult => MessageRole::User,
                };
                Message {
                    role,
                    content: m.content.clone(),
                    images: Vec::new(),
                }
            })
            .collect();

        let client = self.model_client.as_ref().unwrap();
        let result = agentic_loop::run_agentic_loop(
            client.as_ref(),
            self.tool_registry.as_ref(),
            &mut loop_messages,
            options,
            Some(callbacks),
            Some(self.cancel_token.clone()),
            None,
            None,
            Some(Arc::clone(&self.tool_context)),
        )
        .await;

        match result {
            Ok(loop_result) => {
                if !loop_result.final_text.is_empty() {
                    let conv = std::mem::replace(
                        &mut self.conversation,
                        ConversationBuffer::new(ConversationOptions::default()),
                    );
                    self.conversation = conv.add_assistant(&loop_result.final_text);
                }
                Ok(loop_result.final_text)
            }
            Err(e) => Err(RuntimeError::Acp(e.to_string())),
        }
    }

    /// Handle a permission response from the user.
    ///
    /// Forwards the permission decision to the ACP engine if connected.
    ///
    /// **Note:** This is not yet wired to the ACP engine's permission flow.
    /// The ACP connection layer resolves permissions via `PermissionPolicy`
    /// (set to `AutoApprove` on connect), so the agent never sends
    /// `request_permission` requests that would require this callback.
    /// When interactive ACP permission prompts are implemented, this method
    /// will need to send the user's decision back through a channel to the
    /// `SimseClient::request_permission` handler in `connection.rs`.
    pub async fn handle_permission_response(
        &mut self,
        _request_id: &str,
        _option_id: &str,
    ) -> Result<(), RuntimeError> {
        let _engine = self.acp_engine.as_ref().ok_or(RuntimeError::NotConnected)?;

        Err(RuntimeError::Acp(
            "ACP permission response is not yet wired up. \
             The CLI currently uses AutoApprove policy, so this path \
             should not be reached in normal operation."
                .to_string(),
        ))
    }

    /// Cancel the current agentic loop at the next check point.
    pub fn abort(&self) {
        self.cancel_token.cancel();
    }

    /// Check if the cancellation token has been triggered.
    pub fn is_aborted(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Pure onboarding decision — extracted from `needs_onboarding` so it is
    /// unit-testable without reading the auth file on disk.
    fn onboarding_needed(has_model_client: bool, has_auth: bool) -> bool {
        !has_model_client && !has_auth
    }

    /// Check if onboarding is needed (not logged in and no --provider).
    pub fn needs_onboarding(&self) -> bool {
        Self::onboarding_needed(
            self.model_client.is_some(),
            crate::auth::load_auth().is_some(),
        )
    }

    /// Check if a model client is available (either ACP engine or --provider).
    pub fn is_connected(&self) -> bool {
        self.model_client.is_some()
    }

    /// Check if the model client is available.
    pub async fn is_healthy(&self) -> bool {
        self.model_client.is_some()
    }

    /// Get the current session ID, if any.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Set the session ID (used when resuming a session from CLI args).
    pub fn set_session_id(&mut self, id: String) {
        self.session_id = Some(id);
    }

    /// Find the most recent session for a given working directory.
    pub fn latest_session(&self, work_dir: &str) -> Option<String> {
        self.session_store.latest(work_dir)
    }

    /// Check whether a session exists in the store.
    pub fn session_exists(&self, id: &str) -> bool {
        self.session_store.get(id).is_some()
    }

    /// Load a session's messages and restore them into the conversation buffer.
    ///
    /// Sets the session ID and returns the loaded messages for display.
    /// Returns an error if the session is not found.
    pub fn load_session(
        &mut self,
        id: &str,
    ) -> Result<Vec<crate::session_store::SessionMessage>, RuntimeError> {
        let _meta = self
            .session_store
            .get(id)
            .ok_or_else(|| RuntimeError::Acp(format!("Session not found: {id}")))?;
        let messages = self.session_store.load(id);
        let mut conv = ConversationBuffer::new(ConversationOptions::default());
        for msg in &messages {
            conv = match msg.role.as_str() {
                "user" => conv.add_user(&msg.content),
                "assistant" => conv.add_assistant(&msg.content),
                _ => conv,
            };
        }
        self.conversation = conv;
        self.session_id = Some(id.to_string());
        Ok(messages)
    }

    /// Ensure a session exists for persistence, creating one bound to the
    /// given work_dir if none is active. Returns the session id. Used by
    /// print mode so a `simse --print` run is resumable with `--continue`.
    pub fn ensure_session(&mut self, work_dir: &str) -> Result<String, RuntimeError> {
        if let Some(id) = &self.session_id {
            return Ok(id.clone());
        }
        let id = self
            .session_store
            .create(work_dir)
            .map_err(|e| RuntimeError::Acp(format!("Failed to create session: {e}")))?;
        self.session_id = Some(id.clone());
        Ok(id)
    }

    /// Append a single message to the active session's on-disk log. No-op
    /// when no session is active. Lets print mode persist each turn so the
    /// conversation survives for `--continue` / `resume`.
    pub fn persist_message(&self, role: &str, content: &str) {
        let Some(id) = &self.session_id else {
            return;
        };
        let msg = crate::session_store::SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            tool_call_id: None,
            tool_name: None,
        };
        if let Err(e) = self.session_store.append(id, &msg) {
            eprintln!("warning: failed to persist session message: {e}");
        }
    }

    /// Get a reference to the conversation buffer.
    pub fn conversation(&self) -> &ConversationBuffer {
        &self.conversation
    }

    /// Get a mutable reference to the conversation buffer.
    pub fn conversation_mut(&mut self) -> &mut ConversationBuffer {
        &mut self.conversation
    }

    /// Apply a functional transformation to the conversation buffer.
    ///
    /// Takes the current buffer, passes it to the closure, and stores the
    /// result back. This avoids the borrow-and-consume conflict inherent
    /// in owned-return methods called through `&mut self`.
    pub fn update_conversation(
        &mut self,
        f: impl FnOnce(ConversationBuffer) -> ConversationBuffer,
    ) {
        let conv = std::mem::replace(
            &mut self.conversation,
            ConversationBuffer::new(ConversationOptions::default()),
        );
        self.conversation = f(conv);
    }

    /// Get a reference to the tool registry.
    pub fn tool_registry(&self) -> &ToolRegistry {
        self.tool_registry.as_ref()
    }

    /// Get a reference to the permission manager.
    pub fn permission_manager(&self) -> &PermissionManager {
        &self.permission_manager
    }

    /// Get a mutable reference to the permission manager.
    pub fn permission_manager_mut(&mut self) -> &mut PermissionManager {
        &mut self.permission_manager
    }

    /// Get a reference to the loaded configuration.
    pub fn config(&self) -> &LoadedConfig {
        &self.config
    }

    /// Get the cancellation token for sharing with async tasks.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Get the agent name from the ACP engine, if available.
    pub fn agent_name(&self) -> Option<String> {
        // The simse-acp AcpClient does not expose agent_info in the same way
        // as the bridge. Return None for now.
        None
    }

    /// Connected ACP server name, if any.
    pub fn server_name(&self) -> Option<String> {
        self.config.default_server.clone()
    }

    /// Active model/agent name, if any.
    pub fn model_name(&self) -> Option<String> {
        self.config.default_agent.clone()
    }

    /// Accumulated token count across loop runs.
    pub fn total_tokens(&self) -> u64 {
        self.total_tokens
    }

    /// Whether the agentic loop is currently running.
    ///
    /// Note: the runtime lock is held for the duration of the agentic loop,
    /// so this is only observable when the loop is NOT running. Always returns
    /// false — callers should use `App::loop_status` for UI-level loop state.
    pub fn is_loop_active(&self) -> bool {
        // The runtime mutex is held while the loop runs, making this
        // unreachable during active execution. Return false (the only
        // observable state).
        false
    }

    /// Clear the conversation and start fresh.
    pub fn reset_conversation(&mut self) {
        self.conversation = ConversationBuffer::new(ConversationOptions::default());
    }

    /// Build a `CommandContext` snapshot from the current runtime state.
    ///
    /// This creates a read-only snapshot that command handlers can use for
    /// sync operations (listing sessions, tools, config, etc.).
    pub fn build_command_context(&self) -> CommandContext {
        CommandContext {
            server_name: self.config.default_server.clone(),
            model_name: self.config.default_agent.clone(),
            session_id: self.session_id.clone(),
            acp_connected: self.is_connected(),
        }
    }

    /// Execute a bridge action asynchronously.
    ///
    /// Returns a human-readable result string on success, or an error message.
    pub async fn execute_bridge_action(
        &mut self,
        action: BridgeAction,
    ) -> Result<String, RuntimeError> {
        match action {
            BridgeAction::SwitchServer { name } => {
                self.init_tools().await?;
                Ok(format!("ACP tools reloaded from: {name}"))
            }
            BridgeAction::SwitchModel { name } => {
                self.config.default_agent = Some(name.clone());
                Ok(format!("Model set to: {name}"))
            }
            BridgeAction::ResumeSession { id } => {
                let resolve_id = if id.is_empty() {
                    let work_dir = self.config.work_dir.display().to_string();
                    self.session_store.latest(&work_dir).ok_or_else(|| {
                        RuntimeError::Acp("No session found for this directory.".into())
                    })?
                } else {
                    id
                };
                let meta = self
                    .session_store
                    .get(&resolve_id)
                    .ok_or_else(|| RuntimeError::Acp(format!("Session not found: {resolve_id}")))?;
                let messages = self.session_store.load(&resolve_id);
                let mut conv = ConversationBuffer::new(ConversationOptions::default());
                for msg in &messages {
                    conv = match msg.role.as_str() {
                        "user" => conv.add_user(&msg.content),
                        "assistant" => conv.add_assistant(&msg.content),
                        _ => conv,
                    };
                }
                self.conversation = conv;
                self.session_id = Some(resolve_id);
                Ok(format!("Resumed session: {}", meta.title))
            }
            BridgeAction::ForkSession { at } => {
                let sid = self
                    .session_id
                    .as_ref()
                    .ok_or(RuntimeError::Acp("No active session to fork.".into()))?;
                let new_id = self.session_store.fork(sid, at)?;
                Ok(format!("Forked → {new_id}. Resume with: /resume {new_id}"))
            }
            BridgeAction::McpRestart => {
                // Restart ACP with MCP server configurations.
                // The ACP engine forwards MCP server entries to the agent
                // via session/new, so we rebuild the connection with them.
                let mcp_entries: Vec<McpServerEntry> = self
                    .config
                    .mcp_servers
                    .iter()
                    .map(|s| McpServerEntry {
                        name: s.name.clone(),
                        config: serde_json::json!({
                            "transport": s.transport,
                            "command": s.command,
                            "url": s.url,
                            "args": s.args,
                            "env": s.env,
                        }),
                    })
                    .collect();

                let mcp_count = mcp_entries.len();

                if mcp_count == 0 {
                    return Ok("No MCP servers configured.".into());
                }

                // Dispose existing ACP engine if connected.
                if let Some(ref engine) = self.acp_engine {
                    let _ = engine.dispose().await;
                }
                self.acp_engine = None;

                // Reconnect with MCP servers included.
                let server_config = self.resolve_server(None)?;
                let acp_config = AcpEngineConfig {
                    servers: vec![ServerEntry {
                        name: server_config.name.clone(),
                        command: server_config.command.clone(),
                        args: server_config.args.clone(),
                        cwd: server_config.cwd.clone(),
                        env: server_config.env.clone(),
                        default_agent: server_config.default_agent.clone(),
                        timeout_ms: server_config.timeout_ms,
                        permission_policy: Some(PermissionPolicy::AutoApprove),
                    }],
                    default_server: Some(server_config.name.clone()),
                    default_agent: self.config.default_agent.clone(),
                    mcp_servers: mcp_entries,
                };

                let engine = AcpEngine::new(acp_config)
                    .await
                    .map_err(|e: AcpError| RuntimeError::Acp(e.to_string()))?;

                self.acp_engine = Some(Arc::new(engine));

                Ok(format!(
                    "MCP restarted. {mcp_count} server{} reconnected.",
                    if mcp_count == 1 { "" } else { "s" }
                ))
            }
            BridgeAction::AcpRestart => {
                self.init_tools().await?;
                Ok("ACP connection restarted.".into())
            }
            BridgeAction::DiffFiles { path } => {
                let params = match path {
                    Some(p) => serde_json::json!({"path": p, "diff": true}),
                    None => serde_json::json!({"diff": true}),
                };
                self.call_tool("vfs_read", params).await
            }

            // ── Meta ────────────────────────────────────
            BridgeAction::Compact => {
                let msg_count = self.conversation.to_messages().len();
                let conv = std::mem::replace(
                    &mut self.conversation,
                    ConversationBuffer::new(ConversationOptions::default()),
                );
                self.conversation =
                    conv.compact("[User-requested compaction: conversation history summarized]");
                Ok(format!(
                    "Conversation compacted ({msg_count} messages → 1 summary)."
                ))
            }
            BridgeAction::Login => {
                let device_name = hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "simse-cli".to_string());

                let client = simse_core::remote::auth::AuthClient::new();
                match client
                    .login_device(
                        crate::auth_cmd::AUTH_URL,
                        crate::auth_cmd::API_URL,
                        Some(device_name),
                    )
                    .await
                {
                    Ok((_client, state)) => {
                        crate::auth::save_auth(&state)
                            .map_err(|e| RuntimeError::Acp(e.to_string()))?;
                        Ok(format!("Logged in as {}", state.email))
                    }
                    Err((_client, e)) => Err(RuntimeError::Acp(format!("Login failed: {e}"))),
                }
            }
            BridgeAction::Logout => {
                self.disconnect_tunnel().await;
                crate::auth::clear_auth().map_err(|e| RuntimeError::Acp(e.to_string()))?;
                Ok("Logged out.".into())
            }
        }
    }

    /// Execute a bridge action and return an [`AppMessage::BridgeResult`].
    pub async fn dispatch_bridge_action(&mut self, action: BridgeAction) -> AppMessage {
        let action_name = action.action_name().to_string();
        match self.execute_bridge_action(action).await {
            Ok(text) => AppMessage::BridgeResult {
                action: action_name,
                text,
                is_error: false,
            },
            Err(e) => AppMessage::BridgeResult {
                action: action_name,
                text: e.to_string(),
                is_error: true,
            },
        }
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Execute a tool by name with the given arguments.
    ///
    /// Wraps the tool registry's `execute()` API with a generated call ID.
    async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<String, RuntimeError> {
        static CALL_COUNTER: AtomicU64 = AtomicU64::new(1);
        let call = ToolCallRequest {
            id: format!("call_{}", CALL_COUNTER.fetch_add(1, Ordering::Relaxed)),
            name: name.into(),
            arguments,
        };
        let result = self
            .tool_registry
            .execute(&call, Some(Arc::clone(&self.tool_context)))
            .await;
        if result.is_error {
            Err(RuntimeError::Acp(result.output))
        } else {
            Ok(result.output)
        }
    }

    /// Resolve an ACP server config by name, or use the default/first.
    fn resolve_server(&self, server_name: Option<&str>) -> Result<AcpServerConfig, RuntimeError> {
        if self.config.acp.servers.is_empty() {
            return Err(RuntimeError::NoServersConfigured);
        }

        let name = server_name
            .map(String::from)
            .or_else(|| self.config.default_server.clone());

        match name {
            Some(ref n) => self
                .config
                .acp
                .servers
                .iter()
                .find(|s| s.name == *n)
                .cloned()
                .ok_or_else(|| RuntimeError::ServerNotFound(n.clone())),
            None => Ok(self.config.acp.servers[0].clone()),
        }
    }
}

/// Spawn a background task that refreshes the auth token every 10 minutes
/// and updates the tunnel's token so reconnects use a valid JWT.
///
/// On success the new auth state is also persisted to disk.
/// On failure a warning is logged but the task keeps running.
pub fn spawn_token_refresh(tunnel: Arc<TunnelClient>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            crate::constants::TOKEN_REFRESH_INTERVAL_SECS,
        ));
        // The first tick fires immediately; skip it.
        interval.tick().await;

        loop {
            interval.tick().await;

            // Load the current auth state (contains the refresh token).
            let auth_state = match crate::auth::load_auth() {
                Some(s) => s,
                None => {
                    tracing::warn!("Token refresh: no auth state on disk, skipping");
                    continue;
                }
            };

            // Build an AuthClient seeded with the current state so
            // `refresh_access_token` can read the refresh token.
            let mut client = simse_core::remote::auth::AuthClient::from_state(auth_state.clone());

            match client.refresh_access_token().await {
                Ok(new_access) => {
                    // Update the tunnel so reconnects use the fresh JWT.
                    tunnel.update_token(new_access.clone()).await;

                    // Persist the updated tokens to disk.
                    if let Some(updated) = client.state().cloned()
                        && let Err(e) = crate::auth::save_auth(&updated)
                    {
                        tracing::warn!("Token refresh: failed to persist auth state: {e}");
                    }

                    tracing::debug!("Token refresh: access token updated");
                }
                Err(e) => {
                    tracing::warn!("Token refresh failed: {e}");
                }
            }
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        AcpFileConfig, AcpServerConfig, EmbedFileConfig, LibraryFileConfig, UserConfig,
        WorkspaceSettings,
    };
    use std::collections::HashMap;
    use std::path::PathBuf;

    /// Build a minimal LoadedConfig for testing.
    fn test_config() -> LoadedConfig {
        LoadedConfig {
            acp: AcpFileConfig {
                servers: vec![AcpServerConfig {
                    name: "test-server".into(),
                    command: "echo".into(),
                    args: vec!["hello".into()],
                    cwd: None,
                    env: HashMap::new(),
                    default_agent: None,
                    timeout_ms: Some(5000),
                }],
                default_server: Some("test-server".into()),
                default_agent: None,
            },
            mcp_servers: Vec::new(),
            skipped_servers: Vec::new(),
            embed: EmbedFileConfig::default(),
            library: LibraryFileConfig::default(),
            summarize: None,
            user: UserConfig::default(),
            workspace_settings: WorkspaceSettings::default(),
            prompts: HashMap::new(),
            agents: Vec::new(),
            skills: Vec::new(),
            workspace_prompt: None,
            log_level: "warn".into(),
            default_agent: None,
            default_server: Some("test-server".into()),
            embedding_model: "nomic-ai/nomic-embed-text-v1.5".into(),
            data_dir: PathBuf::from("/tmp/simse-test"),
            work_dir: PathBuf::from("/tmp/simse-test-work"),
            plugins: Default::default(),
        }
    }

    /// Build a config with no ACP servers (needs onboarding).
    fn empty_config() -> LoadedConfig {
        LoadedConfig {
            acp: AcpFileConfig::default(),
            mcp_servers: Vec::new(),
            skipped_servers: Vec::new(),
            embed: EmbedFileConfig::default(),
            library: LibraryFileConfig::default(),
            summarize: None,
            user: UserConfig::default(),
            workspace_settings: WorkspaceSettings::default(),
            prompts: HashMap::new(),
            agents: Vec::new(),
            skills: Vec::new(),
            workspace_prompt: None,
            log_level: "warn".into(),
            default_agent: None,
            default_server: None,
            embedding_model: "nomic-ai/nomic-embed-text-v1.5".into(),
            data_dir: PathBuf::from("/tmp/simse-test"),
            work_dir: PathBuf::from("/tmp/simse-test-work"),
            plugins: Default::default(),
        }
    }

    #[test]
    fn event_loop_new_runtime() {
        let config = test_config();
        let runtime = CliRuntime::new(config);
        assert!(!runtime.is_connected());
        assert!(runtime.session_id().is_none());
        assert!(!runtime.verbose);
    }

    #[test]
    fn event_loop_needs_onboarding_no_auth() {
        // Hermetic: the onboarding decision is pure logic, independent of any
        // auth file on the test host. No model client and no auth → onboard.
        assert!(CliRuntime::onboarding_needed(false, false));
        // A model client (e.g. --provider) → no onboarding.
        assert!(!CliRuntime::onboarding_needed(true, false));
        // Auth present on disk → no onboarding.
        assert!(!CliRuntime::onboarding_needed(false, true));
        assert!(!CliRuntime::onboarding_needed(true, true));
    }

    #[test]
    fn event_loop_no_onboarding_with_provider() {
        // With a model_client set (e.g. --provider), onboarding is not needed.
        let config = test_config();
        let mut runtime = CliRuntime::new(config);
        runtime.set_model_client(std::sync::Arc::new(
            crate::openai_compat::OpenAiCompatClient::new("http://localhost:11434", "test"),
        ));
        assert!(!runtime.needs_onboarding());
    }

    #[test]
    fn event_loop_abort_signal() {
        let runtime = CliRuntime::new(test_config());
        assert!(!runtime.is_aborted());
        runtime.abort();
        assert!(runtime.is_aborted());
    }

    #[test]
    fn event_loop_abort_signal_shared() {
        let runtime = CliRuntime::new(test_config());
        let token = runtime.cancel_token();
        assert!(!token.is_cancelled());
        runtime.abort();
        assert!(token.is_cancelled());
    }

    #[test]
    fn event_loop_not_connected_initially() {
        let runtime = CliRuntime::new(test_config());
        assert!(!runtime.is_connected());
    }

    #[tokio::test]
    async fn event_loop_not_healthy_when_disconnected() {
        let runtime = CliRuntime::new(test_config());
        assert!(!runtime.is_healthy().await);
    }

    #[test]
    fn event_loop_conversation_access() {
        let mut runtime = CliRuntime::new(test_config());
        runtime.update_conversation(|c| c.add_user("Hello"));
        let messages = runtime.conversation().to_messages();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn event_loop_reset_conversation() {
        let mut runtime = CliRuntime::new(test_config());
        runtime.update_conversation(|c| c.add_user("Hello"));
        assert!(!runtime.conversation().to_messages().is_empty());
        runtime.reset_conversation();
        assert!(runtime.conversation().to_messages().is_empty());
    }

    #[test]
    fn event_loop_tool_registry_has_tools() {
        let runtime = CliRuntime::new(test_config());
        assert!(
            runtime.tool_registry().tool_count() > 0,
            "sandbox tools should be registered"
        );
    }

    #[test]
    fn event_loop_subagent_tools_registered() {
        let runtime = CliRuntime::new(test_config());
        let registry = runtime.tool_registry();
        assert!(
            registry.is_registered("subagent_spawn"),
            "subagent_spawn should be registered on CliRuntime"
        );
        assert!(
            registry.is_registered("subagent_delegate"),
            "subagent_delegate should be registered on CliRuntime"
        );
    }

    #[test]
    fn event_loop_permission_manager_access() {
        let runtime = CliRuntime::new(test_config());
        let _pm = runtime.permission_manager();
    }

    #[test]
    fn event_loop_config_access() {
        let runtime = CliRuntime::new(test_config());
        assert_eq!(runtime.config().log_level, "warn");
        assert_eq!(
            runtime.config().default_server.as_deref(),
            Some("test-server")
        );
    }

    #[test]
    fn event_loop_resolve_server_default() {
        let runtime = CliRuntime::new(test_config());
        let server = runtime.resolve_server(None).unwrap();
        assert_eq!(server.name, "test-server");
    }

    #[test]
    fn event_loop_resolve_server_by_name() {
        let runtime = CliRuntime::new(test_config());
        let server = runtime.resolve_server(Some("test-server")).unwrap();
        assert_eq!(server.name, "test-server");
    }

    #[test]
    fn event_loop_resolve_server_not_found() {
        let runtime = CliRuntime::new(test_config());
        let err = runtime.resolve_server(Some("nonexistent")).unwrap_err();
        match err {
            RuntimeError::ServerNotFound(name) => assert_eq!(name, "nonexistent"),
            _ => panic!("Expected ServerNotFound"),
        }
    }

    #[test]
    fn event_loop_resolve_server_no_servers() {
        let runtime = CliRuntime::new(empty_config());
        let err = runtime.resolve_server(None).unwrap_err();
        assert!(matches!(err, RuntimeError::NoServersConfigured));
    }

    #[test]
    fn event_loop_resolve_server_first_when_no_default() {
        let mut config = test_config();
        config.default_server = None;
        let runtime = CliRuntime::new(config);
        let server = runtime.resolve_server(None).unwrap();
        assert_eq!(server.name, "test-server");
    }

    #[test]
    fn event_loop_agent_name_none_when_disconnected() {
        let runtime = CliRuntime::new(test_config());
        assert!(runtime.agent_name().is_none());
    }

    #[test]
    fn event_loop_verbose_default_false() {
        let runtime = CliRuntime::new(test_config());
        assert!(!runtime.verbose);
    }

    #[test]
    fn event_loop_verbose_can_be_set() {
        let mut runtime = CliRuntime::new(test_config());
        runtime.verbose = true;
        assert!(runtime.verbose);
    }

    #[tokio::test]
    async fn event_loop_handle_submit_not_connected() {
        let mut runtime = CliRuntime::new(test_config());
        let cb = LoopCallbacks::default();
        let err = runtime.handle_submit("hello", cb).await.unwrap_err();
        assert!(matches!(err, RuntimeError::NotConnected));
    }

    #[tokio::test]
    async fn event_loop_handle_permission_not_connected() {
        let mut runtime = CliRuntime::new(test_config());
        let err = runtime
            .handle_permission_response("req-1", "allow")
            .await
            .unwrap_err();
        assert!(matches!(err, RuntimeError::NotConnected));
    }

    #[test]
    fn event_loop_error_display() {
        assert_eq!(
            format!("{}", RuntimeError::NotConnected),
            "Not connected to ACP server"
        );
        assert_eq!(
            format!("{}", RuntimeError::NoServersConfigured),
            "No ACP servers configured"
        );
        assert_eq!(
            format!("{}", RuntimeError::ServerNotFound("x".into())),
            "ACP server not found: x"
        );
        assert_eq!(format!("{}", RuntimeError::NoSession), "No active session");
        assert_eq!(
            format!("{}", RuntimeError::Acp("timeout".into())),
            "timeout"
        );
    }
}
