use serde::Deserialize;
use std::fs;
use std::path::Path;

use crate::json_io::read_json_file;

/// Scan `~/.simse/plugins/*/plugin.json` and return plugin info.
///
/// Each plugin directory must contain a `plugin.json` with fields:
/// `name`, `kind`, `version`, `description`.
pub fn discover_plugins(data_dir: &Path) -> Vec<crate::commands::PluginInfo> {
    let plugins_dir = data_dir.join("plugins");
    let mut plugins = Vec::new();

    let entries = match fs::read_dir(&plugins_dir) {
        Ok(entries) => entries,
        Err(_) => return plugins,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let manifest_path = path.join("plugin.json");
        if !manifest_path.exists() {
            continue;
        }

        #[derive(Deserialize)]
        struct PluginManifest {
            name: String,
            kind: String,
            #[serde(default)]
            version: String,
            #[serde(default)]
            description: String,
        }

        if let Some(manifest) = read_json_file::<PluginManifest>(&manifest_path) {
            plugins.push(crate::commands::PluginInfo {
                name: manifest.name,
                kind: manifest.kind,
                version: manifest.version,
                description: manifest.description,
            });
        }
    }

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    plugins
}
