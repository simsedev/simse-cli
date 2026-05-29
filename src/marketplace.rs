//! Plugin marketplace — install first-party plugins from the CLI.
//!
//! The default marketplace is the `simsedev/simse-cli` repository's
//! `plugins/` directory. `simse plugins install <name>` downloads a plugin
//! tree into `<data_dir>/plugins/<name>/`; the engine discovers and loads it
//! on the next start. Plugins are intentionally NOT bundled into the `simse`
//! binary (only the plugin engine is) — they are fetched on demand here.

use std::path::Path;

use simse_core::error::SimseError;

/// `owner/repo` of the default marketplace.
const MARKETPLACE_REPO: &str = "simsedev/simse-cli";
/// Git ref the marketplace is read from.
const MARKETPLACE_REF: &str = "main";
/// Directory inside the marketplace repo that holds the plugins.
const MARKETPLACE_DIR: &str = "plugins";

/// A plugin available in the marketplace.
pub struct MarketplacePlugin {
    pub name: String,
}

/// One entry of a GitHub `contents` API directory listing.
#[derive(serde::Deserialize)]
struct ContentEntry {
    name: String,
    path: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    download_url: Option<String>,
}

fn contents_url(path: &str) -> String {
    format!("https://api.github.com/repos/{MARKETPLACE_REPO}/contents/{path}?ref={MARKETPLACE_REF}")
}

/// List one marketplace directory via the GitHub contents API.
async fn list_contents(
    client: &reqwest::Client,
    path: &str,
) -> Result<Vec<ContentEntry>, SimseError> {
    let resp = client
        .get(contents_url(path))
        .header("user-agent", "simse-cli")
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| SimseError::other(format!("marketplace request failed: {e}")))?;
    if !resp.status().is_success() {
        return Err(SimseError::other(format!(
            "marketplace returned {} for '{path}'",
            resp.status()
        )));
    }
    resp.json::<Vec<ContentEntry>>()
        .await
        .map_err(|e| SimseError::other(format!("invalid marketplace response: {e}")))
}

/// List the plugins available in the marketplace.
pub async fn search() -> Result<Vec<MarketplacePlugin>, SimseError> {
    let client = reqwest::Client::new();
    let entries = list_contents(&client, MARKETPLACE_DIR).await?;
    let mut plugins: Vec<MarketplacePlugin> = entries
        .into_iter()
        .filter(|e| e.kind == "dir")
        .map(|e| MarketplacePlugin { name: e.name })
        .collect();
    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(plugins)
}

/// Install a marketplace plugin into `<data_dir>/plugins/<name>/`.
pub async fn install(name: &str, data_dir: &Path) -> Result<(), SimseError> {
    // Reject names that could escape the plugins directory.
    if name.is_empty() || name.contains('/') || name.contains('\\') || name.contains("..") {
        return Err(SimseError::other(format!("invalid plugin name: '{name}'")));
    }

    let client = reqwest::Client::new();
    let dest = data_dir.join("plugins").join(name);

    // Download into a temporary sibling, then swap in atomically so a failed
    // download never leaves a half-written plugin the engine would try to load.
    let staging = data_dir.join("plugins").join(format!(".{name}.partial"));
    let _ = tokio::fs::remove_dir_all(&staging).await;
    let result = download_dir(&client, &format!("{MARKETPLACE_DIR}/{name}"), &staging).await;
    if let Err(e) = result {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(e);
    }

    // Materialize the plugin's npm dependency tree, if it declares one. The
    // marketplace download is source-only, so a plugin that loads an SDK at
    // runtime (claude, openai, gemini, copilot) needs its deps installed here.
    if let Err(e) = install_npm_deps(&staging).await {
        let _ = tokio::fs::remove_dir_all(&staging).await;
        return Err(e);
    }

    let _ = std::fs::remove_dir_all(&dest);
    tokio::fs::rename(&staging, &dest).await.map_err(|e| {
        // sync cleanup here — the .map_err closure can't be async; the
        // outer scope is `pub async fn install(...)` so this only runs on
        // the rare error path right before the function returns.
        let _ = std::fs::remove_dir_all(&staging);
        SimseError::other(format!("cannot install plugin '{name}': {e}"))
    })?;

    // Hook plugins ship shell scripts under `scripts/` that the post-tool
    // hook runner invokes with `sh -c`. The marketplace download is plain
    // file content (no executable bit preserved), so without this pass the
    // hook auto-fire path fails with "Permission denied" the first time it
    // tries to run e.g. format-on-save's `format.sh`. Mark every file under
    // `scripts/` executable on Unix.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let scripts_dir = dest.join("scripts");
        if let Ok(entries) = std::fs::read_dir(&scripts_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && let Ok(meta) = std::fs::metadata(&path)
                {
                    let mut perm = meta.permissions();
                    let mode = perm.mode();
                    // Set owner+group+other execute, keep read/write bits.
                    perm.set_mode(mode | 0o111);
                    let _ = std::fs::set_permissions(&path, perm);
                }
            }
        }
    }
    Ok(())
}

