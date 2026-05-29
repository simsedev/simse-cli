//! Config file loading.
//!
//! Builds the application config from:
//!
//! Global config (in data_dir, default `~/.simse` or platform config dir):
//!   `config.json`     -- General user preferences
//!   `acp.json`        -- ACP server entries
//!   `mcp.json`        -- MCP server entries
//!   `embed.json`      -- Embedding provider config
//!   `memory.json`     -- Library, stacks & storage config
//!   `summarize.json`  -- Summarization ACP server config
//!
//! Workspace config (in `.simse/` relative to work_dir):
//!   `settings.json`   -- Workspace-level overrides
//!   `prompts.json`    -- Named prompt templates and chain definitions
//!   `agents/*.md`     -- Custom agent personas (markdown with YAML frontmatter)
//!   `skills/*/SKILL.md` -- Skill definitions (markdown with YAML frontmatter)
//!
//! `SIMSE.md` in work_dir -- workspace system prompt
//!
//! Precedence: workspace settings > global config > defaults.

mod acp;
mod mcp;
mod plugins;

pub use acp::*;
pub use mcp::*;
pub use plugins::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::json_io::read_json_file;

// ---------------------------------------------------------------------------
// Config file types
// ---------------------------------------------------------------------------

/// Embedding provider config from `embed.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedFileConfig {
    pub embedding_model: Option<String>,
    pub dtype: Option<String>,
    pub tei_url: Option<String>,
}

/// Library/stacks config from `memory.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryFileConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_similarity_threshold")]
    pub similarity_threshold: f64,
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    pub auto_save: Option<bool>,
    pub duplicate_threshold: Option<f64>,
    pub duplicate_behavior: Option<String>,
    pub flush_interval_ms: Option<u64>,
    pub compression_level: Option<u32>,
    pub atomic_write: Option<bool>,
    pub auto_summarize_threshold: Option<usize>,
}

fn default_true() -> bool {
    true
}

fn default_similarity_threshold() -> f64 {
    0.7
}

fn default_max_results() -> usize {
    10
}

impl Default for LibraryFileConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            similarity_threshold: 0.7,
            max_results: 10,
            auto_save: None,
            duplicate_threshold: None,
            duplicate_behavior: None,
            flush_interval_ms: None,
            compression_level: None,
            atomic_write: None,
            auto_summarize_threshold: None,
        }
    }
}

/// Summarization ACP server config from `summarize.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummarizeFileConfig {
    pub server: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub agent: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

/// General user preferences from `config.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserConfig {
    pub default_agent: Option<String>,
    pub log_level: Option<String>,
}

/// Workspace-level overrides from `.simse/settings.json`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSettings {
    pub default_agent: Option<String>,
    pub log_level: Option<String>,
    pub system_prompt: Option<String>,
    pub default_server: Option<String>,
    pub conversation_topic: Option<String>,
    pub chain_topic: Option<String>,
}

// ---------------------------------------------------------------------------
// Prompt config types
// ---------------------------------------------------------------------------

/// A single step in a named prompt chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptStepConfig {
    pub name: String,
    pub template: String,
    pub system_prompt: Option<String>,
    pub agent_id: Option<String>,
    pub server_name: Option<String>,
    #[serde(default)]
    pub input_mapping: HashMap<String, String>,
    pub store_to_memory: Option<bool>,
    #[serde(default)]
    pub memory_metadata: HashMap<String, String>,
}

/// A named prompt -- a reusable single or multi-step chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptConfig {
    pub description: Option<String>,
    pub agent_id: Option<String>,
    pub server_name: Option<String>,
    pub system_prompt: Option<String>,
    pub steps: Vec<PromptStepConfig>,
}

/// `.simse/prompts.json` -- named prompt templates.
pub type PromptsFileConfig = HashMap<String, PromptConfig>;

// ---------------------------------------------------------------------------
// Agent config
// ---------------------------------------------------------------------------

/// Custom agent persona loaded from `.simse/agents/*.md`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentConfig {
    pub name: String,
    pub description: Option<String>,
    pub model: Option<String>,
    pub server_name: Option<String>,
    pub agent_id: Option<String>,
    pub system_prompt: String,
}

