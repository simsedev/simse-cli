//! Platform-agnostic text input state machine.
//!
//! Manages cursor position, text selection, and word boundaries.
//! No rendering — just state transitions.

use serde::{Deserialize, Serialize};

/// Selection state: anchor + cursor defines the selected range.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InputState {
    pub value: String,
    pub cursor: usize,
    pub anchor: Option<usize>,
}

/// The selected range (start inclusive, end exclusive).
pub fn selection_range(state: &InputState) -> Option<(usize, usize)> {
    state.anchor.map(|anchor| {
        let start = anchor.min(state.cursor);
        let end = anchor.max(state.cursor);
        if start == end {
            return None;
        }
        Some((start, end))
    })?
}

/// Find the previous word boundary from a position.
///
/// `pos` must be a valid byte offset on a char boundary in `text`.
pub fn word_boundary_left(text: &str, pos: usize) -> usize {
    debug_assert!(pos == 0 || pos == text.len() || text.is_char_boundary(pos));
    let bytes = text.as_bytes();
    let mut i = pos;
    // Skip non-word chars left
    while i > 0 && !is_word_char(bytes[i - 1]) {
        i -= 1;
    }
    // Skip word chars left
    while i > 0 && is_word_char(bytes[i - 1]) {
        i -= 1;
    }
    i
}

/// Find the next word boundary from a position.
///
/// `pos` must be a valid byte offset on a char boundary in `text`.
pub fn word_boundary_right(text: &str, pos: usize) -> usize {
    debug_assert!(pos == 0 || pos == text.len() || text.is_char_boundary(pos));
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = pos;

    if i < len && !is_word_char(bytes[i]) {
        // Starting on non-word: skip non-word, then skip word
        while i < len && !is_word_char(bytes[i]) {
            i += 1;
        }
        while i < len && is_word_char(bytes[i]) {
            i += 1;
        }
    } else {
        // Starting on word char: skip to end of word
        while i < len && is_word_char(bytes[i]) {
            i += 1;
        }
    }
    i
}

fn is_word_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Byte offset of the previous character boundary (one char back).
fn prev_char_pos(s: &str, pos: usize) -> usize {
    s[..pos].char_indices().next_back().map_or(0, |(i, _)| i)
}

/// Byte offset just past the current character (one char forward).
fn next_char_pos(s: &str, pos: usize) -> usize {
    pos + s[pos..].chars().next().map_or(0, |c| c.len_utf8())
}

/// Insert text at cursor, replacing any selection.
pub fn insert(state: &InputState, text: &str) -> InputState {
    if let Some((start, end)) = selection_range(state) {
        let mut value = state.value[..start].to_string();
        value.push_str(text);
        value.push_str(&state.value[end..]);
        InputState {
            value,
            cursor: start + text.len(),
            anchor: None,
        }
    } else {
        let mut value = state.value[..state.cursor].to_string();
        value.push_str(text);
        value.push_str(&state.value[state.cursor..]);
        InputState {
            value,
            cursor: state.cursor + text.len(),
            anchor: None,
        }
    }
}

/// Delete the selection, or one character before cursor if no selection.
pub fn backspace(state: &InputState) -> InputState {
    if let Some((start, end)) = selection_range(state) {
        let mut value = state.value[..start].to_string();
        value.push_str(&state.value[end..]);
        InputState {
            value,
            cursor: start,
            anchor: None,
        }
    } else if state.cursor > 0 {
        let prev = prev_char_pos(&state.value, state.cursor);
        let mut value = state.value[..prev].to_string();
        value.push_str(&state.value[state.cursor..]);
        InputState {
            value,
            cursor: prev,
            anchor: None,
        }
    } else {
        state.clone()
    }
}

/// Delete the selection, or one character after cursor if no selection.
pub fn delete(state: &InputState) -> InputState {
    if let Some((start, end)) = selection_range(state) {
        let mut value = state.value[..start].to_string();
        value.push_str(&state.value[end..]);
        InputState {
            value,
            cursor: start,
            anchor: None,
        }
    } else if state.cursor < state.value.len() {
        let next = next_char_pos(&state.value, state.cursor);
        let mut value = state.value[..state.cursor].to_string();
        value.push_str(&state.value[next..]);
        InputState {
            value,
            cursor: state.cursor,
            anchor: None,
        }
    } else {
        state.clone()
    }
}

/// Select all text.
pub fn select_all(state: &InputState) -> InputState {
    if state.value.is_empty() {
        return state.clone();
    }
    InputState {
        value: state.value.clone(),
        cursor: state.value.len(),
        anchor: Some(0),
    }
}

