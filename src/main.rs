//! simse CLI — terminal interface.

use std::io::{self, IsTerminal};
use std::sync::Arc;

use clap::Parser;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use futures::StreamExt;
use ratatui::{Terminal, backend::CrosstermBackend};
use tokio::sync::{Mutex, mpsc};

use simse_cli::app::{App, AppMessage, view};
use simse_cli::cli_args::{Command, SimSeCli};
use simse_cli::config;
use simse_cli::event_loop;
use simse_cli::headless::{self, PrintArgs};
use simse_cli::openai_compat::OpenAiCompatClient;
use simse_cli::remote_transport::MessageSender;
use simse_cli::ui_core::app::{OutputItem, ToolCallState, ToolCallStatus};
use simse_cli::update::{Effect, update};

/// Read the current git branch from `.git/HEAD` in the given directory.
fn read_git_branch(work_dir: &std::path::Path) -> Option<String> {
    let head_path = work_dir.join(".git/HEAD");
    let content = std::fs::read_to_string(head_path).ok()?;
    let content = content.trim();
    if let Some(ref_path) = content.strip_prefix("ref: refs/heads/") {
        Some(ref_path.to_string())
    } else if content.len() >= 7 {
        Some(content[..7].to_string())
    } else {
        None
    }
}

/// Create `LoopCallbacks` that forward agentic loop events to the CLI as
/// `AppMessage`s via an unbounded channel.
fn create_loop_callbacks(
    tx: mpsc::UnboundedSender<AppMessage>,
) -> simse_core::agentic_loop::LoopCallbacks {
    let tx_start = tx.clone();
    let tx_delta = tx.clone();
    let tx_error = tx.clone();
    let tx_usage = tx.clone();
    let tx_tool_start = tx.clone();
    let tx_tool_end = tx.clone();
    simse_core::agentic_loop::LoopCallbacks {
        on_stream_start: Some(Box::new(move || {
            let _ = tx_start.send(AppMessage::StreamStart);
        })),
        on_stream_delta: Some(std::sync::Arc::new(move |delta: &str| {
            let _ = tx_delta.send(AppMessage::StreamDelta(delta.to_string()));
        })),
        // Tool lifecycle → drives the live "working" indicator + tool-call
        // boxes. Without these the UI goes dark during tool execution and the
        // user can't tell if it's still working.
        on_tool_call_start: Some(Box::new(
            move |req: &simse_core::tools::types::ToolCallRequest| {
                let _ = tx_tool_start.send(AppMessage::ToolCallStart(ToolCallState {
                    id: req.id.clone(),
                    name: req.name.clone(),
                    args: req.arguments.to_string(),
                    status: ToolCallStatus::Active,
                    started_at: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_millis() as i64)
                        .unwrap_or(0),
                    duration_ms: None,
                    summary: None,
                    error: None,
                    diff: None,
                }));
            },
        )),
        on_tool_call_end: Some(Box::new(
            move |res: &simse_core::tools::types::ToolCallResult| {
                let (status, error) = if res.is_error {
                    (ToolCallStatus::Failed, Some(res.output.clone()))
                } else {
                    (ToolCallStatus::Completed, None)
                };
                let _ = tx_tool_end.send(AppMessage::ToolCallEnd {
                    id: res.id.clone(),
                    status,
                    summary: if res.is_error {
                        None
                    } else {
                        Some(res.output.clone())
                    },
                    error,
                    duration_ms: None,
                    diff: None,
                });
            },
        )),
        on_error: Some(Box::new(move |error: &simse_core::SimseError| {
            let _ = tx_error.send(AppMessage::LoopError(error.to_string()));
        })),
        on_usage_update: Some(Box::new(
            move |usage: &simse_core::agentic_loop::TokenUsage| {
                let prompt = usage.input_tokens.unwrap_or(0);
                let completion = usage.output_tokens.unwrap_or(0);
                let _ = tx_usage.send(AppMessage::TokenUsage { prompt, completion });
            },
        )),
        ..Default::default()
    }
}

/// Build a `LoadedConfig`.
fn build_config(_cli: &SimSeCli) -> config::LoadedConfig {
    config::load_config(&config::ConfigOptions::default())
}

