//! TEA reducer — `pub fn update` is the ONLY place `AppState` is mutated.
//!
//! Plan 13-07 will rewrite `update()` to return `Vec<Effect>` instead of
//! directly calling `tokio::spawn`. For Plan 13-06 this is a verbatim lift
//! from src/app.rs — signature and body are unchanged.

use super::state::{active_output, active_worktree_id, AppState, ErrorState, FocusedPanel, PaletteMode, MAX_COMMAND_LINES};
use crate::domain::action::Action;
use crate::domain::command::{CleanOptions, CommandSpec, ModalState};
use crate::domain::metro::MetroHandle;
use std::path::PathBuf;

/// Directly dispatches a command without going through the pre-processing pipeline.
/// Used by ModalConfirm to run confirmed destructive commands, and internally after
/// text-input and device-picker modals complete.
///
/// Appends separator to per-worktree output, sets running_command, spawns the process task.
fn dispatch_command(
    state: &mut AppState,
    spec: CommandSpec,
    metro_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
) {
    let wt = if !state.worktrees.is_empty() {
        let idx = state.worktree_table_state.selected().unwrap_or(0);
        let idx = idx.min(state.worktrees.len() - 1);
        state.worktrees[idx].clone()
    } else {
        // No worktrees loaded yet — can't dispatch; log to a fallback message (no per-worktree key)
        tracing::warn!("dispatch_command: no worktree selected, dropping command {:?}", spec.label());
        return;
    };

    // Append a separator line to per-worktree output — output persists, not cleared on new command
    let wt_id = wt.id.clone();
    let output = state.command_output_by_worktree.entry(wt_id.clone()).or_default();
    output.push_back(format!("$ {}", spec.to_argv().join(" ")));
    // Cap per-worktree output at MAX_COMMAND_LINES
    while output.len() > MAX_COMMAND_LINES {
        output.pop_front();
    }
    // Reset scroll for this worktree to show the latest output
    state.command_output_scroll_by_worktree.insert(wt_id, 0);

    state.running_command = Some(spec.clone());

    // Abort any existing command task
    if let Some(task) = state.command_task.take() {
        task.abort();
    }

    let tx = metro_tx.clone();
    let path = wt.path.clone();
    let branch = wt.branch.clone();
    let spec_for_task = spec.clone();

    // F-101: infra::command_runner is now ignorant of Action — it emits typed
    // CommandEvents. Translate CommandEvent → Action at this app-side boundary.
    // Plan 13-08 will move this translation into effect_runner once Adapters
    // injection lands; for 13-05 we keep it inline at the only call site.
    use crate::domain::ports::command_runner_port::{CommandEvent, CommandRunnerPort};
    let runner = crate::infra::command_runner::TokioCommandRunner;
    let mut rx = runner.spawn(spec_for_task, path, branch);
    let handle = tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let action = match ev {
                CommandEvent::OutputLine(line) => Action::CommandOutputLine(line),
                CommandEvent::Exited(_status) => Action::CommandExited,
            };
            if tx.send(action).is_err() {
                break;
            }
        }
    });
    state.command_task = Some(handle);
}

