//! Session-related JSON-RPC handlers (`session/*` methods).

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use tokio_util::sync::CancellationToken;

use crate::event_loop::CliRuntime;
use crate::remote_transport::MessageSender;

use super::{
    INVALID_PARAMS, SESSION_BUSY, SESSION_NOT_FOUND, make_error, make_notification, make_response,
};

// ---------------------------------------------------------------------------
// Session state (shared between serve.rs and tunnel handler)
// ---------------------------------------------------------------------------

pub(crate) struct ChatMessage {
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) ts: String,
}

pub(crate) struct Session {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

pub(crate) type SessionMap = HashMap<String, Session>;

/// Shared session state used by both the managed server and the tunnel handler.
pub struct SessionState {
    pub(crate) sessions: RwLock<SessionMap>,
    pub(crate) active_session_id: Mutex<Option<String>>,
    /// Cancellation token for the currently running prompt, if any.
    pub(crate) cancel_token: Mutex<Option<CancellationToken>>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            active_session_id: Mutex::new(None),
            cancel_token: Mutex::new(None),
        }
    }
}

impl SessionState {
    pub fn new() -> Self {
        Self::default()
    }
}

// ---------------------------------------------------------------------------
// Session handlers
// ---------------------------------------------------------------------------

pub(crate) async fn handle_session_new(
    id: Value,
    params: &Value,
    sessions: &SessionState,
) -> String {
    let session_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let title = params
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("New session")
        .to_string();

    let session = Session {
        id: session_id.clone(),
        title: title.clone(),
        messages: Vec::new(),
        created_at: now.clone(),
        updated_at: now.clone(),
    };

    sessions
        .sessions
        .write()
        .await
        .insert(session_id.clone(), session);
    // Do not set active_session_id here; it is set when a prompt starts.

    make_response(
        id,
        serde_json::json!({
            "sessionId": session_id,
            "title": title,
            "createdAt": now,
        }),
    )
}

pub(crate) async fn handle_session_prompt(
    id: Value,
    params: &Value,
    rt: Arc<Mutex<CliRuntime>>,
    sessions: Arc<SessionState>,
    sender: Arc<dyn MessageSender>,
) -> String {
    let session_id = match params.get("sessionId").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return make_error(id, INVALID_PARAMS, "sessionId and text are required");
        }
    };
    let text = match params.get("text").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return make_error(id, INVALID_PARAMS, "sessionId and text are required");
        }
    };

    // Verify session exists.
    {
        let sess = sessions.sessions.read().await;
        if !sess.contains_key(&session_id) {
            return make_error(id, SESSION_NOT_FOUND, "Session not found");
        }
    }

    // Check for busy session and claim it atomically.
    let cancel_token = {
        let mut active = sessions.active_session_id.lock().await;
        if active.is_some() {
            return make_error(id, SESSION_BUSY, "A prompt is already running");
        }
        *active = Some(session_id.clone());

        // Create a new cancellation token for this prompt run.
        let token = CancellationToken::new();
        *sessions.cancel_token.lock().await = Some(token.clone());
        token
    };

    // Store user message.
    let now = chrono::Utc::now().to_rfc3339();
    {
        let mut sess = sessions.sessions.write().await;
        if let Some(session) = sess.get_mut(&session_id) {
            session.messages.push(ChatMessage {
                role: "user".into(),
                content: text.clone(),
                ts: now.clone(),
            });
            session.updated_at = now;
        }
    }

    // Acknowledge prompt received.
    let ack = make_response(id, serde_json::json!({ "ok": true }));
    sender.send_message(&ack).await;

    // Spawn the agentic loop on a background task so incoming messages
    // continue to be processed while the prompt runs.
    tokio::spawn(async move {
        run_acp_prompt(
            sender.as_ref(),
            &session_id,
            &text,
            &rt,
            &sessions,
            cancel_token,
        )
        .await;
    });

    // Return empty string since we already sent the ack via sender.
    // The caller should not send this return value.
    String::new()
}

/// Maximum size (in bytes) for a tool call result in notifications.
/// The WebSocket tunnel has a 128 KB frame limit; we leave headroom for the
/// JSON-RPC envelope by capping the result payload at 120 KB.
const MAX_TOOL_RESULT_BYTES: usize = 120 * 1024;

