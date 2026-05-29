// src/infra/jira_cache.rs
//
// Persistence for the JIRA ticket title cache.
// Titles are stored at ~/.config/ump-dash/jira_cache.json as a flat JSON object:
// { "PROJ-1234": "Fix login timeout", "PROJ-5678": "Add dark mode", ... }
//
// No 0600 permissions needed — ticket titles are not credentials.

#![allow(dead_code)]

use crate::infra::config::config_dir;
use std::collections::HashMap;
use std::path::PathBuf;

/// Returns the path to the JIRA title cache JSON file.
pub fn cache_path() -> PathBuf {
    config_dir().join("jira_cache.json")
}

/// Loads the JIRA title cache from disk.
///
/// Returns an empty HashMap if the file does not exist. All other I/O errors
/// are propagated so the caller can decide whether to log and ignore or surface.
pub fn load_jira_cache() -> anyhow::Result<HashMap<String, String>> {
    let path = cache_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => {
            let map: HashMap<String, String> = serde_json::from_str(&contents)?;
            Ok(map)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(e.into()),
    }
}

/// Saves the JIRA title cache to disk as pretty-printed JSON.
///
/// Creates the config directory if it does not already exist.
pub fn save_jira_cache(cache: &HashMap<String, String>) -> anyhow::Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir)?;
    let json = serde_json::to_string_pretty(cache)?;
    std::fs::write(cache_path(), json)?;
    Ok(())
}
