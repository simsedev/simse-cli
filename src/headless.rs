//! Print mode — non-interactive, same backend as TUI.
//!
//! Activated via `simse --print -p <prompt>` or `echo <prompt> | simse --print`.

use crate::event_loop::CliRuntime;
use simse_core::agentic_loop::LoopCallbacks;

/// Arguments for print mode.
pub struct PrintArgs {
    pub prompt: String,
    pub format: String,
    pub resume: Option<String>,
    pub continue_session: bool,
    pub verbose: bool,
}

/// Run the CLI in print mode with a pre-configured runtime.
///
/// The runtime must have its model client already set (via `set_model_client()`
/// or `connect()`). This is the same runtime the TUI would use.
pub async fn run_print(args: PrintArgs, rt: &mut CliRuntime) -> i32 {
    // Optionally load a session.
    if let Some(ref id) = args.resume {
        if !rt.session_exists(id) {
            eprintln!("error: session not found: {id}");
            return 1;
        }
        if let Err(e) = rt.load_session(id) {
            eprintln!("error: failed to load session: {e}");
            return 1;
        }
    } else if args.continue_session {
        let work_dir = std::env::current_dir()
            .unwrap_or_default()
            .display()
            .to_string();
        match rt.latest_session(&work_dir) {
            Some(id) => {
                if let Err(e) = rt.load_session(&id) {
                    eprintln!("error: failed to load session: {e}");
                    return 1;
                }
            }
            None => {
                eprintln!("error: no session found for the current directory");
                return 1;
            }
        }
    }

    if !rt.is_connected() {
        eprintln!("error: not logged in. Run `simse login` or use --provider <url>.");
        return 1;
    }

    // Ensure a session exists so this run is resumable with `--continue`.
    // Without this, print mode was stateless: it loaded sessions on resume
    // but never created or saved one, so `--continue` always reported "no
    // session found for the current directory".
    let work_dir = std::env::current_dir()
        .unwrap_or_default()
        .display()
        .to_string();
    if let Err(e) = rt.ensure_session(&work_dir) {
        eprintln!("warning: session persistence unavailable: {e}");
    }

    // In verbose mode, surface tool-call activity on stderr so the user can
    // see what the agent did. The final response still goes to stdout, so the
    // command stays pipe-friendly.
    let callbacks = if args.verbose {
        LoopCallbacks {
            on_tool_call_start: Some(Box::new(
                |req: &simse_core::tools::types::ToolCallRequest| {
                    eprintln!("  \u{2192} {}", req.name);
                },
            )),
            on_tool_call_end: Some(Box::new(
                |res: &simse_core::tools::types::ToolCallResult| {
                    eprintln!(
                        "  \u{2190} {} [{}]",
                        res.name,
                        if res.is_error { "error" } else { "ok" }
                    );
                },
            )),
            on_error: Some(Box::new(|e: &simse_core::error::SimseError| {
                eprintln!("  ! {e}");
            })),
            ..Default::default()
        }
    } else {
        LoopCallbacks::default()
    };
    rt.persist_message("user", &args.prompt);
    match rt.handle_submit(&args.prompt, callbacks).await {
        Ok(response) => {
            rt.persist_message("assistant", &response);
            if args.format == "json" {
                let json = serde_json::json!({
                    "role": "assistant",
                    "content": response,
                });
                println!("{}", serde_json::to_string(&json).unwrap_or_default());
            } else {
                println!("{response}");
            }
            0
        }
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}
