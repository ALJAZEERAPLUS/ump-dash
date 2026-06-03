// src/infra/worktrees.rs
//
// Worktree enumeration: parse `git worktree list --porcelain` output into
// domain Worktree values. All I/O is behind async functions; the parser itself
// is pure (no I/O) so it can be unit-tested without a real git repo.

#![allow(dead_code)]

use crate::domain::worktree::{Worktree, WorktreeId, WorktreeMetroStatus};
use std::path::Path;

/// Pure parser. Converts `git worktree list --porcelain` text output into a
/// Vec<Worktree>. Each stanza in the output is separated by a blank line.
///
/// Example stanza:
/// ```text
/// worktree /Users/me/projects/ump
/// HEAD abc1234def5678901234567890abcdef12345678
/// branch refs/heads/feature/UMP-1234-login
/// ```
///
/// Detached HEAD stanzas have "detached" on the third line instead of a branch.
pub fn parse_worktree_porcelain(text: &str) -> anyhow::Result<Vec<Worktree>> {
    let mut worktrees = Vec::new();

    for stanza in text.split("\n\n") {
        let stanza = stanza.trim();
        if stanza.is_empty() {
            continue;
        }

        let mut path_str: Option<&str> = None;
        let mut head_sha: Option<&str> = None;
        let mut branch: Option<String> = None;
        let mut is_bare = false;

        for line in stanza.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path_str = Some(p);
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                // Take only first 7 chars for the short SHA
                head_sha = Some(&h[..h.len().min(7)]);
            } else if let Some(b) = line.strip_prefix("branch refs/heads/") {
                branch = Some(b.to_string());
            } else if line == "detached" {
                branch = Some("(detached)".to_string());
            } else if line == "bare" {
                is_bare = true;
            }
        }

        // Skip bare repos — they have no working tree content to display
        if is_bare {
            continue;
        }

        // Skip stanzas without a path (malformed output)
        let path_str = match path_str {
            Some(p) => p,
            None => continue,
        };

        let path = std::path::PathBuf::from(path_str);
        let head_sha = head_sha.unwrap_or("unknown").to_string();
        let branch = branch.unwrap_or_else(|| "(unknown)".to_string());

        // WorktreeId is derived from the path — stable across renames of the branch
        let id = WorktreeId(path_str.to_string());

        let stale = check_stale(&path);
        let stale_pods = check_stale_pods(&path);

        // jira_key is derived in app.rs WorktreesLoaded handler using the configured
        // project prefix — we don't have access to the prefix here.
        worktrees.push(Worktree {
            id,
            path,
            branch,
            head_sha,
            metro_status: WorktreeMetroStatus::Stopped, // derived later from AppState
            jira_title: None,
            stale,
            stale_pods,
            jira_key: None,
        });
    }

    Ok(worktrees)
}

/// Returns true when dependencies are stale (need `yarn install`).
///
/// Multi-sentinel approach to support different yarn versions:
/// 1. `.yarn/install-state.gz` — yarn Berry (v2/v3/v4) ALWAYS creates this on every install,
///    regardless of linker mode (pnp, node-modules, or pnpm). Most reliable Berry sentinel.
/// 2. `node_modules/.yarn-integrity` — yarn v1 (classic) sentinel
/// 3. If no sentinel found and `node_modules` absent — stale (never installed)
/// 4. If no sentinel found but `node_modules` exists — assume NOT stale (benefit of the doubt)
///
/// When a sentinel IS found, staleness = sentinel mtime < max(package.json, yarn.lock) mtime.
pub fn check_stale(worktree_path: &Path) -> bool {
    crate::domain::staleness::check_stale(worktree_path)
}

/// Returns true when pods are out of sync — same check CocoaPods' build phase uses:
/// compare `ios/Podfile.lock` contents against `ios/Pods/Manifest.lock`.
/// If they differ (or Manifest.lock is missing), pods need `pod install`.
pub fn check_stale_pods(worktree_path: &Path) -> bool {
    crate::domain::staleness::check_stale_pods(worktree_path)
}

