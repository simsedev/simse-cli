//! Input sub-handler — text editing, submission, history, autocomplete.

use crate::app::{App, AppMessage, LoopStatus};
use crate::commands::{CommandContext, CommandOutput, format_table};
use crate::constants::MAX_HISTORY_SIZE;
use crate::dispatch::{DispatchContext, parse_command_line};
use crate::ui_core::app::OutputItem;
use crate::ui_core::input::state as input;
use crate::update::Effect;

pub fn handle(mut app: App, msg: AppMessage) -> (App, Vec<Effect>) {
    match msg {
        AppMessage::CharInput(c) => {
            // Intercept y/n when a permission prompt is pending.
            if app.pending_permission.is_some() {
                match c {
                    'y' | 'Y' => {
                        if let Some(responder) = app.pending_permission.take() {
                            let _ = responder.send(true);
                        }
                        app.output.push(OutputItem::Info {
                            text: "Allowed.".into(),
                        });
                        return (app, vec![]);
                    }
                    'n' | 'N' => {
                        if let Some(responder) = app.pending_permission.take() {
                            let _ = responder.send(false);
                        }
                        app.output.push(OutputItem::Info {
                            text: "Denied.".into(),
                        });
                        return (app, vec![]);
                    }
                    _ => {
                        // Ignore other keys while permission is pending.
                        return (app, vec![]);
                    }
                }
            }
            app.input = input::insert(&app.input, &c.to_string());
            app.autocomplete
                .update_matches(&app.input.value, &app.commands);
        }
        AppMessage::Paste(text) => {
            app.input = input::insert(&app.input, &text);
            app.autocomplete
                .update_matches(&app.input.value, &app.commands);
        }
        AppMessage::Submit => {
            app.autocomplete.deactivate();
            // Don't accept new messages while the agentic loop is running.
            if app.loop_status != LoopStatus::Idle {
                return (app, vec![]);
            }
            let text = app.input.value.trim().to_string();
            if text.is_empty() {
                return (app, vec![]);
            }

            // Add to history (dedup consecutive, cap at limit).
            if app.history.last().is_none_or(|last| *last != text) {
                app.history.push(text.clone());
                if app.history.len() > MAX_HISTORY_SIZE {
                    app.history.remove(0);
                }
            }
            app.history_index = None;
            app.history_draft.clear();

            // Clear input.
            app.input = input::InputState::default();
            app.banner_visible = false;

            // Handle "exit" / "quit" bare words.
            let lower = text.to_lowercase();
            if lower == "exit" || lower == "quit" {
                return (app, vec![Effect::Quit]);
            }

            // Command dispatch.
            if text.starts_with('/') {
                return dispatch_command(app, &text);
            }

            // Regular user message — display it and queue for agentic loop.
            app.output.push(OutputItem::Message {
                role: "user".into(),
                text: text.clone(),
            });
            return (app, vec![Effect::SubmitChat(text)]);
        }
        AppMessage::Backspace => {
            app.input = input::backspace(&app.input);
            app.autocomplete
                .update_matches(&app.input.value, &app.commands);
        }
        AppMessage::Delete => {
            app.input = input::delete(&app.input);
            app.autocomplete
                .update_matches(&app.input.value, &app.commands);
        }
        AppMessage::DeleteWordBack => {
            app.input = input::delete_word_back(&app.input);
            app.autocomplete
                .update_matches(&app.input.value, &app.commands);
        }
        AppMessage::CursorLeft => {
            app.input = input::move_left(&app.input, false);
        }
        AppMessage::CursorRight => {
            app.input = input::move_right(&app.input, false);
        }
        AppMessage::WordLeft => {
            app.input = input::move_word_left(&app.input, false);
        }
        AppMessage::WordRight => {
            app.input = input::move_word_right(&app.input, false);
        }
        AppMessage::Home => {
            app.input = input::move_home(&app.input, false);
        }
        AppMessage::End => {
            app.input = input::move_end(&app.input, false);
        }
        AppMessage::SelectLeft => {
            app.input = input::move_left(&app.input, true);
        }
        AppMessage::SelectRight => {
            app.input = input::move_right(&app.input, true);
        }
        AppMessage::SelectHome => {
            app.input = input::move_home(&app.input, true);
        }
        AppMessage::SelectEnd => {
            app.input = input::move_end(&app.input, true);
        }
        AppMessage::SelectAll => {
            app.input = input::select_all(&app.input);
        }
        AppMessage::HistoryUp => {
            if app.autocomplete.is_active() {
                app.autocomplete.move_up();
                return (app, vec![]);
            }
            if app.history.is_empty() {
                return (app, vec![]);
            }
            match app.history_index {
                None => {
                    // Save current input as draft.
                    app.history_draft = app.input.value.clone();
                    let idx = app.history.len() - 1;
                    app.history_index = Some(idx);
                    let text = app.history[idx].clone();
                    app.input = input::InputState::default();
                    app.input = input::insert(&app.input, &text);
                }
                Some(idx) if idx > 0 => {
                    let new_idx = idx - 1;
                    app.history_index = Some(new_idx);
                    let text = app.history[new_idx].clone();
                    app.input = input::InputState::default();
                    app.input = input::insert(&app.input, &text);
                }
                _ => {}
            }
        }
        AppMessage::HistoryDown => {
            if app.autocomplete.is_active() {
                app.autocomplete.move_down();
                return (app, vec![]);
            }
            if let Some(idx) = app.history_index {
                if idx + 1 < app.history.len() {
                    let new_idx = idx + 1;
                    app.history_index = Some(new_idx);
                    let text = app.history[new_idx].clone();
                    app.input = input::InputState::default();
                    app.input = input::insert(&app.input, &text);
                } else {
                    // Past end: restore draft.
                    app.history_index = None;
                    let draft = app.history_draft.clone();
                    app.input = input::InputState::default();
                    app.input = input::insert(&app.input, &draft);
                }
            }
        }
        AppMessage::Tab => {
            if app.autocomplete.is_active() {
                if let Some(completed) = app.autocomplete.accept() {
                    let with_space = format!("{completed} ");
                    app.input = input::InputState {
                        value: with_space.clone(),
                        cursor: with_space.len(),
                        ..Default::default()
                    };
                }
            } else if app.input.value.starts_with('/') {
                app.autocomplete
                    .update_matches(&app.input.value, &app.commands);
            }
        }
        AppMessage::NewLine => {
            app.input = input::insert(&app.input, "\n");
        }
        _ => {}
    }
    (app, vec![])
}

