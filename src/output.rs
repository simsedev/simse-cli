//! Output item rendering: converts OutputItem variants to ratatui Lines.

use crate::markdown::render_markdown;
use crate::ui_core::app::{OutputItem, ToolCallState, ToolCallStatus};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Fallback render width used when no real terminal width is available
/// (e.g. unit tests calling `render_output_item` directly).
const DEFAULT_RENDER_WIDTH: u16 = 80;

/// Convert output items to renderable Lines.
///
/// `width` is the content-area width and is threaded through to the markdown
/// renderer for assistant messages (horizontal-rule length, code-block borders,
/// table column sizing).
pub fn render_output_items(items: &[OutputItem], width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for item in items {
        lines.extend(render_output_item(item, width));
        lines.push(Line::default()); // spacing between items
    }
    lines
}

/// Convert a single OutputItem to Lines.
pub fn render_output_item(item: &OutputItem, width: u16) -> Vec<Line<'static>> {
    match item {
        OutputItem::Message { role, text } => render_message(role, text, width),
        OutputItem::ToolCall(tc) => render_tool_call(tc),
        OutputItem::CommandResult { text } => render_command_result(text),
        OutputItem::Error { message } => render_error(message),
        OutputItem::Info { text } => render_info(text),
    }
}

/// Render a user or assistant message.
///
/// `width` is the content-area width, used by the markdown renderer for
/// assistant messages.
fn render_message(role: &str, text: &str, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if role == "user" {
        let text_lines: Vec<&str> = text.lines().collect();
        for (i, line) in text_lines.iter().enumerate() {
            if i == 0 {
                lines.push(Line::from(vec![
                    Span::styled(
                        "\u{276f} ",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(line.to_string()),
                ]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::raw(line.to_string()),
                ]));
            }
        }
    } else {
        // Assistant or other roles: render the message body as markdown.
        let render_width = if width == 0 {
            DEFAULT_RENDER_WIDTH
        } else {
            width
        };
        lines.extend(render_markdown(text, render_width));
    }

    if lines.is_empty() {
        lines.push(Line::default());
    }

    lines
}

/// Format a tool call duration for display.
///
/// Displays as "45ms", "1.2s", or "1m30s".
fn format_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let minutes = ms / 60_000;
        let seconds = (ms % 60_000) / 1000;
        format!("{minutes}m{seconds}s")
    }
}

/// Render a tool call with status indicator.
fn render_tool_call(tc: &ToolCallState) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    let (status_color, status_char) = match tc.status {
        ToolCallStatus::Active => (Color::Yellow, "\u{23fa}"),
        ToolCallStatus::Completed => (Color::Green, "\u{2714}"),
        ToolCallStatus::Failed => (Color::Red, "\u{2718}"),
    };

    // First line: status icon + tool name
    let mut first_spans = vec![
        Span::styled(format!("{status_char} "), Style::default().fg(status_color)),
        Span::styled(
            tc.name.clone(),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    // Append duration if present.
    if let Some(ms) = tc.duration_ms {
        first_spans.push(Span::styled(
            format!(" ({})", format_duration_ms(ms)),
            Style::default().fg(Color::DarkGray),
        ));
    }

    lines.push(Line::from(first_spans));

    // Second line: summary or error.
    if let Some(ref error) = tc.error {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(error.clone(), Style::default().fg(Color::Red)),
        ]));
    } else if let Some(ref summary) = tc.summary {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(summary.clone(), Style::default().fg(Color::DarkGray)),
        ]));
    }

    // Diff lines if present.
    if let Some(ref diff) = tc.diff {
        for line in diff.lines() {
            let color = if line.starts_with('+') {
                Color::Green
            } else if line.starts_with('-') {
                Color::Red
            } else {
                Color::DarkGray
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(line.to_string(), Style::default().fg(color)),
            ]));
        }
    }

    lines
}

/// Render a command result as plain text lines.
fn render_command_result(text: &str) -> Vec<Line<'static>> {
    text.lines()
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect()
}

/// Render an error with red prefix.
fn render_error(message: &str) -> Vec<Line<'static>> {
    vec![Line::from(vec![
        Span::styled(
            "\u{2717} ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(message.to_string(), Style::default().fg(Color::Red)),
    ])]
}