/// TEA update function — the ONLY place AppState is mutated.
///
/// `metro_tx` and `handle_tx` are channels that connect update() to the async runtime:
/// - `metro_tx`: background tasks send Action events back to the loop
/// - `handle_tx`: spawn task sends the MetroHandle back so it can be registered in AppState
///
/// Async operations are always dispatched via tokio::spawn — update() never awaits.
pub fn update(
    state: &mut AppState,
    action: Action,
    metro_tx: &tokio::sync::mpsc::UnboundedSender<Action>,
    handle_tx: &tokio::sync::mpsc::UnboundedSender<Box<dyn MetroHandle>>,
) {
    // Clear pending_g on any action except SetPendingG
    if !matches!(action, Action::SetPendingG) {
        state.pending_g = false;
    }

    match action {
        // Phase 1 actions
        Action::FocusNext => state.focused_panel = state.focused_panel.next(),
        Action::FocusPrev => state.focused_panel = state.focused_panel.prev(),
        Action::FocusUp => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state) {
                    let scroll = state.command_output_scroll_by_worktree.entry(id).or_insert(0);
                    *scroll = scroll.saturating_sub(1);
                }
        }
        Action::FocusDown => {
            if state.focused_panel == FocusedPanel::CommandOutput {
                let max = active_output(state).len();
                if let Some(id) = active_worktree_id(state) {
                    let scroll = state.command_output_scroll_by_worktree.entry(id).or_insert(0);
                    if *scroll < max {
                        *scroll += 1;
                    }
                }
            }
        }
        Action::FocusLeft => {}
        Action::FocusRight => {}
        Action::Search => {
            // Phase 4+: stub
        }
        Action::ShowHelp => state.show_help = true,
        Action::DismissHelp => state.show_help = false,
        Action::DismissError => state.error_state = None,
        Action::RetryLastCommand => {
            state.error_state = None;
        }
        Action::Quit => state.should_quit = true,

        // --- Metro control actions ---

        Action::MetroStart => {
            state.palette_mode = None;
            if state.metro.is_running() {
                state.pending_restart = true;
                update(state, Action::MetroStop, metro_tx, handle_tx);
                return;
            }
            // Skip external detection when restarting our own metro (worktree switch or restart).
            // The port may still be releasing from our just-killed process — not an external conflict.
            if state.skip_external_metro_check {
                state.skip_external_metro_check = false;
                let _ = metro_tx.send(Action::MetroStartConfirmed);
                return;
            }
            // Check for external metro conflict before spawning
            let tx = metro_tx.clone();
            tokio::spawn(async move {
                if let Some(info) = crate::infra::port::detect_external_metro(8081).await {
                    let _ = tx.send(Action::ExternalMetroDetected(
                        crate::domain::ports::port_probe_port::ExternalProcessInfo {
                            pid: info.pid,
                            working_dir: info.working_dir,
                        },
                    ));
                } else {
                    let _ = tx.send(Action::MetroStartConfirmed);
                }
            });
        }

        Action::MetroStartConfirmed => {
            state.metro.set_starting();
            let tx = metro_tx.clone();
            let htx = handle_tx.clone();
            let worktree_path = state
                .active_worktree_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("."));
            tokio::spawn(super::runtime::spawn_metro_task(worktree_path, tx, htx));
        }

        Action::MetroStop => {
            state.palette_mode = None;
            if let Some(handle) = state.metro.take_handle() {
                state.metro.set_stopping();
                // Plan 13-03: kill() is the consuming trait method. The
                // InAppMetroHandle bridge (defined in runtime.rs post-split)
                // performs the kill_tx send + stream/stdin task abort that
                // previously lived inline here. Plan 13-07 replaces the
                // bridge with TokioMetroAdapter in src/infra/metro.rs.
                if let Err(e) = handle.kill() {
                    tracing::warn!("metro handle kill failed: {e}");
                }
            }
        }

        Action::MetroSendDebugger => {
            state.palette_mode = None;
            if state.metro.is_running() {
                tokio::spawn(async move {
                    let result = super::runtime::metro_http_post("http://localhost:8081/open-debugger", "{}").await;
                    match result {
                        Ok(_) => tracing::info!("debugger opened via HTTP"),
                        Err(e) => tracing::warn!("debugger open failed: {e}"),
                    }
                });
            }
        }

        Action::MetroSendReload => {
            state.palette_mode = None;
            if state.metro.is_running() {
                tokio::spawn(async move {
                    if let Err(e) = super::runtime::metro_http_post("http://localhost:8081/reload", "").await {
                        tracing::warn!("metro reload failed: {e}");
                    }
                });
            }
        }

        Action::MetroExited => {
            // Clear pending run command if metro exited unexpectedly
            state.pending_metro_run = None;
            state.metro.clear();
            if state.pending_restart {
                state.pending_restart = false;
                // Consume pending_switch_path if set (worktree switch takes priority)
                if let Some(path) = state.pending_switch_path.take() {
                    state.active_worktree_path = Some(path);
                }
                // Signal MetroStart to skip external detection — the port may still be
                // releasing from our just-killed process, not an external conflict.
                state.skip_external_metro_check = true;
                update(state, Action::MetroStart, metro_tx, handle_tx);
            }
            // Refresh worktree list so metro status (green bg) updates immediately
            update(state, Action::RefreshWorktrees, metro_tx, handle_tx);
        }

        Action::MetroSpawnFailed(msg) => {
            state.pending_metro_run = None;
            state.metro.clear();
            state.pending_restart = false;
            state.pending_switch_path = None;
            state.pending_metro_after_sync = false;
            state.error_state = Some(ErrorState {
                message: format!("Metro failed to start: {msg}"),
                can_retry: true,
            });
        }

        Action::MetroActivityUpdate(activity) => {
            state.metro.activity = Some(activity.clone());
            // Auto-dispatch pending RN run command when metro becomes Ready
            if matches!(activity, crate::domain::metro::MetroActivity::Ready)
                && let Some(run_spec) = state.pending_metro_run.take() {
                    // Re-enter the full CommandRun pipeline (sync check, device selection, etc.)
                    update(state, Action::CommandRun(run_spec), metro_tx, handle_tx);
                }
        }

        Action::ExternalMetroDetected(info) => {
            state.modal = Some(ModalState::ExternalMetroConflict {
                pid: info.pid,
                working_dir: info.working_dir,
            });
        }

        Action::KillExternalMetro(pid) => {
            state.modal = None;
            let tx = metro_tx.clone();
            tokio::spawn(async move {
                let _ = crate::infra::port::kill_process(pid).await;
                // Wait briefly for port to free, then auto-start metro
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let _ = tx.send(Action::MetroStartConfirmed);
            });
        }

        // --- Phase 3: Worktree navigation ---

        Action::WorktreeSelectNext => {
            let len = state.worktrees.len();
            if len > 0 {
                let i = state.worktree_table_state.selected().unwrap_or(0);
                let next = if i >= len - 1 { 0 } else { i + 1 };
                state.worktree_table_state.select(Some(next));
                // Update stable selection id
                state.selected_worktree_id = Some(state.worktrees[next].id.clone());
                // Update active worktree for metro
                state.active_worktree_path = Some(state.worktrees[next].path.clone());
            }
        }

        Action::WorktreeSelectPrev => {
            let len = state.worktrees.len();
            if len > 0 {
                let i = state.worktree_table_state.selected().unwrap_or(0);
                let prev = if i == 0 { len - 1 } else { i - 1 };
                state.worktree_table_state.select(Some(prev));
                // Update stable selection id
                state.selected_worktree_id = Some(state.worktrees[prev].id.clone());
                // Update active worktree for metro
                state.active_worktree_path = Some(state.worktrees[prev].path.clone());
            }
        }

        Action::WorktreesLoaded(mut worktrees) => {
            // Re-derive jira_key and re-apply cached JIRA titles using the configured prefix.
            // jira_key is set here (not in list_worktrees) because the prefix comes from config.
            for wt in &mut worktrees {
                if let Some(key) = crate::domain::jira::extract_jira_key(&wt.branch, &state.jira_project_prefix) {
                    if let Some(title) = state.jira_title_cache.get(&key) {
                        wt.jira_title = Some(title.clone());
                    }
                    wt.jira_key = Some(key);
                }
            }

            // Derive metro_status from current MetroManager state
            if let crate::domain::metro::MetroStatus::Running { ref worktree_id, .. } = state.metro.status {
                for wt in &mut worktrees {
                    let wt_name = wt.path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("");
                    if wt_name == worktree_id {
                        wt.metro_status = crate::domain::worktree::WorktreeMetroStatus::Running;
                    }
                }
            }

            state.worktrees = worktrees;

            if !state.worktrees.is_empty() {
                // Re-derive selected index from selected_worktree_id (stable across sorts)
                let selected_idx = state
                    .selected_worktree_id
                    .as_ref()
                    .and_then(|id| state.worktrees.iter().position(|wt| &wt.id == id))
                    .unwrap_or(0);
                state.worktree_table_state.select(Some(selected_idx));
                state.active_worktree_path = Some(state.worktrees[selected_idx].path.clone());
            }

            // Phase 4: fetch titles for uncached branches
            if let Some(ref client) = state.jira_client {
                let keys_to_fetch: Vec<(String, String)> = state.worktrees.iter()
                    .filter_map(|wt| {
                        let key = crate::domain::jira::extract_jira_key(&wt.branch, &state.jira_project_prefix)?;
                        if state.jira_title_cache.contains_key(&key) { return None; }
                        Some((wt.branch.clone(), key))
                    })
                    .collect();

                if !keys_to_fetch.is_empty() {
                    let client = std::sync::Arc::clone(client);
                    let tx = metro_tx.clone();
                    tokio::spawn(async move {
                        let mut results = vec![];
                        for (_branch, key) in keys_to_fetch {
                            if let Some(title) = client.fetch_title(&key).await {
                                results.push((key, title));
                            }
                        }
                        if !results.is_empty() {
                            let _ = tx.send(Action::JiraTitlesFetched(results));
                        }
                    });
                }
            }
        }

        Action::RefreshWorktrees => {
            if state.worktree_op_in_flight {
                tracing::debug!("skipping periodic refresh — worktree op in flight");
                return;
            }
            let repo_root = state.repo_root.clone();
            let tx = metro_tx.clone();
            tokio::spawn(async move {
                match crate::infra::worktrees::list_worktrees(&repo_root).await {
                    Ok(wts) => {
                        let _ = tx.send(Action::WorktreesLoaded(wts));
                    }
                    Err(e) => {
                        tracing::warn!("list_worktrees failed: {e}");
                    }
                }
            });
        }

        // --- Phase 3: Command dispatch ---

        Action::CommandRun(spec) => {
            // Clear palette mode whenever a command is dispatched
            state.palette_mode = None;

            // Get selected worktree (needed for all branches)
            let wt_branch = if !state.worktrees.is_empty() {
                let idx = state.worktree_table_state.selected().unwrap_or(0);
                let idx = idx.min(state.worktrees.len() - 1);
                Some((state.worktrees[idx].branch.clone(), state.worktrees[idx].stale))
            } else {
                None
            };

            // Sync-before-run: stale worktree + run command triggers prompt.
            // This MUST run BEFORE the metro start check — yarn install is a
            // dependency of metro, so running metro against stale deps boots it
            // with a broken dependency tree. Also triggers when ONLY pods are
            // stale (yarn fresh, pods out of sync — e.g. after a git checkout
            // that only touched Podfile.lock).
            if let Some((_, yarn_stale)) = &wt_branch
                && matches!(spec, CommandSpec::RnRunAndroid { .. } | CommandSpec::RnRunIos { .. } | CommandSpec::RnRunIosDevice | CommandSpec::RnReleaseBuild) {
                    let is_ios = matches!(spec, CommandSpec::RnRunIos { .. } | CommandSpec::RnRunIosDevice);
                    let pods_stale = if is_ios {
                        let idx = state.worktree_table_state.selected().unwrap_or(0);
                        let wt_path = &state.worktrees[idx.min(state.worktrees.len() - 1)].path;
                        crate::infra::worktrees::check_stale_pods(wt_path)
                    } else {
                        false
                    };

                    if *yarn_stale || pods_stale {
                        if state.config.as_ref().is_some_and(|c| c.auto_sync) {
                            // Auto-sync: skip modal, execute sync+run directly
                            let mut sequence: Vec<CommandSpec> = Vec::new();
                            if *yarn_stale {
                                sequence.push(CommandSpec::YarnInstall);
                            }
                            if pods_stale {
                                sequence.push(CommandSpec::YarnPodInstall);
                            }
                            sequence.push(spec);
                            let first = sequence.remove(0);
                            for cmd in sequence {
                                state.command_queue.push_back(cmd);
                            }
                            state.palette_mode = None;
                            dispatch_command(state, first, metro_tx);
                            return;
                        }
                        state.modal = Some(ModalState::SyncBeforeRun {
                            run_command: Box::new(spec),
                            needs_yarn: *yarn_stale,
                            needs_pods: pods_stale,
                        });
                        state.palette_mode = None;
                        return;
                    }
                }

            // Metro prerequisite: RN run commands need metro running first
            if spec.needs_metro() && !state.metro.is_running() {
                // Store the run command — will be dispatched when metro reports Ready
                state.pending_metro_run = Some(spec);
                update(state, Action::MetroStart, metro_tx, handle_tx);
                return;
            }

            // Pre-processing pipeline
            if spec.is_destructive() {
                let branch_name = wt_branch
                    .map(|(b, _)| b)
                    .unwrap_or_else(|| "(unknown)".to_string());
                state.modal = Some(ModalState::Confirm {
                    prompt: format!("Run '{}' on {}?", spec.label(), branch_name),
                    pending_command: spec,
                });
                return;
            }

            if spec.needs_text_input() {
                let prompt = match &spec {
                    CommandSpec::GitRebase { .. } => "Rebase onto:".to_string(),
                    CommandSpec::GitCheckout { .. } => "Branch to checkout:".to_string(),
                    CommandSpec::GitCheckoutNew { .. } => "New branch name:".to_string(),
                    CommandSpec::YarnJest { .. } => "Jest filter:".to_string(),
                    _ => "Input:".to_string(),
                };
                state.modal = Some(ModalState::TextInput {
                    prompt,
                    buffer: String::new(),
                    pending_template: Box::new(spec),
                });
                return;
            }

            if spec.needs_device_selection() {
                state.pending_device_command = Some(spec.clone());
                let tx = metro_tx.clone();
                let is_android = matches!(spec, CommandSpec::RnRunAndroid { .. });
                tokio::spawn(async move {
                    let devices = if is_android {
                        crate::infra::devices::list_android_devices().await
                    } else {
                        crate::infra::devices::list_ios_simulators().await
                    };
                    match devices {
                        Ok(devs) => {
                            let _ = tx.send(Action::DevicesEnumerated(devs));
                        }
                        Err(e) => {
                            tracing::warn!("device enumeration failed: {e}");
                            let _ = tx.send(Action::DevicesEnumerated(vec![]));
                        }
                    }
                });
                return;
            }

            // Android release build: queue adb install to run after assembleRelease completes
            if matches!(spec, CommandSpec::RnReleaseBuild) {
                state.command_queue.push_back(CommandSpec::AdbInstallApk);
                dispatch_command(state, spec, metro_tx);
                return;
            }

            // GitResetHardFetch: two-step — dispatch fetch, queue reset --hard origin/<branch>
            if matches!(spec, CommandSpec::GitResetHardFetch) {
                state.command_queue.push_back(CommandSpec::GitResetHard);
                dispatch_command(state, CommandSpec::GitFetch, metro_tx);
                return;
            }

            // Normal dispatch
            dispatch_command(state, spec, metro_tx);
        }

        // --- Phase 3: Command output events ---

        Action::CommandOutputLine(line) => {
            if let Some(id) = active_worktree_id(state) {
                let output = state.command_output_by_worktree.entry(id).or_default();
                output.push_back(line);
                if output.len() > MAX_COMMAND_LINES {
                    output.pop_front();
                }
            }
        }

        Action::CommandExited => {
            let completed_cmd = state.running_command.take();
            state.command_task = None;

            // Refresh staleness BEFORE draining the queue so any queued run command
            // re-entering CommandRun sees up-to-date stale state (prevents re-showing
            // the SyncBeforeRun modal after yarn install just ran).
            if let Some(ref cmd) = completed_cmd {
                let refresh = crate::domain::refresh::refresh_needed(cmd);
                if refresh.worktrees {
                    // Full worktree reload (also re-checks staleness and triggers JIRA fetch
                    // via WorktreesLoaded handler when branch names change)
                    let repo_root = state.repo_root.clone();
                    let tx = metro_tx.clone();
                    tokio::spawn(async move {
                        match crate::infra::worktrees::list_worktrees(&repo_root).await {
                            Ok(wts) => { let _ = tx.send(Action::WorktreesLoaded(wts)); }
                            Err(e) => { tracing::warn!("post-command worktree refresh failed: {e}"); }
                        }
                    });
                } else if refresh.staleness {
                    // Staleness refresh: re-check ALL worktrees (cheap I/O, ensures
                    // correct state even if user changed selection during command)
                    for wt in state.worktrees.iter_mut() {
                        wt.stale = crate::infra::worktrees::check_stale(&wt.path);
                        wt.stale_pods = crate::infra::worktrees::check_stale_pods(&wt.path);
                    }
                }
            }

            // Drain command queue — pop_front and dispatch if non-empty.
            // Route through CommandRun (not dispatch_command) for commands that need
            // metro but metro isn't running — so pending_metro_run + MetroStart fires.
            // This matters for the sync-before-run flow: after yarn install, the queued
            // run_command needs metro to be started before dispatching.
            if let Some(next_spec) = state.command_queue.pop_front() {
                if next_spec.needs_metro() && !state.metro.is_running() {
                    update(state, Action::CommandRun(next_spec), metro_tx, handle_tx);
                } else {
                    dispatch_command(state, next_spec, metro_tx);
                }
            } else if state.pending_metro_after_sync {
                // Sync commands finished — start metro in the (already switched) worktree
                state.pending_metro_after_sync = false;
                update(state, Action::MetroStart, metro_tx, handle_tx);
            }
        }

        Action::CommandOutputClear => {
            if let Some(id) = active_worktree_id(state) {
                state.command_output_by_worktree.remove(&id);
                state.command_output_scroll_by_worktree.remove(&id);
            }
        }

        Action::CommandCancel => {
            if let Some(task) = state.command_task.take() {
                task.abort();
            }
            state.running_command = None;
            // Also clear pending queue items — cancel is all-or-nothing
            state.command_queue.clear();
            state.pending_metro_after_sync = false;
            if let Some(id) = active_worktree_id(state) {
                let output = state.command_output_by_worktree.entry(id).or_default();
                output.push_back("[cancelled]".into());
                if output.len() > MAX_COMMAND_LINES {
                    output.pop_front();
                }
            }
        }

        // --- Phase 5.1: Command queue actions ---

        Action::CommandQueuePush(spec) => {
            state.command_queue.push_back(spec);
        }

        Action::CommandQueueClear => {
            state.command_queue.clear();
        }

        // --- Phase 3: Modal actions ---

        Action::ShowCommandPalette => {
            // Palette activation is handled via EnterGitPalette / EnterRnPalette.
            // This variant is kept for backward compatibility.
        }

        Action::ModalConfirm => {
            // Check for pending worktree removal BEFORE falling through to normal confirm
            if let Some((wt_id, wt_path, _branch)) = state.pending_worktree_removal.take() {
                state.modal = None;

                // Stop metro if it's running on the worktree being removed
                if state.metro.is_running()
                    && state.active_worktree_path.as_ref() == Some(&wt_path) {
                        update(state, Action::MetroStop, metro_tx, handle_tx);
                    }

                // Clean up per-worktree dashboard state
                state.command_output_by_worktree.remove(&wt_id);
                state.command_output_scroll_by_worktree.remove(&wt_id);

                // Immediately remove from worktree list for instant visual feedback
                state.worktrees.retain(|wt| wt.id != wt_id);
                if state.worktrees.is_empty() {
                    state.worktree_table_state.select(None);
                    state.selected_worktree_id = None;
                    state.active_worktree_path = None;
                } else {
                    let idx = state.worktree_table_state.selected().unwrap_or(0)
                        .min(state.worktrees.len() - 1);
                    state.worktree_table_state.select(Some(idx));
                    state.selected_worktree_id = Some(state.worktrees[idx].id.clone());
                    state.active_worktree_path = Some(state.worktrees[idx].path.clone());
                }

                // Spawn async removal task
                state.worktree_op_in_flight = true;
                let repo_root = state.repo_root.clone();
                let tx = metro_tx.clone();
                let path_str = wt_path.to_string_lossy().to_string();
                tokio::spawn(async move {
                    match crate::infra::worktrees::remove_worktree(&repo_root, &wt_path).await {
                        Ok(()) => {
                            let _ = tx.send(Action::WorktreeRemoved(path_str));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeRemoveFailed(e.to_string()));
                        }
                    }
                });
                return;
            }

            if let Some(ModalState::Confirm { pending_command, .. }) = state.modal.take() {
                // Dispatch directly — skip pre-processing (already confirmed)
                dispatch_command(state, pending_command, metro_tx);
            }
        }

        Action::ModalCancel => {
            state.modal = None;
            state.palette_mode = None;
            state.pending_claude_open = None;       // prevent pending state leak on Esc
            state.pending_android_mode = false;
            state.pending_worktree_removal = None;  // discard any pending removal on cancel
            state.pending_worktree_add = false;     // discard any pending add on cancel
            state.pending_new_branch_base = None;   // discard new-branch base on cancel
            state.pending_new_branch_worktree = false;
        }

        Action::ModalInputChar(c) => {
            match state.modal.as_mut() {
                Some(ModalState::TextInput { buffer, .. }) => {
                    buffer.push(c);
                }
                Some(ModalState::DevicePicker { filter, selected, .. }) => {
                    filter.push(c);
                    *selected = 0; // reset selection when filter changes
                }
                _ => {}
            }
        }

        Action::ModalInputBackspace => {
            match state.modal.as_mut() {
                Some(ModalState::TextInput { buffer, .. }) => {
                    buffer.pop();
                }
                Some(ModalState::DevicePicker { filter, selected, .. }) => {
                    filter.pop();
                    *selected = 0; // reset selection when filter changes
                }
                _ => {}
            }
        }

        Action::ModalInputSubmit => {
            if let Some(modal) = state.modal.take() {
                match modal {
                    ModalState::TextInput {
                        buffer,
                        pending_template,
                        ..
                    } => {
                        if state.pending_android_mode {
                            state.pending_android_mode = false;
                            let mode = if buffer.trim().is_empty() { None } else { Some(buffer.trim().to_string()) };
                            state.android_mode = mode.clone();
                            if let Some(ref m) = mode {
                                let _ = crate::infra::android_prefs::save_android_mode(m);
                            }
                        } else if state.pending_new_branch_worktree {
                            state.pending_new_branch_worktree = false;
                            let new_branch_name = buffer.trim().to_string();
                            let base_branch = state.pending_new_branch_base.take();
                            if new_branch_name.is_empty() {
                                return;
                            }
                            let base_branch = match base_branch {
                                Some(b) => b,
                                None => return,
                            };
                            let repo_root = state.repo_root.clone();
                            let tx = metro_tx.clone();
                            state.worktree_op_in_flight = true;
                            tokio::spawn(async move {
                                match crate::infra::worktrees::add_worktree_new_branch(&repo_root, &new_branch_name, &base_branch).await {
                                    Ok(path) => {
                                        let _ = tx.send(Action::WorktreeNewBranchCreated(path.to_string_lossy().to_string()));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::WorktreeNewBranchFailed(e.to_string()));
                                    }
                                }
                            });
                        } else if state.pending_worktree_add {
                            state.pending_worktree_add = false;
                            let branch_name = buffer.trim().to_string();
                            if branch_name.is_empty() {
                                return;
                            }
                            let repo_root = state.repo_root.clone();
                            let tx = metro_tx.clone();
                            state.worktree_op_in_flight = true;
                            tokio::spawn(async move {
                                match crate::infra::worktrees::add_worktree(&repo_root, &branch_name).await {
                                    Ok(path) => {
                                        let _ = tx.send(Action::WorktreeAdded(path.to_string_lossy().to_string()));
                                    }
                                    Err(e) => {
                                        let _ = tx.send(Action::WorktreeAddFailed(e.to_string()));
                                    }
                                }
                            });
                        } else if let Some(wt_id) = state.pending_claude_open.take() {
                            // Claude tab name modal submit
                            let suffix = if buffer.trim().is_empty() {
                                "claude".to_string()
                            } else {
                                buffer
                            };
                            let wt = state.worktrees.iter()
                                .find(|wt| wt.path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("") == wt_id)
                                .cloned();
                            if let Some(wt) = wt {
                                let name = format!("{}-{}", wt.preferred_prefix(), suffix);
                                let path = wt.path.clone();
                                let flags = state.claude_flags.clone();
                                let command = if flags.is_empty() {
                                    "claude".to_string()
                                } else {
                                    format!("claude {flags}")
                                };
                                tokio::task::spawn_blocking(move || {
                                    if let Some(mux) = crate::infra::multiplexer::detect_multiplexer()
                                        && let Err(e) = mux.new_window(&path, &name, &command) {
                                            tracing::warn!("multiplexer new_window (claude) failed: {e}");
                                        }
                                });
                            }
                        } else {
                            // Build the real CommandSpec by filling in the text
                            let real_spec = match *pending_template {
                                CommandSpec::GitRebase { .. } => {
                                    CommandSpec::GitRebase { target: buffer }
                                }
                                CommandSpec::GitCheckout { .. } => {
                                    CommandSpec::GitCheckout { branch: buffer }
                                }
                                CommandSpec::GitCheckoutNew { .. } => {
                                    CommandSpec::GitCheckoutNew { branch: buffer }
                                }
                                CommandSpec::YarnJest { .. } => {
                                    CommandSpec::YarnJest { filter: buffer }
                                }
                                CommandSpec::ShellCommand { .. } => {
                                    CommandSpec::ShellCommand { command: buffer }
                                }
                                other => other,
                            };
                            dispatch_command(state, real_spec, metro_tx);
                        }
                    }
                    other => {
                        // Restore modal if wrong type (shouldn't happen)
                        state.modal = Some(other);
                    }
                }
            }
        }

        Action::ModalDeviceNext => {
            if let Some(ModalState::DevicePicker {
                ref devices,
                ref mut selected,
                ref filter,
                ..
            }) = state.modal
            {
                let count = if filter.is_empty() {
                    devices.len()
                } else {
                    let lower = filter.to_lowercase();
                    devices.iter().filter(|d| d.name.to_lowercase().contains(&lower)).count()
                };
                if count > 0 {
                    *selected = if *selected >= count - 1 { 0 } else { *selected + 1 };
                }
            }
        }

        Action::ModalDevicePrev => {
            if let Some(ModalState::DevicePicker {
                ref devices,
                ref mut selected,
                ref filter,
                ..
            }) = state.modal
            {
                let count = if filter.is_empty() {
                    devices.len()
                } else {
                    let lower = filter.to_lowercase();
                    devices.iter().filter(|d| d.name.to_lowercase().contains(&lower)).count()
                };
                if count > 0 {
                    *selected = if *selected == 0 { count - 1 } else { *selected - 1 };
                }
            }
        }

        Action::ModalDeviceConfirm => {
            if let Some(ModalState::DevicePicker {
                devices,
                selected,
                pending_template,
                filter,
            }) = state.modal.take()
            {
                // Apply filter to get the actual visible list (mirrors render logic)
                let filtered: Vec<&crate::domain::command::DeviceInfo> = if filter.is_empty() {
                    devices.iter().collect()
                } else {
                    let lower = filter.to_lowercase();
                    devices.iter().filter(|d| d.name.to_lowercase().contains(&lower)).collect()
                };
                if let Some(device) = filtered.get(selected) {
                    let device_id = device.id.clone();
                    let device_name = device.name.clone();
                    let is_ios = matches!(pending_template.as_ref(), CommandSpec::RnRunIos { .. });
                    let is_available_emulator = device_name.ends_with("(available)");

                    // Available emulator: boot it, then run via shell command
                    if is_available_emulator {
                        if let CommandSpec::RnRunAndroid { mode, .. } = *pending_template {
                            if let Some(ref m) = mode {
                                let _ = crate::infra::android_prefs::save_android_mode(m);
                            }
                            let mode_flag = mode.map(|m| format!(" --mode {m}")).unwrap_or_default();
                            let cmd = format!(
                                "emulator -avd {device_id} > /dev/null 2>&1 & adb wait-for-device && npx react-native run-android{mode_flag}"
                            );
                            dispatch_command(state, CommandSpec::ShellCommand { command: cmd }, metro_tx);
                        }
                        return;
                    }

                    let real_spec = match *pending_template {
                        CommandSpec::RnRunAndroid { mode, .. } => CommandSpec::RnRunAndroid {
                            device_id: device_id.clone(),
                            mode,
                        },
                        CommandSpec::RnRunIos { .. } => CommandSpec::RnRunIos {
                            device_id: device_id.clone(),
                        },
                        other => other,
                    };
                    // Persist Android mode if present
                    if let CommandSpec::RnRunAndroid { mode: Some(ref m), .. } = real_spec {
                        let _ = crate::infra::android_prefs::save_android_mode(m);
                    }
                    // Record iOS simulator usage for sort-by-recent
                    if is_ios {
                        let _ = metro_tx.send(Action::SimulatorUsed(device_id));
                    }
                    dispatch_command(state, real_spec, metro_tx);
                }
            }
        }

        // --- Phase 3: Device enumeration (async callback) ---

        Action::DevicesEnumerated(devices) => {
            if let Some(spec) = state.pending_device_command.take() {
                match devices.len() {
                    0 => {
                        if let Some(id) = active_worktree_id(state) {
                            let output = state.command_output_by_worktree.entry(id).or_default();
                            output.push_back("[error] no devices found".into());
                        }
                    }
                    1 => {
                        // Only one device — skip picker
                        let is_available_emulator = devices[0].name.ends_with("(available)");

                        // Available emulator: boot it, then run via shell command
                        if is_available_emulator {
                            if let CommandSpec::RnRunAndroid { mode, .. } = spec {
                                if let Some(ref m) = mode {
                                    let _ = crate::infra::android_prefs::save_android_mode(m);
                                }
                                let mode_flag = mode.map(|m| format!(" --mode {m}")).unwrap_or_default();
                                let cmd = format!(
                                    "emulator -avd {} > /dev/null 2>&1 & adb wait-for-device && npx react-native run-android{}",
                                    devices[0].id, mode_flag
                                );
                                dispatch_command(state, CommandSpec::ShellCommand { command: cmd }, metro_tx);
                            }
                        } else {
                            let real_spec = match spec {
                                CommandSpec::RnRunAndroid { mode, .. } => CommandSpec::RnRunAndroid {
                                    device_id: devices[0].id.clone(),
                                    mode,
                                },
                                CommandSpec::RnRunIos { .. } => CommandSpec::RnRunIos {
                                    device_id: devices[0].id.clone(),
                                },
                                other => other,
                            };
                            if let CommandSpec::RnRunAndroid { mode: Some(ref m), .. } = real_spec {
                                let _ = crate::infra::android_prefs::save_android_mode(m);
                            }
                            dispatch_command(state, real_spec, metro_tx);
                        }
                    }
                    _ => {
                        // Multiple devices — show picker
                        // Sort iOS simulators by last-used from sim_history
                        let mut sorted_devices = devices;
                        if matches!(spec, CommandSpec::RnRunIos { .. }) {
                            let history = crate::infra::sim_history::load_sim_history();
                            sorted_devices.sort_by_key(|d| {
                                history.iter().position(|h| h == &d.id)
                                    .unwrap_or(usize::MAX)
                            });
                        }
                        state.modal = Some(ModalState::DevicePicker {
                            devices: sorted_devices,
                            selected: 0,
                            pending_template: Box::new(spec),
                            filter: String::new(),
                        });
                    }
                }
            }
        }

        // --- Phase 3: Palette mode activation ---

        Action::EnterGitPalette => {
            state.palette_mode = Some(PaletteMode::Git);
        }

        Action::EnterRnPalette => {
            // EnterRnPalette kept for backward compat — Phase 05.1 will remap 'c' key
            // to new submenu scheme. For now we just cancel palette mode.
            state.palette_mode = None;
        }



        // --- Phase 5: Worktree switching and Claude Code ---

        Action::WorktreeSwitchToSelected => {
            let selected_idx = state.worktree_table_state.selected().unwrap_or(0);
            // Capture target path NOW — navigation may change active_worktree_path later
            let target_path = state.worktrees
                .get(selected_idx)
                .map(|wt| wt.path.clone());

            // Stale dependency check — metro only needs yarn, not pods
            if let Some(wt) = state.worktrees.get(selected_idx)
                && wt.stale
            {
                if state.config.as_ref().is_some_and(|c| c.auto_sync) {
                    // Auto-sync: skip modal, execute yarn install + metro directly
                    if let Some(path) = target_path {
                        state.active_worktree_path = Some(path);
                    }
                    if state.metro.is_running() {
                        state.pending_restart = false;
                        update(state, Action::MetroStop, metro_tx, handle_tx);
                    }
                    state.pending_metro_after_sync = true;
                    dispatch_command(state, CommandSpec::YarnInstall, metro_tx);
                    return;
                }
                // Store target path for use after sync completes
                state.pending_switch_path = target_path;
                state.modal = Some(ModalState::SyncBeforeMetro { needs_yarn: true, needs_pods: false });
                return;
            }

            // Original logic (unchanged) — only reached when deps are fresh
            if state.metro.is_running() {
                // Kill current → wait for port free → start in new worktree
                state.pending_switch_path = target_path;
                state.pending_restart = true;
                update(state, Action::MetroStop, metro_tx, handle_tx);
            } else {
                // Not running — just start directly in selected worktree
                if let Some(path) = target_path {
                    state.active_worktree_path = Some(path);
                }
                update(state, Action::MetroStart, metro_tx, handle_tx);
            }
        }

        Action::OpenClaudeCode => {
            if state.multiplexer.is_none() {
                state.error_state = Some(ErrorState {
                    message: "Cannot open Claude Code: not inside a tmux or zellij session".into(),
                    can_retry: false,
                });
                return;
            }
            let wt = if !state.worktrees.is_empty() {
                let idx = state.worktree_table_state.selected().unwrap_or(0)
                    .min(state.worktrees.len() - 1);
                &state.worktrees[idx]
            } else {
                return;
            };
            // Store worktree dir name for later use when modal submits
            state.pending_claude_open = Some(
                wt.path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string()
            );
            state.modal = Some(ModalState::TextInput {
                prompt: "Claude tab suffix:".to_string(),
                buffer: String::new(),
                pending_template: Box::new(crate::domain::command::CommandSpec::YarnLint), // sentinel — not used
            });
        }

        Action::OpenShellTab => {
            if state.multiplexer.is_none() {
                state.error_state = Some(ErrorState {
                    message: "Cannot open shell tab: not inside a tmux or zellij session".into(),
                    can_retry: false,
                });
                return;
            }
            let wt = if !state.worktrees.is_empty() {
                let idx = state.worktree_table_state.selected().unwrap_or(0)
                    .min(state.worktrees.len() - 1);
                state.worktrees[idx].clone()
            } else {
                return;
            };
            let path = wt.path.clone();
            let name = format!("{}-shell", wt.preferred_prefix());
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
            tokio::task::spawn_blocking(move || {
                if let Some(mux) = crate::infra::multiplexer::detect_multiplexer()
                    && let Err(e) = mux.new_window(&path, &name, &shell) {
                        tracing::warn!("multiplexer new_window (shell) failed: {e}");
                    }
            });
        }

        // --- Phase 4: JIRA title fetch results ---

        Action::JiraTitlesFetched(titles) => {
            // Update in-memory cache
            for (key, title) in &titles {
                state.jira_title_cache.insert(key.clone(), title.clone());
            }
            // Persist cache to disk (fire-and-forget, log on error)
            if let Err(e) = crate::infra::jira_cache::save_jira_cache(&state.jira_title_cache) {
                tracing::warn!("save_jira_cache failed: {e}");
            }
            // Apply titles to currently loaded worktrees
            for wt in &mut state.worktrees {
                if let Some(key) = crate::domain::jira::extract_jira_key(&wt.branch, &state.jira_project_prefix)
                    && let Some(title) = state.jira_title_cache.get(&key) {
                        wt.jira_title = Some(title.clone());
                    }
            }
        }

        // --- Phase 5.1: New submenu and action stubs ---

        Action::EnterAndroidPalette => {
            state.palette_mode = Some(PaletteMode::Android);
        }
        Action::EnterIosPalette => {
            state.palette_mode = Some(PaletteMode::Ios);
        }
        Action::EnterYarnPalette => {
            state.palette_mode = Some(PaletteMode::Yarn);
        }
        Action::EnterWorktreePalette => {
            state.palette_mode = Some(PaletteMode::Worktree);
        }
        Action::OpenCleanMenu => {
            state.palette_mode = None;
            state.modal = Some(ModalState::CleanToggle { options: CleanOptions::default() });
        }
        Action::CleanToggleNodeModules => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal {
                options.node_modules = !options.node_modules;
            }
        }
        Action::CleanTogglePods => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal {
                options.pods = !options.pods;
            }
        }
        Action::CleanToggleAndroid => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal {
                options.android = !options.android;
            }
        }
        Action::CleanToggleSyncAfter => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal {
                options.sync_after = !options.sync_after;
            }
        }
        Action::CleanConfirm => {
            if let Some(ModalState::CleanToggle { options }) = state.modal.take() {
                state.palette_mode = None;

                // Build command sequence from checked options.
                // Order matters: react-native clean first, node_modules last —
                // removing node_modules before `react-native clean` breaks the
                // RN clean scripts that depend on packages under node_modules.
                let mut cmds: Vec<CommandSpec> = Vec::new();
                if options.pods {
                    cmds.push(CommandSpec::RnCleanCocoapods);
                }
                if options.android {
                    cmds.push(CommandSpec::RnCleanAndroid);
                }
                if options.node_modules {
                    cmds.push(CommandSpec::RmNodeModules);
                }
                if options.sync_after {
                    cmds.push(CommandSpec::YarnInstall);
                    cmds.push(CommandSpec::YarnPodInstall);
                }

                if cmds.is_empty() {
                    return;
                }

                // Dispatch first, queue rest
                let first = cmds.remove(0);
                for cmd in cmds {
                    state.command_queue.push_back(cmd);
                }
                dispatch_command(state, first, metro_tx);
            }
        }
        Action::ToggleFullscreen => {
            if state.fullscreen_panel.is_some() {
                state.fullscreen_panel = None;
                state.focused_panel = state.focused_panel.next();
            } else {
                // Only CommandOutput can be fullscreened
                if state.focused_panel == FocusedPanel::CommandOutput {
                    state.fullscreen_panel = Some(state.focused_panel);
                }
            }
        }
        Action::StartShellCommand => {
            state.modal = Some(ModalState::TextInput {
                prompt: "Shell command:".to_string(),
                buffer: String::new(),
                pending_template: Box::new(CommandSpec::ShellCommand { command: String::new() }),
            });
        }
        Action::StartSetAndroidMode => {
            state.palette_mode = None;
            state.pending_android_mode = true;
            state.modal = Some(ModalState::TextInput {
                prompt: "Android build mode:".to_string(),
                buffer: state.android_mode.clone().unwrap_or_default(),
                pending_template: Box::new(CommandSpec::YarnLint), // sentinel — not actually used
            });
        }
        Action::SimulatorUsed(udid) => {
            // Fire-and-forget write to sim history
            tokio::task::spawn_blocking(move || {
                if let Err(e) = crate::infra::sim_history::record_sim_used(&udid) {
                    tracing::warn!("failed to save sim history: {e}");
                }
            });
        }
        Action::SyncBeforeRunAccept => {
            if let Some(ModalState::SyncBeforeRun { run_command, needs_yarn, needs_pods }) = state.modal.take() {
                // Build sequence: [yarn install?, pod install?, run_command]
                // Dispatch the first, queue the rest.
                let mut sequence: Vec<CommandSpec> = Vec::new();
                if needs_yarn {
                    sequence.push(CommandSpec::YarnInstall);
                }
                if needs_pods {
                    sequence.push(CommandSpec::YarnPodInstall);
                }
                sequence.push(*run_command);

                // Guaranteed non-empty: we only get here from the modal which only
                // appears when needs_yarn || needs_pods, so sequence has ≥2 elements.
                let first = sequence.remove(0);
                for cmd in sequence {
                    state.command_queue.push_back(cmd);
                }
                dispatch_command(state, first, metro_tx);
            }
        }
        Action::SyncBeforeRunDecline => {
            if let Some(ModalState::SyncBeforeRun { run_command, .. }) = state.modal.take() {
                // Skip sync. Since the stale check now runs before the metro check,
                // metro may still need to be started. Route through CommandRun so the
                // metro auto-start via pending_metro_run fires. The stale check won't
                // re-trigger because the user just declined it.
                let spec = *run_command;
                if spec.needs_metro() && !state.metro.is_running() {
                    state.pending_metro_run = Some(spec);
                    update(state, Action::MetroStart, metro_tx, handle_tx);
                } else {
                    dispatch_command(state, spec, metro_tx);
                }
            }
        }

        Action::SyncBeforeMetroAccept => {
            if let Some(ModalState::SyncBeforeMetro { needs_yarn, needs_pods }) = state.modal.take() {
                // Switch active worktree to the target (consume pending_switch_path set in stale check)
                if let Some(path) = state.pending_switch_path.take() {
                    state.active_worktree_path = Some(path);
                }

                // Stop metro if running (no auto-restart — sync must finish first)
                if state.metro.is_running() {
                    state.pending_restart = false;
                    update(state, Action::MetroStop, metro_tx, handle_tx);
                }

                // Build sync sequence and dispatch
                let mut sequence: Vec<CommandSpec> = Vec::new();
                if needs_yarn {
                    sequence.push(CommandSpec::YarnInstall);
                }
                if needs_pods {
                    sequence.push(CommandSpec::YarnPodInstall);
                }

                // Flag: after sync queue drains, start metro
                state.pending_metro_after_sync = true;

                let first = sequence.remove(0);
                for cmd in sequence {
                    state.command_queue.push_back(cmd);
                }
                dispatch_command(state, first, metro_tx);
            }
        }

        Action::SyncBeforeMetroDecline => {
            if let Some(ModalState::SyncBeforeMetro { .. }) = state.modal.take() {
                // Consume pending_switch_path and proceed with original worktree switch logic
                let target_path = state.pending_switch_path.take();

                if state.metro.is_running() {
                    state.pending_switch_path = target_path;
                    state.pending_restart = true;
                    update(state, Action::MetroStop, metro_tx, handle_tx);
                } else {
                    if let Some(path) = target_path {
                        state.active_worktree_path = Some(path);
                    }
                    update(state, Action::MetroStart, metro_tx, handle_tx);
                }
            }
        }

        // --- Phase 5.2: Universal scroll ---

        Action::ScrollToTop => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state) {
                    state.command_output_scroll_by_worktree.insert(id, 0);
                }
        }

        Action::ScrollToBottom => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state) {
                    let max = state.command_output_by_worktree
                        .get(&id)
                        .map(|o| o.len())
                        .unwrap_or(0);
                    state.command_output_scroll_by_worktree.insert(id, max);
                }
        }

        Action::SetPendingG => {
            state.pending_g = true;
        }

        Action::CommandOutputScrollUp => {
            if let Some(id) = active_worktree_id(state) {
                let scroll = state.command_output_scroll_by_worktree.entry(id).or_insert(0);
                *scroll = scroll.saturating_sub(1);
            }
        }

        Action::CommandOutputScrollDown => {
            let max = active_output(state).len();
            if let Some(id) = active_worktree_id(state) {
                let scroll = state.command_output_scroll_by_worktree.entry(id).or_insert(0);
                if *scroll < max {
                    *scroll += 1;
                }
            }
        }

        // --- Quick-2: Worktree removal ---

        Action::WorktreeRemove => {
            if state.worktrees.is_empty() {
                return;
            }
            let idx = state.worktree_table_state.selected().unwrap_or(0)
                .min(state.worktrees.len() - 1);
            let wt = state.worktrees[idx].clone();

            // Guard: cannot remove the main worktree (its path equals repo_root)
            if wt.path == state.repo_root {
                state.error_state = Some(ErrorState {
                    message: "Cannot remove the main worktree".into(),
                    can_retry: false,
                });
                state.palette_mode = None;
                return;
            }

            // Store removal target so ModalConfirm knows what to do
            state.pending_worktree_removal = Some((wt.id.clone(), wt.path.clone(), wt.branch.clone()));

            // Build confirm prompt — mention metro if it will be stopped
            let metro_note = if state.metro.is_running()
                && state.active_worktree_path.as_ref() == Some(&wt.path)
            {
                " (metro will be stopped)"
            } else {
                ""
            };
            let prompt = format!("Remove worktree '{}' and delete directory?{}", wt.branch, metro_note);

            // Use a sentinel CommandSpec for the confirm modal — the actual removal
            // logic is in ModalConfirm when pending_worktree_removal is Some.
            state.modal = Some(ModalState::Confirm {
                prompt,
                pending_command: crate::domain::command::CommandSpec::GitPull, // sentinel
            });
            state.palette_mode = None;
        }

        Action::WorktreeRemoved(path_str) => {
            tracing::info!("worktree removed: {}", path_str);
            state.worktree_op_in_flight = false;
            // Refresh the worktree list to reflect the removal
            let repo_root = state.repo_root.clone();
            let tx = metro_tx.clone();
            tokio::spawn(async move {
                match crate::infra::worktrees::list_worktrees(&repo_root).await {
                    Ok(wts) => {
                        let _ = tx.send(Action::WorktreesLoaded(wts));
                    }
                    Err(e) => {
                        tracing::warn!("worktree refresh after removal failed: {e}");
                    }
                }
            });
        }

        Action::WorktreeRemoveFailed(err) => {
            state.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to remove worktree: {err}"),
                can_retry: false,
            });
            // Re-add the worktree to the UI since git removal failed
            update(state, Action::RefreshWorktrees, metro_tx, handle_tx);
        }

        // --- Quick-260403-dmz: Worktree creation ---

        Action::WorktreeAdd => {
            state.palette_mode = None;
            state.pending_worktree_add = true;
            state.modal = Some(ModalState::TextInput {
                prompt: "New worktree branch name:".to_string(),
                buffer: String::new(),
                pending_template: Box::new(crate::domain::command::CommandSpec::GitPull), // sentinel — not used
            });
        }

        Action::WorktreeAdded(path_str) => {
            tracing::info!("worktree added: {}", path_str);
            state.worktree_op_in_flight = false;
            // Refresh the worktree list to show the new worktree
            let repo_root = state.repo_root.clone();
            let tx = metro_tx.clone();
            tokio::spawn(async move {
                match crate::infra::worktrees::list_worktrees(&repo_root).await {
                    Ok(wts) => {
                        let _ = tx.send(Action::WorktreesLoaded(wts));
                    }
                    Err(e) => {
                        tracing::warn!("worktree refresh after add failed: {e}");
                    }
                }
            });
        }

        Action::WorktreeAddFailed(err) => {
            state.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to create worktree: {err}"),
                can_retry: false,
            });
        }

        // Phase 08-02: New-branch worktree creation flow

        Action::WorktreeAddNewBranch => {
            state.palette_mode = None;
            let repo_root = state.repo_root.clone();
            let tx = metro_tx.clone();
            tokio::spawn(async move {
                match crate::infra::worktrees::list_remote_branches(&repo_root).await {
                    Ok(branches) => {
                        let _ = tx.send(Action::BranchesLoaded(branches));
                    }
                    Err(e) => {
                        let _ = tx.send(Action::WorktreeNewBranchFailed(e.to_string()));
                    }
                }
            });
        }

        Action::BranchesLoaded(branches) => {
            state.modal = Some(ModalState::BranchPicker {
                branches,
                selected: 0,
                filter: String::new(),
            });
        }

        Action::BranchPickerNext => {
            if let Some(ModalState::BranchPicker {
                ref branches,
                ref mut selected,
                ref filter,
            }) = state.modal
            {
                let count = if filter.is_empty() {
                    branches.len()
                } else {
                    let lower = filter.to_lowercase();
                    branches.iter().filter(|b| b.to_lowercase().contains(&lower)).count()
                };
                if count > 0 {
                    *selected = if *selected >= count - 1 { 0 } else { *selected + 1 };
                }
            }
        }

        Action::BranchPickerPrev => {
            if let Some(ModalState::BranchPicker {
                ref branches,
                ref mut selected,
                ref filter,
            }) = state.modal
            {
                let count = if filter.is_empty() {
                    branches.len()
                } else {
                    let lower = filter.to_lowercase();
                    branches.iter().filter(|b| b.to_lowercase().contains(&lower)).count()
                };
                if count > 0 {
                    *selected = if *selected == 0 { count - 1 } else { *selected - 1 };
                }
            }
        }

        Action::BranchPickerFilter(c) => {
            if let Some(ModalState::BranchPicker {
                ref mut filter,
                ref mut selected,
                ..
            }) = state.modal
            {
                filter.push(c);
                *selected = 0;
            }
        }

        Action::BranchPickerBackspace => {
            if let Some(ModalState::BranchPicker {
                ref mut filter,
                ref mut selected,
                ..
            }) = state.modal
            {
                filter.pop();
                *selected = 0;
            }
        }

        Action::BranchPickerConfirm => {
            if let Some(ModalState::BranchPicker {
                branches,
                selected,
                filter,
            }) = state.modal.take()
            {
                // Apply filter to get visible list
                let filtered: Vec<&String> = if filter.is_empty() {
                    branches.iter().collect()
                } else {
                    let lower = filter.to_lowercase();
                    branches.iter().filter(|b| b.to_lowercase().contains(&lower)).collect()
                };
                if let Some(base_branch) = filtered.get(selected) {
                    state.pending_new_branch_base = Some((*base_branch).clone());
                    state.pending_new_branch_worktree = true;
                    state.modal = Some(ModalState::TextInput {
                        prompt: "New branch name:".to_string(),
                        buffer: String::new(),
                        pending_template: Box::new(CommandSpec::GitPull), // sentinel — not used
                    });
                }
            }
        }

        Action::WorktreeNewBranchCreated(path_str) => {
            tracing::info!("worktree with new branch created: {}", path_str);
            state.worktree_op_in_flight = false;
            let repo_root = state.repo_root.clone();
            let tx = metro_tx.clone();
            tokio::spawn(async move {
                match crate::infra::worktrees::list_worktrees(&repo_root).await {
                    Ok(wts) => {
                        let _ = tx.send(Action::WorktreesLoaded(wts));
                    }
                    Err(e) => {
                        tracing::warn!("worktree refresh after new-branch add failed: {e}");
                    }
                }
            });
        }

        Action::WorktreeNewBranchFailed(err) => {
            state.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to create worktree with new branch: {err}"),
                can_retry: false,
            });
        }
    }
}
