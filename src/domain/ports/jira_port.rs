//! JiraPort — domain-layer trait boundary for JIRA title fetching.
//!
//! ARCH-02: Domain and app layers depend only on this trait; `infra::jira`
//! supplies the concrete `HttpJiraClient` adapter. Tests may inject fakes
//! without any real HTTP calls.

#![allow(dead_code)]

use async_trait::async_trait;

/// Abstraction over JIRA title fetching.
///
/// Implementing this as a trait lets unit tests inject a fake client without
/// making real HTTP calls. The bound `Send + Sync` is required so that
/// implementations can be stored in `Arc<dyn JiraPort>` in the app state.
/// The `Debug` bound is required because `AppState` derives `Debug`.
#[async_trait]
pub trait JiraPort: Send + Sync + std::fmt::Debug {
    /// Fetches the summary (title) for the given JIRA ticket key (e.g. "UMP-1234").
    ///
    /// Returns `None` on any failure — network error, auth error, missing key, or
    /// unexpected JSON shape. The TUI should treat `None` as "title not available"
    /// and fall back to displaying the raw ticket key.
    async fn fetch_title(&self, ticket_key: &str) -> Option<String>;
}
