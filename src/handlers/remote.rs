//! Remote-related JSON-RPC handlers (`remote/*` methods).

use std::path::Path;

use serde_json::Value;
use tokio::sync::Mutex;

use crate::event_loop::CliRuntime;

use super::{INVALID_PARAMS, SessionState, make_error, make_response};

// ---------------------------------------------------------------------------
// Remote handlers
// ---------------------------------------------------------------------------

pub(crate) async fn handle_shell(id: Value, params: &Value, rt: &Mutex<CliRuntime>) -> String {
    let command = match params.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return make_error(id, INVALID_PARAMS, "command is required"),
    };

    let work_dir = {
        let rt = rt.lock().await;
        rt.config().work_dir.clone()
    };

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&command)
            .current_dir(&work_dir)
            .output(),
    )
    .await;

    let (output, exit_code) = match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = if stderr.is_empty() {
                stdout.to_string()
            } else if stdout.is_empty() {
                stderr.to_string()
            } else {
                format!("{stdout}{stderr}")
            };
            (combined, output.status.code().unwrap_or(1))
        }
        Ok(Err(e)) => (format!("Failed to execute: {e}"), 1),
        Err(_) => ("Command timed out (30s limit)".to_string(), 124),
    };

    make_response(
        id,
        serde_json::json!({ "output": output, "exitCode": exit_code }),
    )
}

pub(crate) async fn handle_files(id: Value, params: &Value, rt: &Mutex<CliRuntime>) -> String {
    let work_dir = {
        let rt = rt.lock().await;
        rt.config().work_dir.clone()
    };

    let rel_path = params.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    let target = work_dir.join(rel_path);

    // Canonicalize both paths to resolve symlinks before comparison.
    let canon_work_dir = match work_dir.canonicalize() {
        Ok(p) => p,
        Err(_) => return make_error(id, INVALID_PARAMS, "Invalid working directory"),
    };
    let target = match target.canonicalize() {
        Ok(p) if p.starts_with(&canon_work_dir) => p,
        Ok(_) => return make_error(id, INVALID_PARAMS, "Path outside working directory"),
        Err(_) => return make_response(id, serde_json::json!({ "files": [] })),
    };

    let files = list_dir_entries(&target, 1).await;
    make_response(id, serde_json::json!({ "files": files }))
}

/// Recursively list directory entries up to `max_depth` levels.
async fn list_dir_entries(path: &Path, max_depth: u32) -> Vec<Value> {
    let mut entries = Vec::new();
    let mut read_dir = match tokio::fs::read_dir(path).await {
        Ok(rd) => rd,
        Err(_) => return entries,
    };

    let mut items: Vec<(String, std::fs::FileType, std::fs::Metadata)> = Vec::new();
    while let Ok(Some(entry)) = read_dir.next_entry().await {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if let (Ok(ft), Ok(meta)) = (entry.file_type().await, entry.metadata().await) {
            items.push((name, ft, meta));
        }
    }

    // Sort: directories first, then alphabetically.
    items.sort_by(|a, b| {
        let a_dir = a.1.is_dir();
        let b_dir = b.1.is_dir();
        b_dir.cmp(&a_dir).then(a.0.cmp(&b.0))
    });

    for (name, ft, meta) in items {
        let modified = meta
            .modified()
            .ok()
            .map(|t| {
                let dt: chrono::DateTime<chrono::Utc> = t.into();
                dt.to_rfc3339()
            })
            .unwrap_or_default();

        if ft.is_dir() {
            let children = if max_depth > 0 {
                Box::pin(list_dir_entries(&path.join(&name), max_depth - 1)).await
            } else {
                Vec::new()
            };
            entries.push(serde_json::json!({
                "name": name,
                "type": "directory",
                "modified": modified,
                "children": children,
            }));
        } else {
            entries.push(serde_json::json!({
                "name": name,
                "type": "file",
                "size": meta.len(),
                "modified": modified,
            }));
        }
    }

    entries
}

pub(crate) async fn handle_memories(id: Value, sessions: &SessionState) -> String {
    // Return conversation messages from the active session as memory entries.
    let active_id = sessions.active_session_id.lock().await.clone();
    let memories: Vec<Value> = if let Some(ref sid) = active_id {
        let sess = sessions.sessions.read().await;
        if let Some(session) = sess.get(sid) {
            session
                .messages
                .iter()
                .enumerate()
                .map(|(i, m)| {
                    let title: String = m.content.chars().take(50).collect();
                    serde_json::json!({
                        "id": format!("{sid}-{i}"),
                        "title": title,
                        "content": m.content,
                        "topics": [],
                        "createdAt": m.ts,
                        "source": "conversation",
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    make_response(id, serde_json::json!({ "memories": memories }))
}

pub(crate) async fn handle_agents(id: Value, rt: &Mutex<CliRuntime>) -> String {
    let rt = rt.lock().await;

    let agents: Vec<Value> = if let Some(sid) = rt.session_id() {
        let status = if rt.is_loop_active() {
            "running"
        } else {
            "idle"
        };
        vec![serde_json::json!({
            "id": sid,
            "name": rt.server_name().unwrap_or_else(|| "simse".to_string()),
            "description": "ACP session",
            "status": status,
            "taskCount": 0,
            "completedTasks": 0,
            "startedAt": chrono::Utc::now().to_rfc3339(),
            "model": rt.model_name().unwrap_or_else(|| "default".to_string()),
            "tokensUsed": rt.total_tokens(),
        })]
    } else {
        Vec::new()
    };

    make_response(id, serde_json::json!({ "agents": agents }))
}

pub(crate) async fn handle_plugins(id: Value, rt: &Mutex<CliRuntime>) -> String {
    let plugins = {
        let rt = rt.lock().await;
        let defs = rt.tool_registry().get_tool_definitions();
        defs.iter()
            .map(|d| {
                let category = match d.category {
                    simse_core::tools::ToolCategory::Read => "tools",
                    simse_core::tools::ToolCategory::Edit => "tools",
                    simse_core::tools::ToolCategory::Search => "tools",
                    simse_core::tools::ToolCategory::Execute => "tools",
                    simse_core::tools::ToolCategory::Library => "storage",
                    simse_core::tools::ToolCategory::Task => "tools",
                    simse_core::tools::ToolCategory::Subagent => "integration",
                    simse_core::tools::ToolCategory::Network => "network",
                    simse_core::tools::ToolCategory::Provider => "provider",
                    simse_core::tools::ToolCategory::Other => "tools",
                };
                serde_json::json!({
                    "id": d.name,
                    "name": d.name,
                    "description": d.description,
                    "version": "1.0.0",
                    "author": "simse",
                    "enabled": true,
                    "category": category,
                })
            })
            .collect::<Vec<_>>()
    };

    make_response(id, serde_json::json!({ "plugins": plugins }))
}

pub(crate) async fn handle_network(id: Value, rt: &Mutex<CliRuntime>) -> String {
    let rt = rt.lock().await;

    // Return ACP server connection entries.
    let requests: Vec<Value> = if let Some(server_name) = rt.server_name() {
        vec![serde_json::json!({
            "id": server_name,
            "method": "connect",
            "url": server_name,
            "status": 200,
            "duration": 0,
            "size": 0,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "type": "acp",
        })]
    } else {
        Vec::new()
    };

    make_response(id, serde_json::json!({ "requests": requests }))
}