/// Move cursor left, optionally extending selection.
pub fn move_left(state: &InputState, extend: bool) -> InputState {
    if extend {
        let new_cursor = if state.cursor > 0 {
            prev_char_pos(&state.value, state.cursor)
        } else {
            0
        };
        InputState {
            value: state.value.clone(),
            cursor: new_cursor,
            anchor: Some(state.anchor.unwrap_or(state.cursor)),
        }
    } else if state.anchor.is_some() {
        // Collapse selection to left edge
        let (start, _) = selection_range(state).unwrap_or((state.cursor, state.cursor));
        InputState {
            value: state.value.clone(),
            cursor: start,
            anchor: None,
        }
    } else {
        let new_cursor = if state.cursor > 0 {
            prev_char_pos(&state.value, state.cursor)
        } else {
            0
        };
        InputState {
            value: state.value.clone(),
            cursor: new_cursor,
            anchor: None,
        }
    }
}

/// Move cursor right, optionally extending selection.
pub fn move_right(state: &InputState, extend: bool) -> InputState {
    let max = state.value.len();
    if extend {
        let new_cursor = if state.cursor < max {
            next_char_pos(&state.value, state.cursor)
        } else {
            max
        };
        InputState {
            value: state.value.clone(),
            cursor: new_cursor,
            anchor: Some(state.anchor.unwrap_or(state.cursor)),
        }
    } else if state.anchor.is_some() {
        let (_, end) = selection_range(state).unwrap_or((state.cursor, state.cursor));
        InputState {
            value: state.value.clone(),
            cursor: end,
            anchor: None,
        }
    } else {
        let new_cursor = if state.cursor < max {
            next_char_pos(&state.value, state.cursor)
        } else {
            max
        };
        InputState {
            value: state.value.clone(),
            cursor: new_cursor,
            anchor: None,
        }
    }
}

/// Move cursor to Home (position 0).
pub fn move_home(state: &InputState, extend: bool) -> InputState {
    if extend {
        InputState {
            value: state.value.clone(),
            cursor: 0,
            anchor: Some(state.anchor.unwrap_or(state.cursor)),
        }
    } else {
        InputState {
            value: state.value.clone(),
            cursor: 0,
            anchor: None,
        }
    }
}

/// Move cursor to End.
pub fn move_end(state: &InputState, extend: bool) -> InputState {
    let end = state.value.len();
    if extend {
        InputState {
            value: state.value.clone(),
            cursor: end,
            anchor: Some(state.anchor.unwrap_or(state.cursor)),
        }
    } else {
        InputState {
            value: state.value.clone(),
            cursor: end,
            anchor: None,
        }
    }
}

/// Move cursor to previous word boundary.
pub fn move_word_left(state: &InputState, extend: bool) -> InputState {
    let target = word_boundary_left(&state.value, state.cursor);
    if extend {
        InputState {
            value: state.value.clone(),
            cursor: target,
            anchor: Some(state.anchor.unwrap_or(state.cursor)),
        }
    } else {
        InputState {
            value: state.value.clone(),
            cursor: target,
            anchor: None,
        }
    }
}

/// Move cursor to next word boundary.
pub fn move_word_right(state: &InputState, extend: bool) -> InputState {
    let target = word_boundary_right(&state.value, state.cursor);
    if extend {
        InputState {
            value: state.value.clone(),
            cursor: target,
            anchor: Some(state.anchor.unwrap_or(state.cursor)),
        }
    } else {
        InputState {
            value: state.value.clone(),
            cursor: target,
            anchor: None,
        }
    }
}

