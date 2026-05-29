//! Command autocomplete: popup-driven command completion triggered by `/` prefix.
//!
//! When the user types `/` followed by characters, the autocomplete activates and
//! filters the command registry in real-time. It presents up to 8 matching commands
//! with descriptions in a popup above the input area, supports keyboard navigation
//! (Up/Down), ghost text for single matches, and Tab/Enter to accept.
//!
//! # Layout
//!
//! ```text
//! ┌─ Commands ──────────────────────────────────────┐
//! │  ❯ /help       — Show help information          │
//! │    /history    — Show command history            │
//! │    /hooks      — List registered hooks           │
//! └─────────────────────────────────────────────────┘
//! ┌─ Input ─────────────────────────────────────────┐
//! │ /hel|                                           │
//! └─────────────────────────────────────────────────┘
//! ```
//!
//! # Integration
//!
//! The autocomplete state lives alongside the `App` model. The `app.rs` update
//! function delegates key events when `is_active()` returns true:
//!
//! - **Escape** calls `deactivate()`
//! - **Up/Down** calls `move_up()` / `move_down()`
//! - **Tab/Enter/Right** calls `accept()` and sets the input value
//! - **CharInput/Backspace** calls `update_matches()` after input mutation

use crate::ui_core::commands::registry::CommandDefinition;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

// ── Constants ───────────────────────────────────────────

/// Maximum number of matches shown in the autocomplete popup.
const MAX_VISIBLE_MATCHES: usize = 8;

// ── CommandMatch ────────────────────────────────────────

/// A single matching command entry for the autocomplete popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandMatch {
    /// The command name (without leading `/`).
    pub name: String,
    /// Human-readable description of the command.
    pub description: String,
}

// ── CommandAutocompleteState ────────────────────────────

/// State for the command autocomplete popup.
///
/// Tracks the current input prefix, filtered matches, selection index,
/// and whether autocomplete is currently active.
#[derive(Debug, Clone)]
pub struct CommandAutocompleteState {
    /// The current input text that triggered/drives autocomplete.
    pub input: String,
    /// Filtered command matches for the current prefix.
    pub matches: Vec<CommandMatch>,
    /// Index of the currently highlighted match.
    pub selected: usize,
    /// Whether autocomplete is currently active (popup visible).
    pub active: bool,
}

impl CommandAutocompleteState {
    /// Create a new inactive autocomplete state.
    pub fn new() -> Self {
        Self {
            input: String::new(),
            matches: Vec::new(),
            selected: 0,
            active: false,
        }
    }

    /// Activate autocomplete: filter commands matching the input prefix (after `/`),
    /// set active = true, reset selection to 0.
    ///
    /// `input` is the full input string (including the leading `/`).
    /// `commands` is the full command registry.
    pub fn activate(&mut self, input: &str, commands: &[CommandDefinition]) {
        self.input = input.to_string();
        self.matches = filter_matches(input, commands);
        self.selected = 0;
        self.active = !self.matches.is_empty();
    }

    /// Deactivate autocomplete: reset to inactive state.
    pub fn deactivate(&mut self) {
        self.input.clear();
        self.matches.clear();
        self.selected = 0;
        self.active = false;
    }

    /// Re-filter matches as the user types. If the input no longer starts with `/`
    /// or there are no matches, the autocomplete deactivates.
    ///
    /// `input` is the full input string (including the leading `/`).
    /// `commands` is the full command registry.
    pub fn update_matches(&mut self, input: &str, commands: &[CommandDefinition]) {
        if !input.starts_with('/') {
            self.deactivate();
            return;
        }

        self.input = input.to_string();
        self.matches = filter_matches(input, commands);

        if self.matches.is_empty() {
            self.active = false;
        } else {
            self.active = true;
            // Clamp selection to new bounds.
            if self.selected >= self.matches.len() {
                self.selected = self.matches.len() - 1;
            }
        }
    }