/// Truncate a string to at most `max_bytes`, appending a truncation marker if
/// the value was shortened. Ensures the cut happens on a valid UTF-8 boundary.
fn truncate_result(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let marker = "... [truncated]";
    let limit = max_bytes.saturating_sub(marker.len());
    let mut end = limit;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &s[..end], marker)
}

/// Events sent from sync [`LoopCallbacks`] closures to the async forwarder
/// via an unbounded channel.
enum CallbackEvent {
    StreamDelta(String),
    ToolCallStart {
        tool_name: String,
        tool_call_id: String,
    },
    ToolCallEnd {
        tool_call_id: String,
        tool_name: String,
        result: String,
        is_error: bool,
    },
    Error(String),
    UsageUpdate {
        input_tokens: u64,
        output_tokens: u64,
    },
    TurnComplete,
}

async fn run_acp_prompt(
    sender: &dyn MessageSender,
    session_id: &str,
    text: &str,
    rt: &Mutex<CliRuntime>,
    sessions: &SessionState,
    cancel_token: CancellationToken,
) {
    use simse_core::agentic_loop::LoopCallbacks;

    // Restore conversation context from the session's message history.
    // Without this, the agentic loop would run with an empty conversation
    // buffer after a session/resume, losing all prior context.
    //
    // Extract messages first, then drop the read guard before acquiring the
    // runtime mutex. This avoids holding both locks simultaneously and
    // matches the lock ordering used elsewhere (e.g. handle_session_resume).
    let prior_messages: Vec<(String, String)> = {
        let sess = sessions.sessions.read().await;
        sess.get(session_id)
            .map(|s| {
                s.messages
                    .iter()
                    .filter(|m| m.role == "user" || m.role == "assistant")
                    .map(|m| (m.role.clone(), m.content.clone()))
                    .collect()
            })
            .unwrap_or_default()
    };

    // Exclude the last message since it is the current user prompt that was
    // just added in handle_session_prompt. handle_submit will add it again.
    let history = if !prior_messages.is_empty() {
        &prior_messages[..prior_messages.len() - 1]
    } else {
        &[]
    };

    if !history.is_empty() {
        let mut rt_guard = rt.lock().await;
        rt_guard.reset_conversation();
        for (role, content) in history {
            match role.as_str() {
                "user" => rt_guard.update_conversation(|c| c.add_user(content)),
                "assistant" => rt_guard.update_conversation(|c| c.add_assistant(content)),
                _ => {}
            }
        }
    }

    // Set up a unified event channel: sync callbacks -> async sends.
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<CallbackEvent>();
    let stream_session_id = session_id.to_string();

    // Build callbacks that feed the channel. Each callback gets its own clone
    // of the sender half so the channel stays open until all callbacks are
    // dropped (when handle_submit returns and drops the LoopCallbacks).
    let delta_tx = event_tx.clone();
    let tool_start_tx = event_tx.clone();
    let tool_end_tx = event_tx.clone();
    let error_tx = event_tx.clone();
    let usage_tx = event_tx.clone();
    let turn_tx = event_tx.clone();
    // Drop the original so the channel closes when all callback clones drop.
    drop(event_tx);

    let callbacks = LoopCallbacks {
        on_stream_delta: Some(std::sync::Arc::new(move |delta: &str| {
            let _ = delta_tx.send(CallbackEvent::StreamDelta(delta.to_string()));
        })),
        on_tool_call_start: Some(Box::new(
            move |req: &simse_core::tools::types::ToolCallRequest| {
                let _ = tool_start_tx.send(CallbackEvent::ToolCallStart {
                    tool_name: req.name.clone(),
                    tool_call_id: req.id.clone(),
                });
            },
        )),
        on_tool_call_end: Some(Box::new(
            move |res: &simse_core::tools::types::ToolCallResult| {
                let _ = tool_end_tx.send(CallbackEvent::ToolCallEnd {
                    tool_call_id: res.id.clone(),
                    tool_name: res.name.clone(),
                    result: truncate_result(&res.output, MAX_TOOL_RESULT_BYTES),
                    is_error: res.is_error,
                });
            },
        )),
        on_error: Some(Box::new(move |err: &simse_core::error::SimseError| {
            let _ = error_tx.send(CallbackEvent::Error(err.to_string()));
        })),
        on_usage_update: Some(Box::new(
            move |usage: &simse_core::agentic_loop::TokenUsage| {
                let _ = usage_tx.send(CallbackEvent::UsageUpdate {
                    input_tokens: usage.input_tokens.unwrap_or(0),
                    output_tokens: usage.output_tokens.unwrap_or(0),
                });
            },
        )),
        on_turn_complete: Some(Box::new(
            move |_turn: &simse_core::agentic_loop::LoopTurn| {
                let _ = turn_tx.send(CallbackEvent::TurnComplete);
            },
        )),
        ..Default::default()
    };

    // Track the latest accumulated usage for the stream_end notification.
    let latest_usage: Arc<std::sync::Mutex<(u64, u64)>> = Arc::new(std::sync::Mutex::new((0, 0)));
    let usage_capture = Arc::clone(&latest_usage);

    // Run the agentic loop and event forwarding concurrently.
    // The loop produces events via callbacks -> event_tx.
    // The forwarder consumes event_rx and sends via sender in real-time.
    //
    // Because tokio::join! runs both futures on the same task, they alternate
    // at .await points: the forwarder's event_rx.recv().await yields to the
    // loop, and the loop's .await points yield to the forwarder -- giving us
    // real-time streaming without spawning (which would require 'static refs).
    //
    // When handle_submit returns, `callbacks` (and thus all tx clones) is
    // dropped, causing event_rx.recv() to return None and the forwarder to exit.
    let fwd_session_id = stream_session_id.clone();
    let loop_fut = async {
        let mut rt_guard = rt.lock().await;
        rt_guard.handle_submit(text, callbacks).await
    };
    let forward_fut = async {
        while let Some(event) = event_rx.recv().await {
            match event {
                CallbackEvent::StreamDelta(delta) => {
                    let notif = make_notification(
                        "session/stream_delta",
                        serde_json::json!({
                            "sessionId": fwd_session_id,
                            "text": delta,
                        }),
                    );
                    sender.send_message(&notif).await;
                }
                CallbackEvent::ToolCallStart {
                    tool_name,
                    tool_call_id,
                } => {
                    let notif = make_notification(
                        "session/tool_call_start",
                        serde_json::json!({
                            "sessionId": fwd_session_id,
                            "toolName": tool_name,
                            "toolCallId": tool_call_id,
                        }),
                    );
                    sender.send_message(&notif).await;
                }
                CallbackEvent::ToolCallEnd {
                    tool_call_id,
                    tool_name,
                    result,
                    is_error,
                } => {
                    let notif = make_notification(
                        "session/tool_call_end",
                        serde_json::json!({
                            "sessionId": fwd_session_id,
                            "toolCallId": tool_call_id,
                            "toolName": tool_name,
                            "result": result,
                            "isError": is_error,
                        }),
                    );
                    sender.send_message(&notif).await;
                }
                CallbackEvent::Error(error) => {
                    let notif = make_notification(
                        "session/error",
                        serde_json::json!({
                            "sessionId": fwd_session_id,
                            "error": error,
                        }),
                    );
                    sender.send_message(&notif).await;
                }
                CallbackEvent::UsageUpdate {
                    input_tokens,
                    output_tokens,
                } => {
                    // Capture latest usage for the stream_end notification.
                    if let Ok(mut u) = usage_capture.lock() {
                        *u = (input_tokens, output_tokens);
                    }
                    let notif = make_notification(
                        "session/usage_update",
                        serde_json::json!({
                            "sessionId": fwd_session_id,
                            "usage": {
                                "inputTokens": input_tokens,
                                "outputTokens": output_tokens,
                            },
                        }),
                    );
                    sender.send_message(&notif).await;
                }
                CallbackEvent::TurnComplete => {
                    let notif = make_notification(
                        "session/turn_complete",
                        serde_json::json!({
                            "sessionId": fwd_session_id,
                        }),
                    );
                    sender.send_message(&notif).await;
                }
            }
        }
    };
    // Race the loop against the cancellation token so session/abort actually
    // stops an in-flight prompt. When cancelled, the joined future is dropped,
    // which cancels handle_submit at its next await point and releases the
    // runtime lock.
    let result = tokio::select! {
        (res, _) = async { tokio::join!(loop_fut, forward_fut) } => res,
        _ = cancel_token.cancelled() => Ok("[Response aborted]".to_string()),
    };

    // Clear active session and cancellation token now that the prompt is done.
    {
        let mut active = sessions.active_session_id.lock().await;
        *active = None;
    }
    {
        let mut token = sessions.cancel_token.lock().await;
        *token = None;
    }

    // Read the latest accumulated usage captured by the on_usage_update callback.
    let (input_tokens, output_tokens) = latest_usage.lock().map(|u| *u).unwrap_or((0, 0));

    match result {
        Ok(response_text) => {
            let now = chrono::Utc::now().to_rfc3339();
            {
                let mut sess = sessions.sessions.write().await;
                if let Some(session) = sess.get_mut(session_id) {
                    session.messages.push(ChatMessage {
                        role: "assistant".into(),
                        content: response_text.clone(),
                        ts: now.clone(),
                    });
                    session.updated_at = now;
                }
            }

            let notif = make_notification(
                "session/stream_end",
                serde_json::json!({
                    "sessionId": stream_session_id,
                    "text": response_text,
                    "usage": {
                        "inputTokens": input_tokens,
                        "outputTokens": output_tokens,
                    },
                }),
            );
            sender.send_message(&notif).await;
        }
        Err(e) => {
            let error_msg = format!("Error: {e}");
            let notif = make_notification(
                "session/stream_end",
                serde_json::json!({
                    "sessionId": stream_session_id,
                    "text": error_msg,
                    "error": e.to_string(),
                    "usage": {
                        "inputTokens": input_tokens,
                        "outputTokens": output_tokens,
                    },
                }),
            );
            sender.send_message(&notif).await;
        }
    }
}

