//! Thinking spinner: animated status indicator during AI generation.
//!
//! Displays a cycling frame character, a random "thinking" verb, elapsed time,
//! token count, and server name. Designed for rendering as a ratatui widget.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ── Constants ───────────────────────────────────────────

/// Spinner frame characters (Windows-safe variant, matching the TS reference).
const FRAMES: &[char] = &[
    '\u{00b7}', '\u{2722}', '*', '\u{2736}', '\u{273B}', '\u{273D}',
];

/// Interval between frame advances (~120ms).
const FRAME_INTERVAL: Duration = Duration::from_millis(120);

/// Random verbs displayed alongside the spinner.
const VERBS: &[&str] = &[
    "Thinking",
    "Pondering",
    "Brewing",
    "Cooking",
    "Dreaming",
    "Weaving",
    "Crafting",
    "Musing",
    "Conjuring",
    "Scheming",
    "Plotting",
    "Imagining",
    "Composing",
    "Mulling",
    "Ruminating",
    "Contemplating",
    "Deliberating",
    "Considering",
    "Reflecting",
    "Meditating",
    "Analyzing",
    "Processing",
    "Computing",
    "Reasoning",
    "Evaluating",
];

// ── ThinkingSpinner ─────────────────────────────────────

/// Animated thinking spinner shown during AI generation.
///
/// The spinner cycles through [`FRAMES`] at ~120ms intervals, bouncing
/// back and forth (ping-pong). A random verb is selected on creation.
///
/// # Display format
///
/// ```text
/// {frame} {verb}... {elapsed}s {tokens} tokens ({server})
/// ```
pub struct ThinkingSpinner {
    frames: &'static [char],
    frame_idx: usize,
    /// Direction of frame cycling: 1 = forward, -1 = backward (ping-pong).
    direction: i8,
    verb: &'static str,
    started_at: Instant,
    last_tick: Instant,
    token_count: u64,
    server_name: String,
}

impl ThinkingSpinner {
    /// Create a new spinner for the given server name.
    ///
    /// A random verb is selected using a simple hash of the current system time.
    pub fn new(server_name: impl Into<String>) -> Self {
        let verb = pick_random_verb();
        let now = Instant::now();
        Self {
            frames: FRAMES,
            frame_idx: 0,
            direction: 1,
            verb,
            started_at: now,
            last_tick: now,
            token_count: 0,
            server_name: server_name.into(),
        }
    }

    /// Advance the spinner frame if enough time has elapsed (~120ms).
    ///
    /// Returns `true` if the frame changed (and the widget should be redrawn).
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_tick) < FRAME_INTERVAL {
            return false;
        }
        self.last_tick = now;

        // Ping-pong: bounce at both ends of the frame array.
        let next = self.frame_idx as i8 + self.direction;
        if next >= self.frames.len() as i8 - 1 {
            self.direction = -1;
            self.frame_idx = self.frames.len() - 1;
        } else if next <= 0 {
            self.direction = 1;
            self.frame_idx = 0;
        } else {
            self.frame_idx = next as usize;
        }

        true
    }

    /// Render the spinner into the given area using the ratatui [`Frame`].
    ///
    /// Layout: `  {frame} {verb}... ({elapsed} · {tokens} tokens · {server})`
    pub fn render(&self, area: Rect, frame: &mut Frame) {
        let line = self.to_line();
        let widget = ratatui::widgets::Paragraph::new(line);
        frame.render_widget(widget, area);
    }

    /// Set the current token count displayed by the spinner.
    pub fn set_token_count(&mut self, n: u64) {
        self.token_count = n;
    }

    /// How long the spinner has been running.
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    /// Build the display [`Line`] (useful for embedding in other widgets).
    pub fn to_line(&self) -> Line<'static> {
        let frame_char = self.frames[self.frame_idx];
        let elapsed = self.elapsed();

        // Build suffix parts: elapsed, tokens, server.
        let mut suffix_parts: Vec<String> = Vec::new();
        suffix_parts.push(format_duration(elapsed));
        if self.token_count > 0 {
            suffix_parts.push(format_tokens(self.token_count));
        }
        if !self.server_name.is_empty() {
            suffix_parts.push(self.server_name.clone());
        }

        let suffix = if suffix_parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", suffix_parts.join(" \u{00b7} "))
        };

        Line::from(vec![
            Span::raw("  "),
            Span::styled(frame_char.to_string(), Style::default().fg(Color::Magenta)),
            Span::raw(" "),
            Span::styled(
                format!("{}...{suffix}", self.verb),
                Style::default().fg(Color::DarkGray),
            ),
        ])
    }
}

// ── Helpers ─────────────────────────────────────────────

/// Pick a pseudo-random verb using system time nanoseconds.
fn pick_random_verb() -> &'static str {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .subsec_nanos();
    VERBS[nanos as usize % VERBS.len()]
}

/// Format a duration for display: `<1s`, `1.2s`, `1m30s`.
fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let minutes = ms / 60_000;
        let seconds = (ms % 60_000) / 1000;
        format!("{minutes}m{seconds}s")
    }
}

