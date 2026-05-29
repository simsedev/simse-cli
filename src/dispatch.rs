//! Command dispatch — routes slash-command names to handler modules.

use crate::ui_core::commands::registry::{CommandDefinition, all_commands};

use crate::commands::{self, CommandContext, CommandOutput};

/// Parse a `/command args` line into `(command_name, args)`.
///
/// Returns `None` if the input does not start with `/` or has no command name.
pub fn parse_command_line(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let without_slash = &trimmed[1..];
    if without_slash.is_empty() {
        return None;
    }

    let mut parts = without_slash.splitn(2, ' ');
    let command = parts.next()?.to_lowercase();
    let args = parts.next().unwrap_or("").to_string();

    if command.is_empty() {
        return None;
    }

    Some((command, args))
}

/// Holds UI state needed by commands that require context beyond the arguments.
pub struct DispatchContext {
    pub verbose: bool,
    pub plan: bool,
    pub total_tokens: u64,
    pub context_percent: u8,
    pub commands: Vec<CommandDefinition>,
    pub cmd_ctx: CommandContext,
}

impl Default for DispatchContext {
    fn default() -> Self {
        Self {
            verbose: false,
            plan: false,
            total_tokens: 0,
            context_percent: 0,
            commands: all_commands(),
            cmd_ctx: CommandContext::default(),
        }
    }
}

impl DispatchContext {
    pub fn dispatch(&self, command: &str, args: &str) -> Vec<CommandOutput> {
        dispatch_inner(command, args, self)
    }
}

/// Dispatch a slash command to the appropriate handler.
pub fn dispatch_command(command: &str, args: &str) -> Vec<CommandOutput> {
    let ctx = DispatchContext::default();
    dispatch_inner(command, args, &ctx)
}

