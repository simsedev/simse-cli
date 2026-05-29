//! Core application state machine.

use serde::{Deserialize, Serialize};

/// A volume (document) in the library.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeView {
    pub id: String,
    pub text: String,
    pub topic: String,
    pub metadata: std::collections::HashMap<String, String>,
    pub timestamp: i64,
}

/// A search result with relevance score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultView {
    pub volume: VolumeView,
    pub score: f64,
}

/// Aggregated topic metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicView {
    pub topic: String,
    pub volume_count: usize,
}

/// Options for text generation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GenerateOptions {
    pub skip_library: bool,
    pub library_max_results: Option<usize>,
    pub library_threshold: Option<f64>,
}

/// Result of text generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateResult {
    pub content: String,
    pub agent_id: String,
    pub server_name: String,
    pub library_context: Vec<SearchResultView>,
    pub stored_volume_id: Option<String>,
}

/// An item displayed in the chat output.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputItem {
    Message { role: String, text: String },
    ToolCall(ToolCallState),
    CommandResult { text: String },
    Error { message: String },
    Info { text: String },
}

/// State of a tool call in progress or completed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallState {
    pub id: String,
    pub name: String,
    pub args: String,
    pub status: ToolCallStatus,
    pub started_at: i64,
    pub duration_ms: Option<u64>,
    pub summary: Option<String>,
    pub error: Option<String>,
    pub diff: Option<String>,
}

/// Status of a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCallStatus {
    Active,
    Completed,
    Failed,
}

/// A pending permission request from the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub id: String,
    pub tool_name: String,
    pub args: serde_json::Value,
    pub options: Vec<PermissionOption>,
}

/// An option in a permission request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionOption {
    pub id: String,
    pub label: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_item_message() {
        let item = OutputItem::Message {
            role: "user".into(),
            text: "hello".into(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\":\"message\""));
        assert!(json.contains("\"role\":\"user\""));
    }

    #[test]
    fn output_item_tool_call() {
        let item = OutputItem::ToolCall(ToolCallState {
            id: "tc1".into(),
            name: "read_file".into(),
            args: "{}".into(),
            status: ToolCallStatus::Active,
            started_at: 1000,
            duration_ms: None,
            summary: None,
            error: None,
            diff: None,
        });
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\":\"tool_call\""));
        assert!(json.contains("\"name\":\"read_file\""));
    }

    #[test]
    fn output_item_error() {
        let item = OutputItem::Error {
            message: "something broke".into(),
        };
        let json = serde_json::to_string(&item).unwrap();
        assert!(json.contains("\"type\":\"error\""));
    }

    #[test]
    fn tool_call_status_serializes() {
        assert_eq!(
            serde_json::to_string(&ToolCallStatus::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(
            serde_json::to_string(&ToolCallStatus::Completed).unwrap(),
            "\"completed\""
        );
        assert_eq!(
            serde_json::to_string(&ToolCallStatus::Failed).unwrap(),
            "\"failed\""
        );
    }

    #[test]
    fn tool_call_state_defaults() {
        let state = ToolCallState {
            id: "1".into(),
            name: "test".into(),
            args: "{}".into(),
            status: ToolCallStatus::Active,
            started_at: 0,
            duration_ms: None,
            summary: None,
            error: None,
            diff: None,
        };
        assert_eq!(state.status, ToolCallStatus::Active);
        assert!(state.duration_ms.is_none());
    }

    #[test]
    fn permission_request_serializes() {
        let req = PermissionRequest {
            id: "p1".into(),
            tool_name: "write_file".into(),
            args: serde_json::json!({"path": "/tmp/test"}),
            options: vec![
                PermissionOption {
                    id: "allow".into(),
                    label: "Allow once".into(),
                },
                PermissionOption {
                    id: "deny".into(),
                    label: "Deny".into(),
                },
            ],
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"tool_name\":\"write_file\""));
        assert!(json.contains("\"Allow once\""));
    }

    #[test]
    fn output_item_roundtrip() {
        let items = vec![
            OutputItem::Message {
                role: "assistant".into(),
                text: "hi".into(),
            },
            OutputItem::CommandResult {
                text: "done".into(),
            },
            OutputItem::Info {
                text: "info msg".into(),
            },
        ];
        for item in &items {
            let json = serde_json::to_string(item).unwrap();
            let back: OutputItem = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&back).unwrap();
            assert_eq!(json, json2);
        }
    }
}
