//! Dashboard configuration data — pure domain type.
//!
//! Plan 13-08: moved from `crate::infra::config` so `src/app/state.rs` can hold
//! `Option<DashConfig>` without importing `crate::infra::*` (G-01 hexagonal
//! boundary). Disk I/O (`load_config` / `save_config`) stays in
//! `crate::infra::config` — that's the adapter shell over this data type.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

fn default_auth_mode() -> String {
    "cloud".to_string()
}

fn default_claude_flags() -> String {
    "--dangerously-skip-permissions".to_string()
}

fn default_jira_prefix() -> String {
    "UMP".to_string()
}

fn default_app_title() -> String {
    "UMP Dash".to_string()
}

fn default_spinner_style() -> String {
    "circles".to_string()
}

pub const DEFAULT_WORKTREE_COLUMNS: [WorktreeTableColumn; 7] = [
    WorktreeTableColumn::Status,
    WorktreeTableColumn::Branch,
    WorktreeTableColumn::Ticket,
    WorktreeTableColumn::Dir,
    WorktreeTableColumn::Task,
    WorktreeTableColumn::CacheStatus,
    WorktreeTableColumn::Cache,
];

fn default_worktree_columns() -> Vec<WorktreeTableColumn> {
    DEFAULT_WORKTREE_COLUMNS.to_vec()
}

/// Default gitignored local files seeded into every newly-created worktree.
/// `pub` so the composition root can fall back to it when no config is loaded.
pub fn default_seed_files() -> Vec<String> {
    vec![
        ".env".to_string(),
        "android/keystore/release.keystore".to_string(),
        "android/keystore/debug.keystore".to_string(),
    ]
}

fn deserialize_worktree_columns<'de, D>(
    deserializer: D,
) -> Result<Vec<WorktreeTableColumn>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let columns = Vec::<WorktreeTableColumn>::deserialize(deserializer)?;
    let mut seen = Vec::new();
    for column in &columns {
        if seen.contains(column) {
            return Err(serde::de::Error::custom(format!(
                "duplicate worktree table column: {column:?}"
            )));
        }
        seen.push(*column);
    }
    Ok(columns)
}

/// Worktree table columns. The configured list controls both display order and
/// visibility; omit a column name to hide it.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeTableColumn {
    Status,
    Branch,
    Ticket,
    Dir,
    Task,
    CacheStatus,
    Cache,
    AndroidCacheStatus,
    AndroidCache,
}

/// Application configuration stored in ~/.config/ump-dash/config.toml.
///
/// Security note: this file is written with 0600 permissions on Unix because
/// `jira_token` is a credential. Never log or display the token value.
#[derive(Debug, Deserialize, Serialize)]
pub struct DashConfig {
    /// Base URL for the JIRA instance, e.g. "https://example.atlassian.net"
    pub jira_base_url: String,

    /// JIRA account email address. Required for Cloud (Basic Auth), not used
    /// for Data Center (Bearer).
    #[serde(default)]
    pub jira_email: Option<String>,

    /// JIRA API token (Cloud) or Personal Access Token (Data Center).
    pub jira_token: String,

    /// Authentication mode: "cloud" (Basic Auth email:token) or "datacenter"
    /// (Bearer PAT). Defaults to "cloud" if not specified in the config file.
    #[serde(default = "default_auth_mode")]
    pub auth_mode: String,

    /// Command-line flags to pass when launching Claude Code (e.g.,
    /// "--dangerously-skip-permissions").
    #[serde(default = "default_claude_flags")]
    pub claude_flags: String,

    /// Absolute path to the React Native monorepo root (supports ~/). If
    /// None, repo_root will remain an empty PathBuf and worktree listing will
    /// fail gracefully.
    #[serde(default)]
    pub repo_root: Option<String>,

    /// JIRA project key prefix used in branch names (e.g., "UMP" for
    /// UMP-1234). Defaults to "UMP" to preserve backward compatibility with
    /// existing configs.
    #[serde(default = "default_jira_prefix")]
    pub jira_project_prefix: String,

    /// Title shown in the dashboard header. Defaults to "UMP Dash".
    #[serde(default = "default_app_title")]
    pub app_title: String,

    /// When true, automatically accept sync-before-run and sync-before-metro
    /// prompts instead of showing a confirmation modal. Defaults to false.
    #[serde(default)]
    pub auto_sync: bool,

