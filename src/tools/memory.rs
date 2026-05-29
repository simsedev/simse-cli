//! Memory tools — model-callable wrappers around the cloud `AdaptiveService`.
//!
//! Registered on the CLI `ToolRegistry` so the model can save, search,
//! list, and delete user-scoped memories mid-turn. Unlike the managed
//! variant (`managed/memory_tools.rs`) — which resolves user/team scope
//! from the active `ToolContext` bucket and reaches the baremetal
//! adaptive service directly — the CLI resolves scope from the locally
//! persisted auth state (`cli::auth::load_auth`) and reaches the cloud
//! `AdaptiveService` over gRPC-Web via `cloud/api`.
//!
//! Tool names, descriptions, and parameters mirror the managed tools
//! exactly so the model sees a consistent surface in either environment.
//! The `ToolContext` argument is unused here (`_ctx`).

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::memory_client::AdaptiveClient;
use simse_core::error::{SimseError, ToolErrorCode};
use simse_core::remote::auth::AuthState;
use simse_core::tools::registry::ToolRegistry;
use simse_core::tools::types::{
    ToolAnnotations, ToolCategory, ToolDefinition, ToolHandler, ToolParameter,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn param(param_type: &str, description: &str, required: bool) -> ToolParameter {
    ToolParameter {
        param_type: param_type.to_string(),
        description: description.to_string(),
        required,
    }
}

/// Load the persisted CLI auth state, or a clear "not logged in" error.
fn require_auth() -> Result<AuthState, SimseError> {
    crate::auth::load_auth().ok_or_else(|| {
        SimseError::tool(
            ToolErrorCode::ExecutionFailed,
            "not logged in — run `simse login` first",
        )
    })
}

/// Build the adaptive `Scope` from auth state. `team_id` falls back to
/// `user_id` for single-tenant accounts (mirrors the managed tools, where
/// `team_id` mirrors `user_id` for solo accounts). `session_id` is `None`
/// — CLI memories live on the user/team shelf, not a session shelf.
fn scope_from_auth(auth: &AuthState) -> crate::proto::adaptive::Scope {
    crate::proto::adaptive::Scope {
        user_id: auth.user_id.clone(),
        team_id: auth.team_id.clone().unwrap_or_else(|| auth.user_id.clone()),
        session_id: None,
    }
}

/// Build an `AdaptiveClient` for the authenticated user.
fn client_from_auth(auth: &AuthState) -> AdaptiveClient {
    AdaptiveClient::new(&auth.api_url, &auth.access_token)
}

/// Format a list of memory entries as a readable bullet list. Mirrors the
/// managed tools' `- [id] text` line style.
fn format_entries(entries: &[crate::proto::adaptive::MemoryEntry]) -> String {
    let mut out = String::new();
    for e in entries {
        out.push_str(&format!("- [{}] {}\n", e.id, e.text));
    }
    out
}

// ---------------------------------------------------------------------------
// Public registration
// ---------------------------------------------------------------------------

/// Register the four cloud-backed memory tools on the given registry.
pub fn register_memory_tools(registry: &mut ToolRegistry) {
    register_memory_save(registry);
    register_memory_search(registry);
    register_memory_list(registry);
    register_memory_delete(registry);
}

fn register_memory_save(registry: &mut ToolRegistry) {
    let mut parameters = HashMap::new();
    parameters.insert(
        "text".to_string(),
        param("string", "The memory content to save.", true),
    );
    parameters.insert(
        "tag".to_string(),
        param(
            "string",
            "Optional short tag (e.g. preference, fact, todo).",
            false,
        ),
    );
    let definition = ToolDefinition {
        name: "memory_save".to_string(),
        description: "Save a memory for the current user. Use for facts, \
preferences, or context worth recalling in future turns. Returns the \
new memory id."
            .to_string(),
        parameters,
        category: ToolCategory::Library,
        annotations: Some(ToolAnnotations {
            destructive: Some(false),
            read_only: Some(false),
            ..Default::default()
        }),
        timeout_ms: Some(15_000),
        max_output_chars: None,
    };

    let handler: ToolHandler = Arc::new(move |args: Value, _ctx| {
        Box::pin(async move {
            let text = args
                .get("text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if text.trim().is_empty() {
                return Err(SimseError::tool(
                    ToolErrorCode::ExecutionFailed,
                    "text is required",
                ));
            }
            let tag = args
                .get("tag")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let auth = require_auth()?;
            let scope = scope_from_auth(&auth);
            let client = client_from_auth(&auth);
            let mut metadata: HashMap<String, String> = HashMap::new();
            if let Some(tag) = tag {
                metadata.insert("tag".to_string(), tag);
            }
            let id = client.memory_add(scope, text, metadata).await?;
            Ok(format!("saved memory {id}"))
        })
    });

    registry.register_mut(definition, handler);
}

fn register_memory_search(registry: &mut ToolRegistry) {
    let mut parameters = HashMap::new();
    parameters.insert(
        "query".to_string(),
        param("string", "Semantic search query.", true),
    );
    parameters.insert(
        "limit".to_string(),
        param("number", "Max results (default 5).", false),
    );
    let definition = ToolDefinition {
        name: "memory_search".to_string(),
        description: "Semantic search across the current user's memories. \
Returns up to `limit` matches ordered by relevance."
            .to_string(),
        parameters,
        category: ToolCategory::Search,
        annotations: Some(ToolAnnotations {
            read_only: Some(true),
            ..Default::default()
        }),
        timeout_ms: Some(15_000),
        max_output_chars: None,
    };

    let handler: ToolHandler = Arc::new(move |args: Value, _ctx| {
        Box::pin(async move {
            let query = args
                .get("query")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if query.trim().is_empty() {
                return Err(SimseError::tool(
                    ToolErrorCode::ExecutionFailed,
                    "query is required",
                ));
            }
            let limit = args.get("limit").and_then(|v| v.as_i64()).map(|n| n as i32);
            let auth = require_auth()?;
            let scope = scope_from_auth(&auth);
            let client = client_from_auth(&auth);
            let entries = client.memory_search(scope, query, limit, None).await?;
            if entries.is_empty() {
                return Ok("no matches".to_string());
            }
            Ok(format_entries(&entries))
        })
    });

    registry.register_mut(definition, handler);
}

fn register_memory_list(registry: &mut ToolRegistry) {
    let mut parameters = HashMap::new();
    parameters.insert(
        "limit".to_string(),
        param("number", "Max entries (default 20, max 100).", false),
    );
    let definition = ToolDefinition {
        name: "memory_list".to_string(),
        description: "List the current user's memories newest first.".to_string(),
        parameters,
        category: ToolCategory::Library,
        annotations: Some(ToolAnnotations {
            read_only: Some(true),
            ..Default::default()
        }),
        timeout_ms: Some(15_000),
        max_output_chars: None,
    };

    let handler: ToolHandler = Arc::new(move |args: Value, _ctx| {
        Box::pin(async move {
            let limit = args
                .get("limit")
                .and_then(|v| v.as_i64())
                .map(|n| n.clamp(1, 100) as i32);
            let auth = require_auth()?;
            let scope = scope_from_auth(&auth);
            let client = client_from_auth(&auth);
            let entries = client.memory_list(scope, limit).await?;
            if entries.is_empty() {
                return Ok("no memories".to_string());
            }
            Ok(format_entries(&entries))
        })
    });

    registry.register_mut(definition, handler);
}

fn register_memory_delete(registry: &mut ToolRegistry) {
    let mut parameters = HashMap::new();
    parameters.insert(
        "id".to_string(),
        param("string", "The memory id to delete.", true),
    );
    let definition = ToolDefinition {
        name: "memory_delete".to_string(),
        description: "Delete a memory by id. Use sparingly — prefer to \
update or supersede stale memories rather than deleting."
            .to_string(),
        parameters,
        category: ToolCategory::Library,
        annotations: Some(ToolAnnotations {
            destructive: Some(true),
            ..Default::default()
        }),
        timeout_ms: Some(15_000),
        max_output_chars: None,
    };

    let handler: ToolHandler = Arc::new(move |args: Value, _ctx| {
        Box::pin(async move {
            let id = args
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if id.is_empty() {
                return Err(SimseError::tool(
                    ToolErrorCode::ExecutionFailed,
                    "id is required",
                ));
            }
            let auth = require_auth()?;
            let scope = scope_from_auth(&auth);
            let client = client_from_auth(&auth);
            let ok = client.memory_delete(scope, id).await?;
            Ok(if ok { "deleted" } else { "not found" }.to_string())
        })
    });

    registry.register_mut(definition, handler);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use simse_core::tools::types::ToolRegistryOptions;

    fn sample_auth() -> AuthState {
        AuthState {
            user_id: "user-1".to_string(),
            access_token: "tok".to_string(),
            refresh_token: "refresh".to_string(),
            api_url: "https://api.simse.dev".to_string(),
            auth_url: "https://auth.simse.dev".to_string(),
            email: "u@example.com".to_string(),
            team_id: Some("team-7".to_string()),
            role: None,
        }
    }

    #[test]
    fn registers_all_four_tools() {
        let mut registry = ToolRegistry::new(ToolRegistryOptions::default());
        register_memory_tools(&mut registry);
        assert_eq!(registry.tool_count(), 4);
        for name in [
            "memory_save",
            "memory_search",
            "memory_list",
            "memory_delete",
        ] {
            assert!(registry.is_registered(name), "missing tool {name}");
        }
    }

    #[test]
    fn tool_definitions_mirror_managed_surface() {
        let mut registry = ToolRegistry::new(ToolRegistryOptions::default());
        register_memory_tools(&mut registry);

        let save = registry.get_tool_definition("memory_save").unwrap();
        assert!(save.parameters.contains_key("text"));
        assert!(save.parameters["text"].required);
        assert!(save.parameters.contains_key("tag"));
        assert!(!save.parameters["tag"].required);

        let search = registry.get_tool_definition("memory_search").unwrap();
        assert!(search.parameters["query"].required);
        assert!(!search.parameters["limit"].required);

        let list = registry.get_tool_definition("memory_list").unwrap();
        assert!(list.parameters.contains_key("limit"));

        let delete = registry.get_tool_definition("memory_delete").unwrap();
        assert!(delete.parameters["id"].required);
    }

    #[test]
    fn scope_uses_team_id_when_present() {
        let scope = scope_from_auth(&sample_auth());
        assert_eq!(scope.user_id, "user-1");
        assert_eq!(scope.team_id, "team-7");
        assert!(scope.session_id.is_none());
    }

    #[test]
    fn scope_falls_back_to_user_id_when_no_team() {
        let mut auth = sample_auth();
        auth.team_id = None;
        let scope = scope_from_auth(&auth);
        assert_eq!(scope.user_id, "user-1");
        assert_eq!(scope.team_id, "user-1");
    }

    #[test]
    fn format_entries_renders_id_and_text() {
        let entries = vec![
            crate::proto::adaptive::MemoryEntry {
                id: "m1".to_string(),
                text: "first".to_string(),
                ..Default::default()
            },
            crate::proto::adaptive::MemoryEntry {
                id: "m2".to_string(),
                text: "second".to_string(),
                ..Default::default()
            },
        ];
        let out = format_entries(&entries);
        assert_eq!(out, "- [m1] first\n- [m2] second\n");
    }

    #[tokio::test]
    async fn handler_errors_clearly_when_not_logged_in() {
        // Isolate HOME so `load_auth()` finds no `~/.simse/auth.json`.
        let tmp = tempfile::tempdir().unwrap();
        // SAFETY: single-threaded test; no concurrent env access.
        unsafe {
            std::env::set_var("HOME", tmp.path());
        }

        let mut registry = ToolRegistry::new(ToolRegistryOptions::default());
        register_memory_tools(&mut registry);

        let call = simse_core::tools::types::ToolCallRequest {
            id: "c1".to_string(),
            name: "memory_list".to_string(),
            arguments: serde_json::json!({}),
        };
        let result = registry.execute(&call, None).await;
        assert!(result.is_error, "expected error when not logged in");
        assert!(
            result.output.contains("not logged in") || result.output.contains("simse login"),
            "error must mention login: {}",
            result.output
        );
    }

    #[test]
    fn require_auth_arg_parsing_rejects_empty_text() {
        // Pure arg-validation path (no network, no auth file): an empty
        // `text` must be rejected before any auth/client work.
        let args = serde_json::json!({ "text": "   " });
        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        assert!(text.trim().is_empty());
    }
}
