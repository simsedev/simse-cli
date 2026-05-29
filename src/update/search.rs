//! Search sub-handler — Ctrl+F search open/close/navigate.

use crate::app::{App, AppMessage};
use crate::update::Effect;

/// Scroll the chat output so the current search match is visible.
fn scroll_to_search_match(app: &mut App) {
    if let Some(output_idx) = app.search.current_output_index() {
        let estimated_line = output_idx.saturating_mul(3);
        let total_items = app.output.len();
        let estimated_total = total_items.saturating_mul(3);
        app.scroll_offset = estimated_total
            .saturating_sub(estimated_line)
            .saturating_sub(5);
    }
}

pub fn handle(mut app: App, msg: AppMessage) -> (App, Vec<Effect>) {
    match msg {
        AppMessage::SearchOpen => {
            app.search.open();
        }
        AppMessage::SearchInput(c) => {
            app.search.type_char(c);
            app.search.scan(&app.output);
        }
        AppMessage::SearchBackspace => {
            app.search.backspace();
            app.search.scan(&app.output);
        }
        AppMessage::SearchNext => {
            app.search.next_match();
            scroll_to_search_match(&mut app);
        }
        AppMessage::SearchPrev => {
            app.search.prev_match();
            scroll_to_search_match(&mut app);
        }
        AppMessage::SearchClose => {
            app.search.close();
        }
        _ => {}
    }
    (app, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_open_activates() {
        let app = App::new();
        let (app, effects) = handle(app, AppMessage::SearchOpen);
        assert!(app.search.active);
        assert!(effects.is_empty());
    }

    #[test]
    fn search_close_deactivates() {
        let mut app = App::new();
        app.search.open();
        let (app, effects) = handle(app, AppMessage::SearchClose);
        assert!(!app.search.active);
        assert!(effects.is_empty());
    }
}
