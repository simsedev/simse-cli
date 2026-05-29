//! Tool call box: rich display for tool invocations and their results.
//!
//! Renders tool calls with status indicators, arguments, summaries,
//! durations, and optional unified diffs. Supports three states:
//! active (spinning), completed (green check), and failed (red cross).
//!
//! # Display formats
//!
//! ```text
//! ✓ read_file("src/main.rs") Read 152 lines [42ms]
//! ✗ write_file("bad/path.rs") Permission denied
//! ⋯ search("pattern")
//! ```
//!
//! When a tool produces a diff, it is displayed below with `+`/`-` coloring:
//!
//! ```text
//! ✓ edit_file("lib.rs") Applied 2 changes [15ms]
//!   --- a/lib.rs
//!   +++ b/lib.rs
//!   @@ -10,3 +10,4 @@
//!    unchanged
//!   -removed
//!   +added
//! ```

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// ── Types ───────────────────────────────────────────────

/// Status of a tool call execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolCallStatus {
    /// Tool is currently executing (spinner shown).
    Active,
    /// Tool completed successfully with a duration in milliseconds.
    Completed { duration_ms: u64 },
    /// Tool failed with an error message.
    Failed { error: String },
}

/// All data needed to render a single tool call.
#[derive(Debug, Clone)]
pub struct ToolCallDisplay {
    /// Tool name (e.g. "read_file", "bash", "edit").
    pub name: String,
    /// Primary argument shown inline (e.g. the file path or command).
    pub primary_arg: Option<String>,
    /// Current execution status.
    pub status: ToolCallStatus,
    /// Short summary of the result (e.g. "Read 152 lines").
    pub summary: Option<String>,
    /// Unified diff output (shown below the status line).
    pub diff: Option<String>,
    /// Full arguments as JSON (shown only in verbose mode).
    pub args_json: Option<String>,
}

// ── Constants ───────────────────────────────────────────

/// Completed tool indicator.
const CHECK_MARK: &str = "\u{2713}";

/// Failed tool indicator.
const CROSS_MARK: &str = "\u{2717}";

/// Active/spinning tool indicator.
const SPINNER_CHAR: &str = "\u{22ef}";

// ── Rendering ───────────────────────────────────────────

