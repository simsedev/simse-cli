//! @-Mention autocomplete: file path, VFS path, and volume ID completion triggered by `@` prefix.
//!
//! When the user types `@` followed by characters, the autocomplete activates and
//! scans the filesystem (or matches VFS / volume ID patterns) in real-time. It presents
//! up to 8 matching entries in a popup above the input area, supports keyboard navigation
//! (Up/Down), and Tab/Enter to accept or Escape to cancel.
//!
//! # Layout
//!
//! ```text
//! +-- Files ------------------------------------------------+
//! |  > src/                                                  |
//! |    Cargo.toml                                            |
//! |    README.md                                             |
//! +----------------------------------------------------------+
//! +-- Input ------------------------------------------------+
//! | @sr|                                                     |
//! +----------------------------------------------------------+
//! ```
//!
//! # Mention kinds
//!
//! - **File paths**: `@src/main.rs` — resolved via `std::fs::read_dir`
//! - **VFS paths**: `@vfs://workspace/file.txt` — prefixed with `vfs://`
//! - **Volume IDs**: `@a1b2c3d4` — 8-char hex prefix matched against library volumes
//!
//! # Directory traversal
//!
//! Paths ending in `/` keep the autocomplete active and scan the subdirectory,
//! allowing the user to browse deeper into the file tree incrementally.
//!
//! # Integration
//!
//! The @-mention state lives alongside the `App` model. The `app.rs` update
//! function delegates key events when `is_active()` returns true:
//!
//! - **Escape** calls `deactivate()`
//! - **Up/Down** calls `move_up()` / `move_down()`
//! - **Tab/Enter** calls `accept()` and inserts the value into input
//! - **CharInput/Backspace** calls `update()` after input mutation

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

// ── Constants ───────────────────────────────────────────

/// Maximum number of entries shown in the @-mention popup.
const MAX_VISIBLE_ENTRIES: usize = 8;

/// Directories excluded from filesystem scanning.
const EXCLUDED_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "dist",
    "build",
    "target",
    ".cache",
    "__pycache__",
];

// ── VFS Cache ───────────────────────────────────────────

/// Cached VFS entries populated by the event loop. The `complete_vfs` function
/// reads from this cache to provide VFS path completions synchronously, since
/// the @-mention autocomplete runs on the main UI thread and cannot perform
/// async I/O to the VFS subprocess.
static VFS_CACHE: LazyLock<Mutex<Vec<MentionEntry>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Populate the VFS cache with a fresh list of entries.
///
/// Call this from the event loop whenever the VFS file list is refreshed
/// (e.g. on startup, after file mutations, or on a periodic timer).
/// The entries should have `display` set to the VFS path segment shown in the
/// popup (e.g. `"workspace/"` or `"workspace/file.txt"`) and `value` set to
/// the full `vfs://` URI to insert (e.g. `"vfs://workspace/file.txt"`).
pub fn set_vfs_cache(entries: Vec<MentionEntry>) {
    if let Ok(mut cache) = VFS_CACHE.lock() {
        *cache = entries;
    }
}

/// Clear the VFS cache. Exposed for test isolation.
#[cfg(test)]
fn clear_vfs_cache() {
    if let Ok(mut cache) = VFS_CACHE.lock() {
        cache.clear();
    }
}

// ── MentionEntry ────────────────────────────────────────

/// A single matching entry for the @-mention autocomplete popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MentionEntry {
    /// Display text shown in the popup (e.g. "src/", "Cargo.toml").
    pub display: String,
    /// Value inserted into the input when accepted (e.g. "src/", "Cargo.toml").
    pub value: String,
    /// Whether this entry represents a directory.
    pub is_directory: bool,
}

// ── AtMentionState ──────────────────────────────────────

/// State for the @-mention autocomplete popup.
///
/// Tracks the current prefix after `@`, filtered filesystem entries,
/// selection index, and whether the popup is currently active.
#[derive(Debug, Clone)]
pub struct AtMentionState {
    /// The current prefix text after `@` that drives the autocomplete.
    pub prefix: String,
    /// Filtered mention entries for the current prefix.
    pub entries: Vec<MentionEntry>,
    /// Index of the currently highlighted entry.
    pub selected: usize,
    /// Whether the @-mention autocomplete is currently active (popup visible).
    pub active: bool,
    /// Base directory for filesystem scanning. Defaults to current directory.
    base_dir: Option<PathBuf>,
}