pub(crate) async fn handle_session_list(id: Value, sessions: &SessionState) -> String {
    let sess = sessions.sessions.read().await;
    let mut list: Vec<Value> = sess
        .values()
        .map(|s| {
            serde_json::json!({
                "sessionId": s.id,
                "title": s.title,
                "createdAt": s.created_at,
                "updatedAt": s.updated_at,
                "messageCount": s.messages.len(),
            })
        })
        .collect();

    // Sort by updated_at descending.
    list.sort_by(|a, b| {
        let a_time = a.get("updatedAt").and_then(|v| v.as_str()).unwrap_or("");
        let b_time = b.get("updatedAt").and_then(|v| v.as_str()).unwrap_or("");
        b_time.cmp(a_time)
    });

    make_response(id, serde_json::json!({ "sessions": list }))
}

pub(crate) async fn handle_session_resume(
    id: Value,
    params: &Value,
    sessions: &SessionState,
) -> String {
    let session_id = match params.get("sessionId").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return make_error(id, INVALID_PARAMS, "sessionId is required");
        }
    };

    // Extract all data from the read guard, then drop it before acquiring the
    // mutex. This matches the lock ordering in rpc.rs and avoids a potential
    // deadlock (RwLock held while acquiring Mutex).
    let (sid, title, messages) = {
        let sess = sessions.sessions.read().await;
        let session = match sess.get(session_id) {
            Some(s) => s,
            None => {
                return make_error(id, SESSION_NOT_FOUND, "Session not found");
            }
        };

        let msgs: Vec<Value> = session
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role,
                    "content": m.content,
                    "ts": m.ts,
                })
            })
            .collect();

        (session.id.clone(), session.title.clone(), msgs)
    };

    make_response(
        id,
        serde_json::json!({
            "sessionId": sid,
            "title": title,
            "messages": messages,
        }),
    )
}

