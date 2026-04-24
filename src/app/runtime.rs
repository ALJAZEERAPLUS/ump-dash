//! Async runtime — `pub async fn run` hosts the event loop.
//!
//! Post-13-07: the 7 async metro helpers and the `InAppMetroHandle` bridge
//! that lived here temporarily (Plan 13-06 lift) have moved to
//! `src/infra/metro.rs` as the `TokioMetroAdapter` impl of `MetroPort`.
//! This file now only hosts the event-loop orchestration: construct the
//! `EffectRunner`, read events, call `update()` → `Vec<Effect>`, dispatch
//! the effects.

use super::effect_runner::EffectRunner;
use super::handle_key::handle_key;
use super::state::AppState;
use super::update::update;
use crate::domain::action::Action;
use crate::domain::metro::MetroHandle;
use crate::domain::ports::metro_port::MetroPort;
use futures::StreamExt;
use ratatui::crossterm::event::EventStream;
use std::sync::Arc;

/// Main application loop. Runs on the tokio runtime.
/// Renders on every event and on a 250ms tick. Exits when state.should_quit is true.
pub async fn run(mut terminal: ratatui::DefaultTerminal) -> color_eyre::Result<()> {
    let mut state = AppState::default();
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
    let mut refresh_interval = tokio::time::interval(std::time::Duration::from_secs(60));
    refresh_interval.tick().await; // consume the immediate first tick (startup already loads worktrees)

    // Single action channel — consolidated from pre-13-07 metro_tx / handle_tx split.
    // The effect_runner forwards Action results; handle registration flows back via
    // the same channel using Action::MetroHandleRegistered-like shape (we reuse the
    // existing MetroActivityUpdate / MetroSpawnFailed / etc. actions and register the
    // handle via state.metro.register() inline when SpawnMetro completes).
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
    let (handle_tx, mut handle_rx) =
        tokio::sync::mpsc::unbounded_channel::<Box<dyn MetroHandle>>();

    // Construct the MetroPort adapter (Plan 13-07 F-203 consumer). Plan 13-08
    // will replace this with a full Adapters struct injected into EffectRunner.
    let metro: Arc<dyn MetroPort> = Arc::new(crate::infra::metro::TokioMetroAdapter::new());
    let runner = EffectRunner::new(metro, action_tx.clone(), handle_tx.clone());

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

    // Startup effects: load worktrees + check for external metro
    runner
        .run_effects(vec![
            super::effect::Effect::ListWorktrees,
            super::effect::Effect::DetectExternalMetro { port: 8081 },
        ])
        .await;

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
                    let effects = update(&mut state, Action::RefreshWorktrees);
                    runner.run_effects(effects).await;
                }
            }
            maybe_event = events.next() => {
                let Some(Ok(event)) = maybe_event else { break };
                use ratatui::crossterm::event::Event as CE;
                match event {
                    CE::Key(key) => {
                        if let Some(action) = handle_key(&state, key) {
                            let effects = update(&mut state, action);
                            runner.run_effects(effects).await;
                        }
                    }
                    CE::Resize(_, _) => {}
                    _ => {}
                }
            }
            Some(action) = action_rx.recv() => {
                let effects = update(&mut state, action);
                runner.run_effects(effects).await;
            }
            Some(handle) = handle_rx.recv() => {
                state.metro.register(handle);
                let effects = update(&mut state, Action::RefreshWorktrees);
                runner.run_effects(effects).await;
            }
        }

        // Drain all pending actions before redrawing — batches bursts of log lines
        // into a single frame instead of redrawing per-line
        loop {
            use tokio::sync::mpsc::error::TryRecvError;
            match action_rx.try_recv() {
                Ok(action) => {
                    let effects = update(&mut state, action);
                    runner.run_effects(effects).await;
                }
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
    // Plan 13-07: handle is a TokioMetroHandle (from infra/metro.rs); we capture
    // `pid()` before consuming it via `kill()`. The trait's `kill(self: Box<Self>)`
    // aborts the internal tokio tasks + signals kill_tx — the blocking PGID kill
    // stays here as a safety net for shutdown (the adapter's async kill path may
    // not flush before the runtime drops).
    if let Some(handle) = state.metro.take_handle() {
        let pid = handle.pid();
        // Kill the entire process group (yarn + node) so port 8081 is freed.
        // process_group(0) in spawn sets PGID = child PID, so -PID targets the group.
        let _ = std::process::Command::new("kill")
            .args(["-9", &format!("-{pid}")])
            .output();
        // Consuming kill — aborts stream_task / stdin_task + signals kill_tx on
        // the adapter. Ignoring the result: shutdown is best-effort.
        let _ = handle.kill();
    }
    if let Some(task) = state.command_task.take() {
        task.abort();
    }

    Ok(())
}
