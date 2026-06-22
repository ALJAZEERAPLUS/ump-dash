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

/// Copies each `seed_files` entry from `repo_root` into `worktree_path`. Entries
/// are paths RELATIVE to the repo root (identical across every teammate's clone),
/// so seeding works regardless of where main/new worktrees live on disk. The
/// list is supplied by the caller from `DashConfig::seed_files`.
///
/// Best-effort and non-fatal: the worktree is already created, so a missing
/// source or copy error is logged and skipped rather than failing the add. A
/// destination that already exists is left untouched (never clobbers).
fn seed_worktree_files(repo_root: &Path, worktree_path: &Path, seed_files: &[String]) {
    for rel in seed_files {
        let src = repo_root.join(rel);
        let dest = worktree_path.join(rel);
        if !src.exists() || dest.exists() {
            continue;
        }
        if let Some(parent) = dest.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            tracing::warn!(
                "seed_worktree_files: mkdir {} failed: {e}",
                parent.display()
            );
            continue;
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
/// Returns the created worktree path on success. Seeding of gitignored local
/// files happens at the `WorktreePort` boundary (`GitWorktreeAdapter`), not here.
pub async fn add_worktree(
    repo_root: &Path,
    branch_name: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let parent = repo_root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("repo_root has no parent directory"))?;
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

    // If the first attempt fails because the branch already exists, retry
    // without -b to check it out; any other failure is fatal. Both the first-try
    // and retry success paths fall through to the single seed-and-return below.
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !(stderr.contains("already exists") || stderr.contains("branch")) {
            anyhow::bail!("git worktree add -b failed: {}", stderr.trim());
        }

        let retry_output = tokio::process::Command::new("git")
            .args(["worktree", "add", &path_str, branch_name])
            .current_dir(repo_root)
            .output()
            .await?;

        if !retry_output.status.success() {
            let retry_stderr = String::from_utf8_lossy(&retry_output.stderr);
            anyhow::bail!("git worktree add failed: {}", retry_stderr.trim());
        }
    }

    Ok(worktree_path)
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
    let mut branches: Vec<String> = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.contains("->")) // skip HEAD -> origin/main
        .map(|l| l.strip_prefix("origin/").unwrap_or(l).to_string())
        .collect();
    branches.sort();
    branches.dedup();
    Ok(branches)
}

/// Creates a worktree with a new branch based on a given base branch.
/// Runs `git worktree add -b <new_branch> <path> origin/<base_branch>`.
/// Returns the created worktree path on success. Seeding of gitignored local
/// files happens at the `WorktreePort` boundary (`GitWorktreeAdapter`), not here.
pub async fn add_worktree_new_branch(
    repo_root: &Path,
    new_branch: &str,
    base_branch: &str,
) -> anyhow::Result<std::path::PathBuf> {
    let parent = repo_root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("repo_root has no parent directory"))?;
    let worktree_path = parent.join(new_branch);
    if worktree_path.exists() {
        anyhow::bail!("Directory already exists: {}", worktree_path.display());
    }
    let path_str = worktree_path.to_string_lossy().to_string();
    let output = tokio::process::Command::new("git")
        .args([
            "worktree",
            "add",
            "-b",
            new_branch,
            &path_str,
            &format!("origin/{base_branch}"),
        ])
        .current_dir(repo_root)
        .output()
        .await?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add -b failed: {}", stderr.trim());
    }
    Ok(worktree_path)
}

/// Creates a review worktree for a GitHub PR.
///
/// This intentionally updates the local branch named exactly like the PR
/// `headRefName`. The app layer checks current worktree state first and stops
/// if that branch is already checked out anywhere.
pub async fn add_review_worktree(
    repo_root: &Path,
    pr_number: u64,
    branch_name: &str,
    head_oid: &str,
    worktree_name: &str,
) -> anyhow::Result<std::path::PathBuf> {
    if branch_name.trim().is_empty() {
        anyhow::bail!("PR branch name is empty");
    }
    if head_oid.trim().is_empty() {
        anyhow::bail!("PR head OID is empty");
    }
    if worktree_name.trim().is_empty()
        || worktree_name.contains('/')
        || worktree_name.contains('\\')
        || worktree_name == "."
        || worktree_name == ".."
    {
        anyhow::bail!("Invalid worktree name: {worktree_name}");
    }

    let parent = repo_root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("repo_root has no parent directory"))?;
    let worktree_path = parent.join(worktree_name);
    if worktree_path.exists() {
        anyhow::bail!("Directory already exists: {}", worktree_path.display());
    }

    let refspec = format!("+refs/pull/{pr_number}/head:refs/heads/{branch_name}");
    let fetch = tokio::process::Command::new("git")
        .args(["fetch", "origin", &refspec])
        .current_dir(repo_root)
        .output()
        .await?;
    if !fetch.status.success() {
        let stderr = String::from_utf8_lossy(&fetch.stderr);
        anyhow::bail!("git fetch PR head failed: {}", stderr.trim());
    }

    let branch_ref = format!("refs/heads/{branch_name}");
    let rev_parse = tokio::process::Command::new("git")
        .args(["rev-parse", &branch_ref])
        .current_dir(repo_root)
        .output()
        .await?;
    if !rev_parse.status.success() {
        let stderr = String::from_utf8_lossy(&rev_parse.stderr);
        anyhow::bail!("git rev-parse failed: {}", stderr.trim());
    }
    let actual_oid = String::from_utf8_lossy(&rev_parse.stdout)
        .trim()
        .to_string();
    if actual_oid != head_oid {
        anyhow::bail!("Fetched PR branch {branch_name} at {actual_oid}, expected {head_oid}");
    }

    let path_str = worktree_path.to_string_lossy().to_string();
    let add = tokio::process::Command::new("git")
        .args(["worktree", "add", &path_str, branch_name])
        .current_dir(repo_root)
        .output()
        .await?;
    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr.trim());
    }

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

