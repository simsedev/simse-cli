//! CLI argument parsing via clap derive.

use clap::{Parser, Subcommand};

// ---------------------------------------------------------------------------
// Top-level CLI
// ---------------------------------------------------------------------------

/// simse — AI-powered coding agent.
#[derive(Debug, Parser)]
#[command(name = "simse", version, about = "simse coding agent")]
pub struct SimSeCli {
    /// Enable verbose output.
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output format: text (default) or json.
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Use an alternative OpenAI-compatible provider URL.
    #[arg(long)]
    pub provider: Option<String>,

    /// Continue the last session for this working directory.
    #[arg(short = 'c', long = "continue")]
    pub continue_session: bool,

    /// Override model name (used with --provider).
    #[arg(long)]
    pub model: Option<String>,

    /// Resume a specific session by ID.
    #[arg(long)]
    pub resume: Option<String>,

    /// Non-interactive mode: print output to stdout instead of TUI.
    #[arg(long)]
    pub print: bool,

    /// The prompt text (for print mode).
    #[arg(long, short)]
    pub prompt: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

// ---------------------------------------------------------------------------
// Subcommands
// ---------------------------------------------------------------------------

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Log in to your simse account.
    Login,

    /// Log out and clear stored credentials.
    Logout,

    /// Resume a previous session in interactive mode.
    Resume {
        /// Session ID. Omit to resume the most recent session.
        id: Option<String>,
    },

    /// Fork a session at a specific message.
    Fork {
        /// Session ID to fork.
        id: String,

        /// Message index to fork at.
        #[arg(long)]
        at: Option<usize>,
    },

    /// Manage MCP servers.
    Mcp {
        #[command(subcommand)]
        action: ProtocolAction,
    },

    /// Manage ACP servers.
    Acp {
        #[command(subcommand)]
        action: ProtocolAction,
    },

    /// Manage plugins (skills, hooks, mcp, acp).
    Plugins {
        #[command(subcommand)]
        action: PluginsAction,
    },

    /// Run as a background daemon, maintaining a persistent tunnel connection.
    ///
    /// The daemon keeps simse connected to the cloud so the web dashboard
    /// can access your local workspace (files, shell, network, tools).
    /// Use Ctrl-C or `kill` to stop.
    Daemon {
        /// Workspace directory to expose (defaults to current directory).
        #[arg(long)]
        work_dir: Option<String>,

        /// Detach from the terminal (run in background).
        #[arg(short, long)]
        detach: bool,

        /// Write the daemon PID to this file.
        #[arg(long)]
        pid_file: Option<String>,
    },
}

// ---------------------------------------------------------------------------
// Protocol subcommands (shared by mcp/acp)
// ---------------------------------------------------------------------------

#[derive(Debug, Subcommand)]
pub enum ProtocolAction {
    /// Restart all connections.
    Restart,

    /// Show connection status.
    Status,
}

// ---------------------------------------------------------------------------
// Plugins subcommands
// ---------------------------------------------------------------------------

#[derive(Debug, Subcommand)]
pub enum PluginsAction {
    /// List installed plugins, optionally filtered by type.
    List {
        /// Filter by type: skill, hook, mcp, acp.
        #[arg(long, rename_all = "lowercase")]
        r#type: Option<String>,
    },
    /// Search the marketplace for installable plugins.
    Search,
    /// Install a plugin from the marketplace.
    Install {
        /// Name of the plugin to install.
        name: String,
    },
    /// Remove an installed plugin.
    Remove {
        /// Name of the plugin to remove.
        name: String,
    },
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Parse from a string slice (prepends program name).
    fn parse(args: &[&str]) -> SimSeCli {
        let mut v = vec!["simse"];
        v.extend(args);
        SimSeCli::parse_from(v)
    }

    fn try_parse(args: &[&str]) -> Result<SimSeCli, clap::Error> {
        let mut v = vec!["simse"];
        v.extend(args);
        SimSeCli::try_parse_from(v)
    }

    #[test]
    fn default_no_args() {
        let cli = parse(&[]);
        assert!(cli.command.is_none());
        assert!(!cli.verbose);
        assert!(!cli.continue_session);
        assert!(!cli.print);
        assert_eq!(cli.format, "text");
        assert_eq!(cli.provider, None);
        assert_eq!(cli.model, None);
        assert_eq!(cli.prompt, None);
        assert_eq!(cli.resume, None);
    }

    #[test]
    fn verbose_short() {
        let cli = parse(&["-v"]);
        assert!(cli.verbose);
    }

