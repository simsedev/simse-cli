//! Crash-safe JSONL session persistence.
//!
//! Each session has:
//! - An entry in `{data_dir}/sessions/index.json` (array of [`SessionMeta`])
//! - An append-only `{data_dir}/sessions/{id}.jsonl` message log
//!
//! The index is stored newest-first so [`latest`] and [`list`] return
//! most-recent sessions without sorting. The `.jsonl` file is created
//! lazily on the first [`append`] call, so empty sessions cost only an
//! index entry.
//!
//! Corrupt JSONL lines are silently skipped on load — one bad line never
//! loses the rest of the file.

use crate::ui_core::state::session::SessionMeta;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::json_io::{append_json_line, read_json_file, read_json_lines, write_json_file};

/// A message in the session log.
///
/// This is the public type used by callers of [`SessionStore::append`] and
/// returned by [`SessionStore::load`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
}

/// Internal JSONL entry — includes a timestamp that is discarded on load.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionEntry {
    ts: String,
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_name: Option<String>,
}

/// Crash-safe session persistence backed by JSON index + JSONL message logs.
pub struct SessionStore {
    sessions_dir: PathBuf,
    index_path: PathBuf,
}

/// Format an integer in base-36 (digits 0-9 then a-z).
fn radix_fmt(mut n: u64) -> String {
    if n == 0 {
        return "0".into();
    }
    const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut result = Vec::new();
    while n > 0 {
        result.push(CHARS[(n % 36) as usize] as char);
        n /= 36;
    }
    result.into_iter().rev().collect()
}

/// Generate a unique session ID: 8 random hex chars + `-` + timestamp in base-36.
fn generate_id() -> String {
    let uuid_hex = Uuid::new_v4().simple().to_string();
    let random_part = &uuid_hex[..8];
    let ts = Utc::now().timestamp_millis() as u64;
    format!("{}-{}", random_part, radix_fmt(ts))
}

impl SessionStore {
    /// Create a new session store rooted at `data_dir`.
    ///
    /// The sessions directory and index file are created lazily on first write.
    pub fn new(data_dir: &Path) -> Self {
        let sessions_dir = data_dir.join("sessions");
        let index_path = sessions_dir.join("index.json");
        Self {
            sessions_dir,
            index_path,
        }
    }

    /// Create a new session for the given working directory.
    ///
    /// Returns the new session ID. The `.jsonl` message file is NOT created
    /// here — it is created lazily on the first [`append`] call.
    pub fn create(&self, work_dir: &str) -> io::Result<String> {
        let id = generate_id();
        let ts = Utc::now();
        let now = ts.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let title = ts.format("Session %Y-%m-%d %H:%M").to_string();

        let meta = SessionMeta {
            id: id.clone(),
            title,
            created_at: now.clone(),
            updated_at: now,
            message_count: 0,
            work_dir: work_dir.to_string(),
        };

        let mut index = self.load_index();
        index.insert(0, meta);
        write_json_file(&self.index_path, &index)?;

        Ok(id)
    }

    /// Append a message to a session's JSONL log.
    ///
    /// Also increments the session's message count and updates its timestamp
    /// in the index.
    pub fn append(&self, session_id: &str, message: &SessionMessage) -> io::Result<()> {
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();

        let entry = SessionEntry {
            ts: now.clone(),
            role: message.role.clone(),
            content: message.content.clone(),
            tool_call_id: message.tool_call_id.clone(),
            tool_name: message.tool_name.clone(),
        };

        append_json_line(&self.session_file_path(session_id), &entry)?;

        // Update index metadata
        let mut index = self.load_index();
        if let Some(meta) = index.iter_mut().find(|m| m.id == session_id) {
            meta.message_count += 1;
            meta.updated_at = now;
        }
        write_json_file(&self.index_path, &index)?;

        Ok(())
    }

    /// Load all messages from a session's JSONL log.
    ///
    /// Returns an empty `Vec` if the session file does not exist.
    /// Corrupt lines are silently skipped.
    pub fn load(&self, session_id: &str) -> Vec<SessionMessage> {
        let entries: Vec<SessionEntry> = read_json_lines(&self.session_file_path(session_id));
        entries
            .into_iter()
            .map(|e| SessionMessage {
                role: e.role,
                content: e.content,
                tool_call_id: e.tool_call_id,
                tool_name: e.tool_name,
            })
            .collect()
    }

    /// List all sessions, newest first.
    pub fn list(&self) -> Vec<SessionMeta> {
        self.load_index()
    }

    /// Get metadata for a specific session.
    pub fn get(&self, session_id: &str) -> Option<SessionMeta> {
        self.load_index().into_iter().find(|m| m.id == session_id)
    }

