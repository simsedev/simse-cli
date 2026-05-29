//! Unified JSON-RPC handlers for both `session/*` and `remote/*` methods.
//!
//! Used by the embedded tunnel handler (`main.rs`) and the standalone
//! `remote_cmd.rs`. Both code paths delegate to [`dispatch`] which routes
//! to the appropriate handler.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::event_loop::CliRuntime;
use crate::remote_transport::MessageSender;

pub mod remote;
pub mod session;

pub use self::session::SessionState;

// ---------------------------------------------------------------------------
// JSON-RPC envelope helpers
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

#[derive(Serialize)]
struct JsonRpcNotification {
    jsonrpc: &'static str,
    method: &'static str,
    params: Value,
}

fn make_response(id: Value, result: Value) -> String {
    serde_json::to_string(&JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    })
    .unwrap_or_default()
}

fn make_error(id: Value, code: i32, message: &str) -> String {
    serde_json::to_string(&JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.to_string(),
        }),
    })
    .unwrap_or_default()
}

fn make_notification(method: &'static str, params: Value) -> String {
    serde_json::to_string(&JsonRpcNotification {
        jsonrpc: "2.0",
        method,
        params,
    })
    .unwrap_or_default()
}

// Error codes matching cloud/tunnel/src/protocol.ts
const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;
const SESSION_NOT_FOUND: i32 = -32000;
const SESSION_BUSY: i32 = -32001;

// ---------------------------------------------------------------------------
// Public dispatch
// ---------------------------------------------------------------------------

/// Parse a raw JSON-RPC request string and dispatch to the appropriate handler.
///
/// Handles both `session/*` methods (chat) and `remote/*` methods (shell,
/// files, etc.). Returns a JSON-RPC response string. For streaming methods
/// (`session/prompt`), the agentic loop is spawned on a background task and
/// notifications are sent via the `sender` during the loop. The ack response
/// is sent inline and an empty string is returned.
pub async fn dispatch(
    request_text: &str,
    rt: &Arc<Mutex<CliRuntime>>,
    sessions: &Arc<SessionState>,
    sender: &Arc<dyn MessageSender>,
) -> String {
    let req: Value = match serde_json::from_str(request_text) {
        Ok(v) => v,
        Err(_) => return make_error(Value::Null, PARSE_ERROR, "Invalid JSON"),
    };

    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let jsonrpc = req.get("jsonrpc").and_then(|v| v.as_str()).unwrap_or("");
    if jsonrpc != "2.0" || id.is_null() {
        return make_error(id, INVALID_REQUEST, "Invalid JSON-RPC request");
    }

    let method = match req.get("method").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => return make_error(id, INVALID_REQUEST, "Missing method"),
    };

    let params = req
        .get("params")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    match method {
        // Session methods
        "session/new" => session::handle_session_new(id, &params, sessions).await,
        "session/prompt" => {
            session::handle_session_prompt(
                id,
                &params,
                Arc::clone(rt),
                Arc::clone(sessions),
                Arc::clone(sender),
            )
            .await
        }
        "session/list" => session::handle_session_list(id, sessions).await,
        "session/resume" => session::handle_session_resume(id, &params, sessions).await,
        "session/delete" => session::handle_session_delete(id, &params, sessions).await,
        "session/abort" => session::handle_session_abort(id, &params, sessions).await,
        // Remote methods
        "remote/shell" => remote::handle_shell(id, &params, rt).await,
        "remote/files" => remote::handle_files(id, &params, rt).await,
        "remote/memories" => remote::handle_memories(id, sessions).await,
        "remote/agents" => remote::handle_agents(id, rt).await,
        "remote/plugins" => remote::handle_plugins(id, rt).await,
        "remote/network" => remote::handle_network(id, rt).await,
        _ => make_error(id, METHOD_NOT_FOUND, &format!("Unknown method: {method}")),
    }
}

/// Convenience dispatcher for remote/* methods only.
///
/// Used by `remote_cmd.rs` where session/* is not needed.
pub async fn dispatch_remote(
    request_text: &str,
    rt: &Mutex<CliRuntime>,
    sessions: &SessionState,
) -> String {
    let req: Value = match serde_json::from_str(request_text) {
        Ok(v) => v,
        Err(_) => return make_error(Value::Null, PARSE_ERROR, "Invalid JSON"),
    };

    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let jsonrpc = req.get("jsonrpc").and_then(|v| v.as_str()).unwrap_or("");
    if jsonrpc != "2.0" || id.is_null() {
        return make_error(id, INVALID_REQUEST, "Invalid JSON-RPC request");
    }

    let method = match req.get("method").and_then(|v| v.as_str()) {
        Some(m) => m,
        None => return make_error(id, INVALID_REQUEST, "Missing method"),
    };

    let params = req
        .get("params")
        .cloned()
        .unwrap_or(Value::Object(Default::default()));

    match method {
        "remote/shell" => remote::handle_shell(id, &params, rt).await,
        "remote/files" => remote::handle_files(id, &params, rt).await,
        "remote/memories" => remote::handle_memories(id, sessions).await,
        "remote/agents" => remote::handle_agents(id, rt).await,
        "remote/plugins" => remote::handle_plugins(id, rt).await,
        "remote/network" => remote::handle_network(id, rt).await,
        _ => make_error(id, METHOD_NOT_FOUND, &format!("Unknown method: {method}")),
    }
}