    #[test]
    fn verbose_long() {
        let cli = parse(&["--verbose"]);
        assert!(cli.verbose);
    }

    #[test]
    fn continue_session() {
        let cli = parse(&["-c"]);
        assert!(cli.continue_session);
    }

    #[test]
    fn provider_flag() {
        let cli = parse(&["--provider", "http://localhost:11434"]);
        assert_eq!(cli.provider.as_deref(), Some("http://localhost:11434"));
    }

    #[test]
    fn print_mode_with_prompt() {
        let cli = parse(&["--print", "-p", "hello world"]);
        assert!(cli.print);
        assert_eq!(cli.prompt.as_deref(), Some("hello world"));
    }

    #[test]
    fn print_mode_with_continue() {
        let cli = parse(&["--print", "-c", "-p", "continue this"]);
        assert!(cli.print);
        assert!(cli.continue_session);
        assert_eq!(cli.prompt.as_deref(), Some("continue this"));
    }

    #[test]
    fn not_print_mode_by_default() {
        let cli = parse(&[]);
        assert!(!cli.print);
        assert_eq!(cli.prompt, None);
    }

    #[test]
    fn model_flag() {
        let cli = parse(&[
            "--provider",
            "http://localhost:11434",
            "--model",
            "qwen3:8b",
        ]);
        assert_eq!(cli.provider.as_deref(), Some("http://localhost:11434"));
        assert_eq!(cli.model.as_deref(), Some("qwen3:8b"));
    }

    #[test]
    fn resume_flag() {
        let cli = parse(&["--resume", "sess-abc-123"]);
        assert_eq!(cli.resume.as_deref(), Some("sess-abc-123"));
    }

    #[test]
    fn print_with_resume() {
        let cli = parse(&["--print", "--resume", "sess-1", "-p", "continue"]);
        assert!(cli.print);
        assert_eq!(cli.resume.as_deref(), Some("sess-1"));
        assert_eq!(cli.prompt.as_deref(), Some("continue"));
    }

    #[test]
    fn login_subcommand() {
        let cli = parse(&["login"]);
        assert!(matches!(cli.command, Some(Command::Login)));
    }

    #[test]
    fn logout_subcommand() {
        let cli = parse(&["logout"]);
        assert!(matches!(cli.command, Some(Command::Logout)));
    }

    #[test]
    fn resume_with_id() {
        let cli = parse(&["resume", "sess-abc-123"]);
        match cli.command {
            Some(Command::Resume { id }) => {
                assert_eq!(id.as_deref(), Some("sess-abc-123"));
            }
            other => panic!("expected Resume, got {other:?}"),
        }
    }

    #[test]
    fn resume_without_id() {
        let cli = parse(&["resume"]);
        match cli.command {
            Some(Command::Resume { id }) => assert_eq!(id, None),
            other => panic!("expected Resume, got {other:?}"),
        }
    }

    #[test]
    fn fork_subcommand() {
        let cli = parse(&["fork", "sess-1", "--at", "5"]);
        match cli.command {
            Some(Command::Fork { id, at }) => {
                assert_eq!(id, "sess-1");
                assert_eq!(at, Some(5));
            }
            other => panic!("expected Fork, got {other:?}"),
        }
    }

    #[test]
    fn mcp_restart() {
        let cli = parse(&["mcp", "restart"]);
        assert!(matches!(
            cli.command,
            Some(Command::Mcp {
                action: ProtocolAction::Restart
            })
        ));
    }

    #[test]
    fn acp_status() {
        let cli = parse(&["acp", "status"]);
        assert!(matches!(
            cli.command,
            Some(Command::Acp {
                action: ProtocolAction::Status
            })
        ));
    }

    #[test]
    fn multiple_flags_with_print() {
        let cli = parse(&["-v", "--format", "json", "--print", "-p", "test"]);
        assert!(cli.verbose);
        assert_eq!(cli.format, "json");
        assert!(cli.print);
        assert_eq!(cli.prompt.as_deref(), Some("test"));
    }

    #[test]
    fn unknown_flag_errors() {
        let result = try_parse(&["--unknown-flag"]);
        assert!(result.is_err());
    }

    #[test]
    fn format_json() {
        let cli = parse(&["--format", "json"]);
        assert_eq!(cli.format, "json");
    }

    #[test]
    fn verbose_global_with_subcommand() {
        let cli = parse(&["login", "-v"]);
        assert!(cli.verbose);
    }
}
