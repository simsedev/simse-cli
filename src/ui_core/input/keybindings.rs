//! Keybinding registry: maps key combos to labels/IDs with matching logic.
//!
//! This module is platform-agnostic — no I/O, no timing. It stores
//! keybinding definitions and provides combo matching. Actual event
//! handling and double-tap detection live in `simse-cli`.
//!
//! All mutating methods use owned-return (`self -> Self` or `self -> (Self, T)`)
//! for functional-style state transitions. Internal collections use `im::Vector`
//! for cheap cloning via structural sharing.

use im::Vector;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A key combination (e.g. Ctrl+C, Shift+Tab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyCombo {
    pub name: String,
    pub ctrl: bool,
    pub shift: bool,
    pub meta: bool,
}

impl KeyCombo {
    /// Create a plain key combo with no modifiers.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ctrl: false,
            shift: false,
            meta: false,
        }
    }

    /// Builder: set ctrl modifier.
    pub fn ctrl(mut self) -> Self {
        self.ctrl = true;
        self
    }

    /// Builder: set shift modifier.
    pub fn shift(mut self) -> Self {
        self.shift = true;
        self
    }

    /// Builder: set meta modifier.
    pub fn meta(mut self) -> Self {
        self.meta = true;
        self
    }
}

/// A registered keybinding entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingEntry {
    pub combo: KeyCombo,
    pub label: String,
    pub id: usize,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// Keybinding registry — stores entries and provides matching.
///
/// Handler execution and double-tap timing are handled by `simse-cli`,
/// not here. This crate only stores the registry and does matching.
///
/// Uses `im::Vector` for entries to enable cheap cloning and functional-style
/// state transitions. All mutating methods take `self` by value and return
/// the updated `Self` (owned-return pattern).
#[derive(Debug, Clone)]
pub struct KeybindingRegistry {
    entries: Vector<KeybindingEntry>,
    next_id: usize,
    /// Double-tap window in milliseconds (stored for consumers to read).
    pub double_tap_ms: u64,
}

impl Default for KeybindingRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl KeybindingRegistry {
    /// Create a new empty registry with default double-tap window (400ms).
    pub fn new() -> Self {
        Self {
            entries: Vector::new(),
            next_id: 1,
            double_tap_ms: 400,
        }
    }

    /// Create a new registry with a custom double-tap window.
    pub fn with_double_tap_ms(double_tap_ms: u64) -> Self {
        Self {
            entries: Vector::new(),
            next_id: 1,
            double_tap_ms,
        }
    }

    /// Register a keybinding. Returns `(updated_self, entry_id)`.
    pub fn register(mut self, combo: KeyCombo, label: impl Into<String>) -> (Self, usize) {
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push_back(KeybindingEntry {
            combo,
            label: label.into(),
            id,
        });
        (self, id)
    }

    /// Unregister a keybinding by ID. Returns the updated registry.
    pub fn unregister(mut self, id: usize) -> Self {
        self.entries = self.entries.into_iter().filter(|e| e.id != id).collect();
        self
    }

    /// Check if a combo matches an event.
    pub fn matches(combo: &KeyCombo, event: &KeyCombo) -> bool {
        combo.name == event.name
            && combo.ctrl == event.ctrl
            && combo.shift == event.shift
            && combo.meta == event.meta
    }

    /// Find the first matching entry for an event.
    pub fn find_match(&self, event: &KeyCombo) -> Option<&KeybindingEntry> {
        self.entries
            .iter()
            .find(|entry| Self::matches(&entry.combo, event))
    }

    /// List all registered keybindings.
    pub fn list(&self) -> &Vector<KeybindingEntry> {
        &self.entries
    }