impl AtMentionState {
    /// Create a new inactive @-mention state.
    pub fn new() -> Self {
        Self {
            prefix: String::new(),
            entries: Vec::new(),
            selected: 0,
            active: false,
            base_dir: None,
        }
    }

    /// Create a new inactive @-mention state with a specific base directory
    /// for filesystem scanning.
    #[cfg(test)]
    fn with_base_dir(base_dir: &Path) -> Self {
        Self {
            prefix: String::new(),
            entries: Vec::new(),
            selected: 0,
            active: false,
            base_dir: Some(base_dir.to_path_buf()),
        }
    }

    /// Activate the @-mention autocomplete with the given prefix (text after `@`).
    ///
    /// Scans the filesystem starting from the base directory (or cwd), filters
    /// results, and activates the popup if there are matches.
    /// Returns the updated state (owned-return pattern).
    pub fn activate(mut self, prefix: &str) -> Self {
        self.prefix = prefix.to_string();
        self.entries = resolve_entries(prefix, self.base_dir.as_deref());
        self.selected = 0;
        self.active = !self.entries.is_empty();
        self
    }

    /// Deactivate the @-mention autocomplete: reset to inactive state.
    /// Returns the updated state (owned-return pattern).
    pub fn deactivate(mut self) -> Self {
        self.prefix.clear();
        self.entries.clear();
        self.selected = 0;
        self.active = false;
        self
    }

    /// Re-scan entries as the user types. If there are no matches, the
    /// autocomplete deactivates.
    /// Returns the updated state (owned-return pattern).
    pub fn update(mut self, prefix: &str) -> Self {
        self.prefix = prefix.to_string();
        self.entries = resolve_entries(prefix, self.base_dir.as_deref());

        if self.entries.is_empty() {
            self.active = false;
        } else {
            self.active = true;
            // Clamp selection to new bounds.
            if self.selected >= self.entries.len() {
                self.selected = self.entries.len() - 1;
            }
        }
        self
    }

    /// Move selection up by one (wrapping to bottom when at top).
    /// Returns the updated state (owned-return pattern).
    pub fn move_up(mut self) -> Self {
        if !self.entries.is_empty() {
            if self.selected == 0 {
                self.selected = self.entries.len() - 1;
            } else {
                self.selected -= 1;
            }
        }
        self
    }

    /// Move selection down by one (wrapping to top when at bottom).
    /// Returns the updated state (owned-return pattern).
    pub fn move_down(mut self) -> Self {
        if !self.entries.is_empty() {
            if self.selected + 1 >= self.entries.len() {
                self.selected = 0;
            } else {
                self.selected += 1;
            }
        }
        self
    }

    /// Accept the currently selected entry. Returns `(updated_state, value)` where
    /// `value` is the value string to insert into the input (e.g. `"src/"` or
    /// `"Cargo.toml"`), or `None` if no entries are available.
    ///
    /// If the accepted entry is a directory, the autocomplete stays active and
    /// re-scans the subdirectory contents.
    pub fn accept(self) -> (Self, Option<String>) {
        if self.entries.is_empty() {
            return (self, None);
        }
        let entry = self.entries[self.selected].clone();
        let value = entry.value.clone();

        let new_self = if entry.is_directory {
            // Keep mode active: re-scan subdirectory.
            self.update(&value)
        } else {
            self.deactivate()
        };

        (new_self, Some(value))
    }

    /// Whether the @-mention autocomplete is currently active (popup should be visible).
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Return the visible entries (up to `MAX_VISIBLE_ENTRIES`).
    pub fn visible_entries(&self) -> &[MentionEntry] {
        let end = self.entries.len().min(MAX_VISIBLE_ENTRIES);
        &self.entries[..end]
    }
}

impl Default for AtMentionState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Entry resolution ────────────────────────────────────

/// Resolve @-mention entries based on the prefix.
///
/// Dispatches to VFS completion, volume ID completion, or filesystem scanning
/// depending on the prefix pattern.
fn resolve_entries(prefix: &str, base_dir: Option<&Path>) -> Vec<MentionEntry> {
    if prefix.starts_with("vfs://") {
        return complete_vfs(prefix);
    }

    // 8-char hex prefix -> volume ID completion
    if is_volume_id_prefix(prefix) {
        // Volume ID completion would be handled by the bridge/library layer.
        // Return empty for now; the infrastructure will be wired up later.
        return Vec::new();
    }

    scan_directory(prefix, base_dir)
}

