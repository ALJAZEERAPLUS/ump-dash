// src/infra/jira.rs
//
// Concrete JIRA HTTP client (`HttpJiraClient`) — adapter for
// `crate::domain::ports::jira_port::JiraPort`.
//
// Supports two authentication modes:
//   - "cloud"      → Basic Auth (email:api_token)   — Atlassian Cloud instances
//   - "datacenter" → Bearer Auth (PAT)               — JIRA Data Center / Server
//
// The client never panics and never surfaces errors to the TUI layer.
// Any failure (network, auth, parse) results in None from fetch_title().

#![allow(dead_code)]

use crate::domain::ports::jira_port::JiraPort;
use crate::infra::config::DashConfig;
use async_trait::async_trait;

/// Concrete JIRA client that makes real HTTP requests using reqwest.
#[derive(Debug)]
pub struct HttpJiraClient {
    client: reqwest::Client,
    base_url: String,
    auth_mode: String,
    email: Option<String>,
    token: String,
}

impl HttpJiraClient {
    /// Constructs an `HttpJiraClient` from the loaded `DashConfig`.
    ///
    /// Builds a bare `reqwest::Client` with no default auth headers — auth is
    /// applied per-request in `fetch_title` for clarity and correctness.
    pub fn new(config: &DashConfig) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            .build()?;

        Ok(Self {
            client,
            base_url: config.jira_base_url.trim_end_matches('/').to_string(),
            auth_mode: config.auth_mode.clone(),
            email: config.jira_email.clone(),
            token: config.jira_token.clone(),
        })
    }
}

#[async_trait]
impl JiraPort for HttpJiraClient {
    async fn fetch_title(&self, ticket_key: &str) -> Option<String> {
        let url = format!(
            "{}/rest/api/3/issue/{}?fields=summary",
            self.base_url, ticket_key
        );

        let request = self.client.get(&url);

        // Apply authentication based on the configured auth mode.
        let request = if self.auth_mode == "datacenter" {
            // Data Center / Server: Personal Access Token sent as Bearer.
            request.bearer_auth(&self.token)
        } else {
            // Cloud: Basic Auth using email and API token.
            request.basic_auth(
                self.email.as_deref().unwrap_or(""),
                Some(&self.token),
            )
        };

        let response = request.send().await.ok()?;
        let json: serde_json::Value = response.json().await.ok()?;
        let title = json["fields"]["summary"].as_str()?.to_string();
        Some(title)
    }
}

/// Returns `true` when the process is running inside a tmux session.
///
/// Tmux sets the `TMUX` environment variable to the path of the server socket,
/// so its presence is a reliable indicator of a tmux session.
pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}
