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

/// Builds the `.mcp.json` content that points Claude Code at the embedded MCP
/// server (`http://127.0.0.1:<port>/mcp`).
///
/// When `existing` parses as a JSON object, only the `mcpServers."ump-dash"`
/// entry is set/refreshed — any other servers the user configured (and any
/// other top-level keys) are preserved. Otherwise a fresh document is produced.
/// Pure (no I/O) so it can be unit-tested directly.
fn agent_mcp_json(existing: Option<&str>, port: u16) -> String {
    use serde_json::{Value, json};

    let mut doc = existing
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));

    let obj = doc.as_object_mut().expect("doc is a JSON object");
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    if !servers.is_object() {
        *servers = json!({});
    }
    servers
        .as_object_mut()
        .expect("mcpServers is a JSON object")
        .insert(
            "ump-dash".to_string(),
            json!({
                "type": "http",
                "url": format!("http://127.0.0.1:{port}/mcp"),
            }),
        );

    let mut out = serde_json::to_string_pretty(&doc).unwrap_or_default();
    out.push('\n');
    out
}

/// Project run skill seeded into each worktree. The built-in `run` skill defers
/// to a project skill like this when present, so an agent in the worktree drives
/// the dashboard's `ump-dash` MCP tools to run/build instead of hand-rolling
/// metro/yarn/native build commands.
const RUN_SKILL_MD: &str = r#"---
name: run-app
description: Use when asked to run, build, launch, or start this app on a simulator, emulator, or device. Drives the dashboard's `ump-dash` MCP tools instead of hand-running metro/yarn/build.
---

# Run this app via the `ump-dash` MCP tools

This worktree is managed by the UMP dashboard, which exposes an `ump-dash` MCP
server. Use its tools to run or build the app — do **not** run `metro`, `yarn`,
`pod install`, or native build commands yourself. The dashboard syncs
dependencies, starts Metro, and uses the prebuilt cache automatically.

Every tool takes `worktree`: the absolute path of this worktree (your current
working directory).

## To run the app

1. `list_devices` with `platform` `"ios"` or `"android"`, then pick a target's `id`.
2. `run_ios` (or `run_android`) with `worktree` = this directory and
   `device_id` = the `id` from step 1. That's it — do not call `start_metro`,
   `sync_deps`, or `get_worktree_status` first.
3. A cache hit launches instantly with **no task to poll** — its result shows in
   `get_logs`. For a cold build, poll `get_task_status` until it finishes.

## Notes

- To build without launching, use `build`. To force a dependency sync, use
  `sync_deps` — both are usually unnecessary.
- To create or delete dashboard-managed worktrees when explicitly asked, use
  `create_worktree` or `delete_worktree`. Both require `confirm=true`; deletion
  also requires an explicit `target_worktree` absolute path and refuses the main
  repo root.
- Destructive tools (`shell`, `clean`, `reset_hard`, `delete_worktree`) require
  `confirm=true`.
"#;

/// Builds the `.codex/config.toml` content that registers the embedded MCP
/// server with Codex. Merge-safe like `agent_mcp_json`: an existing config's
/// other `[mcp_servers]` entries and unrelated sections are preserved; only the
/// `ump-dash` server block is set/refreshed. Pure (no I/O).
fn agent_codex_config(existing: Option<&str>, port: u16) -> String {
    use toml::{Table, Value};

    let mut doc: Table = existing
        .and_then(|s| toml::from_str::<Table>(s).ok())
        .unwrap_or_default();

    let servers = doc
        .entry("mcp_servers")
        .or_insert_with(|| Value::Table(Table::new()));
    if !servers.is_table() {
        *servers = Value::Table(Table::new());
    }

    let mut entry = Table::new();
    entry.insert(
        "url".into(),
        Value::String(format!("http://127.0.0.1:{port}/mcp")),
    );
    entry.insert("enabled".into(), Value::Boolean(true));
    entry.insert("required".into(), Value::Boolean(false));
    entry.insert("startup_timeout_sec".into(), Value::Integer(3));
    entry.insert("tool_timeout_sec".into(), Value::Integer(900));

    servers
        .as_table_mut()
        .expect("mcp_servers is a table")
        .insert("ump-dash".into(), Value::Table(entry));

    toml::to_string_pretty(&doc).unwrap_or_default()
}