pub(crate) async fn handle_session_delete(
    id: Value,
    params: &Value,
    sessions: &SessionState,
) -> String {
    let session_id = match params.get("sessionId").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => {
            return make_error(id, INVALID_PARAMS, "sessionId is required");
        }
    };

    let removed = sessions
        .sessions
        .write()
        .await
        .remove(&session_id)
        .is_some();
    if !removed {
        return make_error(id, SESSION_NOT_FOUND, "Session not found");
    }

    {
        let mut active = sessions.active_session_id.lock().await;
        if active.as_deref() == Some(&session_id) {
            *active = None;
        }
    }

    make_response(id, serde_json::json!({ "ok": true }))
}

pub(crate) async fn handle_session_abort(
    id: Value,
    params: &Value,
    sessions: &SessionState,
) -> String {
    let session_id = match params.get("sessionId").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return make_error(id, INVALID_PARAMS, "sessionId is required");
        }
    };

    let mut active = sessions.active_session_id.lock().await;
    if active.as_deref() == Some(session_id) {
        *active = None;

        // Cancel the running agentic loop via the shared cancellation token.
        if let Some(token) = sessions.cancel_token.lock().await.take() {
            token.cancel();
        }
    }

    make_response(id, serde_json::json!({ "ok": true }))
}