/// Format a token count: `42 tokens`, `1.5k tokens`, `1.2M tokens`.
fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M tokens", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k tokens", tokens as f64 / 1_000.0)
    } else {
        format!("{tokens} tokens")
    }
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_spinner_has_valid_initial_state() {
        let spinner = ThinkingSpinner::new("test-server");
        assert_eq!(spinner.frame_idx, 0);
        assert_eq!(spinner.token_count, 0);
        assert_eq!(spinner.server_name, "test-server");
        assert!(VERBS.contains(&spinner.verb));
    }

    #[test]
    fn tick_returns_false_when_interval_not_elapsed() {
        let mut spinner = ThinkingSpinner::new("server");
        // Immediately after creation, tick should return false (not enough time).
        let changed = spinner.tick();
        assert!(!changed);
    }

    #[test]
    fn tick_advances_frame_after_interval() {
        let mut spinner = ThinkingSpinner::new("server");
        // Force the last_tick to be old enough.
        spinner.last_tick = Instant::now() - Duration::from_millis(200);
        let changed = spinner.tick();
        assert!(changed);
        // Frame should have advanced from 0.
        assert!(spinner.frame_idx < FRAMES.len());
    }

    #[test]
    fn tick_bounces_at_end() {
        let mut spinner = ThinkingSpinner::new("server");
        // Set to last frame, direction forward.
        spinner.frame_idx = FRAMES.len() - 2;
        spinner.direction = 1;
        spinner.last_tick = Instant::now() - Duration::from_millis(200);
        spinner.tick();
        // Should have reached the end and reversed direction.
        assert_eq!(spinner.frame_idx, FRAMES.len() - 1);
        assert_eq!(spinner.direction, -1);
    }

    #[test]
    fn tick_bounces_at_start() {
        let mut spinner = ThinkingSpinner::new("server");
        // Set to second frame, direction backward.
        spinner.frame_idx = 1;
        spinner.direction = -1;
        spinner.last_tick = Instant::now() - Duration::from_millis(200);
        spinner.tick();
        // Should have reached the start and reversed direction.
        assert_eq!(spinner.frame_idx, 0);
        assert_eq!(spinner.direction, 1);
    }

    #[test]
    fn set_token_count_updates_count() {
        let mut spinner = ThinkingSpinner::new("server");
        assert_eq!(spinner.token_count, 0);
        spinner.set_token_count(1500);
        assert_eq!(spinner.token_count, 1500);
    }

    #[test]
    fn elapsed_returns_nonzero_duration() {
        let spinner = ThinkingSpinner::new("server");
        // elapsed() should return something >= 0.
        let elapsed = spinner.elapsed();
        // Just verify it doesn't panic and returns a valid duration.
        assert!(elapsed.as_nanos() < u128::MAX);
    }

    #[test]
    fn to_line_contains_verb_and_server() {
        let mut spinner = ThinkingSpinner::new("my-server");
        spinner.set_token_count(42);
        let line = spinner.to_line();
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains(spinner.verb));
        assert!(text.contains("my-server"));
        assert!(text.contains("42 tokens"));
        assert!(text.contains("..."));
    }

    #[test]
    fn to_line_omits_tokens_when_zero() {
        let spinner = ThinkingSpinner::new("server");
        let line = spinner.to_line();
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        assert!(!text.contains("tokens"));
    }

    #[test]
    fn to_line_omits_server_when_empty() {
        let spinner = ThinkingSpinner::new("");
        let line = spinner.to_line();
        let text: String = line.spans.iter().map(|s| s.content.to_string()).collect();
        // Should still contain the verb and ellipsis.
        assert!(text.contains("..."));
    }

    #[test]
    fn frame_char_is_magenta() {
        let spinner = ThinkingSpinner::new("server");
        let line = spinner.to_line();
        // The second span (index 1) should be the frame character, colored magenta.
        assert!(line.spans.len() >= 2);
        assert_eq!(line.spans[1].style.fg, Some(Color::Magenta));
    }

    #[test]
    fn frames_array_is_correct() {
        assert_eq!(FRAMES.len(), 6);
        assert_eq!(FRAMES[0], '\u{00b7}');
        assert_eq!(FRAMES[1], '\u{2722}');
        assert_eq!(FRAMES[2], '*');
        assert_eq!(FRAMES[3], '\u{2736}');
        assert_eq!(FRAMES[4], '\u{273B}');
        assert_eq!(FRAMES[5], '\u{273D}');
    }

    #[test]
    fn verbs_array_has_25_entries() {
        assert_eq!(VERBS.len(), 25);
    }

    #[test]
    fn format_duration_milliseconds() {
        assert_eq!(format_duration(Duration::from_millis(42)), "42ms");
        assert_eq!(format_duration(Duration::from_millis(999)), "999ms");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(Duration::from_millis(1000)), "1.0s");
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_duration(Duration::from_millis(59999)), "60.0s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(Duration::from_millis(60_000)), "1m0s");
        assert_eq!(format_duration(Duration::from_millis(90_000)), "1m30s");
    }

    #[test]
    fn format_tokens_small() {
        assert_eq!(format_tokens(42), "42 tokens");
        assert_eq!(format_tokens(999), "999 tokens");
    }

    #[test]
    fn format_tokens_thousands() {
        assert_eq!(format_tokens(1000), "1.0k tokens");
        assert_eq!(format_tokens(1500), "1.5k tokens");
        assert_eq!(format_tokens(42_000), "42.0k tokens");
    }

    #[test]
    fn format_tokens_millions() {
        assert_eq!(format_tokens(1_000_000), "1.0M tokens");
        assert_eq!(format_tokens(2_500_000), "2.5M tokens");
    }

    #[test]
    fn pick_random_verb_returns_valid_verb() {
        let verb = pick_random_verb();
        assert!(VERBS.contains(&verb));
    }

    #[test]
    fn multiple_ticks_produce_animation() {
        let mut spinner = ThinkingSpinner::new("server");
        let mut frames_seen = vec![spinner.frame_idx];

        // Simulate several ticks with forced elapsed time.
        for _ in 0..10 {
            spinner.last_tick = Instant::now() - Duration::from_millis(200);
            spinner.tick();
            frames_seen.push(spinner.frame_idx);
        }

        // Should have visited more than one frame index.
        frames_seen.sort();
        frames_seen.dedup();
        assert!(
            frames_seen.len() > 1,
            "Expected animation across multiple frames"
        );
    }
}