/// Configure the model client on the runtime from CLI flags or login state.
///
/// If `--provider` is set, uses an OpenAI-compatible HTTP client.
/// If logged in, connects to the remote inference service via the API.
/// If neither, onboarding will handle it (login flow).
async fn init_model_client(cli: &SimSeCli, rt: &mut event_loop::CliRuntime) {
    if let Some(url) = cli.provider.as_deref() {
        let model = cli.model.as_deref().unwrap_or("default");
        rt.set_model_client(std::sync::Arc::new(OpenAiCompatClient::new(url, model)));
    } else if simse_cli::auth::load_auth().is_some()
        && let Err(e) = rt.connect_inference_with_model(cli.model.clone()).await
    {
        eprintln!("warning: inference connection failed: {e}");
    }
}

/// Create and configure a `CliRuntime` from CLI arguments.
async fn create_runtime(cli: &SimSeCli) -> event_loop::CliRuntime {
    let cfg = build_config(cli);
    let mut rt = event_loop::CliRuntime::new(cfg);
    rt.verbose = cli.verbose;
    rt.init_plugins().await;
    init_model_client(cli, &mut rt).await;
    rt
}

/// Resolve which session ID to load based on `--continue` / `resume` flags.
fn resolve_session_id(
    continue_session: bool,
    resume: Option<&str>,
    rt: &event_loop::CliRuntime,
) -> Option<String> {
    if let Some(id) = resume {
        if !rt.session_exists(id) {
            eprintln!("error: session not found: {id}");
            std::process::exit(1);
        }
        return Some(id.to_string());
    }

    if continue_session {
        let work_dir = std::env::current_dir()
            .unwrap_or_default()
            .display()
            .to_string();
        match rt.latest_session(&work_dir) {
            Some(id) => return Some(id),
            None => {
                eprintln!("error: no session found for the current directory");
                std::process::exit(1);
            }
        }
    }

    None
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let mut cli = SimSeCli::parse();

    // --- Print mode: non-interactive with full runtime access ---
    if cli.print {
        let prompt_text = if let Some(ref text) = cli.prompt {
            text.clone()
        } else if !std::io::stdin().is_terminal() {
            let mut input = String::new();
            std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)?;
            let trimmed = input.trim().to_string();
            if trimmed.is_empty() {
                eprintln!("error: no input received from stdin");
                std::process::exit(1);
            }
            trimmed
        } else {
            eprintln!("Usage: simse --print -p <prompt>");
            eprintln!("       echo <prompt> | simse --print");
            std::process::exit(1);
        };

        let mut rt = create_runtime(&cli).await;

        let exit_code = headless::run_print(
            PrintArgs {
                prompt: prompt_text,
                format: cli.format.clone(),
                resume: cli.resume.clone(),
                continue_session: cli.continue_session,
                verbose: cli.verbose,
            },
            &mut rt,
        )
        .await;
        std::process::exit(exit_code);
    }

    // --- Subcommands ---
    let command = cli.command.take();

    match command {
        Some(Command::Login) => {
            let exit_code = simse_cli::auth_cmd::run_login().await;
            std::process::exit(exit_code);
        }

        Some(Command::Logout) => {
            let exit_code = simse_cli::auth_cmd::run_logout();
            std::process::exit(exit_code);
        }

        Some(Command::Mcp { action }) => {
            let mut rt = create_runtime(&cli).await;
            let exit_code = run_protocol_action("mcp", action, &mut rt).await;
            std::process::exit(exit_code);
        }

        Some(Command::Acp { action }) => {
            let mut rt = create_runtime(&cli).await;
            let exit_code = run_protocol_action("acp", action, &mut rt).await;
            std::process::exit(exit_code);
        }

        Some(Command::Fork { id, at }) => {
            let cfg = build_config(&cli);
            let exit_code = run_fork(&id, at, cfg);
            std::process::exit(exit_code);
        }

        Some(Command::Plugins { action }) => {
            let cfg = build_config(&cli);
            let exit_code = run_plugins_action(action, cfg).await;
            std::process::exit(exit_code);
        }

        Some(Command::Daemon {
            work_dir,
            detach,
            pid_file,
        }) => {
            let exit_code = run_daemon(cli, work_dir, detach, pid_file).await;
            std::process::exit(exit_code);
        }

        Some(Command::Resume { id }) => run_tui(cli, Some(id)).await,

        None => {
            // Check for --resume flag (alternative to `simse resume <id>`)
            let resume_id = cli.resume.clone();
            run_tui(cli, resume_id.map(Some)).await
        }
    }
}

