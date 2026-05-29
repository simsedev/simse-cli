//! Permission policy, mode cycling, rule storage.

use serde::{Deserialize, Serialize};

/// Permission mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionMode {
    Default,
    AcceptEdits,
    Plan,
    DontAsk,
}

/// Decision for a permission check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PermissionDecision {
    Allow,
    Deny,
    Ask,
}

/// A permission rule for a specific tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionRule {
    pub tool: String,
    pub pattern: Option<String>,
    pub policy: PermissionDecision,
}

/// Permission state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionState {
    pub mode: PermissionMode,
    pub rules: Vec<PermissionRule>,
}

pub fn new_permission_state(mode: PermissionMode) -> PermissionState {
    PermissionState {
        mode,
        rules: Vec::new(),
    }
}

const MODES: [PermissionMode; 4] = [
    PermissionMode::Default,
    PermissionMode::AcceptEdits,
    PermissionMode::Plan,
    PermissionMode::DontAsk,
];

pub fn cycle_mode(state: &PermissionState) -> PermissionMode {
    let idx = MODES.iter().position(|m| *m == state.mode).unwrap_or(0);
    MODES[(idx + 1) % MODES.len()]
}

pub fn format_mode(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Default => "Default",
        PermissionMode::AcceptEdits => "Accept Edits",
        PermissionMode::Plan => "Plan",
        PermissionMode::DontAsk => "Don't Ask",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_through_modes() {
        let mut state = new_permission_state(PermissionMode::Default);
        state.mode = cycle_mode(&state);
        assert_eq!(state.mode, PermissionMode::AcceptEdits);
        state.mode = cycle_mode(&state);
        assert_eq!(state.mode, PermissionMode::Plan);
        state.mode = cycle_mode(&state);
        assert_eq!(state.mode, PermissionMode::DontAsk);
        state.mode = cycle_mode(&state);
        assert_eq!(state.mode, PermissionMode::Default);
    }
}
