//! Error box: red-bordered error display with title, message, and optional details.
//!
//! Renders errors in a visually prominent format with a red border
//! and structured content. Designed for inline rendering within the
//! conversation output area.
//!
//! # Display format
//!
//! ```text
//! ┌─ Error ──────────────────────────────────┐
//! │  ✗ Connection failed                     │
//! │                                          │
//! │  Server at localhost:11434 is not         │
//! │  responding. Check that Ollama is         │
//! │  running.                                │
//! │                                          │
//! │  Details:                                │
//! │  ECONNREFUSED 127.0.0.1:11434            │
//! └──────────────────────────────────────────┘
//! ```

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// ── Types ───────────────────────────────────────────────

/// Data needed to render an error display box.
#[derive(Debug, Clone)]
pub struct ErrorDisplay {
    /// Short error title (e.g. "Connection failed", "Tool error").
    pub title: String,
    /// Human-readable error message explaining what went wrong.
    pub message: String,
    /// Optional technical details (e.g. stack trace, error code).
    pub details: Option<String>,
}

// ── Constants ───────────────────────────────────────────

/// Error indicator icon.
const ERROR_ICON: &str = "\u{2717}";

// ── Rendering ───────────────────────────────────────────

/// Render an error display as styled lines.
///
/// Returns a vector of [`Line`]s that can be embedded in a conversation
/// output area or any other ratatui container. The lines include:
///
/// 1. A red title line with error icon
/// 2. A blank separator
/// 3. The error message (wrapped across multiple lines if needed)
/// 4. Optional details section with a "Details:" header
///
/// # Arguments
///
/// * `display` - The error data to render.
///
/// # Returns
///
/// A vector of styled [`Line`]s.
pub fn render_error_box(display: &ErrorDisplay) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();

    // ── Title line ──────────────────────────────────
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {ERROR_ICON} "),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            display.title.clone(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
    ]));

    // ── Blank separator ─────────────────────────────
    lines.push(Line::from(""));

    // ── Message body ────────────────────────────────
    for msg_line in display.message.lines() {
        lines.push(Line::from(Span::styled(
            format!("  {msg_line}"),
            Style::default().fg(Color::White),
        )));
    }

    // ── Details section ─────────────────────────────
    if let Some(ref details) = display.details {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "  Details:",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        for detail_line in details.lines() {
            lines.push(Line::from(Span::styled(
                format!("  {detail_line}"),
                Style::default().fg(Color::DarkGray),
            )));
        }
    }

    lines
}

