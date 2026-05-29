use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::SkippedServer;

/// A single MCP server entry from `mcp.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub name: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    /// Executable for a `stdio` transport server. Required for `stdio`,
    /// unused for `http`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Endpoint URL for an `http` transport server. Required for `http`,
    /// unused for `stdio`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    #[serde(default)]
    pub required_env: Vec<String>,
}

fn default_transport() -> String {
    "stdio".into()
}

/// Top-level MCP config from `mcp.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct McpFileConfig {
    #[serde(default)]
    pub servers: Vec<McpServerConfig>,
}

/// Check MCP servers for missing required environment variables.
///
/// For each server that has `required_env` entries, checks the server's own
/// `env` map first, then falls back to `std::env::var()`. Servers with all
/// required vars present are returned in the first vec; servers missing any
/// required var are returned as `SkippedServer` in the second vec.
pub fn check_mcp_servers(
    servers: &[McpServerConfig],
) -> (Vec<McpServerConfig>, Vec<SkippedServer>) {
    let mut valid = Vec::new();
    let mut skipped = Vec::new();

    for server in servers {
        if server.required_env.is_empty() {
            valid.push(server.clone());
            continue;
        }

        let missing: Vec<String> = server
            .required_env
            .iter()
            .filter(|key| {
                // Check server.env first, then process env
                let val = server
                    .env
                    .get(key.as_str())
                    .cloned()
                    .or_else(|| std::env::var(key).ok());
                match val {
                    Some(v) => v.is_empty() || v.starts_with("${"),
                    None => true,
                }
            })
            .cloned()
            .collect();

        if missing.is_empty() {
            valid.push(server.clone());
        } else {
            skipped.push(SkippedServer {
                name: server.name.clone(),
                missing_env: missing,
            });
        }
    }

    (valid, skipped)
}