fn dispatch_inner(command: &str, args: &str, ctx: &DispatchContext) -> Vec<CommandOutput> {
    match command {
        // ── Session ──────────────────────────────────────────
        "server" => commands::session::handle_server(args, &ctx.cmd_ctx),
        "model" => commands::session::handle_model(args, &ctx.cmd_ctx),
        "status" => commands::session::handle_status(&ctx.cmd_ctx),

        // ── Files ────────────────────────────────────────────
        "diff" => commands::files::handle_diff(args),

        // ── Meta ─────────────────────────────────────────────
        "help" | "?" => commands::meta::handle_help(args, &ctx.commands),
        // "clear" and "exit"/"quit"/"q" are handled directly by update/input.rs
        // before reaching dispatch_inner — no sentinel needed.
        "verbose" | "v" => commands::meta::handle_verbose(args, ctx.verbose),
        "permissions" | "plan" => commands::meta::handle_plan(args, ctx.plan),
        "context" => commands::meta::handle_context(ctx.total_tokens, ctx.context_percent),
        "compact" => commands::meta::handle_compact(),
        "login" => vec![
            CommandOutput::Info("Logging in...".into()),
            CommandOutput::BridgeRequest(commands::BridgeAction::Login),
        ],
        "logout" => vec![
            CommandOutput::Info("Logging out...".into()),
            CommandOutput::BridgeRequest(commands::BridgeAction::Logout),
        ],
        "resume" => {
            let id = args.trim();
            if id.is_empty() {
                vec![
                    CommandOutput::Info("Resuming latest session...".into()),
                    CommandOutput::BridgeRequest(commands::BridgeAction::ResumeSession {
                        id: String::new(),
                    }),
                ]
            } else {
                vec![
                    CommandOutput::Info(format!("Resuming session {id}...")),
                    CommandOutput::BridgeRequest(commands::BridgeAction::ResumeSession {
                        id: id.to_string(),
                    }),
                ]
            }
        }
        "fork" => {
            let at = args.trim().parse::<usize>().ok();
            vec![
                CommandOutput::Info("Forking current session...".into()),
                CommandOutput::BridgeRequest(commands::BridgeAction::ForkSession { at }),
            ]
        }
        "mcp" => {
            let sub = args.trim().to_lowercase();
            match sub.as_str() {
                "restart" => vec![
                    CommandOutput::Info("Restarting MCP connections...".into()),
                    CommandOutput::BridgeRequest(commands::BridgeAction::McpRestart),
                ],
                _ => commands::session::handle_status(&ctx.cmd_ctx),
            }
        }
        "acp" => {
            let sub = args.trim().to_lowercase();
            match sub.as_str() {
                "restart" => vec![
                    CommandOutput::Info("Restarting ACP connection...".into()),
                    CommandOutput::BridgeRequest(commands::BridgeAction::AcpRestart),
                ],
                _ => commands::session::handle_status(&ctx.cmd_ctx),
            }
        }
        "plugins" => {
            let plugins = crate::config::discover_plugins(&crate::config::default_data_dir());
            let type_filter = args.trim();
            let filtered: Vec<_> = if type_filter.is_empty() {
                plugins
            } else {
                plugins
                    .into_iter()
                    .filter(|p| p.kind == type_filter)
                    .collect()
            };
            if filtered.is_empty() {
                vec![CommandOutput::Info("No plugins found.".into())]
            } else {
                let headers = vec![
                    "Type".into(),
                    "Name".into(),
                    "Version".into(),
                    "Description".into(),
                ];
                let rows: Vec<Vec<String>> = filtered
                    .iter()
                    .map(|p| {
                        vec![
                            p.kind.clone(),
                            p.name.clone(),
                            p.version.clone(),
                            p.description.clone(),
                        ]
                    })
                    .collect();
                vec![CommandOutput::Table { headers, rows }]
            }
        }

        // ── Unknown ──────────────────────────────────────────
        other => {
            let mut suggestions: Vec<&str> = ctx
                .commands
                .iter()
                .filter(|cmd| {
                    crate::levenshtein::levenshtein(other, &cmd.name)
                        <= crate::constants::TYPO_SUGGESTION_DISTANCE
                })
                .map(|cmd| cmd.name.as_str())
                .collect();
            suggestions.sort();
            suggestions.dedup();

            if suggestions.is_empty() {
                vec![CommandOutput::Error(format!("Unknown command: /{other}"))]
            } else {
                vec![CommandOutput::Error(format!(
                    "Unknown command: /{other}. Did you mean /{}?",
                    suggestions.join(", /")
                ))]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_valid_command_no_args() {
        assert_eq!(
            parse_command_line("/help"),
            Some(("help".into(), String::new()))
        );
    }

    #[test]
    fn parse_valid_command_with_args() {
        assert_eq!(
            parse_command_line("/model gpt-4o"),
            Some(("model".into(), "gpt-4o".into()))
        );
    }

    #[test]
    fn parse_no_slash_returns_none() {
        assert_eq!(parse_command_line("help"), None);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert_eq!(parse_command_line(""), None);
    }

    #[test]
    fn parse_just_slash_returns_none() {
        assert_eq!(parse_command_line("/"), None);
    }

    #[test]
    fn parse_lowercases_command() {
        assert_eq!(
            parse_command_line("/HELP"),
            Some(("help".into(), String::new()))
        );
    }

    #[test]
    fn dispatch_help() {
        let out = dispatch_command("help", "");
        assert!(matches!(
            &out[0],
            CommandOutput::Success(msg) if msg.contains("Available commands")
        ));
    }

    #[test]
    fn dispatch_unknown_command() {
        let out = dispatch_command("foobar", "");
        assert!(matches!(
            &out[0],
            CommandOutput::Error(msg) if msg.contains("/foobar")
        ));
    }

    #[test]
    fn dispatch_permissions_alias() {
        let out = dispatch_command("permissions", "");
        // Same as /plan toggle
        assert!(matches!(&out[0], CommandOutput::Success(_)));
    }
}
