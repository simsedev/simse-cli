//! Meta commands: `/help`, `/clear`, `/exit`, `/verbose`, `/plan`, `/context`,
//! `/compact`, `/shortcuts`.
//!
//! These commands are largely UI-only (no bridge calls needed).  The handlers
//! return `CommandOutput` items that the CLI dispatch layer can act on directly.

use crate::ui_core::commands::registry::{CommandCategory, CommandDefinition, parse_bool_arg};
use std::collections::BTreeMap;

use super::{BridgeAction, CommandOutput};

/// `/help [command]` -- show help information.
pub fn handle_help(args: &str, commands: &[CommandDefinition]) -> Vec<CommandOutput> {
    let query = args.trim();
    if query.is_empty() {
        let text = format_help_text(commands);
        vec![CommandOutput::Success(text)]
    } else {
        // Look up a specific command.
        let lower = query.to_lowercase();
        if let Some(cmd) = commands.iter().find(|c| {
            c.name.to_lowercase() == lower || c.aliases.iter().any(|a| a.to_lowercase() == lower)
        }) {
            let aliases = if cmd.aliases.is_empty() {
                String::new()
            } else {
                format!("  Aliases: {}\n", cmd.aliases.join(", "))
            };
            let text = format!(
                "/{name} -- {desc}\n  Usage: /{usage}\n{aliases}",
                name = cmd.name,
                desc = cmd.description,
                usage = cmd.usage,
            );
            vec![CommandOutput::Success(text)]
        } else {
            vec![CommandOutput::Error(format!(
                "Unknown command: \"{query}\". Type /help for a list."
            ))]
        }
    }
}

/// `/verbose [on|off]` -- toggle verbose mode.
pub fn handle_verbose(args: &str, current: bool) -> Vec<CommandOutput> {
    match parse_bool_arg(args.trim(), current) {
        Some(val) => {
            let state = if val { "on" } else { "off" };
            vec![CommandOutput::Success(format!("Verbose mode {state}."))]
        }
        None => vec![CommandOutput::Error(format!(
            "Invalid argument: \"{}\". Use on/off/true/false.",
            args.trim()
        ))],
    }
}

/// `/plan [on|off]` -- toggle plan mode.
pub fn handle_plan(args: &str, current: bool) -> Vec<CommandOutput> {
    match parse_bool_arg(args.trim(), current) {
        Some(val) => {
            let state = if val { "on" } else { "off" };
            vec![CommandOutput::Success(format!("Plan mode {state}."))]
        }
        None => vec![CommandOutput::Error(format!(
            "Invalid argument: \"{}\". Use on/off/true/false.",
            args.trim()
        ))],
    }
}

/// `/context` -- show current context usage.
pub fn handle_context(total_tokens: u64, context_percent: u8) -> Vec<CommandOutput> {
    let tokens = format_tokens(total_tokens);
    vec![CommandOutput::Success(format!(
        "Tokens: {tokens} | Context: {context_percent}%"
    ))]
}

/// `/compact` -- request conversation compaction.
pub fn handle_compact() -> Vec<CommandOutput> {
    vec![
        CommandOutput::Info("Compacting conversation history...".into()),
        CommandOutput::BridgeRequest(BridgeAction::Compact),
    ]
}

// ── Helpers ─────────────────────────────────────────────

/// Format token count for display.
pub fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