/// Delete word backward from cursor.
pub fn delete_word_back(state: &InputState) -> InputState {
    if let Some((start, end)) = selection_range(state) {
        let mut value = state.value[..start].to_string();
        value.push_str(&state.value[end..]);
        return InputState {
            value,
            cursor: start,
            anchor: None,
        };
    }
    let target = word_boundary_left(&state.value, state.cursor);
    let mut value = state.value[..target].to_string();
    value.push_str(&state.value[state.cursor..]);
    InputState {
        value,
        cursor: target,
        anchor: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_boundary_left_from_middle() {
        assert_eq!(word_boundary_left("hello world", 8), 6);
    }

    #[test]
    fn word_boundary_left_from_start_of_word() {
        assert_eq!(word_boundary_left("hello world", 6), 0);
    }

    #[test]
    fn word_boundary_left_at_zero() {
        assert_eq!(word_boundary_left("hello", 0), 0);
    }

    #[test]
    fn word_boundary_right_from_middle() {
        assert_eq!(word_boundary_right("hello world", 3), 5);
    }

    #[test]
    fn word_boundary_right_from_space() {
        assert_eq!(word_boundary_right("hello world", 5), 11);
    }

    #[test]
    fn word_boundary_right_at_end() {
        assert_eq!(word_boundary_right("hello", 5), 5);
    }

    #[test]
    fn insert_at_cursor() {
        let state = InputState {
            value: "hello".into(),
            cursor: 5,
            anchor: None,
        };
        let result = insert(&state, " world");
        assert_eq!(result.value, "hello world");
        assert_eq!(result.cursor, 11);
    }

    #[test]
    fn insert_replaces_selection() {
        let state = InputState {
            value: "hello world".into(),
            cursor: 11,
            anchor: Some(0),
        };
        let result = insert(&state, "x");
        assert_eq!(result.value, "x");
        assert_eq!(result.cursor, 1);
        assert!(result.anchor.is_none());
    }

    #[test]
    fn backspace_deletes_selection() {
        let state = InputState {
            value: "hello world".into(),
            cursor: 11,
            anchor: Some(0),
        };
        let result = backspace(&state);
        assert_eq!(result.value, "");
        assert_eq!(result.cursor, 0);
    }

    #[test]
    fn backspace_deletes_one_char() {
        let state = InputState {
            value: "hello".into(),
            cursor: 5,
            anchor: None,
        };
        let result = backspace(&state);
        assert_eq!(result.value, "hell");
        assert_eq!(result.cursor, 4);
    }

    #[test]
    fn select_all_sets_anchor_and_cursor() {
        let state = InputState {
            value: "hello".into(),
            cursor: 3,
            anchor: None,
        };
        let result = select_all(&state);
        assert_eq!(result.anchor, Some(0));
        assert_eq!(result.cursor, 5);
    }

    #[test]
    fn move_left_collapses_selection() {
        let state = InputState {
            value: "hello".into(),
            cursor: 5,
            anchor: Some(0),
        };
        let result = move_left(&state, false);
        assert_eq!(result.cursor, 0);
        assert!(result.anchor.is_none());
    }

    #[test]
    fn move_right_extends_selection() {
        let state = InputState {
            value: "hello".into(),
            cursor: 0,
            anchor: None,
        };
        let result = move_right(&state, true);
        assert_eq!(result.cursor, 1);
        assert_eq!(result.anchor, Some(0));
    }

    // ── Multi-byte UTF-8 tests ────────────────────────────

    #[test]
    fn backspace_multibyte_char() {
        // "café" — é is 2 bytes (C3 A9), cursor at end (byte 5)
        let state = InputState {
            value: "café".into(),
            cursor: 5,
            anchor: None,
        };
        let result = backspace(&state);
        assert_eq!(result.value, "caf");
        assert_eq!(result.cursor, 3);
    }

    #[test]
    fn delete_multibyte_char() {
        // cursor on é (byte 3), delete should remove the whole é
        let state = InputState {
            value: "café".into(),
            cursor: 3,
            anchor: None,
        };
        let result = delete(&state);
        assert_eq!(result.value, "caf");
        assert_eq!(result.cursor, 3);
    }

    #[test]
    fn move_left_over_multibyte() {
        // cursor at end of "café" (byte 5), move left lands on é start (byte 3)
        let state = InputState {
            value: "café".into(),
            cursor: 5,
            anchor: None,
        };
        let result = move_left(&state, false);
        assert_eq!(result.cursor, 3);
    }

    #[test]
    fn move_right_over_multibyte() {
        // cursor on é (byte 3), move right lands past é (byte 5)
        let state = InputState {
            value: "café".into(),
            cursor: 3,
            anchor: None,
        };
        let result = move_right(&state, false);
        assert_eq!(result.cursor, 5);
    }

    #[test]
    fn backspace_emoji() {
        // "hi👋" — 👋 is 4 bytes, cursor at end
        let state = InputState {
            value: "hi👋".into(),
            cursor: "hi👋".len(),
            anchor: None,
        };
        let result = backspace(&state);
        assert_eq!(result.value, "hi");
        assert_eq!(result.cursor, 2);
    }

    #[test]
    fn delete_emoji() {
        // cursor on 👋 (byte 2)
        let state = InputState {
            value: "hi👋".into(),
            cursor: 2,
            anchor: None,
        };
        let result = delete(&state);
        assert_eq!(result.value, "hi");
        assert_eq!(result.cursor, 2);
    }

    #[test]
    fn delete_word_back_from_end() {
        let state = InputState {
            value: "hello world".into(),
            cursor: 11,
            anchor: None,
        };
        let result = delete_word_back(&state);
        assert_eq!(result.value, "hello ");
        assert_eq!(result.cursor, 6);
    }
}