// ---------------------------------------------------------------------------
// Skill config
// ---------------------------------------------------------------------------

/// Skill loaded from `.simse/skills/{name}/SKILL.md`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillConfig {
    pub name: String,
    pub description: Option<String>,
    pub allowed_tools: Vec<String>,
    pub argument_hint: Option<String>,
    pub model: Option<String>,
    pub server_name: Option<String>,
    pub file_path: PathBuf,
    pub body: String,
}

// ---------------------------------------------------------------------------
// Helper types
// ---------------------------------------------------------------------------

/// An MCP server that was skipped due to missing required env vars.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedServer {
    pub name: String,
    pub missing_env: Vec<String>,
}

/// Options controlling config loading behavior.
#[derive(Debug, Clone, Default)]
pub struct ConfigOptions {
    /// Global data directory (default: platform config dir / simse).
    pub data_dir: Option<PathBuf>,
    /// Working directory to scan for `.simse/`, `SIMSE.md`, etc.
    pub work_dir: Option<PathBuf>,
}

/// Parsed frontmatter result from markdown files.
#[derive(Debug, Clone, PartialEq)]
pub struct Frontmatter {
    pub meta: HashMap<String, String>,
    pub body: String,
}

/// The fully resolved config after loading and merging all sources.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedConfig {
    /// Resolved ACP config.
    pub acp: AcpFileConfig,
    /// Valid MCP servers (after env filtering).
    pub mcp_servers: Vec<McpServerConfig>,
    /// MCP servers skipped due to missing env vars.
    pub skipped_servers: Vec<SkippedServer>,
    /// Embedding provider config.
    pub embed: EmbedFileConfig,
    /// Library/stacks config.
    pub library: LibraryFileConfig,
    /// Summarization config (None if not configured).
    pub summarize: Option<SummarizeFileConfig>,
    /// General user preferences.
    pub user: UserConfig,
    /// Workspace settings.
    pub workspace_settings: WorkspaceSettings,
    /// Named prompts.
    pub prompts: PromptsFileConfig,
    /// Custom agents.
    pub agents: Vec<AgentConfig>,
    /// Skills.
    pub skills: Vec<SkillConfig>,
    /// SIMSE.md workspace prompt (None if missing or empty).
    pub workspace_prompt: Option<String>,
    /// Resolved log level (after precedence).
    pub log_level: String,
    /// Resolved default agent (after precedence).
    pub default_agent: Option<String>,
    /// Resolved default server (after precedence).
    pub default_server: Option<String>,
    /// Resolved embedding model.
    pub embedding_model: String,
    /// Resolved data directory.
    pub data_dir: PathBuf,
    /// Resolved working directory.
    pub work_dir: PathBuf,
    /// Plugin config.
    pub plugins: simse_core::config::PluginsConfig,
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

/// Parse markdown with YAML frontmatter.
///
/// Format:
/// ```text
/// ---
/// key: value
/// another: thing
/// ---
/// Body content here.
/// ```
///
/// Only supports flat `key: value` YAML (no nesting). If there is no closing
/// `---`, the entire content is treated as the body with no metadata.
pub fn parse_frontmatter(content: &str) -> Frontmatter {
    let meta = HashMap::new();

    if !content.starts_with("---") {
        return Frontmatter {
            meta,
            body: content.trim().to_string(),
        };
    }

    // Find the closing `---` delimiter. Search from after the first line.
    let after_open = match content[3..].find('\n') {
        Some(pos) => 3 + pos + 1,
        None => {
            return Frontmatter {
                meta,
                body: content.trim().to_string(),
            };
        }
    };

    let rest = &content[after_open..];
    let end_idx = rest
        .find("\n---\n")
        .or_else(|| rest.find("\n---\r\n"))
        .or_else(|| {
            // Point at the leading `\n`, consistent with the `\n---\n` match
            // above so `end_idx` always marks the start of the closing
            // delimiter regardless of which branch matched.
            if rest.ends_with("\n---") {
                Some(rest.len() - 4)
            } else {
                None
            }
        });

    let end_idx = match end_idx {
        Some(pos) => pos,
        None => {
            return Frontmatter {
                meta,
                body: content.trim().to_string(),
            };
        }
    };

    let frontmatter_text = &rest[..end_idx];
    // Find the start of the body: skip past the "\n---" and the line ending that follows.
    let after_delimiter = after_open + end_idx + 4; // skip past "\n---"
    let body = if after_delimiter >= content.len() {
        String::new()
    } else if content[after_delimiter..].starts_with('\n') {
        content[after_delimiter + 1..].trim().to_string()
    } else if content[after_delimiter..].starts_with("\r\n") {
        content[after_delimiter + 2..].trim().to_string()
    } else {
        content[after_delimiter..].trim().to_string()
    };

    let mut meta = HashMap::new();
    for line in frontmatter_text.lines() {
        if let Some(colon_idx) = line.find(':') {
            let key = line[..colon_idx].trim();
            let value = line[colon_idx + 1..].trim();
            if !key.is_empty() && !value.is_empty() {
                meta.insert(key.to_string(), value.to_string());
            }
        }
    }

    Frontmatter { meta, body }
}

