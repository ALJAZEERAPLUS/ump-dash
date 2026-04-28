//! Async runtime — `pub async fn run` hosts the event loop.
//!
//! Plan 13-08: `run` no longer constructs adapters or loads config. The
//! caller (`src/main.rs`, the composition root) builds an `Adapters` bundle
//! plus a pre-populated `AppState` and passes both in. After this change
//! `src/app/runtime.rs` contains zero `infra::*` references — every
//! infra hop goes through the injected `Adapters`.

use super::adapters::Adapters;
use super::effect_runner::EffectRunner;
use super::handle_key::handle_key;
use super::state::AppState;
use super::update::update;
use crate::domain::action::Action;
use crate::domain::metro::MetroHandle;
use futures::StreamExt;
use ratatui::crossterm::event::EventStream;

/// Main application loop. Runs on the tokio runtime.
/// Renders on every event and on a 250ms tick. Exits when state.should_quit is true.
///
/// `state` arrives pre-populated by the caller (config loaded, jira cache
/// hydrated, multiplexer/jira availability set). `adapters` carries every
/// trait-object port the effect_runner needs.
pub async fn run(
    mut terminal: ratatui::DefaultTerminal,
    adapters: Adapters,
    mut state: AppState,
) -> color_eyre::Result<()> {
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(std::time::Duration::from_millis(250));
    let mut refresh_interval = tokio::time::interval(std::time::Duration::from_secs(60));
    refresh_interval.tick().await; // consume the immediate first tick (startup already loads worktrees)

    // Single action channel for app-layer Action values; a dedicated channel
    // for `Box<dyn MetroHandle>` because Action derives Clone+PartialEq and
    // the trait object does not.
    let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
    let (handle_tx, mut handle_rx) =
        tokio::sync::mpsc::unbounded_channel::<Box<dyn MetroHandle>>();
    // Phase 14 / Q2: dedicated channel for delivering freshly-spawned TaskRecord
    // to the main thread. Mirrors handle_tx for Box<dyn MetroHandle>.
    let (task_handle_tx, mut task_handle_rx) =
        tokio::sync::mpsc::unbounded_channel::<(
            crate::domain::worktree::WorktreeId,
            crate::domain::task::TaskRecord,
        )>();

    let runner = EffectRunner::new(adapters, action_tx.clone(), handle_tx.clone(), task_handle_tx.clone());

    // Startup effects: load worktrees + check for external metro.
    runner
        .run_effects(vec![
            super::effect::Effect::ListWorktrees {
                repo_root: state.app_config.repo_root.clone(),
            },
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
                // 60-second periodic refresh: keeps worktrees, staleness, labels, and JIRA titles current.
                // Plan 14-09: gate walks slices — skip refresh only if any worktree has a running task.
                let any_running = state.worktrees.values().any(|s| s.task.is_some());
                if !any_running {
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
            Some((wt_id, record)) = task_handle_rx.recv() => {
                // Phase 14: write the TaskRecord into the slice on the main thread.
                // RESEARCH §Pitfall P-6 race: if the worktree disappeared between spawn
                // and delivery, abort the orphan handle so the JoinHandle doesn't leak.
                if let Some(slice) = state.worktrees.get_mut(&wt_id) {
                    slice.task = Some(record);
                } else {
                    record.handle.abort();
                }
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
            if let Ok((wt_id, record)) = task_handle_rx.try_recv() {
                if let Some(slice) = state.worktrees.get_mut(&wt_id) {
                    slice.task = Some(record);
                } else {
                    record.handle.abort();
                }
            }
        }

        if state.should_quit {
            break;
        }
    }

    // Cleanup: kill metro process group before exiting.
    // We kill by PGID directly instead of going through the async metro_process_task,
    // because aborting stream_task would race with the kill.
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
    // Plan 14-09: abort all per-worktree tasks on shutdown.
    for slice in state.worktrees.values_mut() {
        if let Some(record) = slice.task.take() {
            record.handle.abort();
        }
    }

    Ok(())
}
