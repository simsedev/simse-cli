//! Permission manager: tool permission checking with modes, rules, and glob matching.
//!
//! This module provides the `PermissionManager` struct that determines whether a tool
//! call should be allowed, denied, or needs user confirmation. It combines:
//!
//! - **Explicit rules** (highest priority): per-tool or glob-matched rules
//! - **Mode-based logic**: `Default`, `AcceptEdits`, `Plan`, `DontAsk`
//!
//! Since `ui_core` is a no-I/O crate, serialization/deserialization is to/from
//! `String` (JSON). The caller is responsible for actual file I/O.

use im::Vector;
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::permissions::{PermissionDecision, PermissionMode, PermissionRule};

// ---------------------------------------------------------------------------
// Tool categories
// ---------------------------------------------------------------------------

/// Tools that modify the filesystem.
const WRITE_TOOLS: &[&str] = &[
    "vfs_write",
    "vfs_delete",
    "vfs_rename",
    "vfs_mkdir",
    "file_write",
    "file_edit",
    "file_create",
];

/// Tools that execute commands.
const BASH_TOOLS: &[&str] = &["bash", "shell", "exec", "execute", "run_command"];

/// Tools that only read data.
const READ_ONLY_TOOLS: &[&str] = &[
    "vfs_read",
    "vfs_list",
    "vfs_stat",
    "vfs_search",
    "vfs_diff",
    "file_read",
    "glob",
    "grep",
    "library_search",
    "library_list",
    "task_list",
    "task_get",
];

fn is_write_tool(name: &str) -> bool {
    WRITE_TOOLS.contains(&name)
}

fn is_bash_tool(name: &str) -> bool {
    BASH_TOOLS.contains(&name)
}

fn is_read_only_tool(name: &str) -> bool {
    READ_ONLY_TOOLS.contains(&name)
}

// ---------------------------------------------------------------------------
// Mode labels and descriptions
// ---------------------------------------------------------------------------

/// Human-readable short label for a permission mode.
pub fn mode_label(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Default => "Default",
        PermissionMode::AcceptEdits => "Auto-Edit",
        PermissionMode::Plan => "Plan (read-only)",
        PermissionMode::DontAsk => "YOLO",
    }
}

/// Human-readable description for a permission mode.
pub fn mode_description(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::Default => "Ask for writes & bash",
        PermissionMode::AcceptEdits => "Auto-allow file edits, ask for bash",
        PermissionMode::Plan => "Read-only \u{2014} deny writes & bash",
        PermissionMode::DontAsk => "Allow everything without asking",
    }
}

// ---------------------------------------------------------------------------
// Glob matching
// ---------------------------------------------------------------------------

/// Convert a simple glob pattern (supporting `*` and `?`) into a regex string.
///
/// Special regex characters are escaped. Then `*` becomes `.*` and `?` becomes `.`.
fn glob_to_regex(pattern: &str) -> String {
    let mut result = String::with_capacity(pattern.len() * 2);
    result.push('^');
    for ch in pattern.chars() {
        match ch {
            '*' => result.push_str(".*"),
            '?' => result.push('.'),
            // Escape regex metacharacters
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                result.push('\\');
                result.push(ch);
            }
            _ => result.push(ch),
        }
    }
    result.push('$');
    result
}

/// Check if a glob pattern matches a value.
fn match_glob(pattern: &str, value: &str) -> bool {
    let regex_str = glob_to_regex(pattern);
    Regex::new(&regex_str)
        .map(|re| re.is_match(value))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Serialization format
// ---------------------------------------------------------------------------

/// JSON-serializable permission state for persistence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionData {
    pub mode: PermissionMode,
    pub rules: Vec<PermissionRule>,
}

// ---------------------------------------------------------------------------
// PermissionManager
// ---------------------------------------------------------------------------

/// Manages permission decisions for tool execution.
///
/// Combines explicit rules (checked first via glob matching) with mode-based
/// logic to produce `Allow`, `Deny`, or `Ask` decisions.
///
/// Uses `im::Vector` for rules to enable cheap cloning and functional-style
/// state transitions. All mutating methods take `self` by value and return
/// the updated `Self` (owned-return pattern).
#[derive(Debug, Clone)]
pub struct PermissionManager {
    mode: PermissionMode,
    rules: Vector<PermissionRule>,
    config_path: Option<String>,
}

