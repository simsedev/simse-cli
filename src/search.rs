//! Conversation search: Ctrl+F search within the chat output.
//!
//! Provides a search state machine that scans `OutputItem` text content
//! for matching substrings and tracks the current match position.

use crate::ui_core::app::OutputItem;

/// State of the in-conversation search (Ctrl+F).
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    /// The current search query string.
    pub query: String,
    /// Matching positions: `(output_index, byte_offset)`. The offset is a
    /// byte index into the item's text (as returned by `str::find`).
    pub matches: Vec<(usize, usize)>,
    /// Index into `matches` for the currently highlighted match.
    pub current_match: usize,
    /// Whether the search bar is active.
    pub active: bool,
}

impl SearchState {
    /// Create a new inactive search state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Activate the search bar.
    pub fn open(&mut self) {
        self.active = true;
        self.query.clear();
        self.matches.clear();
        self.current_match = 0;
    }

    /// Close the search bar and reset state.
    pub fn close(&mut self) {
        self.active = false;
        self.query.clear();
        self.matches.clear();
        self.current_match = 0;
    }

    /// Append a character to the query.
    pub fn type_char(&mut self, c: char) {
        self.query.push(c);
    }

    /// Delete the last character of the query.
    pub fn backspace(&mut self) {
        self.query.pop();
    }

    /// Advance to the next match, wrapping around.
    pub fn next_match(&mut self) {
        if !self.matches.is_empty() {
            self.current_match = (self.current_match + 1) % self.matches.len();
        }
    }

    /// Go to the previous match, wrapping around.
    pub fn prev_match(&mut self) {
        if !self.matches.is_empty() {
            if self.current_match == 0 {
                self.current_match = self.matches.len() - 1;
            } else {
                self.current_match -= 1;
            }
        }
    }

    /// Scan the output items for matches against the current query.
    ///
    /// Populates `self.matches` with `(output_index, byte_offset)` pairs.
    /// Clamps `current_match` to the new match count.
    pub fn scan(&mut self, output: &[OutputItem]) {
        self.matches.clear();
        if self.query.is_empty() {
            self.current_match = 0;
            return;
        }
        let query_lower = self.query.to_lowercase();
        for (idx, item) in output.iter().enumerate() {
            let text = item_text(item);
            let text_lower = text.to_lowercase();
            let mut start = 0;
            while let Some(pos) = text_lower[start..].find(&query_lower) {
                self.matches.push((idx, start + pos));
                start += pos + query_lower.len();
            }
        }
        // Clamp current_match.
        if self.matches.is_empty() {
            self.current_match = 0;
        } else if self.current_match >= self.matches.len() {
            self.current_match = self.matches.len() - 1;
        }
    }

    /// Format the match counter display (e.g., "3/12").
    pub fn match_display(&self) -> String {
        if self.matches.is_empty() {
            if self.query.is_empty() {
                String::new()
            } else {
                "0/0".to_string()
            }
        } else {
            format!("{}/{}", self.current_match + 1, self.matches.len())
        }
    }

    /// Return the output index of the current match, if any.
    pub fn current_output_index(&self) -> Option<usize> {
        self.matches.get(self.current_match).map(|(idx, _)| *idx)
    }
}

