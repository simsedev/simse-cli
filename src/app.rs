//! Elm Architecture: Model, Update, View.

use crate::ui_core::app::{OutputItem, ToolCallState, ToolCallStatus};
use crate::ui_core::commands::registry::{CommandDefinition, all_commands};
use crate::ui_core::input::state as input;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::autocomplete::{CommandAutocompleteState, render_inline_completions};
use crate::commands::CommandContext;
use crate::search::SearchState;
use crate::spinner::ThinkingSpinner;

use crate::banner;
use crate::constants::MAX_VISIBLE_COMPLETIONS;
use crate::output;
use crate::status_bar::{StatusBarState, render_status_bar};

// ── LoopStatus ──────────────────────────────────────────

/// Current status of the agentic loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopStatus {
    Idle,
    Streaming,
    ToolExecuting,
}

// ── App (the Model) ─────────────────────────────────────

/// Application state (the Model).
pub struct App {
    pub input: input::InputState,
    pub output: Vec<OutputItem>,
    pub stream_text: String,
    pub active_tool_calls: Vec<ToolCallState>,
    pub loop_status: LoopStatus,
    /// Command autocomplete state.
    pub autocomplete: CommandAutocompleteState,
    pub scroll_offset: usize,
    /// When true (default) the viewport stays pinned to the latest content.
    /// Cleared when the user scrolls up so streaming/spinner updates don't
    /// drag the view back to the bottom; re-set on scroll-to-bottom + submit.
    pub follow_bottom: bool,
    pub ctrl_c_pending: bool,
    pub plan_mode: bool,
    pub verbose: bool,
    pub permission_mode: String,
    pub total_tokens: u64,
    pub context_percent: u8,
    pub history: Vec<String>,
    pub history_index: Option<usize>,
    pub history_draft: String,
    pub commands: Vec<CommandDefinition>,
    pub banner_visible: bool,
    pub version: String,
    pub server_name: Option<String>,
    pub model_name: Option<String>,
    pub session_id: Option<String>,
    pub acp_connected: bool,
    pub work_dir: Option<String>,
    /// Thinking spinner — visible during the entire generation lifecycle.
    pub spinner: Option<ThinkingSpinner>,
    /// Conversation search state (Ctrl+F).
    pub search: SearchState,
    /// Git branch name (read from .git/HEAD at startup).
    pub git_branch: Option<String>,
    /// Instant tracker for active tool call timing.
    pub tool_call_instants: Vec<(String, std::time::Instant)>,
    /// Whether the WebSocket tunnel is connected to the relay.
    pub remote_connected: bool,
    /// Email of the logged-in user (for display).
    pub remote_email: Option<String>,
    /// Pending permission responder — when set, input intercepts y/n/a.
    pub pending_permission: Option<tokio::sync::oneshot::Sender<bool>>,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            input: input::InputState::default(),
            output: Vec::new(),
            stream_text: String::new(),
            active_tool_calls: Vec::new(),
            loop_status: LoopStatus::Idle,
            autocomplete: CommandAutocompleteState::new(),
            scroll_offset: 0,
            follow_bottom: true,
            ctrl_c_pending: false,
            plan_mode: false,
            verbose: false,
            permission_mode: "ask".into(),
            total_tokens: 0,
            context_percent: 0,
            history: Vec::new(),
            history_index: None,
            history_draft: String::new(),
            commands: all_commands(),
            banner_visible: true,
            version: env!("CARGO_PKG_VERSION").into(),
            server_name: None,
            model_name: None,
            session_id: None,
            acp_connected: false,
            work_dir: None,
            spinner: None,
            search: SearchState::new(),
            git_branch: None,
            tool_call_instants: Vec::new(),
            remote_connected: false,
            remote_email: None,
            pending_permission: None,
        }
    }
}

// ── AppMessage ──────────────────────────────────────────

/// Messages the app can receive.
pub enum AppMessage {
    // Input
    CharInput(char),
    Paste(String),
    Submit,
    Backspace,
    Delete,
    DeleteWordBack,
    CursorLeft,
    CursorRight,
    WordLeft,
    WordRight,
    Home,
    End,
    SelectLeft,
    SelectRight,
    SelectHome,
    SelectEnd,
    SelectAll,
    HistoryUp,
    HistoryDown,

    // Navigation
    ScrollUp(usize),
    ScrollDown(usize),
    ScrollToBottom,

    // App control
    CtrlC,
    CtrlCTimeout,
    Escape,
    CtrlL,
    ShiftTab,
    Tab,
    Quit,

    // Screen transitions
    ShowShortcuts,
    DismissOverlay,

    // Loop events (from bridge)
    StreamStart,
    StreamDelta(String),
    StreamEnd {
        text: String,
    },
    ToolCallStart(ToolCallState),
    ToolCallEnd {
        id: String,
        status: ToolCallStatus,
        summary: Option<String>,
        error: Option<String>,
        duration_ms: Option<u64>,
        diff: Option<String>,
    },
    TokenUsage {
        prompt: u64,
        completion: u64,
    },
    LoopComplete,
    LoopError(String),

