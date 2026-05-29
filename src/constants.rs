//! Named constants for the CLI module.

/// Interval between UI tick events (spinner animation, tool call elapsed time).
pub const TICK_INTERVAL_MS: u64 = 120;

/// Maximum number of entries in the input history ring buffer.
pub const MAX_HISTORY_SIZE: usize = 100;

/// Agentic-turn ceiling. There is no real turn cap: the loop ends when the
/// model stops emitting tool calls (natural task completion) and runaway is
/// bounded by the loop's doom-loop detector (`max_identical_tool_calls`),
/// not by a turn count. A fixed number only ever truncated legitimate long
/// builds mid-task, so this is set to the max — effectively uncapped.
pub const MAX_AGENTIC_TURNS: usize = usize::MAX;

/// Maximum Levenshtein distance for "did you mean?" command suggestions.
pub const TYPO_SUGGESTION_DISTANCE: usize = 2;

/// Timeout before Ctrl+C "pending" state resets (seconds).
pub const CTRL_C_TIMEOUT_SECS: u64 = 2;

/// Interval between auth token refresh attempts (seconds).
pub const TOKEN_REFRESH_INTERVAL_SECS: u64 = 600;

/// Maximum visible rows in the inline completions dropdown.
pub const MAX_VISIBLE_COMPLETIONS: u16 = 8;
