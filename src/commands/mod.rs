//! Command handlers for the simse CLI.

pub mod files;
pub mod meta;
pub mod session;

/// The result type returned by every command handler.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandOutput {
    Success(String),
    Error(String),
    Info(String),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    BridgeRequest(BridgeAction),
}

/// Async operations that the event loop will dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum BridgeAction {
    SwitchServer { name: String },
    SwitchModel { name: String },
    ResumeSession { id: String },
    ForkSession { at: Option<usize> },
    McpRestart,
    AcpRestart,
    DiffFiles { path: Option<String> },
    Compact,
    Login,
    Logout,
}

impl BridgeAction {
    pub fn action_name(&self) -> &'static str {
        match self {
            BridgeAction::SwitchServer { .. } => "switch-server",
            BridgeAction::SwitchModel { .. } => "switch-model",
            BridgeAction::ResumeSession { .. } => "resume-session",
            BridgeAction::ForkSession { .. } => "fork-session",
            BridgeAction::McpRestart => "mcp-restart",
            BridgeAction::AcpRestart => "acp-restart",
            BridgeAction::DiffFiles { .. } => "diff-files",
            BridgeAction::Compact => "compact",
            BridgeAction::Login => "login",
            BridgeAction::Logout => "logout",
        }
    }
}

/// Read-only snapshot of runtime state available to command handlers.
#[derive(Debug, Clone, Default)]
pub struct CommandContext {
    pub server_name: Option<String>,
    pub model_name: Option<String>,
    pub session_id: Option<String>,
    pub acp_connected: bool,
}

/// Installed plugin descriptor (used by `simse plugins list`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PluginInfo {
    pub name: String,
    pub kind: String,
    pub version: String,
    pub description: String,
}

/// Format a table as plain text.
pub fn format_table(headers: &[String], rows: &[Vec<String>]) -> String {
    if headers.is_empty() {
        return String::new();
    }

    let mut widths: Vec<usize> = headers.iter().map(|h| h.len()).collect();
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < widths.len() {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    let mut out = String::new();

    for (i, h) in headers.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(&format!("{:<width$}", h, width = widths[i]));
    }
    out.push('\n');

    for (i, w) in widths.iter().enumerate() {
        if i > 0 {
            out.push_str("  ");
        }
        out.push_str(&"-".repeat(*w));
    }
    out.push('\n');

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                out.push_str("  ");
            }
            let w = widths.get(i).copied().unwrap_or(0);
            out.push_str(&format!("{:<width$}", cell, width = w));
        }
        out.push('\n');
    }

    out
}