    // Permission (inline)
    PermissionPrompt {
        tool_name: String,
        args_summary: String,
        responder: tokio::sync::oneshot::Sender<bool>,
    },

    // Bridge
    BridgeResult {
        action: String,
        text: String,
        is_error: bool,
    },
    RefreshContext(CommandContext),

    /// Tunnel connection status changed.
    RemoteStatus {
        connected: bool,
        email: Option<String>,
    },

    // Settings
    /// Settings config file loaded from storage.
    SettingsFileLoaded(serde_json::Value),
    /// Settings field saved to storage.
    SettingsFieldSaved {
        key: String,
        value: serde_json::Value,
    },
    /// Settings error occurred.
    SettingsError(String),

    // Search (Ctrl+F)
    SearchOpen,
    SearchInput(char),
    SearchBackspace,
    SearchNext,
    SearchPrev,
    SearchClose,

    // Multi-line input
    NewLine,

    // Timer
    Tick,

    // Resize
    Resize {
        width: u16,
        height: u16,
    },
}

// update(), dispatch_command(), scroll_to_search_match() moved to cli::update module.

/// View: render the model to the terminal.
pub fn view(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let completions_height = if app.autocomplete.is_active() {
        let total = app.autocomplete.matches.len() as u16;
        total.min(MAX_VISIBLE_COMPLETIONS)
    } else {
        0
    };

    // Dynamic input height based on newline count (min 3, max 10).
    let line_count = app.input.value.lines().count().max(1);
    let input_height = (line_count as u16 + 2).clamp(3, 10);

    let search_height: u16 = if app.search.active { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if completions_height > 0 {
            vec![
                Constraint::Length(search_height),
                Constraint::Min(1),
                Constraint::Length(input_height),
                Constraint::Length(completions_height),
                Constraint::Length(1),
            ]
        } else {
            vec![
                Constraint::Length(search_height),
                Constraint::Min(1),
                Constraint::Length(input_height),
                Constraint::Length(0),
                Constraint::Length(1),
            ]
        })
        .split(area);

    // 0. Search bar (if active)
    if app.search.active {
        render_search_bar(app, frame, chunks[0]);
    }

    // 1. Chat area
    render_chat_area(app, frame, chunks[1]);

    // 2. Input
    render_input(app, frame, chunks[2]);

    // 3. Completions (inline, below input)
    if completions_height > 0 {
        let lines = render_inline_completions(&app.autocomplete, chunks[3].width);
        let completions = Paragraph::new(lines);
        frame.render_widget(completions, chunks[3]);
    }

    // 4. Status bar
    let status_state = StatusBarState {
        permission_mode: app.permission_mode.clone(),
        server_name: app.server_name.clone(),
        model_name: app.model_name.clone(),
        loop_active: app.loop_status != LoopStatus::Idle,
        plan_mode: app.plan_mode,
        verbose: app.verbose,
        token_count: app.total_tokens,
        context_percent: app.context_percent,
        work_dir: app.work_dir.clone(),
        git_branch: app.git_branch.clone(),
        session_title: app.session_id.clone(),
        remote_connected: app.remote_connected,
    };
    render_status_bar(frame, chunks[4], &status_state);
}

/// Render the chat area: either the banner or scrollable output.
fn render_chat_area(app: &App, frame: &mut Frame, area: Rect) {
    if app.banner_visible && app.output.is_empty() {
        banner::render_banner(frame, area, app);
        return;
    }

    // Build all output lines.
    let mut lines = output::render_output_items(&app.output, area.width);

    // Append the in-progress stream text, rendered through the SAME markdown
    // path as a committed assistant message. Rendering it identically means
    // that when StreamEnd swaps `stream_text` for an `OutputItem::Message`
    // there is no restyle/reflow flash — the content looks the same before
    // and after the swap.
    if !app.stream_text.is_empty() {
        lines.extend(crate::markdown::render_markdown(
            &app.stream_text,
            area.width,
        ));
    }

    // Show active tool calls with elapsed time.
    for tc in &app.active_tool_calls {
        let elapsed_str = app
            .tool_call_instants
            .iter()
            .find(|(id, _)| *id == tc.id)
            .map(|(_, instant)| format_tool_elapsed(instant.elapsed()));
        let tc_lines = render_active_tool_call(tc, elapsed_str.as_deref());
        lines.extend(tc_lines);
    }

    // Show thinking spinner during generation.
    if let Some(ref spinner) = app.spinner {
        lines.push(spinner.to_line());
    }

    // Scroll model: `follow_bottom` (default) pins the viewport to the latest
    // content. Once the user scrolls up, `follow_bottom` is cleared and the
    // viewport holds at `scroll_offset` lines above the bottom — so streaming
    // deltas + the spinner no longer yank the view back down every frame, and
    // history is readable. Re-pins when the user scrolls back to the bottom or
    // submits a new turn.
    let visible_height = area.height as usize;
    let total_lines = lines.len();
    let max_scroll = total_lines.saturating_sub(visible_height);
    // Clamp to u16::MAX before the cast: a transcript past ~65k rendered lines
    // would otherwise truncate-wrap and jump the viewport to the top.
    let scroll = if app.follow_bottom {
        max_scroll.min(u16::MAX as usize) as u16
    } else {
        let clamped_offset = app.scroll_offset.min(max_scroll);
        max_scroll
            .saturating_sub(clamped_offset)
            .min(u16::MAX as usize) as u16
    };

    let chat = Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((scroll, 0));
    frame.render_widget(chat, area);
}

