//! File commands: `/diff`.

use super::{BridgeAction, CommandOutput};

/// `/diff [path]` -- show diff of file changes.
pub fn handle_diff(args: &str) -> Vec<CommandOutput> {
    let path = args.trim();
    if path.is_empty() {
        vec![
            CommandOutput::Info("Generating diff...".into()),
            CommandOutput::BridgeRequest(BridgeAction::DiffFiles { path: None }),
        ]
    } else {
        if path.contains('\0') {
            return vec![CommandOutput::Error(
                "Invalid path: contains null bytes".into(),
            )];
        }
        vec![
            CommandOutput::Info("Generating diff...".into()),
            CommandOutput::BridgeRequest(BridgeAction::DiffFiles {
                path: Some(path.into()),
            }),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_no_args_shows_all() {
        let out = handle_diff("");
        assert_eq!(out.len(), 2);
        assert!(matches!(&out[0], CommandOutput::Info(msg) if msg == "Generating diff..."));
        assert!(matches!(
            &out[1],
            CommandOutput::BridgeRequest(BridgeAction::DiffFiles { path: None })
        ));
    }

    #[test]
    fn diff_with_path() {
        let out = handle_diff("lib.rs");
        assert_eq!(out.len(), 2);
        assert!(matches!(
            &out[1],
            CommandOutput::BridgeRequest(BridgeAction::DiffFiles { path: Some(p) }) if p == "lib.rs"
        ));
    }

    #[test]
    fn diff_null_byte_is_error() {
        let out = handle_diff("z\0z");
        assert!(matches!(&out[0], CommandOutput::Error(_)));
    }
}
