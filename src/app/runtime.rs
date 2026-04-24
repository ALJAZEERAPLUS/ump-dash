//! Async runtime — `pub async fn run` hosts the event loop; the 7 metro
//! helpers and the `InAppMetroHandle` bridge live here temporarily.
//!
//! Plan 13-07 moves the metro helpers and `InAppMetroHandle` out to
//! `src/infra/metro.rs` as the `TokioMetroAdapter` impl of `MetroPort`.

use super::handle_key::handle_key;
use super::state::AppState;
use super::update::update;
use crate::domain::action::Action;
use crate::domain::metro::MetroHandle;
use futures::StreamExt;
use ratatui::crossterm::event::EventStream;
use std::path::PathBuf;

/// Main application loop. Runs on the tokio runtime.
/// Renders on every event and on a 250ms tick. Exits when state.should_quit is true.
pub async fn run(mut terminal: ratatui::DefaultTerminal) -> color_eyre::Result<()> {
    let mut state = AppState::default();
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
    let mut refresh_interval = tokio::time::interval(std::time::Duration::from_secs(60));
    refresh_interval.tick().await; // consume the immediate first tick (startup already loads worktrees)

    // Channel for background tasks (log lines, MetroExited, WorktreesLoaded, etc.)
    let (metro_tx, mut metro_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();

    // Channel for the spawn task to deliver the MetroHandle once spawning is complete.
    // Plan 13-03: MetroHandle is a trait; the channel now carries `Box<dyn MetroHandle>`.
    let (handle_tx, mut handle_rx) =
        tokio::sync::mpsc::unbounded_channel::<Box<dyn MetroHandle>>();

    // Phase 5.1: multiplexer detection (replaces tmux_available bool)
    state.multiplexer = crate::infra::multiplexer::detect_multiplexer();

    // Phase 4: Load config + JIRA client + cache
    if let Ok(Some(config)) = crate::infra::config::load_config() {
        // Extract fields before moving config
        state.claude_flags = config.claude_flags.clone();
        state.jira_project_prefix = config.jira_project_prefix.clone();
        if let Some(path) = config.repo_root_path() {
            state.repo_root = path;
        }

        match crate::infra::jira::HttpJiraClient::new(&config) {
            Ok(client) => {
                state.jira_client = Some(std::sync::Arc::new(client));
            }
            Err(e) => {
                tracing::warn!("JIRA client init failed: {e}");
            }
        }
        state.config = Some(config);
    }
    state.jira_title_cache = crate::infra::jira_cache::load_jira_cache().unwrap_or_default();

    // Spawn initial worktree load
    {
        let repo_root = state.repo_root.clone();
        let init_tx = metro_tx.clone();
        tokio::spawn(async move {
            match crate::infra::worktrees::list_worktrees(&repo_root).await {
                Ok(wts) => {
                    let _ = init_tx.send(Action::WorktreesLoaded(wts));
                }
                Err(e) => {
                    tracing::warn!("initial worktree load failed: {e}");
                }
            }
        });
    }

    // Check for external metro on startup
    {
        let startup_tx = metro_tx.clone();
        tokio::spawn(async move {
            if let Some(info) = crate::infra::port::detect_external_metro(8081).await {
                let _ = startup_tx.send(Action::ExternalMetroDetected(
                    crate::domain::ports::port_probe_port::ExternalProcessInfo {
                        pid: info.pid,
                        working_dir: info.working_dir,
                    },
                ));
            }
        });
    }

    loop {
        // Render once per iteration — after all pending actions have been drained
        terminal.draw(|f| crate::ui::view(f, &mut state))?;

        // Wait for at least one event (blocks until something happens)
        tokio::select! {
            _ = tick.tick() => {
                // Periodic tick: triggers redraw for time-based UI updates
            }
            _ = refresh_interval.tick() => {
                // 60-second periodic refresh: keeps worktrees, staleness, labels, and JIRA titles current
                if state.running_command.is_none() {
                    update(&mut state, Action::RefreshWorktrees, &metro_tx, &handle_tx);
                }
            }
            maybe_event = events.next() => {
                let Some(Ok(event)) = maybe_event else { break };
                use ratatui::crossterm::event::Event as CE;
                match event {
                    CE::Key(key) => {
                        if let Some(action) = handle_key(&state, key) {
                            update(&mut state, action, &metro_tx, &handle_tx);
                        }
                    }
                    CE::Resize(_, _) => {}
                    _ => {}
                }
            }
            Some(action) = metro_rx.recv() => {
                update(&mut state, action, &metro_tx, &handle_tx);
            }
            Some(handle) = handle_rx.recv() => {
                state.metro.register(handle);
                update(&mut state, Action::RefreshWorktrees, &metro_tx, &handle_tx);
            }
        }

        // Drain all pending actions before redrawing — batches bursts of log lines
        // into a single frame instead of redrawing per-line
        loop {
            use tokio::sync::mpsc::error::TryRecvError;
            match metro_rx.try_recv() {
                Ok(action) => update(&mut state, action, &metro_tx, &handle_tx),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => break,
            }
            if let Ok(handle) = handle_rx.try_recv() { state.metro.register(handle) }
        }

        if state.should_quit {
            break;
        }
    }

    // Cleanup: kill metro process group before exiting.
    // We kill by PGID directly instead of going through the async metro_process_task,
    // because aborting stream_task would race with the kill.
    //
    // Plan 13-03: handle is now `Box<dyn MetroHandle>`; we capture `pid()` before
    // consuming it via `kill()`. The trait's `kill(self: Box<Self>)` aborts the
    // internal tokio tasks + signals kill_tx — the blocking PGID kill stays here
    // as a safety net for shutdown (the in-app bridge's async kill_tx path may
    // not flush before the runtime drops).
    if let Some(handle) = state.metro.take_handle() {
        let pid = handle.pid();
        // Kill the entire process group (yarn + node) so port 8081 is freed.
        // process_group(0) in spawn sets PGID = child PID, so -PID targets the group.
        let _ = std::process::Command::new("kill")
            .args(["-9", &format!("-{pid}")])
            .output();
        // Consuming kill — aborts stream_task / stdin_task + signals kill_tx on
        // the in-app bridge. Ignoring the result: shutdown is best-effort.
        let _ = handle.kill();
    }
    if let Some(task) = state.command_task.take() {
        task.abort();
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Async helpers — all run inside tokio::spawn, never blocking the event loop
// ---------------------------------------------------------------------------

/// Temporary bridge: `MetroHandle` impl backed by the same 4 tokio fields that
/// the concrete `domain::metro::MetroHandle` struct previously held publicly.
///
/// Plan 13-03 introduces this to keep `spawn_metro_task` compiling after the
/// struct → trait conversion. Plan 13-07 moves this logic into
/// `src/infra/metro.rs::TokioMetroHandle` (the sole `MetroPort` adapter) and
/// deletes this bridge.
#[derive(Debug)]
struct InAppMetroHandle {
    pid: u32,
    worktree_id: String,
    stdin_tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    stream_task: tokio::task::JoinHandle<()>,
    stdin_task: tokio::task::JoinHandle<()>,
    /// Wrapped in Option so it can be taken exactly once via kill_tx.take().
    kill_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl crate::domain::metro::MetroHandle for InAppMetroHandle {
    fn pid(&self) -> u32 {
        self.pid
    }
    fn worktree_id(&self) -> &str {
        &self.worktree_id
    }
    fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()> {
        self.stdin_tx
            .send(bytes)
            .map_err(|e| anyhow::anyhow!("metro stdin send failed: {e}"))?;
        Ok(())
    }
    fn kill(mut self: Box<Self>) -> anyhow::Result<()> {
        // Order mirrors the previous inline logic at the MetroStop + shutdown
        // call sites: signal kill first, then abort the background tasks. The
        // metro_process_task observes kill_rx and performs the PGID SIGKILL +
        // port-free wait itself.
        if let Some(kill_tx) = self.kill_tx.take() {
            let _ = kill_tx.send(());
        }
        self.stream_task.abort();
        self.stdin_task.abort();
        Ok(())
    }
}

/// Spawns the metro process and delivers a `Box<dyn MetroHandle>` via `handle_tx`.
///
/// Plan 13-03: constructs an `InAppMetroHandle` bridge (defined above) — the
/// trait impl wraps the same 4 tokio fields the old concrete `MetroHandle`
/// struct held. Plan 13-07 moves this logic into `TokioMetroAdapter` inside
/// `src/infra/metro.rs` and removes the bridge.
pub(super) async fn spawn_metro_task(
    worktree_path: PathBuf,
    action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
    handle_tx: tokio::sync::mpsc::UnboundedSender<Box<dyn MetroHandle>>,
) {
    use crate::domain::ports::process_port::ProcessPort;
    use crate::infra::process::TokioProcessClient;

    let client = TokioProcessClient;
    match client.spawn_metro(worktree_path.clone()).await {
        Ok(mut child) => {
            let pid = child.id().unwrap_or(0);

            let stdout = child.stdout.take().expect("stdout piped");
            let stderr = child.stderr.take().expect("stderr piped");
            let stdin = child.stdin.take().expect("stdin piped");

            let (stdin_tx, stdin_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
            let stdin_task = tokio::spawn(stdin_writer(stdin, stdin_rx));

            let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();

            let stream_tx = action_tx.clone();
            let stream_task =
                tokio::spawn(metro_process_task(child, stdout, stderr, kill_rx, stream_tx));

            let worktree_id = worktree_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".to_string());

            let handle: Box<dyn MetroHandle> = Box::new(InAppMetroHandle {
                pid,
                worktree_id,
                stdin_tx,
                stream_task,
                stdin_task,
                kill_tx: Some(kill_tx),
            });

            let _ = handle_tx.send(handle);
        }
        Err(e) => {
            tracing::error!("metro spawn failed: {e}");
            let _ = action_tx.send(Action::MetroSpawnFailed(format!("{e}")));
        }
    }
}

/// Owns the `Child` process. Handles kill signal and natural exit.
async fn metro_process_task(
    mut child: tokio::process::Child,
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    kill_rx: tokio::sync::oneshot::Receiver<()>,
    tx: tokio::sync::mpsc::UnboundedSender<Action>,
) {
    let drain_task = tokio::spawn(drain_metro_output(stdout, stderr, tx.clone()));

    let pid = child.id();

    tokio::select! {
        _ = kill_rx => {
            drain_task.abort();
            // Kill the entire process group. process_group(0) in spawn_metro makes
            // the child the group leader (PID == PGID). Sending SIGKILL to -PGID
            // kills yarn AND the Node metro server that holds port 8081.
            // child.kill() alone only kills yarn — the node subprocess survives.
            if let Some(id) = pid {
                unsafe { libc::kill(-(id as i32), libc::SIGKILL); }
            }
            // Reap the child to prevent zombie processes
            let _ = child.wait().await;
            for _ in 0..50 {
                if crate::infra::port::port_is_free(8081) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            let _ = tx.send(Action::MetroExited);
        }
        _ = child.wait() => {
            drain_task.abort();
            let _ = tx.send(Action::MetroExited);
        }
    }
}

/// Parses a single metro stdout/stderr line into a MetroActivity, if recognizable.
///
/// Uses simple string matching — no regex crate required.
fn parse_metro_line(line: &str) -> Option<crate::domain::metro::MetroActivity> {
    use crate::domain::metro::MetroActivity;

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
        // Try to extract percentage: find digits followed by '%'
        let percent = extract_percent(line);
        return Some(MetroActivity::Bundling { percent });
    }

    // Error lines — skip source-map and deprecated noise
    if lower.contains("error")
        && !lower.contains("source-map")
        && !lower.contains("deprecated")
    {
        let truncated = if line.len() > 80 { line[..80].to_string() } else { line.to_string() };
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

/// Drains stdout and stderr from the metro process, parsing lines for activity updates.
async fn drain_metro_output(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    action_tx: tokio::sync::mpsc::UnboundedSender<Action>,
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
                            let _ = action_tx.send(Action::MetroActivityUpdate(activity));
                        }
                    }
                    _ => { stdout_done = true; }
                }
            }
            line = stderr_lines.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(line)) => {
                        if let Some(activity) = parse_metro_line(&line) {
                            let _ = action_tx.send(Action::MetroActivityUpdate(activity));
                        }
                    }
                    _ => { stderr_done = true; }
                }
            }
        }
        if stdout_done && stderr_done { break; }
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

pub(super) async fn metro_http_post(url: &str, body: &str) -> anyhow::Result<()> {
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
