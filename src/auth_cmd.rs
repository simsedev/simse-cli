//! CLI authentication commands — `simse login` and `simse logout`.
//!
//! These run outside the main CLI as interactive terminal flows.

use simse_core::remote::auth::{AuthClient, AuthState};
use simse_core::remote::error::RemoteError;

/// Auth service endpoint.
pub const AUTH_URL: &str = "https://auth.simse.dev";

/// API gateway endpoint.
pub const API_URL: &str = "https://api.simse.dev";

/// Run the interactive device-login flow — GitHub-CLI-style.
///
/// Requests a device token, prints the approval URL and opens it in the
/// browser, then polls until the user approves. Returns the authenticated
/// [`AuthState`]; the caller is responsible for persisting it.
///
/// This is the single device-login implementation: `simse login` and the
/// first-run onboarding flow both call it so they present an identical UX.
/// It prints to stdout — call it only from a plain-terminal context, never
/// from inside the running TUI (the in-REPL `/login` uses
/// [`AuthClient::login_device`] directly for that reason).
pub async fn device_login_interactive() -> Result<AuthState, RemoteError> {
    let device_name = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "simse-cli".to_string());

    let client = AuthClient::new();

    // Step 1 — request a device token and the approval URL.
    let pending = client
        .begin_device_login(AUTH_URL, Some(device_name))
        .await?;

    // Step 2 — point the user at the approval page: always print the URL so
    // the flow works even when no browser can be launched, then try to open
    // it automatically.
    println!();
    println!("  Open this URL to approve this device:");
    println!("  {}", pending.verification_url);
    println!();
    match open::that(&pending.verification_url) {
        Ok(_) => println!("Opened your browser. Waiting for approval..."),
        Err(_) => {
            println!("Could not open a browser — visit the URL above. Waiting for approval...")
        }
    }

    // Step 3 — poll until the user approves at the page.
    client
        .complete_device_login(AUTH_URL, API_URL, pending)
        .await
        .map(|(_client, state)| state)
        .map_err(|(_client, e)| e)
}

/// Run the `simse login` device flow. Opens a browser for the user to approve.
///
/// Returns 0 on success, 1 on failure.
pub async fn run_login() -> i32 {
    if let Some(state) = crate::auth::load_auth() {
        println!("Already logged in as {} ({})", state.email, state.user_id);
        println!("Run `simse logout` first to log in with a different account.");
        return 0;
    }

    println!("Logging in to simse...");

    match device_login_interactive().await {
        Ok(state) => {
            if let Err(e) = crate::auth::save_auth(&state) {
                eprintln!("error: failed to save credentials: {e}");
                return 1;
            }
            println!("Logged in as {}", state.email);
            0
        }
        Err(e) => {
            eprintln!("error: login failed: {e}");
            1
        }
    }
}

/// Clear stored authentication credentials.
///
/// Returns 0 on success, 1 on failure.
pub fn run_logout() -> i32 {
    match crate::auth::load_auth() {
        Some(state) => {
            if let Err(e) = crate::auth::clear_auth() {
                eprintln!("error: failed to clear credentials: {e}");
                return 1;
            }
            println!("Logged out (was {})", state.email);
            0
        }
        None => {
            println!("Not logged in.");
            0
        }
    }
}