    /// Format a combo as a human-readable string (e.g. "Ctrl+Shift+C").
    pub fn combo_to_string(combo: &KeyCombo) -> String {
        let mut parts: Vec<String> = Vec::new();
        if combo.ctrl {
            parts.push("Ctrl".into());
        }
        if combo.shift {
            parts.push("Shift".into());
        }
        if combo.meta {
            parts.push("Meta".into());
        }
        // Capitalize the first letter of the key name.
        let mut chars = combo.name.chars();
        let capitalized = match chars.next() {
            Some(first) => {
                let upper: String = first.to_uppercase().collect();
                upper + chars.as_str()
            }
            None => String::new(),
        };
        parts.push(capitalized);
        parts.join("+")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_returns_unique_ids() {
        let reg = KeybindingRegistry::new();
        let (reg, id1) = reg.register(KeyCombo::new("c").ctrl(), "Copy");
        let (reg, id2) = reg.register(KeyCombo::new("v").ctrl(), "Paste");
        assert_ne!(id1, id2);
        assert_eq!(reg.list().len(), 2);
    }

    #[test]
    fn unregister_removes_entry() {
        let reg = KeybindingRegistry::new();
        let (reg, id) = reg.register(KeyCombo::new("c").ctrl(), "Copy");
        assert_eq!(reg.list().len(), 1);
        let reg = reg.unregister(id);
        assert_eq!(reg.list().len(), 0);
    }

    #[test]
    fn unregister_nonexistent_is_noop() {
        let reg = KeybindingRegistry::new();
        let (reg, _) = reg.register(KeyCombo::new("c").ctrl(), "Copy");
        let reg = reg.unregister(9999);
        assert_eq!(reg.list().len(), 1);
    }

    #[test]
    fn matches_exact_combo() {
        let combo = KeyCombo::new("c").ctrl();
        let event = KeyCombo::new("c").ctrl();
        assert!(KeybindingRegistry::matches(&combo, &event));
    }

    #[test]
    fn matches_rejects_different_name() {
        let combo = KeyCombo::new("c").ctrl();
        let event = KeyCombo::new("v").ctrl();
        assert!(!KeybindingRegistry::matches(&combo, &event));
    }

    #[test]
    fn matches_rejects_missing_modifier() {
        let combo = KeyCombo::new("c").ctrl();
        let event = KeyCombo::new("c"); // no ctrl
        assert!(!KeybindingRegistry::matches(&combo, &event));
    }

    #[test]
    fn matches_rejects_extra_modifier() {
        let combo = KeyCombo::new("c").ctrl();
        let event = KeyCombo::new("c").ctrl().shift();
        assert!(!KeybindingRegistry::matches(&combo, &event));
    }

    #[test]
    fn matches_all_modifiers() {
        let combo = KeyCombo::new("c").ctrl().shift().meta();
        let event = KeyCombo::new("c").ctrl().shift().meta();
        assert!(KeybindingRegistry::matches(&combo, &event));
    }

    #[test]
    fn matches_no_modifiers() {
        let combo = KeyCombo::new("escape");
        let event = KeyCombo::new("escape");
        assert!(KeybindingRegistry::matches(&combo, &event));
    }

    #[test]
    fn find_match_returns_first_match() {
        let reg = KeybindingRegistry::new();
        let (reg, id1) = reg.register(KeyCombo::new("c").ctrl(), "Copy");
        let (reg, _id2) = reg.register(KeyCombo::new("v").ctrl(), "Paste");
        let event = KeyCombo::new("c").ctrl();
        let found = reg.find_match(&event);
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, id1);
        assert_eq!(found.unwrap().label, "Copy");
    }

    #[test]
    fn find_match_returns_none_when_no_match() {
        let reg = KeybindingRegistry::new();
        let (reg, _) = reg.register(KeyCombo::new("c").ctrl(), "Copy");
        let event = KeyCombo::new("x").ctrl();
        assert!(reg.find_match(&event).is_none());
    }