/// Check if a string looks like a volume ID prefix (1-8 lowercase hex chars, no slashes/dots).
fn is_volume_id_prefix(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 8
        && !s.contains('/')
        && !s.contains('\\')
        && !s.contains('.')
        && s.chars().all(|c| c.is_ascii_hexdigit())
        && s.chars().any(|c| c.is_ascii_alphabetic())
}

/// Provide VFS path completions for `vfs://` prefixed mentions.
///
/// Reads from the `VFS_CACHE` populated by the event loop and filters entries
/// whose `display` or `value` fields match the given prefix (case-insensitive).
fn complete_vfs(prefix: &str) -> Vec<MentionEntry> {
    let cache = match VFS_CACHE.lock() {
        Ok(guard) => guard,
        Err(_) => return Vec::new(),
    };

    let lower_prefix = prefix.to_lowercase();

    cache
        .iter()
        .filter(|entry| {
            entry.display.to_lowercase().starts_with(&lower_prefix)
                || entry.value.to_lowercase().starts_with(&lower_prefix)
        })
        .cloned()
        .collect()
}

// ── Filesystem scanning ─────────────────────────────────

/// Scan the filesystem for entries matching the given prefix.
///
/// The prefix is treated as a partial path relative to the base directory
/// (defaults to the current working directory if `base_dir` is `None`).
/// If the prefix contains a `/`, the directory portion is used as the search
/// root, and the remainder is used as the filename prefix filter.
///
/// Results are sorted: directories first, then files, both alphabetically.
pub fn scan_directory(prefix: &str, base_dir: Option<&Path>) -> Vec<MentionEntry> {
    // Normalize backslashes to forward slashes for consistency.
    let normalized = prefix.replace('\\', "/");

    // Split into directory part and filename prefix.
    let (dir_part, name_prefix) = if normalized.ends_with('/') {
        (normalized.as_str(), "")
    } else if let Some(slash_pos) = normalized.rfind('/') {
        (&normalized[..=slash_pos], &normalized[slash_pos + 1..])
    } else {
        ("", normalized.as_str())
    };

    // Build the absolute search path from base_dir + dir_part.
    let search_path = match base_dir {
        Some(base) => {
            if dir_part.is_empty() {
                base.to_path_buf()
            } else {
                base.join(dir_part)
            }
        }
        None => {
            if dir_part.is_empty() {
                PathBuf::from(".")
            } else {
                PathBuf::from(dir_part)
            }
        }
    };

    let entries = match std::fs::read_dir(&search_path) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    for entry in entries.flatten() {
        let name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(_) => continue,
        };

        // Skip excluded directories.
        if is_excluded(&name) {
            continue;
        }

        // Skip hidden files/dirs (starting with .).
        if name.starts_with('.') {
            continue;
        }

        // Apply prefix filter.
        if !name_prefix.is_empty() && !matches_prefix(&name, name_prefix) {
            continue;
        }

        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);

        // Build the display and value paths.
        let rel_path = if dir_part.is_empty() {
            name.clone()
        } else {
            format!("{}{}", dir_part, name)
        };

        if is_dir {
            dirs.push(MentionEntry {
                display: format!("{}/", name),
                value: format!("{}/", rel_path),
                is_directory: true,
            });
        } else {
            files.push(MentionEntry {
                display: name,
                value: rel_path,
                is_directory: false,
            });
        }
    }

    // Sort alphabetically within each group.
    dirs.sort_by(|a, b| a.display.to_lowercase().cmp(&b.display.to_lowercase()));
    files.sort_by(|a, b| a.display.to_lowercase().cmp(&b.display.to_lowercase()));

    // Directories first, then files.
    dirs.extend(files);
    dirs
}

/// Check whether a filename matches the exclusion list.
pub fn is_excluded(name: &str) -> bool {
    EXCLUDED_DIRS.contains(&name)
}

/// Check whether an entry name matches a prefix (case-insensitive).
pub fn matches_prefix(entry: &str, prefix: &str) -> bool {
    entry.to_lowercase().starts_with(&prefix.to_lowercase())
}

// ── Render ──────────────────────────────────────────────