/// Extract the searchable text from an `OutputItem`.
fn item_text(item: &OutputItem) -> String {
    match item {
        OutputItem::Message { text, .. } => text.clone(),
        OutputItem::ToolCall(tc) => {
            let mut s = tc.name.clone();
            if let Some(ref summary) = tc.summary {
                s.push(' ');
                s.push_str(summary);
            }
            if let Some(ref error) = tc.error {
                s.push(' ');
                s.push_str(error);
            }
            s
        }
        OutputItem::CommandResult { text } => text.clone(),
        OutputItem::Error { message } => message.clone(),
        OutputItem::Info { text } => text.clone(),
    }
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_core::app::{ToolCallState, ToolCallStatus};

    #[test]
    fn new_search_state_is_inactive() {
        let state = SearchState::new();
        assert!(!state.active);
        assert!(state.query.is_empty());
        assert!(state.matches.is_empty());
    }

    #[test]
    fn open_activates() {
        let mut state = SearchState::new();
        state.open();
        assert!(state.active);
    }

    #[test]
    fn close_deactivates() {
        let mut state = SearchState::new();
        state.open();
        state.type_char('h');
        state.close();
        assert!(!state.active);
        assert!(state.query.is_empty());
    }

    #[test]
    fn type_char_appends() {
        let mut state = SearchState::new();
        state.type_char('a');
        state.type_char('b');
        assert_eq!(state.query, "ab");
    }

    #[test]
    fn backspace_removes_last() {
        let mut state = SearchState::new();
        state.type_char('a');
        state.type_char('b');
        state.backspace();
        assert_eq!(state.query, "a");
    }

    #[test]
    fn backspace_on_empty_is_noop() {
        let mut state = SearchState::new();
        state.backspace();
        assert!(state.query.is_empty());
    }

    #[test]
    fn scan_finds_matches_in_messages() {
        let mut state = SearchState::new();
        state.query = "hello".into();
        let output = vec![
            OutputItem::Message {
                role: "user".into(),
                text: "hello world".into(),
            },
            OutputItem::Message {
                role: "assistant".into(),
                text: "goodbye world".into(),
            },
            OutputItem::Message {
                role: "user".into(),
                text: "say hello again".into(),
            },
        ];
        state.scan(&output);
        assert_eq!(state.matches.len(), 2);
        assert_eq!(state.matches[0], (0, 0));
        assert_eq!(state.matches[1], (2, 4));
    }

    #[test]
    fn scan_is_case_insensitive() {
        let mut state = SearchState::new();
        state.query = "Hello".into();
        let output = vec![OutputItem::Message {
            role: "user".into(),
            text: "HELLO world".into(),
        }];
        state.scan(&output);
        assert_eq!(state.matches.len(), 1);
    }

    #[test]
    fn scan_empty_query_clears_matches() {
        let mut state = SearchState::new();
        state.query = String::new();
        let output = vec![OutputItem::Message {
            role: "user".into(),
            text: "hello".into(),
        }];
        state.scan(&output);
        assert!(state.matches.is_empty());
    }

    #[test]
    fn scan_multiple_matches_in_same_item() {
        let mut state = SearchState::new();
        state.query = "ab".into();
        let output = vec![OutputItem::Message {
            role: "user".into(),
            text: "ab cd ab ef ab".into(),
        }];
        state.scan(&output);
        assert_eq!(state.matches.len(), 3);
        assert_eq!(state.matches[0], (0, 0));
        assert_eq!(state.matches[1], (0, 6));
        assert_eq!(state.matches[2], (0, 12));
    }

    #[test]
    fn next_match_wraps() {
        let mut state = SearchState::new();
        state.matches = vec![(0, 0), (1, 5), (2, 10)];
        state.current_match = 2;
        state.next_match();
        assert_eq!(state.current_match, 0);
    }

    #[test]
    fn prev_match_wraps() {
        let mut state = SearchState::new();
        state.matches = vec![(0, 0), (1, 5), (2, 10)];
        state.current_match = 0;
        state.prev_match();
        assert_eq!(state.current_match, 2);
    }

    #[test]
    fn next_match_on_empty_is_noop() {
        let mut state = SearchState::new();
        state.next_match();
        assert_eq!(state.current_match, 0);
    }

    #[test]
    fn prev_match_on_empty_is_noop() {
        let mut state = SearchState::new();
        state.prev_match();
        assert_eq!(state.current_match, 0);
    }

    #[test]
    fn match_display_format() {
        let mut state = SearchState::new();
        state.matches = vec![(0, 0), (1, 5)];
        state.current_match = 0;
        assert_eq!(state.match_display(), "1/2");
        state.current_match = 1;
        assert_eq!(state.match_display(), "2/2");
    }

    #[test]
    fn match_display_empty_query() {
        let state = SearchState::new();
        assert_eq!(state.match_display(), "");
    }

    #[test]
    fn match_display_no_matches() {
        let mut state = SearchState::new();
        state.query = "xyz".into();
        assert_eq!(state.match_display(), "0/0");
    }

    #[test]
    fn current_output_index_returns_some() {
        let mut state = SearchState::new();
        state.matches = vec![(3, 10), (5, 20)];
        state.current_match = 1;
        assert_eq!(state.current_output_index(), Some(5));
    }

    #[test]
    fn current_output_index_returns_none_when_empty() {
        let state = SearchState::new();
        assert_eq!(state.current_output_index(), None);
    }

    #[test]
    fn scan_clamps_current_match() {
        let mut state = SearchState::new();
        state.query = "hello".into();
        state.current_match = 10;
        let output = vec![OutputItem::Message {
            role: "user".into(),
            text: "hello".into(),
        }];
        state.scan(&output);
        assert_eq!(state.current_match, 0);
    }

    #[test]
    fn scan_searches_tool_calls() {
        let mut state = SearchState::new();
        state.query = "read_file".into();
        let output = vec![OutputItem::ToolCall(ToolCallState {
            id: "1".into(),
            name: "read_file".into(),
            args: "{}".into(),
            status: ToolCallStatus::Active,
            started_at: 0,
            duration_ms: None,
            summary: None,
            error: None,
            diff: None,
        })];
        state.scan(&output);
        assert_eq!(state.matches.len(), 1);
    }

    #[test]
    fn scan_searches_errors() {
        let mut state = SearchState::new();
        state.query = "fail".into();
        let output = vec![OutputItem::Error {
            message: "something failed".into(),
        }];
        state.scan(&output);
        assert_eq!(state.matches.len(), 1);
    }

    #[test]
    fn scan_searches_info() {
        let mut state = SearchState::new();
        state.query = "connected".into();
        let output = vec![OutputItem::Info {
            text: "Successfully connected".into(),
        }];
        state.scan(&output);
        assert_eq!(state.matches.len(), 1);
    }
}
