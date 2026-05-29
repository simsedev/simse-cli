//! Command registry: lookup by name/alias, categorization.

use serde::{Deserialize, Serialize};

/// Command category.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCategory {
    Meta,
    Session,
    Files,
}

/// A command definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub name: String,
    pub description: String,
    pub usage: String,
    pub aliases: Vec<String>,
    pub category: CommandCategory,
    pub hidden: bool,
}

/// Look up a command by name or alias from a list of definitions.
pub fn find_command<'a>(
    commands: &'a [CommandDefinition],
    input: &str,
) -> Option<&'a CommandDefinition> {
    let query = input.to_lowercase();
    commands.iter().find(|cmd| {
        cmd.name.to_lowercase() == query || cmd.aliases.iter().any(|a| a.to_lowercase() == query)
    })
}

/// Filter commands matching a prefix (for autocomplete).
pub fn filter_commands<'a>(
    commands: &'a [CommandDefinition],
    prefix: &str,
) -> Vec<&'a CommandDefinition> {
    let prefix = prefix.to_lowercase();
    commands
        .iter()
        .filter(|cmd| {
            cmd.name.to_lowercase().contains(&prefix)
                || cmd
                    .aliases
                    .iter()
                    .any(|a| a.to_lowercase().contains(&prefix))
        })
        .collect()
}

/// Parse "on"/"off"/"true"/"false"/"1"/"0" or empty string (toggle).
pub fn parse_bool_arg(arg: &str, current: bool) -> Option<bool> {
    match arg.trim().to_lowercase().as_str() {
        "" => Some(!current),
        "on" | "true" | "1" => Some(true),
        "off" | "false" | "0" => Some(false),
        _ => None,
    }
}

/// Helper to build a `CommandDefinition` with less boilerplate.
fn cmd(
    name: &str,
    desc: &str,
    usage: &str,
    aliases: &[&str],
    category: CommandCategory,
) -> CommandDefinition {
    CommandDefinition {
        name: name.into(),
        description: desc.into(),
        usage: usage.into(),
        aliases: aliases.iter().map(|a| (*a).into()).collect(),
        category,
        hidden: false,
    }
}

/// Return all built-in command definitions.
pub fn all_commands() -> Vec<CommandDefinition> {
    use CommandCategory::*;

    vec![
        cmd(
            "help",
            "Show help information",
            "help [command]",
            &["?"],
            Meta,
        ),
        cmd("clear", "Clear the screen", "clear", &[], Meta),
        cmd(
            "verbose",
            "Toggle verbose output",
            "verbose [on|off]",
            &["v"],
            Meta,
        ),
        cmd(
            "permissions",
            "Cycle permission mode",
            "permissions",
            &["plan"],
            Meta,
        ),
        cmd(
            "context",
            "Show current context usage",
            "context",
            &[],
            Meta,
        ),
        cmd(
            "compact",
            "Compact conversation history",
            "compact",
            &[],
            Meta,
        ),
        cmd("exit", "Exit the application", "exit", &["quit", "q"], Meta),
        cmd("login", "Log in to your simse account", "login", &[], Meta),
        cmd(
            "logout",
            "Log out and clear credentials",
            "logout",
            &[],
            Meta,
        ),
        cmd("resume", "Resume a session", "resume [id]", &["r"], Session),
        cmd(
            "fork",
            "Fork the current session",
            "fork [at_message]",
            &[],
            Session,
        ),
        cmd(
            "mcp",
            "MCP server management",
            "mcp [restart]",
            &[],
            Session,
        ),
        cmd(
            "acp",
            "ACP server management",
            "acp [restart]",
            &[],
            Session,
        ),
        cmd(
            "plugins",
            "List installed plugins",
            "plugins [type]",
            &[],
            Meta,
        ),
        cmd(
            "server",
            "Show or change ACP server",
            "server [name]",
            &[],
            Session,
        ),
        cmd(
            "model",
            "Show or change model",
            "model [name]",
            &[],
            Session,
        ),
        cmd("status", "Show connection status", "status", &[], Session),
        cmd("diff", "Show file diffs", "diff [path]", &[], Files),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_command_by_name() {
        let cmds = all_commands();
        let found = find_command(&cmds, "help");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "help");
    }

    #[test]
    fn find_command_by_alias() {
        let cmds = all_commands();
        let found = find_command(&cmds, "q");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "exit");
    }

    #[test]
    fn find_command_case_insensitive() {
        let cmds = all_commands();
        assert!(find_command(&cmds, "HELP").is_some());
    }

    #[test]
    fn find_command_returns_none_for_unknown() {
        let cmds = all_commands();
        assert!(find_command(&cmds, "nonexistent").is_none());
    }

    #[test]
    fn all_commands_count() {
        let cmds = all_commands();
        assert_eq!(cmds.len(), 18, "expected 18 commands, got {}", cmds.len());
    }

    #[test]
    fn all_categories_represented() {
        let cmds = all_commands();
        let categories: std::collections::HashSet<_> = cmds.iter().map(|c| &c.category).collect();
        assert!(categories.contains(&CommandCategory::Meta));
        assert!(categories.contains(&CommandCategory::Session));
        assert!(categories.contains(&CommandCategory::Files));
    }

    #[test]
    fn parse_bool_arg_on_off() {
        assert_eq!(parse_bool_arg("on", false), Some(true));
        assert_eq!(parse_bool_arg("off", true), Some(false));
    }

    #[test]
    fn parse_bool_arg_toggle() {
        assert_eq!(parse_bool_arg("", true), Some(false));
        assert_eq!(parse_bool_arg("", false), Some(true));
    }

    #[test]
    fn permissions_alias_plan() {
        let cmds = all_commands();
        let found = find_command(&cmds, "plan");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "permissions");
    }
}
