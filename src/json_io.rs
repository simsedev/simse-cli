//! JSON file I/O utilities.
//!
//! Provides helpers for reading/writing JSON files and JSONL (JSON Lines) files.
//!
//! **Design principles:**
//! - Reads are lenient: missing files, empty files, and malformed data return `None`
//!   or empty `Vec` rather than errors. This supports crash-safe recovery.
//! - Writes propagate errors: callers must handle I/O failures explicitly.
//! - JSONL reads skip malformed lines individually, so one corrupt line never
//!   loses the rest of the file's data.
//! - JSON output uses tab indentation to match the project's Biome/formatting convention.

use serde::Serialize;
use serde::de::DeserializeOwned;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

/// Read and deserialize a JSON file.
///
/// Returns `None` if the file is missing, empty, or contains invalid JSON.
/// Failures are silent — no errors are logged or propagated.
pub fn read_json_file<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let contents = fs::read_to_string(path).ok()?;
    if contents.trim().is_empty() {
        return None;
    }
    serde_json::from_str(&contents).ok()
}

/// Write a value as pretty-printed JSON with tab indentation to a file.
///
/// Creates parent directories if they do not exist.
/// Errors are propagated to the caller.
pub fn write_json_file<T: Serialize>(path: &Path, data: &T) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
    let mut buf = Vec::new();
    let mut serializer = serde_json::Serializer::with_formatter(&mut buf, formatter);
    data.serialize(&mut serializer)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    buf.push(b'\n');

    fs::write(path, &buf)
}

/// Append a single JSON object as one compact line to a JSONL file.
///
/// Creates parent directories if they do not exist.
/// Errors are propagated to the caller.
pub fn append_json_line<T: Serialize>(path: &Path, data: &T) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let line =
        serde_json::to_string(data).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(file, "{line}")
}

/// Read all lines from a JSONL file, deserializing each line.
///
/// Returns an empty `Vec` if the file is missing or unreadable.
/// Individual malformed lines are silently skipped — this is critical for
/// crash safety, ensuring one corrupt line does not lose all data.
pub fn read_json_lines<T: DeserializeOwned>(path: &Path) -> Vec<T> {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };

    io::BufReader::new(file)
        .lines()
        .filter_map(|line| {
            let line = line.ok()?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            serde_json::from_str(trimmed).ok()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use tempfile::TempDir;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct TestRecord {
        name: String,
        value: i64,
    }

    #[test]
    fn read_json_file_missing_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent.json");
        let result: Option<TestRecord> = read_json_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn read_json_file_empty_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("empty.json");
        fs::write(&path, "").unwrap();
        let result: Option<TestRecord> = read_json_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn read_json_file_malformed_returns_none() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bad.json");
        fs::write(&path, "{ not valid json !!!").unwrap();
        let result: Option<TestRecord> = read_json_file(&path);
        assert!(result.is_none());
    }

    #[test]
    fn write_and_read_json_file_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("data.json");
        let record = TestRecord {
            name: "alice".into(),
            value: 42,
        };

        write_json_file(&path, &record).unwrap();
        let loaded: Option<TestRecord> = read_json_file(&path);
        assert_eq!(loaded, Some(record));
    }

    #[test]
    fn write_json_file_uses_tab_indentation() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("tabs.json");
        let record = TestRecord {
            name: "bob".into(),
            value: 7,
        };

        write_json_file(&path, &record).unwrap();
        let contents = fs::read_to_string(&path).unwrap();

        // Should contain tab-indented fields
        assert!(
            contents.contains("\t\"name\""),
            "Expected tab indentation, got:\n{contents}"
        );
        // Should NOT contain space indentation (2 or 4 spaces)
        assert!(
            !contents.contains("  \"name\""),
            "Should not use space indentation"
        );
        // Should end with a trailing newline
        assert!(contents.ends_with('\n'));
    }

    #[test]
    fn write_json_file_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("a").join("b").join("c").join("data.json");
        let record = TestRecord {
            name: "nested".into(),
            value: 99,
        };

        write_json_file(&path, &record).unwrap();
        let loaded: Option<TestRecord> = read_json_file(&path);
        assert_eq!(loaded, Some(record));
    }

    #[test]
    fn append_and_read_json_lines_roundtrip() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("log.jsonl");

        let r1 = TestRecord {
            name: "first".into(),
            value: 1,
        };
        let r2 = TestRecord {
            name: "second".into(),
            value: 2,
        };
        let r3 = TestRecord {
            name: "third".into(),
            value: 3,
        };

        append_json_line(&path, &r1).unwrap();
        append_json_line(&path, &r2).unwrap();
        append_json_line(&path, &r3).unwrap();

        let records: Vec<TestRecord> = read_json_lines(&path);
        assert_eq!(records, vec![r1, r2, r3]);
    }

    #[test]
    fn read_json_lines_missing_returns_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("missing.jsonl");
        let records: Vec<TestRecord> = read_json_lines(&path);
        assert!(records.is_empty());
    }

    #[test]
    fn read_json_lines_skips_malformed_lines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("mixed.jsonl");

        let content = r#"{"name":"good1","value":1}
NOT JSON AT ALL
{"name":"good2","value":2}
{broken
{"name":"good3","value":3}
"#;
        fs::write(&path, content).unwrap();

        let records: Vec<TestRecord> = read_json_lines(&path);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].name, "good1");
        assert_eq!(records[1].name, "good2");
        assert_eq!(records[2].name, "good3");
    }

    #[test]
    fn read_json_lines_handles_trailing_newlines() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("trailing.jsonl");

        let content = "{\"name\":\"only\",\"value\":10}\n\n\n";
        fs::write(&path, content).unwrap();

        let records: Vec<TestRecord> = read_json_lines(&path);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].name, "only");
    }

    #[test]
    fn append_json_line_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("x").join("y").join("log.jsonl");
        let record = TestRecord {
            name: "deep".into(),
            value: 55,
        };

        append_json_line(&path, &record).unwrap();

        let records: Vec<TestRecord> = read_json_lines(&path);
        assert_eq!(records, vec![record]);
    }
}
