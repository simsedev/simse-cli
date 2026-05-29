//! Markdown renderer: converts markdown text to ratatui `Line`/`Span` elements.
//!
//! Uses `pulldown-cmark` for parsing and applies syntax highlighting for fenced
//! code blocks in JS/TS, Python, Bash, JSON, and Rust.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

// ── Public API ──────────────────────────────────────────

/// Render markdown text into a list of ratatui `Line`s suitable for display.
///
/// `width` controls horizontal-rule length and table column sizing.
pub fn render_markdown(text: &str, width: u16) -> Vec<Line<'static>> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let parser = Parser::new_ext(text, options);
    let mut renderer = MarkdownRenderer::new(width);
    renderer.render(parser);
    renderer.lines
}

// ── Renderer state ──────────────────────────────────────

struct MarkdownRenderer {
    width: u16,
    lines: Vec<Line<'static>>,

    /// Accumulated inline spans for the current line/paragraph.
    current_spans: Vec<Span<'static>>,

    /// Style stack for nested inline formatting.
    style_stack: Vec<Style>,

    /// Current heading level (None when not inside a heading).
    heading_level: Option<HeadingLevel>,

    /// Are we inside a blockquote?
    in_blockquote: bool,

    /// Whether we are inside a code block (fenced or indented).
    in_code_block: bool,
    /// Code block language (Some when inside a fenced code block with language info).
    code_block_lang: Option<String>,
    /// Accumulated code block text.
    code_block_buf: String,

    /// Current link URL (set during Start(Link), consumed at End(Link)).
    link_url: String,

    /// List nesting: each entry is `None` for unordered, `Some(n)` for ordered.
    list_stack: Vec<Option<u64>>,
    /// Whether the current list item has emitted its first line.
    item_started: bool,

    /// Table state.
    in_table: bool,
    table_head: bool,
    table_rows: Vec<Vec<Vec<Span<'static>>>>,
    table_current_row: Vec<Vec<Span<'static>>>,
    table_cell_spans: Vec<Span<'static>>,
    table_alignments: Vec<pulldown_cmark::Alignment>,
}

impl MarkdownRenderer {
    fn new(width: u16) -> Self {
        Self {
            width,
            lines: Vec::new(),
            current_spans: Vec::new(),
            style_stack: vec![Style::default()],
            heading_level: None,
            in_blockquote: false,
            in_code_block: false,
            code_block_lang: None,
            code_block_buf: String::new(),
            link_url: String::new(),
            list_stack: Vec::new(),
            item_started: false,
            in_table: false,
            table_head: false,
            table_rows: Vec::new(),
            table_current_row: Vec::new(),
            table_cell_spans: Vec::new(),
            table_alignments: Vec::new(),
        }
    }

    fn current_style(&self) -> Style {
        self.style_stack.last().copied().unwrap_or_default()
    }

    fn push_style(&mut self, modifier: Modifier) {
        let base = self.current_style();
        self.style_stack.push(base.add_modifier(modifier));
    }

    fn push_style_full(&mut self, style: Style) {
        self.style_stack.push(style);
    }

    fn pop_style(&mut self) {
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }

    /// Flush current_spans into a Line and push it.
    fn flush_line(&mut self) {
        if self.current_spans.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.current_spans);
        let line = if self.in_blockquote {
            let mut prefixed = vec![Span::styled(
                "\u{2502} ".to_string(),
                Style::default().fg(Color::DarkGray),
            )];
            prefixed.extend(spans);
            Line::from(prefixed)
        } else {
            Line::from(spans)
        };
        self.lines.push(line);
    }

    /// Push a complete line directly.
    fn push_line(&mut self, line: Line<'static>) {
        if self.in_blockquote {
            let mut prefixed = vec![Span::styled(
                "\u{2502} ".to_string(),
                Style::default().fg(Color::DarkGray),
            )];
            prefixed.extend(line.spans);
            self.lines.push(Line::from(prefixed));
        } else {
            self.lines.push(line);
        }
    }

    fn render<'a>(&mut self, parser: Parser<'a>) {
        for event in parser {
            self.process_event(event);
        }
        // Flush any remaining spans.
        self.flush_line();
    }

    fn process_event<'a>(&mut self, event: Event<'a>) {
        match event {
            // ── Block-level starts ──────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                self.flush_line();
                self.heading_level = Some(level);
                match level {
                    HeadingLevel::H1 => {
                        self.push_style_full(
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        );
                    }
                    HeadingLevel::H2 => {
                        self.push_style(Modifier::BOLD);
                    }
                    HeadingLevel::H3 => {
                        self.push_style(Modifier::UNDERLINED);
                    }
                    _ => {
                        self.push_style(Modifier::BOLD);
                    }
                }
            }

            Event::Start(Tag::Paragraph) => {
                self.flush_line();
            }

            Event::Start(Tag::BlockQuote(_)) => {
                self.flush_line();
                self.in_blockquote = true;
            }

            Event::Start(Tag::CodeBlock(kind)) => {
                self.flush_line();
                self.in_code_block = true;
                let lang = match &kind {
                    CodeBlockKind::Fenced(info) => {
                        let lang = info.split_whitespace().next().unwrap_or("");
                        if lang.is_empty() {
                            None
                        } else {
                            Some(lang.to_lowercase())
                        }
                    }
                    CodeBlockKind::Indented => None,
                };
                self.code_block_lang = lang;
                self.code_block_buf.clear();
            }

            Event::Start(Tag::List(first_item)) => {
                self.flush_line();
                self.list_stack.push(first_item);
            }

            Event::Start(Tag::Item) => {
                self.flush_line();
                self.item_started = false;
            }

            Event::Start(Tag::Table(alignments)) => {
                self.flush_line();
                self.in_table = true;
                self.table_alignments = alignments;
                self.table_rows.clear();
                self.table_current_row.clear();
            }

            Event::Start(Tag::TableHead) => {
                self.table_head = true;
                self.table_current_row.clear();
            }

            Event::Start(Tag::TableRow) => {
                self.table_current_row.clear();
            }

            Event::Start(Tag::TableCell) => {
                self.table_cell_spans.clear();
            }

            // ── Inline starts ───────────────────────
            Event::Start(Tag::Emphasis) => {
                self.push_style(Modifier::ITALIC);
            }

            Event::Start(Tag::Strong) => {
                self.push_style(Modifier::BOLD);
            }

            Event::Start(Tag::Strikethrough) => {
                self.push_style_full(Style::default().fg(Color::DarkGray));
            }

            Event::Start(Tag::Link { dest_url, .. }) => {
                self.push_style_full(
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::UNDERLINED),
                );
                // Store URL to display after the link text at End(Link).
                self.link_url = dest_url.to_string();
            }

            // ── Text content ────────────────────────
            Event::Text(text) => {
                // Inside a code block: accumulate text for later highlighting.
                if self.in_code_block {
                    self.code_block_buf.push_str(&text);
                    return;
                }

                if self.in_table {
                    let style = if self.table_head {
                        Style::default().add_modifier(Modifier::BOLD)
                    } else {
                        self.current_style()
                    };
                    self.table_cell_spans
                        .push(Span::styled(text.to_string(), style));
                    return;
                }

                // Handle list item prefix on first text.
                if !self.list_stack.is_empty() && !self.item_started {
                    self.emit_list_prefix();
                    self.item_started = true;
                }

                let style = self.current_style();
                self.current_spans
                    .push(Span::styled(text.to_string(), style));
            }

            Event::Code(code) => {
                if self.in_table {
                    self.table_cell_spans.push(Span::styled(
                        code.to_string(),
                        Style::default().fg(Color::Cyan),
                    ));
                    return;
                }
                self.current_spans.push(Span::styled(
                    code.to_string(),
                    Style::default().fg(Color::Cyan),
                ));
            }

            Event::SoftBreak => {
                if self.in_table {
                    self.table_cell_spans.push(Span::raw(" ".to_string()));
                } else {
                    self.current_spans.push(Span::raw(" ".to_string()));
                }
            }

            Event::HardBreak => {
                self.flush_line();
            }

            Event::Rule => {
                self.flush_line();
                let rule_len = (self.width as usize).saturating_sub(2);
                self.push_line(Line::from(Span::styled(
                    "\u{2500}".repeat(rule_len),
                    Style::default().fg(Color::DarkGray),
                )));
            }

            Event::TaskListMarker(checked) => {
                let marker = if checked { "\u{2611} " } else { "\u{2610} " };
                // Prepend checkbox before list item text.
                if !self.list_stack.is_empty() && !self.item_started {
                    self.emit_list_prefix();
                    self.item_started = true;
                }
                self.current_spans.push(Span::raw(marker.to_string()));
            }

            // ── Block-level ends ────────────────────
            Event::End(TagEnd::Heading(_level)) => {
                self.flush_line();
                self.heading_level = None;
                self.pop_style();
            }

            Event::End(TagEnd::Paragraph) => {
                self.flush_line();
                self.lines.push(Line::default());
            }

            Event::End(TagEnd::BlockQuote(_)) => {
                self.flush_line();
                self.in_blockquote = false;
            }

            Event::End(TagEnd::CodeBlock) => {
                // Emit syntax-highlighted code block.
                self.in_code_block = false;
                let code = std::mem::take(&mut self.code_block_buf);
                let lang = self.code_block_lang.take();
                self.emit_code_block(&code, lang.as_deref());
            }

            Event::End(TagEnd::List(_)) => {
                self.flush_line();
                self.list_stack.pop();
                if self.list_stack.is_empty() {
                    self.lines.push(Line::default());
                }
            }

            Event::End(TagEnd::Item) => {
                self.flush_line();
            }

            Event::End(TagEnd::Table) => {
                // Emit the complete table.
                self.emit_table();
                self.in_table = false;
            }

            Event::End(TagEnd::TableHead) => {
                self.table_head = false;
                let row = std::mem::take(&mut self.table_current_row);
                self.table_rows.insert(0, row); // Header is first row.
            }

            Event::End(TagEnd::TableRow) => {
                let row = std::mem::take(&mut self.table_current_row);
                self.table_rows.push(row);
            }

            Event::End(TagEnd::TableCell) => {
                let cell = std::mem::take(&mut self.table_cell_spans);
                self.table_current_row.push(cell);
            }

            // ── Inline ends ─────────────────────────
            Event::End(TagEnd::Emphasis) => {
                self.pop_style();
            }

            Event::End(TagEnd::Strong) => {
                self.pop_style();
            }

            Event::End(TagEnd::Strikethrough) => {
                self.pop_style();
            }

            Event::End(TagEnd::Link) => {
                self.pop_style();
                // Append the URL in parentheses after the link text.
                let url = std::mem::take(&mut self.link_url);
                if !url.is_empty() {
                    self.current_spans.push(Span::styled(
                        format!(" ({url})"),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }

            // Catch-all for unhandled events.
            _ => {}
        }
    }

    /// Emit the list item prefix (bullet or number) with proper indentation.
    fn emit_list_prefix(&mut self) {
        let depth = self.list_stack.len().saturating_sub(1);
        let indent = "  ".repeat(depth);

        if let Some(list_kind) = self.list_stack.last_mut() {
            match list_kind {
                Some(n) => {
                    let prefix = format!("{indent}{n}. ");
                    self.current_spans
                        .push(Span::styled(prefix, Style::default().fg(Color::DarkGray)));
                    *n += 1;
                }
                None => {
                    let prefix = format!("{indent}\u{2022} ");
                    self.current_spans
                        .push(Span::styled(prefix, Style::default().fg(Color::DarkGray)));
                }
            }
        }
    }

    /// Emit a code block with optional syntax highlighting.
    fn emit_code_block(&mut self, code: &str, lang: Option<&str>) {
        let dim_border = Style::default().fg(Color::DarkGray);

        // Top border with language tag.
        let lang_label = lang.unwrap_or("");
        let border_len = (self.width as usize).saturating_sub(lang_label.len() + 4);
        let top_border = if lang_label.is_empty() {
            format!(
                "\u{256d}{}",
                "\u{2500}".repeat((self.width as usize).saturating_sub(1))
            )
        } else {
            format!(
                "\u{256d}\u{2500} {lang_label} {}",
                "\u{2500}".repeat(border_len.max(1))
            )
        };
        self.push_line(Line::from(Span::styled(top_border, dim_border)));

        // Code lines with syntax highlighting.
        for line in code.lines() {
            let mut spans = vec![Span::styled("\u{2502} ".to_string(), dim_border)];
            spans.extend(highlight_line(line, lang));
            self.push_line(Line::from(spans));
        }

        // Handle empty code blocks.
        if code.is_empty() || (code.len() == 1 && code.ends_with('\n')) {
            self.push_line(Line::from(Span::styled(
                "\u{2502} ".to_string(),
                dim_border,
            )));
        }

        // Bottom border.
        let bottom_border = format!(
            "\u{2570}{}",
            "\u{2500}".repeat((self.width as usize).saturating_sub(1))
        );
        self.push_line(Line::from(Span::styled(bottom_border, dim_border)));
    }

    /// Emit the accumulated table as a simple grid.
    fn emit_table(&mut self) {
        // Take ownership of rows to avoid borrow conflicts with self.push_line().
        let rows = std::mem::take(&mut self.table_rows);

        if rows.is_empty() {
            return;
        }

        let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if num_cols == 0 {
            return;
        }

        // Calculate column widths from cell content.
        let mut col_widths = vec![0usize; num_cols];
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                let cell_len: usize = cell.iter().map(|s| s.content.len()).sum();
                if cell_len > col_widths[i] {
                    col_widths[i] = cell_len;
                }
            }
        }

        // Clamp total width.
        let total: usize = col_widths.iter().sum::<usize>() + (num_cols + 1) * 3;
        let max_width = self.width as usize;
        if total > max_width && max_width > num_cols * 4 {
            let available = max_width.saturating_sub((num_cols + 1) * 3);
            let scale = available as f64 / col_widths.iter().sum::<usize>() as f64;
            for w in &mut col_widths {
                *w = ((*w as f64 * scale) as usize).max(1);
            }
        }

        let dim = Style::default().fg(Color::DarkGray);

        for (row_idx, row) in rows.iter().enumerate() {
            let mut line_spans: Vec<Span<'static>> = Vec::new();
            line_spans.push(Span::styled("\u{2502} ".to_string(), dim));

            for (col_idx, cell) in row.iter().enumerate() {
                let cell_text: String = cell.iter().map(|s| s.content.to_string()).collect();
                let width = col_widths.get(col_idx).copied().unwrap_or(0);
                let padded = format!("{:<width$}", cell_text, width = width);

                if row_idx == 0 {
                    // Header row: bold.
                    line_spans.push(Span::styled(
                        padded,
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                } else {
                    // Copy styling from original spans if single span, else use default.
                    let style = if cell.len() == 1 {
                        cell[0].style
                    } else {
                        Style::default()
                    };
                    line_spans.push(Span::styled(padded, style));
                }

                if col_idx < num_cols - 1 {
                    line_spans.push(Span::styled(" \u{2502} ".to_string(), dim));
                }
            }

            line_spans.push(Span::styled(" \u{2502}".to_string(), dim));
            self.push_line(Line::from(line_spans));

            // Separator after header.
            if row_idx == 0 {
                let sep: String = col_widths
                    .iter()
                    .map(|w| "\u{2500}".repeat(*w))
                    .collect::<Vec<_>>()
                    .join("\u{2500}\u{253c}\u{2500}");
                let sep_line = format!("\u{251c}\u{2500}{sep}\u{2500}\u{2524}");
                self.push_line(Line::from(Span::styled(sep_line, dim)));
            }
        }
    }
}

