// src/infra/port.rs
//
// Port availability probe. Uses std::net::TcpListener::bind as the check — a successful
// bind means no process currently holds the port. This is the correct approach for
// verifying that metro's port 8081 is free after a kill (research pattern 3).

/// Returns true if no process is currently bound to `port` on 127.0.0.1.
///
/// Uses `TcpListener::bind` as the probe — bind succeeds only when the address is free.
/// Call this in a retry loop after killing metro; the port may briefly remain in
/// TIME_WAIT even after SIGKILL (research pitfall 3).
pub fn port_is_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

/// Information about an external (non-dashboard) process occupying a port.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalMetroInfo {
    pub pid: u32,
    pub working_dir: String,
}

/// Detect if an external (non-dashboard) process is listening on the given port.
/// Fast path: a successful `TcpListener::bind` means the port is free — returns None
/// without invoking `lsof` (lsof can hang for seconds on macOS while scanning mounts).
/// Slow path: port occupied → run `lsof -nP -i :PORT -sTCP:LISTEN -t` (no name/port
/// resolution) under a 2s timeout to get the PID, then another bounded lsof for cwd.
pub async fn detect_external_metro(port: u16) -> Option<ExternalMetroInfo> {
    // Fast path — skip lsof entirely when the port is free.
    if port_is_free(port) {
        return None;
    }

    let timeout = std::time::Duration::from_secs(2);

    let pid_fut = tokio::process::Command::new("lsof")
        .args(["-nP", "-i", &format!(":{port}"), "-sTCP:LISTEN", "-t"])
        .output();
    let output = tokio::time::timeout(timeout, pid_fut).await.ok()?.ok()?;

    let pid_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let pid: u32 = pid_str.lines().next()?.parse().ok()?;

    let cwd_fut = tokio::process::Command::new("lsof")
        .args(["-nP", "-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output();
    let cwd_output = tokio::time::timeout(timeout, cwd_fut).await.ok()?.ok()?;

    let cwd_text = String::from_utf8_lossy(&cwd_output.stdout);
    let working_dir = cwd_text
        .lines()
        .find(|l| l.starts_with('n'))
        .map(|l| l[1..].to_string())
        .unwrap_or_else(|| "unknown".to_string());

    Some(ExternalMetroInfo { pid, working_dir })
}

/// Kill a process by PID using SIGKILL.
pub async fn kill_process(pid: u32) -> anyhow::Result<()> {
    tokio::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output()
        .await?;
    Ok(())
}

/// F-102 adapter: wraps the existing free fns behind the `PortProbePort` trait.
///
/// Consumers after Plan 13-08 receive this as `Arc<dyn PortProbePort>` and see
/// only the trait surface. Until then the free fns above remain the primary
/// call path (app.rs still calls them directly).
pub struct LsofPortProbe;

#[async_trait::async_trait]
impl crate::domain::ports::port_probe_port::PortProbePort for LsofPortProbe {
    fn port_is_free(&self, port: u16) -> bool {
        port_is_free(port)
    }

    async fn detect_external(
        &self,
        port: u16,
    ) -> Option<crate::domain::ports::port_probe_port::ExternalProcessInfo> {
        detect_external_metro(port).await.map(|info| {
            crate::domain::ports::port_probe_port::ExternalProcessInfo {
                pid: info.pid,
                working_dir: info.working_dir,
            }
        })
    }

    async fn kill_process(&self, pid: u32) -> anyhow::Result<()> {
        kill_process(pid).await
    }
}