/// Render an error display as styled lines wrapped in a red border.
///
/// Similar to [`render_error_box`] but includes top/bottom border lines
/// with the error title in the top border. Suitable for standalone display.
///
/// # Arguments
///
/// * `display` - The error data to render.
/// * `width` - Available width for the box (including borders).
///
/// # Returns
///
/// A vector of styled [`Line`]s including border decorations.
pub fn render_error_box_bordered(display: &ErrorDisplay, width: u16) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let inner_width = (width as usize).saturating_sub(2);
    let border_style = Style::default().fg(Color::Red);

    // ── Top border with title ───────────────────────
    let title = format!(" {} ", display.title);
    let title_len = title.len();
    let remaining = inner_width.saturating_sub(title_len + 1); // +1 for leading ─
    let top = format!(
        "\u{250c}\u{2500}{title}{}\u{2510}",
        "\u{2500}".repeat(remaining)
    );
    lines.push(Line::from(Span::styled(top, border_style)));

    // ── Content lines ───────────────────────────────
    let content_lines = render_error_box(display);
    for content_line in &content_lines {
        let mut bordered_spans: Vec<Span<'static>> = Vec::new();
        bordered_spans.push(Span::styled("\u{2502}", border_style));
        bordered_spans.extend(content_line.spans.clone());

        // Pad to fill width, then add right border.
        let content_width: usize = content_line.spans.iter().map(|s| s.content.len()).sum();
        let padding = inner_width.saturating_sub(content_width);
        bordered_spans.push(Span::raw(" ".repeat(padding)));
        bordered_spans.push(Span::styled("\u{2502}", border_style));

        lines.push(Line::from(bordered_spans));
    }

    // ── Bottom border ───────────────────────────────
    let bottom = format!("\u{2514}{}\u{2518}", "\u{2500}".repeat(inner_width));
    lines.push(Line::from(Span::styled(bottom, border_style)));

    lines
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helper ─────────────────────────────────────

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

    // ── ErrorDisplay creation ──────────────────────

    #[test]
    fn error_display_clone() {
        let display = ErrorDisplay {
            title: "Test error".into(),
            message: "Something went wrong".into(),
            details: Some("stack trace".into()),
        };
        let cloned = display.clone();
        assert_eq!(cloned.title, "Test error");
        assert_eq!(cloned.message, "Something went wrong");
        assert_eq!(cloned.details, Some("stack trace".into()));
    }

    #[test]
    fn error_display_without_details() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "Failed".into(),
            details: None,
        };
        assert!(display.details.is_none());
    }

    // ── render_error_box ───────────────────────────

    #[test]
    fn error_box_contains_title() {
        let display = ErrorDisplay {
            title: "Connection failed".into(),
            message: "Server not responding".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        let text = lines_to_text(&lines);
        assert!(text.contains("Connection failed"));
    }

    #[test]
    fn error_box_contains_error_icon() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "Oops".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        let text = lines_to_text(&lines);
        assert!(text.contains(ERROR_ICON));
    }

    #[test]
    fn error_box_contains_message() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "Server at localhost:11434 is not responding".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        let text = lines_to_text(&lines);
        assert!(text.contains("Server at localhost:11434 is not responding"));
    }

    #[test]
    fn error_box_multiline_message() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "Line one\nLine two\nLine three".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        let text = lines_to_text(&lines);
        assert!(text.contains("Line one"));
        assert!(text.contains("Line two"));
        assert!(text.contains("Line three"));
    }

    #[test]
    fn error_box_with_details() {
        let display = ErrorDisplay {
            title: "Connection error".into(),
            message: "Cannot reach server".into(),
            details: Some("ECONNREFUSED 127.0.0.1:11434".into()),
        };
        let lines = render_error_box(&display);
        let text = lines_to_text(&lines);
        assert!(text.contains("Details:"));
        assert!(text.contains("ECONNREFUSED 127.0.0.1:11434"));
    }

    #[test]
    fn error_box_multiline_details() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "Failed".into(),
            details: Some("Line A\nLine B".into()),
        };
        let lines = render_error_box(&display);
        let text = lines_to_text(&lines);
        assert!(text.contains("Line A"));
        assert!(text.contains("Line B"));
    }

    #[test]
    fn error_box_without_details_omits_section() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "Simple error".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        let text = lines_to_text(&lines);
        assert!(!text.contains("Details:"));
    }

    #[test]
    fn error_box_title_is_red_bold() {
        let display = ErrorDisplay {
            title: "Fatal".into(),
            message: "Crash".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        // First line has the title spans.
        let title_span = &lines[0].spans[1]; // spans[0] = icon, spans[1] = title
        assert_eq!(title_span.style.fg, Some(Color::Red));
        assert!(title_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn error_box_icon_is_red_bold() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "msg".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        let icon_span = &lines[0].spans[0];
        assert_eq!(icon_span.style.fg, Some(Color::Red));
        assert!(icon_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn error_box_message_is_white() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "something".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        // Message starts at line index 2 (after title + blank).
        let msg_span = &lines[2].spans[0];
        assert_eq!(msg_span.style.fg, Some(Color::White));
    }

    // ── render_error_box line count ────────────────

    #[test]
    fn error_box_line_count_without_details() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "One line message".into(),
            details: None,
        };
        let lines = render_error_box(&display);
        // title + blank + message = 3 lines
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn error_box_line_count_with_details() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "message".into(),
            details: Some("detail".into()),
        };
        let lines = render_error_box(&display);
        // title + blank + message + blank + "Details:" + detail = 6 lines
        assert_eq!(lines.len(), 6);
    }

    // ── render_error_box_bordered ──────────────────

    #[test]
    fn bordered_box_has_top_and_bottom_borders() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "msg".into(),
            details: None,
        };
        let lines = render_error_box_bordered(&display, 50);
        let text = lines_to_text(&lines);
        // Top border starts with box-drawing char.
        assert!(text.contains('\u{250c}'));
        // Bottom border.
        assert!(text.contains('\u{2514}'));
    }

    #[test]
    fn bordered_box_contains_title_in_border() {
        let display = ErrorDisplay {
            title: "My Error".into(),
            message: "msg".into(),
            details: None,
        };
        let lines = render_error_box_bordered(&display, 50);
        let first_line: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert!(first_line.contains("My Error"));
    }

    #[test]
    fn bordered_box_has_side_borders() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "msg".into(),
            details: None,
        };
        let lines = render_error_box_bordered(&display, 50);
        // Content lines (not first/last) should have vertical borders.
        for line in &lines[1..lines.len() - 1] {
            let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
            assert!(text.starts_with('\u{2502}'));
            assert!(text.ends_with('\u{2502}'));
        }
    }

    #[test]
    fn bordered_box_border_is_red() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "msg".into(),
            details: None,
        };
        let lines = render_error_box_bordered(&display, 50);
        // Top border should be red.
        let top_span = &lines[0].spans[0];
        assert_eq!(top_span.style.fg, Some(Color::Red));
    }

    #[test]
    fn bordered_box_narrow_width_does_not_panic() {
        let display = ErrorDisplay {
            title: "Very Long Error Title Here".into(),
            message: "A long message that exceeds the box width".into(),
            details: Some("Extra details".into()),
        };
        let lines = render_error_box_bordered(&display, 10);
        assert!(!lines.is_empty());
    }

    #[test]
    fn bordered_box_zero_width_does_not_panic() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "msg".into(),
            details: None,
        };
        let lines = render_error_box_bordered(&display, 0);
        assert!(!lines.is_empty());
    }

    // ── Edge cases ─────────────────────────────────

    #[test]
    fn empty_title_and_message() {
        let display = ErrorDisplay {
            title: String::new(),
            message: String::new(),
            details: None,
        };
        let lines = render_error_box(&display);
        // Should still produce lines (title + blank + empty message).
        assert!(!lines.is_empty());
    }

    #[test]
    fn empty_details_string() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "msg".into(),
            details: Some(String::new()),
        };
        let lines = render_error_box(&display);
        let text = lines_to_text(&lines);
        // Should still show "Details:" header.
        assert!(text.contains("Details:"));
    }

    #[test]
    fn error_display_debug_format() {
        let display = ErrorDisplay {
            title: "Error".into(),
            message: "msg".into(),
            details: None,
        };
        // Verify Debug impl works.
        let debug = format!("{display:?}");
        assert!(debug.contains("ErrorDisplay"));
    }
}
