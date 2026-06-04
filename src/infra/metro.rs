//! Metro lifecycle adapter — F-203 consumer (Plan 13-07).
//!
//! Moves the 7 async metro helpers and the `InAppMetroHandle` bridge out of
//! `src/app/runtime.rs` (where Plan 13-06 had parked them temporarily) into
//! this infra-side adapter. The adapter implements `MetroPort` (defined by
//! Plan 13-03 in `src/domain/ports/metro_port.rs`); the concrete
//! `TokioMetroHandle` replaces `InAppMetroHandle`.
//!
//! Preserved invariants:
//! - process_group(0) + kill_on_drop(true) on the spawn (from TokioProcessClient)
//! - PGID broadcast via `libc::kill(-pid, SIGKILL)` when killing
//! - 50 × 100ms selected-port-free wait loop before declaring the port free
//! - per-worktree duplicate prevention enforced by the caller (WorktreeMetro.register)
//!
//! Design note: `on_activity` is a `Box<dyn Fn(MetroActivity) + Send + Sync>`
//! callback per Plan 13-03 Pitfall 8 — keeps the trait tokio-free while the
//! adapter internally uses tokio primitives.

#![allow(dead_code)]

use crate::domain::metro::MetroActivity;
use crate::domain::ports::metro_port::{MetroHandle, MetroPort};
use crate::domain::ports::process_port::MetroPortReservation;
use std::path::PathBuf;
use std::sync::Arc;

/// Concrete `MetroPort` impl. Owns no state — one per process lifetime.
pub struct TokioMetroAdapter;

