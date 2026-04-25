//! TEA reducer — `pub fn update` is the ONLY place `AppState` is mutated.
//!
//! Plan 13-07 (F-201 consumer): `update()` is now a PURE function —
//! `pub fn update(state: &mut AppState, action: Action) -> Vec<Effect>`.
//! Every side-effect that used to be an inline async spawn is now an
//! `Effect` variant pushed onto the returned `Vec<Effect>`. The event loop
//! (`src/app/runtime.rs`) calls `runner.run_effects(effects).await` after
//! every invocation.
//!
//! Grep invariants (enforced by `make arch-lint` G-04 / G-05):
//! - no async spawn primitives in this file (G-04)
//! - no HTTP client or process-launch imports anywhere under src/app/ (G-05)

use super::effect::Effect;
use super::state::{active_output, active_worktree_id, AppState, ErrorState, FocusedPanel, PaletteMode, MAX_COMMAND_LINES};
use crate::domain::action::Action;
use crate::domain::command::{CleanOptions, CommandSpec, ModalState};
use crate::domain::pipeline::{DependencyState, Recipe};
use std::path::PathBuf;

// Plan 13-09 (F-204 consumer): the 11 inline prereq sites are rewritten to
// build a `Recipe` and call `Recipe::expand(&deps)` — the dispatcher never
// inlines prereq ordering. The boolean coordination flags
// `pending_metro_run`, `pending_metro_after_sync`, and `pending_switch_path`
// are deleted; their semantics are absorbed into the command_queue front-push
// pattern (deferred run waits for metro Ready), `post_drain_action` (post-
// queue-drain action), and direct `active_worktree_path` updates respectively.
//
// Site → Recipe variant mapping:
//   SyncBeforeRunAccept (auto-sync fast path + modal accept) → Recipe::SyncThenRun
//   SyncBeforeMetroAccept (modal accept) + WorktreeSwitch auto-sync → Recipe::SyncThenStartMetro
//   CleanConfirm                                                    → Recipe::Clean
//   RnReleaseBuild dispatch                                         → Recipe::ReleaseBuildAndInstall
//   GitResetHardFetch dispatch                                      → Recipe::GitFetchThenReset
//   needs_metro pre-dispatch (3 sites: CommandRun / CommandExited drain / SyncBeforeRunDecline)
//                                                                   → command_queue.push_front + MetroStart

/// Directly dispatches a command without going through the pre-processing pipeline.
/// Used by ModalConfirm to run confirmed destructive commands, and internally after
/// text-input and device-picker modals complete.
///
/// Appends separator to per-worktree output, sets running_command, and returns the
/// Effect::SpawnCommand that the effect_runner will consume.
fn dispatch_command(state: &mut AppState, spec: CommandSpec) -> Option<Effect> {
    let wt = if !state.worktree_browser.worktrees.is_empty() {
        let idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0);
        let idx = idx.min(state.worktree_browser.worktrees.len() - 1);
        state.worktree_browser.worktrees[idx].clone()
    } else {
        // No worktrees loaded yet — can't dispatch; log to a fallback message (no per-worktree key)
        tracing::warn!("dispatch_command: no worktree selected, dropping command {:?}", spec.label());
        return None;
    };

    // Append a separator line to per-worktree output — output persists, not cleared on new command
    let wt_id = wt.id.clone();
    let output = state.command_runner.command_output_by_worktree.entry(wt_id.clone()).or_default();
    output.push_back(format!("$ {}", spec.to_argv().join(" ")));
    // Cap per-worktree output at MAX_COMMAND_LINES
    while output.len() > MAX_COMMAND_LINES {
        output.pop_front();
    }
    // Reset scroll for this worktree to show the latest output
    state.command_runner.command_output_scroll_by_worktree.insert(wt_id, 0);

    state.command_runner.running_command = Some(spec.clone());

    // Abort any existing command task
    if let Some(task) = state.command_runner.command_task.take() {
        task.abort();
    }

    // F-201: return Effect::SpawnCommand — effect_runner owns the actual
    // async task + CommandEvent -> Action translation.
    Some(Effect::SpawnCommand {
        spec,
        cwd: wt.path.clone(),
        branch: wt.branch.clone(),
    })
}

