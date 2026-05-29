//! Session metadata types shared across crates.

use serde::{Deserialize, Serialize};

/// Metadata for a persisted session.
///
/// Stored in the session index file. All timestamps are ISO 8601 strings.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub message_count: usize,
    pub work_dir: String,
}
