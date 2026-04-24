//! Effect interpreter — F-201 consumer boundary (Plan 13-07 partial; Plan
//! 13-08 completes the Adapters injection).
//!
//! `EffectRunner::run_effects` is the single boundary between the pure
//! TEA `update()` function and the tokio runtime. Every side-effect that
//! `update()` needs to trigger is returned as an `Effect` variant; this
//! interpreter spawns the actual tokio task for each one.
//!
//! Plan 13-07 scope: metro-related variants + dispatch for all 17 variants
//! (temporarily using direct `crate::infra::*` calls — G-01 is PENDING
//! until Plan 13-08 moves each infra call behind a port adapter in the
//! `Adapters` struct).
//!
//! The `metro` field is already typed against the `MetroPort` trait (from
//! Plan 13-03 / F-203) — this is the first consumer of that port.

#![allow(dead_code)]

use super::effect::Effect;
use crate::domain::action::Action;
use crate::domain::metro::MetroActivity;
use crate::domain::metro::MetroHandle;
use crate::domain::ports::device_port::DeviceKind;
use crate::domain::ports::metro_port::MetroPort;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

/// Effect interpreter. Owns the `MetroPort` adapter (Plan 13-07) + action /
/// handle channels. Plan 13-08 expands to a full `Adapters` struct that holds
/// port_probe, command_runner, worktrees, devices, jira, multiplexer.
pub struct EffectRunner {
    pub metro: Arc<dyn MetroPort>,
    pub action_tx: UnboundedSender<Action>,
    pub handle_tx: UnboundedSender<Box<dyn MetroHandle>>,
}

