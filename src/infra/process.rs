// src/infra/process.rs
//
// TokioProcessClient — concrete adapter implementing `ProcessPort` (defined in
// `crate::domain::ports::process_port`).
// ARCH-02: All infra behind trait boundaries — swap TokioProcessClient for a
// FakeProcessClient in tests without touching any domain or app code.

#![allow(dead_code)]

use crate::domain::ports::process_port::{MetroPortReservation, ProcessPort, SpawnedMetroProcess};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

/// Production implementation that calls `tokio::process::Command` directly.
pub struct TokioProcessClient;

const DEFAULT_METRO_PORT: u16 = 8081;

static RESERVED_METRO_PORTS: LazyLock<Mutex<HashSet<u16>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

#[derive(Debug)]
struct ReservedMetroPort {
    port: u16,
}

impl ReservedMetroPort {
    fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ReservedMetroPort {
    fn drop(&mut self) {
        if let Ok(mut reserved) = RESERVED_METRO_PORTS.lock() {
            reserved.remove(&self.port);
        }
    }
}

impl MetroPortReservation for ReservedMetroPort {}

fn metro_spawn_program() -> &'static str {
    "yarn"
}

fn reserve_next_available_metro_port(start: u16) -> anyhow::Result<ReservedMetroPort> {
    let mut reserved = RESERVED_METRO_PORTS
        .lock()
        .map_err(|_| anyhow::anyhow!("metro port reservation lock poisoned"))?;

    for port in start..=u16::MAX {
        if !reserved.contains(&port) && crate::infra::port::port_is_free(port) {
            reserved.insert(port);
            return Ok(ReservedMetroPort { port });
        }
    }
    for port in 1..start {
        if !reserved.contains(&port) && crate::infra::port::port_is_free(port) {
            reserved.insert(port);
            return Ok(ReservedMetroPort { port });
        }
    }

    Err(anyhow::anyhow!(
        "no available Metro port found from {start}"
    ))
}

fn metro_spawn_args() -> Vec<String> {
    let reservation = reserve_next_available_metro_port(DEFAULT_METRO_PORT)
        .expect("metro spawn args test helper should find a free port");
    metro_spawn_args_for_port(reservation.port())
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
    async fn spawn_metro(
        &self,
        worktree_path: PathBuf,
        port: u16,
    ) -> anyhow::Result<SpawnedMetroProcess> {
        let requested_port = port;
        tracing::info!(
            worktree = %worktree_path.display(),
            requested_port = requested_port,
            "[metro-start] reserving Metro port"
        );
        let port_reservation = reserve_next_available_metro_port(requested_port).map_err(|e| {
            let message = e.to_string();
            tracing::warn!(
                worktree = %worktree_path.display(),
                requested_port = requested_port,
                error = %message,
                "[metro-start] Metro port reservation failed"
            );
            anyhow::anyhow!("failed to reserve Metro port from {requested_port}: {message}")
        })?;
        let selected_port = port_reservation.port();
        let program = metro_spawn_program();
        let args = metro_spawn_args_for_port(selected_port);
        let command = format!("{program} {}", args.join(" "));
        tracing::info!(
            worktree = %worktree_path.display(),
            requested_port = requested_port,
            selected_port = selected_port,
            command = %command,
            "[metro-start] spawning Metro process"
        );

        let mut cmd = tokio::process::Command::new(program);
        cmd.args(&args)
            .current_dir(&worktree_path)
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

        let child = cmd.spawn().map_err(|e| {
            let message = e.to_string();
            tracing::warn!(
                worktree = %worktree_path.display(),
                requested_port = requested_port,
                selected_port = selected_port,
                command = %command,
                error = %message,
                "[metro-start] Metro process spawn failed"
            );
            anyhow::anyhow!(
                "failed to spawn Metro command `{command}` in {}: {message}",
                worktree_path.display()
            )
        })?;
        let pid = child.id().unwrap_or(0);
        tracing::info!(
            worktree = %worktree_path.display(),
            requested_port = requested_port,
            selected_port = selected_port,
            pid = pid,
            "[metro-start] Metro process spawned"
        );

        Ok(SpawnedMetroProcess {
            child,
            port: selected_port,
            port_reservation: Box::new(port_reservation),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{LazyLock, Mutex};

    static PORT_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    #[test]
    fn metro_spawn_argv_uses_ump_rozenite_script_with_reset_cache_and_free_port() {
        let _guard = PORT_TEST_LOCK.lock().unwrap();
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

    #[test]
    fn metro_spawn_args_use_supplied_port() {
        let args = super::metro_spawn_args_for_port(8097);

        let port_flag_index = args
            .iter()
            .position(|arg| arg == "--port")
            .expect("metro spawn args must include an explicit --port");
        assert_eq!(args.get(port_flag_index + 1), Some(&"8097".to_string()));
    }

    #[test]
    fn metro_port_selection_reserves_unbound_port_for_next_spawn() {
        let _guard = PORT_TEST_LOCK.lock().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let start = listener.local_addr().unwrap().port();
        drop(listener);

        let first = super::reserve_next_available_metro_port(start).unwrap();
        let second = super::reserve_next_available_metro_port(start).unwrap();

        assert_ne!(
            second.port(),
            first.port(),
            "a second Metro start must skip the first selected port even before Metro binds it"
        );
    }

    #[test]
    fn metro_port_reservation_drop_allows_port_reuse() {
        let _guard = PORT_TEST_LOCK.lock().unwrap();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let start = listener.local_addr().unwrap().port();
        drop(listener);

        let first = super::reserve_next_available_metro_port(start).unwrap();
        let first_port = first.port();
        assert!(
            super::RESERVED_METRO_PORTS
                .lock()
                .unwrap()
                .contains(&first_port),
            "reservation should mark the selected port as unavailable to later spawns"
        );
        drop(first);

        assert!(
            !super::RESERVED_METRO_PORTS
                .lock()
                .unwrap()
                .contains(&first_port),
            "dropping the Metro handle reservation should release the selected port"
        );
    }
}
