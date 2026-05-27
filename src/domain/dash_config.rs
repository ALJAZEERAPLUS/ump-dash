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
    "RN Dash".to_string()
}

fn default_spinner_style() -> String {
    "circles".to_string()
}

fn default_column_status() -> u16 {
    4
}

fn default_column_branch() -> u16 {
    20
}

fn default_column_ticket() -> u16 {
    20
}

fn default_column_dir() -> u16 {
    16
}

fn default_column_task() -> u16 {
    20
}

/// Worktree table column widths.
///
/// `ticket` is used as the minimum width for the flexible ticket/title column;
/// the other values are fixed column widths.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorktreeTableColumns {
    #[serde(default = "default_column_status")]
    pub status: u16,
    #[serde(default = "default_column_branch")]
    pub branch: u16,
    #[serde(default = "default_column_ticket")]
    pub ticket: u16,
    #[serde(default = "default_column_dir")]
    pub dir: u16,
    #[serde(default = "default_column_task")]
    pub task: u16,
}

impl Default for WorktreeTableColumns {
    fn default() -> Self {
        Self {
            status: default_column_status(),
            branch: default_column_branch(),
            ticket: default_column_ticket(),
            dir: default_column_dir(),
            task: default_column_task(),
        }
    }
}

/// Application configuration stored in ~/.config/rn-dash/config.toml.
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

    /// Title shown in the dashboard header. Defaults to "RN Dash".
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

    /// Worktree table column widths. Defaults preserve the built-in layout.
    #[serde(default)]
    pub columns: WorktreeTableColumns,
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
    fn columns_default_to_current_worktree_table_layout() {
        let config = parse_config("");

        assert_eq!(
            config.columns,
            WorktreeTableColumns {
                status: 4,
                branch: 20,
                ticket: 20,
                dir: 16,
                task: 20,
            }
        );
    }

    #[test]
    fn partial_columns_config_uses_per_field_defaults() {
        let config = parse_config(
            r#"
[columns]
branch = 32
ticket = 40
"#,
        );

        assert_eq!(config.columns.status, 4);
        assert_eq!(config.columns.branch, 32);
        assert_eq!(config.columns.ticket, 40);
        assert_eq!(config.columns.dir, 16);
        assert_eq!(config.columns.task, 20);
    }
}
