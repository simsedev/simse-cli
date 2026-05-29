//! Bridge sub-handler — bridge results, context refresh, remote status, settings.

use crate::app::{App, AppMessage};
use crate::ui_core::app::OutputItem;
use crate::update::Effect;

pub fn handle(mut app: App, msg: AppMessage) -> (App, Vec<Effect>) {
    match msg {
        AppMessage::BridgeResult {
            action,
            text,
            is_error,
        } => {
            if is_error {
                app.output.push(OutputItem::Error { message: text });
            } else {
                app.output.push(OutputItem::CommandResult { text });
            }
            if action == "factory-reset" && !is_error {
                app.server_name = None;
                app.model_name = None;
                app.acp_connected = false;
            }
        }
        AppMessage::RefreshContext(ctx) => {
            app.server_name = ctx.server_name;
            app.model_name = ctx.model_name;
            app.session_id = ctx.session_id;
            app.acp_connected = ctx.acp_connected;
        }
        AppMessage::RemoteStatus { connected, email } => {
            app.remote_connected = connected;
            app.remote_email = email;
        }
        AppMessage::SettingsFileLoaded(_)
        | AppMessage::SettingsFieldSaved { .. }
        | AppMessage::SettingsError(_) => {
            // No-op: overlay removed.
        }
        _ => {}
    }
    (app, vec![])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CommandContext;

    #[test]
    fn bridge_result_factory_reset_clears_state() {
        let mut app = App::new();
        app.server_name = Some("test".into());
        app.model_name = Some("model".into());
        app.acp_connected = true;
        let (app, _) = handle(
            app,
            AppMessage::BridgeResult {
                action: "factory-reset".into(),
                text: "Reset complete.".into(),
                is_error: false,
            },
        );
        assert!(app.server_name.is_none());
        assert!(app.model_name.is_none());
        assert!(!app.acp_connected);
    }

    #[test]
    fn refresh_context_updates_fields() {
        let app = App::new();
        let ctx = CommandContext {
            server_name: Some("srv".into()),
            model_name: Some("mdl".into()),
            session_id: Some("sid".into()),
            acp_connected: true,
        };
        let (app, _) = handle(app, AppMessage::RefreshContext(ctx));
        assert_eq!(app.server_name, Some("srv".into()));
        assert_eq!(app.model_name, Some("mdl".into()));
        assert_eq!(app.session_id, Some("sid".into()));
        assert!(app.acp_connected);
    }
}