    /// Spinner glyph set for live task indicators: `"circles"` (half-circles
    /// ◐◓◑◒, the default) or `"braille"`/`"dots"` (⠋⠙⠹⠸⠼⠴, guaranteed
    /// single-cell width). Parsed via `ui::indicators::SpinnerStyle::from_config`.
    #[serde(default = "default_spinner_style")]
    pub spinner_style: String,

    /// Worktree table columns in display order. Omit names to hide columns.
    #[serde(
        default = "default_worktree_columns",
        deserialize_with = "deserialize_worktree_columns"
    )]
    pub columns: Vec<WorktreeTableColumn>,

    /// Gitignored local files copied into each newly-created worktree so it
    /// builds — the repo's `.gitignore` excludes them, so git can't carry them
    /// across. Paths are relative to the repo root. Defaults to `.env` plus the
    /// Android keystores; override in config.toml to seed a different set.
    #[serde(default = "default_seed_files")]
    pub seed_files: Vec<String>,
}

impl DashConfig {
    /// Resolves `repo_root` to a `PathBuf`, expanding `~/` to the home
    /// directory. Returns `None` when `repo_root` is not set in config.
    pub fn repo_root_path(&self) -> Option<std::path::PathBuf> {
        self.repo_root.as_ref().map(|s| {
            if let Some(rest) = s.strip_prefix("~/") {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
                std::path::PathBuf::from(home).join(rest)
            } else {
                std::path::PathBuf::from(s)
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_config(extra: &str) -> DashConfig {
        toml::from_str(&format!(
            r#"
jira_base_url = "https://example.atlassian.net"
jira_token = "token"
{extra}
"#
        ))
        .unwrap()
    }

    #[test]
    fn columns_default_to_current_worktree_table_order() {
        let config = parse_config("");

        assert_eq!(config.columns, DEFAULT_WORKTREE_COLUMNS);
    }

    #[test]
    fn columns_config_controls_order_and_visibility() {
        let config =
            parse_config(r#"columns = ["ticket", "branch", "cache_status", "cache", "task"]"#);

        assert_eq!(
            config.columns,
            vec![
                WorktreeTableColumn::Ticket,
                WorktreeTableColumn::Branch,
                WorktreeTableColumn::CacheStatus,
                WorktreeTableColumn::Cache,
                WorktreeTableColumn::Task,
            ]
        );
    }

    #[test]
    fn seed_files_default_to_env_and_keystores() {
        let config = parse_config("");

        assert_eq!(config.seed_files, default_seed_files());
    }

    #[test]

    fn seed_files_config_overrides_default() {
        let config = parse_config(r#"seed_files = [".env.local", "fastlane/.env"]"#);

        assert_eq!(config.seed_files, vec![".env.local", "fastlane/.env"]);
    }

    #[test]

    fn columns_config_accepts_android_cache_columns() {
        let config = parse_config(
            r#"columns = ["ticket", "android_cache_status", "android_cache", "task"]"#,
        );

        assert_eq!(
            config.columns,
            vec![
                WorktreeTableColumn::Ticket,
                WorktreeTableColumn::AndroidCacheStatus,
                WorktreeTableColumn::AndroidCache,
                WorktreeTableColumn::Task,
            ]
        );
    }

    #[test]
    fn default_columns_end_with_cache_status_and_cache() {
        let config = parse_config("");

        assert_eq!(
            config.columns.as_slice(),
            [
                WorktreeTableColumn::Status,
                WorktreeTableColumn::Branch,
                WorktreeTableColumn::Ticket,
                WorktreeTableColumn::Dir,
                WorktreeTableColumn::Task,
                WorktreeTableColumn::CacheStatus,
                WorktreeTableColumn::Cache,
            ]
        );
    }

    #[test]
    fn unknown_column_name_rejects_config() {
        let config = r#"
jira_base_url = "https://example.atlassian.net"
jira_token = "token"
columns = ["ticket", "owner"]
"#;

        assert!(toml::from_str::<DashConfig>(config).is_err());
    }

    #[test]
    fn duplicate_column_name_rejects_config() {
        let config = r#"
jira_base_url = "https://example.atlassian.net"
jira_token = "token"
columns = ["ticket", "ticket"]
"#;

        assert!(toml::from_str::<DashConfig>(config).is_err());
    }
}
