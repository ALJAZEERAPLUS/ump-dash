// src/infra/process.rs
//
// TokioProcessClient — concrete adapter implementing `ProcessPort` (defined in
// `crate::domain::ports::process_port`).
// ARCH-02: All infra behind trait boundaries — swap TokioProcessClient for a
// FakeProcessClient in tests without touching any domain or app code.

#![allow(dead_code)]

use crate::domain::ports::process_port::ProcessPort;
use std::path::PathBuf;
use tokio::process::Child;

/// Production implementation that calls `tokio::process::Command` directly.
pub struct TokioProcessClient;

fn metro_spawn_program() -> &'static str {
    "yarn"
}

fn metro_spawn_args() -> [&'static str; 2] {
    ["start:rozenite", "--reset-cache"]
}

#[async_trait::async_trait]
impl ProcessPort for TokioProcessClient {
    async fn spawn_metro(&self, worktree_path: PathBuf) -> anyhow::Result<Child> {
        let mut cmd = tokio::process::Command::new(metro_spawn_program());
        cmd.args(metro_spawn_args())
            .current_dir(worktree_path)
            // CRITICAL: process_group(0) puts yarn + all Node children in their own
            // process group. kill() on the Child will send SIGKILL to the whole group,
            // ensuring the Node subprocess that holds port 8081 is also killed.
            // Without this, only yarn dies and the port stays bound (research pitfall 2).
            .process_group(0)
            // Drop safety net: if the Child is dropped without an explicit kill() call
            // (e.g., panic), tokio will issue SIGKILL automatically.
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::piped());

        Ok(cmd.spawn()?)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn metro_spawn_argv_uses_ump_rozenite_script_with_reset_cache() {
        assert_eq!(super::metro_spawn_program(), "yarn");
        assert_eq!(super::metro_spawn_args(), ["start:rozenite", "--reset-cache"]);
    }
}
