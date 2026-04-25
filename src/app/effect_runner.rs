//! Effect interpreter — F-202 consumer boundary (Plan 13-08).
//!
//! `EffectRunner::run_effects` is the single boundary between the pure TEA
//! `update()` function and the tokio runtime. Every side-effect that
//! `update()` returns as an `Effect` variant is dispatched here through the
//! injected `Adapters` bundle. After Plan 13-08 there are zero direct
//! `infra::*` calls in this module — every port hop goes through
//! `self.adapters.<port>.<method>()`.
//!
//! Channels:
//! - `action_tx` — the canonical Action stream `update()` consumes.
//! - `handle_tx` — a dedicated channel for `Box<dyn MetroHandle>` since
//!   `Action` derives `Clone + PartialEq` (which `Box<dyn MetroHandle>` does
//!   not implement) and the handle must reach the main thread for
//!   `state.metro.register()`.
//!
//! Effect coverage map (every variant has a match arm below):
//!   ScheduleAction(a)                              → action_tx.send(a)
//!   SpawnCommand { spec, cwd, branch }             → adapters.command_runner.spawn(...) + CommandEvent→Action
//!   DetectExternalMetro { port }                   → adapters.port_probe.detect_external(port)
//!   SpawnMetro { worktree }                        → adapters.metro.start(worktree, on_activity)
//!   MetroHttpPost { url, body }                    → adapters.metro.http_post(url, body)
//!   KillProcess { pid }                            → adapters.port_probe.kill_process(pid)
//!   LoadDevices { kind }                           → adapters.devices.list(kind)
//!   ListWorktrees { repo_root }                    → adapters.worktrees.list(repo_root)
//!   RemoveWorktree { repo_root, path }             → adapters.worktrees.remove(...)
//!   AddWorktree { repo_root, branch }              → adapters.worktrees.add(...)
//!   AddWorktreeNewBranch { repo_root, new, base }  → adapters.worktrees.add_new_branch(...)
//!   ListRemoteBranches { repo_root }               → adapters.worktrees.list_remote_branches(repo_root)
//!   FetchJiraTitles { keys }                       → adapters.jira.as_ref()?.fetch_title(...)
//!   SaveJiraCache(map)                             → spawn_blocking infra::jira_cache::save_jira_cache  (F-111 deferred)
//!   SaveAndroidMode(mode)                          → spawn_blocking infra::android_prefs::save_android_mode  (F-111 deferred)
//!   RecordSimUsed(udid)                            → spawn_blocking infra::sim_history::record_sim_used  (F-111 deferred)
//!   OpenInMultiplexer { worktree, name, command }  → adapters.multiplexer.as_ref()?.new_window(...)
//!
//! G-01 carve-out (whitelisted in `Makefile` arch-lint): the three
//! persistence variants (SaveJiraCache, SaveAndroidMode, RecordSimUsed) still
//! call `infra::<module>::save_*` directly. F-111 (PersistencePort) is
//! deferred — when it lands those three lines route through
//! `adapters.persistence` and the whitelist disappears.

#![allow(dead_code)]

use super::adapters::Adapters;
use super::effect::Effect;
use crate::domain::action::Action;
use crate::domain::metro::MetroActivity;
use crate::domain::metro::MetroHandle;
use tokio::sync::mpsc::UnboundedSender;

/// Effect interpreter. Owns the `Adapters` bundle (Plan 13-08) + the action
/// stream + the handle-delivery channel.
pub struct EffectRunner {
    pub adapters: Adapters,
    pub action_tx: UnboundedSender<Action>,
    pub handle_tx: UnboundedSender<Box<dyn MetroHandle>>,
}

impl EffectRunner {
    pub fn new(
        adapters: Adapters,
        action_tx: UnboundedSender<Action>,
        handle_tx: UnboundedSender<Box<dyn MetroHandle>>,
    ) -> Self {
        Self {
            adapters,
            action_tx,
            handle_tx,
        }
    }

    pub async fn run_effects(&self, effects: Vec<Effect>) {
        for effect in effects {
            self.run_one(effect);
        }
    }

    fn run_one(&self, effect: Effect) {
        match effect {
            Effect::ScheduleAction(action) => {
                let _ = self.action_tx.send(action);
            }

            Effect::DetectExternalMetro { port } => {
                let probe = self.adapters.port_probe.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match probe.detect_external(port).await {
                        Some(info) => {
                            let _ = tx.send(Action::ExternalMetroDetected(info));
                        }
                        None => {
                            let _ = tx.send(Action::MetroStartConfirmed);
                        }
                    }
                });
            }