// ── Syntax highlighting ─────────────────────────────────

/// Highlight a single line of code based on language.
fn highlight_line(line: &str, lang: Option<&str>) -> Vec<Span<'static>> {
    match lang {
        Some("js" | "javascript" | "ts" | "typescript" | "jsx" | "tsx") => highlight_js_ts(line),
        Some("py" | "python") => highlight_python(line),
        Some("bash" | "sh" | "zsh" | "shell") => highlight_bash(line),
        Some("json" | "jsonc") => highlight_json(line),
        Some("rust" | "rs") => highlight_rust(line),
        _ => vec![Span::raw(line.to_string())],
    }
}

/// Configuration for the simple syntax highlighter.
struct HighlightConfig<'a> {
    keywords: &'a [&'a str],
    single_line_comment: &'a str,
    string_delims: &'a [char],
    keyword_color: Color,
    string_color: Color,
    number_color: Color,
    comment_color: Color,
}

/// Tokenize a line into classified spans for syntax highlighting.
///
/// This is a simple lexer that handles: keywords, strings, numbers, and comments.
/// It avoids regex for performance and simplicity.
fn tokenize_and_highlight(line: &str, config: &HighlightConfig) -> Vec<Span<'static>> {
    let keywords = config.keywords;
    let single_line_comment = config.single_line_comment;
    let string_delims = config.string_delims;
    let keyword_color = config.keyword_color;
    let string_color = config.string_color;
    let number_color = config.number_color;
    let comment_color = config.comment_color;
    let mut spans = Vec::new();
    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;

    // Pre-compute comment prefix chars to avoid O(N²) String allocation.
    let comment_chars: Vec<char> = single_line_comment.chars().collect();
    let comment_len = comment_chars.len();

    while i < len {
        // Check for single-line comment.
        if comment_len > 0
            && len - i >= comment_len
            && chars[i..i + comment_len] == comment_chars[..]
        {
            let remaining: String = chars[i..].iter().collect();
            spans.push(Span::styled(remaining, Style::default().fg(comment_color)));
            return spans;
        }

        // Check for string.
        if string_delims.contains(&chars[i]) {
            let delim = chars[i];
            let mut s = String::new();
            s.push(delim);
            i += 1;
            while i < len {
                s.push(chars[i]);
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                    s.push(chars[i]);
                } else if chars[i] == delim {
                    i += 1;
                    break;
                }
                i += 1;
            }
            spans.push(Span::styled(s, Style::default().fg(string_color)));
            continue;
        }

        // Check for number (digit or negative sign followed by digit).
        if chars[i].is_ascii_digit()
            || (chars[i] == '-' && i + 1 < len && chars[i + 1].is_ascii_digit())
        {
            let mut s = String::new();
            if chars[i] == '-' {
                s.push('-');
                i += 1;
            }
            while i < len
                && (chars[i].is_ascii_digit()
                    || chars[i] == '.'
                    || chars[i] == 'x'
                    || chars[i] == 'e'
                    || chars[i] == 'E')
            {
                s.push(chars[i]);
                i += 1;
            }
            spans.push(Span::styled(s, Style::default().fg(number_color)));
            continue;
        }

        // Check for keyword (word boundary required).
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' {
            let mut word = String::new();
            let start = i;
            while i < len && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                word.push(chars[i]);
                i += 1;
            }
            // Check word boundary: previous char must not be alphanumeric.
            let prev_ok = start == 0 || !chars[start - 1].is_ascii_alphanumeric();
            let next_ok = i >= len || !chars[i].is_ascii_alphanumeric();
            if prev_ok && next_ok && keywords.contains(&word.as_str()) {
                spans.push(Span::styled(word, Style::default().fg(keyword_color)));
            } else {
                spans.push(Span::raw(word));
            }
            continue;
        }

        // Default: emit character as-is.
        spans.push(Span::raw(chars[i].to_string()));
        i += 1;
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }

    spans
}

