//! `simse remote connect` — connect to the cloud relay as a remote.
//!
//! Opens a gRPC tunnel to the API service so the web dashboard
//! can interact with this local simse instance (shell, files, plugins, etc.).
//!
//! When the library is enabled, a [`MemorySyncClient`] is started to
//! synchronize adaptive memory entries with the cloud via the tunnel.

use std::sync::Arc;

use simse_core::remote::tunnel::TunnelClient;
use tokio::sync::Mutex;

use crate::config::LoadedConfig;
use crate::event_loop::CliRuntime;
use crate::handlers::SessionState;
use crate::remote_transport::{MessageSender, TunnelSender};
#[cfg(feature = "adaptive")]
use simse_core::memory_sync;

/// API endpoint for the remote tunnel gRPC stream.
const API_URL: &str = "https://api.simse.dev";

/// Run the remote connect loop. Blocks until Ctrl+C.
pub async fn run_remote_connect(cfg: LoadedConfig) -> i32 {
    let auth = match crate::auth::load_auth() {
        Some(a) => a,
        None => {
            eprintln!("error: not logged in. Run `simse login` first.");
            return 1;
        }
    };

    let relay_url = API_URL;

    eprintln!("Connecting to relay as {}...", auth.email);

    let tunnel = Arc::new(TunnelClient::new());

    // Subscribe to incoming messages before connecting.
    let mut incoming_rx = tunnel.subscribe_messages().await;

    // Connect to the relay.
    let tunnel_id = match tunnel.connect(relay_url, &auth.access_token).await {
        Ok(id) => id,
        Err(e) => {
            eprintln!("error: failed to connect to relay: {e}");
            return 1;
        }
    };

    // Spawn token refresh loop to keep the tunnel JWT valid.
    crate::event_loop::spawn_token_refresh(tunnel.clone());

    eprintln!("Connected. Tunnel ID: {tunnel_id}");
    eprintln!("This instance is now accessible from your simse dashboard.");
    eprintln!("Press Ctrl+C to disconnect.");

    // Start memory sync client — bridges local library to cloud via tunnel.
    #[cfg(feature = "adaptive")]
    let sync_handle = {
        let tunnel_for_sync = tunnel.clone();
        let sync_cfg = simse_core::memory_sync::MemorySyncConfig {
            data_dir: cfg.data_dir.clone(),
            library_enabled: cfg.library.enabled,
            duplicate_threshold: cfg.library.duplicate_threshold,
            embedding_model: cfg.embedding_model.clone(),
            max_results: cfg.library.max_results,
            similarity_threshold: cfg.library.similarity_threshold,
            flush_interval_ms: cfg.library.flush_interval_ms,
        };
        memory_sync::start_memory_sync(&sync_cfg, auth.user_id, move |msg| {
            let t = tunnel_for_sync.clone();
            tokio::spawn(async move {
                let _ = t.send_message(&msg).await;
            });
        })
        .await
    };

    // Set up the runtime for handling remote/* requests.
    let rt = Arc::new(Mutex::new(CliRuntime::new(cfg)));
    let sessions = Arc::new(SessionState::new());
    let sender: Arc<dyn MessageSender> = Arc::new(TunnelSender::new(tunnel.clone()));

    // Spawn the message handler loop.
    let handler = tokio::spawn(async move {
        while let Some(msg) = incoming_rx.recv().await {
            // Route memory:* sync responses to the sync client first.
            #[cfg(feature = "adaptive")]
            if let Some(ref handle) = sync_handle
                && memory_sync::try_handle_sync_message(handle, &msg)
            {
                continue;
            }

            let response = crate::handlers::dispatch(&msg, &rt, &sessions, &sender).await;
            if !response.is_empty() {
                sender.send_message(&response).await;
            }
        }
    });

    // Wait for Ctrl+C.
    tokio::signal::ctrl_c().await.ok();

    eprintln!("\nDisconnecting...");
    let _ = tunnel.disconnect().await;
    handler.abort();

    eprintln!("Disconnected.");
    0
}