/// Mode cycle order.
const MODES: [PermissionMode; 4] = [
    PermissionMode::Default,
    PermissionMode::AcceptEdits,
    PermissionMode::Plan,
    PermissionMode::DontAsk,
];

impl PermissionManager {
    /// Create a new `PermissionManager` with the given initial mode.
    pub fn new(mode: PermissionMode) -> Self {
        Self {
            mode,
            rules: Vector::new(),
            config_path: None,
        }
    }

    /// Create a new `PermissionManager` with a config path for external persistence.
    pub fn with_config_path(mode: PermissionMode, config_path: String) -> Self {
        Self {
            mode,
            rules: Vector::new(),
            config_path: Some(config_path),
        }
    }

    /// Get the config path.
    pub fn config_path(&self) -> Option<&str> {
        self.config_path.as_deref()
    }

    /// Set the config path. Returns the updated manager.
    pub fn set_config_path(mut self, path: Option<String>) -> Self {
        self.config_path = path;
        self
    }

    /// Check whether a tool call should be allowed, denied, or needs user confirmation.
    ///
    /// Rules are checked first (highest priority). If no rule matches, mode-based logic
    /// applies. The `_args` parameter is reserved for future pattern matching on arguments.
    pub fn check(&self, tool_name: &str, _args: Option<&str>) -> PermissionDecision {
        // Check explicit rules first (highest priority)
        for rule in &self.rules {
            if rule.tool == tool_name || match_glob(&rule.tool, tool_name) {
                return rule.policy;
            }
        }

        // Mode-based decisions
        match self.mode {
            PermissionMode::DontAsk => PermissionDecision::Allow,

            PermissionMode::Plan => {
                if is_read_only_tool(tool_name) {
                    PermissionDecision::Allow
                } else if is_write_tool(tool_name) || is_bash_tool(tool_name) {
                    PermissionDecision::Deny
                } else {
                    PermissionDecision::Ask
                }
            }

            PermissionMode::AcceptEdits => {
                if is_bash_tool(tool_name) {
                    PermissionDecision::Ask
                } else {
                    PermissionDecision::Allow
                }
            }

            PermissionMode::Default => {
                if is_read_only_tool(tool_name) {
                    PermissionDecision::Allow
                } else {
                    PermissionDecision::Ask
                }
            }
        }
    }

    /// Get the current permission mode.
    pub fn get_mode(&self) -> PermissionMode {
        self.mode
    }

    /// Set the permission mode. Returns the updated manager.
    pub fn set_mode(mut self, mode: PermissionMode) -> Self {
        self.mode = mode;
        self
    }

    /// Cycle to the next permission mode.
    ///
    /// Order: Default -> AcceptEdits -> Plan -> DontAsk -> Default.
    /// Returns `(updated_self, new_mode)`.
    pub fn cycle_mode(mut self) -> (Self, PermissionMode) {
        let idx = MODES.iter().position(|m| *m == self.mode).unwrap_or(0);
        self.mode = MODES[(idx + 1) % MODES.len()];
        let mode = self.mode;
        (self, mode)
    }

    /// Add a permission rule. If a rule for the same tool pattern already exists,
    /// it is replaced. Returns the updated manager.
    pub fn add_rule(mut self, rule: PermissionRule) -> Self {
        if let Some(idx) = self.rules.iter().position(|r| r.tool == rule.tool) {
            self.rules.remove(idx);
        }
        self.rules.push_back(rule);
        self
    }

    /// Remove a rule by tool name/pattern. Returns `(updated_self, was_removed)`.
    pub fn remove_rule(mut self, tool_name: &str) -> (Self, bool) {
        if let Some(idx) = self.rules.iter().position(|r| r.tool == tool_name) {
            self.rules.remove(idx);
            (self, true)
        } else {
            (self, false)
        }
    }

