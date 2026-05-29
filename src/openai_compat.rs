//! OpenAI-compatible HTTP client implementing `AcpClient`.
//!
//! Used with `--provider` to connect to Ollama, vLLM, LM Studio, or any
//! endpoint that speaks the OpenAI `/v1/chat/completions` API.

use async_trait::async_trait;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use simse_core::agentic_loop::{
    AcpClient, GenerateResponse, Message, MessageRole, PromptCacheConfig, SamplingConfig,
    StreamDeltaCallback, TokenUsage,
};
use simse_core::error::SimseError;

/// An `AcpClient` implementation that talks to any OpenAI-compatible HTTP API.
pub struct OpenAiCompatClient {
    base_url: String,
    model: String,
    http: Client,
}

impl OpenAiCompatClient {
    /// Create a new client.
    ///
    /// `base_url` should be the root URL without trailing slash, e.g.
    /// `http://localhost:11434` for Ollama or `https://api.openai.com`.
    /// `model` is the model name to use for completions.
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            http: Client::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// OpenAI chat completions types
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f64>,
}

#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct ChatChunk {
    choices: Vec<ChunkChoice>,
    #[serde(default)]
    usage: Option<ChunkUsage>,
}

#[derive(Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
}

#[derive(Deserialize)]
struct ChunkDelta {
    #[serde(default)]
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChunkUsage {
    #[serde(default, alias = "prompt_tokens")]
    input_tokens: Option<u64>,
    #[serde(default, alias = "completion_tokens")]
    output_tokens: Option<u64>,
    #[serde(default)]
    total_tokens: Option<u64>,
}

// ---------------------------------------------------------------------------
// AcpClient implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl AcpClient for OpenAiCompatClient {
    async fn generate(
        &self,
        messages: &[Message],
        system: Option<&str>,
        on_delta: Option<StreamDeltaCallback>,
        sampling: Option<&SamplingConfig>,
        _cache_config: Option<&PromptCacheConfig>,
    ) -> Result<GenerateResponse, SimseError> {
        // Build message list with optional system prompt.
        let mut chat_messages = Vec::new();
        if let Some(sys) = system {
            chat_messages.push(ChatMessage {
                role: "system".into(),
                content: sys.to_string(),
            });
        }
        for msg in messages {
            chat_messages.push(ChatMessage {
                role: match msg.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::System => "system",
                    MessageRole::Tool => "tool",
                }
                .into(),
                content: msg.content.clone(),
            });
        }

        let request = ChatRequest {
            model: self.model.clone(),
            messages: chat_messages,
            stream: true,
            temperature: sampling.and_then(|s| s.temperature),
            max_tokens: sampling.and_then(|s| s.max_tokens),
            top_p: sampling.and_then(|s| s.top_p),
        };

        let url = format!("{}/v1/chat/completions", self.base_url);
        let response = self
            .http
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| SimseError::other(format!("provider request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(SimseError::other(format!(
                "provider returned {status}: {body}"
            )));
        }

        // Stream SSE response.
        let mut full_text = String::new();
        let mut usage = None;
        let mut stream = response.bytes_stream();

        // Raw byte buffer: SSE lines are newline-delimited and `\n` (0x0A) is
        // ASCII, so it never falls inside a multi-byte UTF-8 sequence. Decoding
        // per complete line (rather than per network chunk) avoids corrupting
        // characters that straddle a chunk boundary.
        let mut buffer: Vec<u8> = Vec::new();

        // Cap a single un-terminated line so a malformed/adversarial provider
        // that never sends `\n` can't grow the buffer until OOM.
        const MAX_SSE_LINE_BYTES: usize = 8 * 1024 * 1024;
        'stream: while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| SimseError::other(format!("stream error: {e}")))?;
            buffer.extend_from_slice(&bytes);
            if buffer.len() > MAX_SSE_LINE_BYTES && !buffer.contains(&b'\n') {
                return Err(SimseError::other(
                    "stream error: SSE line exceeded 8 MiB without a newline",
                ));
            }

            // Process complete SSE lines from the buffer.
            while let Some(nl) = buffer.iter().position(|&b| b == b'\n') {
                let line_bytes: Vec<u8> = buffer.drain(..=nl).collect();
                let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1])
                    .trim()
                    .to_string();

                if line.is_empty() || line.starts_with(':') {
                    continue;
                }

                let data = if let Some(d) = line.strip_prefix("data: ") {
                    d.trim()
                } else {
                    continue;
                };

                if data == "[DONE]" {
                    break 'stream;
                }

                if let Ok(chunk) = serde_json::from_str::<ChatChunk>(data) {
                    if let Some(choice) = chunk.choices.first()
                        && let Some(content) = &choice.delta.content
                    {
                        full_text.push_str(content);
                        if let Some(ref cb) = on_delta {
                            cb(content);
                        }
                    }
                    if let Some(u) = chunk.usage {
                        usage = Some(TokenUsage {
                            input_tokens: u.input_tokens,
                            output_tokens: u.output_tokens,
                            total_tokens: u.total_tokens,
                            cache_creation_input_tokens: None,
                            cache_read_input_tokens: None,
                        });
                    }
                }
            }
        }

        Ok(GenerateResponse {
            text: full_text,
            usage,
        })
    }
}
