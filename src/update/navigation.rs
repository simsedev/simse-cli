//! Navigation sub-handler — viewport scrolling.

use crate::app::{App, AppMessage};
use crate::update::Effect;

pub fn handle(mut app: App, msg: AppMessage) -> (App, Vec<Effect>) {
    match msg {
        AppMessage::ScrollUp(n) => {
            app.scroll_offset = app.scroll_offset.saturating_add(n);
            app.follow_bottom = false;
        }
        AppMessage::ScrollDown(n) => {
            app.scroll_offset = app.scroll_offset.saturating_sub(n);
            if app.scroll_offset == 0 {
                app.follow_bottom = true;
            }
        }
        AppMessage::ScrollToBottom => {
            app.scroll_offset = 0;
            app.follow_bottom = true;
        }
        _ => {}
    }
    (app, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_up_increases_offset() {
        let app = App::new();
        let (app, _) = handle(app, AppMessage::ScrollUp(5));
        assert_eq!(app.scroll_offset, 5);
    }

    #[test]
    fn scroll_down_decreases_offset() {
        let mut app = App::new();
        app.scroll_offset = 10;
        let (app, _) = handle(app, AppMessage::ScrollDown(3));
        assert_eq!(app.scroll_offset, 7);
    }

    #[test]
    fn scroll_to_bottom_resets() {
        let mut app = App::new();
        app.scroll_offset = 50;
        let (app, _) = handle(app, AppMessage::ScrollToBottom);
        assert_eq!(app.scroll_offset, 0);
    }
}
