//! Plugin loader for the CLI.
//!
//! Discovers plugins from the data dir and registers their capabilities.

use std::path::Path;
use std::sync::Arc;

use simse_core::error::SimseError;
use simse_core::plugin_tools::{
    PluginCapabilities, PluginHandle, PluginHookDef, PluginSkillDef, PluginToolDef,
    register_plugin_capabilities,
};
use simse_core::tools::registry::ToolRegistry;

/// Load all plugins and register their capabilities as tools.
pub async fn load_all_plugins(registry: &mut ToolRegistry, plugins_dir: &Path) {
    load_file_plugins(registry, plugins_dir);

    #[cfg(feature = "plugins")]
    load_runtime_plugins(registry, plugins_dir).await;
}

// ---------------------------------------------------------------------------
// File-based plugins (skills, hooks — no Deno needed)
// ---------------------------------------------------------------------------

fn load_file_plugins(registry: &mut ToolRegistry, plugins_dir: &Path) {
    let entries = match std::fs::read_dir(plugins_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = match read_manifest(&path) {
            Some(m) => m,
            None => continue,
        };
        match manifest.kind.as_str() {
            "skill" => {
                if let Some(caps) = load_skill(&path, &manifest.name) {
                    let handle = Arc::new(NoopHandle(manifest.name.clone()));
                    register_plugin_capabilities(registry, &caps, handle);
                }
            }
            "hook" => {
                if let Some(caps) = load_hook(&path, &manifest.name) {
                    let handle = Arc::new(NoopHandle(manifest.name.clone()));
                    register_plugin_capabilities(registry, &caps, handle);
                }
            }
            _ => {}
        }
    }
}

fn load_skill(dir: &Path, name: &str) -> Option<PluginCapabilities> {
    let content = std::fs::read_to_string(dir.join("SKILL.md")).ok()?;
    let (description, template) = if let Some(after_open) = content.strip_prefix("---") {
        let close = after_open.find("---")?;
        let fm = &after_open[..close];
        let body = after_open[close + 3..].trim().to_string();
        let desc = fm
            .lines()
            .find_map(|l| l.strip_prefix("description:").map(|v| v.trim().to_string()))
            .unwrap_or_default();
        (desc, body)
    } else {
        (String::new(), content)
    };

    Some(PluginCapabilities {
        name: name.into(),
        models: vec![],
        tools: vec![],
        resources: vec![],
        skills: vec![PluginSkillDef {
            name: name.into(),
            description,
            prompt_template: template,
        }],
        hooks: vec![],
    })
}

fn load_hook(dir: &Path, name: &str) -> Option<PluginCapabilities> {
    let content = std::fs::read_to_string(dir.join("hooks.toml")).ok()?;
    let mut hooks = Vec::new();
    let mut matcher = String::new();
    let mut command = String::new();

    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("matcher")
            && let Some(v) = t.split('=').nth(1)
        {
            matcher = v.trim().trim_matches('"').to_string();
        }
        if t.starts_with("command")
            && let Some(v) = t.split('=').nth(1)
        {
            command = v
                .trim()
                .trim_matches('"')
                .replace("${PLUGIN_ROOT}", &dir.display().to_string());
        }
        if !matcher.is_empty() && !command.is_empty() {
            hooks.push(PluginHookDef {
                name: name.into(),
                description: format!("Hook on: {matcher}"),
                matcher: matcher.clone(),
                command: command.clone(),
            });
            matcher.clear();
            command.clear();
        }
    }

    if hooks.is_empty() {
        return None;
    }
    Some(PluginCapabilities {
        name: name.into(),
        models: vec![],
        tools: vec![],
        resources: vec![],
        skills: vec![],
        hooks,
    })
}

// ---------------------------------------------------------------------------
// Runtime plugins (ACP, MCP — Deno runtime)
// ---------------------------------------------------------------------------