    /// Rename a session (update its title).
    ///
    /// Silently succeeds if the session ID does not exist (idempotent).
    pub fn rename(&self, session_id: &str, title: &str) -> io::Result<()> {
        let mut index = self.load_index();
        if let Some(meta) = index.iter_mut().find(|m| m.id == session_id) {
            meta.title = title.to_string();
        }
        write_json_file(&self.index_path, &index)
    }

    /// Remove a session from the index and delete its JSONL file.
    ///
    /// Silently succeeds if the session ID does not exist (idempotent).
    pub fn remove(&self, session_id: &str) -> io::Result<()> {
        let mut index = self.load_index();
        index.retain(|m| m.id != session_id);
        write_json_file(&self.index_path, &index)?;

        let jsonl_path = self.session_file_path(session_id);
        if jsonl_path.exists() {
            std::fs::remove_file(jsonl_path)?;
        }

        Ok(())
    }

    /// Find the most recent session for a given working directory.
    ///
    /// Because the index is stored newest-first, the first match is the
    /// most recent.
    pub fn latest(&self, work_dir: &str) -> Option<String> {
        self.load_index()
            .into_iter()
            .find(|m| m.work_dir == work_dir)
            .map(|m| m.id)
    }

    /// Fork a session at a specific message index.
    ///
    /// Creates a new session containing messages [0..at] from the source session.
    /// Returns the new session ID.
    pub fn fork(
        &self,
        session_id: &str,
        at: Option<usize>,
    ) -> Result<String, crate::event_loop::RuntimeError> {
        use crate::event_loop::RuntimeError;

        let messages = self.load(session_id);
        if messages.is_empty() {
            return Err(RuntimeError::Acp("session has no messages".into()));
        }

        let source = self
            .get(session_id)
            .ok_or_else(|| RuntimeError::Acp("session not found".into()))?;

        let fork_point = at.unwrap_or(messages.len());
        if fork_point > messages.len() {
            return Err(RuntimeError::Acp(format!(
                "--at {} exceeds message count ({})",
                fork_point,
                messages.len()
            )));
        }

        let new_id = self
            .create(&source.work_dir)
            .map_err(|e| RuntimeError::Io(e.to_string()))?;

        for msg in &messages[..fork_point] {
            self.append(&new_id, msg)
                .map_err(|e| RuntimeError::Io(e.to_string()))?;
        }

        Ok(new_id)
    }

    /// Load the index file, returning an empty vec on any failure.
    fn load_index(&self) -> Vec<SessionMeta> {
        read_json_file::<Vec<SessionMeta>>(&self.index_path).unwrap_or_default()
    }

    /// Path to a session's JSONL message file.
    fn session_file_path(&self, session_id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{session_id}.jsonl"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_store() -> (TempDir, SessionStore) {
        let dir = TempDir::new().unwrap();
        let store = SessionStore::new(dir.path());
        (dir, store)
    }

    fn msg(role: &str, content: &str) -> SessionMessage {
        SessionMessage {
            role: role.into(),
            content: content.into(),
            tool_call_id: None,
            tool_name: None,
        }
    }