fn highlight_js_ts(line: &str) -> Vec<Span<'static>> {
    tokenize_and_highlight(
        line,
        &HighlightConfig {
            keywords: &[
                "const",
                "let",
                "var",
                "function",
                "return",
                "if",
                "else",
                "for",
                "while",
                "async",
                "await",
                "class",
                "new",
                "this",
                "import",
                "export",
                "from",
                "default",
                "switch",
                "case",
                "break",
                "continue",
                "try",
                "catch",
                "finally",
                "throw",
                "typeof",
                "instanceof",
                "in",
                "of",
                "yield",
                "void",
                "delete",
                "true",
                "false",
                "null",
                "undefined",
            ],
            single_line_comment: "//",
            string_delims: &['"', '\'', '`'],
            keyword_color: Color::Cyan,
            string_color: Color::Green,
            number_color: Color::Yellow,
            comment_color: Color::DarkGray,
        },
    )
}

fn highlight_python(line: &str) -> Vec<Span<'static>> {
    tokenize_and_highlight(
        line,
        &HighlightConfig {
            keywords: &[
                "def", "class", "import", "from", "return", "if", "elif", "else", "for", "while",
                "with", "as", "try", "except", "finally", "raise", "pass", "break", "continue",
                "yield", "lambda", "and", "or", "not", "in", "is", "None", "True", "False",
                "global", "nonlocal", "assert", "del", "async", "await",
            ],
            single_line_comment: "#",
            string_delims: &['"', '\''],
            keyword_color: Color::Cyan,
            string_color: Color::Green,
            number_color: Color::Yellow,
            comment_color: Color::DarkGray,
        },
    )
}