/// Render a single tool call as one or more styled lines.
///
/// # Arguments
///
/// * `display` - The tool call data to render.
/// * `verbose` - If true, show the full arguments JSON below the status line.
///
/// # Returns
///
/// A vector of [`Line`]s representing the tool call display.
pub fn render_tool_call(display: &ToolCallDisplay, verbose: bool) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // ── Status line ─────────────────────────────────
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Indent.
    spans.push(Span::raw("  "));

    // Status icon.
    match &display.status {
        ToolCallStatus::Active => {
            spans.push(Span::styled(
                SPINNER_CHAR,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        ToolCallStatus::Completed { .. } => {
            spans.push(Span::styled(
                CHECK_MARK,
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        ToolCallStatus::Failed { .. } => {
            spans.push(Span::styled(
                CROSS_MARK,
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        }
    }

    spans.push(Span::raw(" "));

    // Tool name.
    let name_color = match &display.status {
        ToolCallStatus::Active => Color::Yellow,
        ToolCallStatus::Completed { .. } => Color::Green,
        ToolCallStatus::Failed { .. } => Color::Red,
    };
    spans.push(Span::styled(
        display.name.clone(),
        Style::default().fg(name_color).add_modifier(Modifier::BOLD),
    ));

    // Primary argument in parentheses.
    if let Some(ref arg) = display.primary_arg {
        spans.push(Span::styled(
            format!("(\"{}\")", truncate_arg(arg, 60)),
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Summary or error.
    match &display.status {
        ToolCallStatus::Completed { duration_ms } => {
            if let Some(ref summary) = display.summary {
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    summary.clone(),
                    Style::default().fg(Color::White),
                ));
            }
            spans.push(Span::styled(
                format!(" [{duration_ms}ms]"),
                Style::default().fg(Color::DarkGray),
            ));
        }
        ToolCallStatus::Failed { error } => {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(error.clone(), Style::default().fg(Color::Red)));
        }
        ToolCallStatus::Active => {
            // No extra info for active calls.
        }
    }

    lines.push(Line::from(spans));

    // ── Verbose: full args JSON ─────────────────────
    if verbose && let Some(ref json) = display.args_json {
        for json_line in json.lines() {
            lines.push(Line::from(Span::styled(
                format!("    {json_line}"),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    // ── Diff output ─────────────────────────────────
    if let Some(ref diff) = display.diff {
        lines.extend(render_diff_lines(diff));
    }

    lines
}

/// Render unified diff text as colored lines.
///
/// Lines starting with `+` are green, `-` are red, `@@` are cyan,
/// and everything else is dimmed. Each line is indented by 4 spaces.
///
/// # Arguments
///
/// * `diff` - A unified diff string.
///
/// # Returns
///
/// A vector of styled [`Line`]s.
pub fn render_diff_lines(diff: &str) -> Vec<Line<'static>> {
    diff.lines()
        .map(|line| {
            let (color, prefix_style) = if line.starts_with('+') {
                (Color::Green, None)
            } else if line.starts_with('-') {
                (Color::Red, None)
            } else if line.starts_with("@@") {
                (Color::Cyan, Some(Modifier::BOLD))
            } else {
                (Color::DarkGray, None)
            };

            let mut style = Style::default().fg(color);
            if let Some(modifier) = prefix_style {
                style = style.add_modifier(modifier);
            }

            Line::from(Span::styled(format!("    {line}"), style))
        })
        .collect()
}

// ── Helpers ─────────────────────────────────────────────

/// Truncate an argument string to a maximum length, adding `...` if truncated.
///
/// Path-aware: if the string contains `/`, the end (filename) is preserved
/// with a `...` prefix. Otherwise, the start is preserved with a `...` suffix.
fn truncate_arg(arg: &str, max_len: usize) -> String {
    if arg.chars().count() <= max_len {
        return arg.to_string();
    }
    if max_len <= 3 {
        return "...".to_string();
    }
    if arg.contains('/') {
        // Path: keep the end (filename is most important).
        let keep = max_len - 3;
        let tail: String = arg.chars().skip(arg.chars().count() - keep).collect();
        format!("...{tail}")
    } else {
        // Non-path: keep the start.
        let keep = max_len - 3;
        let head: String = arg.chars().take(keep).collect();
        format!("{head}...")
    }
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper to collect all text from lines ──────

    fn lines_to_text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|s| s.content.to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    // ── truncate_arg ───────────────────────────────

    #[test]
    fn truncate_arg_short_string_unchanged() {
        assert_eq!(truncate_arg("hello", 10), "hello");
    }

    #[test]
    fn truncate_arg_exact_length_unchanged() {
        assert_eq!(truncate_arg("hello", 5), "hello");
    }

    #[test]
    fn truncate_arg_long_non_path_keeps_start() {
        let result = truncate_arg("a very long argument string here", 15);
        assert!(result.ends_with("..."));
        assert!(result.starts_with("a very long "));
        assert!(result.len() <= 15);
    }

    #[test]
    fn truncate_arg_long_path_keeps_end() {
        let result = truncate_arg("/home/user/projects/simse/src/main.rs", 20);
        assert!(result.starts_with("..."));
        assert!(result.ends_with("main.rs"));
        assert!(result.len() <= 20);
    }

    #[test]
    fn truncate_arg_path_preserves_filename() {
        let result = truncate_arg(
            "/very/deep/nested/directory/structure/important_file.rs",
            25,
        );
        assert!(result.starts_with("..."));
        assert!(result.contains("important_file.rs"));
        assert!(result.len() <= 25);
    }

    #[test]
    fn truncate_arg_short_path_unchanged() {
        assert_eq!(truncate_arg("src/main.rs", 20), "src/main.rs");
    }

    #[test]
    fn truncate_arg_empty_string() {
        assert_eq!(truncate_arg("", 10), "");
    }

    #[test]
    fn truncate_arg_very_short_max() {
        assert_eq!(truncate_arg("a long string", 3), "...");
        assert_eq!(truncate_arg("/a/long/path.rs", 3), "...");
    }

    // ── render_tool_call: Active ───────────────────

    #[test]
    fn active_tool_shows_spinner() {
        let display = ToolCallDisplay {
            name: "search".into(),
            primary_arg: Some("pattern".into()),
            status: ToolCallStatus::Active,
            summary: None,
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        let text = lines_to_text(&lines);
        assert!(text.contains(SPINNER_CHAR));
        assert!(text.contains("search"));
        assert!(text.contains("pattern"));
    }

    #[test]
    fn active_tool_is_yellow() {
        let display = ToolCallDisplay {
            name: "bash".into(),
            primary_arg: None,
            status: ToolCallStatus::Active,
            summary: None,
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        // First line, find the icon span.
        let icon_span = &lines[0].spans[1]; // spans[0] = indent, spans[1] = icon
        assert_eq!(icon_span.style.fg, Some(Color::Yellow));
    }

    #[test]
    fn active_tool_single_line_no_diff() {
        let display = ToolCallDisplay {
            name: "read_file".into(),
            primary_arg: Some("main.rs".into()),
            status: ToolCallStatus::Active,
            summary: None,
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        assert_eq!(lines.len(), 1);
    }

    // ── render_tool_call: Completed ────────────────

    #[test]
    fn completed_tool_shows_check_mark() {
        let display = ToolCallDisplay {
            name: "read_file".into(),
            primary_arg: Some("src/main.rs".into()),
            status: ToolCallStatus::Completed { duration_ms: 42 },
            summary: Some("Read 152 lines".into()),
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        let text = lines_to_text(&lines);
        assert!(text.contains(CHECK_MARK));
        assert!(text.contains("read_file"));
        assert!(text.contains("src/main.rs"));
        assert!(text.contains("Read 152 lines"));
        assert!(text.contains("[42ms]"));
    }

    #[test]
    fn completed_tool_is_green() {
        let display = ToolCallDisplay {
            name: "write".into(),
            primary_arg: None,
            status: ToolCallStatus::Completed { duration_ms: 10 },
            summary: None,
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        let icon_span = &lines[0].spans[1];
        assert_eq!(icon_span.style.fg, Some(Color::Green));
    }

    #[test]
    fn completed_tool_without_summary() {
        let display = ToolCallDisplay {
            name: "bash".into(),
            primary_arg: Some("ls -la".into()),
            status: ToolCallStatus::Completed { duration_ms: 100 },
            summary: None,
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        let text = lines_to_text(&lines);
        assert!(text.contains("[100ms]"));
        assert!(text.contains("bash"));
    }

    // ── render_tool_call: Failed ───────────────────

    #[test]
    fn failed_tool_shows_cross_mark() {
        let display = ToolCallDisplay {
            name: "write_file".into(),
            primary_arg: Some("bad/path.rs".into()),
            status: ToolCallStatus::Failed {
                error: "Permission denied".into(),
            },
            summary: None,
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        let text = lines_to_text(&lines);
        assert!(text.contains(CROSS_MARK));
        assert!(text.contains("write_file"));
        assert!(text.contains("bad/path.rs"));
        assert!(text.contains("Permission denied"));
    }

    #[test]
    fn failed_tool_is_red() {
        let display = ToolCallDisplay {
            name: "delete".into(),
            primary_arg: None,
            status: ToolCallStatus::Failed {
                error: "Not found".into(),
            },
            summary: None,
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        let icon_span = &lines[0].spans[1];
        assert_eq!(icon_span.style.fg, Some(Color::Red));
    }

    // ── render_tool_call: verbose mode ─────────────

    #[test]
    fn verbose_mode_shows_args_json() {
        let display = ToolCallDisplay {
            name: "edit".into(),
            primary_arg: Some("lib.rs".into()),
            status: ToolCallStatus::Completed { duration_ms: 15 },
            summary: Some("Applied 2 changes".into()),
            diff: None,
            args_json: Some("{\n  \"file\": \"lib.rs\",\n  \"line\": 42\n}".into()),
        };
        let lines = render_tool_call(&display, true);
        let text = lines_to_text(&lines);
        assert!(text.contains("\"file\": \"lib.rs\""));
        assert!(text.contains("\"line\": 42"));
    }

    #[test]
    fn non_verbose_hides_args_json() {
        let display = ToolCallDisplay {
            name: "edit".into(),
            primary_arg: Some("lib.rs".into()),
            status: ToolCallStatus::Completed { duration_ms: 15 },
            summary: Some("Applied 2 changes".into()),
            diff: None,
            args_json: Some("{\n  \"file\": \"lib.rs\"\n}".into()),
        };
        let lines = render_tool_call(&display, false);
        let text = lines_to_text(&lines);
        assert!(!text.contains("\"file\""));
    }

    // ── render_tool_call: with diff ────────────────

    #[test]
    fn tool_call_with_diff_shows_colored_lines() {
        let diff = "--- a/lib.rs\n+++ b/lib.rs\n@@ -10,3 +10,4 @@\n unchanged\n-removed\n+added";
        let display = ToolCallDisplay {
            name: "edit_file".into(),
            primary_arg: Some("lib.rs".into()),
            status: ToolCallStatus::Completed { duration_ms: 15 },
            summary: Some("Applied 2 changes".into()),
            diff: Some(diff.into()),
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        // Status line + 6 diff lines.
        assert_eq!(lines.len(), 7);
    }

    // ── render_diff_lines ──────────────────────────

    #[test]
    fn diff_lines_added_are_green() {
        let lines = render_diff_lines("+new line");
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(span.style.fg, Some(Color::Green));
        assert!(span.content.contains("+new line"));
    }

    #[test]
    fn diff_lines_removed_are_red() {
        let lines = render_diff_lines("-old line");
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(span.style.fg, Some(Color::Red));
        assert!(span.content.contains("-old line"));
    }

    #[test]
    fn diff_lines_header_is_cyan_bold() {
        let lines = render_diff_lines("@@ -1,3 +1,4 @@");
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(span.style.fg, Some(Color::Cyan));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn diff_lines_context_is_dim() {
        let lines = render_diff_lines(" unchanged context line");
        assert_eq!(lines.len(), 1);
        let span = &lines[0].spans[0];
        assert_eq!(span.style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn diff_lines_multiline() {
        let diff = "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new";
        let lines = render_diff_lines(diff);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn diff_lines_empty_input() {
        let lines = render_diff_lines("");
        // Empty string produces no lines (Rust's str::lines() yields nothing for "").
        assert_eq!(lines.len(), 0);
    }

    #[test]
    fn diff_lines_are_indented() {
        let lines = render_diff_lines("+added");
        let content = &lines[0].spans[0].content;
        assert!(content.starts_with("    "));
    }

    // ── render_tool_call: no primary arg ───────────

    #[test]
    fn tool_call_without_primary_arg() {
        let display = ToolCallDisplay {
            name: "list_tools".into(),
            primary_arg: None,
            status: ToolCallStatus::Completed { duration_ms: 5 },
            summary: Some("12 tools".into()),
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        let text = lines_to_text(&lines);
        assert!(text.contains("list_tools"));
        assert!(!text.contains("(\""));
        assert!(text.contains("12 tools"));
    }

    // ── render_tool_call: long primary arg ─────────

    #[test]
    fn long_primary_arg_is_truncated() {
        let long_path = "a".repeat(100);
        let display = ToolCallDisplay {
            name: "read_file".into(),
            primary_arg: Some(long_path),
            status: ToolCallStatus::Active,
            summary: None,
            diff: None,
            args_json: None,
        };
        let lines = render_tool_call(&display, false);
        let text = lines_to_text(&lines);
        assert!(text.contains("..."));
    }

    // ── ToolCallStatus equality ────────────────────

    #[test]
    fn status_active_eq() {
        assert_eq!(ToolCallStatus::Active, ToolCallStatus::Active);
    }

    #[test]
    fn status_completed_eq() {
        assert_eq!(
            ToolCallStatus::Completed { duration_ms: 42 },
            ToolCallStatus::Completed { duration_ms: 42 }
        );
    }

    #[test]
    fn status_failed_eq() {
        assert_eq!(
            ToolCallStatus::Failed {
                error: "oops".into()
            },
            ToolCallStatus::Failed {
                error: "oops".into()
            }
        );
    }

    #[test]
    fn status_different_variants_ne() {
        assert_ne!(
            ToolCallStatus::Active,
            ToolCallStatus::Completed { duration_ms: 0 }
        );
    }

    // ── ToolCallDisplay clone ──────────────────────

    #[test]
    fn display_clone_is_independent() {
        let original = ToolCallDisplay {
            name: "test".into(),
            primary_arg: Some("arg".into()),
            status: ToolCallStatus::Active,
            summary: None,
            diff: None,
            args_json: None,
        };
        let cloned = original.clone();
        assert_eq!(cloned.name, "test");
        assert_eq!(cloned.primary_arg, Some("arg".into()));
    }
}