/// TEA update function — the ONLY place AppState is mutated.
///
/// Post-F-201 (Plan 13-07) signature: pure `(state, action) -> Vec<Effect>`.
/// Every async dispatch / channel send / recursive self-dispatch is
/// expressed as an Effect variant pushed onto the returned vec. The event
/// loop interprets effects via `EffectRunner::run_effects`.
pub fn update(state: &mut AppState, action: Action) -> Vec<Effect> {
    let mut effects: Vec<Effect> = Vec::new();

    // Clear pending_g on any action except SetPendingG
    if !matches!(action, Action::SetPendingG) {
        state.modal_stack.pending_g = false;
    }

    match action {
        // Phase 1 actions
        Action::FocusNext => state.focused_panel = state.focused_panel.next(),
        Action::FocusPrev => state.focused_panel = state.focused_panel.prev(),
        Action::FocusUp => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state) {
                    let scroll = state.command_runner.command_output_scroll_by_worktree.entry(id).or_insert(0);
                    *scroll = scroll.saturating_sub(1);
                }
        }
        Action::FocusDown => {
            if state.focused_panel == FocusedPanel::CommandOutput {
                let max = active_output(state).len();
                if let Some(id) = active_worktree_id(state) {
                    let scroll = state.command_runner.command_output_scroll_by_worktree.entry(id).or_insert(0);
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
            state.modal_stack.palette_mode = None;
            if state.metro.is_running() {
                state.metro_state.pending_restart = true;
                effects.extend(update(state, Action::MetroStop));
                return effects;
            }
            // Skip external detection when restarting our own metro (worktree switch or restart).
            // The port may still be releasing from our just-killed process — not an external conflict.
            if state.metro_state.skip_external_metro_check {
                state.metro_state.skip_external_metro_check = false;
                effects.push(Effect::ScheduleAction(Action::MetroStartConfirmed));
                return effects;
            }
            // Check for external metro conflict before spawning (F-201: DetectExternalMetro)
            effects.push(Effect::DetectExternalMetro { port: 8081 });
        }

        Action::MetroStartConfirmed => {
            state.metro.set_starting();
            let worktree_path = state
                .metro_state
                .active_worktree_path
                .clone()
                .unwrap_or_else(|| PathBuf::from("."));
            effects.push(Effect::SpawnMetro { worktree: worktree_path });
        }

        Action::MetroStop => {
            state.modal_stack.palette_mode = None;
            if let Some(handle) = state.metro.take_handle() {
                state.metro.set_stopping();
                // Plan 13-03: kill() is the consuming trait method.
                // Post-13-07, the handle is a TokioMetroHandle from TokioMetroAdapter.
                if let Err(e) = handle.kill() {
                    tracing::warn!("metro handle kill failed: {e}");
                }
            }
        }

        Action::MetroSendDebugger => {
            state.modal_stack.palette_mode = None;
            if state.metro.is_running() {
                effects.push(Effect::MetroHttpPost {
                    url: "http://localhost:8081/open-debugger".to_string(),
                    body: "{}".to_string(),
                });
            }
        }

        Action::MetroSendReload => {
            state.modal_stack.palette_mode = None;
            if state.metro.is_running() {
                effects.push(Effect::MetroHttpPost {
                    url: "http://localhost:8081/reload".to_string(),
                    body: String::new(),
                });
            }
        }

        Action::MetroExited => {
            // Plan 13-09: pending_metro_run is gone — the deferred run command
            // (if any) sits in command_queue.front() and is drained on the
            // next MetroActivityUpdate(Ready). If metro exited unexpectedly,
            // clear the queue so a stale deferred command doesn't fire on the
            // next successful start.
            if !state.metro_state.pending_restart {
                state.command_runner.command_queue.clear();
                state.command_runner.post_drain_action = None;
            }
            state.metro.clear();
            if state.metro_state.pending_restart {
                state.metro_state.pending_restart = false;
                // active_worktree_path is already updated synchronously at the
                // worktree-switch call site (Plan 13-09 — no pending_switch_path).
                // Signal MetroStart to skip external detection — the port may still be
                // releasing from our just-killed process, not an external conflict.
                state.metro_state.skip_external_metro_check = true;
                effects.extend(update(state, Action::MetroStart));
            }
            // Refresh worktree list so metro status (green bg) updates immediately
            effects.extend(update(state, Action::RefreshWorktrees));
        }

        Action::MetroSpawnFailed(msg) => {
            // Plan 13-09: clear queue + post-drain action; pending flags gone.
            state.command_runner.command_queue.clear();
            state.command_runner.post_drain_action = None;
            state.metro.clear();
            state.metro_state.pending_restart = false;
            state.error_state = Some(ErrorState {
                message: format!("Metro failed to start: {msg}"),
                can_retry: true,
            });
        }

        Action::MetroActivityUpdate(activity) => {
            state.metro.activity = Some(activity.clone());
            // Plan 13-09: pending_metro_run absorbed into command_queue. When
            // metro becomes Ready and nothing is currently running, drain the
            // queue front via CommandRun (preserves the full pipeline:
            // sync-check, device selection, etc.). The push_front-on-defer
            // pattern at the dispatch sites guarantees the awaited spec sits
            // at the head of the queue.
            if matches!(activity, crate::domain::metro::MetroActivity::Ready)
                && state.command_runner.running_command.is_none()
                && let Some(spec) = state.command_runner.command_queue.pop_front() {
                    effects.extend(update(state, Action::CommandRun(spec)));
                }
        }

        Action::ExternalMetroDetected(info) => {
            state.modal_stack.modal = Some(ModalState::ExternalMetroConflict {
                pid: info.pid,
                working_dir: info.working_dir,
            });
        }

        Action::KillExternalMetro(pid) => {
            state.modal_stack.modal = None;
            // F-201: effect_runner handles the kill + 500ms wait + schedule
            // MetroStartConfirmed follow-up.
            effects.push(Effect::KillProcess { pid });
        }

        // --- Phase 3: Worktree navigation ---

        Action::WorktreeSelectNext => {
            let len = state.worktree_browser.worktrees.len();
            if len > 0 {
                let i = state.worktree_browser.worktree_table_state.selected().unwrap_or(0);
                let next = if i >= len - 1 { 0 } else { i + 1 };
                state.worktree_browser.worktree_table_state.select(Some(next));
                // Update stable selection id
                state.worktree_browser.selected_worktree_id = Some(state.worktree_browser.worktrees[next].id.clone());
                // Update active worktree for metro
                state.metro_state.active_worktree_path = Some(state.worktree_browser.worktrees[next].path.clone());
            }
        }

        Action::WorktreeSelectPrev => {
            let len = state.worktree_browser.worktrees.len();
            if len > 0 {
                let i = state.worktree_browser.worktree_table_state.selected().unwrap_or(0);
                let prev = if i == 0 { len - 1 } else { i - 1 };
                state.worktree_browser.worktree_table_state.select(Some(prev));
                // Update stable selection id
                state.worktree_browser.selected_worktree_id = Some(state.worktree_browser.worktrees[prev].id.clone());
                // Update active worktree for metro
                state.metro_state.active_worktree_path = Some(state.worktree_browser.worktrees[prev].path.clone());
            }
        }

        Action::WorktreesLoaded(mut worktrees) => {
            // Re-derive jira_key and re-apply cached JIRA titles using the configured prefix.
            // jira_key is set here (not in list_worktrees) because the prefix comes from config.
            for wt in &mut worktrees {
                if let Some(key) = crate::domain::jira::extract_jira_key(&wt.branch, &state.jira.project_prefix) {
                    if let Some(title) = state.jira.title_cache.get(&key) {
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

            state.worktree_browser.worktrees = worktrees;

            if !state.worktree_browser.worktrees.is_empty() {
                // Re-derive selected index from selected_worktree_id (stable across sorts)
                let selected_idx = state
                    .worktree_browser
                    .selected_worktree_id
                    .as_ref()
                    .and_then(|id| state.worktree_browser.worktrees.iter().position(|wt| &wt.id == id))
                    .unwrap_or(0);
                state.worktree_browser.worktree_table_state.select(Some(selected_idx));
                state.metro_state.active_worktree_path = Some(state.worktree_browser.worktrees[selected_idx].path.clone());
            }

            // Phase 4: fetch titles for uncached branches
            if state.jira.available {
                let keys_to_fetch: Vec<String> = state.worktree_browser.worktrees.iter()
                    .filter_map(|wt| {
                        let key = crate::domain::jira::extract_jira_key(&wt.branch, &state.jira.project_prefix)?;
                        if state.jira.title_cache.contains_key(&key) { return None; }
                        Some(key)
                    })
                    .collect();

                if !keys_to_fetch.is_empty() {
                    effects.push(Effect::FetchJiraTitles { keys: keys_to_fetch });
                }
            }
        }

        Action::RefreshWorktrees => {
            if state.worktree_browser.worktree_op_in_flight {
                tracing::debug!("skipping periodic refresh — worktree op in flight");
                return effects;
            }
            effects.push(Effect::ListWorktrees { repo_root: state.app_config.repo_root.clone() });
        }

        // --- Phase 3: Command dispatch ---

        Action::CommandRun(spec) => {
            // Clear palette mode whenever a command is dispatched
            state.modal_stack.palette_mode = None;

            // Get selected worktree (needed for all branches)
            let wt_branch = if !state.worktree_browser.worktrees.is_empty() {
                let idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0);
                let idx = idx.min(state.worktree_browser.worktrees.len() - 1);
                Some((state.worktree_browser.worktrees[idx].branch.clone(), state.worktree_browser.worktrees[idx].stale))
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
                        let idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0);
                        let wt_path = &state.worktree_browser.worktrees[idx.min(state.worktree_browser.worktrees.len() - 1)].path;
                        crate::domain::staleness::check_stale_pods(wt_path)
                    } else {
                        false
                    };

                    if *yarn_stale || pods_stale {
                        if state.app_config.config.as_ref().is_some_and(|c| c.auto_sync) {
                            // Plan 13-09 (F-204 site 1): Recipe::SyncThenRun replaces
                            // the inline yarn/pod sequencing. Auto-sync fast path —
                            // skip the modal, expand the recipe, queue + dispatch.
                            let deps = DependencyState::new(*yarn_stale, pods_stale, is_ios);
                            let mut sequence = Recipe::SyncThenRun(spec).expand(&deps);
                            let first = sequence.remove(0);
                            for cmd in sequence {
                                state.command_runner.command_queue.push_back(cmd);
                            }
                            state.modal_stack.palette_mode = None;
                            if let Some(eff) = dispatch_command(state, first) {
                                effects.push(eff);
                            }
                            return effects;
                        }
                        state.modal_stack.modal = Some(ModalState::SyncBeforeRun {
                            run_command: Box::new(spec),
                            needs_yarn: *yarn_stale,
                            needs_pods: pods_stale,
                        });
                        state.modal_stack.palette_mode = None;
                        return effects;
                    }
                }

            // Plan 13-09 (F-204 site 2): Metro prerequisite — RN run commands
            // need metro running first. The deferred spec is pushed to the
            // FRONT of the command_queue; on `MetroActivityUpdate(Ready)` the
            // queue is drained head-first via CommandRun (preserves the full
            // pipeline). Replaces the deleted `pending_metro_run` field.
            if spec.needs_metro() && !state.metro.is_running() {
                state.command_runner.command_queue.push_front(spec);
                effects.extend(update(state, Action::MetroStart));
                return effects;
            }

            // Pre-processing pipeline
            if spec.is_destructive() {
                let branch_name = wt_branch
                    .map(|(b, _)| b)
                    .unwrap_or_else(|| "(unknown)".to_string());
                state.modal_stack.modal = Some(ModalState::Confirm {
                    prompt: format!("Run '{}' on {}?", spec.label(), branch_name),
                    pending_command: spec,
                });
                return effects;
            }

            if spec.needs_text_input() {
                let prompt = match &spec {
                    CommandSpec::GitRebase { .. } => "Rebase onto:".to_string(),
                    CommandSpec::GitCheckout { .. } => "Branch to checkout:".to_string(),
                    CommandSpec::GitCheckoutNew { .. } => "New branch name:".to_string(),
                    CommandSpec::YarnJest { .. } => "Jest filter:".to_string(),
                    _ => "Input:".to_string(),
                };
                state.modal_stack.modal = Some(ModalState::TextInput {
                    prompt,
                    buffer: String::new(),
                    pending_template: Box::new(spec),
                });
                return effects;
            }

            if spec.needs_device_selection() {
                state.modal_stack.pending_device_command = Some(spec.clone());
                let kind = if matches!(spec, CommandSpec::RnRunAndroid { .. }) {
                    crate::domain::ports::device_port::DeviceKind::Android
                } else {
                    crate::domain::ports::device_port::DeviceKind::Ios
                };
                effects.push(Effect::LoadDevices { kind });
                return effects;
            }

            // Plan 13-09 (F-204 site 3): RnReleaseBuild → Recipe::ReleaseBuildAndInstall
            // Android release build: queue adb install to run after assembleRelease completes.
            if matches!(spec, CommandSpec::RnReleaseBuild) {
                let mut sequence = Recipe::ReleaseBuildAndInstall.expand(&DependencyState::new(false, false, false));
                let first = sequence.remove(0);
                for cmd in sequence {
                    state.command_runner.command_queue.push_back(cmd);
                }
                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
                return effects;
            }

            // Plan 13-09 (F-204 site 4): GitResetHardFetch → Recipe::GitFetchThenReset
            // Two-step — dispatch fetch, queue reset --hard origin/<branch>.
            if matches!(spec, CommandSpec::GitResetHardFetch) {
                let mut sequence = Recipe::GitFetchThenReset.expand(&DependencyState::new(false, false, false));
                let first = sequence.remove(0);
                for cmd in sequence {
                    state.command_runner.command_queue.push_back(cmd);
                }
                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
                return effects;
            }

            // Normal dispatch
            if let Some(eff) = dispatch_command(state, spec) {
                effects.push(eff);
            }
        }

        // --- Phase 3: Command output events ---

        Action::CommandOutputLine(line) => {
            if let Some(id) = active_worktree_id(state) {
                let output = state.command_runner.command_output_by_worktree.entry(id).or_default();
                output.push_back(line);
                if output.len() > MAX_COMMAND_LINES {
                    output.pop_front();
                }
            }
        }

        Action::CommandExited => {
            let completed_cmd = state.command_runner.running_command.take();
            state.command_runner.command_task = None;

            // Refresh staleness BEFORE draining the queue so any queued run command
            // re-entering CommandRun sees up-to-date stale state (prevents re-showing
            // the SyncBeforeRun modal after yarn install just ran).
            if let Some(ref cmd) = completed_cmd {
                let refresh = crate::domain::refresh::refresh_needed(cmd);
                if refresh.worktrees {
                    // Full worktree reload (also re-checks staleness and triggers JIRA fetch
                    // via WorktreesLoaded handler when branch names change)
                    effects.push(Effect::ListWorktrees { repo_root: state.app_config.repo_root.clone() });
                } else if refresh.staleness {
                    // Staleness refresh: re-check ALL worktrees (cheap I/O, ensures
                    // correct state even if user changed selection during command)
                    for wt in state.worktree_browser.worktrees.iter_mut() {
                        wt.stale = crate::domain::staleness::check_stale(&wt.path);
                        wt.stale_pods = crate::domain::staleness::check_stale_pods(&wt.path);
                    }
                }
            }

            // Drain command queue — pop_front and dispatch if non-empty.
            // Plan 13-09 (F-204 site 5): for specs that need metro but metro
            // isn't running, push the spec BACK to the front and dispatch
            // MetroStart. The queue acts as the deferred-spec store
            // (replacing `pending_metro_run`); MetroActivityUpdate(Ready)
            // drains the head when metro is up.
            if let Some(next_spec) = state.command_runner.command_queue.pop_front() {
                if next_spec.needs_metro() && !state.metro.is_running() {
                    state.command_runner.command_queue.push_front(next_spec);
                    effects.extend(update(state, Action::MetroStart));
                } else if let Some(eff) = dispatch_command(state, next_spec) {
                    effects.push(eff);
                }
            } else if let Some(action) = state.command_runner.post_drain_action.take() {
                // Plan 13-09: replaces `pending_metro_after_sync` with a
                // generalized post-queue-drain action. Sync-then-metro flow
                // stores Some(MetroStart) here; future post-drain coordination
                // can reuse the same slot.
                effects.extend(update(state, *action));
            }
        }

        Action::CommandOutputClear => {
            if let Some(id) = active_worktree_id(state) {
                state.command_runner.command_output_by_worktree.remove(&id);
                state.command_runner.command_output_scroll_by_worktree.remove(&id);
            }
        }

        Action::CommandCancel => {
            if let Some(task) = state.command_runner.command_task.take() {
                task.abort();
            }
            state.command_runner.running_command = None;
            // Also clear pending queue items — cancel is all-or-nothing
            state.command_runner.command_queue.clear();
            // Plan 13-09: clear post-drain action so a cancel mid-sync doesn't
            // silently fire metro after the user explicitly cancelled.
            state.command_runner.post_drain_action = None;
            if let Some(id) = active_worktree_id(state) {
                let output = state.command_runner.command_output_by_worktree.entry(id).or_default();
                output.push_back("[cancelled]".into());
                if output.len() > MAX_COMMAND_LINES {
                    output.pop_front();
                }
            }
        }

        // --- Phase 5.1: Command queue actions ---

        Action::CommandQueuePush(spec) => {
            state.command_runner.command_queue.push_back(spec);
        }

        Action::CommandQueueClear => {
            state.command_runner.command_queue.clear();
        }

        // --- Phase 3: Modal actions ---

        Action::ShowCommandPalette => {
            // Palette activation is handled via EnterGitPalette / EnterRnPalette.
            // This variant is kept for backward compatibility.
        }

        Action::ModalConfirm => {
            // Check for pending worktree removal BEFORE falling through to normal confirm
            if let Some((wt_id, wt_path, _branch)) = state.modal_stack.pending_worktree_removal.take() {
                state.modal_stack.modal = None;

                // Stop metro if it's running on the worktree being removed
                if state.metro.is_running()
                    && state.metro_state.active_worktree_path.as_ref() == Some(&wt_path) {
                        effects.extend(update(state, Action::MetroStop));
                    }

                // Clean up per-worktree dashboard state
                state.command_runner.command_output_by_worktree.remove(&wt_id);
                state.command_runner.command_output_scroll_by_worktree.remove(&wt_id);

                // Immediately remove from worktree list for instant visual feedback
                state.worktree_browser.worktrees.retain(|wt| wt.id != wt_id);
                if state.worktree_browser.worktrees.is_empty() {
                    state.worktree_browser.worktree_table_state.select(None);
                    state.worktree_browser.selected_worktree_id = None;
                    state.metro_state.active_worktree_path = None;
                } else {
                    let idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0)
                        .min(state.worktree_browser.worktrees.len() - 1);
                    state.worktree_browser.worktree_table_state.select(Some(idx));
                    state.worktree_browser.selected_worktree_id = Some(state.worktree_browser.worktrees[idx].id.clone());
                    state.metro_state.active_worktree_path = Some(state.worktree_browser.worktrees[idx].path.clone());
                }

                // Emit the async removal effect
                state.worktree_browser.worktree_op_in_flight = true;
                effects.push(Effect::RemoveWorktree { repo_root: state.app_config.repo_root.clone(), path: wt_path });
                return effects;
            }

            if let Some(ModalState::Confirm { pending_command, .. }) = state.modal_stack.modal.take() {
                // Dispatch directly — skip pre-processing (already confirmed)
                if let Some(eff) = dispatch_command(state, pending_command) {
                    effects.push(eff);
                }
            }
        }

        Action::ModalCancel => {
            state.modal_stack.modal = None;
            state.modal_stack.palette_mode = None;
            state.modal_stack.pending_claude_open = None;       // prevent pending state leak on Esc
            state.modal_stack.pending_android_mode = false;
            state.modal_stack.pending_worktree_removal = None;  // discard any pending removal on cancel
            state.modal_stack.pending_worktree_add = false;     // discard any pending add on cancel
            state.modal_stack.pending_new_branch_base = None;   // discard new-branch base on cancel
            state.modal_stack.pending_new_branch_worktree = false;
        }

        Action::ModalInputChar(c) => {
            // Plan 13-09 (F-205): exhaustive ModalState enumeration. Rust's
            // exhaustiveness checker now guards future ModalState additions —
            // a new variant will trigger a compile error here until explicitly
            // handled.
            match state.modal_stack.modal.as_mut() {
                Some(ModalState::TextInput { buffer, .. }) => {
                    buffer.push(c);
                }
                Some(ModalState::DevicePicker { filter, selected, .. }) => {
                    filter.push(c);
                    *selected = 0; // reset selection when filter changes
                }
                // Intentionally ignore char input — these modals do not accept
                // arbitrary characters as text. (BranchPicker filter input
                // arrives as Action::BranchPickerFilter, not ModalInputChar.)
                Some(ModalState::Confirm { .. })
                | Some(ModalState::CleanToggle { .. })
                | Some(ModalState::SyncBeforeRun { .. })
                | Some(ModalState::SyncBeforeMetro { .. })
                | Some(ModalState::ExternalMetroConflict { .. })
                | Some(ModalState::BranchPicker { .. })
                | None => { /* no-op */ }
            }
        }

        Action::ModalInputBackspace => {
            // Plan 13-09 (F-205): exhaustive ModalState enumeration — same
            // rationale as ModalInputChar above.
            match state.modal_stack.modal.as_mut() {
                Some(ModalState::TextInput { buffer, .. }) => {
                    buffer.pop();
                }
                Some(ModalState::DevicePicker { filter, selected, .. }) => {
                    filter.pop();
                    *selected = 0; // reset selection when filter changes
                }
                // Intentionally ignore backspace — these modals do not have
                // editable text buffers. (BranchPicker filter editing arrives
                // as Action::BranchPickerBackspace.)
                Some(ModalState::Confirm { .. })
                | Some(ModalState::CleanToggle { .. })
                | Some(ModalState::SyncBeforeRun { .. })
                | Some(ModalState::SyncBeforeMetro { .. })
                | Some(ModalState::ExternalMetroConflict { .. })
                | Some(ModalState::BranchPicker { .. })
                | None => { /* no-op */ }
            }
        }

        Action::ModalInputSubmit => {
            if let Some(modal) = state.modal_stack.modal.take() {
                match modal {
                    ModalState::TextInput {
                        buffer,
                        pending_template,
                        ..
                    } => {
                        if state.modal_stack.pending_android_mode {
                            state.modal_stack.pending_android_mode = false;
                            let mode = if buffer.trim().is_empty() { None } else { Some(buffer.trim().to_string()) };
                            state.app_config.android_mode = mode.clone();
                            if let Some(m) = mode {
                                effects.push(Effect::SaveAndroidMode(m));
                            }
                        } else if state.modal_stack.pending_new_branch_worktree {
                            state.modal_stack.pending_new_branch_worktree = false;
                            let new_branch_name = buffer.trim().to_string();
                            let base_branch = state.modal_stack.pending_new_branch_base.take();
                            if new_branch_name.is_empty() {
                                return effects;
                            }
                            let base_branch = match base_branch {
                                Some(b) => b,
                                None => return effects,
                            };
                            state.worktree_browser.worktree_op_in_flight = true;
                            effects.push(Effect::AddWorktreeNewBranch {
                                repo_root: state.app_config.repo_root.clone(),
                                new: new_branch_name,
                                base: base_branch,
                            });
                        } else if state.modal_stack.pending_worktree_add {
                            state.modal_stack.pending_worktree_add = false;
                            let branch_name = buffer.trim().to_string();
                            if branch_name.is_empty() {
                                return effects;
                            }
                            state.worktree_browser.worktree_op_in_flight = true;
                            effects.push(Effect::AddWorktree { repo_root: state.app_config.repo_root.clone(), branch: branch_name });
                        } else if let Some(wt_id) = state.modal_stack.pending_claude_open.take() {
                            // Claude tab name modal submit
                            let suffix = if buffer.trim().is_empty() {
                                "claude".to_string()
                            } else {
                                buffer
                            };
                            let wt = state.worktree_browser.worktrees.iter()
                                .find(|wt| wt.path.file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or("") == wt_id)
                                .cloned();
                            if let Some(wt) = wt {
                                let name = format!("{}-{}", wt.preferred_prefix(), suffix);
                                let path = wt.path.clone();
                                let flags = state.app_config.claude_flags.clone();
                                let command = if flags.is_empty() {
                                    "claude".to_string()
                                } else {
                                    format!("claude {flags}")
                                };
                                effects.push(Effect::OpenInMultiplexer {
                                    worktree: path,
                                    name,
                                    command,
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
                            if let Some(eff) = dispatch_command(state, real_spec) {
                                effects.push(eff);
                            }
                        }
                    }
                    other => {
                        // Restore modal if wrong type (shouldn't happen)
                        state.modal_stack.modal = Some(other);
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
            }) = state.modal_stack.modal
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
            }) = state.modal_stack.modal
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
            }) = state.modal_stack.modal.take()
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
                                effects.push(Effect::SaveAndroidMode(m.clone()));
                            }
                            let mode_flag = mode.map(|m| format!(" --mode {m}")).unwrap_or_default();
                            let cmd = format!(
                                "emulator -avd {device_id} > /dev/null 2>&1 & adb wait-for-device && npx react-native run-android{mode_flag}"
                            );
                            if let Some(eff) = dispatch_command(state, CommandSpec::ShellCommand { command: cmd }) {
                                effects.push(eff);
                            }
                        }
                        return effects;
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
                        effects.push(Effect::SaveAndroidMode(m.clone()));
                    }
                    // Record iOS simulator usage for sort-by-recent
                    if is_ios {
                        effects.push(Effect::ScheduleAction(Action::SimulatorUsed(device_id)));
                    }
                    if let Some(eff) = dispatch_command(state, real_spec) {
                        effects.push(eff);
                    }
                }
            }
        }

        // --- Phase 3: Device enumeration (async callback) ---

        Action::DevicesEnumerated(devices) => {
            if let Some(spec) = state.modal_stack.pending_device_command.take() {
                match devices.len() {
                    0 => {
                        if let Some(id) = active_worktree_id(state) {
                            let output = state.command_runner.command_output_by_worktree.entry(id).or_default();
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
                                    effects.push(Effect::SaveAndroidMode(m.clone()));
                                }
                                let mode_flag = mode.map(|m| format!(" --mode {m}")).unwrap_or_default();
                                let cmd = format!(
                                    "emulator -avd {} > /dev/null 2>&1 & adb wait-for-device && npx react-native run-android{}",
                                    devices[0].id, mode_flag
                                );
                                if let Some(eff) = dispatch_command(state, CommandSpec::ShellCommand { command: cmd }) {
                                    effects.push(eff);
                                }
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
                                effects.push(Effect::SaveAndroidMode(m.clone()));
                            }
                            if let Some(eff) = dispatch_command(state, real_spec) {
                                effects.push(eff);
                            }
                        }
                    }
                    _ => {
                        // Multiple devices — show picker
                        // Sort iOS simulators by last-used from sim_history
                        let mut sorted_devices = devices;
                        if matches!(spec, CommandSpec::RnRunIos { .. }) {
                            // Plan 13-08: sim_history is loaded into AppState at startup
                            // (src/main.rs) so update() never crosses the infra boundary.
                            let history = &state.app_config.sim_history;
                            sorted_devices.sort_by_key(|d| {
                                history.iter().position(|h| h == &d.id)
                                    .unwrap_or(usize::MAX)
                            });
                        }
                        state.modal_stack.modal = Some(ModalState::DevicePicker {
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
            state.modal_stack.palette_mode = Some(PaletteMode::Git);
        }

        Action::EnterRnPalette => {
            // EnterRnPalette kept for backward compat — Phase 05.1 will remap 'c' key
            // to new submenu scheme. For now we just cancel palette mode.
            state.modal_stack.palette_mode = None;
        }



        // --- Phase 5: Worktree switching and Claude Code ---

        Action::WorktreeSwitchToSelected => {
            let selected_idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0);
            // Capture target path NOW — navigation may change active_worktree_path later
            let target_path = state.worktree_browser.worktrees
                .get(selected_idx)
                .map(|wt| wt.path.clone());

            // Stale dependency check — metro only needs yarn, not pods
            if let Some(wt) = state.worktree_browser.worktrees.get(selected_idx)
                && wt.stale
            {
                if state.app_config.config.as_ref().is_some_and(|c| c.auto_sync) {
                    // Plan 13-09 (F-204 site 10a): auto-sync fast path uses
                    // Recipe::SyncThenStartMetro. Set active_worktree_path
                    // synchronously (replaces pending_switch_path).
                    if let Some(path) = target_path {
                        state.metro_state.active_worktree_path = Some(path);
                    }
                    if state.metro.is_running() {
                        state.metro_state.pending_restart = false;
                        effects.extend(update(state, Action::MetroStop));
                    }
                    let deps = DependencyState::new(true, false, false);
                    let mut sequence = Recipe::SyncThenStartMetro.expand(&deps);
                    // post_drain_action replaces pending_metro_after_sync.
                    state.command_runner.post_drain_action = Some(Box::new(Action::MetroStart));
                    if sequence.is_empty() {
                        state.command_runner.post_drain_action = None;
                        effects.extend(update(state, Action::MetroStart));
                        return effects;
                    }
                    let first = sequence.remove(0);
                    for cmd in sequence {
                        state.command_runner.command_queue.push_back(cmd);
                    }
                    if let Some(eff) = dispatch_command(state, first) {
                        effects.push(eff);
                    }
                    return effects;
                }
                // Plan 13-09 (F-204 site 10b): set active_worktree_path NOW so the
                // SyncBeforeMetro modal accept/decline paths don't need a stash.
                if let Some(path) = target_path {
                    state.metro_state.active_worktree_path = Some(path);
                }
                state.modal_stack.modal = Some(ModalState::SyncBeforeMetro { needs_yarn: true, needs_pods: false });
                return effects;
            }

            // Original logic — only reached when deps are fresh.
            // Plan 13-09: set active_worktree_path immediately; pending_restart
            // alone signals the MetroExited handler to dispatch MetroStart.
            if let Some(path) = target_path {
                state.metro_state.active_worktree_path = Some(path);
            }
            if state.metro.is_running() {
                // Kill current → MetroExited → MetroStart in (already-updated) worktree
                state.metro_state.pending_restart = true;
                effects.extend(update(state, Action::MetroStop));
            } else {
                // Not running — just start directly in selected worktree
                effects.extend(update(state, Action::MetroStart));
            }
        }

        Action::OpenClaudeCode => {
            if !state.app_config.multiplexer_available {
                state.error_state = Some(ErrorState {
                    message: "Cannot open Claude Code: not inside a tmux or zellij session".into(),
                    can_retry: false,
                });
                return effects;
            }
            let wt = if !state.worktree_browser.worktrees.is_empty() {
                let idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0)
                    .min(state.worktree_browser.worktrees.len() - 1);
                &state.worktree_browser.worktrees[idx]
            } else {
                return effects;
            };
            // Store worktree dir name for later use when modal submits
            state.modal_stack.pending_claude_open = Some(
                wt.path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string()
            );
            state.modal_stack.modal = Some(ModalState::TextInput {
                prompt: "Claude tab suffix:".to_string(),
                buffer: String::new(),
                pending_template: Box::new(crate::domain::command::CommandSpec::YarnLint), // sentinel — not used
            });
        }

        Action::OpenShellTab => {
            if !state.app_config.multiplexer_available {
                state.error_state = Some(ErrorState {
                    message: "Cannot open shell tab: not inside a tmux or zellij session".into(),
                    can_retry: false,
                });
                return effects;
            }
            let wt = if !state.worktree_browser.worktrees.is_empty() {
                let idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0)
                    .min(state.worktree_browser.worktrees.len() - 1);
                state.worktree_browser.worktrees[idx].clone()
            } else {
                return effects;
            };
            let path = wt.path.clone();
            let name = format!("{}-shell", wt.preferred_prefix());
            let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
            effects.push(Effect::OpenInMultiplexer {
                worktree: path,
                name,
                command: shell,
            });
        }

        // --- Phase 4: JIRA title fetch results ---

        Action::JiraTitlesFetched(titles) => {
            // Update in-memory cache
            for (key, title) in &titles {
                state.jira.title_cache.insert(key.clone(), title.clone());
            }
            // Persist cache to disk via effect_runner (F-201)
            effects.push(Effect::SaveJiraCache(state.jira.title_cache.clone()));
            // Apply titles to currently loaded worktrees
            for wt in &mut state.worktree_browser.worktrees {
                if let Some(key) = crate::domain::jira::extract_jira_key(&wt.branch, &state.jira.project_prefix)
                    && let Some(title) = state.jira.title_cache.get(&key) {
                        wt.jira_title = Some(title.clone());
                    }
            }
        }

        // --- Phase 5.1: New submenu and action stubs ---

        Action::EnterAndroidPalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Android);
        }
        Action::EnterIosPalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Ios);
        }
        Action::EnterYarnPalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Yarn);
        }
        Action::EnterWorktreePalette => {
            state.modal_stack.palette_mode = Some(PaletteMode::Worktree);
        }
        Action::OpenCleanMenu => {
            state.modal_stack.palette_mode = None;
            state.modal_stack.modal = Some(ModalState::CleanToggle { options: CleanOptions::default() });
        }
        Action::CleanToggleNodeModules => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal_stack.modal {
                options.node_modules = !options.node_modules;
            }
        }
        Action::CleanTogglePods => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal_stack.modal {
                options.pods = !options.pods;
            }
        }
        Action::CleanToggleAndroid => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal_stack.modal {
                options.android = !options.android;
            }
        }
        Action::CleanToggleSyncAfter => {
            if let Some(ModalState::CleanToggle { ref mut options }) = state.modal_stack.modal {
                options.sync_after = !options.sync_after;
            }
        }
        Action::CleanConfirm => {
            if let Some(ModalState::CleanToggle { options }) = state.modal_stack.modal.take() {
                state.modal_stack.palette_mode = None;

                // Plan 13-09 (F-204 site 6): Recipe::Clean replaces inline
                // option-to-command sequencing. Order is enforced by
                // Recipe::expand: react-native clean first, node_modules last,
                // sync_after appended (preserves the comment's hard ordering
                // rule).
                let mut cmds = Recipe::Clean(options).expand(&DependencyState::new(false, false, false));

                if cmds.is_empty() {
                    return effects;
                }

                // Dispatch first, queue rest
                let first = cmds.remove(0);
                for cmd in cmds {
                    state.command_runner.command_queue.push_back(cmd);
                }
                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
            }
        }
        Action::ToggleFullscreen => {
            if state.worktree_browser.fullscreen_panel.is_some() {
                state.worktree_browser.fullscreen_panel = None;
                state.focused_panel = state.focused_panel.next();
            } else {
                // Only CommandOutput can be fullscreened
                if state.focused_panel == FocusedPanel::CommandOutput {
                    state.worktree_browser.fullscreen_panel = Some(state.focused_panel);
                }
            }
        }
        Action::StartShellCommand => {
            state.modal_stack.modal = Some(ModalState::TextInput {
                prompt: "Shell command:".to_string(),
                buffer: String::new(),
                pending_template: Box::new(CommandSpec::ShellCommand { command: String::new() }),
            });
        }
        Action::StartSetAndroidMode => {
            state.modal_stack.palette_mode = None;
            state.modal_stack.pending_android_mode = true;
            state.modal_stack.modal = Some(ModalState::TextInput {
                prompt: "Android build mode:".to_string(),
                buffer: state.app_config.android_mode.clone().unwrap_or_default(),
                pending_template: Box::new(CommandSpec::YarnLint), // sentinel — not actually used
            });
        }
        Action::SimulatorUsed(udid) => {
            // Fire-and-forget write to sim history via effect_runner
            effects.push(Effect::RecordSimUsed(udid));
        }
        Action::SyncBeforeRunAccept => {
            if let Some(ModalState::SyncBeforeRun { run_command, needs_yarn, needs_pods }) = state.modal_stack.modal.take() {
                // Plan 13-09 (F-204 site 7): Recipe::SyncThenRun expansion.
                // The modal already encodes the staleness decision in
                // (needs_yarn, needs_pods); rebuild a DependencyState that
                // reproduces the same expansion. The is_ios_target flag is
                // derived from needs_pods being meaningful — only iOS run
                // commands ever set needs_pods=true at the modal-construction
                // site (CommandRun stale check).
                let deps = DependencyState::new(needs_yarn, needs_pods, needs_pods);
                let mut sequence = Recipe::SyncThenRun(*run_command).expand(&deps);

                // Guaranteed non-empty: we only get here from the modal which only
                // appears when needs_yarn || needs_pods, so sequence has ≥2 elements.
                let first = sequence.remove(0);
                for cmd in sequence {
                    state.command_runner.command_queue.push_back(cmd);
                }
                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
            }
        }
        Action::SyncBeforeRunDecline => {
            if let Some(ModalState::SyncBeforeRun { run_command, .. }) = state.modal_stack.modal.take() {
                // Plan 13-09 (F-204 site 8): skip sync. Metro may still need to
                // start — push the spec onto command_queue front and trigger
                // MetroStart (replaces `pending_metro_run`). The stale check
                // won't re-trigger because the user just declined it.
                let spec = *run_command;
                if spec.needs_metro() && !state.metro.is_running() {
                    state.command_runner.command_queue.push_front(spec);
                    effects.extend(update(state, Action::MetroStart));
                } else if let Some(eff) = dispatch_command(state, spec) {
                    effects.push(eff);
                }
            }
        }

        Action::SyncBeforeMetroAccept => {
            if let Some(ModalState::SyncBeforeMetro { needs_yarn, needs_pods }) = state.modal_stack.modal.take() {
                // Plan 13-09: pending_switch_path deleted. The active_worktree_path
                // was already updated synchronously at the WorktreeSwitchToSelected
                // call site when the SyncBeforeMetro modal was constructed.

                // Stop metro if running (no auto-restart — sync must finish first)
                if state.metro.is_running() {
                    state.metro_state.pending_restart = false;
                    effects.extend(update(state, Action::MetroStop));
                }

                // Plan 13-09 (F-204 site 9): Recipe::SyncThenStartMetro expansion.
                // is_ios_target is irrelevant for SyncThenStartMetro (pods always
                // included when stale per the metro-platform-agnostic rule).
                let deps = DependencyState::new(needs_yarn, needs_pods, false);
                let sequence = Recipe::SyncThenStartMetro.expand(&deps);

                // Plan 13-09: replaces `pending_metro_after_sync = true`.
                // After the queue drains, dispatch MetroStart.
                state.command_runner.post_drain_action = Some(Box::new(Action::MetroStart));

                if sequence.is_empty() {
                    // No sync needed (modal arose from a different stale signal,
                    // both flags now false) — start metro directly.
                    state.command_runner.post_drain_action = None;
                    effects.extend(update(state, Action::MetroStart));
                    return effects;
                }

                let mut sequence = sequence;
                let first = sequence.remove(0);
                for cmd in sequence {
                    state.command_runner.command_queue.push_back(cmd);
                }
                if let Some(eff) = dispatch_command(state, first) {
                    effects.push(eff);
                }
            }
        }

        Action::SyncBeforeMetroDecline => {
            if let Some(ModalState::SyncBeforeMetro { .. }) = state.modal_stack.modal.take() {
                // Plan 13-09: active_worktree_path is already set at the
                // WorktreeSwitchToSelected call site (no pending_switch_path).
                if state.metro.is_running() {
                    state.metro_state.pending_restart = true;
                    effects.extend(update(state, Action::MetroStop));
                } else {
                    effects.extend(update(state, Action::MetroStart));
                }
            }
        }

        // --- Phase 5.2: Universal scroll ---

        Action::ScrollToTop => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state) {
                    state.command_runner.command_output_scroll_by_worktree.insert(id, 0);
                }
        }

        Action::ScrollToBottom => {
            if state.focused_panel == FocusedPanel::CommandOutput
                && let Some(id) = active_worktree_id(state) {
                    let max = state.command_runner.command_output_by_worktree
                        .get(&id)
                        .map(|o| o.len())
                        .unwrap_or(0);
                    state.command_runner.command_output_scroll_by_worktree.insert(id, max);
                }
        }

        Action::SetPendingG => {
            state.modal_stack.pending_g = true;
        }

        Action::CommandOutputScrollUp => {
            if let Some(id) = active_worktree_id(state) {
                let scroll = state.command_runner.command_output_scroll_by_worktree.entry(id).or_insert(0);
                *scroll = scroll.saturating_sub(1);
            }
        }

        Action::CommandOutputScrollDown => {
            let max = active_output(state).len();
            if let Some(id) = active_worktree_id(state) {
                let scroll = state.command_runner.command_output_scroll_by_worktree.entry(id).or_insert(0);
                if *scroll < max {
                    *scroll += 1;
                }
            }
        }

        // --- Quick-2: Worktree removal ---

        Action::WorktreeRemove => {
            if state.worktree_browser.worktrees.is_empty() {
                return effects;
            }
            let idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0)
                .min(state.worktree_browser.worktrees.len() - 1);
            let wt = state.worktree_browser.worktrees[idx].clone();

            // Guard: cannot remove the main worktree (its path equals repo_root)
            if wt.path == state.app_config.repo_root {
                state.error_state = Some(ErrorState {
                    message: "Cannot remove the main worktree".into(),
                    can_retry: false,
                });
                state.modal_stack.palette_mode = None;
                return effects;
            }

            // Store removal target so ModalConfirm knows what to do
            state.modal_stack.pending_worktree_removal = Some((wt.id.clone(), wt.path.clone(), wt.branch.clone()));

            // Build confirm prompt — mention metro if it will be stopped
            let metro_note = if state.metro.is_running()
                && state.metro_state.active_worktree_path.as_ref() == Some(&wt.path)
            {
                " (metro will be stopped)"
            } else {
                ""
            };
            let prompt = format!("Remove worktree '{}' and delete directory?{}", wt.branch, metro_note);

            // Use a sentinel CommandSpec for the confirm modal — the actual removal
            // logic is in ModalConfirm when pending_worktree_removal is Some.
            state.modal_stack.modal = Some(ModalState::Confirm {
                prompt,
                pending_command: crate::domain::command::CommandSpec::GitPull, // sentinel
            });
            state.modal_stack.palette_mode = None;
        }

        Action::WorktreeRemoved(path_str) => {
            tracing::info!("worktree removed: {}", path_str);
            state.worktree_browser.worktree_op_in_flight = false;
            // Refresh the worktree list to reflect the removal
            effects.push(Effect::ListWorktrees { repo_root: state.app_config.repo_root.clone() });
        }

        Action::WorktreeRemoveFailed(err) => {
            state.worktree_browser.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to remove worktree: {err}"),
                can_retry: false,
            });
            // Re-add the worktree to the UI since git removal failed
            effects.extend(update(state, Action::RefreshWorktrees));
        }

        // --- Quick-260403-dmz: Worktree creation ---

        Action::WorktreeAdd => {
            state.modal_stack.palette_mode = None;
            state.modal_stack.pending_worktree_add = true;
            state.modal_stack.modal = Some(ModalState::TextInput {
                prompt: "New worktree branch name:".to_string(),
                buffer: String::new(),
                pending_template: Box::new(crate::domain::command::CommandSpec::GitPull), // sentinel — not used
            });
        }

        Action::WorktreeAdded(path_str) => {
            tracing::info!("worktree added: {}", path_str);
            state.worktree_browser.worktree_op_in_flight = false;
            effects.push(Effect::ListWorktrees { repo_root: state.app_config.repo_root.clone() });
        }

        Action::WorktreeAddFailed(err) => {
            state.worktree_browser.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to create worktree: {err}"),
                can_retry: false,
            });
        }

        // Phase 08-02: New-branch worktree creation flow

        Action::WorktreeAddNewBranch => {
            state.modal_stack.palette_mode = None;
            effects.push(Effect::ListRemoteBranches { repo_root: state.app_config.repo_root.clone() });
        }

        Action::BranchesLoaded(branches) => {
            state.modal_stack.modal = Some(ModalState::BranchPicker {
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
            }) = state.modal_stack.modal
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
            }) = state.modal_stack.modal
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
            }) = state.modal_stack.modal
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
            }) = state.modal_stack.modal
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
            }) = state.modal_stack.modal.take()
            {
                // Apply filter to get visible list
                let filtered: Vec<&String> = if filter.is_empty() {
                    branches.iter().collect()
                } else {
                    let lower = filter.to_lowercase();
                    branches.iter().filter(|b| b.to_lowercase().contains(&lower)).collect()
                };
                if let Some(base_branch) = filtered.get(selected) {
                    state.modal_stack.pending_new_branch_base = Some((*base_branch).clone());
                    state.modal_stack.pending_new_branch_worktree = true;
                    state.modal_stack.modal = Some(ModalState::TextInput {
                        prompt: "New branch name:".to_string(),
                        buffer: String::new(),
                        pending_template: Box::new(CommandSpec::GitPull), // sentinel — not used
                    });
                }
            }
        }

        Action::WorktreeNewBranchCreated(path_str) => {
            tracing::info!("worktree with new branch created: {}", path_str);
            state.worktree_browser.worktree_op_in_flight = false;
            effects.push(Effect::ListWorktrees { repo_root: state.app_config.repo_root.clone() });
        }

        Action::WorktreeNewBranchFailed(err) => {
            state.worktree_browser.worktree_op_in_flight = false;
            state.error_state = Some(ErrorState {
                message: format!("Failed to create worktree with new branch: {err}"),
                can_retry: false,
            });
        }
    }

    effects
}