impl TokioMetroAdapter {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TokioMetroAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Concrete `MetroHandle` impl. Owns the 4 tokio resources (stdin channel,
/// stream task, stdin task, kill oneshot) that `InAppMetroHandle` used to own
/// in `src/app/runtime.rs`. The trait's consuming `kill(self: Box<Self>)`
/// aborts the background tasks and signals the kill oneshot — the
/// `metro_process_task` observes the oneshot and does the PGID SIGKILL +
/// port-free wait itself.
#[derive(Debug)]
struct TokioMetroHandle {
    pid: u32,
    worktree_id: String,
    port: u16,
    port_reservation: Box<dyn MetroPortReservation>,
    stdin_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    stream_task: tokio::task::JoinHandle<()>,
    stdin_task: tokio::task::JoinHandle<()>,
    /// Wrapped in Option so it can be taken exactly once via kill_tx.take().
    kill_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl MetroHandle for TokioMetroHandle {
    fn pid(&self) -> u32 {
        self.pid
    }
    fn worktree_id(&self) -> &str {
        &self.worktree_id
    }
    fn port(&self) -> u16 {
        self.port
    }
    fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        self.stdin_tx
            .send(bytes)
            .map_err(|e| anyhow::anyhow!("metro stdin send failed: {e}"))?;
        Ok(())
    }
    fn kill(mut self: Box<Self>) -> anyhow::Result<()> {
        // Signal the owner task and let it perform PGID SIGKILL, reap, and
        // MetroActivity::Exited emission. Dropping a JoinHandle detaches the
        // task; aborting it here would race with the kill signal.
        if let Some(kill_tx) = self.kill_tx.take() {
            let _ = kill_tx.send(());
        }
        self.stdin_task.abort();
        Ok(())
    }
}

#[async_trait::async_trait]
impl MetroPort for TokioMetroAdapter {
    /// Spawn metro in the given worktree, returning an opaque handle.
    ///
    /// `on_activity` is invoked for every `MetroActivity` parsed from the
    /// child's stdout/stderr. The adapter owns the channel plumbing internally
    /// — the callback is the trait-visible contract.
    async fn start(
        &self,
        worktree: PathBuf,
        port: u16,
        on_activity: Box<dyn Fn(MetroActivity) + Send + Sync>,
    ) -> anyhow::Result<Box<dyn MetroHandle>> {
        use crate::domain::ports::process_port::ProcessPort;
        use crate::infra::process::TokioProcessClient;

        let client = TokioProcessClient;
        let spawned = client.spawn_metro(worktree.clone(), port).await?;
        let mut child = spawned.child;
        let port = spawned.port;
        let port_reservation = spawned.port_reservation;

        let pid = child.id().unwrap_or(0);

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let stdin = child.stdin.take().expect("stdin piped");

        let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
        let stdin_task = tokio::spawn(stdin_writer(stdin, stdin_rx));

        let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();

        let stream_task = tokio::spawn(metro_process_task(
            child,
            stdout,
            stderr,
            kill_rx,
            port,
            on_activity,
        ));

        let worktree_id = worktree
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let handle: Box<dyn MetroHandle> = Box::new(TokioMetroHandle {
            pid,
            worktree_id,
            port,
            port_reservation,
            stdin_tx,
            stream_task,
            stdin_task,
            kill_tx: Some(kill_tx),
        });

        Ok(handle)
    }

    async fn http_post(&self, url: &str, body: &str) -> anyhow::Result<()> {
        metro_http_post(url, body).await
    }
}

// ---------------------------------------------------------------------------
// Private module-scope helpers (moved verbatim from src/app/runtime.rs)
// ---------------------------------------------------------------------------

/// Owns the `Child` process. Handles kill signal and natural exit. Differs
/// from the pre-13-07 version only in the sink: instead of sending
/// `Action::MetroActivityUpdate(activity)` on an mpsc channel, it invokes the
/// `on_activity` callback.
async fn metro_process_task(
    mut child: tokio::process::Child,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    mut kill_rx: tokio::sync::oneshot::Receiver<()>,
    port: u16,
    on_activity: Box<dyn Fn(MetroActivity) + Send + Sync>,
) {
    let on_activity: Arc<dyn Fn(MetroActivity) + Send + Sync> = Arc::from(on_activity);
    let drain_task = tokio::spawn(drain_metro_output(stdout, stderr, on_activity.clone()));

    let pid = child.id();

    tokio::select! {
        kill_result = &mut kill_rx => {
            drain_task.abort();
            if kill_result.is_ok() {
                // Kill the entire process group. process_group(0) in spawn_metro makes
                // the child the group leader (PID == PGID). Sending SIGKILL to -PGID
                // kills yarn AND the Node metro server that holds the selected port.
                if let Some(id) = pid {
                    unsafe { libc::kill(-(id as i32), libc::SIGKILL); }
                }
                let _ = child.wait().await;
                for _ in 0..50 {
                    if crate::infra::port::port_is_free(port) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            } else {
                let _ = child.wait().await;
            }
            on_activity(MetroActivity::Exited);
        }
        _ = child.wait() => {
            drain_task.abort();
            on_activity(MetroActivity::Exited);
        }
    }
}

/// Parses a single metro stdout/stderr line into a `MetroActivity`, if
/// recognizable. Uses simple string matching — no regex crate required.
fn parse_metro_line(line: &str) -> Option<MetroActivity> {
    let lower = line.to_lowercase();

    // Server ready signal
    if line.contains("Welcome to Metro") || line.contains("Fast - Scalable - Integrated") {
        return Some(MetroActivity::Ready);
    }

    // Device connection
    if lower.contains("client connected") {
        return Some(MetroActivity::DeviceConnected);
    }

    // Bundling progress — look for "BUNDLE" with optional percentage
    if line.contains("BUNDLE") {
        let percent = extract_percent(line);
        return Some(MetroActivity::Bundling { percent });
    }

    // Error lines — skip source-map and deprecated noise
    if lower.contains("error") && !lower.contains("source-map") && !lower.contains("deprecated") {
        let truncated = if line.chars().count() > 80 {
            line.chars().take(80).collect()
        } else {
            line.to_string()
        };
        return Some(MetroActivity::Error(truncated));
    }

    None
}

/// Extracts the first percentage value (e.g. "75") from a string like "BUNDLE 75%".
/// Returns None if no percentage pattern is found.
fn extract_percent(s: &str) -> Option<u8> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_digit() {
            let start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'%' {
                let num_str = &s[start..i];
                if let Ok(n) = num_str.parse::<u8>() {
                    return Some(n);
                }
            }
        }
        i += 1;
    }
    None
}

/// Drains stdout and stderr from the metro process, parsing lines for activity
/// updates. Invokes `on_activity` for every parsed activity.
async fn drain_metro_output(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    on_activity: Arc<dyn Fn(MetroActivity) + Send + Sync>,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();
    let mut stdout_done = false;
    let mut stderr_done = false;

    loop {
        tokio::select! {
            line = stdout_lines.next_line(), if !stdout_done => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(activity) = parse_metro_line(&line) {
                            on_activity(activity);
                        }
                    }
                    _ => { stdout_done = true; }
                }
            }
            line = stderr_lines.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(activity) = parse_metro_line(&line) {
                            on_activity(activity);
                        }
                    }
                    _ => { stderr_done = true; }
                }
            }
        }
        if stdout_done && stderr_done {
            break;
        }
    }
}

