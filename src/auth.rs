use simse_core::remote::auth::AuthState;
use std::path::PathBuf;

fn auth_file_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".simse").join("auth.json")
}

pub fn save_auth(state: &AuthState) -> std::io::Result<()> {
    let path = auth_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
        // The data directory holds credentials and session history — keep it
        // owner-only.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }

    let json = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;

    // Create the file with owner-only permissions up front so the token is
    // never briefly world-readable. `mode()` only applies when the file is
    // created, so an existing file still gets an explicit chmod afterwards.
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        f.write_all(json.as_bytes())?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    }
    #[cfg(not(unix))]
    std::fs::write(&path, json)?;

    Ok(())
}

pub fn load_auth() -> Option<AuthState> {
    let path = auth_file_path();
    let json = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&json).ok()
}

pub fn clear_auth() -> std::io::Result<()> {
    let path = auth_file_path();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