/// Launch the interactive TUI.
async fn run_tui(cli: SimSeCli, resume_id: Option<Option<String>>) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, cli, resume_id).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Run a protocol management subcommand (mcp/acp restart/status).
async fn run_protocol_action(
    protocol: &str,
    action: simse_cli::cli_args::ProtocolAction,
    rt: &mut event_loop::CliRuntime,
) -> i32 {
    use simse_cli::cli_args::ProtocolAction;
    use simse_cli::commands::BridgeAction;

    match action {
        ProtocolAction::Restart => {
            if !rt.is_connected() {
                eprintln!("error: not connected. Use --provider or configure ACP.");
                return 1;
            }

            let bridge = match protocol {
                "mcp" => BridgeAction::McpRestart,
                "acp" => BridgeAction::AcpRestart,
                _ => unreachable!(),
            };
            match rt.execute_bridge_action(bridge).await {
                Ok(msg) => {
                    println!("{msg}");
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        ProtocolAction::Status => {
            let server = rt.server_name().unwrap_or_else(|| "(none)".into());
            let connected = rt.is_connected();
            println!("{protocol} server: {server}");
            println!("connected: {connected}");
            if protocol == "mcp" {
                let count = rt.config().mcp_servers.len();
                println!("mcp servers configured: {count}");
            }
            0
        }
    }
}

/// Fork a session at a specific message index.
fn run_fork(session_id: &str, at: Option<usize>, cfg: config::LoadedConfig) -> i32 {
    let store = simse_cli::session_store::SessionStore::new(&cfg.data_dir);

    let fork_point = at.unwrap_or_else(|| store.load(session_id).len());

    match store.fork(session_id, at) {
        Ok(new_id) => {
            println!("Forked session {session_id} at message {fork_point} -> {new_id}");
            println!("Resume with: simse resume {new_id}");
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}

/// List, search, install, or remove plugins.
///
/// Plugins are not bundled into the `simse` binary — only the plugin engine
/// is. They are installed on demand from the marketplace (the `simse-cli`
/// repo's `plugins/` directory) into `<data_dir>/plugins/<name>/`.
async fn run_plugins_action(
    action: simse_cli::cli_args::PluginsAction,
    cfg: config::LoadedConfig,
) -> i32 {
    use simse_cli::cli_args::PluginsAction;
    use simse_cli::marketplace;

    match action {
        PluginsAction::List { r#type } => {
            let all = config::discover_plugins(&cfg.data_dir);

            let filtered: Vec<_> = if let Some(ref t) = r#type {
                all.into_iter().filter(|p| p.kind == *t).collect()
            } else {
                all
            };

            if filtered.is_empty() {
                match r#type {
                    Some(t) => println!("No {t} plugins installed."),
                    None => println!("No plugins installed."),
                }
                println!("Install one with: simse plugins install <name>");
                return 0;
            }

            for plugin in &filtered {
                let desc = if plugin.description.is_empty() {
                    "(no description)"
                } else {
                    &plugin.description
                };
                let version = if plugin.version.is_empty() {
                    String::new()
                } else {
                    format!(" v{}", plugin.version)
                };
                println!("  [{}] {}{} — {}", plugin.kind, plugin.name, version, desc);
            }
            0
        }

        PluginsAction::Search => match marketplace::search().await {
            Ok(plugins) => {
                if plugins.is_empty() {
                    println!("No plugins found in the marketplace.");
                    return 0;
                }
                let installed: std::collections::HashSet<String> =
                    config::discover_plugins(&cfg.data_dir)
                        .into_iter()
                        .map(|p| p.name)
                        .collect();
                println!("Marketplace plugins:");
                for p in &plugins {
                    let mark = if installed.contains(&p.name) {
                        " (installed)"
                    } else {
                        ""
                    };
                    println!("  {}{}", p.name, mark);
                }
                println!("\nInstall with: simse plugins install <name>");
                0
            }
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },

        PluginsAction::Install { name } => {
            println!("Installing plugin '{name}'...");
            match marketplace::install(&name, &cfg.data_dir).await {
                Ok(()) => {
                    println!("Installed '{name}'. It loads on the next simse start.");
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }

        PluginsAction::Remove { name } => {
            if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
                eprintln!("error: invalid plugin name: '{name}'");
                return 1;
            }
            let dir = cfg.data_dir.join("plugins").join(&name);
            if !dir.is_dir() {
                eprintln!("error: plugin '{name}' is not installed");
                return 1;
            }
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => {
                    println!("Removed plugin '{name}'.");
                    0
                }
                Err(e) => {
                    eprintln!("error: cannot remove '{name}': {e}");
                    1
                }
            }
        }
    }
}

/// Run simse as a background daemon with a persistent tunnel connection.
///
/// The daemon keeps the tunnel alive so the web dashboard can access
/// the local workspace (files, shell, network, plugins, tools).
async fn run_daemon(
    cli: SimSeCli,
    work_dir: Option<String>,
    detach: bool,
    pid_file: Option<String>,
) -> i32 {
    // Detach from terminal if --detach
    if detach {
        #[cfg(unix)]
        {
            use std::process;
            match unsafe { libc::fork() } {
                -1 => {
                    eprintln!("error: fork failed");
                    return 1;
                }
                0 => {
                    // Child — continue as daemon
                    unsafe { libc::setsid() };
                }
                child_pid => {
                    // Parent — write PID and exit
                    if let Some(ref path) = pid_file
                        && let Err(e) = std::fs::write(path, child_pid.to_string())
                    {
                        eprintln!("warning: could not write pid file {path}: {e}");
                    }
                    println!("simse daemon started (pid: {child_pid})");
                    process::exit(0);
                }
            }
        }
        #[cfg(not(unix))]
        {
            eprintln!("error: --detach is only supported on Unix systems");
            return 1;
        }
    }

    // Set working directory
    if let Some(ref dir) = work_dir
        && let Err(e) = std::env::set_current_dir(dir)
    {
        eprintln!("error: cannot set working directory: {e}");
        return 1;
    }

    // Write PID file (if not detached — detached case handled above)
    if !detach
        && let Some(ref path) = pid_file
        && let Err(e) = std::fs::write(path, std::process::id().to_string())
    {
        eprintln!("warning: could not write pid file {path}: {e}");
    }

    // Check auth (gate daemon start on login presence; the loaded state is unused here —
    // tunnel + token refresh below load it again on demand).
    if simse_cli::auth::load_auth().is_none() {
        eprintln!("error: not logged in. Run `simse login` first.");
        return 1;
    }

    let mut rt = create_runtime(&cli).await;

    // Connect tunnel
    let (tunnel, incoming_rx) = match rt.connect_tunnel().await {
        Ok((tunnel_id, rx)) => {
            let tunnel = rt.tunnel().expect("tunnel set after connect").clone();
            eprintln!("[daemon] connected (tunnel: {tunnel_id})");
            (tunnel, rx)
        }
        Err(e) => {
            eprintln!("error: tunnel connection failed: {e}");
            return 1;
        }
    };

    // Refresh auth token periodically
    event_loop::spawn_token_refresh(tunnel.clone());

    let runtime = Arc::new(Mutex::new(rt));

    // Spawn handler for incoming tunnel messages (remote/*, session/*)
    let _handler = spawn_tunnel_handler(incoming_rx, tunnel.clone(), Arc::clone(&runtime));

    let cwd = std::env::current_dir()
        .unwrap_or_default()
        .display()
        .to_string();
    eprintln!("[daemon] workspace: {cwd}");
    eprintln!("[daemon] ready — web dashboard can now access this device");
    eprintln!("[daemon] press Ctrl-C to stop");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await.ok();
    eprintln!("\n[daemon] shutting down...");

    // Cleanup PID file
    if let Some(ref path) = pid_file {
        let _ = std::fs::remove_file(path);
    }

    0
}

/// Spawn a background task that handles incoming tunnel messages.
fn spawn_tunnel_handler(
    mut incoming_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    tunnel: Arc<simse_core::remote::tunnel::TunnelClient>,
    runtime: Arc<Mutex<event_loop::CliRuntime>>,
) -> tokio::task::JoinHandle<()> {
    let sessions = Arc::new(simse_cli::handlers::SessionState::new());
    let sender: Arc<dyn MessageSender> =
        Arc::new(simse_cli::remote_transport::TunnelSender::new(tunnel));
    tokio::spawn(async move {
        while let Some(msg) = incoming_rx.recv().await {
            let response = simse_cli::handlers::dispatch(&msg, &runtime, &sessions, &sender).await;
            if !response.is_empty() {
                sender.send_message(&response).await;
            }
        }
    })
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    cli: SimSeCli,
    resume_id: Option<Option<String>>,
) -> io::Result<()> {
    let mut rt = create_runtime(&cli).await;

    // Resolve session to load: from `simse resume <id>`, `--continue`, or none.
    let session_to_load = if let Some(explicit_id) = resume_id {
        // `simse resume [id]` — if id is None, use latest session
        match explicit_id {
            Some(id) => Some(id),
            None => {
                let work_dir = std::env::current_dir()
                    .unwrap_or_default()
                    .display()
                    .to_string();
                match rt.latest_session(&work_dir) {
                    Some(id) => Some(id),
                    None => {
                        eprintln!("error: no session found for the current directory");
                        std::process::exit(1);
                    }
                }
            }
        }
    } else {
        resolve_session_id(cli.continue_session, None, &rt)
    };

    let mut initial_output: Vec<OutputItem> = Vec::new();
    if let Some(session_id) = session_to_load {
        match rt.load_session(&session_id) {
            Ok(messages) => {
                for msg in &messages {
                    initial_output.push(OutputItem::Message {
                        role: msg.role.clone(),
                        text: msg.content.clone(),
                    });
                }
            }
            Err(e) => {
                eprintln!("error: failed to load session: {e}");
                std::process::exit(1);
            }
        }
    }

    let runtime = Arc::new(Mutex::new(rt));

    // Auto-connect tunnel if already logged in.
    if let Some(_auth) = simse_cli::auth::load_auth() {
        let mut rt = runtime.lock().await;
        match rt.connect_tunnel().await {
            Ok((_tunnel_id, incoming_rx)) => {
                let tunnel = rt.tunnel().expect("tunnel set after connect").clone();
                event_loop::spawn_token_refresh(tunnel.clone());
                let handle = spawn_tunnel_handler(incoming_rx, tunnel, Arc::clone(&runtime));
                rt.set_tunnel_handler(handle);
            }
            Err(_e) => {
                // Auto-connect is best-effort; continue without tunnel.
            }
        }
        drop(rt);
    }

    // Onboarding: if not logged in and no --provider, run login flow.
    {
        let rt = runtime.lock().await;
        if rt.needs_onboarding() {
            drop(rt);
            match simse_cli::auth_cmd::device_login_interactive().await {
                Ok(state) => {
                    if let Err(e) = simse_cli::auth::save_auth(&state) {
                        eprintln!("error: failed to save auth: {e}");
                        std::process::exit(1);
                    }
                    let mut rt = runtime.lock().await;
                    if let Err(e) = rt.connect_inference().await {
                        eprintln!("warning: inference connection failed: {e}");
                    }
                }
                Err(e) => {
                    eprintln!("error: login failed: {e}");
                    eprintln!("You can also use --provider <url> for a local model.");
                    std::process::exit(1);
                }
            }
        }
    }

    let mut app = App::new();
    let cwd = std::env::current_dir().unwrap_or_default();
    app.git_branch = read_git_branch(&cwd);
    app.work_dir = Some(cwd.display().to_string());
    {
        let rt = runtime.lock().await;
        if rt.tunnel_connected()
            && let Some(auth) = simse_cli::auth::load_auth()
        {
            app.remote_connected = true;
            app.remote_email = Some(auth.email);
        }
    }

    if !initial_output.is_empty() {
        app.output = initial_output;
        app.banner_visible = false;
        let rt = runtime.lock().await;
        app.session_id = rt.session_id().map(String::from);
        drop(rt);
    }
    let (msg_tx, mut msg_rx) = mpsc::unbounded_channel::<AppMessage>();
    let mut reader = EventStream::new();

    let mut deferred_effect: Option<Effect> = None;

    loop {
        terminal.draw(|frame| view(&app, frame))?;

        let tick_duration =
            std::time::Duration::from_millis(simse_cli::constants::TICK_INTERVAL_MS);
        let needs_tick = app.spinner.is_some() || !app.tool_call_instants.is_empty();

        // Collect effects from event processing.
        let mut pending_effects: Vec<Effect> = Vec::new();

        // Re-queue any deferred effect from the previous iteration.
        if let Some(effect) = deferred_effect.take() {
            pending_effects.push(effect);
        }

        tokio::select! {
            Some(Ok(event)) = reader.next() => {
                if let Some(msg) = map_event(event, app.search.active) {
                    if matches!(msg, AppMessage::CtrlC) && !app.ctrl_c_pending {
                        let tx = msg_tx.clone();
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_secs(simse_cli::constants::CTRL_C_TIMEOUT_SECS)).await;
                            let _ = tx.send(AppMessage::CtrlCTimeout);
                        });
                    }
                    let (new_app, effects) = update(app, msg);
                    app = new_app;
                    pending_effects.extend(effects);
                }
            }
            Some(msg) = msg_rx.recv() => {
                let (new_app, effects) = update(app, msg);
                app = new_app;
                pending_effects.extend(effects);
            }
            _ = tokio::time::sleep(tick_duration), if needs_tick => {
                let (new_app, effects) = update(app, AppMessage::Tick);
                app = new_app;
                pending_effects.extend(effects);
            }
        }

        // Dispatch effects.
        for effect in pending_effects {
            match effect {
                Effect::Quit => {
                    let mut rt = runtime.lock().await;
                    rt.disconnect_tunnel().await;
                    return Ok(());
                }
                Effect::Abort => {
                    if let Ok(rt) = runtime.try_lock() {
                        rt.abort();
                    }
                }
                Effect::Bridge(action) => {
                    if let Ok(mut rt) = runtime.try_lock() {
                        let action_name = action.action_name().to_string();
                        let result_msg = rt.dispatch_bridge_action(action).await;
                        let (new_app, _) = update(app, result_msg);
                        app = new_app;

                        // After successful login, connect tunnel.
                        if action_name == "login"
                            && !app
                                .output
                                .last()
                                .is_none_or(|o| matches!(o, OutputItem::Error { .. }))
                        {
                            match rt.connect_tunnel().await {
                                Ok((_tunnel_id, incoming_rx)) => {
                                    let tunnel = rt.tunnel().expect("tunnel set").clone();
                                    event_loop::spawn_token_refresh(tunnel.clone());
                                    let handle = spawn_tunnel_handler(
                                        incoming_rx,
                                        tunnel,
                                        Arc::clone(&runtime),
                                    );
                                    rt.set_tunnel_handler(handle);
                                    if let Some(auth) = simse_cli::auth::load_auth() {
                                        let (new_app, _) = update(
                                            app,
                                            AppMessage::RemoteStatus {
                                                connected: true,
                                                email: Some(auth.email),
                                            },
                                        );
                                        app = new_app;
                                    }
                                }
                                Err(_e) => {
                                    let (new_app, _) = update(
                                        app,
                                        AppMessage::RemoteStatus {
                                            connected: false,
                                            email: None,
                                        },
                                    );
                                    app = new_app;
                                }
                            }
                        }

                        if action_name == "logout" {
                            let (new_app, _) = update(
                                app,
                                AppMessage::RemoteStatus {
                                    connected: false,
                                    email: None,
                                },
                            );
                            app = new_app;
                        }
                    } else {
                        deferred_effect = Some(Effect::Bridge(action));
                    }
                }
                Effect::SubmitChat(text) => {
                    let mut rt = runtime.lock().await;
                    if !rt.is_connected() && !rt.needs_onboarding() {
                        match rt.connect_inference().await {
                            Ok(()) => {
                                let ctx = rt.build_command_context();
                                let (new_app, _) = update(app, AppMessage::RefreshContext(ctx));
                                app = new_app;
                            }
                            Err(e) => {
                                let (new_app, _) = update(
                                    app,
                                    AppMessage::LoopError(format!("ACP connection failed: {e}")),
                                );
                                app = new_app;
                                continue;
                            }
                        }
                    }
                    drop(rt);

                    let (new_app, _) = update(app, AppMessage::StreamStart);
                    app = new_app;
                    terminal.draw(|frame| view(&app, frame))?;

                    let rt = Arc::clone(&runtime);
                    let tx = msg_tx.clone();
                    tokio::spawn(async move {
                        let callbacks = create_loop_callbacks(tx.clone());
                        match rt.lock().await.handle_submit(&text, callbacks).await {
                            Ok(final_text) => {
                                let _ = tx.send(AppMessage::StreamEnd { text: final_text });
                            }
                            Err(e) => {
                                let _ = tx.send(AppMessage::LoopError(e.to_string()));
                            }
                        }
                    });
                }
            }
        }
    }
}

