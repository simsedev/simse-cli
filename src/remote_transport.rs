//! Transport abstraction for sending JSON-RPC messages.
//!
//! The [`MessageSender`] trait abstracts the difference between a direct
//! connection and a tunnel relay connection. The tunnel handler in
//! `main.rs` uses this trait through `handlers::dispatch`.

use std::sync::Arc;

use async_trait::async_trait;

use simse_core::remote::tunnel::TunnelClient;

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Fire-and-forget message sender used by shared handlers to push
/// streaming notifications (deltas, tool calls, stream-end) back to the
/// connected client.
#[async_trait]
pub trait MessageSender: Send + Sync {
    async fn send_message(&self, text: &str);
}

// ---------------------------------------------------------------------------
// TunnelSender — wraps Arc<TunnelClient>
// ---------------------------------------------------------------------------

/// Sends messages through the API-backed gRPC tunnel relay.
pub struct TunnelSender {
    tunnel: Arc<TunnelClient>,
}

impl TunnelSender {
    pub fn new(tunnel: Arc<TunnelClient>) -> Self {
        Self { tunnel }
    }
}

#[async_trait]
impl MessageSender for TunnelSender {
    async fn send_message(&self, text: &str) {
        let _ = self.tunnel.send_message(text).await;
    }
}