            Effect::SpawnMetro { worktree } => {
                let metro = self.adapters.metro.clone();
                let action_tx = self.action_tx.clone();
                let handle_tx = self.handle_tx.clone();
                let activity_tx = action_tx.clone();
                let exited_tx = action_tx.clone();
                tokio::spawn(async move {
                    // The on_activity callback bridges the callback-style
                    // MetroPort trait to the existing Action channel that
                    // update() consumes. Plan 13-08: we additionally wire a
                    // natural-exit signal — when the adapter delivers
                    // MetroActivity::Error("exited"-shaped) we forward
                    // Action::MetroExited so the state machine clears the
                    // handle. (See D-13-07-06 deferral.)
                    let on_activity: Box<dyn Fn(MetroActivity) + Send + Sync> =
                        Box::new(move |act| {
                            // Heuristic: the adapter emits
                            // MetroActivity::Error("...") when stdout/stderr
                            // close unexpectedly. That's the natural-crash
                            // signal — additionally fire MetroExited so
                            // update() clears state.metro.
                            if matches!(&act, MetroActivity::Error(_)) {
                                let _ = exited_tx.send(Action::MetroExited);
                            }
                            let _ = activity_tx.send(Action::MetroActivityUpdate(act));
                        });
                    match metro.start(worktree, on_activity).await {
                        Ok(handle) => {
                            // Deliver via the dedicated handle channel — the
                            // event loop calls state.metro.register() on the
                            // main thread. AppState is not Send across the
                            // spawn boundary.
                            let _ = handle_tx.send(handle);
                        }
                        Err(e) => {
                            let _ = action_tx.send(Action::MetroSpawnFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::MetroHttpPost { url, body } => {
                let metro = self.adapters.metro.clone();
                tokio::spawn(async move {
                    if let Err(e) = metro.http_post(&url, &body).await {
                        tracing::warn!("metro http_post failed: {e}");
                    }
                });
            }

            Effect::KillProcess { pid } => {
                let probe = self.adapters.port_probe.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let _ = probe.kill_process(pid).await;
                    // Wait briefly for port to free, then auto-start metro.
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = tx.send(Action::MetroStartConfirmed);
                });
            }

            Effect::SpawnCommand { spec, cwd, branch } => {
                use crate::domain::ports::command_runner_port::CommandEvent;
                // F-101 consumer (Plan 13-08): the CommandEvent → Action
                // translation lives here — the canonical app-layer boundary.
                let runner = self.adapters.command_runner.clone();
                let mut rx = runner.spawn(spec, cwd, branch);
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
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
            }

            Effect::LoadDevices { kind } => {
                let devices = self.adapters.devices.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match devices.list(kind).await {
                        Ok(devs) => {
                            let _ = tx.send(Action::DevicesEnumerated(devs));
                        }
                        Err(e) => {
                            tracing::warn!("device enumeration failed: {e}");
                            let _ = tx.send(Action::DevicesEnumerated(vec![]));
                        }
                    }
                });
            }

            Effect::ListWorktrees { repo_root } => {
                let wt = self.adapters.worktrees.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match wt.list(&repo_root).await {
                        Ok(wts) => {
                            let _ = tx.send(Action::WorktreesLoaded(wts));
                        }
                        Err(e) => {
                            tracing::warn!("list_worktrees failed: {e}");
                        }
                    }
                });
            }

            Effect::RemoveWorktree { repo_root, path } => {
                let wt = self.adapters.worktrees.clone();
                let tx = self.action_tx.clone();
                let path_str = path.to_string_lossy().to_string();
                tokio::spawn(async move {
                    match wt.remove(&repo_root, &path).await {
                        Ok(()) => {
                            let _ = tx.send(Action::WorktreeRemoved(path_str));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeRemoveFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::AddWorktree { repo_root, branch } => {
                let wt = self.adapters.worktrees.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match wt.add(&repo_root, &branch).await {
                        Ok(path) => {
                            let _ = tx.send(Action::WorktreeAdded(
                                path.to_string_lossy().to_string(),
                            ));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeAddFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::AddWorktreeNewBranch { repo_root, new, base } => {
                let wt = self.adapters.worktrees.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match wt.add_new_branch(&repo_root, &new, &base).await {
                        Ok(path) => {
                            let _ = tx.send(Action::WorktreeNewBranchCreated(
                                path.to_string_lossy().to_string(),
                            ));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeNewBranchFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::ListRemoteBranches { repo_root } => {
                let wt = self.adapters.worktrees.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match wt.list_remote_branches(&repo_root).await {
                        Ok(branches) => {
                            let _ = tx.send(Action::BranchesLoaded(branches));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeNewBranchFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::FetchJiraTitles { keys } => {
                // Plan 13-08: closes the D-13-07-02 deferral.
                // adapters.jira is Option<Arc<dyn JiraPort>>; when None we
                // skip — update() also pre-checks state.jira.available, but
                // a defensive guard here keeps the runner robust.
                let Some(jira) = self.adapters.jira.clone() else {
                    return;
                };
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let mut fetched: Vec<(String, String)> = Vec::new();
                    for key in keys {
                        if let Some(title) = jira.fetch_title(&key).await {
                            fetched.push((key, title));
                        }
                    }
                    if !fetched.is_empty() {
                        let _ = tx.send(Action::JiraTitlesFetched(fetched));
                    }
                });
            }

            Effect::SaveJiraCache(cache) => {
                // F-111 PersistencePort deferred — direct infra call kept
                // behind the G-01 whitelist (see Makefile arch-lint).
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = crate::infra::jira_cache::save_jira_cache(&cache) {
                        tracing::warn!("save_jira_cache failed: {e}");
                    }
                });
            }

            Effect::SaveAndroidMode(mode) => {
                // F-111 deferred — see SaveJiraCache.
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = crate::infra::android_prefs::save_android_mode(&mode) {
                        tracing::warn!("save_android_mode failed: {e}");
                    }
                });
            }

            Effect::RecordSimUsed(udid) => {
                // F-111 deferred — see SaveJiraCache.
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = crate::infra::sim_history::record_sim_used(&udid) {
                        tracing::warn!("failed to save sim history: {e}");
                    }
                });
            }

            Effect::OpenInMultiplexer { worktree, name, command } => {
                let Some(mux) = self.adapters.multiplexer.clone() else {
                    return;
                };
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = mux.new_window(&worktree, &name, &command) {
                        tracing::warn!("multiplexer new_window failed: {e}");
                    }
                });
            }
        }
    }
}