/// Render the @-mention autocomplete popup above the input area.
///
/// `area` should be the region *above* the input line where the popup can
/// appear. The popup is anchored to the bottom of this area and grows upward.
///
/// The popup shows up to `MAX_VISIBLE_ENTRIES` entries, each formatted as:
/// `  > name/` (directory) or `    name` (file)
///
/// The selected entry is highlighted in cyan with a `>` prefix. Directories
/// are shown with a trailing `/` and a folder indicator.
pub fn render_at_mention_popup(frame: &mut Frame, area: Rect, state: &AtMentionState) {
    if !state.is_active() {
        return;
    }

    let visible = state.visible_entries();
    if visible.is_empty() {
        return;
    }

    // Calculate popup dimensions.
    let line_count = visible.len() as u16;
    // +2 for top/bottom border
    let popup_height = (line_count + 2).min(area.height);
    let popup_width = area.width.clamp(30, 60);

    // Anchor to bottom-left of the area (just above the input).
    let popup_y = area.y + area.height.saturating_sub(popup_height);
    let popup_x = area.x;

    let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

    // Build lines.
    let selected_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let normal_style = Style::default().fg(Color::White);
    let dir_style = Style::default().fg(Color::Blue);
    let selected_dir_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let lines: Vec<Line> = visible
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let is_selected = i == state.selected;
            let indicator = if is_selected { " > " } else { "   " };

            let name_style = if is_selected {
                if entry.is_directory {
                    selected_dir_style
                } else {
                    selected_style
                }
            } else if entry.is_directory {
                dir_style
            } else {
                normal_style
            };

            let kind_label = if entry.is_directory { " [dir]" } else { "" };
            let kind_style = Style::default().fg(Color::DarkGray);

            Line::from(vec![
                Span::styled(indicator, name_style),
                Span::styled(entry.display.clone(), name_style),
                Span::styled(kind_label, kind_style),
            ])
        })
        .collect();

    // Clear area behind popup, then render.
    frame.render_widget(Clear, popup_area);
    let popup = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Files "),
    );
    frame.render_widget(popup, popup_area);
}

// ── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Serialize VFS cache tests to prevent race conditions on the global
    /// `VFS_CACHE` static. Each test that touches the cache must acquire
    /// this lock before calling `clear_vfs_cache` / `set_vfs_cache`.
    static VFS_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Helper: create a temporary directory with a known structure for testing.
    fn setup_test_dir() -> TempDir {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let base = dir.path();

        // Create files.
        fs::write(base.join("README.md"), "# readme").unwrap();
        fs::write(base.join("Cargo.toml"), "[package]").unwrap();
        fs::write(base.join("main.rs"), "fn main() {}").unwrap();

        // Create directories.
        fs::create_dir(base.join("src")).unwrap();
        fs::write(base.join("src").join("lib.rs"), "pub mod foo;").unwrap();
        fs::create_dir(base.join("tests")).unwrap();
        fs::write(base.join("tests").join("test_it.rs"), "#[test]").unwrap();

        // Create excluded directories.
        fs::create_dir(base.join("node_modules")).unwrap();
        fs::create_dir(base.join(".git")).unwrap();
        fs::create_dir(base.join("target")).unwrap();

        // Create a hidden file.
        fs::write(base.join(".hidden"), "secret").unwrap();

        dir
    }

    // ── new / default ───────────────────────────────────

    #[test]
    fn new_creates_inactive_state() {
        let state = AtMentionState::new();
        assert!(!state.is_active());
        assert!(state.entries.is_empty());
        assert_eq!(state.selected, 0);
        assert!(state.prefix.is_empty());
    }

    #[test]
    fn default_is_same_as_new() {
        let state = AtMentionState::default();
        assert!(!state.is_active());
    }

    // ── is_excluded ─────────────────────────────────────

    #[test]
    fn excluded_dirs_are_filtered() {
        assert!(is_excluded("node_modules"));
        assert!(is_excluded(".git"));
        assert!(is_excluded("dist"));
        assert!(is_excluded("build"));
        assert!(is_excluded("target"));
        assert!(is_excluded(".cache"));
        assert!(is_excluded("__pycache__"));
    }

    #[test]
    fn non_excluded_dirs_pass() {
        assert!(!is_excluded("src"));
        assert!(!is_excluded("lib"));
        assert!(!is_excluded("docs"));
        assert!(!is_excluded("Cargo.toml"));
    }

    // ── matches_prefix ──────────────────────────────────

    #[test]
    fn prefix_match_case_insensitive() {
        assert!(matches_prefix("Cargo.toml", "car"));
        assert!(matches_prefix("Cargo.toml", "Car"));
        assert!(matches_prefix("Cargo.toml", "CARGO"));
        assert!(matches_prefix("readme.md", "READ"));
    }

    #[test]
    fn prefix_match_no_match() {
        assert!(!matches_prefix("Cargo.toml", "src"));
        assert!(!matches_prefix("main.rs", "lib"));
    }

    #[test]
    fn prefix_match_empty_matches_all() {
        assert!(matches_prefix("anything", ""));
    }

    // ── is_volume_id_prefix ─────────────────────────────

    #[test]
    fn volume_id_prefix_valid() {
        assert!(is_volume_id_prefix("a1b2c3d4"));
        assert!(is_volume_id_prefix("abcd"));
        assert!(is_volume_id_prefix("0a"));
        assert!(is_volume_id_prefix("ff00aa"));
    }

    #[test]
    fn volume_id_prefix_invalid() {
        // Too long.
        assert!(!is_volume_id_prefix("a1b2c3d4e"));
        // Empty.
        assert!(!is_volume_id_prefix(""));
        // Contains slash.
        assert!(!is_volume_id_prefix("ab/cd"));
        // Contains dot.
        assert!(!is_volume_id_prefix("ab.cd"));
        // Non-hex chars.
        assert!(!is_volume_id_prefix("ghij"));
        // Pure digits (no letters) -- not a volume ID.
        assert!(!is_volume_id_prefix("12345678"));
    }

    // ── scan_directory ──────────────────────────────────

    #[test]
    fn scan_lists_files_and_dirs() {
        let dir = setup_test_dir();
        let entries = scan_directory("", Some(dir.path()));

        // Should find: Cargo.toml, main.rs, README.md, src/, tests/
        // Should NOT find: node_modules/, .git/, target/, .hidden
        assert!(!entries.is_empty());

        let names: Vec<&str> = entries.iter().map(|e| e.display.as_str()).collect();

        // Directories should be present.
        assert!(names.contains(&"src/"), "Expected src/ in {:?}", names);
        assert!(names.contains(&"tests/"), "Expected tests/ in {:?}", names);

        // Files should be present.
        assert!(
            names.contains(&"Cargo.toml"),
            "Expected Cargo.toml in {:?}",
            names
        );
        assert!(
            names.contains(&"main.rs"),
            "Expected main.rs in {:?}",
            names
        );
        assert!(
            names.contains(&"README.md"),
            "Expected README.md in {:?}",
            names
        );

        // Excluded dirs should NOT be present.
        assert!(!names.contains(&"node_modules/"));
        assert!(!names.contains(&".git/"));
        assert!(!names.contains(&"target/"));

        // Hidden files should NOT be present.
        assert!(!names.contains(&".hidden"));
    }

    #[test]
    fn scan_dirs_come_first() {
        let dir = setup_test_dir();
        let entries = scan_directory("", Some(dir.path()));

        // All directories should appear before any files.
        let first_file_idx = entries.iter().position(|e| !e.is_directory);
        let last_dir_idx = entries.iter().rposition(|e| e.is_directory);

        if let (Some(first_file), Some(last_dir)) = (first_file_idx, last_dir_idx) {
            assert!(
                last_dir < first_file,
                "Directories should come before files: last_dir={}, first_file={}",
                last_dir,
                first_file
            );
        }
    }

    #[test]
    fn scan_with_prefix_filters() {
        let dir = setup_test_dir();
        let entries = scan_directory("Car", Some(dir.path()));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].display, "Cargo.toml");
        assert!(!entries[0].is_directory);
    }

    #[test]
    fn scan_subdirectory() {
        let dir = setup_test_dir();
        let entries = scan_directory("src/", Some(dir.path()));

        // Should list contents of src/
        assert!(!entries.is_empty());
        let names: Vec<&str> = entries.iter().map(|e| e.display.as_str()).collect();
        assert!(names.contains(&"lib.rs"), "Expected lib.rs in {:?}", names);
    }

    #[test]
    fn scan_subdirectory_with_prefix() {
        let dir = setup_test_dir();
        let entries = scan_directory("src/l", Some(dir.path()));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].display, "lib.rs");
        assert_eq!(entries[0].value, "src/lib.rs");
    }

    #[test]
    fn scan_nonexistent_dir_returns_empty() {
        let entries = scan_directory("nonexistent_dir_12345/", None);
        assert!(entries.is_empty());
    }

    #[test]
    fn scan_prefix_case_insensitive() {
        let dir = setup_test_dir();
        let entries = scan_directory("car", Some(dir.path()));

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].display, "Cargo.toml");
    }

    // ── AtMentionState: activate ────────────────────────

    #[test]
    fn activate_with_matching_prefix() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("Car");

        assert!(state.is_active());
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].display, "Cargo.toml");
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn activate_with_no_matches_stays_inactive() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("zzzzz");

        assert!(!state.is_active());
        assert!(state.entries.is_empty());
    }

    #[test]
    fn activate_empty_prefix_shows_all() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("");

        assert!(state.is_active());
        // Should have: src/, tests/, Cargo.toml, main.rs, README.md = 5 entries
        assert_eq!(state.entries.len(), 5);
    }

    // ── AtMentionState: deactivate ──────────────────────

    #[test]
    fn deactivate_resets_state() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("");
        assert!(state.is_active());

        state = state.deactivate();
        assert!(!state.is_active());
        assert!(state.entries.is_empty());
        assert_eq!(state.selected, 0);
        assert!(state.prefix.is_empty());
    }

    // ── AtMentionState: update ──────────────────────────

    #[test]
    fn update_narrows_results() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("");
        let initial_count = state.entries.len();
        assert!(initial_count > 1);

        state = state.update("Car");
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].display, "Cargo.toml");
    }

    #[test]
    fn update_deactivates_on_no_results() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("");
        assert!(state.is_active());

        state = state.update("zzzzz");
        assert!(!state.is_active());
    }

    #[test]
    fn update_clamps_selected_index() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("");
        assert_eq!(state.entries.len(), 5);
        state.selected = 4; // last of 5

        state = state.update("Car");
        // Only "Cargo.toml" matches now.
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.selected, 0);
    }

    // ── AtMentionState: move_up / move_down ─────────────

    #[test]
    fn move_down_wraps_around() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("");
        let count = state.entries.len();
        assert!(count > 1);

        // Navigate to the end.
        for _ in 0..count - 1 {
            state = state.move_down();
        }
        assert_eq!(state.selected, count - 1);

        // Wrap to 0.
        state = state.move_down();
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn move_up_wraps_around() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("");
        let count = state.entries.len();
        assert!(count > 1);
        assert_eq!(state.selected, 0);

        // Wrap to bottom.
        state = state.move_up();
        assert_eq!(state.selected, count - 1);

        // Move up.
        state = state.move_up();
        assert_eq!(state.selected, count - 2);
    }

    #[test]
    fn move_up_down_noop_when_empty() {
        let mut state = AtMentionState::new();
        state = state.move_up();
        assert_eq!(state.selected, 0);
        state = state.move_down();
        assert_eq!(state.selected, 0);
    }

    // ── AtMentionState: accept ──────────────────────────

    #[test]
    fn accept_returns_selected_file() {
        let dir = setup_test_dir();
        let state = AtMentionState::with_base_dir(dir.path());
        let state = state.activate("Car");
        assert_eq!(state.entries.len(), 1);

        let (state, result) = state.accept();
        assert_eq!(result, Some("Cargo.toml".into()));
        assert!(!state.is_active()); // File acceptance deactivates.
    }

    #[test]
    fn accept_directory_keeps_active() {
        let dir = setup_test_dir();
        let state = AtMentionState::with_base_dir(dir.path());
        let state = state.activate("sr");
        // Should match src/
        assert!(state.is_active());
        assert_eq!(state.entries.len(), 1);
        assert!(state.entries[0].is_directory);

        let (state, result) = state.accept();
        assert_eq!(result, Some("src/".into()));
        // After accepting a directory, state should re-scan the subdirectory.
        assert!(state.is_active());
        // Should now show contents of src/
        let names: Vec<&str> = state.entries.iter().map(|e| e.display.as_str()).collect();
        assert!(
            names.contains(&"lib.rs"),
            "Expected lib.rs in src/ contents: {:?}",
            names
        );
    }

    #[test]
    fn accept_returns_none_when_empty() {
        let state = AtMentionState::new();
        let (_state, result) = state.accept();
        assert_eq!(result, None);
    }

    // ── AtMentionState: visible_entries ──────────────────

    #[test]
    fn visible_entries_caps_at_max() {
        let dir = TempDir::new().expect("Failed to create temp dir");
        let base = dir.path();

        // Create more than MAX_VISIBLE_ENTRIES files.
        for i in 0..12 {
            fs::write(base.join(format!("file{:02}.txt", i)), "").unwrap();
        }

        let mut state = AtMentionState::with_base_dir(base);
        state = state.activate("");

        assert!(
            state.entries.len() >= 12,
            "Expected at least 12 entries, got {}",
            state.entries.len()
        );
        let visible = state.visible_entries();
        assert!(visible.len() <= MAX_VISIBLE_ENTRIES);
        assert_eq!(visible.len(), MAX_VISIBLE_ENTRIES);
    }

    #[test]
    fn visible_entries_returns_all_when_fewer_than_max() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("Car");

        let visible = state.visible_entries();
        assert_eq!(visible.len(), 1);
    }

    // ── VFS cache completion ────────────────────────────

    /// Helper: build a set of VFS entries for testing.
    fn setup_vfs_cache_entries() -> Vec<MentionEntry> {
        vec![
            MentionEntry {
                display: "vfs://workspace/".to_string(),
                value: "vfs://workspace/".to_string(),
                is_directory: true,
            },
            MentionEntry {
                display: "vfs://workspace/main.rs".to_string(),
                value: "vfs://workspace/main.rs".to_string(),
                is_directory: false,
            },
            MentionEntry {
                display: "vfs://workspace/lib.rs".to_string(),
                value: "vfs://workspace/lib.rs".to_string(),
                is_directory: false,
            },
            MentionEntry {
                display: "vfs://workspace/src/".to_string(),
                value: "vfs://workspace/src/".to_string(),
                is_directory: true,
            },
            MentionEntry {
                display: "vfs://workspace/src/app.rs".to_string(),
                value: "vfs://workspace/src/app.rs".to_string(),
                is_directory: false,
            },
            MentionEntry {
                display: "vfs://data/config.json".to_string(),
                value: "vfs://data/config.json".to_string(),
                is_directory: false,
            },
        ]
    }

    #[test]
    fn vfs_cache_empty_returns_empty() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        let entries = complete_vfs("vfs://");
        assert!(entries.is_empty());
    }

    #[test]
    fn vfs_cache_set_and_retrieve() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());

        let entries = complete_vfs("vfs://");
        assert_eq!(entries.len(), 6);

        clear_vfs_cache();
    }

    #[test]
    fn vfs_cache_filters_by_prefix() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());

        let entries = complete_vfs("vfs://workspace/main");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, "vfs://workspace/main.rs");

        clear_vfs_cache();
    }

    #[test]
    fn vfs_cache_filters_by_directory_prefix() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());

        let entries = complete_vfs("vfs://workspace/src/");
        assert_eq!(entries.len(), 2); // src/ directory + src/app.rs
        assert!(entries.iter().any(|e| e.value == "vfs://workspace/src/"));
        assert!(
            entries
                .iter()
                .any(|e| e.value == "vfs://workspace/src/app.rs")
        );

        clear_vfs_cache();
    }

    #[test]
    fn vfs_cache_filters_case_insensitive() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());

        let entries = complete_vfs("VFS://WORKSPACE/MAIN");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, "vfs://workspace/main.rs");

        clear_vfs_cache();
    }

    #[test]
    fn vfs_cache_no_match_returns_empty() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());

        let entries = complete_vfs("vfs://nonexistent/");
        assert!(entries.is_empty());

        clear_vfs_cache();
    }

    #[test]
    fn vfs_cache_separate_scheme_prefix() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());

        // Only "vfs://data/" should match, not "vfs://workspace/".
        let entries = complete_vfs("vfs://data/");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, "vfs://data/config.json");

        clear_vfs_cache();
    }

    #[test]
    fn vfs_cache_overwrite_replaces_entries() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());
        assert_eq!(complete_vfs("vfs://").len(), 6);

        // Overwrite with a smaller set.
        set_vfs_cache(vec![MentionEntry {
            display: "vfs://new/file.txt".to_string(),
            value: "vfs://new/file.txt".to_string(),
            is_directory: false,
        }]);

        let entries = complete_vfs("vfs://");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, "vfs://new/file.txt");

        clear_vfs_cache();
    }

    #[test]
    fn vfs_prefix_triggers_vfs_completion_in_resolve() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());

        // resolve_entries should dispatch to complete_vfs for vfs:// prefixes.
        let entries = resolve_entries("vfs://workspace/lib", None);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].value, "vfs://workspace/lib.rs");

        clear_vfs_cache();
    }

    #[test]
    fn vfs_completion_integrates_with_at_mention_state() {
        let _guard = VFS_TEST_LOCK.lock().unwrap();
        clear_vfs_cache();
        set_vfs_cache(setup_vfs_cache_entries());

        let mut state = AtMentionState::new();
        state = state.activate("vfs://workspace/");

        assert!(state.is_active());
        assert_eq!(state.entries.len(), 5); // workspace/, main.rs, lib.rs, src/, src/app.rs
        // All entries should have values starting with vfs://workspace/
        for entry in &state.entries {
            assert!(
                entry.value.starts_with("vfs://workspace/"),
                "Expected vfs://workspace/ prefix in value: {}",
                entry.value,
            );
        }

        clear_vfs_cache();
    }

    // ── round-trip workflow ─────────────────────────────

    #[test]
    fn full_workflow_activate_navigate_accept_file() {
        let dir = setup_test_dir();
        let state = AtMentionState::with_base_dir(dir.path());

        // 1. Activate with empty prefix.
        let state = state.activate("");
        assert!(state.is_active());
        let total = state.entries.len();
        assert!(total > 0);

        // 2. Update with "m" -- should narrow to main.rs.
        let state = state.update("m");
        assert_eq!(state.entries.len(), 1);
        assert_eq!(state.entries[0].display, "main.rs");

        // 3. Accept.
        let (state, result) = state.accept();
        assert_eq!(result, Some("main.rs".into()));
        assert!(!state.is_active());
    }

    #[test]
    fn full_workflow_directory_traversal() {
        let dir = setup_test_dir();
        let state = AtMentionState::with_base_dir(dir.path());

        // 1. Activate with "s" prefix -- matches src/.
        let state = state.activate("s");
        assert!(state.is_active());
        // Should match src/ (dir).
        assert!(state.entries.iter().any(|e| e.display == "src/"));

        // 2. Update to narrow to "sr" -- just src/.
        let state = state.update("sr");
        assert_eq!(state.entries.len(), 1);
        assert!(state.entries[0].is_directory);

        // 3. Accept directory -- should re-scan src/ contents.
        let (state, result) = state.accept();
        assert_eq!(result, Some("src/".into()));
        assert!(state.is_active()); // Still active for subdirectory browsing.

        // 4. Now see src/ contents.
        assert!(state.entries.iter().any(|e| e.display == "lib.rs"));

        // 5. Accept the file.
        let (state, result) = state.accept();
        assert_eq!(result, Some("src/lib.rs".into()));
        assert!(!state.is_active());
    }

    #[test]
    fn full_workflow_escape_cancels() {
        let dir = setup_test_dir();
        let mut state = AtMentionState::with_base_dir(dir.path());
        state = state.activate("");
        assert!(state.is_active());

        state = state.deactivate();
        assert!(!state.is_active());
    }

    // ── directory entry values include path prefix ──────

    #[test]
    fn directory_entry_values_include_trailing_slash() {
        let dir = setup_test_dir();
        let entries = scan_directory("", Some(dir.path()));

        for entry in &entries {
            if entry.is_directory {
                assert!(
                    entry.value.ends_with('/'),
                    "Directory value should end with /: {}",
                    entry.value
                );
                assert!(
                    entry.display.ends_with('/'),
                    "Directory display should end with /: {}",
                    entry.display
                );
            }
        }
    }

    #[test]
    fn subdirectory_entry_values_include_parent_path() {
        let dir = setup_test_dir();
        let entries = scan_directory("src/", Some(dir.path()));

        for entry in &entries {
            assert!(
                entry.value.starts_with("src/"),
                "Subdirectory entry value should include parent path: {}",
                entry.value
            );
        }
    }
}
