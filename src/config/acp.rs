use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single ACP server entry from `acp.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpServerConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
    pub default_agent: Option<String>,
    pub timeout_ms: Option<u64>,
}

/// Top-level ACP config from `acp.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(Default)]
pub struct AcpFileConfig {
    #[serde(default)]
    pub servers: Vec<AcpServerConfig>,
    pub default_server: Option<String>,
    pub default_agent: Option<String>,
}