/// Removes a worktree from git and deletes its directory.
///
/// Runs `git worktree remove --force <worktree_path>` followed by
/// `git worktree prune` to clean up stale git metadata.
///
/// The `--force` flag is required when the worktree has local modifications or an
/// untracked branch; it makes removal unconditional (analogous to `rm -rf` for the
/// git side). After the remove command the directory is gone; prune cleans any
/// leftover `.git/worktrees/<name>` entries.
pub async fn remove_worktree(repo_root: &Path, worktree_path: &Path) -> anyhow::Result<()> {
    let path_str = worktree_path.to_string_lossy().to_string();

    let output = tokio::process::Command::new("git")
        .args(["worktree", "remove", "--force", &path_str])
        .current_dir(repo_root)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree remove --force failed: {}", stderr.trim());
    }

    // Prune stale git metadata regardless of whether the directory is still present
    let prune_output = tokio::process::Command::new("git")
        .args(["worktree", "prune"])
        .current_dir(repo_root)
        .output()
        .await?;

    if !prune_output.status.success() {
        // Non-fatal — log the warning but don't fail the overall removal
        let stderr = String::from_utf8_lossy(&prune_output.stderr);
        tracing::warn!("git worktree prune failed after removal: {}", stderr.trim());
    }

    // Safety check: directory should be gone after --force remove
    if worktree_path.exists() {
        tracing::warn!(
            path = %worktree_path.display(),
            "remove_worktree: directory still exists after git worktree remove --force"
        );
    }

    Ok(())
}

/// Gitignored local files the ump repo needs at build time but git does not
/// track, so a freshly-created worktree lacks them (build failure). Each entry
/// is a path RELATIVE to the repo root — identical across every teammate's
/// clone, so seeding works regardless of where main/new worktrees live on disk.
/// Edit this list to change what every dashboard user copies into new worktrees.
const WORKTREE_SEED_FILES: &[&str] = &[
    ".env",
    "android/keystore/release.keystore",
    "android/keystore/debug.keystore",
];

/// Copies each `WORKTREE_SEED_FILES` entry from `repo_root` into `worktree_path`.
///
/// Best-effort and non-fatal: the worktree is already created, so a missing
/// source or copy error is logged and skipped rather than failing the add. A
/// destination that already exists is left untouched (never clobbers).
fn seed_worktree_files(repo_root: &Path, worktree_path: &Path) {
    for rel in WORKTREE_SEED_FILES {
        let src = repo_root.join(rel);
        let dest = worktree_path.join(rel);
        if !src.exists() || dest.exists() {
            continue;
        }
        if let Some(parent) = dest.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!("seed_worktree_files: mkdir {} failed: {e}", parent.display());
                continue;
            }
        }
        match std::fs::copy(&src, &dest) {
            Ok(_) => tracing::info!("seed_worktree_files: copied {rel} into new worktree"),
            Err(e) => tracing::warn!("seed_worktree_files: copy {rel} failed: {e}"),
        }
    }
}