/// Codex reads `.agents/skills/`, so the run-app skill is seeded there too with
/// a richer, RN-specific body. Mirrors `RUN_SKILL_MD` in intent: drive the
/// `ump-dash` MCP tools rather than hand-running Metro/yarn/native builds.
const CODEX_RUN_SKILL_MD: &str = r#"---
name: run-app
description: Use when asked to run, launch, build, install, or start this React Native app on iOS or Android simulators/devices, including iPhone device requests such as Dafone. Always use the `ump-dash` MCP tools instead of running Metro, yarn, pod install, xcodebuild, react-native run-ios, or native build commands manually.
---

# Run this app via `ump-dash`

This worktree is managed by the UMP dashboard. Use the `ump-dash` MCP tools for
run/build tasks; do not hand-run Metro, dependency installs, CocoaPods, Xcode,
Gradle, or React Native CLI commands.

Every `ump-dash` tool call takes `worktree`, the absolute path of this worktree.

## Workflow

1. Call `list_devices` with `platform: "ios"` or `platform: "android"`.
2. Pick the target device's `id`. Use the `id`, not the display name.
3. Call `run_ios` or `run_android` with `worktree` and `device_id`.
4. If the result includes a `task_id`, poll `get_task_status` until it finishes.
   A cache hit can launch instantly without a task; use `get_logs` to confirm.

## If `ump-dash` Is Missing

If the `ump-dash` MCP tools are not available in the Codex tool surface, stop and
report that the dashboard MCP is not registered for this session. Do not fall
back to shell build commands. The expected Codex project config is
`.codex/config.toml`, and a new Codex session may be required after config
changes.

## Notes

- `build`, `sync_deps`, and `start_metro` are optional diagnostics and are
  usually unnecessary before `run_ios` or `run_android`.
- When explicitly asked to manage worktrees, use `create_worktree` or
  `delete_worktree`; both require `confirm=true`, and deletion requires an
  explicit `target_worktree` absolute path.
- Destructive tools such as `shell`, `clean`, `reset_hard`, and
  `delete_worktree` require explicit confirmation.
"#;

/// Builds the Codex/OpenAI skill manifest (`agents/openai.yaml`) for the seeded
/// `run-app` skill, pointing its MCP dependency at the embedded server.
fn agent_openai_yaml(port: u16) -> String {
    format!(
        r#"interface:
  display_name: "Run UMP App"
  short_description: "Launch UMP through the dashboard MCP"
  default_prompt: "Use $run-app to run this branch on my iPhone."

dependencies:
  tools:
    - type: "mcp"
      value: "ump-dash"
      description: "UMP dashboard MCP server for worktree run and build tasks"
      transport: "streamable_http"
      url: "http://127.0.0.1:{port}/mcp"

policy:
  allow_implicit_invocation: true
"#
    )
}

/// Writes `content` to `path` only when it does not already exist, creating
/// parent dirs first. Best-effort and non-clobber (respects user edits),
/// mirroring `seed_worktree_files`. `label` is for logging only.
fn write_if_absent(path: &Path, content: &str, label: &str) {
    if path.exists() {
        return;
    }
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("provision: mkdir {} failed: {e}", parent.display());
        return;
    }
    match std::fs::write(path, content) {
        Ok(_) => tracing::info!("provision: wrote {label}"),
        Err(e) => tracing::warn!("provision: write {label} failed: {e}"),
    }
}

/// Reads any existing file at `path`, runs `merge` to compute fresh content, and
/// (re)writes it — so a merge-safe config keeps its `ump-dash` endpoint current
/// while preserving the user's other entries. Best-effort, non-fatal.
fn write_merged(path: &Path, label: &str, merge: impl FnOnce(Option<&str>) -> String) {
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("provision: mkdir {} failed: {e}", parent.display());
        return;
    }
    let existing = std::fs::read_to_string(path).ok();
    let content = merge(existing.as_deref());
    match std::fs::write(path, content) {
        Ok(_) => tracing::info!("provision: wrote {label}"),
        Err(e) => tracing::warn!("provision: write {label} failed: {e}"),
    }
}

