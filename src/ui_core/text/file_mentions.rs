//! @-mention parsing and autocomplete.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Regex for @-mentions: @vfs://path or @file.ext paths
static MENTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@(vfs://[\w./-]+|[\w./-]+(?:\.\w+)?)").expect("mention regex is valid")
});

/// Extract the @partial from the end of an input string.
pub fn extract_at_query(input: &str) -> Option<&str> {
    let at_pos = input.rfind('@')?;
    let after = &input[at_pos + 1..];
    // Must not contain whitespace
    if after.contains(char::is_whitespace) {
        return None;
    }
    Some(after)
}

/// Extract @-mentions from input. Returns (cleaned input, unique mention paths).
/// Deduplicates mentions by path.
pub fn extract_mentions(input: &str) -> (String, Vec<String>) {
    let mut seen = HashSet::new();
    let mut mentions = Vec::new();
    let clean = MENTION_RE.replace_all(input, |caps: &regex::Captures| {
        let mention = caps[1].to_string();
        if seen.insert(mention.clone()) {
            mentions.push(mention);
        }
        ""
    });
    (clean.into_owned(), mentions)
}

/// XML-escape a string so file names/content can't break out of the
/// surrounding tag or inject prompt structure (`</file><system>…`).
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Format a mention as XML context for AI prompts. Both the path/id attribute
/// and the body are escaped — a hostile file name or file content must not be
/// able to forge tags or inject instructions into the prompt.
pub fn format_mention_context(path_or_id: &str, content: &str, kind: &str) -> String {
    let id = xml_escape(path_or_id);
    let body = xml_escape(content);
    match kind {
        "volume" => format!("<volume id=\"{id}\">\n{body}\n</volume>"),
        _ => format!("<file path=\"{id}\">\n{body}\n</file>"),
    }
}

/// Validate that an @-mention path stays inside `work_dir`. Returns the
/// resolved absolute path on success, or `None` for absolute paths, `..`
/// traversal, or anything resolving outside the workspace. `vfs://` ids are
/// handled separately by the caller (not a host path). Mirrors the
/// confinement in `handlers/remote.rs`.
pub fn confine_mention_path(rel: &str, work_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    let p = std::path::Path::new(rel);
    if p.is_absolute() || p.components().any(|c| c == std::path::Component::ParentDir) {
        return None;
    }
    let joined = work_dir.join(p);
    let base = work_dir.canonicalize().ok()?;
    // Canonicalize the longest existing ancestor to resolve symlinks, then
    // require the result stays under the workspace root.
    let resolved = joined.canonicalize().unwrap_or(joined);
    resolved.starts_with(&base).then_some(resolved)
}

/// Simple fuzzy match: all chars in query appear in order in target (case-insensitive).
pub fn fuzzy_match(query: &str, target: &str) -> bool {
    let mut target_chars = target.chars();
    for qc in query.chars() {
        let qc_lower = qc.to_ascii_lowercase();
        loop {
            match target_chars.next() {
                Some(tc) if tc.to_ascii_lowercase() == qc_lower => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

/// Format file size as human-readable string.
pub fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{}KB", bytes / 1024)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Check if a string looks like a volume ID (8 hex chars).
pub fn is_volume_id(s: &str) -> bool {
    s.len() == 8 && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Default directories to exclude from file path completion.
pub const DEFAULT_EXCLUDE_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    ".svn",
    ".hg",
    "dist",
    "build",
    "out",
    ".next",
    ".cache",
    "coverage",
    "__pycache__",
    ".venv",
    "venv",
    "target",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_at_query_basic() {
        assert_eq!(extract_at_query("hello @src/m"), Some("src/m"));
    }

    #[test]
    fn extract_at_query_empty_after_at() {
        assert_eq!(extract_at_query("hello @"), Some(""));
    }

    #[test]
    fn extract_at_query_no_at() {
        assert_eq!(extract_at_query("hello world"), None);
    }

    #[test]
    fn extract_at_query_with_space_after() {
        assert_eq!(extract_at_query("hello @foo bar"), None);
    }

    #[test]
    fn extract_mentions_single_file() {
        let (clean, mentions) = extract_mentions("check @src/main.rs please");
        assert_eq!(clean.trim(), "check  please");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0], "src/main.rs");
    }

    #[test]
    fn extract_mentions_multiple() {
        let (_, mentions) = extract_mentions("compare @a.ts and @b.ts");
        assert_eq!(mentions.len(), 2);
        assert_eq!(mentions[0], "a.ts");
        assert_eq!(mentions[1], "b.ts");
    }

    #[test]
    fn extract_mentions_vfs() {
        let (_, mentions) = extract_mentions("read @vfs://output.json");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0], "vfs://output.json");
    }

    #[test]
    fn extract_mentions_deduplicates() {
        let (_, mentions) = extract_mentions("@file.rs and @file.rs again");
        assert_eq!(mentions.len(), 1);
    }

    #[test]
    fn extract_mentions_no_mentions() {
        let (clean, mentions) = extract_mentions("no mentions here");
        assert_eq!(clean, "no mentions here");
        assert!(mentions.is_empty());
    }

    #[test]
    fn format_file_context() {
        let ctx = format_mention_context("src/main.rs", "fn main() {}", "file");
        assert!(ctx.contains("<file path=\"src/main.rs\">"));
        assert!(ctx.contains("fn main() {}"));
        assert!(ctx.contains("</file>"));
    }

    #[test]
    fn format_volume_context() {
        let ctx = format_mention_context("abc12345", "some text", "volume");
        assert!(ctx.contains("<volume id=\"abc12345\">"));
        assert!(ctx.contains("</volume>"));
    }

    #[test]
    fn fuzzy_match_basic() {
        assert!(fuzzy_match("mn", "main"));
        assert!(fuzzy_match("abc", "a_b_c"));
        assert!(!fuzzy_match("xyz", "abc"));
    }

    #[test]
    fn fuzzy_match_empty_query() {
        assert!(fuzzy_match("", "anything"));
    }

    #[test]
    fn fuzzy_match_case_insensitive() {
        assert!(fuzzy_match("MN", "main"));
        assert!(fuzzy_match("mn", "Main"));
    }

    #[test]
    fn format_size_bytes() {
        assert_eq!(format_size(500), "500B");
    }

    #[test]
    fn format_size_kilobytes() {
        assert_eq!(format_size(2048), "2KB");
    }

    #[test]
    fn format_size_megabytes() {
        assert_eq!(format_size(1_500_000), "1.4MB");
    }

    #[test]
    fn is_volume_id_valid() {
        assert!(is_volume_id("abcd1234"));
        assert!(is_volume_id("00ff00ff"));
    }

    #[test]
    fn is_volume_id_invalid() {
        assert!(!is_volume_id("short"));
        assert!(!is_volume_id("not-hex!!"));
        assert!(!is_volume_id("abcdefgh")); // g and h are not hex
        assert!(!is_volume_id("abcd12345")); // too long
    }
}
