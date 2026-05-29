//! Protocol types for TUI ↔ runtime communication.

use std::fmt;

use tokio::sync::{mpsc, oneshot};

use simse_core::agentic_loop::TokenUsage;
use simse_core::permissions::PermissionDecision;

// ---------------------------------------------------------------------------
// TUI → Runtime
// ---------------------------------------------------------------------------

/// Messages submitted by the TUI to the runtime.
#[derive(Debug)]
pub enum Submission {
    /// A new user chat message.
    ChatMessage { text: String },
    /// A parsed slash-command (name without the leading `/`).
    SlashCommand { name: String, args: String },
    /// Response to a [`Event::PermissionRequired`] prompt.
    PermissionResponse {
        request_id: String,
        decision: PermissionDecision,
    },
    /// Cancel the running agentic loop.
    Abort,
    /// Shut down the session.
    Quit,
}

// ---------------------------------------------------------------------------
// Runtime → TUI
// ---------------------------------------------------------------------------

/// Events pushed from the runtime to the TUI.
pub enum Event {
    /// The model has started streaming a response.
    StreamStart,
    /// An incremental token chunk.
    StreamDelta(String),
    /// The stream finished; `text` is the full assembled response.
    StreamEnd { text: String },
    /// A tool call has been dispatched.
    ToolCallStart {
        id: String,
        name: String,
        args: String,
    },
    /// A tool call completed.
    ToolCallEnd {
        id: String,
        success: bool,
        summary: Option<String>,
        duration_ms: u64,
        diff: Option<String>,
    },
    /// Accumulated token usage for this turn.
    TokenUsage(TokenUsage),
    /// The agentic loop finished a full turn.
    LoopComplete { text: String, aborted: bool },
    /// The agentic loop hit an unrecoverable error.
    LoopError(String),
    /// A tool needs explicit permission before it can run.
    PermissionRequired {
        tool_name: String,
        args: String,
        responder: oneshot::Sender<bool>,
    },
    /// Result of a bridge action dispatched via a slash-command.
    BridgeResult {
        action: String,
        text: String,
        is_error: bool,
    },
    /// Initial handshake: the runtime is ready.
    Connected,
    /// Remote-connection status changed.
    RemoteStatus {
        connected: bool,
        email: Option<String>,
    },
}

// `oneshot::Sender` does not implement `Debug`, so we provide a manual impl.
impl fmt::Debug for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StreamStart => write!(f, "StreamStart"),
            Self::StreamDelta(s) => f.debug_tuple("StreamDelta").field(s).finish(),
            Self::StreamEnd { text } => f.debug_struct("StreamEnd").field("text", text).finish(),
            Self::ToolCallStart { id, name, args } => f
                .debug_struct("ToolCallStart")
                .field("id", id)
                .field("name", name)
                .field("args", args)
                .finish(),
            Self::ToolCallEnd {
                id,
                success,
                summary,
                duration_ms,
                diff,
            } => f
                .debug_struct("ToolCallEnd")
                .field("id", id)
                .field("success", success)
                .field("summary", summary)
                .field("duration_ms", duration_ms)
                .field("diff", diff)
                .finish(),
            Self::TokenUsage(u) => f.debug_tuple("TokenUsage").field(u).finish(),
            Self::LoopComplete { text, aborted } => f
                .debug_struct("LoopComplete")
                .field("text", text)
                .field("aborted", aborted)
                .finish(),
            Self::LoopError(e) => f.debug_tuple("LoopError").field(e).finish(),
            Self::PermissionRequired {
                tool_name,
                args,
                responder: _,
            } => f
                .debug_struct("PermissionRequired")
                .field("tool_name", tool_name)
                .field("args", args)
                .field("responder", &"<oneshot::Sender>")
                .finish(),
            Self::BridgeResult {
                action,
                text,
                is_error,
            } => f
                .debug_struct("BridgeResult")
                .field("action", action)
                .field("text", text)
                .field("is_error", is_error)
                .finish(),
            Self::Connected => write!(f, "Connected"),
            Self::RemoteStatus { connected, email } => f
                .debug_struct("RemoteStatus")
                .field("connected", connected)
                .field("email", email)
                .finish(),
        }
    }
}

// ---------------------------------------------------------------------------
// Channel aliases
// ---------------------------------------------------------------------------

/// Sender half for [`Submission`] messages (TUI → runtime).
pub type SubTx = mpsc::UnboundedSender<Submission>;
/// Receiver half for [`Submission`] messages (TUI → runtime).
pub type SubRx = mpsc::UnboundedReceiver<Submission>;
/// Sender half for [`Event`] messages (runtime → TUI).
pub type EvTx = mpsc::UnboundedSender<Event>;
/// Receiver half for [`Event`] messages (runtime → TUI).
pub type EvRx = mpsc::UnboundedReceiver<Event>;