/// Render info text in dim gray.
fn render_info(text: &str) -> Vec<Line<'static>> {
    text.lines()
        .map(|l| {
            Line::from(Span::styled(
                l.to_string(),
                Style::default().fg(Color::DarkGray),
            ))
        })
        .collect()
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_user_message_has_prefix() {
        let lines = render_output_item(
            &OutputItem::Message {
                role: "user".into(),
                text: "hello".into(),
            },
            80,
        );
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_error_is_nonempty() {
        let lines = render_output_item(
            &OutputItem::Error {
                message: "fail".into(),
            },
            80,
        );
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_tool_call_completed() {
        let tc = ToolCallState {
            id: "1".into(),
            name: "read_file".into(),
            args: r#"{"path": "test.rs"}"#.into(),
            status: ToolCallStatus::Completed,
            started_at: 0,
            duration_ms: Some(150),
            summary: Some("Read 42 lines".into()),
            error: None,
            diff: None,
        };
        let lines = render_output_item(&OutputItem::ToolCall(tc), 80);
        assert!(lines.len() >= 2);
    }

    #[test]
    fn render_info_is_nonempty() {
        let lines = render_output_item(
            &OutputItem::Info {
                text: "info msg".into(),
            },
            80,
        );
        assert!(!lines.is_empty());
    }

    #[test]
    fn render_tool_call_with_diff() {
        let tc = ToolCallState {
            id: "1".into(),
            name: "write_file".into(),
            args: "{}".into(),
            status: ToolCallStatus::Completed,
            started_at: 0,
            duration_ms: Some(50),
            summary: Some("Wrote file".into()),
            error: None,
            diff: Some("+added line\n-removed line\n context".into()),
        };
        let lines = render_output_item(&OutputItem::ToolCall(tc), 80);
        assert!(lines.len() >= 5); // name + summary + 3 diff lines
    }

    #[test]
    fn render_multiple_items() {
        let items = vec![
            OutputItem::Message {
                role: "user".into(),
                text: "hi".into(),
            },
            OutputItem::Info {
                text: "done".into(),
            },
        ];
        let lines = render_output_items(&items, 80);
        assert!(lines.len() >= 3); // at least 2 items + spacing
    }

    #[test]
    fn assistant_message_renders_markdown() {
        // An assistant message containing markdown should be rendered through
        // `render_markdown` — the literal `**` markers must not survive.
        let lines = render_output_item(
            &OutputItem::Message {
                role: "assistant".into(),
                text: "This is **bold** text.".into(),
            },
            80,
        );
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            !text.contains("**"),
            "markdown markers should be stripped, got: {text}"
        );
        assert!(text.contains("bold"), "content should survive: {text}");
        // The "bold" word should carry the BOLD modifier from the renderer.
        let bold_span = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .find(|s| s.content.contains("bold"));
        assert!(bold_span.is_some());
        assert!(
            bold_span
                .unwrap()
                .style
                .add_modifier
                .contains(Modifier::BOLD)
        );
    }

    #[test]
    fn assistant_heading_renders_markdown() {
        let lines = render_output_item(
            &OutputItem::Message {
                role: "assistant".into(),
                text: "# Title".into(),
            },
            80,
        );
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(
            !text.contains('#'),
            "heading marker should be stripped: {text}"
        );
        assert!(text.contains("Title"));
    }

    // ── format_duration_ms ────────────────────────

    #[test]
    fn format_duration_ms_milliseconds() {
        assert_eq!(format_duration_ms(0), "0ms");
        assert_eq!(format_duration_ms(42), "42ms");
        assert_eq!(format_duration_ms(999), "999ms");
    }

    #[test]
    fn format_duration_ms_seconds() {
        assert_eq!(format_duration_ms(1000), "1.0s");
        assert_eq!(format_duration_ms(1500), "1.5s");
        assert_eq!(format_duration_ms(59999), "60.0s");
    }

    #[test]
    fn format_duration_ms_minutes() {
        assert_eq!(format_duration_ms(60_000), "1m0s");
        assert_eq!(format_duration_ms(90_000), "1m30s");
    }

    #[test]
    fn render_tool_call_duration_formatted() {
        let tc = ToolCallState {
            id: "1".into(),
            name: "bash".into(),
            args: "{}".into(),
            status: ToolCallStatus::Completed,
            started_at: 0,
            duration_ms: Some(2500),
            summary: Some("Ran command".into()),
            error: None,
            diff: None,
        };
        let lines = render_output_item(&OutputItem::ToolCall(tc), 80);
        let text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(text.contains("2.5s"), "Expected '2.5s' in: {text}");
    }
}