/// Best-effort provisioning of agent files into a freshly-created worktree
/// (mirrors `seed_worktree_files`' log-on-error, non-fatal style). Seeds both
/// Claude Code and Codex so an agent in either tool discovers the dashboard:
/// - `.mcp.json` (Claude Code) and `.codex/config.toml` (Codex): MCP server
///   registration, merge-safe so other servers/sections are preserved and the
///   `ump-dash` endpoint stays current.
/// - `.claude/skills/run-app/SKILL.md`, `.agents/skills/run-app/SKILL.md`, and
///   its `agents/openai.yaml` manifest (Codex): the run-app skill, written only
///   when absent (never clobbers a user's edits).
fn provision_worktree_agent_files(worktree_path: &Path, port: u16) {
    // MCP server registration — merge-safe for both tools.
    write_merged(&worktree_path.join(".mcp.json"), ".mcp.json", |existing| {
        agent_mcp_json(existing, port)
    });
    write_merged(
        &worktree_path.join(".codex/config.toml"),
        ".codex/config.toml",
        |existing| agent_codex_config(existing, port),
    );

    // run-app skill — non-clobber, seeded for Claude Code and Codex.
    write_if_absent(
        &worktree_path.join(".claude/skills/run-app/SKILL.md"),
        RUN_SKILL_MD,
        ".claude run-app skill",
    );
    write_if_absent(
        &worktree_path.join(".agents/skills/run-app/SKILL.md"),
        CODEX_RUN_SKILL_MD,
        ".agents run-app skill",
    );
    write_if_absent(
        &worktree_path.join(".agents/skills/run-app/agents/openai.yaml"),
        &agent_openai_yaml(port),
        ".agents run-app openai.yaml",
    );
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

async fn resolve_worktree_base_ref(repo_root: &Path, base_branch: &str) -> anyhow::Result<String> {
    let candidates = [
        format!("refs/remotes/origin/{base_branch}"),
        format!("refs/heads/{base_branch}"),
    ];

    for candidate in candidates {
        let output = tokio::process::Command::new("git")
            .args(["show-ref", "--verify", "--quiet", &candidate])
            .current_dir(repo_root)
            .output()
            .await?;
        if output.status.success() {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "Base branch not found: {base_branch} (checked origin/{base_branch} and local {base_branch})"
    );
}

/// Creates a worktree with a new branch based on a given base branch.
/// Prefers `origin/<base_branch>` when present; falls back to a local branch.
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
    let base_ref = resolve_worktree_base_ref(repo_root, base_branch).await?;
    let path_str = worktree_path.to_string_lossy().to_string();
    let output = tokio::process::Command::new("git")
        .args(["worktree", "add", "-b", new_branch, &path_str, &base_ref])
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
    /// `Some(port)` when the embedded MCP server is enabled: newly-created
    /// worktrees are then provisioned with `.mcp.json` + the run-app skill so
    /// agents inside them discover the dashboard. `None` disables provisioning.
    mcp_port: Option<u16>,
}

impl GitWorktreeAdapter {
    /// Builds the adapter with the seed-file list (see `DashConfig::seed_files`)
    /// and the MCP port (`Some` when the embedded MCP server is enabled).
    pub fn new(seed_files: Vec<String>, mcp_port: Option<u16>) -> Self {
        Self {
            seed_files,
            mcp_port,
        }
    }

    /// Provisions MCP-agent files into a freshly-created worktree when the
    /// embedded MCP server is enabled. No-op when `mcp_port` is `None`.
    fn provision_agent_files(&self, worktree_path: &Path) {
        if let Some(port) = self.mcp_port {
            provision_worktree_agent_files(worktree_path, port);
        }
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
        self.provision_agent_files(&worktree_path);
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
        self.provision_agent_files(&worktree_path);
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
        self.provision_agent_files(&worktree_path);
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

    const TEAM_MCP_JSON: &str = r#"{
  "mcpServers": {
    "sauce-api-mcp-core": {
      "command": "./scripts/mcp-with-env.sh",
      "args": ["uvx", "--from", "sauce-api-mcp==1.2.2", "sauce-api-mcp"]
    },
    "sauce-api-mcp-rdc": {
      "command": "./scripts/mcp-with-env.sh",
      "args": ["uvx", "--from", "sauce-api-mcp==1.2.2", "sauce-api-mcp-rdc"]
    },
    "atlassian": {
      "type": "http",
      "url": "https://mcp.atlassian.com/v1/mcp"
    },
    "bugsnag": {
      "command": "./scripts/mcp-with-env.sh",
      "args": ["npx", "-y", "@smartbear/mcp@0.25.1"]
    },
    "figma": {
      "type": "http",
      "url": "https://mcp.figma.com/mcp"
    },
    "amplitude": {
      "type": "http",
      "url": "https://mcp.amplitude.com/mcp"
    }
  }
}"#;

    const TEAM_CODEX_CONFIG: &str = r#"# Project-scoped Codex MCP config (read for trusted projects; shared between
# the Codex CLI and IDE extension). Mirrors .mcp.json, the Claude Code
# equivalent — keep the two in sync. See docs/mcp-setup.md.

# Sauce Labs (one PyPI package, two entry points). Credentials come from the
# gitignored .env via the wrapper. First launch downloads the package via uvx,
# which can exceed the default startup timeout.
[mcp_servers.sauce-api-mcp-core]
command = "./scripts/mcp-with-env.sh"
args = ["uvx", "--from", "sauce-api-mcp==1.2.2", "sauce-api-mcp"]
startup_timeout_sec = 60

[mcp_servers.sauce-api-mcp-rdc]
command = "./scripts/mcp-with-env.sh"
args = ["uvx", "--from", "sauce-api-mcp==1.2.2", "sauce-api-mcp-rdc"]
startup_timeout_sec = 60

# Per-user OAuth: `codex mcp login atlassian`
[mcp_servers.atlassian]
url = "https://mcp.atlassian.com/v1/mcp"

[mcp_servers.bugsnag]
command = "./scripts/mcp-with-env.sh"
args = ["npx", "-y", "@smartbear/mcp@0.25.1"]

# Disabled until design seats are provisioned — mirrors disabledMcpjsonServers
# in .claude/settings.json. Once enabled: `codex mcp login figma`.
[mcp_servers.figma]
url = "https://mcp.figma.com/mcp"
enabled = false

# Per-user OAuth: `codex mcp login amplitude`
[mcp_servers.amplitude]
url = "https://mcp.amplitude.com/mcp"
"#;

    const TEAM_SERVER_NAMES: [&str; 6] = [
        "sauce-api-mcp-core",
        "sauce-api-mcp-rdc",
        "atlassian",
        "bugsnag",
        "figma",
        "amplitude",
    ];

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

    fn run_git(repo: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed\nstdout:\n{}\nstderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[tokio::test]
    async fn add_new_branch_uses_local_base_branch_when_no_origin_ref() {
        let root = TempDir::new("local-base");
        let repo = root.path().join("repo");
        fs::create_dir_all(&repo).unwrap();

        run_git(&repo, &["init"]);
        run_git(&repo, &["config", "user.email", "test@example.com"]);
        run_git(&repo, &["config", "user.name", "Test User"]);
        fs::write(repo.join("README.md"), b"root\n").unwrap();
        run_git(&repo, &["add", "README.md"]);
        run_git(&repo, &["commit", "-m", "initial"]);
        run_git(&repo, &["switch", "-c", "UMP-6831"]);
        fs::write(repo.join("feature.txt"), b"feature\n").unwrap();
        run_git(&repo, &["add", "feature.txt"]);
        run_git(&repo, &["commit", "-m", "feature"]);

        let worktree_path = add_worktree_new_branch(&repo, "branch-test", "UMP-6831")
            .await
            .unwrap();

        assert_eq!(worktree_path, root.path().join("branch-test"));
        assert!(
            worktree_path.join("feature.txt").exists(),
            "new worktree should be based on the local-only branch"
        );
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

    /// A fresh `.mcp.json` carries the loopback URL with the configured port and
    /// the `ump-dash` HTTP server entry.
    #[test]
    fn agent_mcp_json_fresh_has_url_and_port() {
        let content = agent_mcp_json(None, 8790);
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        let server = &v["mcpServers"]["ump-dash"];
        assert_eq!(server["type"], "http");
        assert_eq!(server["url"], "http://127.0.0.1:8790/mcp");
    }

    /// Merging into an existing doc preserves other servers and other top-level
    /// keys while refreshing only the `ump-dash` entry.
    #[test]
    fn agent_mcp_json_merges_preserving_other_servers() {
        let existing = r#"{
            "mcpServers": {
                "other": { "type": "http", "url": "http://example/other" }
            },
            "someOtherKey": 42
        }"#;
        let content = agent_mcp_json(Some(existing), 9000);
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            v["mcpServers"]["other"]["url"], "http://example/other",
            "pre-existing server must be preserved"
        );
        assert_eq!(v["someOtherKey"], 42, "other top-level keys must survive");
        assert_eq!(
            v["mcpServers"]["ump-dash"]["url"],
            "http://127.0.0.1:9000/mcp"
        );
    }

    /// The tracked team config introduced by UMP PR #3992 keeps every shared
    /// server unchanged while gaining the local dashboard endpoint.
    #[test]
    fn agent_mcp_json_merges_pr_3992_team_servers() {
        let before: serde_json::Value = serde_json::from_str(TEAM_MCP_JSON).unwrap();
        let content = agent_mcp_json(Some(TEAM_MCP_JSON), 8790);
        let after: serde_json::Value = serde_json::from_str(&content).unwrap();

        for name in TEAM_SERVER_NAMES {
            assert_eq!(
                after["mcpServers"][name], before["mcpServers"][name],
                "team server {name} must remain unchanged"
            );
        }
        assert_eq!(after["mcpServers"].as_object().unwrap().len(), 7);
        assert_eq!(after["mcpServers"]["ump-dash"]["type"], "http");
        assert_eq!(
            after["mcpServers"]["ump-dash"]["url"],
            "http://127.0.0.1:8790/mcp"
        );
    }

    /// Refreshing an existing endpoint changes only `ump-dash`; the PR #3992
    /// siblings and unrelated top-level values survive.
    #[test]
    fn agent_mcp_json_refreshes_ump_dash_only() {
        let mut before: serde_json::Value = serde_json::from_str(TEAM_MCP_JSON).unwrap();
        before["mcpServers"]["ump-dash"] = serde_json::json!({
            "type": "http",
            "url": "http://127.0.0.1:7000/mcp",
            "legacy": true
        });
        before["someOtherKey"] = serde_json::json!(42);

        let input = serde_json::to_string_pretty(&before).unwrap();
        let content = agent_mcp_json(Some(&input), 8790);
        let after: serde_json::Value = serde_json::from_str(&content).unwrap();

        for name in TEAM_SERVER_NAMES {
            assert_eq!(
                after["mcpServers"][name], before["mcpServers"][name],
                "team server {name} must remain unchanged"
            );
        }
        assert_eq!(after["someOtherKey"], 42);
        assert_eq!(
            after["mcpServers"]["ump-dash"],
            serde_json::json!({
                "type": "http",
                "url": "http://127.0.0.1:8790/mcp"
            })
        );
    }

    /// Non-JSON existing content is replaced by a fresh, valid document.
    #[test]
    fn agent_mcp_json_replaces_invalid_existing() {
        let content = agent_mcp_json(Some("not json {"), 8790);
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(
            v["mcpServers"]["ump-dash"]["url"],
            "http://127.0.0.1:8790/mcp"
        );
    }

    /// Provisioning writes both the `.mcp.json` and the run-app skill.
    #[test]
    fn provision_writes_mcp_json_and_skill() {
        let wt = TempDir::new("wt");

        provision_worktree_agent_files(wt.path(), 8790);

        let mcp = fs::read_to_string(wt.path().join(".mcp.json")).unwrap();
        assert!(mcp.contains("http://127.0.0.1:8790/mcp"));

        let skill = fs::read_to_string(wt.path().join(".claude/skills/run-app/SKILL.md")).unwrap();
        assert!(skill.contains("name: run-app"), "skill frontmatter present");
    }

    /// An existing run-app skill is never clobbered (respects user edits).
    #[test]
    fn provision_does_not_clobber_existing_skill() {
        let wt = TempDir::new("wt");
        let skill_path = wt.path().join(".claude/skills/run-app/SKILL.md");
        fs::create_dir_all(skill_path.parent().unwrap()).unwrap();
        fs::write(&skill_path, b"USER EDITED").unwrap();

        provision_worktree_agent_files(wt.path(), 8790);

        assert_eq!(
            fs::read(&skill_path).unwrap(),
            b"USER EDITED",
            "an existing skill file must be left untouched"
        );
    }

    /// With the MCP server disabled (`mcp_port` = None), no agent files are
    /// written into the worktree.
    #[test]
    fn provision_skipped_when_mcp_disabled() {
        let wt = TempDir::new("wt");
        let adapter = GitWorktreeAdapter::new(vec![], None);

        adapter.provision_agent_files(wt.path());

        assert!(
            !wt.path().join(".mcp.json").exists(),
            ".mcp.json must not be written when MCP is disabled"
        );
        assert!(
            !wt.path().join(".claude/skills/run-app/SKILL.md").exists(),
            "run-app skill must not be written when MCP is disabled"
        );
        assert!(
            !wt.path().join(".codex/config.toml").exists(),
            "codex config must not be written when MCP is disabled"
        );
        assert!(
            !wt.path().join(".agents").exists(),
            "codex .agents skill must not be written when MCP is disabled"
        );
    }

    /// A fresh `.codex/config.toml` registers the `ump-dash` server with the
    /// configured port and the expected Codex fields.
    #[test]
    fn agent_codex_config_fresh_has_url_and_port() {
        let content = agent_codex_config(None, 8790);
        let doc: toml::Table = toml::from_str(&content).unwrap();
        let server = &doc["mcp_servers"]["ump-dash"];
        assert_eq!(server["url"].as_str(), Some("http://127.0.0.1:8790/mcp"));
        assert_eq!(server["enabled"].as_bool(), Some(true));
        assert_eq!(server["required"].as_bool(), Some(false));
        assert_eq!(server["tool_timeout_sec"].as_integer(), Some(900));
    }

    /// Merging into an existing Codex config preserves other servers and other
    /// sections while refreshing only the `ump-dash` entry.
    #[test]
    fn agent_codex_config_merges_preserving_other() {
        let existing = "\
[mcp_servers.other]
url = \"http://example/other\"

[profile]
name = \"me\"
";
        let content = agent_codex_config(Some(existing), 9000);
        let doc: toml::Table = toml::from_str(&content).unwrap();
        assert_eq!(
            doc["mcp_servers"]["other"]["url"].as_str(),
            Some("http://example/other"),
            "pre-existing server must be preserved"
        );
        assert_eq!(
            doc["profile"]["name"].as_str(),
            Some("me"),
            "unrelated sections must survive"
        );
        assert_eq!(
            doc["mcp_servers"]["ump-dash"]["url"].as_str(),
            Some("http://127.0.0.1:9000/mcp")
        );
    }

    /// The tracked Codex config from UMP PR #3992 retains every team setting
    /// and its setup guidance while gaining the local dashboard endpoint.
    #[test]
    fn agent_codex_config_merges_pr_3992_team_servers_and_comments() {
        let before: toml::Table = toml::from_str(TEAM_CODEX_CONFIG).unwrap();
        let content = agent_codex_config(Some(TEAM_CODEX_CONFIG), 8790);
        let after: toml::Table = toml::from_str(&content).unwrap();

        for name in TEAM_SERVER_NAMES {
            assert_eq!(
                after["mcp_servers"][name], before["mcp_servers"][name],
                "team server {name} must remain unchanged"
            );
        }
        let dashboard = &after["mcp_servers"]["ump-dash"];
        assert_eq!(dashboard["url"].as_str(), Some("http://127.0.0.1:8790/mcp"));
        assert_eq!(dashboard["enabled"].as_bool(), Some(true));
        assert_eq!(dashboard["required"].as_bool(), Some(false));
        assert_eq!(dashboard["startup_timeout_sec"].as_integer(), Some(3));
        assert_eq!(dashboard["tool_timeout_sec"].as_integer(), Some(900));

        for comment in [
            "# Project-scoped Codex MCP config (read for trusted projects; shared between",
            "# Sauce Labs (one PyPI package, two entry points). Credentials come from the",
            "# Per-user OAuth: `codex mcp login atlassian`",
            "# Disabled until design seats are provisioned — mirrors disabledMcpjsonServers",
            "# Per-user OAuth: `codex mcp login amplitude`",
        ] {
            assert!(
                content.contains(comment),
                "missing authored comment: {comment}"
            );
        }
    }

    /// Refreshing a canonical dashboard section changes that section only;
    /// team server tables, comments, and following sections remain authored.
    #[test]
    fn agent_codex_config_refreshes_ump_dash_only() {
        let existing = format!(
            "{TEAM_CODEX_CONFIG}\n[mcp_servers.\"ump-dash\"]\nurl = \"http://127.0.0.1:7000/mcp\"\nenabled = false\nrequired = true\nstartup_timeout_sec = 99\ntool_timeout_sec = 1\n\n[profile]\nname = \"me\"\n"
        );
        let before: toml::Table = toml::from_str(&existing).unwrap();
        let content = agent_codex_config(Some(&existing), 8790);
        let after: toml::Table = toml::from_str(&content).unwrap();

        for name in TEAM_SERVER_NAMES {
            assert_eq!(
                after["mcp_servers"][name], before["mcp_servers"][name],
                "team server {name} must remain unchanged"
            );
        }
        assert_eq!(after["profile"], before["profile"]);
        assert!(content.contains("# Per-user OAuth: `codex mcp login atlassian`"));
        assert_eq!(content.matches("ump-dash").count(), 1);

        let dashboard = &after["mcp_servers"]["ump-dash"];
        assert_eq!(dashboard["url"].as_str(), Some("http://127.0.0.1:8790/mcp"));
        assert_eq!(dashboard["enabled"].as_bool(), Some(true));
        assert_eq!(dashboard["required"].as_bool(), Some(false));
        assert_eq!(dashboard["startup_timeout_sec"].as_integer(), Some(3));
        assert_eq!(dashboard["tool_timeout_sec"].as_integer(), Some(900));
    }

    #[test]
    fn agent_codex_config_replaces_invalid_existing() {
        let content = agent_codex_config(Some("not valid toml = ["), 8790);
        let doc: toml::Table = toml::from_str(&content).unwrap();
        assert_eq!(
            doc["mcp_servers"]["ump-dash"]["url"].as_str(),
            Some("http://127.0.0.1:8790/mcp")
        );
    }

    #[test]
    fn agent_codex_config_normalizes_non_table_mcp_servers() {
        let content = agent_codex_config(Some("mcp_servers = \"invalid\"\n"), 8790);
        let doc: toml::Table = toml::from_str(&content).unwrap();
        assert_eq!(
            doc["mcp_servers"]["ump-dash"]["url"].as_str(),
            Some("http://127.0.0.1:8790/mcp")
        );
    }

    /// Provisioning writes the Codex config, the `.agents` run-app skill, and the
    /// openai.yaml manifest with the configured port.
    #[test]
    fn provision_writes_codex_files() {
        let wt = TempDir::new("wt");

        provision_worktree_agent_files(wt.path(), 8790);

        let cfg = fs::read_to_string(wt.path().join(".codex/config.toml")).unwrap();
        assert!(cfg.contains("http://127.0.0.1:8790/mcp"));
        assert!(cfg.contains("ump-dash"));

        let skill = fs::read_to_string(wt.path().join(".agents/skills/run-app/SKILL.md")).unwrap();
        assert!(
            skill.contains("name: run-app"),
            "codex skill frontmatter present"
        );

        let yaml = fs::read_to_string(wt.path().join(".agents/skills/run-app/agents/openai.yaml"))
            .unwrap();
        assert!(
            yaml.contains("http://127.0.0.1:8790/mcp"),
            "manifest carries the port"
        );
    }

    /// An existing Codex `.agents` run-app skill is never clobbered.
    #[test]
    fn provision_does_not_clobber_existing_codex_skill() {
        let wt = TempDir::new("wt");
        let skill_path = wt.path().join(".agents/skills/run-app/SKILL.md");
        fs::create_dir_all(skill_path.parent().unwrap()).unwrap();
        fs::write(&skill_path, b"USER EDITED").unwrap();

        provision_worktree_agent_files(wt.path(), 8790);

        assert_eq!(
            fs::read(&skill_path).unwrap(),
            b"USER EDITED",
            "an existing codex skill file must be left untouched"
        );
    }
}
