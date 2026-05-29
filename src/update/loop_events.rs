//! Loop events sub-handler — agentic loop lifecycle.

use crate::app::{App, AppMessage, LoopStatus};
use crate::spinner::ThinkingSpinner;
use crate::ui_core::app::OutputItem;
use crate::update::Effect;

pub fn handle(mut app: App, msg: AppMessage) -> (App, Vec<Effect>) {
    match msg {
        AppMessage::StreamStart => {
            app.loop_status = LoopStatus::Streaming;
            app.stream_text.clear();
            // A new turn is starting — follow the latest content again.
            app.follow_bottom = true;
            app.scroll_offset = 0;
            let server = app.server_name.clone().unwrap_or_default();
            app.spinner = Some(ThinkingSpinner::new(server));
        }
        AppMessage::StreamDelta(delta) => {
            app.stream_text.push_str(&delta);
        }
        AppMessage::StreamEnd { text } => {
            app.spinner = None;
            app.output.push(OutputItem::Message {
                role: "assistant".into(),
                text,
            });
            app.stream_text.clear();
            app.loop_status = LoopStatus::Idle;
        }
        AppMessage::ToolCallStart(tc) => {
            app.tool_call_instants
                .push((tc.id.clone(), std::time::Instant::now()));
            app.active_tool_calls.push(tc);
            app.loop_status = LoopStatus::ToolExecuting;
        }
        AppMessage::ToolCallEnd {
            id,
            status,
            summary,
            error,
            duration_ms,
            diff,
        } => {
            if let Some(pos) = app.active_tool_calls.iter().position(|tc| tc.id == id) {
                let mut tc = app.active_tool_calls.remove(pos);
                tc.status = status;
                tc.summary = summary;
                tc.error = error;
                tc.duration_ms = duration_ms;
                tc.diff = diff;
                app.tool_call_instants.retain(|(tid, _)| *tid != id);
                app.output.push(OutputItem::ToolCall(tc));
                if app.active_tool_calls.is_empty() {
                    app.loop_status = LoopStatus::Streaming;
                }
            }
        }
        AppMessage::TokenUsage { prompt, completion } => {
            app.total_tokens += prompt + completion;
            if let Some(ref mut spinner) = app.spinner {
                spinner.set_token_count(app.total_tokens);
            }
        }
        AppMessage::LoopComplete => {
            app.loop_status = LoopStatus::Idle;
            app.spinner = None;
            for tc in app.active_tool_calls.drain(..) {
                app.output.push(OutputItem::ToolCall(tc));
            }
        }
        AppMessage::LoopError(message) => {
            app.output.push(OutputItem::Error { message });
            app.loop_status = LoopStatus::Idle;
            app.spinner = None;
        }
        AppMessage::PermissionPrompt {
            tool_name,
            args_summary,
            responder,
        } => {
            app.output.push(OutputItem::Info {
                text: format!(
                    "\u{26a0} {tool_name} wants to run: {args_summary}\n  [y] allow  [n] deny"
                ),
            });
            app.pending_permission = Some(responder);
            app.scroll_offset = 0;
        }
        _ => {}
    }
    (app, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_core::app::{ToolCallState, ToolCallStatus};

    #[test]
    fn stream_delta_appends() {
        let app = App::new();
        let (app, _) = handle(app, AppMessage::StreamDelta("hello ".into()));
        let (app, _) = handle(app, AppMessage::StreamDelta("world".into()));
        assert_eq!(app.stream_text, "hello world");
    }

    #[test]
    fn stream_end_resets_state() {
        let mut app = App::new();
        app.loop_status = LoopStatus::Streaming;
        app.stream_text = "partial".into();
        app.spinner = Some(ThinkingSpinner::new("test"));
        let (app, _) = handle(
            app,
            AppMessage::StreamEnd {
                text: "full".into(),
            },
        );
        assert!(app.stream_text.is_empty());
        assert!(app.spinner.is_none());
        assert_eq!(app.loop_status, LoopStatus::Idle);
        assert!(!app.output.is_empty());
    }

    #[test]
    fn tool_call_end_unknown_id_is_noop() {
        let app = App::new();
        let output_len = app.output.len();
        let (app, _) = handle(
            app,
            AppMessage::ToolCallEnd {
                id: "nonexistent".into(),
                status: ToolCallStatus::Completed,
                summary: None,
                error: None,
                duration_ms: None,
                diff: None,
            },
        );
        assert_eq!(app.output.len(), output_len);
    }

    #[test]
    fn loop_complete_drains_active_tool_calls() {
        let mut app = App::new();
        app.active_tool_calls.push(ToolCallState {
            id: "tc1".into(),
            name: "read".into(),
            args: "{}".into(),
            status: ToolCallStatus::Active,
            started_at: 0,
            duration_ms: None,
            summary: None,
            error: None,
            diff: None,
        });
        app.spinner = Some(ThinkingSpinner::new("test"));
        let (app, _) = handle(app, AppMessage::LoopComplete);
        assert!(app.active_tool_calls.is_empty());
        assert!(app.spinner.is_none());
        assert_eq!(app.loop_status, LoopStatus::Idle);
        assert!(
            app.output
                .iter()
                .any(|o| matches!(o, OutputItem::ToolCall(_)))
        );
    }
}
