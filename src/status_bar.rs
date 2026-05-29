//! Status bar: bottom-of-screen contextual information bar.
//!
//! Displays permission mode, server name, model name, loop status,
//! plan/verbose indicators, token count, context percentage, and
//! keyboard shortcut hints. Items are separated by `·` dots.
//!
//! # Layout
//!
//! ```text
//! auto-edit (shift+tab) · my-server · opus-4  esc to interrupt  plan · verbose  1.5k tokens · 42% ctx · ? shortcuts
//! ├── left ──────────────────────────────────┤├── center ──────┤├── right ─────────────────────────────────────────┤
//! ```

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

// ── State ───────────────────────────────────────────────

/// All data needed to render the status bar.
///
/// Fields are set by the app layer and passed to [`render_status_bar`].
#[derive(Debug, Clone, Default)]
pub struct StatusBarState {
    /// Current permission mode label (e.g. "auto-edit", "plan", "manual").
    pub permission_mode: String,
    /// Name of the connected server (e.g. "ollama", "anthropic").
    pub server_name: Option<String>,
    /// Name of the active model (e.g. "opus-4", "llama3").
    pub model_name: Option<String>,
    /// Whether the agentic loop is currently running.
    pub loop_active: bool,
    /// Whether plan mode is enabled.
    pub plan_mode: bool,
    /// Whether verbose output is enabled.
    pub verbose: bool,
    /// Total tokens consumed in the current session.
    pub token_count: u64,
    /// Context window usage as a percentage (0..=100).
    pub context_percent: u8,
    /// Working directory path (shown abbreviated as last 2 path components).
    pub work_dir: Option<String>,
    /// Git branch name (read from .git/HEAD at startup).
    pub git_branch: Option<String>,
    /// Session title (displayed when a session is resumed).
    pub session_title: Option<String>,
    /// Whether the remote tunnel is connected.
    pub remote_connected: bool,
}

// ── Rendering ───────────────────────────────────────────

/// Render the status bar into the given single-line area.
///
/// The bar is rendered as a single-line [`Paragraph`] with left-aligned
/// connection info, center indicators, and right-aligned stats.
///
/// # Arguments
///
/// * `frame` - The ratatui frame to render into.
/// * `area` - A single-row [`Rect`] for the status bar.
/// * `state` - Current status bar data.
pub fn render_status_bar(frame: &mut Frame, area: Rect, state: &StatusBarState) {
    let total_width = area.width as usize;
    let left = build_left_spans(state, total_width);
    let center = build_center_spans(state);
    let right = build_right_spans(state);

    let left_width = span_width(&left);
    let right_width = span_width(&right);
    let center_width = span_width(&center);

    // Assemble all spans with dynamic spacing.
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.extend(left);

    // Calculate gap between left and center, and center and right.
    let used = left_width + center_width + right_width;
    if total_width > used {
        let remaining = total_width - used;
        let gap_left = remaining / 2;
        let gap_right = remaining - gap_left;

        if !center.is_empty() {
            spans.push(Span::raw(" ".repeat(gap_left)));
            spans.extend(center);
            spans.push(Span::raw(" ".repeat(gap_right)));
        } else {
            spans.push(Span::raw(" ".repeat(remaining)));
        }
    } else if !center.is_empty() {
        spans.push(Span::raw("  "));
        spans.extend(center);
        spans.push(Span::raw("  "));
    }

    spans.extend(right);

    let line = Line::from(spans);
    let widget = Paragraph::new(line).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(widget, area);
}

/// Build the status bar content as a [`Line`] (for embedding or testing).
pub fn build_status_line(state: &StatusBarState) -> Line<'static> {
    let left = build_left_spans(state, usize::MAX);
    let center = build_center_spans(state);
    let right = build_right_spans(state);

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.extend(left);

    if !center.is_empty() {
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.extend(center);
    }

    if !right.is_empty() {
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.extend(right);
    }

    Line::from(spans)
}