// ---------------------------------------------------------------------------
// Agent loading
// ---------------------------------------------------------------------------

/// Load agent configs from `.simse/agents/*.md` files.
///
/// Each file is parsed for YAML frontmatter. The markdown body becomes the
/// agent's system prompt. Files with empty bodies are skipped. If the
/// directory does not exist, returns an empty vec.
pub fn load_agents(agents_dir: &Path) -> Vec<AgentConfig> {
    let entries = match fs::read_dir(agents_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut agents = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }

        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let Frontmatter { meta, body } = parse_frontmatter(&content);

        if body.is_empty() {
            continue;
        }

        let fallback_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let name = meta.get("name").cloned().unwrap_or(fallback_name);

        agents.push(AgentConfig {
            name,
            description: meta.get("description").cloned(),
            model: meta.get("model").cloned(),
            server_name: meta.get("serverName").cloned(),
            agent_id: meta.get("agentId").cloned(),
            system_prompt: body,
        });
    }

    agents
}

// ---------------------------------------------------------------------------
// Skill loading
// ---------------------------------------------------------------------------

/// Load skill configs from `.simse/skills/*/SKILL.md` files.
///
/// Each subdirectory is checked for a `SKILL.md` file. The file is parsed
/// for YAML frontmatter. The `allowed-tools` field is a comma-separated list.
/// If the directory does not exist, returns an empty vec.
pub fn load_skills(skills_dir: &Path) -> Vec<SkillConfig> {
    let entries = match fs::read_dir(skills_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut skills = Vec::new();

    for entry in entries.flatten() {
        let dir_path = entry.path();

        if !dir_path.is_dir() {
            continue;
        }

        let skill_path = dir_path.join("SKILL.md");

        let content = match fs::read_to_string(&skill_path) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let Frontmatter { meta, body } = parse_frontmatter(&content);

        if body.is_empty() {
            continue;
        }

        let dir_name = dir_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();

        let name = meta.get("name").cloned().unwrap_or(dir_name);

        let allowed_tools = meta
            .get("allowed-tools")
            .map(|s| {
                s.split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let description = meta.get("description").cloned();
        let argument_hint = meta.get("argument-hint").cloned();

        skills.push(SkillConfig {
            name,
            description,
            allowed_tools,
            argument_hint,
            model: meta.get("model").cloned(),
            server_name: meta.get("server-name").cloned(),
            file_path: skill_path,
            body,
        });
    }

    skills
}

// ---------------------------------------------------------------------------
// Default data directory
// ---------------------------------------------------------------------------

/// Returns the default data directory for simse config.
///
/// Uses the platform config directory (e.g. `~/.config/simse` on Linux,
/// `~/Library/Application Support/simse` on macOS).
/// The default config/data directory (`<platform config dir>/simse`).
///
/// Used both when loading config and by commands that need the data
/// directory without a fully loaded config (e.g. the `/plugins` command).
pub fn default_data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("simse")
}

/// Default embedding model when none is configured.
const DEFAULT_EMBEDDING_MODEL: &str = "nomic-ai/nomic-embed-text-v1.5";

/// Default log level when none is configured.
const DEFAULT_LOG_LEVEL: &str = "warn";

// ---------------------------------------------------------------------------
// Environment variable helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Full config loading
// ---------------------------------------------------------------------------

/// Read a text file, trim it, and return `None` if missing or empty.
fn read_text_file(path: &Path) -> Option<String> {
    let content = fs::read_to_string(path).ok()?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Load and merge all config sources into a single [`LoadedConfig`].
///
/// Loading order:
/// 1. Global configs from `data_dir` (`config.json`, `acp.json`, `mcp.json`,
///    `embed.json`, `memory.json`, `summarize.json`, `plugins.json`)
/// 2. Workspace configs from `work_dir/.simse/` (`settings.json`,
///    `prompts.json`, `agents/*.md`, `skills/*/SKILL.md`)
/// 3. `SIMSE.md` from `work_dir`
///
/// Precedence: workspace settings > global config > defaults.
pub fn load_config(options: &ConfigOptions) -> LoadedConfig {
    let data_dir = options.data_dir.clone().unwrap_or_else(default_data_dir);

    let work_dir = options
        .work_dir
        .clone()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let work_simse_dir = work_dir.join(".simse");

    // -- Global configs (from files) ------------------------------------------

    let user_config: UserConfig = read_json_file(&data_dir.join("config.json")).unwrap_or_default();

    let acp: AcpFileConfig = read_json_file(&data_dir.join("acp.json")).unwrap_or_default();

    let mcp_file: McpFileConfig = read_json_file(&data_dir.join("mcp.json")).unwrap_or_default();

    let embed: EmbedFileConfig = read_json_file(&data_dir.join("embed.json")).unwrap_or_default();

    let library: LibraryFileConfig =
        read_json_file(&data_dir.join("memory.json")).unwrap_or_default();

    let summarize: Option<SummarizeFileConfig> = read_json_file(&data_dir.join("summarize.json"));

    let mut plugins: simse_core::config::PluginsConfig =
        read_json_file(&data_dir.join("plugins.json")).unwrap_or_default();

    // -- Bundled plugins (from repo/install) -----------------------------------
    // Discover bundled plugins and merge them as defaults — user-configured
    // plugins override by name.
    {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.to_path_buf()));
        // Exe-relative candidates. Owned PathBufs kept alive in `exe_rel` so the
        // `&Path` refs handed to `resolve_bundled_plugins_dir` stay valid.
        let mut exe_rel: Vec<std::path::PathBuf> = Vec::new();
        if let Some(ref dir) = exe_dir {
            // Installed layout: `<prefix>/bin/simse` -> `<prefix>/share/simse/plugins`.
            // Covers /usr/local/bin, ~/.local/bin, and Windows %LOCALAPPDATA%\simse\bin.
            exe_rel.push(dir.join("../share/simse/plugins"));
            // Dev super-repo layout: `core/target/release/simse` -> `simse-cli/plugins`.
            exe_rel.push(dir.join("../../../simse-cli/plugins"));
            // Legacy dev layout: `core/target/release/simse` -> `core/plugins/plugins`.
            exe_rel.push(dir.join("../../plugins/plugins"));
        }
        let mut fallbacks: Vec<&Path> = exe_rel.iter().map(|p| p.as_path()).collect();
        let usr_share = Path::new("/usr/local/share/simse/plugins");
        fallbacks.push(usr_share);

        if let Some(bundled_dir) = simse_core::config::resolve_bundled_plugins_dir(&fallbacks) {
            let bundled = simse_core::config::discover_bundled_plugins(&bundled_dir);
            let user_names: std::collections::HashSet<String> =
                plugins.plugins.iter().map(|p| p.name.clone()).collect();
            for entry in bundled {
                if !user_names.contains(&entry.name) {
                    plugins.plugins.push(entry);
                }
            }
        }
    }

    // -- Workspace configs ----------------------------------------------------

    let workspace_settings: WorkspaceSettings =
        read_json_file(&work_simse_dir.join("settings.json")).unwrap_or_default();

    let prompts: PromptsFileConfig =
        read_json_file(&work_simse_dir.join("prompts.json")).unwrap_or_default();

    let agents = load_agents(&work_simse_dir.join("agents"));
    let skills = load_skills(&work_simse_dir.join("skills"));

    // -- SIMSE.md -------------------------------------------------------------

    let workspace_prompt = read_text_file(&work_dir.join("SIMSE.md"));

    // -- MCP filtering --------------------------------------------------------

    let (mcp_servers, skipped_servers) = check_mcp_servers(&mcp_file.servers);

    // -- Precedence resolution (workspace > global > default) ------------------

    let log_level = workspace_settings
        .log_level
        .clone()
        .or_else(|| user_config.log_level.clone())
        .unwrap_or_else(|| DEFAULT_LOG_LEVEL.to_string());

    let default_agent = workspace_settings
        .default_agent
        .clone()
        .or_else(|| user_config.default_agent.clone());

    let default_server = workspace_settings
        .default_server
        .clone()
        .or_else(|| acp.default_server.clone());

    let embedding_model = embed
        .embedding_model
        .clone()
        .unwrap_or_else(|| DEFAULT_EMBEDDING_MODEL.to_string());

    LoadedConfig {
        acp,
        mcp_servers,
        skipped_servers,
        embed,
        library,
        summarize,
        user: user_config,
        workspace_settings,
        prompts,
        agents,
        skills,
        workspace_prompt,
        log_level,
        default_agent,
        default_server,
        embedding_model,
        data_dir,
        work_dir,
        plugins,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    // These tests build config structs with every field listed plus a
    // trailing `..Default::default()` as a forward-compatible defensive
    // pattern; clippy's `needless_update` (newer than core's baseline) flags
    // the redundant spread. Allowed at the module scope to preserve the
    // upstream `core/src/cli/config` source verbatim.
    #![allow(clippy::needless_update)]
    use super::*;
    use tempfile::TempDir;

    // -----------------------------------------------------------------------
    // Frontmatter tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_frontmatter_with_meta() {
        let content = "---\nname: test-agent\ndescription: A test\n---\nBody here.";
        let result = parse_frontmatter(content);
        assert_eq!(result.meta.get("name").unwrap(), "test-agent");
        assert_eq!(result.meta.get("description").unwrap(), "A test");
        assert_eq!(result.body, "Body here.");
    }

    #[test]
    fn parse_frontmatter_no_meta() {
        let content = "Just a body with no frontmatter.";
        let result = parse_frontmatter(content);
        assert!(result.meta.is_empty());
        assert_eq!(result.body, "Just a body with no frontmatter.");
    }

    #[test]
    fn parse_frontmatter_empty_body() {
        let content = "---\nname: empty\n---\n";
        let result = parse_frontmatter(content);
        assert_eq!(result.meta.get("name").unwrap(), "empty");
        assert_eq!(result.body, "");
    }

    #[test]
    fn parse_frontmatter_no_closing_delimiter() {
        let content = "---\nname: broken\nThis has no closing delimiter.";
        let result = parse_frontmatter(content);
        // No closing --- so entire content is treated as body, no meta
        assert!(result.meta.is_empty());
        assert_eq!(
            result.body,
            "---\nname: broken\nThis has no closing delimiter."
        );
    }

    #[test]
    fn parse_frontmatter_delimiter_like_content_in_value() {
        // A value that starts with "---" should not be treated as a closing delimiter.
        let content = "---\nname: test\nseparator: ---in-value\n---\nBody content.";
        let result = parse_frontmatter(content);
        assert_eq!(result.meta.get("name").unwrap(), "test");
        assert_eq!(result.meta.get("separator").unwrap(), "---in-value");
        assert_eq!(result.body, "Body content.");
    }

    #[test]
    fn parse_frontmatter_closing_delimiter_at_eof() {
        // Closing delimiter at end of file with no trailing newline.
        let content = "---\nname: eof\n---";
        let result = parse_frontmatter(content);
        assert_eq!(result.meta.get("name").unwrap(), "eof");
        assert_eq!(result.body, "");
    }

    // -----------------------------------------------------------------------
    // Config type serialization tests
    // -----------------------------------------------------------------------

    #[test]
    fn acp_file_config_deserializes() {
        let json = r#"{
			"servers": [{
				"name": "test-server",
				"command": "node",
				"args": ["server.js"],
				"defaultAgent": "my-agent",
				"timeoutMs": 5000
			}],
			"defaultServer": "test-server",
			"defaultAgent": "global-agent"
		}"#;

        let config: AcpFileConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.servers.len(), 1);
        assert_eq!(config.servers[0].name, "test-server");
        assert_eq!(config.servers[0].command, "node");
        assert_eq!(config.servers[0].args, vec!["server.js"]);
        assert_eq!(config.servers[0].default_agent.as_deref(), Some("my-agent"));
        assert_eq!(config.servers[0].timeout_ms, Some(5000));
        assert_eq!(config.default_server.as_deref(), Some("test-server"));
        assert_eq!(config.default_agent.as_deref(), Some("global-agent"));
    }

    #[test]
    fn mcp_server_config_with_required_env() {
        let json = r#"{
			"name": "web-search",
			"transport": "stdio",
			"command": "npx",
			"args": ["-y", "@anthropic/search-mcp"],
			"env": {"API_KEY": "secret"},
			"requiredEnv": ["API_KEY", "OTHER_KEY"]
		}"#;

        let config: McpServerConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.name, "web-search");
        assert_eq!(config.transport, "stdio");
        assert_eq!(config.required_env, vec!["API_KEY", "OTHER_KEY"]);
        assert_eq!(config.env.get("API_KEY").unwrap(), "secret");
    }

    #[test]
    fn library_file_config_defaults() {
        let json = "{}";
        let config: LibraryFileConfig = serde_json::from_str(json).unwrap();
        assert!(config.enabled);
        assert!((config.similarity_threshold - 0.7).abs() < f64::EPSILON);
        assert_eq!(config.max_results, 10);
        assert!(config.auto_save.is_none());
        assert!(config.duplicate_threshold.is_none());
    }

    #[test]
    fn embed_file_config_optional_fields() {
        let json = r#"{"embeddingModel": "nomic-ai/nomic-embed-text-v1.5"}"#;
        let config: EmbedFileConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.embedding_model.as_deref(),
            Some("nomic-ai/nomic-embed-text-v1.5")
        );
        assert!(config.dtype.is_none());
        assert!(config.tei_url.is_none());
    }

    #[test]
    fn workspace_settings_partial() {
        let json = r#"{"logLevel": "debug", "systemPrompt": "Be helpful."}"#;
        let config: WorkspaceSettings = serde_json::from_str(json).unwrap();
        assert_eq!(config.log_level.as_deref(), Some("debug"));
        assert_eq!(config.system_prompt.as_deref(), Some("Be helpful."));
        assert!(config.default_agent.is_none());
        assert!(config.default_server.is_none());
    }

    // -----------------------------------------------------------------------
    // Agent loading tests
    // -----------------------------------------------------------------------

    #[test]
    fn load_agents_from_dir() {
        let dir = TempDir::new().unwrap();
        let agents_dir = dir.path().join("agents");
        fs::create_dir_all(&agents_dir).unwrap();

        fs::write(
            agents_dir.join("coder.md"),
            "---\nname: coder\ndescription: A coding agent\n---\nYou are a coding assistant.",
        )
        .unwrap();

        let agents = load_agents(&agents_dir);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "coder");
        assert_eq!(agents[0].description.as_deref(), Some("A coding agent"));
        assert_eq!(agents[0].system_prompt, "You are a coding assistant.");
    }

    #[test]
    fn load_agents_fallback_name_from_filename() {
        let dir = TempDir::new().unwrap();
        let agents_dir = dir.path().join("agents");
        fs::create_dir_all(&agents_dir).unwrap();

        // No name in frontmatter -- should fall back to filename
        fs::write(
            agents_dir.join("writer.md"),
            "---\ndescription: A writing agent\n---\nYou write things.",
        )
        .unwrap();

        let agents = load_agents(&agents_dir);
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "writer");
    }

    #[test]
    fn load_agents_skips_empty_body() {
        let dir = TempDir::new().unwrap();
        let agents_dir = dir.path().join("agents");
        fs::create_dir_all(&agents_dir).unwrap();

        fs::write(agents_dir.join("empty.md"), "---\nname: empty-agent\n---\n").unwrap();

        let agents = load_agents(&agents_dir);
        assert!(agents.is_empty());
    }

    #[test]
    fn load_agents_missing_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let agents = load_agents(&dir.path().join("nonexistent"));
        assert!(agents.is_empty());
    }

    // -----------------------------------------------------------------------
    // Skill loading tests
    // -----------------------------------------------------------------------

    #[test]
    fn load_skills_from_dir() {
        let dir = TempDir::new().unwrap();
        let skills_dir = dir.path().join("skills");
        let search_dir = skills_dir.join("search");
        fs::create_dir_all(&search_dir).unwrap();

        fs::write(
			search_dir.join("SKILL.md"),
			"---\nname: search\ndescription: Search the web\nallowed-tools: web_search, fetch\nargument-hint: query\n---\nSearch for information.",
		)
		.unwrap();

        let skills = load_skills(&skills_dir);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "search");
        assert_eq!(skills[0].description.as_deref(), Some("Search the web"));
        assert_eq!(skills[0].allowed_tools, vec!["web_search", "fetch"]);
        assert_eq!(skills[0].argument_hint.as_deref(), Some("query"));
        assert_eq!(skills[0].body, "Search for information.");
    }

    #[test]
    fn load_skills_missing_dir_returns_empty() {
        let dir = TempDir::new().unwrap();
        let skills = load_skills(&dir.path().join("nonexistent"));
        assert!(skills.is_empty());
    }

    // -----------------------------------------------------------------------
    // MCP server filtering tests
    // -----------------------------------------------------------------------

    #[test]
    fn check_mcp_servers_filters_missing_env() {
        let servers = vec![
            McpServerConfig {
                name: "no-reqs".into(),
                transport: "stdio".into(),
                command: Some("cmd1".into()),
                url: None,
                args: vec![],
                env: HashMap::new(),
                required_env: vec![],
            },
            McpServerConfig {
                name: "has-env".into(),
                transport: "stdio".into(),
                command: Some("cmd2".into()),
                url: None,
                args: vec![],
                env: HashMap::from([("API_KEY".into(), "secret".into())]),
                required_env: vec!["API_KEY".into()],
            },
            McpServerConfig {
                name: "missing-env".into(),
                transport: "stdio".into(),
                command: Some("cmd3".into()),
                url: None,
                args: vec![],
                env: HashMap::new(),
                required_env: vec!["NONEXISTENT_VAR_12345".into()],
            },
        ];

        let (valid, skipped) = check_mcp_servers(&servers);
        assert_eq!(valid.len(), 2);
        assert_eq!(valid[0].name, "no-reqs");
        assert_eq!(valid[1].name, "has-env");
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].name, "missing-env");
        assert_eq!(skipped[0].missing_env, vec!["NONEXISTENT_VAR_12345"]);
    }

    #[test]
    fn check_mcp_servers_filters_unresolved_placeholders() {
        let servers = vec![McpServerConfig {
            name: "placeholder-env".into(),
            transport: "stdio".into(),
            command: Some("cmd".into()),
            url: None,
            args: vec![],
            env: HashMap::from([("TOKEN".into(), "${SOME_TOKEN}".into())]),
            required_env: vec!["TOKEN".into()],
        }];

        let (valid, skipped) = check_mcp_servers(&servers);
        assert!(valid.is_empty());
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0].name, "placeholder-env");
        assert_eq!(skipped[0].missing_env, vec!["TOKEN"]);
    }

    // -----------------------------------------------------------------------
    // Full config loading tests
    // -----------------------------------------------------------------------

    #[test]
    fn load_config_with_minimal_files() {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        let work_dir = dir.path().join("work");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&work_dir).unwrap();

        let options = ConfigOptions {
            data_dir: Some(data_dir),
            work_dir: Some(work_dir),
            ..Default::default()
        };

        let config = load_config(&options);
        assert_eq!(config.log_level, "warn");
        assert!(config.default_agent.is_none());
        assert!(config.acp.servers.is_empty());
        assert!(config.mcp_servers.is_empty());
        assert!(config.agents.is_empty());
        assert!(config.skills.is_empty());
        assert!(config.prompts.is_empty());
        assert!(config.workspace_prompt.is_none());
        assert_eq!(config.embedding_model, DEFAULT_EMBEDDING_MODEL);
        assert!(config.library.enabled);
    }

    #[test]
    fn load_config_precedence_cli_over_workspace_over_global() {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        let work_dir = dir.path().join("work");
        let simse_dir = work_dir.join(".simse");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&simse_dir).unwrap();

        // Global: log_level = "info", default_agent = "global-agent"
        let user_config = UserConfig {
            log_level: Some("info".into()),
            default_agent: Some("global-agent".into()),
            ..Default::default()
        };
        crate::json_io::write_json_file(&data_dir.join("config.json"), &user_config).unwrap();

        // Workspace: log_level = "debug"
        let ws = WorkspaceSettings {
            log_level: Some("debug".into()),
            ..Default::default()
        };
        crate::json_io::write_json_file(&simse_dir.join("settings.json"), &ws).unwrap();

        // Without CLI overrides: workspace wins for log_level, global for agent
        let options = ConfigOptions {
            data_dir: Some(data_dir.clone()),
            work_dir: Some(work_dir.clone()),
            ..Default::default()
        };
        let config = load_config(&options);
        assert_eq!(config.log_level, "debug");
        assert_eq!(config.default_agent.as_deref(), Some("global-agent"));

        // Config options only have data_dir and work_dir now
        let options = ConfigOptions {
            data_dir: Some(data_dir),
            work_dir: Some(work_dir),
        };
        let config = load_config(&options);
        // log_level comes from workspace settings or global config
        assert!(!config.log_level.is_empty());
    }

    #[test]
    fn load_config_reads_workspace_prompt() {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        let work_dir = dir.path().join("work");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&work_dir).unwrap();

        fs::write(work_dir.join("SIMSE.md"), "You are a helpful assistant.").unwrap();

        let options = ConfigOptions {
            data_dir: Some(data_dir),
            work_dir: Some(work_dir),
            ..Default::default()
        };
        let config = load_config(&options);
        assert_eq!(
            config.workspace_prompt.as_deref(),
            Some("You are a helpful assistant.")
        );
    }

    #[test]
    fn load_config_empty_workspace_prompt_is_none() {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        let work_dir = dir.path().join("work");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&work_dir).unwrap();

        fs::write(work_dir.join("SIMSE.md"), "   \n  \n  ").unwrap();

        let options = ConfigOptions {
            data_dir: Some(data_dir),
            work_dir: Some(work_dir),
            ..Default::default()
        };
        let config = load_config(&options);
        assert!(config.workspace_prompt.is_none());
    }

    #[test]
    fn load_config_loads_prompts() {
        let dir = TempDir::new().unwrap();
        let data_dir = dir.path().join("data");
        let work_dir = dir.path().join("work");
        let simse_dir = work_dir.join(".simse");
        fs::create_dir_all(&data_dir).unwrap();
        fs::create_dir_all(&simse_dir).unwrap();

        let mut prompts = PromptsFileConfig::new();
        prompts.insert(
            "summarize".into(),
            PromptConfig {
                description: Some("Summarize text".into()),
                agent_id: None,
                server_name: None,
                system_prompt: None,
                steps: vec![PromptStepConfig {
                    name: "summarize".into(),
                    template: "Summarize: {input}".into(),
                    system_prompt: None,
                    agent_id: None,
                    server_name: None,
                    input_mapping: HashMap::new(),
                    store_to_memory: None,
                    memory_metadata: HashMap::new(),
                }],
            },
        );

        crate::json_io::write_json_file(&simse_dir.join("prompts.json"), &prompts).unwrap();

        let options = ConfigOptions {
            data_dir: Some(data_dir),
            work_dir: Some(work_dir),
            ..Default::default()
        };
        let config = load_config(&options);
        assert!(config.prompts.contains_key("summarize"));
        let prompt = &config.prompts["summarize"];
        assert_eq!(prompt.description.as_deref(), Some("Summarize text"));
        assert_eq!(prompt.steps.len(), 1);
        assert_eq!(prompt.steps[0].template, "Summarize: {input}");
    }
}
