//! Session commands: `/server`, `/model`, `/status`.

use super::{BridgeAction, CommandContext, CommandOutput};

/// `/server [name]` -- show or change the ACP server.
pub fn handle_server(args: &str, ctx: &CommandContext) -> Vec<CommandOutput> {
    let name = args.trim();
    if name.is_empty() {
        return match &ctx.server_name {
            Some(server) => vec![CommandOutput::Success(format!("Current server: {server}"))],
            None => vec![CommandOutput::Info("No ACP server configured.".into())],
        };
    }

    vec![
        CommandOutput::Info(format!("Switching to server: {name}")),
        CommandOutput::BridgeRequest(BridgeAction::SwitchServer {
            name: name.to_string(),
        }),
    ]
}

/// `/model [name]` -- show or change the model.
pub fn handle_model(args: &str, ctx: &CommandContext) -> Vec<CommandOutput> {
    let name = args.trim();
    if name.is_empty() {
        return match &ctx.model_name {
            Some(model) => vec![CommandOutput::Success(format!("Current model: {model}"))],
            None => vec![CommandOutput::Info("No model configured.".into())],
        };
    }

    vec![
        CommandOutput::Info(format!("Switching to model: {name}")),
        CommandOutput::BridgeRequest(BridgeAction::SwitchModel {
            name: name.to_string(),
        }),
    ]
}

/// `/status` -- show connection status.
pub fn handle_status(ctx: &CommandContext) -> Vec<CommandOutput> {
    let server = ctx.server_name.as_deref().unwrap_or("(none)");
    let model = ctx.model_name.as_deref().unwrap_or("(none)");
    let connected = if ctx.acp_connected {
        "connected"
    } else {
        "disconnected"
    };

    vec![CommandOutput::Success(format!(
        "server: {server}\nmodel: {model}\nstatus: {connected}"
    ))]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_ctx() -> CommandContext {
        CommandContext::default()
    }

    #[test]
    fn server_no_args_no_server() {
        let out = handle_server("", &empty_ctx());
        assert!(matches!(&out[0], CommandOutput::Info(msg) if msg.contains("No ACP server")));
    }

    #[test]
    fn server_no_args_shows_current() {
        let ctx = CommandContext {
            server_name: Some("ollama".into()),
            ..Default::default()
        };
        let out = handle_server("", &ctx);
        assert!(matches!(&out[0], CommandOutput::Success(msg) if msg.contains("ollama")));
    }

    #[test]
    fn server_with_name_switches() {
        let out = handle_server("ollama", &empty_ctx());
        assert_eq!(out.len(), 2);
        assert!(matches!(
            &out[1],
            CommandOutput::BridgeRequest(BridgeAction::SwitchServer { name }) if name == "ollama"
        ));
    }

    #[test]
    fn model_no_args_no_model() {
        let out = handle_model("", &empty_ctx());
        assert!(matches!(&out[0], CommandOutput::Info(msg) if msg.contains("No model")));
    }

    #[test]
    fn model_with_name_switches() {
        let out = handle_model("gpt-4o", &empty_ctx());
        assert_eq!(out.len(), 2);
        assert!(matches!(
            &out[1],
            CommandOutput::BridgeRequest(BridgeAction::SwitchModel { name }) if name == "gpt-4o"
        ));
    }

    #[test]
    fn status_shows_info() {
        let ctx = CommandContext {
            server_name: Some("test".into()),
            model_name: Some("gpt-4o".into()),
            acp_connected: true,
            ..Default::default()
        };
        let out = handle_status(&ctx);
        assert!(matches!(
            &out[0],
            CommandOutput::Success(msg) if msg.contains("test") && msg.contains("gpt-4o") && msg.contains("connected")
        ));
    }
}