    /// Get all rules as a slice-like view.
    pub fn get_rules(&self) -> &Vector<PermissionRule> {
        &self.rules
    }

    /// Format the current mode for display (label + description).
    pub fn format_mode(&self) -> String {
        format!(
            "{} \u{2014} {}",
            mode_label(self.mode),
            mode_description(self.mode)
        )
    }

    /// Serialize the current state (mode + rules) to a JSON string.
    ///
    /// The caller is responsible for writing the string to disk.
    pub fn save(&self) -> Result<String, serde_json::Error> {
        let data = PermissionData {
            mode: self.mode,
            rules: self.rules.iter().cloned().collect(),
        };
        serde_json::to_string_pretty(&data)
    }

    /// Load state (mode + rules) from a JSON string.
    ///
    /// The caller is responsible for reading the string from disk.
    /// On success, returns the updated manager. On error, returns `self` unchanged.
    pub fn load(mut self, json: &str) -> Result<Self, serde_json::Error> {
        let data: PermissionData = serde_json::from_str(json)?;
        self.mode = data.mode;
        self.rules = data.rules.into_iter().collect();
        Ok(self)
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new(PermissionMode::Default)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // Construction and defaults
    // -----------------------------------------------------------------------

    #[test]
    fn new_with_default_mode() {
        let pm = PermissionManager::new(PermissionMode::Default);
        assert_eq!(pm.get_mode(), PermissionMode::Default);
        assert!(pm.get_rules().is_empty());
    }

    #[test]
    fn default_impl() {
        let pm = PermissionManager::default();
        assert_eq!(pm.get_mode(), PermissionMode::Default);
    }

    #[test]
    fn new_with_custom_mode() {
        let pm = PermissionManager::new(PermissionMode::DontAsk);
        assert_eq!(pm.get_mode(), PermissionMode::DontAsk);
    }

    // -----------------------------------------------------------------------
    // Mode management
    // -----------------------------------------------------------------------

    #[test]
    fn set_mode() {
        let pm = PermissionManager::default();
        let pm = pm.set_mode(PermissionMode::Plan);
        assert_eq!(pm.get_mode(), PermissionMode::Plan);
    }

    #[test]
    fn cycle_mode_full_circle() {
        let pm = PermissionManager::default();
        assert_eq!(pm.get_mode(), PermissionMode::Default);

        let (pm, next) = pm.cycle_mode();
        assert_eq!(next, PermissionMode::AcceptEdits);
        assert_eq!(pm.get_mode(), PermissionMode::AcceptEdits);

        let (pm, next) = pm.cycle_mode();
        assert_eq!(next, PermissionMode::Plan);

        let (pm, next) = pm.cycle_mode();
        assert_eq!(next, PermissionMode::DontAsk);

        let (_pm, next) = pm.cycle_mode();
        assert_eq!(next, PermissionMode::Default);
    }

    #[test]
    fn format_mode_default() {
        let pm = PermissionManager::default();
        assert_eq!(pm.format_mode(), "Default \u{2014} Ask for writes & bash");
    }

    #[test]
    fn format_mode_accept_edits() {
        let pm = PermissionManager::new(PermissionMode::AcceptEdits);
        assert_eq!(
            pm.format_mode(),
            "Auto-Edit \u{2014} Auto-allow file edits, ask for bash"
        );
    }

    #[test]
    fn format_mode_plan() {
        let pm = PermissionManager::new(PermissionMode::Plan);
        assert_eq!(
            pm.format_mode(),
            "Plan (read-only) \u{2014} Read-only \u{2014} deny writes & bash"
        );
    }

    #[test]
    fn format_mode_dont_ask() {
        let pm = PermissionManager::new(PermissionMode::DontAsk);
        assert_eq!(
            pm.format_mode(),
            "YOLO \u{2014} Allow everything without asking"
        );
    }

    // -----------------------------------------------------------------------
    // Default mode: check logic
    // -----------------------------------------------------------------------

    #[test]
    fn default_mode_allows_read_only_tools() {
        let pm = PermissionManager::default();
        for tool in READ_ONLY_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Allow,
                "read-only tool {tool} should be allowed in default mode"
            );
        }
    }

    #[test]
    fn default_mode_asks_for_write_tools() {
        let pm = PermissionManager::default();
        for tool in WRITE_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Ask,
                "write tool {tool} should ask in default mode"
            );
        }
    }

    #[test]
    fn default_mode_asks_for_bash_tools() {
        let pm = PermissionManager::default();
        for tool in BASH_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Ask,
                "bash tool {tool} should ask in default mode"
            );
        }
    }

    #[test]
    fn default_mode_asks_for_unknown_tools() {
        let pm = PermissionManager::default();
        assert_eq!(pm.check("some_custom_tool", None), PermissionDecision::Ask);
    }

    // -----------------------------------------------------------------------
    // DontAsk mode
    // -----------------------------------------------------------------------

    #[test]
    fn dont_ask_mode_allows_everything() {
        let pm = PermissionManager::new(PermissionMode::DontAsk);
        assert_eq!(pm.check("vfs_write", None), PermissionDecision::Allow);
        assert_eq!(pm.check("bash", None), PermissionDecision::Allow);
        assert_eq!(pm.check("file_read", None), PermissionDecision::Allow);
        assert_eq!(pm.check("unknown_tool", None), PermissionDecision::Allow);
    }

    // -----------------------------------------------------------------------
    // AcceptEdits mode
    // -----------------------------------------------------------------------

    #[test]
    fn accept_edits_allows_reads() {
        let pm = PermissionManager::new(PermissionMode::AcceptEdits);
        for tool in READ_ONLY_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Allow,
                "read-only tool {tool} should be allowed in acceptEdits"
            );
        }
    }

    #[test]
    fn accept_edits_allows_writes() {
        let pm = PermissionManager::new(PermissionMode::AcceptEdits);
        for tool in WRITE_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Allow,
                "write tool {tool} should be allowed in acceptEdits"
            );
        }
    }

    #[test]
    fn accept_edits_asks_for_bash() {
        let pm = PermissionManager::new(PermissionMode::AcceptEdits);
        for tool in BASH_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Ask,
                "bash tool {tool} should ask in acceptEdits"
            );
        }
    }

    #[test]
    fn accept_edits_allows_unknown() {
        let pm = PermissionManager::new(PermissionMode::AcceptEdits);
        assert_eq!(
            pm.check("some_plugin_tool", None),
            PermissionDecision::Allow
        );
    }

    // -----------------------------------------------------------------------
    // Plan mode
    // -----------------------------------------------------------------------

    #[test]
    fn plan_mode_allows_reads() {
        let pm = PermissionManager::new(PermissionMode::Plan);
        for tool in READ_ONLY_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Allow,
                "read-only tool {tool} should be allowed in plan mode"
            );
        }
    }

    #[test]
    fn plan_mode_denies_writes() {
        let pm = PermissionManager::new(PermissionMode::Plan);
        for tool in WRITE_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Deny,
                "write tool {tool} should be denied in plan mode"
            );
        }
    }

    #[test]
    fn plan_mode_denies_bash() {
        let pm = PermissionManager::new(PermissionMode::Plan);
        for tool in BASH_TOOLS {
            assert_eq!(
                pm.check(tool, None),
                PermissionDecision::Deny,
                "bash tool {tool} should be denied in plan mode"
            );
        }
    }

    #[test]
    fn plan_mode_asks_for_unknown() {
        let pm = PermissionManager::new(PermissionMode::Plan);
        assert_eq!(pm.check("custom_tool", None), PermissionDecision::Ask);
    }

    // -----------------------------------------------------------------------
    // Rules
    // -----------------------------------------------------------------------

    #[test]
    fn add_rule_overrides_mode() {
        let pm = PermissionManager::default();
        // bash would normally Ask in default mode
        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        assert_eq!(pm.check("bash", None), PermissionDecision::Allow);
    }

    #[test]
    fn add_rule_replaces_existing() {
        let pm = PermissionManager::default();
        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        assert_eq!(pm.get_rules().len(), 1);

        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            policy: PermissionDecision::Deny,
        });
        assert_eq!(pm.get_rules().len(), 1);
        assert_eq!(pm.check("bash", None), PermissionDecision::Deny);
    }

    #[test]
    fn remove_rule() {
        let pm = PermissionManager::default();
        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        let (pm, removed) = pm.remove_rule("bash");
        assert!(removed);
        assert!(pm.get_rules().is_empty());
        // Falls back to mode-based
        assert_eq!(pm.check("bash", None), PermissionDecision::Ask);
    }

    #[test]
    fn remove_rule_nonexistent_returns_false() {
        let pm = PermissionManager::default();
        let (_pm, removed) = pm.remove_rule("nonexistent");
        assert!(!removed);
    }

    #[test]
    fn get_rules_returns_all() {
        let pm = PermissionManager::default();
        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        let pm = pm.add_rule(PermissionRule {
            tool: "vfs_write".to_string(),
            pattern: None,
            policy: PermissionDecision::Deny,
        });
        assert_eq!(pm.get_rules().len(), 2);
    }

    // -----------------------------------------------------------------------
    // Glob matching
    // -----------------------------------------------------------------------

    #[test]
    fn glob_star_matches() {
        let pm = PermissionManager::default();
        let pm = pm.add_rule(PermissionRule {
            tool: "vfs_*".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        assert_eq!(pm.check("vfs_write", None), PermissionDecision::Allow);
        assert_eq!(pm.check("vfs_read", None), PermissionDecision::Allow);
        assert_eq!(pm.check("vfs_delete", None), PermissionDecision::Allow);
    }

    #[test]
    fn glob_question_mark_matches() {
        let pm = PermissionManager::default();
        let pm = pm.add_rule(PermissionRule {
            tool: "bas?".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        assert_eq!(pm.check("bash", None), PermissionDecision::Allow);
        assert_eq!(pm.check("bass", None), PermissionDecision::Allow);
        // "ba" should not match glob (? requires exactly one character), falls to mode-based
        assert_eq!(pm.check("ba", None), PermissionDecision::Ask); // unknown tool in Default mode
    }

    #[test]
    fn glob_does_not_partial_match() {
        let pm = PermissionManager::default();
        let pm = pm.add_rule(PermissionRule {
            tool: "vfs".to_string(),
            pattern: None,
            policy: PermissionDecision::Deny,
        });
        // "vfs_write" should NOT match the exact "vfs" rule (no glob)
        // Falls back to mode-based (Ask for write tools in default mode)
        assert_eq!(pm.check("vfs_write", None), PermissionDecision::Ask);
    }

    #[test]
    fn glob_match_all() {
        let pm = PermissionManager::new(PermissionMode::Plan);
        let pm = pm.add_rule(PermissionRule {
            tool: "*".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        // Everything matches the wildcard rule
        assert_eq!(pm.check("bash", None), PermissionDecision::Allow);
        assert_eq!(pm.check("vfs_write", None), PermissionDecision::Allow);
        assert_eq!(pm.check("anything", None), PermissionDecision::Allow);
    }

    // -----------------------------------------------------------------------
    // Rule priority over mode
    // -----------------------------------------------------------------------

    #[test]
    fn rule_overrides_plan_mode_deny() {
        let pm = PermissionManager::new(PermissionMode::Plan);
        // Plan mode denies bash, but a rule can override
        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        assert_eq!(pm.check("bash", None), PermissionDecision::Allow);
    }

    #[test]
    fn rule_overrides_dont_ask_mode_allow() {
        let pm = PermissionManager::new(PermissionMode::DontAsk);
        // DontAsk allows everything, but a rule can deny
        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            policy: PermissionDecision::Deny,
        });
        assert_eq!(pm.check("bash", None), PermissionDecision::Deny);
    }

    // -----------------------------------------------------------------------
    // Serialization / deserialization
    // -----------------------------------------------------------------------

    #[test]
    fn save_and_load_roundtrip() {
        let pm = PermissionManager::new(PermissionMode::AcceptEdits);
        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: Some("npm *".to_string()),
            policy: PermissionDecision::Allow,
        });
        let pm = pm.add_rule(PermissionRule {
            tool: "vfs_*".to_string(),
            pattern: None,
            policy: PermissionDecision::Deny,
        });

        let json = pm.save().expect("save should succeed");

        let pm2 = PermissionManager::default();
        let pm2 = pm2.load(&json).expect("load should succeed");

        assert_eq!(pm2.get_mode(), PermissionMode::AcceptEdits);
        assert_eq!(pm2.get_rules().len(), 2);
        assert_eq!(pm2.get_rules()[0].tool, "bash");
        assert_eq!(pm2.get_rules()[0].pattern.as_deref(), Some("npm *"));
        assert_eq!(pm2.get_rules()[0].policy, PermissionDecision::Allow);
        assert_eq!(pm2.get_rules()[1].tool, "vfs_*");
        assert_eq!(pm2.get_rules()[1].policy, PermissionDecision::Deny);
    }

    #[test]
    fn load_replaces_existing_state() {
        let pm = PermissionManager::new(PermissionMode::DontAsk);
        let pm = pm.add_rule(PermissionRule {
            tool: "old_rule".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });

        let json =
            r#"{"mode":"Plan","rules":[{"tool":"new_rule","pattern":null,"policy":"Deny"}]}"#;
        let pm = pm.load(json).expect("load should succeed");

        assert_eq!(pm.get_mode(), PermissionMode::Plan);
        assert_eq!(pm.get_rules().len(), 1);
        assert_eq!(pm.get_rules()[0].tool, "new_rule");
    }

    #[test]
    fn load_invalid_json_returns_error() {
        let pm = PermissionManager::default();
        let result = pm.load("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn save_empty_rules() {
        let pm = PermissionManager::new(PermissionMode::Default);
        let json = pm.save().expect("save should succeed");
        assert!(json.contains("\"mode\""));
        assert!(json.contains("\"rules\""));
        assert!(json.contains("[]"));
    }

    // -----------------------------------------------------------------------
    // Glob internals
    // -----------------------------------------------------------------------

    #[test]
    fn glob_to_regex_escapes_dots() {
        let re = glob_to_regex("file.txt");
        assert_eq!(re, r"^file\.txt$");
    }

    #[test]
    fn glob_to_regex_star_and_question() {
        let re = glob_to_regex("vfs_*_?.log");
        assert_eq!(re, r"^vfs_.*_.\.log$");
    }

    #[test]
    fn match_glob_exact() {
        assert!(match_glob("bash", "bash"));
        assert!(!match_glob("bash", "basher"));
    }

    #[test]
    fn match_glob_star_prefix() {
        assert!(match_glob("*_write", "vfs_write"));
        assert!(match_glob("*_write", "file_write"));
        assert!(!match_glob("*_write", "vfs_read"));
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn check_with_args_param() {
        // args is currently unused but should not cause errors
        let pm = PermissionManager::default();
        assert_eq!(
            pm.check("file_read", Some(r#"{"path":"test.txt"}"#)),
            PermissionDecision::Allow
        );
    }

    #[test]
    fn multiple_rules_first_match_wins() {
        let pm = PermissionManager::default();
        let pm = pm.add_rule(PermissionRule {
            tool: "vfs_*".to_string(),
            pattern: None,
            policy: PermissionDecision::Deny,
        });
        let pm = pm.add_rule(PermissionRule {
            tool: "vfs_read".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        // "vfs_*" is first in the rules list, so it should match first
        assert_eq!(pm.check("vfs_read", None), PermissionDecision::Deny);
    }

    #[test]
    fn exact_rule_match_before_glob() {
        let pm = PermissionManager::default();
        // Add exact rule first
        let pm = pm.add_rule(PermissionRule {
            tool: "bash".to_string(),
            pattern: None,
            policy: PermissionDecision::Allow,
        });
        // Add glob rule second
        let pm = pm.add_rule(PermissionRule {
            tool: "bas*".to_string(),
            pattern: None,
            policy: PermissionDecision::Deny,
        });
        // Exact match "bash" == "bash" is checked first via iteration order
        assert_eq!(pm.check("bash", None), PermissionDecision::Allow);
    }
}