/// Install a plugin's npm dependencies into `<plugin_dir>/node_modules`, if
/// the plugin declares any.
///
/// The `@simse/plugin-sdk` dependency is a workspace alias, not a real npm
/// package (the plugin runtime injects the SDK as ambient globals) — it is
/// stripped before installing so an isolated install resolves only real
/// packages.
async fn install_npm_deps(plugin_dir: &Path) -> Result<(), SimseError> {
    let manifest_path = plugin_dir.join("package.json");
    let raw = match tokio::fs::read_to_string(&manifest_path).await {
        Ok(r) => r,
        // No package.json — a skill or hook plugin; nothing to install.
        Err(_) => return Ok(()),
    };
    let mut manifest: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| SimseError::other(format!("plugin package.json is invalid: {e}")))?;

    // Drop the workspace-only SDK dep; check whether real deps remain.
    let mut has_deps = false;
    if let Some(deps) = manifest
        .get_mut("dependencies")
        .and_then(|d| d.as_object_mut())
    {
        deps.remove("@simse/plugin-sdk");
        has_deps = !deps.is_empty();
    }
    if !has_deps {
        return Ok(());
    }

    // Rewrite the manifest without the workspace dep so an isolated install
    // resolves only real npm packages.
    let rewritten = serde_json::to_string_pretty(&manifest).unwrap_or(raw);
    tokio::fs::write(&manifest_path, rewritten)
        .await
        .map_err(|e| SimseError::other(format!("cannot rewrite package.json: {e}")))?;

    // Prefer bun (fast); fall back to npm.
    let (program, args): (&str, &[&str]) = if pm_available("bun").await {
        ("bun", &["install", "--production"])
    } else if pm_available("npm").await {
        ("npm", &["install", "--omit=dev"])
    } else {
        return Err(SimseError::other(
            "this plugin has npm dependencies but neither 'bun' nor 'npm' is \
             installed — install one and retry",
        ));
    };

    let status = tokio::process::Command::new(program)
        .args(args)
        .current_dir(plugin_dir)
        .stdin(std::process::Stdio::null())
        .status()
        .await
        .map_err(|e| SimseError::other(format!("failed to run {program}: {e}")))?;
    if !status.success() {
        return Err(SimseError::other(format!(
            "{program} install failed for the plugin's dependencies"
        )));
    }

    // The lockfile the install wrote is not needed in the deployed plugin.
    for lock in ["bun.lock", "bun.lockb", "package-lock.json"] {
        let _ = tokio::fs::remove_file(plugin_dir.join(lock)).await;
    }
    Ok(())
}

/// Whether a package manager is on `PATH` (probed via `<pm> --version`).
async fn pm_available(program: &str) -> bool {
    tokio::process::Command::new(program)
        .arg("--version")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Recursively download a marketplace directory into `dest`.
async fn download_dir(client: &reqwest::Client, path: &str, dest: &Path) -> Result<(), SimseError> {
    let entries = list_contents(client, path).await?;
    tokio::fs::create_dir_all(dest)
        .await
        .map_err(|e| SimseError::other(format!("cannot create {}: {e}", dest.display())))?;

    for entry in entries {
        let target = dest.join(&entry.name);
        match entry.kind.as_str() {
            "dir" => {
                // `Box::pin` — recursion in an async fn needs a boxed future.
                Box::pin(download_dir(client, &entry.path, &target)).await?;
            }
            "file" => {
                let url = entry.download_url.ok_or_else(|| {
                    SimseError::other(format!("no download URL for '{}'", entry.path))
                })?;
                let bytes = client
                    .get(&url)
                    .header("user-agent", "simse-cli")
                    .send()
                    .await
                    .map_err(|e| SimseError::other(format!("download failed: {e}")))?
                    .bytes()
                    .await
                    .map_err(|e| SimseError::other(format!("download failed: {e}")))?;
                tokio::fs::write(&target, &bytes).await.map_err(|e| {
                    SimseError::other(format!("cannot write {}: {e}", target.display()))
                })?;
            }
            // Symlinks / submodules in a plugin tree are unexpected — skip.
            _ => {}
        }
    }
    Ok(())
}
