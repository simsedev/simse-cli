//! Conversation state management.
//!
//! Types are re-exported from `simse-core`. `ConversationBuffer` is a thin
//! wrapper around `simse_core::Conversation` that provides backward-compatible
//! APIs for the UI layer.
//!
//! All mutating methods use owned-return (`self -> Self`) for functional-style
//! state transitions. The inner `simse_core::Conversation` also uses this pattern.

// Re-export core conversation types as the single source of truth.
pub use simse_core::conversation::{ConversationMessage, ConversationOptions, Role};

/// Backward-compatibility alias.
pub type ConversationRole = Role;

/// Backward-compatibility alias.
pub type Message = ConversationMessage;

// ---------------------------------------------------------------------------
// ConversationBuffer
// ---------------------------------------------------------------------------

/// Conversation buffer with auto-compaction and trimming support.
///
/// Thin wrapper around `simse_core::Conversation` that provides the same
/// API surface used by the UI layer. All mutating methods take `self` by
/// value and return the updated `Self` (owned-return pattern).
#[derive(Debug, Clone)]
pub struct ConversationBuffer {
    inner: simse_core::Conversation,
}

impl ConversationBuffer {
    /// Create a new conversation buffer with the given options.
    pub fn new(options: ConversationOptions) -> Self {
        Self {
            inner: simse_core::Conversation::new(Some(options)),
        }
    }

    /// Add a user message to the conversation.
    pub fn add_user(self, content: &str) -> Self {
        Self {
            inner: self.inner.add_user(content),
        }
    }

    /// Add an assistant message to the conversation.
    pub fn add_assistant(self, content: &str) -> Self {
        Self {
            inner: self.inner.add_assistant(content),
        }
    }

    /// Add a tool result message to the conversation.
    pub fn add_tool_result(self, tool_call_id: &str, tool_name: &str, content: &str) -> Self {
        Self {
            inner: self.inner.add_tool_result(tool_call_id, tool_name, content),
        }
    }

    /// Set the system prompt (replaces any existing system prompt).
    pub fn set_system_prompt(self, prompt: &str) -> Self {
        Self {
            inner: self.inner.set_system_prompt(prompt.to_string()),
        }
    }

    /// Load messages from a saved session.
    ///
    /// Clears existing messages and replays the provided list.
    /// System-role messages are extracted and used to set the system prompt.
    /// All other messages are pushed into the buffer.
    pub fn load_messages(self, msgs: &[ConversationMessage]) -> Self {
        let mut inner = self.inner;

        // Extract system prompt from system-role messages, load the rest.
        let mut non_system = Vec::new();
        for msg in msgs {
            if msg.role == Role::System {
                inner = inner.set_system_prompt(msg.content.clone());
            } else {
                non_system.push(msg.clone());
            }
        }
        Self {
            inner: inner.load_messages(non_system),
        }
    }

    /// Return all messages with the system prompt prepended (if set).
    pub fn to_messages(&self) -> Vec<ConversationMessage> {
        self.inner.to_messages()
    }

    /// Serialize the conversation to a human-readable string.
    ///
    /// Each message is formatted as `[Role]\ncontent` and joined by double newlines.
    /// Tool results use the format `[Tool Result: {tool_name or tool_call_id}]`.
    pub fn serialize(&self) -> String {
        self.inner.serialize()
    }

    /// Clear all messages but preserve the system prompt.
    pub fn clear(self) -> Self {
        Self {
            inner: self.inner.clear(),
        }
    }

    /// Replace all messages with a single user message containing the summary.
    pub fn compact(self, summary: &str) -> Self {
        Self {
            inner: self.inner.compact(summary),
        }
    }

    /// Count of non-system messages in the buffer.
    pub fn message_count(&self) -> usize {
        self.inner.message_count()
    }

    /// Approximate character count of the entire conversation.
    ///
    /// Includes the system prompt length plus the sum of all message content lengths.
    pub fn estimated_chars(&self) -> usize {
        self.inner.estimated_chars()
    }

    /// Returns true when the estimated character count exceeds the auto-compact threshold.
    pub fn needs_compaction(&self) -> bool {
        self.inner.needs_compaction()
    }

    /// Get the current system prompt, if any.
    pub fn system_prompt(&self) -> Option<&str> {
        self.inner.system_prompt()
    }

    /// Get a reference to the conversation messages (excludes system prompt).
    ///
    /// Returns `&im::Vector<ConversationMessage>` (persistent data structure).
    pub fn messages(&self) -> &im::Vector<ConversationMessage> {
        self.inner.messages()
    }
}