// ── Span builders ───────────────────────────────────────

/// Abbreviate a path to its last 2 components (e.g., "/home/user/foo/bar" -> "foo/bar").
fn abbreviate_path(path: &str) -> String {
    let parts: Vec<&str> = path
        .trim_end_matches('/')
        .rsplit('/')
        .filter(|s| !s.is_empty())
        .take(2)
        .collect();
    match parts.len() {
        0 => path.to_string(),
        1 => parts[0].to_string(),
        _ => format!("{}/{}", parts[1], parts[0]),
    }
}

/// Left section: permission mode, work dir, git branch, server, model, session.
///
/// Items are progressively hidden as `width` decreases:
/// - `>= 80`: everything (permission mode + hint, work_dir, git_branch, server, model, session, remote)
/// - `>= 60`: hide "(shift+tab)" hint text
/// - `>= 45`: also hide work_dir and git_branch
/// - `<  45`: also hide session title
///
/// Permission mode, server name, and model name are always visible.
fn build_left_spans(state: &StatusBarState, width: usize) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Permission mode (always visible).
    spans.push(Span::styled(
        format!(" {}", state.permission_mode),
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ));

    // Hint text: only at width >= 80.
    if width >= 80 {
        spans.push(Span::styled(
            " (shift+tab)",
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Abbreviated working directory: only at width >= 60.
    if width >= 60
        && let Some(ref work_dir) = state.work_dir
    {
        let abbrev = abbreviate_path(work_dir);
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(abbrev, Style::default().fg(Color::White)));
    }

    // Git branch: only at width >= 60.
    if width >= 60
        && let Some(ref branch) = state.git_branch
    {
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(
            format!("\u{e0a0} {branch}"),
            Style::default().fg(Color::Magenta),
        ));
    }

    // Server name (always visible).
    if let Some(ref server) = state.server_name {
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(
            server.clone(),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Model name (always visible).
    if let Some(ref model) = state.model_name {
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(
            model.clone(),
            Style::default().fg(Color::LightBlue),
        ));
    }

    // Session title: only at width >= 45.
    if width >= 45
        && let Some(ref title) = state.session_title
    {
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(
            format!("session:{title}"),
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Remote tunnel indicator (always visible).
    if state.remote_connected {
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(
            "remote",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans
}

/// Center section: loop interrupt hint, plan/verbose indicators.
fn build_center_spans(state: &StatusBarState) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // "esc to interrupt" when the loop is running.
    if state.loop_active {
        spans.push(Span::styled(
            "esc to interrupt",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    // Plan mode indicator.
    if state.plan_mode {
        if !spans.is_empty() {
            spans.push(Span::styled(
                " \u{00b7} ",
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.push(Span::styled(
            "plan",
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Verbose indicator.
    if state.verbose {
        if !spans.is_empty() {
            spans.push(Span::styled(
                " \u{00b7} ",
                Style::default().fg(Color::DarkGray),
            ));
        }
        spans.push(Span::styled(
            "verbose",
            Style::default()
                .fg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        ));
    }

    spans
}

/// Right section: token count, context %, shortcuts hint.
fn build_right_spans(state: &StatusBarState) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    // Token count.
    if state.token_count > 0 {
        spans.push(Span::styled(
            format_tokens(state.token_count),
            Style::default().fg(Color::DarkGray),
        ));
        spans.push(Span::styled(
            " \u{00b7} ",
            Style::default().fg(Color::DarkGray),
        ));
    }

    // Context percentage.
    let ctx_color = context_color(state.context_percent);
    spans.push(Span::styled(
        format!("{}% ctx", state.context_percent),
        Style::default().fg(ctx_color),
    ));

    // Shortcuts hint.
    spans.push(Span::styled(
        " \u{00b7} ",
        Style::default().fg(Color::DarkGray),
    ));
    spans.push(Span::styled(
        "? shortcuts ",
        Style::default().fg(Color::DarkGray),
    ));

    spans
}

// ── Helpers ─────────────────────────────────────────────

/// Calculate the total display width of a slice of spans.
fn span_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

/// Format a token count for compact display.
///
/// - < 1000: `"42 tokens"`
/// - 1000..999_999: `"1.5k tokens"`
/// - >= 1_000_000: `"1.2M tokens"`
fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tokens", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens} tokens")
    }
}

/// Pick a color for the context percentage based on usage level.
///
/// - 0..50: green (healthy)
/// - 50..80: yellow (warning)
/// - 80..100: red (critical)
fn context_color(percent: u8) -> Color {
    if percent >= 80 {
        Color::Red
    } else if percent >= 50 {
        Color::Yellow
    } else {
        Color::Green
    }
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── StatusBarState defaults ─────────────────────

    #[test]
    fn default_state_has_empty_fields() {
        let state = StatusBarState::default();
        assert!(state.permission_mode.is_empty());
        assert!(state.server_name.is_none());
        assert!(state.model_name.is_none());
        assert!(!state.loop_active);
        assert!(!state.plan_mode);
        assert!(!state.verbose);
        assert_eq!(state.token_count, 0);
        assert_eq!(state.context_percent, 0);
    }

    // ── format_tokens ──────────────────────────────

    #[test]
    fn format_tokens_small() {
        assert_eq!(format_tokens(0), "0 tokens");
        assert_eq!(format_tokens(42), "42 tokens");
        assert_eq!(format_tokens(999), "999 tokens");
    }

    #[test]
    fn format_tokens_thousands() {
        assert_eq!(format_tokens(1_000), "1.0k tokens");
        assert_eq!(format_tokens(1_500), "1.5k tokens");
        assert_eq!(format_tokens(42_000), "42.0k tokens");
    }

    #[test]
    fn format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.0M tokens");
        assert_eq!(format_tokens(2_500_000), "2.5M tokens");
    }

    // ── context_color ──────────────────────────────

    #[test]
    fn context_color_green_for_low_usage() {
        assert_eq!(context_color(0), Color::Green);
        assert_eq!(context_color(25), Color::Green);
        assert_eq!(context_color(49), Color::Green);
    }

    #[test]
    fn context_color_yellow_for_medium_usage() {
        assert_eq!(context_color(50), Color::Yellow);
        assert_eq!(context_color(65), Color::Yellow);
        assert_eq!(context_color(79), Color::Yellow);
    }

    #[test]
    fn context_color_red_for_high_usage() {
        assert_eq!(context_color(80), Color::Red);
        assert_eq!(context_color(90), Color::Red);
        assert_eq!(context_color(100), Color::Red);
    }

    // ── span_width ─────────────────────────────────

    #[test]
    fn span_width_empty() {
        assert_eq!(span_width(&[]), 0);
    }

    #[test]
    fn span_width_sums_content_lengths() {
        let spans = vec![Span::raw("abc"), Span::raw("de")];
        assert_eq!(span_width(&spans), 5);
    }

    // ── build_left_spans ───────────────────────────

    #[test]
    fn left_spans_include_permission_mode() {
        let state = StatusBarState {
            permission_mode: "auto-edit".into(),
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("auto-edit"));
        assert!(text.contains("(shift+tab)"));
    }

    #[test]
    fn left_spans_include_server_when_present() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            server_name: Some("ollama".into()),
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("ollama"));
    }

    #[test]
    fn left_spans_include_model_when_present() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            model_name: Some("opus-4".into()),
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("opus-4"));
    }

    #[test]
    fn left_spans_omit_server_when_none() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            server_name: None,
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        // Should not contain a server separator.
        assert!(!text.contains("ollama"));
    }

    #[test]
    fn left_spans_permission_is_yellow_bold() {
        let state = StatusBarState {
            permission_mode: "auto-edit".into(),
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let perm_span = &spans[0];
        assert_eq!(perm_span.style.fg, Some(Color::Yellow));
        assert!(perm_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn left_spans_server_is_cyan() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            server_name: Some("anthropic".into()),
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        // Find the server span (after separator).
        let server_span = spans
            .iter()
            .find(|s| s.content.as_ref() == "anthropic")
            .expect("server span should exist");
        assert_eq!(server_span.style.fg, Some(Color::Cyan));
    }

    // ── build_center_spans ─────────────────────────

    #[test]
    fn center_spans_empty_when_all_inactive() {
        let state = StatusBarState::default();
        let spans = build_center_spans(&state);
        assert!(spans.is_empty());
    }

    #[test]
    fn center_spans_include_esc_when_loop_active() {
        let state = StatusBarState {
            loop_active: true,
            ..Default::default()
        };
        let spans = build_center_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("esc to interrupt"));
    }

    #[test]
    fn center_spans_esc_is_red_bold() {
        let state = StatusBarState {
            loop_active: true,
            ..Default::default()
        };
        let spans = build_center_spans(&state);
        let esc_span = &spans[0];
        assert_eq!(esc_span.style.fg, Some(Color::Red));
        assert!(esc_span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn center_spans_include_plan_indicator() {
        let state = StatusBarState {
            plan_mode: true,
            ..Default::default()
        };
        let spans = build_center_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("plan"));
    }

    #[test]
    fn center_spans_include_verbose_indicator() {
        let state = StatusBarState {
            verbose: true,
            ..Default::default()
        };
        let spans = build_center_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("verbose"));
    }

    #[test]
    fn center_spans_both_plan_and_verbose() {
        let state = StatusBarState {
            plan_mode: true,
            verbose: true,
            ..Default::default()
        };
        let spans = build_center_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("plan"));
        assert!(text.contains("verbose"));
    }

    #[test]
    fn center_spans_all_indicators_active() {
        let state = StatusBarState {
            loop_active: true,
            plan_mode: true,
            verbose: true,
            ..Default::default()
        };
        let spans = build_center_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("esc to interrupt"));
        assert!(text.contains("plan"));
        assert!(text.contains("verbose"));
    }

    // ── build_right_spans ──────────────────────────

    #[test]
    fn right_spans_include_context_percent() {
        let state = StatusBarState {
            context_percent: 42,
            ..Default::default()
        };
        let spans = build_right_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("42% ctx"));
    }

    #[test]
    fn right_spans_include_shortcuts_hint() {
        let state = StatusBarState::default();
        let spans = build_right_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("? shortcuts"));
    }

    #[test]
    fn right_spans_include_tokens_when_nonzero() {
        let state = StatusBarState {
            token_count: 1500,
            ..Default::default()
        };
        let spans = build_right_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("1.5k tokens"));
    }

    #[test]
    fn right_spans_omit_tokens_when_zero() {
        let state = StatusBarState {
            token_count: 0,
            ..Default::default()
        };
        let spans = build_right_spans(&state);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(!text.contains("tokens"));
    }

    // ── build_status_line (integration) ────────────

    #[test]
    fn status_line_minimal() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            ..Default::default()
        };
        let line = build_status_line(&state);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("manual"));
        assert!(text.contains("? shortcuts"));
    }

    #[test]
    fn status_line_fully_populated() {
        let state = StatusBarState {
            permission_mode: "auto-edit".into(),
            server_name: Some("anthropic".into()),
            model_name: Some("opus-4".into()),
            loop_active: true,
            plan_mode: true,
            verbose: true,
            token_count: 42_000,
            context_percent: 75,
            ..Default::default()
        };
        let line = build_status_line(&state);
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("auto-edit"));
        assert!(text.contains("anthropic"));
        assert!(text.contains("opus-4"));
        assert!(text.contains("esc to interrupt"));
        assert!(text.contains("plan"));
        assert!(text.contains("verbose"));
        assert!(text.contains("42.0k tokens"));
        assert!(text.contains("75% ctx"));
        assert!(text.contains("? shortcuts"));
    }

    // ── render_status_bar (smoke tests) ────────────

    #[test]
    fn render_default_state_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(120, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = StatusBarState {
            permission_mode: "auto-edit".into(),
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_status_bar(frame, area, &state);
            })
            .unwrap();
    }

    #[test]
    fn render_fully_populated_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(120, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = StatusBarState {
            permission_mode: "auto-edit".into(),
            server_name: Some("anthropic".into()),
            model_name: Some("opus-4".into()),
            loop_active: true,
            plan_mode: true,
            verbose: true,
            token_count: 42_000,
            context_percent: 75,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_status_bar(frame, area, &state);
            })
            .unwrap();
    }

    #[test]
    fn render_narrow_terminal_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(40, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = StatusBarState {
            permission_mode: "auto-edit".into(),
            server_name: Some("anthropic".into()),
            model_name: Some("opus-4".into()),
            loop_active: true,
            plan_mode: true,
            verbose: true,
            token_count: 100_000,
            context_percent: 95,
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_status_bar(frame, area, &state);
            })
            .unwrap();
    }

    #[test]
    fn render_very_narrow_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(10, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = StatusBarState {
            permission_mode: "x".into(),
            ..Default::default()
        };

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_status_bar(frame, area, &state);
            })
            .unwrap();
    }

    #[test]
    fn render_zero_width_does_not_panic() {
        let backend = ratatui::backend::TestBackend::new(1, 1);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        let state = StatusBarState::default();

        terminal
            .draw(|frame| {
                let area = Rect::new(0, 0, 0, 1);
                render_status_bar(frame, area, &state);
            })
            .unwrap();
    }

    // ── abbreviate_path ───────────────────────────

    #[test]
    fn abbreviate_path_two_components() {
        assert_eq!(abbreviate_path("/home/user/simse/core"), "simse/core");
    }

    #[test]
    fn abbreviate_path_one_component() {
        assert_eq!(abbreviate_path("/root"), "root");
    }

    #[test]
    fn abbreviate_path_trailing_slash() {
        assert_eq!(abbreviate_path("/home/user/simse/core/"), "simse/core");
    }

    #[test]
    fn abbreviate_path_deep() {
        assert_eq!(abbreviate_path("/a/b/c/d/e/f"), "e/f");
    }

    // ── work_dir, git_branch, session_title ───────

    #[test]
    fn left_spans_include_work_dir() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            work_dir: Some("/home/user/simse/core".into()),
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("simse/core"));
    }

    #[test]
    fn left_spans_include_git_branch() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            git_branch: Some("feature/search".into()),
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("feature/search"));
    }

    #[test]
    fn left_spans_include_session_title() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            session_title: Some("abc123".into()),
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("session:abc123"));
    }

    #[test]
    fn left_spans_omit_work_dir_when_none() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            work_dir: None,
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        // Should only contain permission + hint, no path separator.
        assert!(!text.contains("simse"));
    }

    #[test]
    fn left_spans_omit_git_branch_when_none() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            git_branch: None,
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        // Should not contain the git branch icon.
        assert!(!text.contains('\u{e0a0}'));
    }

    #[test]
    fn left_spans_include_remote_when_connected() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            remote_connected: true,
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("remote"));
    }

    #[test]
    fn left_spans_omit_remote_when_disconnected() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            remote_connected: false,
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(!text.contains("remote"));
    }

    #[test]
    fn left_spans_remote_is_green_bold() {
        let state = StatusBarState {
            permission_mode: "manual".into(),
            remote_connected: true,
            ..Default::default()
        };
        let spans = build_left_spans(&state, usize::MAX);
        let remote_span = spans
            .iter()
            .find(|s| s.content.as_ref() == "remote")
            .expect("remote span should exist");
        assert_eq!(remote_span.style.fg, Some(Color::Green));
        assert!(remote_span.style.add_modifier.contains(Modifier::BOLD));
    }
}
