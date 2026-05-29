//! Tool call parser — extracts `<tool_use>` blocks from LLM responses.
//!
//! LLM responses may contain one or more `<tool_use>...</tool_use>` blocks,
//! each wrapping a JSON object with `id`, `name`, and `arguments` fields.
//! This module extracts those blocks into [`ToolCallRequest`] values and
//! returns the remaining text with the blocks stripped out.

use std::sync::LazyLock;

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

static TOOL_USE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"<tool_use>\s*([\s\S]*?)\s*</tool_use>").expect("valid regex"));

use super::ToolCallRequest;

/// Result of parsing an LLM response for tool calls.
#[derive(Debug, Clone)]
pub struct ParsedResponse {
    /// The response text with all `<tool_use>` blocks removed and trimmed.
    pub text: String,
    /// Zero or more tool call requests extracted from the response.
    pub tool_calls: Vec<ToolCallRequest>,
}

/// Intermediate deserialization target for the JSON inside `<tool_use>` blocks.
#[derive(Deserialize)]
struct RawToolCall {
    id: Option<String>,
    name: Option<String>,
    #[serde(default)]
    arguments: Option<Value>,
}

/// Parse tool calls from an LLM response string.
///
/// Looks for `<tool_use>...</tool_use>` blocks containing JSON objects.
/// Each JSON object is expected to have at least a `name` field. The `id`
/// field is auto-generated as `call_1`, `call_2`, etc. when absent. The
/// `arguments` field defaults to `{}` when absent.
///
/// Malformed JSON or blocks missing a `name` field are silently skipped.
///
/// # Examples
///
/// ```
/// use simse_cli::ui_core::tools::parser::parse_tool_calls;
///
/// let response = r#"Let me search for that.
/// <tool_use>
/// { "name": "search", "arguments": { "query": "rust" } }
/// </tool_use>
/// "#;
///
/// let parsed = parse_tool_calls(response);
/// assert_eq!(parsed.tool_calls.len(), 1);
/// assert_eq!(parsed.tool_calls[0].name, "search");
/// assert_eq!(parsed.text, "Let me search for that.");
/// ```
pub fn parse_tool_calls(response: &str) -> ParsedResponse {
    let pattern = &*TOOL_USE_RE;

    let mut tool_calls = Vec::new();

    for cap in pattern.captures_iter(response) {
        let json_str = cap[1].trim();
        match serde_json::from_str::<RawToolCall>(json_str) {
            Ok(raw) => {
                if let Some(name) = raw.name {
                    tool_calls.push(ToolCallRequest {
                        id: raw
                            .id
                            .unwrap_or_else(|| format!("call_{}", tool_calls.len() + 1)),
                        name,
                        arguments: raw.arguments.unwrap_or(Value::Object(Default::default())),
                    });
                }
                // No name → skip silently
            }
            Err(_) => {
                // Malformed JSON → skip silently
            }
        }
    }

    let text = pattern.replace_all(response, "").trim().to_string();

    ParsedResponse { text, tool_calls }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_tool_calls() {
        let parsed = parse_tool_calls("Just a plain text response.");
        assert_eq!(parsed.text, "Just a plain text response.");
        assert!(parsed.tool_calls.is_empty());
    }

    #[test]
    fn single_tool_call_with_id() {
        let response = r#"Here is my answer.
<tool_use>
{ "id": "tc_1", "name": "read_file", "arguments": { "path": "/tmp/foo.txt" } }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].id, "tc_1");
        assert_eq!(parsed.tool_calls[0].name, "read_file");
        assert_eq!(parsed.tool_calls[0].arguments["path"], "/tmp/foo.txt");
        assert_eq!(parsed.text, "Here is my answer.");
    }

    #[test]
    fn auto_generated_ids() {
        let response = r#"<tool_use>
{ "name": "tool_a", "arguments": {} }
</tool_use>
<tool_use>
{ "name": "tool_b", "arguments": {} }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 2);
        assert_eq!(parsed.tool_calls[0].id, "call_1");
        assert_eq!(parsed.tool_calls[0].name, "tool_a");
        assert_eq!(parsed.tool_calls[1].id, "call_2");
        assert_eq!(parsed.tool_calls[1].name, "tool_b");
    }

    #[test]
    fn mixed_ids_auto_and_explicit() {
        let response = r#"<tool_use>
{ "name": "first" }
</tool_use>
<tool_use>
{ "id": "explicit_id", "name": "second" }
</tool_use>
<tool_use>
{ "name": "third" }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 3);
        assert_eq!(parsed.tool_calls[0].id, "call_1");
        assert_eq!(parsed.tool_calls[1].id, "explicit_id");
        assert_eq!(parsed.tool_calls[2].id, "call_3");
    }

    #[test]
    fn malformed_json_skipped() {
        let response = r#"Some text.
<tool_use>
not valid json at all
</tool_use>
<tool_use>
{ "name": "valid_tool", "arguments": { "x": 1 } }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "valid_tool");
        assert_eq!(parsed.tool_calls[0].id, "call_1");
    }

    #[test]
    fn missing_name_skipped() {
        let response = r#"<tool_use>
{ "id": "no_name", "arguments": { "x": 1 } }
</tool_use>
<tool_use>
{ "name": "has_name" }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "has_name");
        // ID auto-generated starting from 1 since the skipped block doesn't count
        assert_eq!(parsed.tool_calls[0].id, "call_1");
    }

    #[test]
    fn missing_arguments_defaults_to_empty_object() {
        let response = r#"<tool_use>
{ "name": "no_args" }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].arguments, serde_json::json!({}));
    }

    #[test]
    fn text_stripped_of_tool_blocks() {
        let response = r#"Before.
<tool_use>
{ "name": "tool1" }
</tool_use>
Middle.
<tool_use>
{ "name": "tool2" }
</tool_use>
After."#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 2);
        // Text should have blocks removed and be trimmed
        assert!(parsed.text.contains("Before."));
        assert!(parsed.text.contains("Middle."));
        assert!(parsed.text.contains("After."));
        assert!(!parsed.text.contains("<tool_use>"));
        assert!(!parsed.text.contains("</tool_use>"));
    }

    #[test]
    fn empty_response() {
        let parsed = parse_tool_calls("");
        assert_eq!(parsed.text, "");
        assert!(parsed.tool_calls.is_empty());
    }

    #[test]
    fn whitespace_only_response() {
        let parsed = parse_tool_calls("   \n\t  ");
        assert_eq!(parsed.text, "");
        assert!(parsed.tool_calls.is_empty());
    }

    #[test]
    fn tool_use_with_extra_whitespace() {
        let response = "<tool_use>   \n\n  { \"name\": \"spaced\" }   \n\n  </tool_use>";
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "spaced");
    }

    #[test]
    fn nested_json_arguments() {
        let response = r#"<tool_use>
{
  "name": "complex_tool",
  "arguments": {
    "nested": { "a": 1, "b": [2, 3] },
    "flag": true
  }
}
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].arguments["nested"]["a"], 1);
        assert_eq!(parsed.tool_calls[0].arguments["flag"], true);
    }

    #[test]
    fn only_tool_blocks_no_surrounding_text() {
        let response = r#"<tool_use>
{ "name": "solo" }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.text, "");
    }

    #[test]
    fn partial_tool_use_tag_not_matched() {
        let response = "Some text <tool_use> but no closing tag";
        let parsed = parse_tool_calls(response);
        assert!(parsed.tool_calls.is_empty());
        assert_eq!(parsed.text, response);
    }

    #[test]
    fn multiple_tool_calls_preserve_order() {
        let response = r#"<tool_use>
{ "id": "a", "name": "first" }
</tool_use>
<tool_use>
{ "id": "b", "name": "second" }
</tool_use>
<tool_use>
{ "id": "c", "name": "third" }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls.len(), 3);
        assert_eq!(parsed.tool_calls[0].name, "first");
        assert_eq!(parsed.tool_calls[1].name, "second");
        assert_eq!(parsed.tool_calls[2].name, "third");
    }

    #[test]
    fn string_arguments_value() {
        let response = r#"<tool_use>
{ "name": "echo", "arguments": { "message": "hello world" } }
</tool_use>"#;
        let parsed = parse_tool_calls(response);
        assert_eq!(parsed.tool_calls[0].arguments["message"], "hello world");
    }
}
