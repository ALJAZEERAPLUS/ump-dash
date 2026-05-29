// src/infra/config.rs
//
// Dashboard configuration persistence.
// Config is stored at ~/.config/ump-dash/config.toml with 0600 permissions
// because it contains JIRA credentials (token/email).
//
// Plan 13-08: the `DashConfig` *type* lives in `crate::domain::dash_config`
// (pure data, no I/O). This module owns the on-disk persistence
// (`load_config` / `save_config` / `config_dir`) and re-exports the type
// for backward compat.

#![allow(dead_code)]

pub use crate::domain::dash_config::DashConfig;

/// Returns the `~/.config/ump-dash/` config directory path.
pub fn config_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "~".to_string());
    std::path::PathBuf::from(home).join(".config").join("ump-dash")
}

/// Loads the dashboard configuration from disk.
///
/// Returns `Ok(None)` when the config file does not exist — callers should
/// treat this as "not configured" and either prompt the user or skip JIRA
/// integration silently. All other I/O errors are propagated.
pub fn load_config() -> anyhow::Result<Option<DashConfig>> {
    let path = config_dir().join("config.toml");
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let config: DashConfig = toml::from_str(&contents)?;
            Ok(Some(config))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Saves the dashboard configuration to disk as TOML.
///
/// Creates the config directory if it does not already exist.
/// On Unix systems the file is immediately chmod'd to 0600 so that the JIRA
/// token is not world-readable.
pub fn save_config(config: &DashConfig) -> anyhow::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.toml");
    let toml_str = toml::to_string_pretty(config)?;
    std::fs::write(&path, toml_str)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    #[cfg(not(unix))]
    {
        // Non-Unix platforms do not support POSIX permission bits.
        // The file is written but permissions cannot be restricted.
        let _ = &path;
    }

    Ok(())
}