/// Creates a new worktree as a sibling directory of repo_root.
///
/// Computes the worktree path as `repo_root.parent().unwrap().join(branch_name)`.
/// Runs `git worktree add -b <branch_name> <path>` to create a new branch, or
/// retries with `git worktree add <path> <branch_name>` if the branch already exists.
/// On success, seeds gitignored local files (see `seed_worktree_files`) so the
/// new worktree builds. Returns the created worktree path on success.
pub async fn add_worktree(repo_root: &Path, branch_name: &str) -> anyhow::Result<std::path::PathBuf> {
    let parent = repo_root.parent().ok_or_else(|| anyhow::anyhow!("repo_root has no parent directory"))?;
    let worktree_path = parent.join(branch_name);

    if worktree_path.exists() {
        anyhow::bail!("Directory already exists: {}", worktree_path.display());
    }

    let path_str = worktree_path.to_string_lossy().to_string();

    // First try: create with new branch (-b flag)
    let output = tokio::process::Command::new("git")
        .args(["worktree", "add", &path_str, "-b", branch_name])
        .current_dir(repo_root)
        .output()
        .await?;

    if output.status.success() {
        seed_worktree_files(repo_root, &worktree_path);
        return Ok(worktree_path);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    // If branch already exists, retry without -b to checkout existing branch
    if stderr.contains("already exists") || stderr.contains("branch") {
        let retry_output = tokio::process::Command::new("git")
            .args(["worktree", "add", &path_str, branch_name])
            .current_dir(repo_root)
            .output()
            .await?;

        if retry_output.status.success() {
            seed_worktree_files(repo_root, &worktree_path);
            return Ok(worktree_path);
        }

        let retry_stderr = String::from_utf8_lossy(&retry_output.stderr);
        anyhow::bail!("git worktree add failed: {}", retry_stderr.trim());
    }

    anyhow::bail!("git worktree add -b failed: {}", stderr.trim());
}

/// Lists remote branch names by running `git branch -r` in repo_root.
/// Returns branch names with "origin/" prefix stripped, excluding HEAD pointers.
/// Results are sorted alphabetically.
pub async fn list_remote_branches(repo_root: &Path) -> anyhow::Result<Vec<String>> {
    let output = tokio::process::Command::new("git")
        .args(["branch", "-r", "--no-color"])
        .current_dir(repo_root)
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git branch -r failed: {stderr}");
    }
    let text = String::from_utf8(output.stdout)?;
    let mut branches: Vec<String> = text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.contains("->"))  // skip HEAD -> origin/main
        .map(|l| l.strip_prefix("origin/").unwrap_or(l).to_string())
        .collect();
    branches.sort();
    branches.dedup();
    Ok(branches)
}

/// Creates a worktree with a new branch based on a given base branch.
/// Runs `git worktree add -b <new_branch> <path> origin/<base_branch>`.
/// Returns the created worktree path on success.
pub async fn add_worktree_new_branch(
    repo_root: &Path,
    new_branch: &str,
    base_branch: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let parent = repo_root.parent()
        .ok_or_else(|| anyhow::anyhow!("repo_root has no parent directory"))?;
    let worktree_path = parent.join(new_branch);
    if worktree_path.exists() {
        anyhow::bail!("Directory already exists: {}", worktree_path.display());
    }
    let path_str = worktree_path.to_string_lossy().to_string();
    let output = tokio::process::Command::new("git")
        .args(["worktree", "add", "-b", new_branch, &path_str, &format!("origin/{base_branch}")])
        .current_dir(repo_root)
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add -b failed: {}", stderr.trim());
    }
    seed_worktree_files(repo_root, &worktree_path);
    Ok(worktree_path)
}

/// Runs `git worktree list --porcelain` in `repo_root` and parses the output.
pub async fn list_worktrees(repo_root: &Path) -> anyhow::Result<Vec<Worktree>> {
    let output = tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree list failed: {stderr}");
    }

    let text = String::from_utf8(output.stdout)?;
    parse_worktree_porcelain(&text)
}

/// F-104 adapter: wraps the existing async worktree free fns behind the
/// `WorktreePort` trait. Consumers after Plan 13-08 receive this as
/// `Arc<dyn WorktreePort>`; until then the free fns above remain the primary
/// call path.
pub struct GitWorktreeAdapter;

#[async_trait::async_trait]
impl crate::domain::ports::worktree_port::WorktreePort for GitWorktreeAdapter {
    async fn list(
        &self,
        repo_root: &std::path::Path,
    ) -> anyhow::Result<Vec<crate::domain::worktree::Worktree>> {
        list_worktrees(repo_root).await
    }

    async fn remove(
        &self,
        repo_root: &std::path::Path,
        worktree_path: &std::path::Path,
    ) -> anyhow::Result<()> {
        remove_worktree(repo_root, worktree_path).await
    }

    async fn add(
        &self,
        repo_root: &std::path::Path,
        branch_name: &str,
    ) -> anyhow::Result<std::path::PathBuf> {
        add_worktree(repo_root, branch_name).await
    }

    async fn add_new_branch(
        &self,
        repo_root: &std::path::Path,
        new_branch: &str,
        base_branch: &str,
    ) -> anyhow::Result<std::path::PathBuf> {
        add_worktree_new_branch(repo_root, new_branch, base_branch).await
    }

    async fn list_remote_branches(
        &self,
        repo_root: &std::path::Path,
    ) -> anyhow::Result<Vec<String>> {
        list_remote_branches(repo_root).await
    }
}