#[cfg(feature = "plugins")]
async fn load_runtime_plugins(registry: &mut ToolRegistry, plugins_dir: &Path) {
    use simse_core::plugin_manager::manager::PluginManager;
    use simse_core::plugin_manager::types::{PluginConfig, PluginKind};

    let entries = match std::fs::read_dir(plugins_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut manager = PluginManager::new();

    // Collect ACP/MCP plugin paths first.
    let mut to_load: Vec<(String, PluginKind, std::path::PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest = match read_manifest(&path) {
            Some(m) => m,
            None => continue,
        };
        let kind = match manifest.kind.as_str() {
            "acp" => PluginKind::Acp,
            "mcp" => PluginKind::Mcp,
            _ => continue,
        };
        let entry_point = manifest.main.as_deref().unwrap_or("src/index.ts");
        let entry_path = path.join(entry_point);
        if !entry_path.exists() {
            continue;
        }
        to_load.push((manifest.name, kind, entry_path));
    }

    // Load each plugin.
    for (name, kind, entry_path) in &to_load {
        let config = PluginConfig {
            name: name.clone(),
            kind: kind.clone(),
            path: entry_path.display().to_string(),
            config: serde_json::json!({}),
        };
        if let Err(e) = manager.load(config).await {
            tracing::debug!("plugin {name} load failed: {e}");
            continue;
        }
    }

    // Initialize and register each plugin.
    for (name, kind, _) in &to_load {
        let handle = match manager.get(name) {
            Ok(h) => h,
            Err(_) => continue,
        };

        match kind {
            PluginKind::Acp => {
                match handle.initialize(serde_json::json!({})).await {
                    Ok(info) => {
                        tracing::info!("loaded acp plugin: {name}");
                        // We'll register the delegation tool after removing the handle.
                        // For now just record the models.
                        let caps = PluginCapabilities {
                            name: name.clone(),
                            models: info.models,
                            tools: vec![],
                            resources: vec![],
                            skills: vec![],
                            hooks: vec![],
                        };
                        // Remove handle from manager to get ownership for Arc.
                        if let Some(owned) = manager.remove(name) {
                            let bridge = Arc::new(RuntimeBridge {
                                name: name.clone(),
                                handle: Arc::new(owned),
                            });
                            register_plugin_capabilities(registry, &caps, bridge);
                        }
                    }
                    Err(e) => tracing::debug!("plugin {name} init skipped: {e}"),
                }
            }
            PluginKind::Mcp => match handle.initialize_mcp(serde_json::json!({})).await {
                Ok(info) => {
                    tracing::info!("loaded mcp plugin: {name}");
                    let caps = PluginCapabilities {
                        name: name.clone(),
                        models: vec![],
                        tools: info
                            .tools
                            .iter()
                            .map(|t| PluginToolDef {
                                name: t.name.clone(),
                                description: t.description.clone(),
                                input_schema: t.input_schema.clone(),
                            })
                            .collect(),
                        resources: vec![],
                        skills: vec![],
                        hooks: vec![],
                    };
                    if let Some(owned) = manager.remove(name) {
                        let bridge = Arc::new(RuntimeBridge {
                            name: name.clone(),
                            handle: Arc::new(owned),
                        });
                        register_plugin_capabilities(registry, &caps, bridge);
                    }
                }
                Err(e) => tracing::debug!("plugin {name} init skipped: {e}"),
            },
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct Manifest {
    name: String,
    kind: String,
    #[serde(default)]
    main: Option<String>,
}

fn read_manifest(dir: &Path) -> Option<Manifest> {
    let content = std::fs::read_to_string(dir.join("plugin.json")).ok()?;
    serde_json::from_str(&content).ok()
}

// ---------------------------------------------------------------------------
// NoopHandle for file-based plugins
// ---------------------------------------------------------------------------

struct NoopHandle(String);

#[async_trait::async_trait]
impl PluginHandle for NoopHandle {
    async fn call_tool(&self, _: &str, _: serde_json::Value) -> Result<String, SimseError> {
        Err(SimseError::other(format!("{}: file-based plugin", self.0)))
    }
    async fn read_resource(&self, _: &str) -> Result<String, SimseError> {
        Err(SimseError::other(format!("{}: file-based plugin", self.0)))
    }
    async fn prompt(
        &self,
        _: &str,
        _: Option<&str>,
        _: Option<&str>,
        _: Option<f64>,
    ) -> Result<String, SimseError> {
        Err(SimseError::other(format!("{}: file-based plugin", self.0)))
    }
    fn plugin_name(&self) -> &str {
        &self.0
    }
    fn supported_models(&self) -> &[String] {
        &[]
    }
}

// ---------------------------------------------------------------------------
// RuntimeBridge — wraps a PluginHandle for use as a PluginHandle trait object
// ---------------------------------------------------------------------------

#[cfg(feature = "plugins")]
struct RuntimeBridge {
    name: String,
    handle: Arc<simse_core::plugin_manager::manager::PluginHandle>,
}

#[cfg(feature = "plugins")]
#[async_trait::async_trait]
impl PluginHandle for RuntimeBridge {
    async fn call_tool(&self, name: &str, args: serde_json::Value) -> Result<String, SimseError> {
        use simse_core::plugin_manager::types::McpContentItem;
        let result = self
            .handle
            .call_tool(name.to_string(), args)
            .await
            .map_err(|e| SimseError::other(e.to_string()))?;
        let text: String = result
            .content
            .iter()
            .filter_map(|c| match c {
                McpContentItem::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok(text)
    }

    async fn read_resource(&self, uri: &str) -> Result<String, SimseError> {
        let result = self
            .handle
            .read_resource(uri.to_string())
            .await
            .map_err(|e| SimseError::other(e.to_string()))?;
        let text: String = result
            .contents
            .iter()
            .filter_map(|c| c.text.as_deref())
            .collect::<Vec<_>>()
            .join("\n");
        Ok(text)
    }

    async fn prompt(
        &self,
        prompt: &str,
        model: Option<&str>,
        system: Option<&str>,
        _temp: Option<f64>,
    ) -> Result<String, SimseError> {
        use simse_core::plugin_manager::types::{PluginMessage, SessionOptions};

        let session_id = uuid::Uuid::new_v4().to_string();
        self.handle
            .new_session(
                session_id.clone(),
                SessionOptions {
                    model: model.map(String::from),
                    system_prompt: system.map(String::from),
                },
            )
            .await
            .map_err(|e| SimseError::other(e.to_string()))?;

        let messages = vec![PluginMessage {
            role: "user".into(),
            content: prompt.into(),
        }];

        // prompt() returns PromptResult (stop_reason, usage) — the response
        // text arrives via the event channel. Since we don't have the event_rx
        // here (it was taken during init), we return the stop reason.
        // Full streaming delegation requires the event channel bridge.
        use simse_core::plugin_manager::types::PromptOptions;
        let result = self
            .handle
            .prompt(session_id, messages, PromptOptions::default())
            .await
            .map_err(|e| SimseError::other(e.to_string()))?;

        Ok(format!("(delegation completed: {})", result.stop_reason))
    }

    fn plugin_name(&self) -> &str {
        &self.name
    }
    fn supported_models(&self) -> &[String] {
        &[]
    }
}
