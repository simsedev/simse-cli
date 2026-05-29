//! CLI sub-agent + ACP-delegate runners.
//!
//! Implements the shared `SubagentLoopRunner` / `DelegateRunner` traits for
//! the `simse` CLI, so `subagent_spawn` / `subagent_delegate` work as plain
//! model-invokable tools. A tool handler cannot borrow `CliRuntime`, so the
//! runners hold their dependencies in a `OnceCell` filled once the runtime is
//! built (the pattern `managed/subagent.rs` uses).

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

use simse_core::acp::client::{AcpClient as AcpEngine, GenerateOptions};
use simse_core::agentic_loop::{
    AcpClient, AgenticLoopOptions, Message, MessageRole, ToolExecutor, run_agentic_loop,
};
use simse_core::error::SimseError;
use simse_core::tools::subagent::{DelegateRunner, SubagentLoopRunner, SubagentResult};
use simse_core::tools::types::ToolContext;

/// Dependencies the CLI sub-agent / delegate runners need, filled in once the
/// `CliRuntime` is built.
pub struct CliSubagentDeps {
    /// Model client used for the nested agentic loop and the delegate fallback.
    pub model_client: Arc<dyn AcpClient>,
    /// Shared tool registry — subagents run with the same tools as the host.
    pub tool_registry: Arc<dyn ToolExecutor>,
    /// Shared host `ToolContext` so subagents act on the same workspace.
    pub tool_context: Arc<ToolContext>,
    /// Optional ACP engine for routing single-shot delegations to plugins.
    pub acp_engine: Option<Arc<AcpEngine>>,
    /// Optional default system prompt for spawned subagents / delegations.
    pub system_prompt: Option<String>,
    /// CLI-level cancellation token. Each subagent run gets a child of this
    /// so a user abort (Ctrl-C) also stops in-flight `subagent_spawn` loops.
    pub cancel_token: CancellationToken,
}

/// Shared, deferred reference to the CLI sub-agent dependencies.
///
/// Created empty before tool registration, filled once the `CliRuntime` is
/// constructed. Both runners share the same cell via `Arc`.
pub type DeferredCliSubagent = Arc<OnceCell<CliSubagentDeps>>;

/// Create an empty deferred CLI sub-agent dependency cell.
pub fn deferred_cli_subagent() -> DeferredCliSubagent {
    Arc::new(OnceCell::new())
}

/// Runs a nested agentic loop for spawned subagents in CLI mode.
pub struct CliSubagentRunner {
    /// Deferred dependencies, shared with [`CliDelegateRunner`].
    pub deps: DeferredCliSubagent,
}

/// Routes a single-shot task to a named ACP plugin server in CLI mode.
pub struct CliDelegateRunner {
    /// Deferred dependencies, shared with [`CliSubagentRunner`].
    pub deps: DeferredCliSubagent,
}

/// Resolve the deferred dependencies, returning an error if not yet filled.
fn resolve(cell: &DeferredCliSubagent) -> Result<&CliSubagentDeps, SimseError> {
    cell.get()
        .ok_or_else(|| SimseError::other("subagent runner: CLI runtime not yet initialized"))
}

#[async_trait]
impl SubagentLoopRunner for CliSubagentRunner {
    async fn run_subagent(
        &self,
        task: &str,
        max_turns: u32,
        system_prompt: Option<&str>,
        _depth: u32,
        // The CLI shares a single ambient `tool_context` (already vgit-wired
        // via `ToolContext::host`) for the whole CLI session, so the parent
        // vgit threaded here is informational only — the CLI deliberately
        // continues to reuse `deps.tool_context` to keep all sessions on
        // one shared sandbox.
        _parent_vgit: simse_core::tools::subagent::ParentVgit,
    ) -> Result<SubagentResult, SimseError> {
        let deps = resolve(&self.deps)?;

        // Build the conversation with the task as the first user message.
        let mut messages = vec![Message {
            role: MessageRole::User,
            content: task.to_string(),
            images: Vec::new(),
        }];

        // Resolve the system prompt: explicit override > configured default.
        let resolved_prompt = system_prompt
            .map(str::to_string)
            .or_else(|| deps.system_prompt.clone());

        let options = AgenticLoopOptions {
            max_turns: max_turns as usize,
            system_prompt: resolved_prompt,
            ..AgenticLoopOptions::default()
        };

        let result = run_agentic_loop(
            deps.model_client.as_ref(),
            deps.tool_registry.as_ref(),
            &mut messages,
            options,
            None, // no lifecycle callbacks for subagents
            Some(deps.cancel_token.child_token()),
            None, // no context pruner
            None, // no compaction provider
            Some(Arc::clone(&deps.tool_context)),
        )
        .await?;

        let (input_tokens, output_tokens) = result
            .total_usage
            .as_ref()
            .map(|u| (u.input_tokens, u.output_tokens))
            .unwrap_or((None, None));

        Ok(SubagentResult {
            text: result.final_text,
            turns: result.total_turns as u32,
            duration_ms: result.total_duration_ms,
            input_tokens,
            output_tokens,
        })
    }
}

#[async_trait]
impl DelegateRunner for CliDelegateRunner {
    async fn delegate(
        &self,
        task: &str,
        server_name: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<String, SimseError> {
        let deps = resolve(&self.deps)?;

        // Prefer routing through the ACP engine when one is available.
        if let Some(engine) = &deps.acp_engine {
            let target = server_name
                .map(str::to_string)
                .or_else(|| engine.default_server_name().map(str::to_string));
            if let Some(target) = target {
                let options = GenerateOptions {
                    server_name: Some(target),
                    agent_id: agent_id.map(str::to_string),
                    system_prompt: deps.system_prompt.clone(),
                    ..GenerateOptions::default()
                };
                return engine
                    .generate(task, options)
                    .await
                    .map(|r| r.content)
                    .map_err(|e| SimseError::other(e.to_string()));
            }
        }

        // No ACP engine — fall back to a single-shot model generation.
        let messages = vec![Message {
            role: MessageRole::User,
            content: task.to_string(),
            images: Vec::new(),
        }];
        let resp = deps
            .model_client
            .generate(&messages, deps.system_prompt.as_deref(), None, None, None)
            .await?;
        Ok(resp.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfilled_cell_resolves_to_error() {
        let cell = deferred_cli_subagent();
        assert!(resolve(&cell).is_err());
    }
}