/// Render the search bar at the top of the chat area.
fn render_search_bar(app: &App, frame: &mut Frame, area: Rect) {
    let match_info = app.search.match_display();
    let mut spans = vec![
        Span::styled(
            " Find: ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(app.search.query.clone()),
    ];
    if !match_info.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            match_info,
            Style::default().fg(Color::DarkGray),
        ));
    }
    let line = Line::from(spans);
    let widget = Paragraph::new(line).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(widget, area);

    // Place cursor at end of search query.
    if app.search.active {
        let cursor_x = area
            .x
            .saturating_add(7) // " Find: " is 7 chars
            .saturating_add(app.search.query.chars().count() as u16);
        frame.set_cursor_position((cursor_x, area.y));
    }
}

/// Render the input area (supports multi-line).
fn render_input(app: &App, frame: &mut Frame, area: Rect) {
    let ghost = app.autocomplete.ghost_text();

    let input_lines: Vec<Line<'static>> = if app.input.value.is_empty() {
        if app.ctrl_c_pending {
            vec![Line::from(Span::styled(
                "Press Ctrl-C again to exit",
                Style::default().fg(Color::Yellow),
            ))]
        } else {
            vec![Line::from(Span::styled(
                "Type a message... (Shift+Enter for new line)",
                Style::default().fg(Color::DarkGray),
            ))]
        }
    } else if let Some(ref ghost_str) = ghost {
        // Ghost text only applies to the last line of input.
        let text_lines: Vec<&str> = app.input.value.split('\n').collect();
        let mut lines: Vec<Line<'static>> = Vec::new();
        for (i, line) in text_lines.iter().enumerate() {
            if i == text_lines.len() - 1 {
                lines.push(Line::from(vec![
                    Span::raw(line.to_string()),
                    Span::styled(ghost_str.clone(), Style::default().fg(Color::DarkGray)),
                ]));
            } else {
                lines.push(Line::from(line.to_string()));
            }
        }
        lines
    } else {
        app.input
            .value
            .split('\n')
            .map(|l| Line::from(l.to_string()))
            .collect()
    };

    let input_widget =
        Paragraph::new(input_lines).block(Block::default().borders(Borders::ALL).title("Input"));
    frame.render_widget(input_widget, area);

    // Hide cursor when search is active.
    if !app.search.active {
        // Calculate cursor position accounting for multi-line input.
        // Use char count (not byte offset) for correct display with multi-byte UTF-8.
        let text_before_cursor = &app.input.value[..app.input.cursor];
        let cursor_line = text_before_cursor.matches('\n').count();
        let last_newline = text_before_cursor.rfind('\n').map_or(0, |p| p + 1);
        let cursor_col = text_before_cursor[last_newline..].chars().count();

        let cursor_x = area
            .x
            .saturating_add(1)
            .saturating_add((cursor_col as u16).min(area.width.saturating_sub(2)));
        let cursor_y = area.y + 1 + cursor_line as u16;
        // Only show cursor if within the visible area.
        if cursor_y < area.y + area.height.saturating_sub(1) {
            frame.set_cursor_position((cursor_x, cursor_y));
        }
    }
}

/// Render an active tool call with optional elapsed time.
fn render_active_tool_call(tc: &ToolCallState, elapsed: Option<&str>) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let status_char = "\u{23fa}";
    let status_color = Color::Yellow;

    let mut first_spans = vec![
        Span::styled(format!("{status_char} "), Style::default().fg(status_color)),
        Span::styled(
            tc.name.clone(),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if let Some(elapsed_str) = elapsed {
        first_spans.push(Span::styled(
            format!(" ({elapsed_str})"),
            Style::default().fg(Color::DarkGray),
        ));
    }

    lines.push(Line::from(first_spans));

    if let Some(ref summary) = tc.summary {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(summary.clone(), Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines
}

/// Format elapsed time for tool calls.
///
/// Displays as "45ms", "1.2s", or "1m30s".
fn format_tool_elapsed(d: std::time::Duration) -> String {
    let ms = d.as_millis();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_app_defaults() {
        let app = App::new();
        assert_eq!(app.loop_status, LoopStatus::Idle);
        assert!(app.banner_visible);
        assert!(!app.commands.is_empty());
    }
}