/// Alias for backward compatibility.
pub type Conversation = ConversationBuffer;

// ---------------------------------------------------------------------------
// Backward-compatible free functions (owned-return style)
// ---------------------------------------------------------------------------

/// Create a new conversation (backward-compatible free function).
pub fn new_conversation(
    system_prompt: Option<String>,
    max_messages: Option<usize>,
    auto_compact_chars: Option<usize>,
) -> ConversationBuffer {
    ConversationBuffer::new(ConversationOptions {
        system_prompt,
        max_messages,
        auto_compact_chars,
        context_window_tokens: None,
    })
}

/// Add a user message (backward-compatible free function).
pub fn add_user(conv: ConversationBuffer, content: String) -> ConversationBuffer {
    conv.add_user(&content)
}

/// Add an assistant message (backward-compatible free function).
pub fn add_assistant(conv: ConversationBuffer, content: String) -> ConversationBuffer {
    conv.add_assistant(&content)
}

/// Add a tool result (backward-compatible free function).
pub fn add_tool_result(
    conv: ConversationBuffer,
    tool_call_id: String,
    tool_name: String,
    content: String,
) -> ConversationBuffer {
    conv.add_tool_result(&tool_call_id, &tool_name, &content)
}

/// Get all messages including system prompt (backward-compatible free function).
pub fn to_messages(conv: &ConversationBuffer) -> Vec<ConversationMessage> {
    conv.to_messages()
}

/// Estimated character count (backward-compatible free function).
pub fn estimated_chars(conv: &ConversationBuffer) -> usize {
    conv.estimated_chars()
}

/// Check if compaction is needed (backward-compatible free function).
pub fn needs_compaction(conv: &ConversationBuffer) -> bool {
    conv.needs_compaction()
}

/// Clear messages (backward-compatible free function).
pub fn clear(conv: ConversationBuffer) -> ConversationBuffer {
    conv.clear()
}

