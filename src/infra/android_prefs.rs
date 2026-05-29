//! Android run preferences — persists last-used build mode for auto-application.
//! Persists to ~/.config/ump-dash/android_prefs.json as JSON: {"mode": "debugOptimized"}.

use crate::infra::config::config_dir;

fn android_prefs_path() -> std::path::PathBuf {
    config_dir().join("android_prefs.json")
}

/// Loads the saved Android mode string (e.g. "debugOptimized").
/// Returns None if file doesn't exist or is malformed.
pub fn load_android_mode() -> Option<String> {
    let path = android_prefs_path();
    let contents = std::fs::read_to_string(&path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&contents).ok()?;
    value.get("mode")?.as_str().map(|s| s.to_string())
}

/// Saves the Android mode string to disk.
/// Creates the config dir if it doesn't exist.
pub fn save_android_mode(mode: &str) -> anyhow::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::json!({ "mode": mode });
    std::fs::write(android_prefs_path(), serde_json::to_string_pretty(&json)?)?;
    Ok(())
}