impl EffectRunner {
    pub fn new(
        metro: Arc<dyn MetroPort>,
        action_tx: UnboundedSender<Action>,
        handle_tx: UnboundedSender<Box<dyn MetroHandle>>,
    ) -> Self {
        Self { metro, action_tx, handle_tx }
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
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match crate::infra::port::detect_external_metro(port).await {
                        Some(info) => {
                            let _ = tx.send(Action::ExternalMetroDetected(
                                crate::domain::ports::port_probe_port::ExternalProcessInfo {
                                    pid: info.pid,
                                    working_dir: info.working_dir,
                                },
                            ));
                        }
                        None => {
                            let _ = tx.send(Action::MetroStartConfirmed);
                        }
                    }
                });
            }

            Effect::SpawnMetro { worktree } => {
                let metro = Arc::clone(&self.metro);
                let action_tx = self.action_tx.clone();
                let handle_tx = self.handle_tx.clone();
                let activity_tx = action_tx.clone();
                let exited_tx = action_tx.clone();
                tokio::spawn(async move {
                    // The on_activity callback bridges the callback-style
                    // MetroPort trait to the existing Action channel that
                    // update() consumes.
                    let on_activity: Box<dyn Fn(MetroActivity) + Send + Sync> =
                        Box::new(move |act| {
                            let _ = activity_tx.send(Action::MetroActivityUpdate(act));
                        });
                    match metro.start(worktree, on_activity).await {
                        Ok(handle) => {
                            // Deliver the handle via the dedicated channel so the
                            // event loop can call state.metro.register() on the
                            // main thread (AppState is not Send across the
                            // spawn boundary).
                            let _ = handle_tx.send(handle);
                            // Exit notification is not yet wired through the
                            // adapter — pre-13-07 the metro_process_task sent
                            // Action::MetroExited on the action channel, and the
                            // moved helper still emits an implicit exit signal
                            // when the drain loop ends. For now, rely on natural
                            // `state.metro.clear()` paths via MetroStop /
                            // explicit MetroExited from the caller.
                            let _ = exited_tx; // suppress unused warning
                        }
                        Err(e) => {
                            let _ = action_tx.send(Action::MetroSpawnFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::MetroHttpPost { url, body } => {
                let metro = Arc::clone(&self.metro);
                tokio::spawn(async move {
                    if let Err(e) = metro.http_post(&url, &body).await {
                        tracing::warn!("metro http_post failed: {e}");
                    }
                });
            }

            Effect::KillProcess { pid } => {
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let _ = crate::infra::port::kill_process(pid).await;
                    // Wait briefly for port to free, then auto-start metro
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    let _ = tx.send(Action::MetroStartConfirmed);
                });
            }

            Effect::SpawnCommand { spec, cwd, branch } => {
                use crate::domain::ports::command_runner_port::{CommandEvent, CommandRunnerPort};
                let runner = crate::infra::command_runner::TokioCommandRunner;
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
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let devices = match kind {
                        DeviceKind::Android => crate::infra::devices::list_android_devices().await,
                        DeviceKind::Ios => crate::infra::devices::list_ios_simulators().await,
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
            }

            Effect::ListWorktrees => {
                // repo_root is captured from the env via the well-known location in
                // the current working directory. Plan 13-08 will inject it via
                // Adapters; for now use crate::infra::worktrees with cwd semantics.
                let tx = self.action_tx.clone();
                let repo_root = std::env::current_dir().unwrap_or_default();
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

            Effect::RemoveWorktree { path } => {
                let tx = self.action_tx.clone();
                let repo_root = std::env::current_dir().unwrap_or_default();
                let path_str = path.to_string_lossy().to_string();
                tokio::spawn(async move {
                    match crate::infra::worktrees::remove_worktree(&repo_root, &path).await {
                        Ok(()) => {
                            let _ = tx.send(Action::WorktreeRemoved(path_str));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeRemoveFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::AddWorktree { branch } => {
                let tx = self.action_tx.clone();
                let repo_root = std::env::current_dir().unwrap_or_default();
                tokio::spawn(async move {
                    match crate::infra::worktrees::add_worktree(&repo_root, &branch).await {
                        Ok(path) => {
                            let _ = tx.send(Action::WorktreeAdded(path.to_string_lossy().to_string()));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeAddFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::AddWorktreeNewBranch { new, base } => {
                let tx = self.action_tx.clone();
                let repo_root = std::env::current_dir().unwrap_or_default();
                tokio::spawn(async move {
                    match crate::infra::worktrees::add_worktree_new_branch(&repo_root, &new, &base).await {
                        Ok(path) => {
                            let _ = tx.send(Action::WorktreeNewBranchCreated(path.to_string_lossy().to_string()));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeNewBranchFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::ListRemoteBranches => {
                let tx = self.action_tx.clone();
                let repo_root = std::env::current_dir().unwrap_or_default();
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

            Effect::FetchJiraTitles { keys } => {
                // JIRA fetch requires the Arc<dyn JiraPort> which lives in AppState.
                // Plan 13-08's Adapters struct will hold it; for now this effect is
                // emitted from update() only when state.jira_client is Some, but we
                // don't have access to it here. Fall back: emit a warning. Plan
                // 13-08 resolves this by adding jira to the EffectRunner struct.
                //
                // HACK for 13-07: We can't do the JIRA fetch here without access to
                // the Arc<dyn JiraPort>. Instead, update() will push
                // Effect::FetchJiraTitles with the keys, and we log. Plan 13-08
                // closes this gap when adapters.jira lands.
                tracing::debug!(
                    "Effect::FetchJiraTitles with {} keys — deferred to Plan 13-08 Adapters",
                    keys.len()
                );
            }

            Effect::SaveJiraCache(cache) => {
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = crate::infra::jira_cache::save_jira_cache(&cache) {
                        tracing::warn!("save_jira_cache failed: {e}");
                    }
                });
            }

            Effect::SaveAndroidMode(mode) => {
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = crate::infra::android_prefs::save_android_mode(&mode) {
                        tracing::warn!("save_android_mode failed: {e}");
                    }
                });
            }

            Effect::RecordSimUsed(udid) => {
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = crate::infra::sim_history::record_sim_used(&udid) {
                        tracing::warn!("failed to save sim history: {e}");
                    }
                });
            }

            Effect::OpenInMultiplexer { worktree, name, command } => {
                tokio::task::spawn_blocking(move || {
                    if let Some(mux) = crate::infra::multiplexer::detect_multiplexer()
                        && let Err(e) = mux.new_window(&worktree, &name, &command) {
                            tracing::warn!("multiplexer new_window failed: {e}");
                        }
                });
            }
        }
    }
}