/// Compact with summary (backward-compatible free function).
pub fn compact(conv: ConversationBuffer, summary: String) -> ConversationBuffer {
    conv.compact(&summary)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // 1. new_default_options -- default state is empty
    #[test]
    fn new_default_options() {
        let buf = ConversationBuffer::new(ConversationOptions::default());
        assert_eq!(buf.message_count(), 0);
        assert!(buf.system_prompt().is_none());
        assert_eq!(buf.to_messages().len(), 0);
        assert_eq!(buf.estimated_chars(), 0);
        assert!(!buf.needs_compaction());
    }

    // 2. add_user_and_count -- add user message, count is 1
    #[test]
    fn add_user_and_count() {
        let buf = ConversationBuffer::new(ConversationOptions::default());
        let buf = buf.add_user("Hello world");
        assert_eq!(buf.message_count(), 1);
        assert_eq!(buf.messages()[0].role, Role::User);
        assert_eq!(buf.messages()[0].content, "Hello world");
    }

    // 3. add_assistant -- adds assistant message
    #[test]
    fn add_assistant_message() {
        let buf = ConversationBuffer::new(ConversationOptions::default());
        let buf = buf.add_assistant("I can help with that");
        assert_eq!(buf.message_count(), 1);
        assert_eq!(buf.messages()[0].role, Role::Assistant);
        assert_eq!(buf.messages()[0].content, "I can help with that");
    }

    // 4. add_tool_result_with_fields -- tool_call_id and tool_name preserved
    #[test]
    fn add_tool_result_with_fields() {
        let buf = ConversationBuffer::new(ConversationOptions::default());
        let buf = buf.add_tool_result("call_123", "read_file", "file contents here");
        assert_eq!(buf.message_count(), 1);
        let msg = &buf.messages()[0];
        assert_eq!(msg.role, Role::ToolResult);
        assert_eq!(msg.content, "file contents here");
        assert_eq!(msg.tool_call_id.as_deref(), Some("call_123"));
        assert_eq!(msg.tool_name.as_deref(), Some("read_file"));
    }

    // 5. serialize_format -- verify serialize produces correct [Role]\ncontent\n\n... format
    #[test]
    fn serialize_format() {
        let buf = ConversationBuffer::new(ConversationOptions {
            system_prompt: Some("You are helpful.".to_string()),
            ..ConversationOptions::default()
        });
        let buf = buf.add_user("Hello");
        let buf = buf.add_assistant("Hi there");
        let buf = buf.add_tool_result("tc1", "bash", "output");

        let serialized = buf.serialize();
        let expected = "[System]\nYou are helpful.\n\n\
		               [User]\nHello\n\n\
		               [Assistant]\nHi there\n\n\
		               [Tool Result: bash]\noutput";
        assert_eq!(serialized, expected);
    }

    // 6. compact_replaces_messages -- compact clears and inserts summary
    #[test]
    fn compact_replaces_messages() {
        let buf = ConversationBuffer::new(ConversationOptions::default());
        let buf = buf.add_user("msg1");
        let buf = buf.add_assistant("msg2");
        let buf = buf.add_user("msg3");
        let buf = buf.compact("This is a summary of the conversation.");

        assert_eq!(buf.message_count(), 1);
        assert_eq!(buf.messages()[0].role, Role::User);
        assert_eq!(
            buf.messages()[0].content,
            "[Conversation summary]\nThis is a summary of the conversation."
        );
    }

    // 7. needs_compaction_threshold -- returns true when chars exceed threshold
    #[test]
    fn needs_compaction_threshold() {
        let buf = ConversationBuffer::new(ConversationOptions {
            auto_compact_chars: Some(10),
            ..ConversationOptions::default()
        });
        assert!(!buf.needs_compaction());

        let buf = buf.add_user(&"a".repeat(20));
        assert!(buf.needs_compaction());
    }

    // 8. trim_oldest_when_max_exceeded -- with max_messages=2, adding 3 messages trims oldest
    #[test]
    fn trim_oldest_when_max_exceeded() {
        let buf = ConversationBuffer::new(ConversationOptions {
            max_messages: Some(2),
            ..ConversationOptions::default()
        });
        let buf = buf.add_user("first");
        let buf = buf.add_user("second");
        let buf = buf.add_user("third");

        assert_eq!(buf.message_count(), 2);
        assert_eq!(buf.messages()[0].content, "second");
        assert_eq!(buf.messages()[1].content, "third");
    }

    // 9. load_messages_extracts_system -- system messages become system_prompt
    #[test]
    fn load_messages_extracts_system() {
        let buf = ConversationBuffer::new(ConversationOptions::default());

        let msgs = vec![
            ConversationMessage {
                role: Role::System,
                content: "Be helpful".to_string(),
                tool_call_id: None,
                tool_name: None,
                timestamp: None,
            },
            ConversationMessage {
                role: Role::User,
                content: "Hello".to_string(),
                tool_call_id: None,
                tool_name: None,
                timestamp: None,
            },
            ConversationMessage {
                role: Role::Assistant,
                content: "Hi".to_string(),
                tool_call_id: None,
                tool_name: None,
                timestamp: None,
            },
        ];

        let buf = buf.load_messages(&msgs);

        assert_eq!(buf.system_prompt(), Some("Be helpful"));
        assert_eq!(buf.message_count(), 2);
        assert_eq!(buf.messages()[0].role, Role::User);
        assert_eq!(buf.messages()[1].role, Role::Assistant);
    }

    // 10. clear_preserves_system_prompt -- clear removes messages but keeps system_prompt
    #[test]
    fn clear_preserves_system_prompt() {
        let buf = ConversationBuffer::new(ConversationOptions {
            system_prompt: Some("System prompt here".to_string()),
            ..ConversationOptions::default()
        });
        let buf = buf.add_user("hello");
        let buf = buf.add_assistant("hi");

        assert_eq!(buf.message_count(), 2);
        let buf = buf.clear();

        assert_eq!(buf.message_count(), 0);
        assert_eq!(buf.system_prompt(), Some("System prompt here"));
    }

    // 11. to_messages_includes_system -- system prompt prepended to output
    #[test]
    fn to_messages_includes_system() {
        let buf = ConversationBuffer::new(ConversationOptions {
            system_prompt: Some("You are an AI.".to_string()),
            ..ConversationOptions::default()
        });
        let buf = buf.add_user("Hello");

        let msgs = buf.to_messages();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, Role::System);
        assert_eq!(msgs[0].content, "You are an AI.");
        assert_eq!(msgs[1].role, Role::User);
        assert_eq!(msgs[1].content, "Hello");
    }

    // 12. set_system_prompt -- replaces existing system prompt
    #[test]
    fn set_system_prompt_replaces() {
        let buf = ConversationBuffer::new(ConversationOptions {
            system_prompt: Some("Old prompt".to_string()),
            ..ConversationOptions::default()
        });
        let buf = buf.set_system_prompt("New prompt");
        assert_eq!(buf.system_prompt(), Some("New prompt"));

        let msgs = buf.to_messages();
        assert_eq!(msgs[0].content, "New prompt");
    }

    // 13. estimated_chars_includes_system -- system prompt counted in estimated chars
    #[test]
    fn estimated_chars_includes_system() {
        let buf = ConversationBuffer::new(ConversationOptions {
            system_prompt: Some("12345".to_string()),
            ..ConversationOptions::default()
        });
        let buf = buf.add_user("abc"); // 3 chars

        assert_eq!(buf.estimated_chars(), 8); // 5 + 3
    }

    // 14. serialize_tool_result_fallback -- tool result uses tool_call_id when tool_name is None
    #[test]
    fn serialize_tool_result_fallback() {
        let buf = ConversationBuffer::new(ConversationOptions::default());
        // Use load_messages to add a message with tool_name = None
        let buf = buf.load_messages(&[ConversationMessage {
            role: Role::ToolResult,
            content: "result data".to_string(),
            tool_call_id: Some("tc_abc".to_string()),
            tool_name: None,
            timestamp: None,
        }]);

        let serialized = buf.serialize();
        assert_eq!(serialized, "[Tool Result: tc_abc]\nresult data");
    }

    // 15. backward compat -- old free functions still work
    #[test]
    fn backward_compat_free_functions() {
        let conv = new_conversation(Some("sys".into()), None, None);
        let conv = add_user(conv, "hello".into());
        let conv = add_assistant(conv, "hi".into());
        let conv = add_tool_result(conv, "tc1".into(), "bash".into(), "output".into());

        let msgs = to_messages(&conv);
        assert_eq!(msgs.len(), 4); // system + 3 messages
        assert_eq!(msgs[0].role, Role::System);

        assert_eq!(estimated_chars(&conv), 3 + 5 + 2 + 6); // sys + hello + hi + output
        assert!(!needs_compaction(&conv));

        let conv = clear(conv);
        assert_eq!(conv.message_count(), 0);
    }

    // 16. compact_backward_compat -- old compact function
    #[test]
    fn compact_backward_compat() {
        let conv = new_conversation(None, None, None);
        let conv = add_user(conv, "msg1".into());
        let conv = compact(conv, "summary".into());

        let msgs = to_messages(&conv);
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].content.contains("[Conversation summary]"));
        assert!(msgs[0].content.contains("summary"));
    }

    // 17. empty serialize -- no messages produces empty string
    #[test]
    fn serialize_empty() {
        let buf = ConversationBuffer::new(ConversationOptions::default());
        assert_eq!(buf.serialize(), "");
    }

    // 18. trim does not affect count when below max
    #[test]
    fn trim_no_op_when_below_max() {
        let buf = ConversationBuffer::new(ConversationOptions {
            max_messages: Some(5),
            ..ConversationOptions::default()
        });
        let buf = buf.add_user("one");
        let buf = buf.add_user("two");
        assert_eq!(buf.message_count(), 2);
    }

    // 19. load_messages clears existing messages
    #[test]
    fn load_messages_clears_existing() {
        let buf = ConversationBuffer::new(ConversationOptions::default());
        let buf = buf.add_user("old message");
        assert_eq!(buf.message_count(), 1);

        let buf = buf.load_messages(&[ConversationMessage {
            role: Role::User,
            content: "new message".to_string(),
            tool_call_id: None,
            tool_name: None,
            timestamp: None,
        }]);

        assert_eq!(buf.message_count(), 1);
        assert_eq!(buf.messages()[0].content, "new message");
    }

    // 20. needs_compaction false when equal to threshold
    #[test]
    fn needs_compaction_false_at_threshold() {
        let buf = ConversationBuffer::new(ConversationOptions {
            auto_compact_chars: Some(5),
            ..ConversationOptions::default()
        });
        let buf = buf.add_user("12345"); // exactly 5 chars
        assert!(!buf.needs_compaction()); // not strictly greater
    }

    // 21. clone produces independent copy (im::Vector structural sharing)
    #[test]
    fn clone_is_independent() {
        let buf1 = ConversationBuffer::new(ConversationOptions::default());
        let buf1 = buf1.add_user("hello");
        let buf2 = buf1.clone();
        let buf1 = buf1.add_user("world");

        assert_eq!(buf1.message_count(), 2);
        assert_eq!(buf2.message_count(), 1);
    }
}