fn highlight_bash(line: &str) -> Vec<Span<'static>> {
    tokenize_and_highlight(
        line,
        &HighlightConfig {
            keywords: &[
                "if", "then", "else", "elif", "fi", "for", "do", "done", "while", "until", "case",
                "esac", "in", "function", "return", "local", "export", "source", "echo", "exit",
                "set", "unset", "readonly", "shift", "trap", "eval", "exec", "true", "false",
            ],
            single_line_comment: "#",
            string_delims: &['"', '\''],
            keyword_color: Color::Cyan,
            string_color: Color::Green,
            number_color: Color::Yellow,
            comment_color: Color::DarkGray,
        },
    )
}

fn highlight_json(line: &str) -> Vec<Span<'static>> {
    // JSON is special: keys are strings before ":", values are strings/numbers/booleans.
    let trimmed = line.trim();
    let mut spans = Vec::new();

    // Leading whitespace.
    let leading: String = line.chars().take_while(|c| c.is_whitespace()).collect();
    if !leading.is_empty() {
        spans.push(Span::raw(leading));
    }

    // Try to detect key: value pattern.
    // A JSON key line looks like: "key": value
    if let Some(colon_pos) = trimmed
        .starts_with('"')
        .then(|| find_json_colon(trimmed))
        .flatten()
    {
        let key_part = &trimmed[..colon_pos];
        let rest = &trimmed[colon_pos..];

        // Key in cyan.
        spans.push(Span::styled(
            key_part.to_string(),
            Style::default().fg(Color::Cyan),
        ));

        // Colon.
        spans.push(Span::raw(":".to_string()));
        let after_colon = &rest[1..];

        // Value.
        let val = after_colon.trim();
        let val_leading: String = after_colon
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        if !val_leading.is_empty() {
            spans.push(Span::raw(val_leading));
        }

        spans.extend(highlight_json_value(val));
        return spans;
    }

    // Not a key:value line — could be a value, bracket, etc.
    spans.extend(highlight_json_value(trimmed));
    spans
}

/// Find the colon that separates a JSON key from its value, skipping the key string.
fn find_json_colon(s: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in s.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == '\\' && in_string {
            escaped = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if c == ':' && !in_string {
            return Some(i);
        }
    }
    None
}

/// Highlight a JSON value (string, number, boolean, null, brackets).
fn highlight_json_value(val: &str) -> Vec<Span<'static>> {
    let stripped = val.trim_end_matches(',');
    let trailing = if val.ends_with(',') { "," } else { "" };

    let span = if stripped.starts_with('"') {
        Span::styled(stripped.to_string(), Style::default().fg(Color::Green))
    } else if stripped == "true" || stripped == "false" || stripped == "null" {
        Span::styled(stripped.to_string(), Style::default().fg(Color::Magenta))
    } else if stripped.parse::<f64>().is_ok() {
        Span::styled(stripped.to_string(), Style::default().fg(Color::Yellow))
    } else {
        Span::raw(stripped.to_string())
    };

    let mut spans = vec![span];
    if !trailing.is_empty() {
        spans.push(Span::raw(trailing.to_string()));
    }
    spans
}

