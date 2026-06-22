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
use crate::domain::worktree::WorktreeId;
use futures::StreamExt;
use ratatui::crossterm::event::EventStream;

fn metro_worktree_id_from_path(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn slice_id_for_metro_worktree_id(state: &AppState, worktree_id: &str) -> WorktreeId {
    state
        .worktree_browser
        .worktrees
        .iter()
        .find(|wt| metro_worktree_id_from_path(&wt.path) == worktree_id)
        .map(|wt| wt.id.clone())
        .or_else(|| {
            state
                .worktrees
                .keys()
                .find(|id| id.0 == worktree_id)
                .cloned()
        })
        .unwrap_or_else(|| WorktreeId(worktree_id.to_string()))
}

fn register_metro_handle(state: &mut AppState, handle: Box<dyn MetroHandle>) {
    let worktree_id = handle.worktree_id().to_string();
    let slice_id = slice_id_for_metro_worktree_id(state, &worktree_id);
    let slice = state.worktrees.entry(slice_id.clone()).or_insert_with(|| {
        crate::domain::worktree_slice::WorktreeSlice {
            id: slice_id,
            ..Default::default()
        }
    });
    slice.metro.register(handle);
}

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
    let (handle_tx, mut handle_rx) = tokio::sync::mpsc::unbounded_channel::<Box<dyn MetroHandle>>();
    // Phase 14 / Q2: dedicated channel for delivering freshly-spawned TaskRecord
    // to the main thread. Mirrors handle_tx for Box<dyn MetroHandle>.
    let (task_handle_tx, mut task_handle_rx) = tokio::sync::mpsc::unbounded_channel::<(
        crate::domain::worktree::WorktreeId,
        crate::domain::task::TaskRecord,
    )>();

    let runner = EffectRunner::new(
        adapters,
        action_tx.clone(),
        handle_tx.clone(),
        task_handle_tx.clone(),
    );

    // Startup effects: load worktrees. Metro launches choose an available port
    // at spawn time, so startup no longer locks on external 8081 ownership.
    runner
        .run_effects(vec![super::effect::Effect::ListWorktrees {
            repo_root: state.app_config.repo_root.clone(),
        }])
        .await;

    loop {
        // Render once per iteration — after all pending actions have been drained
        terminal.draw(|f| crate::ui::view(f, &mut state))?;

        // Wait for at least one event (blocks until something happens)
        tokio::select! {
            biased;
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
                register_metro_handle(&mut state, handle);
                let effects = update(&mut state, Action::RefreshWorktrees);
                runner.run_effects(effects).await;
            }
        }

        // Drain pending task records before actions so fast process exits can
        // still find their owner slice and drain any queued follow-up command.
        while let Ok((wt_id, record)) = task_handle_rx.try_recv() {
            if let Some(slice) = state.worktrees.get_mut(&wt_id) {
                slice.task = Some(record);
            } else {
                record.handle.abort();
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
            if let Ok(handle) = handle_rx.try_recv() {
                register_metro_handle(&mut state, handle)
            }
            while let Ok((wt_id, record)) = task_handle_rx.try_recv() {
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

    // Cleanup: kill metro process groups before exiting.
    // We kill by PGID directly instead of going through the async metro_process_task,
    // because aborting stream_task would race with the kill.
    for handle in state
        .worktrees
        .values_mut()
        .filter_map(|slice| slice.metro.take_handle())
    {
        let pid = handle.pid();
        // Kill the entire process group (yarn + node) so its port is freed.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::worktree::{Worktree, WorktreeMetroStatus};

    #[derive(Debug)]
    struct FakeMetroHandle {
        pid: u32,
        worktree_id: String,
        port: u16,
    }

    impl MetroHandle for FakeMetroHandle {
        fn pid(&self) -> u32 {
            self.pid
        }
        fn worktree_id(&self) -> &str {
            &self.worktree_id
        }
        fn port(&self) -> u16 {
            self.port
        }
        fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
            Ok(())
        }
        fn kill(self: Box<Self>) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn register_metro_handle_resolves_directory_name_to_full_path_slice_id() {
        let mut state = AppState::default();
        let wt_id = WorktreeId("/tmp/wt-a".into());
        state.worktree_browser.worktrees.push(Worktree {
            id: wt_id.clone(),
            path: std::path::PathBuf::from("/tmp/wt-a"),
            branch: "main".into(),
            head_sha: "0000000".into(),
            metro_status: WorktreeMetroStatus::Stopped,
            jira_title: None,
            stale: false,
            stale_pods: false,
            jira_key: None,
        });
        state.worktrees.insert(
            wt_id.clone(),
            crate::domain::worktree_slice::WorktreeSlice {
                id: wt_id.clone(),
                ..Default::default()
            },
        );

        register_metro_handle(
            &mut state,
            Box::new(FakeMetroHandle {
                pid: 9001,
                worktree_id: "wt-a".into(),
                port: 8081,
            }),
        );

        let slice = state.worktrees.get(&wt_id).unwrap();
        assert!(slice.metro.is_running());
        assert_eq!(slice.metro.running_port(), Some(8081));
    }
}