    #[test]
    fn create_returns_id_and_appears_in_list() {
        let (_dir, store) = make_store();
        let id = store.create("/home/user/project").unwrap();
        assert!(!id.is_empty());

        let list = store.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, id);
        assert_eq!(list[0].work_dir, "/home/user/project");
        assert_eq!(list[0].message_count, 0);
    }

    #[test]
    fn create_newest_first() {
        let (_dir, store) = make_store();
        let id1 = store.create("/a").unwrap();
        let id2 = store.create("/b").unwrap();
        let id3 = store.create("/c").unwrap();

        let list = store.list();
        assert_eq!(list.len(), 3);
        // Newest (id3) should be first
        assert_eq!(list[0].id, id3);
        assert_eq!(list[1].id, id2);
        assert_eq!(list[2].id, id1);
    }

    #[test]
    fn append_and_load_messages() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();

        store.append(&id, &msg("user", "Hello")).unwrap();
        store.append(&id, &msg("assistant", "Hi there!")).unwrap();

        let messages = store.load(&id);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "Hi there!");
    }

    #[test]
    fn append_updates_message_count() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();

        store.append(&id, &msg("user", "one")).unwrap();
        store.append(&id, &msg("assistant", "two")).unwrap();
        store.append(&id, &msg("user", "three")).unwrap();

        let meta = store.get(&id).unwrap();
        assert_eq!(meta.message_count, 3);
    }

    #[test]
    fn load_nonexistent_returns_empty() {
        let (_dir, store) = make_store();
        let messages = store.load("nonexistent-id");
        assert!(messages.is_empty());
    }

    #[test]
    fn get_returns_none_for_missing() {
        let (_dir, store) = make_store();
        assert!(store.get("no-such-session").is_none());
    }

    #[test]
    fn rename_updates_title() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();

        store.rename(&id, "My Custom Title").unwrap();

        let meta = store.get(&id).unwrap();
        assert_eq!(meta.title, "My Custom Title");
    }

    #[test]
    fn remove_deletes_from_index_and_file() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();
        store.append(&id, &msg("user", "hello")).unwrap();

        // Verify file exists
        let jsonl_path = store.session_file_path(&id);
        assert!(jsonl_path.exists());

        store.remove(&id).unwrap();

        assert!(store.get(&id).is_none());
        assert!(store.list().is_empty());
        assert!(!jsonl_path.exists());
    }

    #[test]
    fn latest_returns_most_recent_for_workdir() {
        let (_dir, store) = make_store();
        let _id1 = store.create("/project-a").unwrap();
        let _id2 = store.create("/project-b").unwrap();
        let id3 = store.create("/project-a").unwrap();

        // id3 is the newest for /project-a (inserted at front)
        let latest = store.latest("/project-a").unwrap();
        assert_eq!(latest, id3);
    }

    #[test]
    fn load_tolerates_corrupt_lines() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();

        // Manually write a JSONL file with a corrupt line in the middle
        let jsonl_path = store.session_file_path(&id);
        let good1 = r#"{"ts":"2025-01-01T00:00:00.000Z","role":"user","content":"first"}"#;
        let bad = "NOT VALID JSON";
        let good2 = r#"{"ts":"2025-01-01T00:01:00.000Z","role":"assistant","content":"second"}"#;
        std::fs::create_dir_all(&store.sessions_dir).unwrap();
        std::fs::write(&jsonl_path, format!("{good1}\n{bad}\n{good2}\n")).unwrap();

        let messages = store.load(&id);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "first");
        assert_eq!(messages[1].content, "second");
    }

    #[test]
    fn append_with_tool_fields() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();

        let tool_msg = SessionMessage {
            role: "tool_result".into(),
            content: "file contents here".into(),
            tool_call_id: Some("call_abc123".into()),
            tool_name: Some("read_file".into()),
        };
        store.append(&id, &tool_msg).unwrap();

        let messages = store.load(&id);
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, "tool_result");
        assert_eq!(messages[0].tool_call_id.as_deref(), Some("call_abc123"));
        assert_eq!(messages[0].tool_name.as_deref(), Some("read_file"));
    }

    #[test]
    fn fork_copies_messages_up_to_fork_point() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();
        store.append(&id, &msg("user", "one")).unwrap();
        store.append(&id, &msg("assistant", "two")).unwrap();
        store.append(&id, &msg("user", "three")).unwrap();

        let forked = store.fork(&id, Some(2)).unwrap();

        let messages = store.load(&forked);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "one");
        assert_eq!(messages[1].content, "two");

        let meta = store.get(&forked).unwrap();
        assert_eq!(meta.work_dir, "/project");
        assert_eq!(meta.message_count, 2);
    }

    #[test]
    fn fork_no_at_copies_all_messages() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();
        store.append(&id, &msg("user", "one")).unwrap();
        store.append(&id, &msg("assistant", "two")).unwrap();

        let forked = store.fork(&id, None).unwrap();

        let messages = store.load(&forked);
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].content, "one");
        assert_eq!(messages[1].content, "two");
    }

    #[test]
    fn fork_nonexistent_session_returns_error() {
        let (_dir, store) = make_store();
        let result = store.fork("nonexistent-id", None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("session has no messages") || err.contains("session not found"));
    }

    #[test]
    fn fork_at_exceeding_message_count_returns_error() {
        let (_dir, store) = make_store();
        let id = store.create("/project").unwrap();
        store.append(&id, &msg("user", "one")).unwrap();

        let result = store.fork(&id, Some(5));
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("exceeds message count")
        );
    }

    #[test]
    fn radix_fmt_base36() {
        assert_eq!(radix_fmt(0), "0");
        assert_eq!(radix_fmt(10), "a");
        assert_eq!(radix_fmt(35), "z");
        assert_eq!(radix_fmt(36), "10");
        assert_eq!(radix_fmt(1_000_000), "lfls");
    }

    #[test]
    fn generate_id_format() {
        let id = generate_id();
        let parts: Vec<&str> = id.splitn(2, '-').collect();
        assert_eq!(parts.len(), 2);
        // First part: 8 hex chars
        assert_eq!(parts[0].len(), 8);
        assert!(parts[0].chars().all(|c| c.is_ascii_hexdigit()));
        // Second part: base-36 timestamp (non-empty)
        assert!(!parts[1].is_empty());
    }
}