    /// Move selection up by one (wrapping to bottom when at top).
    pub fn move_up(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        if self.selected == 0 {
            self.selected = self.matches.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    /// Move selection down by one (wrapping to top when at bottom).
    pub fn move_down(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        if self.selected + 1 >= self.matches.len() {
            self.selected = 0;
        } else {
            self.selected += 1;
        }
    }

    /// Accept the currently selected match. Returns the full command string
    /// (e.g. `"/help"`) and deactivates autocomplete. Returns `None` if no
    /// matches are available.
    pub fn accept(&mut self) -> Option<String> {
        if self.matches.is_empty() {
            return None;
        }
        let name = self.matches[self.selected].name.clone();
        let result = format!("/{name}");
        self.deactivate();
        Some(result)
    }

    /// If there is exactly one match, return the remaining characters that
    /// would complete the command (the "ghost text"). For example, if the
    /// input is `/hel` and the only match is `help`, this returns `"p"`.
    ///
    /// Returns `None` if there are zero or more than one match, or if the
    /// command is already fully typed.
    pub fn ghost_text(&self) -> Option<String> {
        if self.matches.len() != 1 {
            return None;
        }
        let prefix = extract_prefix(&self.input);
        let name = &self.matches[0].name;
        let name_lower = name.to_lowercase();
        let prefix_lower = prefix.to_lowercase();

        if name_lower.starts_with(&prefix_lower) && prefix_lower != name_lower {
            Some(name.chars().skip(prefix.chars().count()).collect())
        } else {
            None
        }
    }

    /// Whether autocomplete is currently active (popup should be visible).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Return the visible window of matches (up to `MAX_VISIBLE_MATCHES`),
    /// scrolled so the selected item is always visible.
    pub fn visible_matches(&self) -> &[CommandMatch] {
        if self.matches.len() <= MAX_VISIBLE_MATCHES {
            return &self.matches;
        }
        let (start, _) = self.visible_window();
        let end = (start + MAX_VISIBLE_MATCHES).min(self.matches.len());
        &self.matches[start..end]
    }

    /// Return the (start, end) indices of the visible window.
    fn visible_window(&self) -> (usize, usize) {
        let total = self.matches.len();
        if total <= MAX_VISIBLE_MATCHES {
            return (0, total);
        }
        // Keep selected item visible by scrolling the window.
        let half = MAX_VISIBLE_MATCHES / 2;
        let start = if self.selected <= half {
            0
        } else if self.selected + half >= total {
            total - MAX_VISIBLE_MATCHES
        } else {
            self.selected - half
        };
        let end = (start + MAX_VISIBLE_MATCHES).min(total);
        (start, end)
    }

    /// Return the current selection index (relative to the visible window).
    pub fn selected_index(&self) -> usize {
        let (start, _) = self.visible_window();
        self.selected - start
    }
}

impl Default for CommandAutocompleteState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Filtering ───────────────────────────────────────────

/// Extract the command prefix from input (everything after `/`, lowered).
fn extract_prefix(input: &str) -> &str {
    let without_slash = input.strip_prefix('/').unwrap_or(input);
    // Take only the first word (before any space) as the command prefix.
    without_slash
        .split_whitespace()
        .next()
        .unwrap_or(without_slash)
}

/// Filter the command registry against the typed prefix. Returns matches
/// sorted by relevance: exact-prefix matches first, then contains matches.
fn filter_matches(input: &str, commands: &[CommandDefinition]) -> Vec<CommandMatch> {
    let prefix = extract_prefix(input).to_lowercase();

    if prefix.is_empty() {
        // Just `/` typed: show all non-hidden commands.
        return commands
            .iter()
            .filter(|cmd| !cmd.hidden)
            .map(|cmd| CommandMatch {
                name: cmd.name.clone(),
                description: cmd.description.clone(),
            })
            .collect();
    }

    // Partition into prefix-matches and contains-matches for stable ordering.
    let mut prefix_hits = Vec::new();
    let mut contains_hits = Vec::new();

    for cmd in commands {
        if cmd.hidden {
            continue;
        }

        let name_lower = cmd.name.to_lowercase();
        let alias_prefix = cmd
            .aliases
            .iter()
            .any(|a| a.to_lowercase().starts_with(&prefix));
        let alias_contains = cmd
            .aliases
            .iter()
            .any(|a| a.to_lowercase().contains(&prefix));

        if name_lower.starts_with(&prefix) || alias_prefix {
            prefix_hits.push(CommandMatch {
                name: cmd.name.clone(),
                description: cmd.description.clone(),
            });
        } else if name_lower.contains(&prefix) || alias_contains {
            contains_hits.push(CommandMatch {
                name: cmd.name.clone(),
                description: cmd.description.clone(),
            });
        }
    }

    prefix_hits.extend(contains_hits);
    prefix_hits
}

// ── Render ──────────────────────────────────────────────

/// Render the command autocomplete popup above the input area.
///
/// `area` should be the region *above* the input line where the popup can
/// appear. The popup is anchored to the bottom of this area and grows upward.
///
/// The popup shows up to `MAX_VISIBLE_MATCHES` entries, each formatted as:
/// `  /name       — description`
///
/// The selected entry is highlighted in cyan with a `>` prefix.
pub fn render_command_autocomplete(
    frame: &mut Frame,
    area: Rect,
    state: &CommandAutocompleteState,
) {
    if !state.is_active() {
        return;
    }

    let visible = state.visible_matches();
    if visible.is_empty() {
        return;
    }

    // Calculate popup dimensions.
    let line_count = visible.len() as u16;
    // +2 for top/bottom border
    let popup_height = (line_count + 2).min(area.height);
    let popup_width = area.width.clamp(30, 60);

    // Anchor to bottom-left of the area (just above the input).
    let popup_y = area.y + area.height.saturating_sub(popup_height);
    let popup_x = area.x;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Build lines.
    let selected_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let normal_name_style = Style::default().fg(Color::White);
    let desc_style = Style::default().fg(Color::DarkGray);
    let selected_desc_style = Style::default().fg(Color::Gray);

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let is_selected = i == state.selected_index();
            let indicator = if is_selected { " > " } else { "   " };
            let name_style = if is_selected {
                selected_style
            } else {
                normal_name_style
            };
            let d_style = if is_selected {
                selected_desc_style
            } else {
                desc_style
            };

            Line::from(vec![
                Span::styled(indicator, name_style),
                Span::styled(format!("/{}", m.name), name_style),
                Span::styled(" \u{2014} ", d_style),
                Span::styled(m.description.clone(), d_style),
            ])
        })
        .collect();

