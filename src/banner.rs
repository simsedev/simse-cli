//! Welcome banner widget with mascot art, tips, and status.

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::App;

/// Render the welcome banner in the given area.
pub fn render_banner(frame: &mut Frame, area: Rect, app: &App) {
    // Split into left (30%) and right (70%) columns.
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    // ── Left column: mascot + server info ──────────
    let left_lines = build_left_column(app);
    let left_widget = Paragraph::new(left_lines);
    frame.render_widget(left_widget, cols[0]);

    // ── Right column: tips + status ────────────────
    let right_lines = build_right_column(app, cols[1].width);
    let right_widget = Paragraph::new(right_lines);
    frame.render_widget(right_widget, cols[1]);
}

fn build_left_column(app: &App) -> Vec<Line<'static>> {
    let cyan = Style::default().fg(Color::Cyan);
    let green = Style::default().fg(Color::Green);
    let dim = Style::default().fg(Color::DarkGray);

    let version = app.version.clone();
    let server = app.server_name.clone().unwrap_or_else(|| "server".into());
    let model = app.model_name.clone().unwrap_or_else(|| "default".into());

    vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  simse v{version}"),
            cyan.add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        // Mascot art: a stylized "s" shape
        Line::from(Span::styled("     \u{256d}\u{2500}\u{2500}\u{256e}", green)),
        Line::from(Span::styled("     \u{2570}\u{2500}\u{256e}\u{2502}", green)),
        Line::from(Span::styled("       \u{2570}\u{256f}", green)),
        Line::from(""),
        Line::from(Span::styled(format!("  {server}"), dim)),
        Line::from(Span::styled(format!("  {model}"), dim)),
    ]
}

fn build_right_column(app: &App, width: u16) -> Vec<Line<'static>> {
    let cyan = Style::default().fg(Color::Cyan);
    let dim = Style::default().fg(Color::DarkGray);
    let green = Style::default().fg(Color::Green);

    let sep_len = (width as usize).saturating_sub(2);
    let separator = "\u{2500}".repeat(sep_len);

    let status = if app.loop_status == crate::app::LoopStatus::Idle {
        "Ready."
    } else {
        "Working..."
    };

    vec![
        Line::from(""),
        Line::from(Span::styled(" Tips", cyan.add_modifier(Modifier::BOLD))),
        Line::from(""),
        Line::from(vec![
            Span::styled(" /help", green),
            Span::styled(" \u{2014} list commands", dim),
        ]),
        Line::from(vec![
            Span::styled(" /add ", green),
            Span::styled(" \u{2014} save a volume", dim),
        ]),
        Line::from(vec![
            Span::styled(" ?    ", green),
            Span::styled(" \u{2014} keyboard shortcuts", dim),
        ]),
        Line::from(""),
        Line::from(Span::styled(format!(" {separator}"), dim)),
        Line::from(""),
        Line::from(Span::styled(
            format!(" {status}"),
            Style::default().fg(Color::White),
        )),
    ]
}
