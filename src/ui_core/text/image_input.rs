//! Image path detection and MIME type mapping.

use regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;

/// Supported image file extensions.
pub const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "svg"];

/// Regex matching image file paths in input text.
///
/// Captures the path in group 1. The leading `(?:^|\s)` ensures the path
/// starts at a word boundary. The trailing whitespace/end-of-string check
/// is done manually after matching to avoid consuming separating spaces
/// (Rust's regex crate does not support lookahead).
static IMAGE_PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\.{0,2}[\\/])?[\w./-]+\.(?:png|jpg|jpeg|gif|webp|bmp|svg)\b")
        .expect("image path regex is valid")
});

/// Extract image file paths from input text.
///
/// Returns (cleaned input with paths removed, list of unique detected paths).
pub fn detect_image_paths(input: &str) -> (String, Vec<String>) {
    let mut seen = HashSet::new();
    let mut paths = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();

    for m in IMAGE_PATH_RE.find_iter(input) {
        // Verify leading boundary: must be at start or preceded by whitespace
        if m.start() > 0 {
            let prev = input.as_bytes()[m.start() - 1];
            if !prev.is_ascii_whitespace() {
                continue;
            }
        }
        let path = m.as_str().to_string();
        if seen.insert(path.clone()) {
            paths.push(path);
        }
        ranges.push((m.start(), m.end()));
    }

    // Build cleaned string by skipping matched ranges
    let mut clean = String::with_capacity(input.len());
    let mut last = 0;
    for (start, end) in &ranges {
        clean.push_str(&input[last..*start]);
        last = *end;
    }
    clean.push_str(&input[last..]);

    // Collapse multiple spaces and trim
    let clean = clean.split_whitespace().collect::<Vec<_>>().join(" ");

    (clean, paths)
}

/// Map a file extension (lowercase, no dot) to its image MIME type.
pub fn image_mime_type(ext: &str) -> Option<&'static str> {
    match ext {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_image_paths_single() {
        let (clean, paths) = detect_image_paths("check image.png please");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "image.png");
        assert!(!clean.contains("image.png"));
    }

    #[test]
    fn detect_multiple_image_formats() {
        let (_, paths) = detect_image_paths("see a.jpg b.webp c.gif");
        assert_eq!(paths.len(), 3);
    }

    #[test]
    fn detect_no_images() {
        let (clean, paths) = detect_image_paths("no images here");
        assert_eq!(paths.len(), 0);
        assert_eq!(clean, "no images here");
    }

    #[test]
    fn detect_relative_path() {
        let (_, paths) = detect_image_paths("look at ./screenshots/test.png");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "./screenshots/test.png");
    }

    #[test]
    fn detect_deduplicates() {
        let (_, paths) = detect_image_paths("a.png and a.png again");
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn mime_type_mapping() {
        assert_eq!(image_mime_type("png"), Some("image/png"));
        assert_eq!(image_mime_type("jpg"), Some("image/jpeg"));
        assert_eq!(image_mime_type("jpeg"), Some("image/jpeg"));
        assert_eq!(image_mime_type("gif"), Some("image/gif"));
        assert_eq!(image_mime_type("webp"), Some("image/webp"));
        assert_eq!(image_mime_type("bmp"), Some("image/bmp"));
        assert_eq!(image_mime_type("svg"), Some("image/svg+xml"));
        assert_eq!(image_mime_type("txt"), None);
    }

    #[test]
    fn image_extensions_list() {
        assert!(IMAGE_EXTENSIONS.contains(&"png"));
        assert!(IMAGE_EXTENSIONS.contains(&"jpg"));
        assert!(!IMAGE_EXTENSIONS.contains(&"txt"));
    }

    #[test]
    fn extensions_regex_and_mime_in_sync() {
        for ext in IMAGE_EXTENSIONS {
            assert!(image_mime_type(ext).is_some(), "missing mime for {ext}");
            let test_input = format!("test.{ext}");
            let (_, paths) = detect_image_paths(&test_input);
            assert!(!paths.is_empty(), "regex missed extension {ext}");
        }
    }
}