fn highlight_rust(line: &str) -> Vec<Span<'static>> {
    tokenize_and_highlight(
        line,
        &HighlightConfig {
            keywords: &[
                "fn", "let", "mut", "pub", "struct", "enum", "impl", "use", "mod", "match", "if",
                "else", "for", "while", "loop", "return", "async", "await", "move", "self", "Self",
                "super", "crate", "trait", "type", "where", "const", "static", "ref", "in", "as",
                "unsafe", "extern", "dyn", "true", "false", "break", "continue", "yield",
            ],
            single_line_comment: "//",
            string_delims: &['"', '\''],
            keyword_color: Color::Cyan,
            string_color: Color::Green,
            number_color: Color::Yellow,
            comment_color: Color::DarkGray,
        },
    )
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Basic rendering ─────────────────────────────

    #[test]
    fn empty_input_returns_empty() {
        let lines = render_markdown("", 80);
        assert!(lines.is_empty());
    }

    #[test]
    fn plain_text_renders_as_single_paragraph() {
        let lines = render_markdown("Hello, world!", 80);
        assert!(!lines.is_empty());
        let text: String = lines[0]
            .spans
            .iter()
            .map(|s| s.content.to_string())
            .collect();
        assert_eq!(text, "Hello, world!");
    }

    // ── Headings ────────────────────────────────────

    #[test]
    fn h1_is_cyan_bold() {
        let lines = render_markdown("# Heading One", 80);
        assert!(!lines.is_empty());
        let span = &lines[0].spans[0];
        assert!(span.content.contains("Heading One"));
        assert_eq!(span.style.fg, Some(Color::Cyan));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn h2_is_bold() {
        let lines = render_markdown("## Heading Two", 80);
        assert!(!lines.is_empty());
        let span = &lines[0].spans[0];
        assert!(span.content.contains("Heading Two"));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn h3_is_underlined() {
        let lines = render_markdown("### Heading Three", 80);
        assert!(!lines.is_empty());
        let span = &lines[0].spans[0];
        assert!(span.content.contains("Heading Three"));
        assert!(span.style.add_modifier.contains(Modifier::UNDERLINED));
    }

    // ── Inline formatting ───────────────────────────

    #[test]
    fn inline_code_is_cyan() {
        let lines = render_markdown("Use `cargo build` to compile.", 80);
        let all_spans: Vec<&Span> = lines.iter().flat_map(|l| l.spans.iter()).collect();
        let code_span = all_spans.iter().find(|s| s.content.contains("cargo build"));
        assert!(code_span.is_some());
        assert_eq!(code_span.unwrap().style.fg, Some(Color::Cyan));
    }

    #[test]
    fn bold_text_has_bold_modifier() {
        let lines = render_markdown("This is **bold** text.", 80);
        let all_spans: Vec<&Span> = lines.iter().flat_map(|l| l.spans.iter()).collect();
        let bold_span = all_spans.iter().find(|s| s.content.contains("bold"));
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
    fn italic_text_has_italic_modifier() {
        let lines = render_markdown("This is *italic* text.", 80);
        let all_spans: Vec<&Span> = lines.iter().flat_map(|l| l.spans.iter()).collect();
        let italic_span = all_spans.iter().find(|s| s.content.contains("italic"));
        assert!(italic_span.is_some());
        assert!(
            italic_span
                .unwrap()
                .style
                .add_modifier
                .contains(Modifier::ITALIC)
        );
    }

    #[test]
    fn strikethrough_text_is_dim() {
        let lines = render_markdown("This is ~~deleted~~ text.", 80);
        let all_spans: Vec<&Span> = lines.iter().flat_map(|l| l.spans.iter()).collect();
        let strike_span = all_spans.iter().find(|s| s.content.contains("deleted"));
        assert!(strike_span.is_some());
        assert_eq!(strike_span.unwrap().style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn link_is_blue_underline() {
        let lines = render_markdown("[Click here](https://example.com)", 80);
        let all_spans: Vec<&Span> = lines.iter().flat_map(|l| l.spans.iter()).collect();
        let link_span = all_spans.iter().find(|s| s.content.contains("Click here"));
        assert!(link_span.is_some());
        assert_eq!(link_span.unwrap().style.fg, Some(Color::Blue));
        assert!(
            link_span
                .unwrap()
                .style
                .add_modifier
                .contains(Modifier::UNDERLINED)
        );
    }

    #[test]
    fn link_shows_url() {
        let lines = render_markdown("[Click](https://example.com)", 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains("https://example.com"));
    }

    // ── Code blocks ─────────────────────────────────

    #[test]
    fn fenced_code_block_has_borders() {
        let md = "```rust\nfn main() {}\n```";
        let lines = render_markdown(md, 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // Should contain the box-drawing top and bottom borders.
        assert!(full_text.contains('\u{256d}')); // top-left corner
        assert!(full_text.contains('\u{2570}')); // bottom-left corner
        // Should contain the language label.
        assert!(full_text.contains("rust"));
    }

    #[test]
    fn code_block_without_lang_renders() {
        let md = "```\nhello\n```";
        let lines = render_markdown(md, 80);
        assert!(!lines.is_empty());
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains("hello"));
    }

    // ── Lists ───────────────────────────────────────

    #[test]
    fn unordered_list_has_bullet_prefix() {
        let md = "- First\n- Second\n- Third";
        let lines = render_markdown(md, 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains('\u{2022}')); // bullet character
        assert!(full_text.contains("First"));
        assert!(full_text.contains("Second"));
    }

    #[test]
    fn ordered_list_has_number_prefix() {
        let md = "1. Alpha\n2. Beta\n3. Gamma";
        let lines = render_markdown(md, 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains("1."));
        assert!(full_text.contains("Alpha"));
        assert!(full_text.contains("2."));
        assert!(full_text.contains("Beta"));
    }

    #[test]
    fn task_list_renders_checkboxes() {
        let md = "- [ ] Unchecked\n- [x] Checked";
        let lines = render_markdown(md, 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains('\u{2610}')); // unchecked
        assert!(full_text.contains('\u{2611}')); // checked
    }

    // ── Blockquotes ─────────────────────────────────

    #[test]
    fn blockquote_has_bar_prefix() {
        let md = "> This is a quote.";
        let lines = render_markdown(md, 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains('\u{2502}')); // vertical bar
        assert!(full_text.contains("This is a quote."));
    }

    // ── Horizontal rule ─────────────────────────────

    #[test]
    fn horizontal_rule_renders() {
        let md = "Above\n\n---\n\nBelow";
        let lines = render_markdown(md, 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains('\u{2500}')); // horizontal line
    }

    // ── Tables ──────────────────────────────────────

    #[test]
    fn simple_table_renders() {
        let md = "| Name | Value |\n|------|-------|\n| foo  | 42    |";
        let lines = render_markdown(md, 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains("Name"));
        assert!(full_text.contains("foo"));
        assert!(full_text.contains("42"));
    }

    // ── Syntax highlighting ─────────────────────────

    #[test]
    fn js_keywords_are_cyan() {
        let spans = highlight_js_ts("const x = 42;");
        let kw_span = spans.iter().find(|s| s.content == "const");
        assert!(kw_span.is_some());
        assert_eq!(kw_span.unwrap().style.fg, Some(Color::Cyan));
    }

    #[test]
    fn js_strings_are_green() {
        let spans = highlight_js_ts("let name = \"hello\";");
        let str_span = spans.iter().find(|s| s.content.contains("hello"));
        assert!(str_span.is_some());
        assert_eq!(str_span.unwrap().style.fg, Some(Color::Green));
    }

    #[test]
    fn js_numbers_are_yellow() {
        let spans = highlight_js_ts("const x = 42;");
        let num_span = spans.iter().find(|s| s.content == "42");
        assert!(num_span.is_some());
        assert_eq!(num_span.unwrap().style.fg, Some(Color::Yellow));
    }

    #[test]
    fn js_comments_are_dim() {
        let spans = highlight_js_ts("// this is a comment");
        assert!(!spans.is_empty());
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn python_keywords_are_cyan() {
        let spans = highlight_python("def hello():");
        let kw_span = spans.iter().find(|s| s.content == "def");
        assert!(kw_span.is_some());
        assert_eq!(kw_span.unwrap().style.fg, Some(Color::Cyan));
    }

    #[test]
    fn python_comments_are_dim() {
        let spans = highlight_python("# comment");
        assert!(!spans.is_empty());
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn bash_keywords_are_cyan() {
        let spans = highlight_bash("if [ -f file ]; then");
        let kw_span = spans.iter().find(|s| s.content == "if");
        assert!(kw_span.is_some());
        assert_eq!(kw_span.unwrap().style.fg, Some(Color::Cyan));
    }

    #[test]
    fn json_keys_are_cyan() {
        let spans = highlight_json("  \"name\": \"value\"");
        let key_span = spans.iter().find(|s| s.content.contains("name"));
        assert!(key_span.is_some());
        assert_eq!(key_span.unwrap().style.fg, Some(Color::Cyan));
    }

    #[test]
    fn json_string_values_are_green() {
        let spans = highlight_json("  \"name\": \"value\"");
        let val_span = spans.iter().find(|s| s.content.contains("value"));
        assert!(val_span.is_some());
        assert_eq!(val_span.unwrap().style.fg, Some(Color::Green));
    }

    #[test]
    fn json_numbers_are_yellow() {
        let spans = highlight_json("  \"count\": 42");
        let num_span = spans.iter().find(|s| s.content.contains("42"));
        assert!(num_span.is_some());
        assert_eq!(num_span.unwrap().style.fg, Some(Color::Yellow));
    }

    #[test]
    fn json_booleans_are_magenta() {
        let spans = highlight_json("  \"active\": true");
        let bool_span = spans.iter().find(|s| s.content.contains("true"));
        assert!(bool_span.is_some());
        assert_eq!(bool_span.unwrap().style.fg, Some(Color::Magenta));
    }

    #[test]
    fn json_null_is_magenta() {
        let spans = highlight_json("  \"data\": null");
        let null_span = spans.iter().find(|s| s.content.contains("null"));
        assert!(null_span.is_some());
        assert_eq!(null_span.unwrap().style.fg, Some(Color::Magenta));
    }

    #[test]
    fn rust_keywords_are_cyan() {
        let spans = highlight_rust("fn main() {}");
        let kw_span = spans.iter().find(|s| s.content == "fn");
        assert!(kw_span.is_some());
        assert_eq!(kw_span.unwrap().style.fg, Some(Color::Cyan));
    }

    #[test]
    fn rust_comments_are_dim() {
        let spans = highlight_rust("// a rust comment");
        assert!(!spans.is_empty());
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn rust_strings_are_green() {
        let spans = highlight_rust("let s = \"hello\";");
        let str_span = spans.iter().find(|s| s.content.contains("hello"));
        assert!(str_span.is_some());
        assert_eq!(str_span.unwrap().style.fg, Some(Color::Green));
    }

    // ── Integration: code block with syntax highlighting ──

    #[test]
    fn rust_code_block_highlights_keywords() {
        let md = "```rust\nfn main() {\n    let x = 42;\n}\n```";
        let lines = render_markdown(md, 80);
        let all_spans: Vec<&Span> = lines.iter().flat_map(|l| l.spans.iter()).collect();
        // Should find the "fn" keyword highlighted in cyan.
        let fn_span = all_spans.iter().find(|s| s.content == "fn");
        assert!(fn_span.is_some());
        assert_eq!(fn_span.unwrap().style.fg, Some(Color::Cyan));
    }

    #[test]
    fn js_code_block_highlights() {
        let md = "```javascript\nconst x = \"hello\";\n```";
        let lines = render_markdown(md, 80);
        let all_spans: Vec<&Span> = lines.iter().flat_map(|l| l.spans.iter()).collect();
        let const_span = all_spans.iter().find(|s| s.content == "const");
        assert!(const_span.is_some());
        assert_eq!(const_span.unwrap().style.fg, Some(Color::Cyan));
    }

    // ── Width parameter ─────────────────────────────

    #[test]
    fn width_affects_horizontal_rule() {
        let lines_40 = render_markdown("---", 40);
        let lines_80 = render_markdown("---", 80);
        let rule_40: String = lines_40
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        let rule_80: String = lines_80
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        // The 80-width rule should be longer.
        assert!(rule_80.len() > rule_40.len());
    }

    // ── Nested formatting ───────────────────────────

    #[test]
    fn nested_bold_italic() {
        let lines = render_markdown("***bold italic***", 80);
        let all_spans: Vec<&Span> = lines.iter().flat_map(|l| l.spans.iter()).collect();
        let text_span = all_spans.iter().find(|s| s.content.contains("bold italic"));
        assert!(text_span.is_some());
        let style = text_span.unwrap().style;
        assert!(style.add_modifier.contains(Modifier::BOLD));
        assert!(style.add_modifier.contains(Modifier::ITALIC));
    }

    // ── Edge cases ──────────────────────────────────

    #[test]
    fn multiple_paragraphs() {
        let md = "First paragraph.\n\nSecond paragraph.";
        let lines = render_markdown(md, 80);
        // Should have a blank line between paragraphs.
        assert!(lines.len() >= 3);
    }

    #[test]
    fn nested_list_indentation() {
        let md = "- Outer\n  - Inner";
        let lines = render_markdown(md, 80);
        let full_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.to_string())
            .collect();
        assert!(full_text.contains("Outer"));
        assert!(full_text.contains("Inner"));
    }

    #[test]
    fn highlight_unknown_lang_returns_raw() {
        let spans = highlight_line("hello world", Some("brainfuck"));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "hello world");
        assert_eq!(spans[0].style, Style::default());
    }

    #[test]
    fn highlight_no_lang_returns_raw() {
        let spans = highlight_line("hello world", None);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "hello world");
    }

    #[test]
    fn escaped_string_in_js() {
        let spans = highlight_js_ts(r#"const s = "he\"llo";"#);
        let str_span = spans.iter().find(|s| s.content.contains("he\\\"llo"));
        assert!(str_span.is_some());
        assert_eq!(str_span.unwrap().style.fg, Some(Color::Green));
    }
}
