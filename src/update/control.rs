//! Control sub-handler — app-level control keys.

use crate::app::{App, AppMessage, LoopStatus};
use crate::ui_core::app::OutputItem;
use crate::update::Effect;

pub fn handle(mut app: App, msg: AppMessage) -> (App, Vec<Effect>) {
    match msg {
        AppMessage::CtrlC => {
            if app.ctrl_c_pending {
                return (app, vec![Effect::Quit]);
            } else {
                app.ctrl_c_pending = true;
            }
        }
        AppMessage::CtrlCTimeout => {
            app.ctrl_c_pending = false;
        }
        AppMessage::Escape => {
            if app.autocomplete.is_active() {
                app.autocomplete.deactivate();
            } else if app.loop_status != LoopStatus::Idle {
                app.loop_status = LoopStatus::Idle;
                app.output.push(OutputItem::Info {
                    text: "Interrupted.".into(),
                });
            }
        }
        AppMessage::CtrlL => {
            app.output.clear();
            app.banner_visible = true;
        }
        AppMessage::ShiftTab => {
            app.permission_mode = match app.permission_mode.as_str() {
                "ask" => "auto".into(),
                "auto" => "bypass".into(),
                _ => "ask".into(),
            };
        }
        AppMessage::Quit => {
            return (app, vec![Effect::Quit]);
        }
        AppMessage::ShowShortcuts | AppMessage::DismissOverlay => {
            // No-op: overlay screens have been removed.
        }
        _ => {}
    }
    (app, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ctrl_c_double_press_quits() {
        let app = App::new();
        let (app, effects) = handle(app, AppMessage::CtrlC);
        assert!(app.ctrl_c_pending);
        assert!(effects.is_empty());
        let (_, effects) = handle(app, AppMessage::CtrlC);
        assert!(effects.contains(&Effect::Quit));
    }

    #[test]
    fn ctrl_c_timeout_resets() {
        let mut app = App::new();
        app.ctrl_c_pending = true;
        let (app, _) = handle(app, AppMessage::CtrlCTimeout);
        assert!(!app.ctrl_c_pending);
    }

    #[test]
    fn escape_deactivates_autocomplete_first() {
        let mut app = App::new();
        app.loop_status = LoopStatus::Streaming;
        app.autocomplete.update_matches("/he", &app.commands);
        assert!(app.autocomplete.is_active());
        let (app, _) = handle(app, AppMessage::Escape);
        assert!(!app.autocomplete.is_active());
        assert_eq!(app.loop_status, LoopStatus::Streaming);
    }

    #[test]
    fn escape_interrupts_loop_when_no_autocomplete() {
        let mut app = App::new();
        app.loop_status = LoopStatus::Streaming;
        let (app, _) = handle(app, AppMessage::Escape);
        assert_eq!(app.loop_status, LoopStatus::Idle);
    }

    #[test]
    fn shift_tab_cycles_permission_mode() {
        let app = App::new();
        assert_eq!(app.permission_mode, "ask");
        let (app, _) = handle(app, AppMessage::ShiftTab);
        assert_eq!(app.permission_mode, "auto");
        let (app, _) = handle(app, AppMessage::ShiftTab);
        assert_eq!(app.permission_mode, "bypass");
        let (app, _) = handle(app, AppMessage::ShiftTab);
        assert_eq!(app.permission_mode, "ask");
    }

    #[test]
    fn ctrl_l_clears_output() {
        let mut app = App::new();
        app.output.push(OutputItem::Info {
            text: "test".into(),
        });
        app.banner_visible = false;
        let (app, _) = handle(app, AppMessage::CtrlL);
        assert!(app.output.is_empty());
        assert!(app.banner_visible);
    }
}