/// Forwards byte buffers from the `rx` channel to the child's stdin handle.
async fn stdin_writer(
    mut stdin: tokio::process::ChildStdin,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>,
) {
    use tokio::io::AsyncWriteExt;
    while let Some(bytes) = rx.recv().await {
        if let Err(e) = stdin.write_all(&bytes).await {
            tracing::warn!("metro stdin write failed: {e}");
            break;
        }
    }
}

/// Fire-and-forget HTTP POST to metro's control endpoint (e.g. `/reload`,
/// `/open-debugger`). 5 second timeout; any non-2xx response is an error.
async fn metro_http_post(url: &str, body: &str) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;
    let resp = client
        .post(url)
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("HTTP {} from {}", resp.status(), url);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_metro_line_ready_signal() {
        assert_eq!(
            parse_metro_line("Welcome to Metro!"),
            Some(MetroActivity::Ready)
        );
        assert_eq!(
            parse_metro_line("Fast - Scalable - Integrated"),
            Some(MetroActivity::Ready)
        );
    }

    #[test]
    fn parse_metro_line_bundling_with_percent() {
        match parse_metro_line("BUNDLE ./index.js 75%") {
            Some(MetroActivity::Bundling { percent: Some(75) }) => {}
            other => panic!("expected Bundling 75%; got {other:?}"),
        }
    }

    #[test]
    fn parse_metro_line_truncates_unicode_error_on_char_boundary() {
        let line = format!("error: {}", "é".repeat(100));

        match parse_metro_line(&line) {
            Some(MetroActivity::Error(msg)) => assert!(msg.chars().count() <= 80),
            other => panic!("expected unicode error activity; got {other:?}"),
        }
    }

    #[tokio::test]
    async fn metro_process_task_emits_exited_when_child_exits() -> anyhow::Result<()> {
        let mut child = tokio::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let (_kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();
        let (activity_tx, mut activity_rx) = tokio::sync::mpsc::unbounded_channel();

        metro_process_task(
            child,
            stdout,
            stderr,
            kill_rx,
            8081,
            Box::new(move |activity| {
                let _ = activity_tx.send(activity);
            }),
        )
        .await;

        let activity = tokio::time::timeout(std::time::Duration::from_secs(1), activity_rx.recv())
            .await?
            .expect("activity channel should stay open");
        assert_eq!(activity, MetroActivity::Exited);

        Ok(())
    }

    #[test]
    fn extract_percent_finds_first_value() {
        assert_eq!(extract_percent("progress 42%"), Some(42));
        assert_eq!(extract_percent("no percent here"), None);
        assert_eq!(extract_percent("multiple 10% and 90%"), Some(10));
    }

    #[test]
    fn tokio_metro_adapter_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TokioMetroAdapter>();
    }
}