/// Format help text grouped by category.
fn format_help_text(commands: &[CommandDefinition]) -> String {
    let mut groups: BTreeMap<String, Vec<&CommandDefinition>> = BTreeMap::new();
    for cmd in commands {
        if cmd.hidden {
            continue;
        }
        let cat = match cmd.category {
            CommandCategory::Meta => "Meta",
            CommandCategory::Session => "Session",
            CommandCategory::Files => "Files",
        };
        groups.entry(cat.into()).or_default().push(cmd);
    }

    let mut out = String::from("Available commands:\n");
    for (cat, cmds) in &groups {
        out.push_str(&format!("\n  {cat}:\n"));
        for cmd in cmds {
            let aliases = if cmd.aliases.is_empty() {
                String::new()
            } else {
                format!(" ({})", cmd.aliases.join(", "))
            };
            out.push_str(&format!(
                "    /{}{} -- {}\n",
                cmd.name, aliases, cmd.description
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_core::commands::registry::all_commands;

    fn test_commands() -> Vec<CommandDefinition> {
        all_commands()
    }

    // ── /help ────────────────────────────────────────────

    #[test]
    fn help_no_args_lists_all() {
        let cmds = test_commands();
        let out = handle_help("", &cmds);
        assert_eq!(out.len(), 1);
        assert!(
            matches!(&out[0], CommandOutput::Success(text) if text.contains("Available commands"))
        );
    }

    #[test]
    fn help_shows_categories() {
        let cmds = test_commands();
        let out = handle_help("", &cmds);
        if let CommandOutput::Success(text) = &out[0] {
            assert!(text.contains("Meta"));
            assert!(text.contains("Session"));
            assert!(text.contains("Files"));
        } else {
            panic!("Expected Success");
        }
    }

    #[test]
    fn help_specific_command() {
        let cmds = test_commands();
        let out = handle_help("help", &cmds);
        assert!(matches!(&out[0], CommandOutput::Success(text) if text.contains("help")));
    }

    #[test]
    fn help_specific_command_by_alias() {
        let cmds = test_commands();
        let out = handle_help("?", &cmds);
        assert!(matches!(&out[0], CommandOutput::Success(text) if text.contains("help")));
    }

    #[test]
    fn help_unknown_command() {
        let cmds = test_commands();
        let out = handle_help("nonexistent", &cmds);
        assert!(matches!(&out[0], CommandOutput::Error(msg) if msg.contains("Unknown")));
    }

    #[test]
    fn help_case_insensitive() {
        let cmds = test_commands();
        let out = handle_help("HELP", &cmds);
        assert!(matches!(&out[0], CommandOutput::Success(_)));
    }

    // ── /verbose ─────────────────────────────────────────

    #[test]
    fn verbose_toggle_off_to_on() {
        let out = handle_verbose("", false);
        assert!(matches!(&out[0], CommandOutput::Success(msg) if msg.contains("on")));
    }

    #[test]
    fn verbose_toggle_on_to_off() {
        let out = handle_verbose("", true);
        assert!(matches!(&out[0], CommandOutput::Success(msg) if msg.contains("off")));
    }

    #[test]
    fn verbose_explicit_on() {
        let out = handle_verbose("on", false);
        assert!(matches!(&out[0], CommandOutput::Success(msg) if msg.contains("on")));
    }

    #[test]
    fn verbose_explicit_off() {
        let out = handle_verbose("off", true);
        assert!(matches!(&out[0], CommandOutput::Success(msg) if msg.contains("off")));
    }

    #[test]
    fn verbose_invalid() {
        let out = handle_verbose("maybe", false);
        assert!(matches!(&out[0], CommandOutput::Error(msg) if msg.contains("maybe")));
    }

    // ── /plan ────────────────────────────────────────────

    #[test]
    fn plan_toggle() {
        let out = handle_plan("", false);
        assert!(matches!(&out[0], CommandOutput::Success(msg) if msg.contains("on")));
    }

    #[test]
    fn plan_explicit() {
        let out = handle_plan("off", true);
        assert!(matches!(&out[0], CommandOutput::Success(msg) if msg.contains("off")));
    }

    #[test]
    fn plan_invalid() {
        let out = handle_plan("nah", false);
        assert!(matches!(&out[0], CommandOutput::Error(_)));
    }

    // ── /context ─────────────────────────────────────────

    #[test]
    fn context_shows_tokens_and_percent() {
        let out = handle_context(1500, 42);
        assert!(
            matches!(&out[0], CommandOutput::Success(msg) if msg.contains("1.5k") && msg.contains("42%"))
        );
    }

    #[test]
    fn context_zero_tokens() {
        let out = handle_context(0, 0);
        assert!(
            matches!(&out[0], CommandOutput::Success(msg) if msg.contains("0") && msg.contains("0%"))
        );
    }

    // ── /compact ─────────────────────────────────────────

    #[test]
    fn compact_returns_bridge_request() {
        let out = handle_compact();
        assert_eq!(out.len(), 2);
        assert!(
            matches!(&out[0], CommandOutput::Info(msg) if msg == "Compacting conversation history...")
        );
        assert!(matches!(
            &out[1],
            CommandOutput::BridgeRequest(BridgeAction::Compact)
        ));
    }

    // ── /shortcuts ───────────────────────────────────────

    // ── format_tokens ────────────────────────────────────

    #[test]
    fn format_tokens_small() {
        assert_eq!(format_tokens(42), "42");
        assert_eq!(format_tokens(999), "999");
    }

    #[test]
    fn format_tokens_thousands() {
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(42000), "42.0k");
    }

    #[test]
    fn format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.0M");
        assert_eq!(format_tokens(2_500_000), "2.5M");
    }
}