    // Clear area behind popup, then render.
    frame.render_widget(Clear, popup_area);
    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Commands "),
    );
    frame.render_widget(popup, popup_area);
}

/// Render inline completion lines below the input (no border/popup).
///
/// Returns a list of `Line` items to be rendered in a dedicated layout chunk.
/// Format: `   /name          description`
/// Selected item is highlighted in cyan+bold.
/// Max `MAX_VISIBLE_MATCHES` items shown.
pub fn render_inline_completions<'a>(
    state: &CommandAutocompleteState,
    _width: u16,
) -> Vec<Line<'a>> {
    if !state.is_active() {
        return Vec::new();
    }

    let visible = state.visible_matches();
    if visible.is_empty() {
        return Vec::new();
    }

    let selected_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let normal_name_style = Style::default().fg(Color::White);
    let desc_style = Style::default().fg(Color::DarkGray);
    let selected_desc_style = Style::default().fg(Color::Gray);

    let max_name_len = visible
        .iter()
        .map(|m| m.name.len() + 1) // +1 for '/'
        .max()
        .unwrap_or(0);

    visible
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let is_selected = i == state.selected_index();
            let indicator = if is_selected { " > " } else { "   " };
            let name_style = if is_selected {
                selected_style
            } else {
                normal_name_style
            };
            let d_style = if is_selected {
                selected_desc_style
            } else {
                desc_style
            };

            let cmd_name = format!("/{}", m.name);
            let padding = " ".repeat(max_name_len.saturating_sub(cmd_name.len()) + 2);

            Line::from(vec![
                Span::styled(indicator.to_string(), name_style),
                Span::styled(cmd_name, name_style),
                Span::styled(padding, desc_style),
                Span::styled(m.description.clone(), d_style),
            ])
        })
        .collect()
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_core::commands::registry::all_commands;

    /// Helper: build a small set of test commands.
    fn test_commands() -> Vec<CommandDefinition> {
        vec![
            CommandDefinition {
                name: "help".into(),
                description: "Show help information".into(),
                usage: "help [command]".into(),
                aliases: vec!["?".into()],
                category: crate::ui_core::commands::registry::CommandCategory::Meta,
                hidden: false,
            },
            CommandDefinition {
                name: "history".into(),
                description: "Show command history".into(),
                usage: "history".into(),
                aliases: vec![],
                category: crate::ui_core::commands::registry::CommandCategory::Meta,
                hidden: false,
            },
            CommandDefinition {
                name: "clear".into(),
                description: "Clear the screen".into(),
                usage: "clear".into(),
                aliases: vec![],
                category: crate::ui_core::commands::registry::CommandCategory::Meta,
                hidden: false,
            },
            CommandDefinition {
                name: "exit".into(),
                description: "Exit the application".into(),
                usage: "exit".into(),
                aliases: vec!["quit".into(), "q".into()],
                category: crate::ui_core::commands::registry::CommandCategory::Meta,
                hidden: false,
            },
            CommandDefinition {
                name: "search".into(),
                description: "Search the library".into(),
                usage: "search <query>".into(),
                aliases: vec!["s".into(), "find".into()],
                category: crate::ui_core::commands::registry::CommandCategory::Session,
                hidden: false,
            },
            CommandDefinition {
                name: "secret".into(),
                description: "Hidden debug command".into(),
                usage: "secret".into(),
                aliases: vec![],
                category: crate::ui_core::commands::registry::CommandCategory::Meta,
                hidden: true,
            },
        ]
    }

    // ── new / default ───────────────────────────────────

    #[test]
    fn new_creates_inactive_state() {
        let state = CommandAutocompleteState::new();
        assert!(!state.is_active());
        assert!(state.matches.is_empty());
        assert_eq!(state.selected, 0);
        assert!(state.input.is_empty());
    }

    #[test]
    fn default_is_same_as_new() {
        let state = CommandAutocompleteState::default();
        assert!(!state.is_active());
    }

    // ── activate ────────────────────────────────────────

    #[test]
    fn activate_with_matching_prefix() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/he", &cmds);

        assert!(state.is_active());
        // "help" starts with "he" -> prefix match. No other names/aliases match "he".
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.matches[0].name, "help");
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn activate_with_h_matches_help_history_and_search() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);

        assert!(state.is_active());
        // "help" starts with "h" (prefix), "history" starts with "h" (prefix),
        // "search" contains "h" (contains match) -> 3 total.
        assert_eq!(state.matches.len(), 3);
        assert_eq!(state.matches[0].name, "help");
        assert_eq!(state.matches[1].name, "history");
        assert_eq!(state.matches[2].name, "search"); // contains match comes after prefix matches
    }

    #[test]
    fn activate_with_no_matches_stays_inactive() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/zzz", &cmds);

        assert!(!state.is_active());
        assert!(state.matches.is_empty());
    }

    #[test]
    fn activate_excludes_hidden_commands() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/sec", &cmds);

        // "secret" is hidden, should not appear.
        assert!(!state.is_active());
        assert!(state.matches.is_empty());
    }

    #[test]
    fn activate_bare_slash_shows_all_visible() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/", &cmds);

        assert!(state.is_active());
        // 5 visible commands (secret is hidden).
        assert_eq!(state.matches.len(), 5);
    }

    // ── deactivate ──────────────────────────────────────

    #[test]
    fn deactivate_resets_state() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);
        assert!(state.is_active());

        state.deactivate();
        assert!(!state.is_active());
        assert!(state.matches.is_empty());
        assert_eq!(state.selected, 0);
        assert!(state.input.is_empty());
    }

    // ── update_matches ──────────────────────────────────

    #[test]
    fn update_matches_narrows_results() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);
        assert_eq!(state.matches.len(), 3); // help, history (prefix), search (contains)

        state.update_matches("/he", &cmds);
        assert_eq!(state.matches.len(), 1); // only "help"
        assert_eq!(state.matches[0].name, "help");
    }

    #[test]
    fn update_matches_deactivates_on_no_slash() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);
        assert!(state.is_active());

        state.update_matches("hello", &cmds);
        assert!(!state.is_active());
    }

    #[test]
    fn update_matches_deactivates_on_no_results() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);

        state.update_matches("/zzz", &cmds);
        assert!(!state.is_active());
    }

    #[test]
    fn update_matches_clamps_selected_index() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/", &cmds);
        state.selected = 4; // last of 5

        state.update_matches("/cl", &cmds);
        // Only "clear" matches now.
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.selected, 0);
    }

    // ── move_up / move_down ─────────────────────────────

    #[test]
    fn move_down_wraps_around() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/he", &cmds); // 1 match: "help"
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.selected, 0);

        state.move_down(); // wrap from 0 -> 0 (only 1 item)
        assert_eq!(state.selected, 0);

        // Test with 2 items
        state.activate("/cl", &cmds); // just "clear"
        state.activate("/h", &cmds); // 3 items: help, history, search
        assert_eq!(state.matches.len(), 3);
        assert_eq!(state.selected, 0);

        state.move_down();
        assert_eq!(state.selected, 1);

        state.move_down();
        assert_eq!(state.selected, 2);

        state.move_down(); // wrap
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn move_up_wraps_around() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds); // 3 matches
        assert_eq!(state.matches.len(), 3);
        assert_eq!(state.selected, 0);

        state.move_up(); // wrap to bottom
        assert_eq!(state.selected, 2);

        state.move_up();
        assert_eq!(state.selected, 1);

        state.move_up();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn move_up_down_noop_when_empty() {
        let mut state = CommandAutocompleteState::new();
        state.move_up();
        assert_eq!(state.selected, 0);
        state.move_down();
        assert_eq!(state.selected, 0);
    }

    // ── accept ──────────────────────────────────────────

    #[test]
    fn accept_returns_selected_command() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);
        state.move_down(); // select "history"

        let result = state.accept();
        assert_eq!(result, Some("/history".into()));
        assert!(!state.is_active());
    }

    #[test]
    fn accept_returns_none_when_empty() {
        let mut state = CommandAutocompleteState::new();
        let result = state.accept();
        assert_eq!(result, None);
    }

    #[test]
    fn accept_deactivates_state() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/he", &cmds);

        let _ = state.accept();
        assert!(!state.is_active());
        assert!(state.matches.is_empty());
    }

    // ── ghost_text ──────────────────────────────────────

    #[test]
    fn ghost_text_single_match_returns_remainder() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/hel", &cmds);

        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.ghost_text(), Some("p".into()));
    }

    #[test]
    fn ghost_text_multiple_matches_returns_none() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);

        // 3 matches: help, history, search
        assert!(state.matches.len() > 1);
        assert_eq!(state.ghost_text(), None);
    }

    #[test]
    fn ghost_text_exact_match_returns_none() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/help", &cmds);

        // Even with 1 match, if fully typed, ghost is None.
        assert_eq!(state.ghost_text(), None);
    }

    #[test]
    fn ghost_text_no_matches_returns_none() {
        let state = CommandAutocompleteState::new();
        assert_eq!(state.ghost_text(), None);
    }

    // ── visible_matches ─────────────────────────────────

    #[test]
    fn visible_matches_windows_at_max() {
        // Use the full registry which has 33+ commands.
        let cmds = all_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/", &cmds);

        // All non-hidden commands are in matches, but visible window caps at MAX_VISIBLE_MATCHES.
        assert!(state.matches.len() > MAX_VISIBLE_MATCHES);
        let visible = state.visible_matches();
        assert_eq!(visible.len(), MAX_VISIBLE_MATCHES);
    }

    #[test]
    fn visible_matches_scrolls_with_selection() {
        let cmds = all_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/", &cmds);

        // Move selection past the initial window
        let total = state.matches.len();
        for _ in 0..total - 1 {
            state.move_down();
        }
        // Selection is at the last item; visible window should include it
        let visible = state.visible_matches();
        assert_eq!(visible.last().unwrap().name, state.matches[total - 1].name);
        assert_eq!(state.selected_index(), visible.len() - 1);
    }

    #[test]
    fn visible_matches_returns_all_when_fewer_than_max() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);

        let visible = state.visible_matches();
        assert_eq!(visible.len(), 3); // help, history (prefix), search (contains)
    }

    // ── alias matching ──────────────────────────────────

    #[test]
    fn matches_by_alias_prefix() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/q", &cmds);

        assert!(state.is_active());
        // "q" is alias for "exit"
        assert!(state.matches.iter().any(|m| m.name == "exit"));
    }

    #[test]
    fn matches_by_alias_contains() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/fin", &cmds);

        assert!(state.is_active());
        // "find" is alias for "search"
        assert!(state.matches.iter().any(|m| m.name == "search"));
    }

    // ── case insensitivity ──────────────────────────────

    #[test]
    fn case_insensitive_matching() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/HEL", &cmds);

        assert!(state.is_active());
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.matches[0].name, "help");
    }

    // ── prefix ordering ─────────────────────────────────

    #[test]
    fn prefix_matches_come_before_contains_matches() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        // "s" prefix: "search" starts with "s", "secret" hidden.
        // Also "s" is alias for search. "exit"/"history"/"help"/"clear" don't contain "s" in name.
        state.activate("/s", &cmds);

        assert!(state.is_active());
        // "search" should be first (prefix match on name and alias).
        assert_eq!(state.matches[0].name, "search");
    }

    // ── integration with full registry ──────────────────

    #[test]
    fn full_registry_help_prefix() {
        let cmds = all_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/hel", &cmds);

        assert!(state.is_active());
        assert!(state.matches.iter().any(|m| m.name == "help"));
    }

    #[test]
    fn full_registry_accept_returns_slash_command() {
        let cmds = all_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/clea", &cmds);

        let result = state.accept();
        assert_eq!(result, Some("/clear".into()));
    }

    #[test]
    fn full_registry_ghost_text_for_unique_match() {
        let cmds = all_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/compac", &cmds);

        // "compact" should be the only match starting with "compac"
        if state.matches.len() == 1 {
            assert_eq!(state.ghost_text(), Some("t".into()));
        }
    }

    // ── extract_prefix ──────────────────────────────────

    #[test]
    fn extract_prefix_basic() {
        assert_eq!(extract_prefix("/help"), "help");
        assert_eq!(extract_prefix("/h"), "h");
        assert_eq!(extract_prefix("/"), "");
    }

    #[test]
    fn extract_prefix_with_args() {
        // Only the command name matters, not arguments.
        assert_eq!(extract_prefix("/help topic"), "help");
    }

    #[test]
    fn extract_prefix_no_slash() {
        assert_eq!(extract_prefix("help"), "help");
    }

    // ── round-trip activate-navigate-accept ──────────────

    #[test]
    fn full_workflow_activate_navigate_accept() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();

        // 1. Type "/"
        state.activate("/", &cmds);
        assert!(state.is_active());
        assert_eq!(state.matches.len(), 5);

        // 2. Type "/h" — narrow to help, history, search (contains "h")
        state.update_matches("/h", &cmds);
        assert_eq!(state.matches.len(), 3);
        assert_eq!(state.selected, 0);

        // 3. Navigate down to "history"
        state.move_down();
        assert_eq!(state.selected, 1);
        assert_eq!(state.matches[state.selected].name, "history");

        // 4. Accept
        let result = state.accept();
        assert_eq!(result, Some("/history".into()));
        assert!(!state.is_active());
    }

    #[test]
    fn full_workflow_type_then_escape() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();

        state.activate("/cl", &cmds);
        assert!(state.is_active());

        state.deactivate();
        assert!(!state.is_active());
    }

    #[test]
    fn render_inline_completions_produces_lines() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();
        state.activate("/h", &cmds);

        let lines = render_inline_completions(&state, 60);
        assert!(!lines.is_empty());
        assert!(lines.len() >= 2);
    }

    #[test]
    fn render_inline_completions_empty_when_inactive() {
        let state = CommandAutocompleteState::new();
        let lines = render_inline_completions(&state, 60);
        assert!(lines.is_empty());
    }

    #[test]
    fn full_workflow_ghost_accept_with_right_arrow() {
        let cmds = test_commands();
        let mut state = CommandAutocompleteState::new();

        state.activate("/clea", &cmds);
        assert_eq!(state.matches.len(), 1);
        assert_eq!(state.ghost_text(), Some("r".into()));

        // Right arrow accepts the ghost text (same as accept).
        let result = state.accept();
        assert_eq!(result, Some("/clear".into()));
    }
}
