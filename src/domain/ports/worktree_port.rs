//! Worktree port — git worktree CRUD + remote branch listing (F-104).
//!
//! Wraps the five `pub async fn` free functions currently in
//! `src/infra/worktrees.rs`. `GitWorktreeAdapter` in `src/infra/worktrees.rs`
//! is the production impl; plan 13-08 rewires app.rs consumers to depend on
//! this trait instead of calling the free fns directly.
//!
//! Note: `check_stale`, `check_stale_pods`, and `parse_worktree_porcelain` are
//! NOT on this trait — they are staleness-check helpers / pure parsers used
//! internally by the adapter, not worktree CRUD ops.

#![allow(dead_code)]

use crate::domain::worktree::Worktree;
use std::path::{Path, PathBuf};

/// Trait boundary for git worktree CRUD + remote branch enumeration.
#[async_trait::async_trait]
pub trait WorktreePort: Send + Sync {
    /// Runs `git worktree list --porcelain` and returns parsed worktrees.
    async fn list(&self, repo_root: &Path) -> anyhow::Result<Vec<Worktree>>;

    /// Removes a worktree (`git worktree remove --force`) and prunes metadata.
    async fn remove(&self, repo_root: &Path, worktree_path: &Path) -> anyhow::Result<()>;

    /// Creates a worktree as a sibling of `repo_root` using `branch_name`.
    /// Creates the branch with `-b` on first try; retries with an existing
    /// branch checkout if `-b` fails because the branch already exists.
    async fn add(&self, repo_root: &Path, branch_name: &str) -> anyhow::Result<PathBuf>;

    /// Creates a worktree with a new branch based on `base_branch`.
    /// Implementations may prefer a remote-tracking branch when available, but
    /// local-only base branches are valid.
    async fn add_new_branch(
        &self,
        repo_root: &Path,
        new_branch: &str,
        base_branch: &str,
    ) -> anyhow::Result<PathBuf>;

    /// Creates a review worktree by force-updating the local PR branch to the
    /// current GitHub PR head and adding a worktree at `worktree_name`.
    async fn add_review_worktree(
        &self,
        repo_root: &Path,
        pr_number: u64,
        branch_name: &str,
        head_oid: &str,
        worktree_name: &str,
    ) -> anyhow::Result<PathBuf>;

    /// Lists remote branches by running `git branch -r` in `repo_root`.
    /// Strips the `origin/` prefix and excludes HEAD pointers.
    async fn list_remote_branches(&self, repo_root: &Path) -> anyhow::Result<Vec<String>>;
}
