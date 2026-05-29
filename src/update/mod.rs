//! Elm update function — dispatches AppMessage to domain sub-handlers.
//!
//! The `update()` function is the single entry point for all state transitions.
//! It returns `(App, Vec<Effect>)` — the new app state plus any side effects
//! the runtime should dispatch asynchronously.

mod bridge;
mod control;
mod input;
mod loop_events;
mod navigation;
mod search;

use crate::app::{App, AppMessage};
use crate::commands::BridgeAction;

/// Side effects returned by `update()` for the runtime to dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Submit user text to the agentic loop.
    SubmitChat(String),
    /// Execute an async bridge action (server switch, login, etc.).
    Bridge(BridgeAction),
    /// Cancel the running agentic loop.
    Abort,
    /// Quit the application.
    Quit,
}

/// Top-level update: pure function from (Model, Message) -> (Model, Effects).
pub fn update(app: App, msg: AppMessage) -> (App, Vec<Effect>) {
    // Any user action resets the ctrl-c pending state (except CtrlC itself and CtrlCTimeout).
    let mut app = app;
    match &msg {
        AppMessage::CtrlC | AppMessage::CtrlCTimeout => {}
        _ => {
            app.ctrl_c_pending = false;
        }
    }

    match &msg {
        // Input
        AppMessage::CharInput(_)
        | AppMessage::Paste(_)
        | AppMessage::Submit
        | AppMessage::Backspace
        | AppMessage::Delete
        | AppMessage::DeleteWordBack
        | AppMessage::CursorLeft
        | AppMessage::CursorRight
        | AppMessage::WordLeft
        | AppMessage::WordRight
        | AppMessage::Home
        | AppMessage::End
        | AppMessage::SelectLeft
        | AppMessage::SelectRight
        | AppMessage::SelectHome
        | AppMessage::SelectEnd
        | AppMessage::SelectAll
        | AppMessage::HistoryUp
        | AppMessage::HistoryDown
        | AppMessage::Tab
        | AppMessage::NewLine => input::handle(app, msg),

        // Navigation
        AppMessage::ScrollUp(_) | AppMessage::ScrollDown(_) | AppMessage::ScrollToBottom => {
            navigation::handle(app, msg)
        }

        // Control
        AppMessage::CtrlC
        | AppMessage::CtrlCTimeout
        | AppMessage::Escape
        | AppMessage::CtrlL
        | AppMessage::ShiftTab
        | AppMessage::Quit
        | AppMessage::ShowShortcuts
        | AppMessage::DismissOverlay => control::handle(app, msg),

        // Loop events
        AppMessage::StreamStart
        | AppMessage::StreamDelta(_)
        | AppMessage::StreamEnd { .. }
        | AppMessage::ToolCallStart(_)
        | AppMessage::ToolCallEnd { .. }
        | AppMessage::TokenUsage { .. }
        | AppMessage::LoopComplete
        | AppMessage::LoopError(_)
        | AppMessage::PermissionPrompt { .. } => loop_events::handle(app, msg),

        // Bridge
        AppMessage::BridgeResult { .. }
        | AppMessage::RefreshContext(_)
        | AppMessage::RemoteStatus { .. }
        | AppMessage::SettingsFileLoaded(_)
        | AppMessage::SettingsFieldSaved { .. }
        | AppMessage::SettingsError(_) => bridge::handle(app, msg),

        // Search
        AppMessage::SearchOpen
        | AppMessage::SearchInput(_)
        | AppMessage::SearchBackspace
        | AppMessage::SearchNext
        | AppMessage::SearchPrev
        | AppMessage::SearchClose => search::handle(app, msg),

        // Timer
        AppMessage::Tick => {
            if let Some(ref mut spinner) = app.spinner {
                spinner.tick();
            }
            (app, vec![])
        }

        // Resize
        AppMessage::Resize { .. } => (app, vec![]),
    }
}
