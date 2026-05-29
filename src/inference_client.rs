//! Remote inference client for the CLI.
//!
//! The CLI runs its agentic loop and tools locally; for raw token generation
//! it calls simse's hosted inference through `cloud/api` via the
//! `TunnelService.RemoteGenerate` RPC ("inference proxy for remote CLI
//! devices"). Transport is gRPC-Web — the api is a fetch-based (workerd)
//! server that speaks Connect / gRPC-Web, not raw h2 gRPC. See
//! [`simse_core::remote::grpc_web`].

use async_trait::async_trait;
use prost::Message as _;
use reqwest::Client;
use tracing::{debug, warn};

use simse_core::agentic_loop::{
    AcpClient as AcpClientTrait, GenerateResponse, Message, MessageRole, PromptCacheConfig,
    SamplingConfig, StreamDeltaCallback, TokenUsage,
};
use simse_core::error::SimseError;
use simse_core::remote::grpc_web::{FLAG_TRAILER, FrameParser, GrpcWebClient, parse_trailer};
use simse_core::remote::proto::quantiz::inference as pb;

/// Fully-qualified gRPC service + the inference-proxy method.
const TUNNEL_SERVICE: &str = "simse.tunnel.TunnelService";
const REMOTE_GENERATE: &str = "RemoteGenerate";

/// Decode a base64url-encoded JWT payload section.
fn base64_decode_payload(payload: &str) -> Result<Vec<u8>, String> {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|e| format!("base64 decode: {e}"))
}

/// Map the agentic-loop sampling config to inference `SamplingParams`.
fn sampling_params(sampling: Option<&SamplingConfig>) -> pb::SamplingParams {
    let mut p = pb::SamplingParams::default();
    if let Some(s) = sampling {
        p.temperature = s.temperature;
        p.top_p = s.top_p;
        p.max_tokens = s.max_tokens.map(|m| m as i32);
    }
    p
}

/// Inference client that reaches `TunnelService.RemoteGenerate` over gRPC-Web.
#[derive(Clone)]
pub struct RemoteInferenceClient {
    /// gRPC-Web transport bound to the api base URL.
    grpc: GrpcWebClient,
    token: std::sync::Arc<tokio::sync::Mutex<String>>,
    refresh_token: String,
    /// Configured model identity (empty = let the server pick).
    model: String,
}

impl RemoteInferenceClient {
    const API_URL: &'static str = "https://api.simse.dev";

    pub async fn connect(token: &str, model: String) -> Result<Self, String> {
        // Load the refresh token from auth state for automatic renewal.
        let auth = crate::auth::load_auth();
        let refresh_token = auth.map(|a| a.refresh_token).unwrap_or_default();
        Self::connect_to(Self::API_URL, token, &refresh_token, model).await
    }

    pub async fn connect_to(
        api_url: &str,
        token: &str,
        refresh_token: &str,
        model: String,
    ) -> Result<Self, String> {
        let http = Client::builder()
            .timeout(std::time::Duration::from_secs(300))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;

        debug!(api_url, model = %model, "Remote inference client configured");
        Ok(Self {
            grpc: GrpcWebClient::new(http, api_url),
            token: std::sync::Arc::new(tokio::sync::Mutex::new(token.to_string())),
            refresh_token: refresh_token.to_string(),
            model,
        })
    }

    /// Get the current access token, refreshing if it is expired or close to.
    async fn get_token(&self) -> String {
        let token = self.token.lock().await.clone();

        // Refresh if the JWT is expired or within 5 minutes of expiry.
        if let Some(payload) = token.split('.').nth(1)
            && let Ok(decoded) = base64_decode_payload(payload)
            && let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&decoded)
            && let Some(exp) = claims.get("exp").and_then(|v| v.as_i64())
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if exp - now < 300
                && !self.refresh_token.is_empty()
                && let Some(new_token) = self.refresh_access_token().await
            {
                return new_token;
            }
        }
        token
    }

    /// Obtain a fresh access token via the auth service's `RefreshToken` RPC.
    ///
    /// Delegates to [`AuthClient`](simse_core::remote::auth::AuthClient), which
    /// speaks gRPC-Web (the auth service is a workerd Connect server — there
    /// is no REST refresh endpoint). The refreshed access + refresh tokens
    /// are persisted back to the on-disk auth state.
    async fn refresh_access_token(&self) -> Option<String> {
        let auth = crate::auth::load_auth()?;
        let mut client = simse_core::remote::auth::AuthClient::from_state(auth);

        let new_token = match client.refresh_access_token().await {
            Ok(token) => token,
            Err(e) => {
                warn!("Token refresh failed: {e}");
                return None;
            }
        };

        *self.token.lock().await = new_token.clone();

        // Persist the refreshed access + refresh tokens to disk.
        if let Some(state) = client.state()
            && let Err(e) = crate::auth::save_auth(state)
        {
            warn!("Failed to persist refreshed access token: {e}");
        }

        debug!("Access token refreshed");
        Some(new_token)
    }
}