// ── Command dispatch ────────────────────────────────────

/// Dispatch a slash command and return (App, Effects).
fn dispatch_command(mut app: App, text: &str) -> (App, Vec<Effect>) {
    let mut effects = Vec::new();

    let Some((command, args)) = parse_command_line(text) else {
        app.output.push(OutputItem::Error {
            message: "Invalid command.".into(),
        });
        return (app, effects);
    };

    // Handle clear and exit directly — no sentinel needed.
    match command.as_str() {
        "clear" => {
            app.output.clear();
            app.banner_visible = true;
            return (app, effects);
        }
        "exit" | "quit" | "q" => {
            return (app, vec![Effect::Quit]);
        }
        _ => {}
    }

    let cmd_ctx = CommandContext {
        server_name: app.server_name.clone(),
        model_name: app.model_name.clone(),
        session_id: app.session_id.clone(),
        acp_connected: app.acp_connected,
    };

    let ctx = DispatchContext {
        verbose: app.verbose,
        plan: app.plan_mode,
        total_tokens: app.total_tokens,
        context_percent: app.context_percent,
        commands: app.commands.clone(),
        cmd_ctx,
    };

    let results = ctx.dispatch(&command, &args);

    // Apply side effects for commands that mutate app state. Aliases share
    // their canonical name's mutation branch so `/permissions` actually
    // flips `app.plan_mode` (dispatch maps both to handle_plan, but the
    // mutation match was previously keyed on the canonical name only).
    match command.as_str() {
        "verbose" | "v" => {
            for r in &results {
                if let CommandOutput::Success(msg) = r {
                    app.verbose = msg.contains(" on");
                }
            }
        }
        "plan" | "permissions" => {
            for r in &results {
                if let CommandOutput::Success(msg) = r {
                    app.plan_mode = msg.contains(" on");
                }
            }
        }
        _ => {}
    }

    // Convert CommandOutput items into App output.
    for result in results {
        match result {
            CommandOutput::Success(text) => {
                app.output.push(OutputItem::CommandResult { text });
            }
            CommandOutput::Error(message) => {
                app.output.push(OutputItem::Error { message });
            }
            CommandOutput::Info(text) => {
                // Filter out sentinel values (clear/exit handled above).
                if text == "__clear__" || text == "__exit__" {
                    continue;
                }
                app.output.push(OutputItem::Info { text });
            }
            CommandOutput::Table { headers, rows } => {
                let text = format_table(&headers, &rows);
                app.output.push(OutputItem::CommandResult { text });
            }
            CommandOutput::BridgeRequest(action) => {
                effects.push(Effect::Bridge(action));
            }
        }
    }
    (app, effects)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submit_empty_does_nothing() {
        let app = App::new();
        let (app, effects) = handle(app, AppMessage::Submit);
        assert!(app.history.is_empty());
        assert!(app.output.is_empty());
        assert!(effects.is_empty());
    }

    #[test]
    fn submit_during_active_loop_rejected() {
        let mut app = App::new();
        app.loop_status = LoopStatus::Streaming;
        app.input = input::insert(&app.input, "hello");
        let (app, effects) = handle(app, AppMessage::Submit);
        assert_eq!(app.input.value, "hello");
        assert!(effects.is_empty());
    }

    #[test]
    fn submit_text_returns_submit_chat_effect() {
        let mut app = App::new();
        app.input = input::insert(&app.input, "hello world");
        let (app, effects) = handle(app, AppMessage::Submit);
        assert!(app.input.value.is_empty());
        assert_eq!(app.history.len(), 1);
        assert!(effects.iter().any(|e| matches!(e, Effect::SubmitChat(_))));
    }

    #[test]
    fn submit_slash_command_returns_bridge_effect() {
        let mut app = App::new();
        app.input = input::insert(&app.input, "/compact");
        let (_, effects) = handle(app, AppMessage::Submit);
        assert!(effects.iter().any(|e| matches!(e, Effect::Bridge(_))));
    }

    #[test]
    fn submit_exit_returns_quit_effect() {
        let mut app = App::new();
        app.input = input::insert(&app.input, "exit");
        let (_, effects) = handle(app, AppMessage::Submit);
        assert!(effects.contains(&Effect::Quit));
    }

    #[test]
    fn slash_exit_returns_quit_effect() {
        let mut app = App::new();
        app.input = input::insert(&app.input, "/exit");
        let (_, effects) = handle(app, AppMessage::Submit);
        assert!(effects.contains(&Effect::Quit));
    }

    #[test]
    fn slash_clear_clears_output() {
        let mut app = App::new();
        app.output.push(OutputItem::Info {
            text: "test".into(),
        });
        app.banner_visible = false;
        app.input = input::insert(&app.input, "/clear");
        let (app, _) = handle(app, AppMessage::Submit);
        assert!(app.output.is_empty());
        assert!(app.banner_visible);
    }

    #[test]
    fn tab_with_no_slash_is_noop() {
        let mut app = App::new();
        app.input = input::insert(&app.input, "hello");
        let (app, _) = handle(app, AppMessage::Tab);
        assert!(!app.autocomplete.is_active());
    }

    #[test]
    fn history_navigation() {
        let mut app = App::new();
        app.history = vec!["first".into(), "second".into()];
        app.input = input::insert(&app.input, "draft");
        let (app, _) = handle(app, AppMessage::HistoryUp);
        assert_eq!(app.input.value, "second");
        let (app, _) = handle(app, AppMessage::HistoryUp);
        assert_eq!(app.input.value, "first");
        let (app, _) = handle(app, AppMessage::HistoryDown);
        assert_eq!(app.input.value, "second");
        let (app, _) = handle(app, AppMessage::HistoryDown);
        assert_eq!(app.input.value, "draft");
    }

    #[test]
    fn slash_verbose_toggles() {
        let mut app = App::new();
        assert!(!app.verbose);
        app.input = input::insert(&app.input, "/verbose");
        let (app, _) = handle(app, AppMessage::Submit);
        assert!(app.verbose);
    }

    #[test]
    fn unknown_command_shows_error() {
        let mut app = App::new();
        app.input = input::insert(&app.input, "/nonexistent");
        let (app, _) = handle(app, AppMessage::Submit);
        assert!(
            app.output
                .iter()
                .any(|o| matches!(o, OutputItem::Error { .. }))
        );
    }

    #[test]
    fn question_mark_inserts_as_regular_char() {
        let app = App::new();
        let (app, _) = handle(app, AppMessage::CharInput('?'));
        assert_eq!(app.input.value, "?");
    }
}