fn map_event(event: Event, search_active: bool) -> Option<AppMessage> {
    match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => {
            if search_active {
                return match (key.code, key.modifiers) {
                    (KeyCode::Esc, _) => Some(AppMessage::SearchClose),
                    (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
                        Some(AppMessage::SearchPrev)
                    }
                    (KeyCode::Enter, _) => Some(AppMessage::SearchNext),
                    (KeyCode::Backspace, _) => Some(AppMessage::SearchBackspace),
                    (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                        Some(AppMessage::SearchClose)
                    }
                    (KeyCode::Char(c), _) => Some(AppMessage::SearchInput(c)),
                    _ => None,
                };
            }

            match (key.code, key.modifiers) {
                (KeyCode::Char('c'), m) if m.contains(KeyModifiers::CONTROL) => {
                    Some(AppMessage::CtrlC)
                }
                (KeyCode::Char('l'), m) if m.contains(KeyModifiers::CONTROL) => {
                    Some(AppMessage::CtrlL)
                }
                (KeyCode::Char('f'), m) if m.contains(KeyModifiers::CONTROL) => {
                    Some(AppMessage::SearchOpen)
                }
                (KeyCode::Char('a'), m) if m.contains(KeyModifiers::CONTROL) => {
                    Some(AppMessage::SelectAll)
                }
                (KeyCode::Esc, _) => Some(AppMessage::Escape),
                (KeyCode::BackTab, _) => Some(AppMessage::ShiftTab),
                (KeyCode::Tab, _) => Some(AppMessage::Tab),
                (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => Some(AppMessage::NewLine),
                (KeyCode::Enter, _) => Some(AppMessage::Submit),
                (KeyCode::Backspace, m) if m.contains(KeyModifiers::ALT) => {
                    Some(AppMessage::DeleteWordBack)
                }
                (KeyCode::Backspace, _) => Some(AppMessage::Backspace),
                (KeyCode::Delete, _) => Some(AppMessage::Delete),
                (KeyCode::Left, m) if m.contains(KeyModifiers::SHIFT) => {
                    Some(AppMessage::SelectLeft)
                }
                (KeyCode::Right, m) if m.contains(KeyModifiers::SHIFT) => {
                    Some(AppMessage::SelectRight)
                }
                (KeyCode::Home, m) if m.contains(KeyModifiers::SHIFT) => {
                    Some(AppMessage::SelectHome)
                }
                (KeyCode::End, m) if m.contains(KeyModifiers::SHIFT) => Some(AppMessage::SelectEnd),
                (KeyCode::Left, m)
                    if m.contains(KeyModifiers::ALT) || m.contains(KeyModifiers::CONTROL) =>
                {
                    Some(AppMessage::WordLeft)
                }
                (KeyCode::Right, m)
                    if m.contains(KeyModifiers::ALT) || m.contains(KeyModifiers::CONTROL) =>
                {
                    Some(AppMessage::WordRight)
                }
                (KeyCode::Left, _) => Some(AppMessage::CursorLeft),
                (KeyCode::Right, _) => Some(AppMessage::CursorRight),
                (KeyCode::Home, _) => Some(AppMessage::Home),
                (KeyCode::End, _) => Some(AppMessage::End),
                (KeyCode::Up, _) => Some(AppMessage::HistoryUp),
                (KeyCode::Down, _) => Some(AppMessage::HistoryDown),
                (KeyCode::PageUp, _) => Some(AppMessage::ScrollUp(10)),
                (KeyCode::PageDown, _) => Some(AppMessage::ScrollDown(10)),
                (KeyCode::Char(c), _) => Some(AppMessage::CharInput(c)),
                _ => None,
            }
        }
        Event::Resize(w, h) => Some(AppMessage::Resize {
            width: w,
            height: h,
        }),
        Event::Paste(text) => Some(AppMessage::Paste(text)),
        _ => None,
    }
}