#[async_trait]
impl AcpClientTrait for RemoteInferenceClient {
    async fn generate(
        &self,
        messages: &[Message],
        system: Option<&str>,
        on_delta: Option<StreamDeltaCallback>,
        sampling: Option<&SamplingConfig>,
        _cache_config: Option<&PromptCacheConfig>,
    ) -> Result<GenerateResponse, SimseError> {
        let token = self.get_token().await;

        // Thin client: send raw conversation messages and let the engine
        // own templating + system-prompt injection. The `system` arg here is
        // client context (tool schemas / workspace), sent as a leading
        // system message; the engine merges it after the tier prompt.
        let mut proto_messages: Vec<pb::ChatMessage> = Vec::with_capacity(messages.len() + 1);
        if let Some(sys) = system {
            proto_messages.push(pb::ChatMessage {
                role: "system".to_string(),
                content: sys.to_string(),
            });
        }
        for m in messages {
            let role = match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Tool => "tool",
            };
            proto_messages.push(pb::ChatMessage {
                role: role.to_string(),
                content: m.content.clone(),
            });
        }

        let request = pb::GenerateRequest {
            model: self.model.clone(),
            prompt: String::new(),
            system: None,
            params: Some(sampling_params(sampling)),
            request_id: None,
            priority: None,
            stream: Some(true),
            lora_adapter: None,
            images: Vec::new(),
            messages: proto_messages,
        };

        let resp = self
            .grpc
            .open(
                TUNNEL_SERVICE,
                REMOTE_GENERATE,
                Some(token.as_str()),
                &request,
            )
            .await
            .map_err(|e| {
                warn!("Remote inference request failed: {e}");
                e
            })?;

        // Parse the gRPC-Web frame stream incrementally so token chunks reach
        // the UI as they arrive.
        use futures::StreamExt;

        let mut stream = resp.bytes_stream();
        let mut parser = FrameParser::default();
        let mut text = String::new();
        let mut usage: Option<TokenUsage> = None;

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.map_err(|e| SimseError::other(format!("stream error: {e}")))?;
            parser.push(&bytes);

            while let Some(frame) = parser.next_frame() {
                if frame.flag & FLAG_TRAILER != 0 {
                    let (status, msg) = parse_trailer(&frame.payload);
                    if status != 0 {
                        return Err(SimseError::other(format!(
                            "remote inference error {status}: {msg}"
                        )));
                    }
                    continue;
                }

                let event = pb::GenerateResponse::decode(frame.payload.as_slice())
                    .map_err(|e| SimseError::other(format!("inference decode failed: {e}")))?;

                match event.event {
                    Some(pb::generate_response::Event::Token(tok)) => {
                        if !tok.text.is_empty() {
                            if let Some(ref cb) = on_delta {
                                cb(&tok.text);
                            }
                            text.push_str(&tok.text);
                        }
                    }
                    Some(pb::generate_response::Event::Result(result)) => {
                        // The final result carries the AUTHORITATIVE full text
                        // (server-side channel-stripped + complete). The
                        // streamed token deltas are for live display only and
                        // may be truncated by the channel filter's tail-hold,
                        // so always prefer full_text for the returned response
                        // — tool-call parsing depends on the complete text.
                        if !result.full_text.is_empty() {
                            text = result.full_text;
                        }
                        if let Some(u) = result.usage {
                            usage = Some(TokenUsage {
                                input_tokens: Some(u.input_tokens),
                                output_tokens: Some(u.output_tokens),
                                total_tokens: Some(u.input_tokens + u.output_tokens),
                                cache_creation_input_tokens: None,
                                cache_read_input_tokens: None,
                            });
                        }
                    }
                    None => {}
                }
            }
        }

        Ok(GenerateResponse { text, usage })
    }
}
