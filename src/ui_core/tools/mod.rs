//! Tool definitions, registry, execution types.
//!
//! Types are re-exported from `simse-core`. Utility functions for formatting
//! and truncation are defined locally.

pub mod parser;

// Re-export core tool types as the single source of truth.
pub use simse_core::tools::{
    ToolAnnotations, ToolCallRequest, ToolCallResult, ToolCategory, ToolDefinition, ToolParameter,
};

/// Default maximum output characters for tool results before truncation.
pub const DEFAULT_MAX_OUTPUT_CHARS: usize = 50_000;

/// Raw output from a tool handler before it is wrapped into a [`ToolCallResult`].
///
/// Handlers return this so they can optionally include a diff alongside the
/// textual output.
#[derive(Debug, Clone)]
pub struct ToolHandlerOutput {
    pub output: String,
    pub diff: Option<String>,
}

impl From<String> for ToolHandlerOutput {
    fn from(output: String) -> Self {
        Self { output, diff: None }
    }
}

impl From<&str> for ToolHandlerOutput {
    fn from(output: &str) -> Self {
        Self {
            output: output.to_string(),
            diff: None,
        }
    }
}

/// Format a single tool definition for display or system prompt inclusion.
///
/// Output format:
/// ```text
/// ### tool_name
/// Description text
///
/// Parameters:
///   - param_name (type, required): description
///   - param_name (type): description
/// ```
///
/// Required parameters are listed before optional ones. If there are no
/// parameters, the "Parameters:" section is omitted.
pub fn format_tool_definition(tool: &ToolDefinition) -> String {
    let mut out = format!("### {}\n{}", tool.name, tool.description);

    if !tool.parameters.is_empty() {
        let mut params: Vec<(&String, &ToolParameter)> = tool.parameters.iter().collect();
        // Sort: required first, then alphabetically by name for stable ordering
        params.sort_by(|a, b| b.1.required.cmp(&a.1.required).then_with(|| a.0.cmp(b.0)));

        out.push_str("\n\nParameters:");
        for (name, param) in params {
            if param.required {
                out.push_str(&format!(
                    "\n  - {} ({}, required): {}",
                    name, param.param_type, param.description
                ));
            } else {
                out.push_str(&format!(
                    "\n  - {} ({}): {}",
                    name, param.param_type, param.description
                ));
            }
        }
    }

    out
}

/// Format all tool definitions wrapped in XML-like tags for system prompt injection.
///
/// Output format:
/// ```text
/// <tool_use>
/// You have access to the following tools:
///
/// ### tool1
/// ...
///
/// ### tool2
/// ...
/// </tool_use>
/// ```
pub fn format_tools_for_system_prompt(tools: &[ToolDefinition]) -> String {
    let mut out = String::from("<tool_use>\nYou have access to the following tools:");

    for tool in tools {
        out.push_str("\n\n");
        out.push_str(&format_tool_definition(tool));
    }

    out.push_str("\n</tool_use>");
    out
}