/// F-104 adapter: wraps the async worktree free fns behind the `WorktreePort`
/// trait. Consumers receive this as `Arc<dyn WorktreePort>`. Seeding of
/// gitignored local files is owned here at the port boundary, so every worktree
/// created through the port is seeded regardless of which `add*` path runs.
pub struct GitWorktreeAdapter {
    /// Files copied into each newly-created worktree, resolved from
    /// `DashConfig::seed_files` at the composition root.
    seed_files: Vec<String>,
}

impl GitWorktreeAdapter {
    /// Builds the adapter with the seed-file list (see `DashConfig::seed_files`).
    pub fn new(seed_files: Vec<String>) -> Self {
        Self { seed_files }
    }
}

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
        let worktree_path = add_worktree(repo_root, branch_name).await?;
        seed_worktree_files(repo_root, &worktree_path, &self.seed_files);
        Ok(worktree_path)
    }

    async fn add_new_branch(
        &self,
        repo_root: &std::path::Path,
        new_branch: &str,
        base_branch: &str,
    ) -> anyhow::Result<std::path::PathBuf> {
        let worktree_path = add_worktree_new_branch(repo_root, new_branch, base_branch).await?;
        seed_worktree_files(repo_root, &worktree_path, &self.seed_files);
        Ok(worktree_path)
    }

    async fn add_review_worktree(
        &self,
        repo_root: &std::path::Path,
        pr_number: u64,
        branch_name: &str,
        head_oid: &str,
        worktree_name: &str,
    ) -> anyhow::Result<std::path::PathBuf> {
        let worktree_path =
            add_review_worktree(repo_root, pr_number, branch_name, head_oid, worktree_name).await?;
        seed_worktree_files(repo_root, &worktree_path, &self.seed_files);
        Ok(worktree_path)
    }

    async fn list_remote_branches(
        &self,
        repo_root: &std::path::Path,
    ) -> anyhow::Result<Vec<String>> {
        list_remote_branches(repo_root).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Unique temp dir per call; removed when the returned guard drops.
    struct TempDir(PathBuf);
    impl TempDir {
        fn new(tag: &str) -> Self {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let p =
                std::env::temp_dir().join(format!("ump-seed-{}-{tag}-{n}", std::process::id(),));
            fs::create_dir_all(&p).unwrap();
            TempDir(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Seeds a flat file present in the repo root but absent in the worktree.
    #[test]
    fn seeds_existing_file_into_worktree() {
        let repo = TempDir::new("repo");
        let wt = TempDir::new("wt");
        fs::write(repo.path().join(".env"), b"SECRET=1").unwrap();

        seed_worktree_files(repo.path(), wt.path(), &[".env".to_string()]);

        assert_eq!(
            fs::read(wt.path().join(".env")).unwrap(),
            b"SECRET=1",
            ".env should be copied into the worktree"
        );
    }

    /// Nested seed paths get their parent directories created in the worktree.
    #[test]
    fn creates_parent_dirs_for_nested_seed() {
        let repo = TempDir::new("repo");
        let wt = TempDir::new("wt");
        let nested = repo.path().join("android/keystore/release.keystore");
        fs::create_dir_all(nested.parent().unwrap()).unwrap();
        fs::write(&nested, b"keystore-bytes").unwrap();

        seed_worktree_files(
            repo.path(),
            wt.path(),
            &["android/keystore/release.keystore".to_string()],
        );

        assert_eq!(
            fs::read(wt.path().join("android/keystore/release.keystore")).unwrap(),
            b"keystore-bytes",
            "nested seed file should be copied, creating parent dirs"
        );
    }

    /// An existing destination is never clobbered.
    #[test]
    fn does_not_clobber_existing_dest() {
        let repo = TempDir::new("repo");
        let wt = TempDir::new("wt");
        fs::write(repo.path().join(".env"), b"FROM_REPO").unwrap();
        fs::write(wt.path().join(".env"), b"ALREADY_THERE").unwrap();

        seed_worktree_files(repo.path(), wt.path(), &[".env".to_string()]);

        assert_eq!(
            fs::read(wt.path().join(".env")).unwrap(),
            b"ALREADY_THERE",
            "pre-existing worktree file must be left untouched"
        );
    }

    /// A missing source file is skipped without creating anything in the worktree.
    #[test]
    fn skips_missing_source() {
        let repo = TempDir::new("repo");
        let wt = TempDir::new("wt");

        seed_worktree_files(repo.path(), wt.path(), &[".env".to_string()]);

        assert!(
            !wt.path().join(".env").exists(),
            "no dest should be created when source is absent"
        );
    }
}