    #[test]
    fn list_returns_all_entries_in_order() {
        let reg = KeybindingRegistry::new();
        let (reg, _) = reg.register(KeyCombo::new("c").ctrl(), "Copy");
        let (reg, _) = reg.register(KeyCombo::new("v").ctrl(), "Paste");
        let (reg, _) = reg.register(KeyCombo::new("escape"), "Cancel");
        let entries = reg.list();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].label, "Copy");
        assert_eq!(entries[1].label, "Paste");
        assert_eq!(entries[2].label, "Cancel");
    }

    #[test]
    fn combo_to_string_plain_key() {
        let combo = KeyCombo::new("escape");
        assert_eq!(KeybindingRegistry::combo_to_string(&combo), "Escape");
    }

    #[test]
    fn combo_to_string_ctrl() {
        let combo = KeyCombo::new("c").ctrl();
        assert_eq!(KeybindingRegistry::combo_to_string(&combo), "Ctrl+C");
    }

    #[test]
    fn combo_to_string_ctrl_shift() {
        let combo = KeyCombo::new("z").ctrl().shift();
        assert_eq!(KeybindingRegistry::combo_to_string(&combo), "Ctrl+Shift+Z");
    }

    #[test]
    fn combo_to_string_all_modifiers() {
        let combo = KeyCombo::new("a").ctrl().shift().meta();
        assert_eq!(
            KeybindingRegistry::combo_to_string(&combo),
            "Ctrl+Shift+Meta+A"
        );
    }

    #[test]
    fn combo_to_string_meta_only() {
        let combo = KeyCombo::new("tab").meta();
        assert_eq!(KeybindingRegistry::combo_to_string(&combo), "Meta+Tab");
    }

    #[test]
    fn combo_to_string_shift_only() {
        let combo = KeyCombo::new("tab").shift();
        assert_eq!(KeybindingRegistry::combo_to_string(&combo), "Shift+Tab");
    }

    #[test]
    fn default_double_tap_ms() {
        let reg = KeybindingRegistry::new();
        assert_eq!(reg.double_tap_ms, 400);
    }

    #[test]
    fn custom_double_tap_ms() {
        let reg = KeybindingRegistry::with_double_tap_ms(200);
        assert_eq!(reg.double_tap_ms, 200);
    }

    #[test]
    fn default_trait_creates_empty_registry() {
        let reg = KeybindingRegistry::default();
        assert_eq!(reg.list().len(), 0);
        assert_eq!(reg.double_tap_ms, 400);
    }

    #[test]
    fn register_after_unregister_gives_new_id() {
        let reg = KeybindingRegistry::new();
        let (reg, id1) = reg.register(KeyCombo::new("c").ctrl(), "Copy");
        let reg = reg.unregister(id1);
        let (_, id2) = reg.register(KeyCombo::new("c").ctrl(), "Copy");
        assert!(id2 > id1);
    }

    #[test]
    fn multiple_bindings_same_combo() {
        let reg = KeybindingRegistry::new();
        let (reg, _) = reg.register(KeyCombo::new("escape"), "Cancel");
        let (reg, _) = reg.register(KeyCombo::new("escape"), "Close Dialog");
        // find_match returns the first one registered
        let event = KeyCombo::new("escape");
        let found = reg.find_match(&event);
        assert_eq!(found.unwrap().label, "Cancel");
    }

    #[test]
    fn entry_stores_combo_and_label() {
        let reg = KeybindingRegistry::new();
        let (reg, id) = reg.register(KeyCombo::new("o").ctrl(), "Open File");
        let entry = reg.list().iter().find(|e| e.id == id).unwrap();
        assert_eq!(entry.label, "Open File");
        assert_eq!(entry.combo.name, "o");
        assert!(entry.combo.ctrl);
        assert!(!entry.combo.shift);
        assert!(!entry.combo.meta);
    }

    #[test]
    fn combo_to_string_empty_name() {
        let combo = KeyCombo::new("");
        assert_eq!(KeybindingRegistry::combo_to_string(&combo), "");
    }
}
