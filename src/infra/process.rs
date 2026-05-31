// src/infra/process.rs
//
// TokioProcessClient — concrete adapter implementing `ProcessPort` (defined in
// `crate::domain::ports::process_port`).
// ARCH-02: All infra behind trait boundaries — swap TokioProcessClient for a
// FakeProcessClient in tests without touching any domain or app code.

#![allow(dead_code)]

use crate::domain::ports::process_port::{ProcessPort, SpawnedMetroProcess};
use std::path::PathBuf;

/// Production implementation that calls `tokio::process::Command` directly.
pub struct TokioProcessClient;

const DEFAULT_METRO_PORT: u16 = 8081;

fn metro_spawn_program() -> &'static str {
    "yarn"
}

fn next_available_metro_port(start: u16) -> u16 {
    let mut port = start;
    loop {
        if crate::infra::port::port_is_free(port) {
            return port;
        }
        if port == u16::MAX {
            return start;
        }
        port += 1;
    }
}

fn metro_spawn_args() -> Vec<String> {
    metro_spawn_args_for_port(next_available_metro_port(DEFAULT_METRO_PORT))
}

fn metro_spawn_args_for_port(port: u16) -> Vec<String> {
    vec![
        "start:rozenite".to_string(),
        "--reset-cache".to_string(),
        "--port".to_string(),
        port.to_string(),
    ]
}

#[async_trait::async_trait]
impl ProcessPort for TokioProcessClient {
    async fn spawn_metro(&self, worktree_path: PathBuf) -> anyhow::Result<SpawnedMetroProcess> {
        let port = next_available_metro_port(DEFAULT_METRO_PORT);
        let mut cmd = tokio::process::Command::new(metro_spawn_program());
        cmd.args(metro_spawn_args_for_port(port))
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

        Ok(SpawnedMetroProcess {
            child: cmd.spawn()?,
            port,
        })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn metro_spawn_argv_uses_ump_rozenite_script_with_reset_cache_and_free_port() {
        let _occupy_default_port = std::net::TcpListener::bind(("127.0.0.1", 8081)).ok();
        let args = super::metro_spawn_args();

        assert_eq!(super::metro_spawn_program(), "yarn");
        assert_eq!(args[0], "start:rozenite");
        assert!(args.iter().any(|arg| arg == "--reset-cache"));

        let port_flag_index = args
            .iter()
            .position(|arg| arg == "--port")
            .expect("metro spawn args must include an explicit --port");
        let port: u16 = args
            .get(port_flag_index + 1)
            .expect("--port must be followed by a numeric port")
            .parse()
            .expect("metro port must parse as u16");
        assert_ne!(port, 8081, "occupied default port must be skipped");
    }
}