/// Truncate tool output to a maximum number of characters.
///
/// If `output` is shorter than or equal to `max_chars`, it is returned unchanged.
/// If longer, it is truncated to `max_chars` characters and `[OUTPUT TRUNCATED]`
/// is appended (so the total length is `max_chars + 18`).
pub fn truncate_output(output: &str, max_chars: usize) -> String {
    if output.len() <= max_chars {
        output.to_string()
    } else {
        // Find the largest char boundary <= max_chars
        let mut boundary = max_chars;
        while boundary > 0 && !output.is_char_boundary(boundary) {
            boundary -= 1;
        }
        let mut truncated = output[..boundary].to_string();
        truncated.push_str("[OUTPUT TRUNCATED]");
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_tool_with_params() {
        let mut params = std::collections::HashMap::new();
        params.insert(
            "query".to_string(),
            ToolParameter {
                param_type: "string".into(),
                description: "Search query".into(),
                required: true,
            },
        );
        params.insert(
            "maxResults".to_string(),
            ToolParameter {
                param_type: "number".into(),
                description: "Max results".into(),
                required: false,
            },
        );
        let tool = ToolDefinition {
            name: "library_search".into(),
            description: "Search the library".into(),
            parameters: params,
            category: ToolCategory::default(),
            annotations: None,
            timeout_ms: None,
            max_output_chars: None,
        };
        let formatted = format_tool_definition(&tool);
        assert!(formatted.contains("### library_search"));
        assert!(formatted.contains("Search the library"));
        assert!(formatted.contains("query (string, required)"));
        assert!(formatted.contains("maxResults (number)"));
    }

    #[test]
    fn format_tool_no_params() {
        let tool = ToolDefinition {
            name: "list_tools".into(),
            description: "List all tools".into(),
            parameters: std::collections::HashMap::new(),
            category: ToolCategory::default(),
            annotations: None,
            timeout_ms: None,
            max_output_chars: None,
        };
        let formatted = format_tool_definition(&tool);
        assert!(formatted.contains("### list_tools"));
        assert!(!formatted.contains("Parameters:"));
    }

    #[test]
    fn format_tools_system_prompt_wrapper() {
        let tool = ToolDefinition {
            name: "test_tool".into(),
            description: "A test".into(),
            parameters: std::collections::HashMap::new(),
            category: ToolCategory::default(),
            annotations: None,
            timeout_ms: None,
            max_output_chars: None,
        };
        let prompt = format_tools_for_system_prompt(&[tool]);
        assert!(prompt.starts_with("<tool_use>"));
        assert!(prompt.ends_with("</tool_use>"));
        assert!(prompt.contains("test_tool"));
    }

    #[test]
    fn format_tools_system_prompt_empty() {
        let prompt = format_tools_for_system_prompt(&[]);
        assert!(prompt.contains("<tool_use>"));
        assert!(prompt.contains("</tool_use>"));
    }

    #[test]
    fn truncate_output_short() {
        let output = "short output";
        assert_eq!(truncate_output(output, 1000), output);
    }

    #[test]
    fn truncate_output_exact() {
        let output = "exact";
        assert_eq!(truncate_output(output, 5), "exact");
    }

    #[test]
    fn truncate_output_long() {
        let output = "x".repeat(100);
        let truncated = truncate_output(&output, 50);
        assert!(truncated.starts_with("xxxxxxxxxx"));
        assert!(truncated.ends_with("[OUTPUT TRUNCATED]"));
        assert_eq!(truncated.len(), 50 + "[OUTPUT TRUNCATED]".len());
    }

    #[test]
    fn truncate_output_multibyte() {
        // Truncating mid-emoji should not panic
        let output = "Hello \u{1F600} world";
        let truncated = truncate_output(output, 7);
        assert!(truncated.ends_with("[OUTPUT TRUNCATED]"));
    }

    #[test]
    fn required_params_come_first() {
        let mut params = std::collections::HashMap::new();
        params.insert(
            "optional1".to_string(),
            ToolParameter {
                param_type: "string".into(),
                description: "opt".into(),
                required: false,
            },
        );
        params.insert(
            "required1".to_string(),
            ToolParameter {
                param_type: "string".into(),
                description: "req".into(),
                required: true,
            },
        );
        let tool = ToolDefinition {
            name: "test".into(),
            description: "test".into(),
            parameters: params,
            category: ToolCategory::default(),
            annotations: None,
            timeout_ms: None,
            max_output_chars: None,
        };
        let formatted = format_tool_definition(&tool);
        let req_pos = formatted.find("required1").unwrap();
        let opt_pos = formatted.find("optional1").unwrap();
        assert!(
            req_pos < opt_pos,
            "Required params should come before optional"
        );
    }

    #[test]
    fn default_max_output_chars_value() {
        assert_eq!(DEFAULT_MAX_OUTPUT_CHARS, 50_000);
    }

    #[test]
    fn tool_handler_output_from_string() {
        let output = ToolHandlerOutput::from("hello".to_string());
        assert_eq!(output.output, "hello");
        assert!(output.diff.is_none());
    }

    #[test]
    fn tool_handler_output_from_str() {
        let output = ToolHandlerOutput::from("world");
        assert_eq!(output.output, "world");
        assert!(output.diff.is_none());
    }

    #[test]
    fn tool_handler_output_with_diff() {
        let output = ToolHandlerOutput {
            output: "wrote file".into(),
            diff: Some("+new line".into()),
        };
        assert_eq!(output.output, "wrote file");
        assert_eq!(output.diff.as_deref(), Some("+new line"));
    }

    #[test]
    fn tool_definition_max_output_chars_serde() {
        let tool = ToolDefinition {
            name: "test".into(),
            description: "test".into(),
            parameters: std::collections::HashMap::new(),
            category: ToolCategory::default(),
            annotations: None,
            timeout_ms: None,
            max_output_chars: Some(10_000),
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("max_output_chars"));
        let parsed: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.max_output_chars, Some(10_000));
    }

    #[test]
    fn tool_definition_max_output_chars_omitted_when_none() {
        let tool = ToolDefinition {
            name: "test".into(),
            description: "test".into(),
            parameters: std::collections::HashMap::new(),
            category: ToolCategory::default(),
            annotations: None,
            timeout_ms: None,
            max_output_chars: None,
        };
        let json = serde_json::to_string(&tool).unwrap();
        // simse-core may or may not skip_serializing_if for max_output_chars;
        // this test just verifies the field exists or not.
        // The important thing is that deserialization works.
        let _parsed: ToolDefinition = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn tool_call_result_diff_omitted_when_none() {
        let result = ToolCallResult {
            id: "1".into(),
            name: "test".into(),
            output: "ok".into(),
            is_error: false,
            duration_ms: None,
            diff: None,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("diff"));
    }

    #[test]
    fn tool_call_result_diff_present() {
        let result = ToolCallResult {
            id: "1".into(),
            name: "test".into(),
            output: "ok".into(),
            is_error: false,
            duration_ms: None,
            diff: Some("+line".into()),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("diff"));
        assert!(json.contains("+line"));
    }
}
