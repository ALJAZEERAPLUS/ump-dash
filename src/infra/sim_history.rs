//! Simulator usage history — tracks last-used simulators for sort-by-recent.
//! Persists to ~/.config/ump-dash/sim_history.json as a JSON array of UDIDs.

use crate::infra::config::config_dir;

fn sim_history_path() -> std::path::PathBuf {
    config_dir().join("sim_history.json")
}

/// Loads the simulator history as an ordered list of UDIDs (most recent first).
/// Returns empty vec if file doesn't exist or is malformed.
pub fn load_sim_history() -> Vec<String> {
    let path = sim_history_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// Records a simulator UDID as most recently used.
/// Pushes to front, deduplicates, truncates to 20 entries.
pub fn record_sim_used(udid: &str) -> anyhow::Result<()> {
    let mut history = load_sim_history();
    history.retain(|u| u != udid);
    history.insert(0, udid.to_string());
    history.truncate(20);
    let json = serde_json::to_string_pretty(&history)?;
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    std::fs::write(sim_history_path(), json)?;
    Ok(())
}
