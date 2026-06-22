//! GitHub pull request listing port.
//!
//! The app layer uses this trait to request PR metadata; concrete `gh` process
//! execution lives in `src/infra/review.rs`.

use crate::domain::review::{PullRequest, PullRequestFilter};
use std::path::Path;

#[async_trait::async_trait]
pub trait ReviewPort: Send + Sync {
    async fn list_pull_requests(
        &self,
        repo_root: &Path,
        filter: PullRequestFilter,
    ) -> anyhow::Result<Vec<PullRequest>>;
}
