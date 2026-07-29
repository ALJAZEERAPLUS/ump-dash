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
//!   not implement) and the handle must reach the main thread for registration
//!   in the matching WorktreeSlice.
//!
//! Effect coverage map (every variant has a match arm below):
//!   ScheduleAction(a)                              → action_tx.send(a)
//!   SpawnTask { task_id, worktree_id, spec, cwd, branch } → adapters.command_runner.spawn(...) + task_handle_tx
//!   DetectExternalMetro { port }                   → adapters.port_probe.detect_external(port)
//!   SpawnMetro { worktree, port }                  → adapters.metro.start(worktree, port, on_activity)
//!   MetroHttpPost { url, body }                    → adapters.metro.http_post(url, body)
//!   KillProcess { pid }                            → adapters.port_probe.kill_process(pid)
//!   LoadDevices { kind, request_id }               → adapters.devices.list(kind)
//!   LookupIosSimulatorCache { worktree_id, worktree_path } → adapters.native_cache.lookup_ios_simulator(...)
//!   StoreIosSimulatorCache { worktree_id, request } → adapters.native_cache.store_ios_simulator(...)
//!   InstallAndLaunchCachedIosSimulator { worktree_id, request } → Task 6 implements launch behavior
//!   ListWorktrees { repo_root }                    → adapters.worktrees.list(repo_root)
//!   RemoveWorktree { repo_root, path }             → adapters.worktrees.remove(...)
//!   AddWorktree { repo_root, branch }              → adapters.worktrees.add(...)
//!   AddWorktreeNewBranch { repo_root, new, base }  → adapters.worktrees.add_new_branch(...)
//!   AddReviewWorktree { repo_root, pr, worktree_name } → adapters.worktrees.add_review_worktree(...)
//!   ListRemoteBranches { repo_root }               → adapters.worktrees.list_remote_branches(repo_root)
//!   ListPullRequests { repo_root, filter }          → adapters.review.list_pull_requests(repo_root, filter)
//!   FetchJiraTitles { keys }                       → adapters.jira.as_ref()?.fetch_title(...)
//!   SaveJiraCache(map)                             → spawn_blocking infra::jira_cache::save_jira_cache  (F-111 deferred)
//!   RecordSimUsed(udid)                            → spawn_blocking infra::sim_history::record_sim_used  (F-111 deferred)
//!   OpenInMultiplexer { worktree, name, command }  → adapters.multiplexer.as_ref()?.new_window(...)
//!   OpenExternalEditor { command }                 → adapters.external_command.run_shell_command(...)
//!   OpenInFinder { path }                          → adapters.external_command.open_in_finder(...)
//!
//! G-01 carve-out (whitelisted in `Makefile` arch-lint): the two
//! persistence variants (SaveJiraCache, RecordSimUsed) still
//! call `infra::<module>::save_*` directly. F-111 (PersistencePort) is
//! deferred — when it lands those two lines route through
//! `adapters.persistence` and the whitelist disappears.

#![allow(dead_code)]

use super::adapters::Adapters;
use super::effect::Effect;
use crate::domain::action::Action;
use crate::domain::metro::MetroActivity;
use crate::domain::metro::MetroHandle;
use tokio::sync::mpsc::UnboundedSender;

fn forward_metro_activity(
    worktree_id: &str,
    activity: MetroActivity,
    activity_tx: &UnboundedSender<Action>,
    exited_tx: &UnboundedSender<Action>,
) {
    if matches!(&activity, MetroActivity::Exited) {
        let _ = exited_tx.send(Action::MetroExited(worktree_id.to_string()));
    }
    let _ = activity_tx.send(Action::MetroActivityUpdate {
        worktree_id: worktree_id.to_string(),
        activity,
    });
}

/// Effect interpreter. Owns the `Adapters` bundle (Plan 13-08) + the action
/// stream + the handle-delivery channel.
pub struct EffectRunner {
    pub adapters: Adapters,
    pub action_tx: UnboundedSender<Action>,
    pub handle_tx: UnboundedSender<Box<dyn MetroHandle>>,
    /// Phase 14 / D-06 + Q2 + Q3 lock: dedicated channel for delivering
    /// freshly-spawned `TaskRecord`s to the main-thread receiver in
    /// `runtime.rs`. Mirrors `handle_tx` for `Box<dyn MetroHandle>` because
    /// neither `TaskRecord` (carries `Box<dyn TaskHandle>`) nor
    /// `Box<dyn MetroHandle>` are `Clone + PartialEq` — incompatible with the
    /// `Action` enum's derives.
    ///
    /// Single ownership: the `TaskRecord` (and its `handle: Box<dyn TaskHandle>`)
    /// lives in `slice.task` after delivery. EffectRunner does NOT keep a
    /// JoinHandle map — Phase 15's CommandCancel reads `slice.task.take().handle`.
    pub task_handle_tx: UnboundedSender<(
        crate::domain::worktree::WorktreeId,
        crate::domain::task::TaskRecord,
    )>,

    /// Plan 15-03 / TASK-06: per-repo-root yarn-family install semaphore.
    /// Keyed by the canonicalized `repo_root` of `Effect::SpawnTask`; each
    /// entry is a `Semaphore(1)` that serializes concurrent `YarnInstall`,
    /// `YarnPodInstall`, and `RmNodeModules` invocations across worktrees
    /// sharing the same upstream repo (they all write/delete `node_modules`).
    /// Non-yarn specs skip the lookup entirely.
    ///
    /// `std::sync::Mutex` is intentional (not `tokio::sync::Mutex`) — the
    /// guard is held only across a synchronous HashMap insert/clone and is
    /// dropped before any `.await`. See 15-RESEARCH §Pitfall 4 + §Pattern 4.
    /// The Rust compiler enforces this — `MutexGuard` is `!Send`.
    pub yarn_semaphores: std::sync::Mutex<
        std::collections::HashMap<std::path::PathBuf, std::sync::Arc<tokio::sync::Semaphore>>,
    >,

    /// MCP correlation sink. When set (by `runtime.rs` if the embedded MCP server
    /// is enabled), `Effect::AgentReply` forwards `(request_id, outcome)` here.
    /// The infra MCP server drains it and resolves the waiting tool call's
    /// oneshot. `None` when the MCP server is disabled — replies are dropped.
    pub agent_reply_tx: Option<
        UnboundedSender<(
            crate::domain::agent_protocol::AgentRequestId,
            crate::domain::agent_protocol::AgentOutcome,
        )>,
    >,
}

impl EffectRunner {
    pub fn new(
        adapters: Adapters,
        action_tx: UnboundedSender<Action>,
        handle_tx: UnboundedSender<Box<dyn MetroHandle>>,
        task_handle_tx: UnboundedSender<(
            crate::domain::worktree::WorktreeId,
            crate::domain::task::TaskRecord,
        )>,
    ) -> Self {
        Self {
            adapters,
            action_tx,
            handle_tx,
            task_handle_tx,
            yarn_semaphores: std::sync::Mutex::new(std::collections::HashMap::new()),
            agent_reply_tx: None,
        }
    }

    /// Attach the MCP correlation sink. Called by `runtime.rs` when the embedded
    /// MCP server is enabled; `Effect::AgentReply` is dropped until this is set.
    pub fn with_agent_reply_tx(
        mut self,
        tx: UnboundedSender<(
            crate::domain::agent_protocol::AgentRequestId,
            crate::domain::agent_protocol::AgentOutcome,
        )>,
    ) -> Self {
        self.agent_reply_tx = Some(tx);
        self
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

            Effect::SpawnMetro { worktree, port } => {
                let metro = self.adapters.metro.clone();
                let action_tx = self.action_tx.clone();
                let handle_tx = self.handle_tx.clone();
                let activity_tx = action_tx.clone();
                let exited_tx = action_tx.clone();
                tokio::spawn(async move {
                    let worktree_id = worktree
                        .file_name()
                        .map(|name| name.to_string_lossy().to_string())
                        .unwrap_or_else(|| "unknown".to_string());
                    // The on_activity callback bridges the callback-style
                    // MetroPort trait to the existing Action channel that
                    // update() consumes. Metro stderr errors are normal activity;
                    // only the adapter's explicit Exited activity clears the
                    // registered handle.
                    let on_activity_worktree_id = worktree_id.clone();
                    let on_activity: Box<dyn Fn(MetroActivity) + Send + Sync> =
                        Box::new(move |act| {
                            forward_metro_activity(
                                &on_activity_worktree_id,
                                act,
                                &activity_tx,
                                &exited_tx,
                            );
                        });
                    match metro.start(worktree, port, on_activity).await {
                        Ok(handle) => {
                            // Deliver via the dedicated handle channel — the
                            // event loop registers the handle into the
                            // matching WorktreeSlice on the main thread.
                            // AppState is not Send across the spawn boundary.
                            let _ = handle_tx.send(handle);
                        }
                        Err(e) => {
                            let _ = action_tx.send(Action::MetroSpawnFailed {
                                worktree_id,
                                message: e.to_string(),
                            });
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

            Effect::LoadDevices { kind, request_id } => {
                let devices = self.adapters.devices.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match devices.list(kind).await {
                        Ok(devs) => {
                            let _ = tx.send(Action::DevicesEnumerated {
                                kind,
                                request_id,
                                devices: devs,
                            });
                        }
                        Err(e) => {
                            tracing::warn!("device enumeration failed: {e}");
                            let _ = tx.send(Action::DevicesEnumerated {
                                kind,
                                request_id,
                                devices: vec![],
                            });
                        }
                    }
                });
            }

            Effect::LookupIosSimulatorCache {
                worktree_id,
                worktree_path,
            } => {
                let native_cache = self.adapters.native_cache.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let result = native_cache
                        .lookup_ios_simulator(worktree_path)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Action::IosSimulatorCacheLookupFinished {
                        worktree_id,
                        result,
                    });
                });
            }

            Effect::StoreIosSimulatorCache {
                worktree_id,
                request,
            } => {
                let native_cache = self.adapters.native_cache.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let result = native_cache
                        .store_ios_simulator(request)
                        .await
                        .map(|hit| {
                            crate::domain::native_cache::IosSimulatorCacheLookup::Hit(Box::new(hit))
                        })
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Action::IosSimulatorCacheLookupFinished {
                        worktree_id,
                        result,
                    });
                });
            }

            Effect::LookupAndroidCache {
                worktree_id,
                worktree_path,
            } => {
                let native_cache = self.adapters.native_cache.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let result = native_cache
                        .lookup_android(worktree_path)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Action::AndroidCacheLookupFinished {
                        worktree_id,
                        result,
                    });
                });
            }

            Effect::StoreAndroidCache {
                worktree_id,
                request,
            } => {
                let native_cache = self.adapters.native_cache.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let result = native_cache
                        .store_android(request)
                        .await
                        .map(|hit| {
                            crate::domain::native_cache::AndroidCacheLookup::Hit(Box::new(hit))
                        })
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Action::AndroidCacheLookupFinished {
                        worktree_id,
                        result,
                    });
                });
            }

            Effect::InstallAndLaunchCachedIosSimulator {
                worktree_id,
                request,
            } => {
                let native_cache = self.adapters.native_cache.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let fallback_device_id = request.simulator_udid.clone();
                    let fallback_variant = request.variant;
                    let result = match native_cache.install_and_launch_ios_simulator(request).await
                    {
                        Ok(lines) => {
                            crate::domain::native_cache::CachedIosLaunchResult::Success(lines)
                        }
                        Err(e) => {
                            if e.downcast_ref::<
                                crate::domain::native_cache::CachedArtifactValidationError,
                            >()
                            .is_some()
                            {
                                crate::domain::native_cache::CachedIosLaunchResult::InvalidArtifact {
                                    message: e.to_string(),
                                    device_id: fallback_device_id,
                                    variant: fallback_variant,
                                }
                            } else {
                                crate::domain::native_cache::CachedIosLaunchResult::Failure(
                                    e.to_string(),
                                )
                            }
                        }
                    };
                    let _ = tx.send(Action::CachedIosLaunchFinished {
                        worktree_id,
                        result,
                    });
                });
            }

            Effect::InstallAndLaunchCachedAndroid {
                worktree_id,
                request,
            } => {
                let native_cache = self.adapters.native_cache.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    let fallback_device_id = request.device_id.clone();
                    let fallback_variant = request.variant;
                    let result = match native_cache.install_and_launch_android(request).await {
                        Ok(lines) => {
                            crate::domain::native_cache::CachedAndroidLaunchResult::Success(lines)
                        }
                        Err(e) => {
                            if e.downcast_ref::<
                                crate::domain::native_cache::CachedArtifactValidationError,
                            >()
                            .is_some()
                            {
                                crate::domain::native_cache::CachedAndroidLaunchResult::InvalidArtifact {
                                    message: e.to_string(),
                                    device_id: fallback_device_id,
                                    variant: fallback_variant,
                                }
                            } else {
                                crate::domain::native_cache::CachedAndroidLaunchResult::Failure(
                                    e.to_string(),
                                )
                            }
                        }
                    };
                    let _ = tx.send(Action::CachedAndroidLaunchFinished {
                        worktree_id,
                        result,
                    });
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
                            let _ =
                                tx.send(Action::WorktreeAdded(path.to_string_lossy().to_string()));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::WorktreeAddFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::AddWorktreeNewBranch {
                repo_root,
                new,
                base,
            } => {
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

            Effect::ListPullRequests { repo_root, filter } => {
                let review = self.adapters.review.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match review.list_pull_requests(&repo_root, filter).await {
                        Ok(pull_requests) => {
                            let _ = tx.send(Action::PullRequestsLoaded(pull_requests));
                        }
                        Err(e) => {
                            let _ = tx.send(Action::PullRequestsLoadFailed(e.to_string()));
                        }
                    }
                });
            }

            Effect::AddReviewWorktree {
                repo_root,
                pr,
                worktree_name,
            } => {
                let wt = self.adapters.worktrees.clone();
                let tx = self.action_tx.clone();
                tokio::spawn(async move {
                    match wt
                        .add_review_worktree(
                            &repo_root,
                            pr.number,
                            &pr.head_ref_name,
                            &pr.head_ref_oid,
                            &worktree_name,
                        )
                        .await
                    {
                        Ok(path) => {
                            let head_sha = pr.head_ref_oid.chars().take(7).collect::<String>();
                            let _ = tx.send(Action::ReviewWorktreeCreated {
                                branch: pr.head_ref_name,
                                path,
                                head_sha,
                            });
                        }
                        Err(e) => {
                            let _ = tx.send(Action::ReviewWorktreeCreateFailed(e.to_string()));
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

            Effect::RecordSimUsed(udid) => {
                // F-111 deferred — see SaveJiraCache.
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = crate::infra::sim_history::record_sim_used(&udid) {
                        tracing::warn!("failed to save sim history: {e}");
                    }
                });
            }

            Effect::OpenInMultiplexer {
                worktree,
                name,
                command,
            } => {
                let Some(mux) = self.adapters.multiplexer.clone() else {
                    return;
                };
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = mux.new_window(&worktree, &name, &command) {
                        tracing::warn!("multiplexer new_window failed: {e}");
                    }
                });
            }
            Effect::OpenExternalEditor { command } => {
                let external_command = self.adapters.external_command.clone();
                let tx = self.action_tx.clone();
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = external_command.run_shell_command(&command) {
                        let _ = tx.send(Action::OpenEditorFailed(e.to_string()));
                    }
                });
            }
            Effect::OpenInFinder { path } => {
                let external_command = self.adapters.external_command.clone();
                let tx = self.action_tx.clone();
                tokio::task::spawn_blocking(move || {
                    if let Err(e) = external_command.open_in_finder(&path) {
                        let _ = tx.send(Action::OpenFinderFailed(e.to_string()));
                    }
                });
            }

            // Plan 14-06 / Plan 15-03: per-task spawn chokepoint (D-10, D-20, Q1, Q2, Q3 + TASK-04, TASK-06).
            //
            // Phase 15 wiring (Plan 15-03):
            //   - Reads CommandEvent::ProcessStarted { pid } as the FIRST event
            //     and delivers it via tokio::sync::oneshot to the TaskRecord
            //     assembly task, which builds TokioTaskHandle with the real pid.
            //   - Threads a tokio_util::sync::CancellationToken into the
            //     forwarding loop's tokio::select! so abort() can fire
            //     Action::CommandExited { status: Cancelled } without waiting
            //     for the OS wait() to return.
            //   - For yarn-family specs (YarnInstall, YarnPodInstall,
            //     RmNodeModules) acquires an OwnedSemaphorePermit from the
            //     per-canonicalized-repo-root Semaphore(1) BEFORE invoking
            //     runner.spawn() — serializing concurrent installs across
            //     sibling worktrees. Non-yarn specs skip the semaphore.
            Effect::SpawnTask {
                task_id,
                worktree_id,
                spec,
                cwd,
                branch,
                repo_root,
            } => {
                use crate::domain::ports::command_runner_port::CommandEvent;
                use crate::domain::task::{ExitStatus, TaskRecord};

                let runner = self.adapters.command_runner.clone();
                // D-06: started_at captured at the runner's spawn moment, NOT in update().
                let started_at = std::time::Instant::now();

                // Cancellation plumbing — shared between the abort() ladder
                // (held by TokioTaskHandle) and the forwarding loop below.
                let cancel_token = tokio_util::sync::CancellationToken::new();
                let cancel_token_for_loop = cancel_token.clone();

                // Yarn-family predicate. Only these three specs serialize via
                // the per-repo-root semaphore (15-RESEARCH §F8) — they all
                // mutate node_modules under the worktree.
                let is_yarn_family = matches!(
                    spec,
                    crate::domain::command::CommandSpec::YarnInstall
                        | crate::domain::command::CommandSpec::YarnPodInstall
                        | crate::domain::command::CommandSpec::RmNodeModules
                );

                // Canonicalize the repo_root so semantically-equal paths
                // (./foo vs /abs/foo, symlinks, trailing slashes) hash to the
                // same HashMap bucket. Fall back to the raw path on errors
                // (NFS, missing dir at start-of-day — 15-RESEARCH §Pattern 4).
                let canonical_repo_root = repo_root
                    .canonicalize()
                    .unwrap_or_else(|_| repo_root.clone());

                // Look up (or create) the per-repo-root semaphore.
                // CRITICAL (15-RESEARCH §Pitfall 4): the std::sync::Mutex guard
                // MUST be dropped before any `.await` because MutexGuard is
                // `!Send`. The explicit `let mut map = ...; ... .clone()` in
                // a scope block forces the guard to drop at the closing brace
                // — Rust's compiler enforces the rest.
                let semaphore_opt: Option<std::sync::Arc<tokio::sync::Semaphore>> =
                    if is_yarn_family {
                        let mut map = self.yarn_semaphores.lock().unwrap();
                        Some(
                            map.entry(canonical_repo_root.clone())
                                .or_insert_with(|| {
                                    std::sync::Arc::new(tokio::sync::Semaphore::new(1))
                                })
                                .clone(),
                        )
                    } else {
                        None
                    };
                // MutexGuard dropped here — safe to `.await` below.

                // PID delivery (15-RESEARCH §F2 Option B): the forwarding task
                // reads CommandEvent::ProcessStarted { pid } as its first event
                // and ships the pid via a oneshot to the assembly task that
                // builds the TaskRecord. The assembly task wraps the oneshot
                // in a 5-second timeout so the spawn-failure path (oneshot
                // sender dropped) does not leak an awaiter (T-15-03-05).
                let (pid_tx, pid_rx) = tokio::sync::oneshot::channel::<u32>();
                // Do not forward output/exit before the main loop can receive
                // the TaskRecord; fast commands otherwise lose queue ownership.
                let (record_ready_tx, record_ready_rx) = tokio::sync::oneshot::channel::<()>();

                let tx = self.action_tx.clone();
                let spec_for_record = spec.clone();

                // D-10: per-task closure capture. TaskId is Copy; async move
                // captures it so concurrent spawns interleave correctly
                // (RESEARCH P-2). The cancel_token is cloned into the loop's
                // own variable above so the outer scope can still hand the
                // original to TokioTaskHandle.
                let join_handle = tokio::spawn(async move {
                    // Step A: acquire the yarn-family semaphore permit BEFORE
                    // invoking runner.spawn() so the subprocess does not start
                    // until our turn comes up. OwnedSemaphorePermit is Drop —
                    // released on task exit, abort, OR panic (T-15-03-03).
                    let _permit: Option<tokio::sync::OwnedSemaphorePermit> =
                        if let Some(sem) = semaphore_opt {
                            match sem.acquire_owned().await {
                                Ok(p) => Some(p),
                                Err(_closed) => {
                                    // Semaphore was closed — should never happen
                                    // (we never call .close()) but if it does
                                    // we still want to surface a clean cancel
                                    // rather than silently hang.
                                    let _ = tx.send(Action::CommandExited {
                                        task_id,
                                        status: ExitStatus::Cancelled,
                                    });
                                    return;
                                }
                            }
                        } else {
                            None
                        };

                    // Step B: now spawn the subprocess. Per the port contract
                    // (command_runner_port.rs), ProcessStarted arrives BEFORE
                    // any OutputLine on the success path; spawn-failure path
                    // skips ProcessStarted and goes straight to Exited.
                    let mut rx = runner.spawn(spec.clone(), cwd, branch);

                    // Step C: consume the first event. Forward pid via the
                    // oneshot so the assembly task can build the TaskRecord.
                    // On spawn-failure (first event is Exited / channel
                    // closes), we drop pid_tx — the assembly task's timeout
                    // wrapper will resolve with the placeholder pid=0, which
                    // abort()'s `pid <= 1` guard makes a no-op (Plan 15-02).
                    let _child_pid: u32 = match rx.recv().await {
                        Some(CommandEvent::ProcessStarted { pid }) => {
                            let _ = pid_tx.send(pid);
                            pid
                        }
                        Some(CommandEvent::Exited(status)) => {
                            // Spawn-failure fast path — runner emitted
                            // a synthetic Exited. Forward it and bail.
                            let _ = tx.send(Action::CommandExited {
                                task_id,
                                status: ExitStatus::from(status),
                            });
                            return;
                        }
                        Some(CommandEvent::OutputLine(_)) | None => {
                            // Either the contract was violated (OutputLine
                            // before ProcessStarted) or the channel closed
                            // immediately. Either way, treat as failure.
                            let _ = tx.send(Action::CommandExited {
                                task_id,
                                status: ExitStatus::Failure { code: None },
                            });
                            return;
                        }
                    };

                    let _ = record_ready_rx.await;

                    // Step D: forwarding loop with cancel select. The cancel
                    // arm fires Action::CommandExited { Cancelled } and breaks
                    // immediately — the OS-level Exited(status) that arrives
                    // later (after abort()'s SIGTERM → 200ms → SIGKILL ladder
                    // resolves) is dropped because the loop has exited. Phase
                    // 14 D-08 stale-task-drop in update.rs handles any race
                    // where a second CommandExited would arrive.
                    loop {
                        tokio::select! {
                            maybe_ev = rx.recv() => {
                                match maybe_ev {
                                    Some(CommandEvent::OutputLine(line)) => {
                                        if tx.send(Action::CommandLogLine { task_id, line }).is_err() {
                                            break;
                                        }
                                    }
                                    Some(CommandEvent::Exited(status)) => {
                                        let _ = tx.send(Action::CommandExited {
                                            task_id,
                                            status: ExitStatus::from(status),
                                        });
                                        break;
                                    }
                                    Some(CommandEvent::ProcessStarted { .. }) => {
                                        // Spec says exactly one — ignore stragglers.
                                    }
                                    None => break,
                                }
                            }
                            _ = cancel_token_for_loop.cancelled() => {
                                let _ = tx.send(Action::CommandExited {
                                    task_id,
                                    status: ExitStatus::Cancelled,
                                });
                                break;
                            }
                        }
                    }
                    // _permit drops here — next queued yarn install can proceed.
                });

                // Assembly task: wait for the real child pid (with a 5s hard
                // timeout to handle the spawn-failure path where pid_tx was
                // dropped without sending — T-15-03-05) then build the
                // TaskRecord and deliver it via the dedicated channel. The
                // main thread writes it into slice.task. Single ownership —
                // no JoinHandle map here.
                let task_handle_tx = self.task_handle_tx.clone();
                tokio::spawn(async move {
                    let real_child_pid =
                        match tokio::time::timeout(std::time::Duration::from_secs(5), pid_rx).await
                        {
                            Ok(Ok(pid)) => pid,
                            // Timeout OR sender dropped (spawn failure). Use 0 —
                            // abort()'s `pid <= 1` guard makes the resulting kill
                            // a no-op (Plan 15-02 / T-15-03-05).
                            _ => 0,
                        };
                    let record = TaskRecord {
                        id: task_id,
                        spec: spec_for_record,
                        started_at,
                        handle: Box::new(crate::infra::task_handle::TokioTaskHandle {
                            join_handle,
                            child_pid: real_child_pid,
                            cancel_token: cancel_token.clone(),
                        }),
                    };
                    let _ = task_handle_tx.send((worktree_id, record));
                    let _ = record_ready_tx.send(());
                });
            }

            Effect::AgentReply {
                request_id,
                outcome,
            } => {
                // Forward to the infra MCP correlation registry (if the server is
                // enabled). Synchronous send — no spawn, no .await.
                if let Some(tx) = &self.agent_reply_tx {
                    let _ = tx.send((request_id, outcome));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::adapters::Adapters;
    use crate::domain::command::{CommandSpec, DeviceInfo};
    use crate::domain::native_cache::{
        AndroidCacheHit, AndroidCacheLookup, AndroidCacheStoreRequest, CachedAndroidLaunchRequest,
        CachedAndroidLaunchResult, CachedIosLaunchRequest, CachedIosLaunchResult,
        IOS_APP_ARTIFACT_KIND, IOS_SIMULATOR_PLATFORM, IosSimulatorCacheHit,
        IosSimulatorCacheLookup, IosSimulatorCacheMetadata, IosSimulatorCacheStoreRequest,
    };
    use crate::domain::ports::command_runner_port::{CommandEvent, CommandRunnerPort};
    use crate::domain::ports::device_port::{DeviceKind, DevicePort};
    use crate::domain::ports::external_command_port::ExternalCommandPort;
    use crate::domain::ports::metro_port::{MetroHandle, MetroPort};
    use crate::domain::ports::native_cache_port::NativeCachePort;
    use crate::domain::ports::port_probe_port::{ExternalProcessInfo, PortProbePort};
    use crate::domain::ports::review_port::ReviewPort;
    use crate::domain::ports::worktree_port::WorktreePort;
    use crate::domain::review::{PullRequest, PullRequestFilter};
    use crate::domain::worktree::{Worktree, WorktreeId};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tokio::sync::mpsc::UnboundedReceiver;

    fn collect_forwarded_actions(activity: MetroActivity) -> Vec<Action> {
        let (activity_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let exited_tx = activity_tx.clone();

        forward_metro_activity("wt-a", activity, &activity_tx, &exited_tx);

        let mut actions = Vec::new();
        while let Ok(action) = action_rx.try_recv() {
            actions.push(action);
        }
        actions
    }

    #[test]
    fn metro_error_activity_does_not_emit_metro_exited() {
        let actions = collect_forwarded_actions(MetroActivity::Error("bundle failed".to_string()));

        assert!(
            actions.contains(&Action::MetroActivityUpdate {
                worktree_id: "wt-a".to_string(),
                activity: MetroActivity::Error("bundle failed".to_string()),
            }),
            "expected Metro error output to be forwarded as activity; got {actions:?}"
        );
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::MetroExited(_))),
            "Metro error output must not be treated as process exit; got {actions:?}"
        );
    }

    #[test]
    fn metro_exited_activity_emits_metro_exited() {
        let actions = collect_forwarded_actions(MetroActivity::Exited);

        assert!(
            actions
                .iter()
                .any(|action| matches!(action, Action::MetroExited(id) if id == "wt-a")),
            "Metro exit activity must notify the state machine; got {actions:?}"
        );
    }

    #[derive(Debug)]
    struct NoopCommandRunner;

    impl CommandRunnerPort for NoopCommandRunner {
        fn spawn(
            &self,
            _spec: CommandSpec,
            _cwd: PathBuf,
            _branch: String,
        ) -> UnboundedReceiver<CommandEvent> {
            let (_tx, rx) = tokio::sync::mpsc::unbounded_channel();
            rx
        }
    }

    #[derive(Debug)]
    struct FastExitCommandRunner;

    impl CommandRunnerPort for FastExitCommandRunner {
        fn spawn(
            &self,
            _spec: CommandSpec,
            _cwd: PathBuf,
            _branch: String,
        ) -> UnboundedReceiver<CommandEvent> {
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg("true")
                .status()
                .expect("test should create a successful exit status");
            tx.send(CommandEvent::ProcessStarted { pid: 4242 })
                .expect("receiver should be open");
            tx.send(CommandEvent::Exited(status))
                .expect("receiver should be open");
            rx
        }
    }

    #[derive(Debug)]
    struct NoopMetroHandle;

    impl MetroHandle for NoopMetroHandle {
        fn pid(&self) -> u32 {
            0
        }

        fn worktree_id(&self) -> &str {
            "noop"
        }

        fn port(&self) -> u16 {
            0
        }

        fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
            Ok(())
        }

        fn kill(self: Box<Self>) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct NoopMetro;

    #[async_trait::async_trait]
    impl MetroPort for NoopMetro {
        async fn start(
            &self,
            _worktree: PathBuf,
            _port: u16,
            _on_activity: Box<dyn Fn(MetroActivity) + Send + Sync>,
        ) -> anyhow::Result<Box<dyn MetroHandle>> {
            Ok(Box::new(NoopMetroHandle))
        }

        async fn http_post(&self, _path: &str, _body: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct NoopProbe;

    #[async_trait::async_trait]
    impl PortProbePort for NoopProbe {
        fn port_is_free(&self, _port: u16) -> bool {
            true
        }

        async fn detect_external(&self, _port: u16) -> Option<ExternalProcessInfo> {
            None
        }

        async fn kill_process(&self, _pid: u32) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct NoopWorktrees;

    #[async_trait::async_trait]
    impl WorktreePort for NoopWorktrees {
        async fn list(&self, _repo_root: &Path) -> anyhow::Result<Vec<Worktree>> {
            Ok(Vec::new())
        }

        async fn remove(&self, _repo_root: &Path, _worktree_path: &Path) -> anyhow::Result<()> {
            Ok(())
        }

        async fn add(&self, _repo_root: &Path, _branch_name: &str) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from("/tmp/noop"))
        }

        async fn add_new_branch(
            &self,
            _repo_root: &Path,
            _new_branch: &str,
            _base_branch: &str,
        ) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from("/tmp/noop"))
        }

        async fn add_review_worktree(
            &self,
            _repo_root: &Path,
            _pr_number: u64,
            _branch_name: &str,
            _head_oid: &str,
            _worktree_name: &str,
        ) -> anyhow::Result<PathBuf> {
            Ok(PathBuf::from("/tmp/noop"))
        }

        async fn list_remote_branches(&self, _repo_root: &Path) -> anyhow::Result<Vec<String>> {
            Ok(Vec::new())
        }
    }

    struct NoopReview;

    #[async_trait::async_trait]
    impl ReviewPort for NoopReview {
        async fn list_pull_requests(
            &self,
            _repo_root: &Path,
            _filter: PullRequestFilter,
        ) -> anyhow::Result<Vec<PullRequest>> {
            Ok(Vec::new())
        }
    }

    struct NoopDevices;

    #[async_trait::async_trait]
    impl DevicePort for NoopDevices {
        async fn list(&self, _kind: DeviceKind) -> anyhow::Result<Vec<DeviceInfo>> {
            Ok(Vec::new())
        }
    }

    #[derive(Debug)]
    struct NoopExternalCommand;

    impl ExternalCommandPort for NoopExternalCommand {
        fn run_shell_command(&self, _command: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn open_in_finder(&self, _path: &Path) -> anyhow::Result<()> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct RecordingExternalCommand {
        paths: Arc<Mutex<Vec<PathBuf>>>,
        failure: Option<String>,
    }

    impl ExternalCommandPort for RecordingExternalCommand {
        fn run_shell_command(&self, _command: &str) -> anyhow::Result<()> {
            Ok(())
        }

        fn open_in_finder(&self, path: &Path) -> anyhow::Result<()> {
            self.paths.lock().unwrap().push(path.to_path_buf());
            if let Some(message) = &self.failure {
                anyhow::bail!(message.clone());
            }
            Ok(())
        }
    }

    enum NativeScript {
        LookupIos(Result<IosSimulatorCacheLookup, String>),
        StoreIos(Result<IosSimulatorCacheHit, String>),
        LaunchIos(Result<Vec<String>, String>),
        LaunchIosValidationError(String),
        LookupAndroid(Result<AndroidCacheLookup, String>),
        StoreAndroid(Result<AndroidCacheHit, String>),
        LaunchAndroid(Result<Vec<String>, String>),
        LaunchAndroidValidationError(String),
    }

    struct ScriptedNativeCache {
        script: Mutex<Option<NativeScript>>,
    }

    impl ScriptedNativeCache {
        fn new(script: NativeScript) -> Self {
            Self {
                script: Mutex::new(Some(script)),
            }
        }

        fn take(&self) -> NativeScript {
            self.script
                .lock()
                .unwrap()
                .take()
                .expect("native cache fake should be called once")
        }
    }

    fn scripted_result<T>(result: Result<T, String>) -> anyhow::Result<T> {
        result.map_err(|message| anyhow::anyhow!(message))
    }

    #[async_trait::async_trait]
    impl NativeCachePort for ScriptedNativeCache {
        async fn lookup_ios_simulator(
            &self,
            _worktree_path: PathBuf,
        ) -> anyhow::Result<IosSimulatorCacheLookup> {
            let NativeScript::LookupIos(result) = self.take() else {
                panic!("expected LookupIos script");
            };
            scripted_result(result)
        }

        async fn store_ios_simulator(
            &self,
            _request: IosSimulatorCacheStoreRequest,
        ) -> anyhow::Result<IosSimulatorCacheHit> {
            let NativeScript::StoreIos(result) = self.take() else {
                panic!("expected StoreIos script");
            };
            scripted_result(result)
        }

        async fn lookup_android(
            &self,
            _worktree_path: PathBuf,
        ) -> anyhow::Result<AndroidCacheLookup> {
            let NativeScript::LookupAndroid(result) = self.take() else {
                panic!("expected LookupAndroid script");
            };
            scripted_result(result)
        }

        async fn store_android(
            &self,
            _request: AndroidCacheStoreRequest,
        ) -> anyhow::Result<AndroidCacheHit> {
            let NativeScript::StoreAndroid(result) = self.take() else {
                panic!("expected StoreAndroid script");
            };
            scripted_result(result)
        }

        async fn install_and_launch_ios_simulator(
            &self,
            _request: CachedIosLaunchRequest,
        ) -> anyhow::Result<Vec<String>> {
            match self.take() {
                NativeScript::LaunchIos(result) => scripted_result(result),
                NativeScript::LaunchIosValidationError(message) => Err(
                    crate::domain::native_cache::CachedArtifactValidationError { message }.into(),
                ),
                _ => panic!("expected LaunchIos script"),
            }
        }

        async fn install_and_launch_android(
            &self,
            _request: CachedAndroidLaunchRequest,
        ) -> anyhow::Result<Vec<String>> {
            match self.take() {
                NativeScript::LaunchAndroid(result) => scripted_result(result),
                NativeScript::LaunchAndroidValidationError(message) => Err(
                    crate::domain::native_cache::CachedArtifactValidationError { message }.into(),
                ),
                _ => panic!("expected LaunchAndroid script"),
            }
        }
    }

    fn ios_hit() -> IosSimulatorCacheHit {
        IosSimulatorCacheHit {
            metadata: IosSimulatorCacheMetadata {
                platform: IOS_SIMULATOR_PLATFORM.into(),
                fingerprint: "ios-fp".into(),
                bundle_id: "com.aljazeera.test".into(),
                variant: "Debug".into(),
                created_at: "1".into(),
                source_worktree: "wt-a".into(),
                artifact_kind: IOS_APP_ARTIFACT_KIND.into(),
                storage_mode: "copy".into(),
                source_artifact_path: PathBuf::from("/tmp/wt-a/app.app"),
                artifact_digest_algorithm: "sha256".into(),
                artifact_digest: "digest".into(),
            },
            artifact_path: PathBuf::from("/tmp/app"),
        }
    }

    fn runner_with_script(script: NativeScript) -> (EffectRunner, UnboundedReceiver<Action>) {
        let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel();
        let (handle_tx, _handle_rx) = tokio::sync::mpsc::unbounded_channel();
        let (task_handle_tx, _task_handle_rx) = tokio::sync::mpsc::unbounded_channel();
        let runner = EffectRunner::new(
            Adapters {
                command_runner: Arc::new(NoopCommandRunner),
                metro: Arc::new(NoopMetro),
                port_probe: Arc::new(NoopProbe),
                worktrees: Arc::new(NoopWorktrees),
                devices: Arc::new(NoopDevices),
                native_cache: Arc::new(ScriptedNativeCache::new(script)),
                external_command: Arc::new(NoopExternalCommand),
                review: Arc::new(NoopReview),
                jira: None,
                multiplexer: None,
                mcp_server: None,
            },
            action_tx,
            handle_tx,
            task_handle_tx,
        );
        (runner, action_rx)
    }

    fn runner_with_external_command(
        external_command: Arc<dyn ExternalCommandPort>,
    ) -> (EffectRunner, UnboundedReceiver<Action>) {
        let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel();
        let (handle_tx, _handle_rx) = tokio::sync::mpsc::unbounded_channel();
        let (task_handle_tx, _task_handle_rx) = tokio::sync::mpsc::unbounded_channel();
        let runner = EffectRunner::new(
            Adapters {
                command_runner: Arc::new(NoopCommandRunner),
                metro: Arc::new(NoopMetro),
                port_probe: Arc::new(NoopProbe),
                worktrees: Arc::new(NoopWorktrees),
                devices: Arc::new(NoopDevices),
                native_cache: Arc::new(ScriptedNativeCache::new(NativeScript::LookupAndroid(Ok(
                    AndroidCacheLookup::Miss {
                        fingerprint: "unused".into(),
                    },
                )))),
                external_command,
                review: Arc::new(NoopReview),
                jira: None,
                multiplexer: None,
                mcp_server: None,
            },
            action_tx,
            handle_tx,
            task_handle_tx,
        );
        (runner, action_rx)
    }

    async fn receive_action(action_rx: &mut UnboundedReceiver<Action>) -> Action {
        tokio::time::timeout(Duration::from_secs(1), action_rx.recv())
            .await
            .expect("native cache effect should send an action")
            .expect("action channel should stay open")
    }

    #[tokio::test]
    async fn open_finder_forwards_exact_path_and_dispatches_failure() {
        let paths = Arc::new(Mutex::new(Vec::new()));
        let external_command = Arc::new(RecordingExternalCommand {
            paths: paths.clone(),
            failure: Some("launch failed".into()),
        });
        let (runner, mut action_rx) = runner_with_external_command(external_command);
        let path = PathBuf::from("/tmp/ump dash");

        runner
            .run_effects(vec![Effect::OpenInFinder { path: path.clone() }])
            .await;

        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::OpenFinderFailed("launch failed".into())
        );
        assert_eq!(*paths.lock().unwrap(), vec![path]);
    }

    #[tokio::test]
    async fn spawn_task_delivers_task_record_before_fast_command_exit() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel();
        let (handle_tx, _handle_rx) = tokio::sync::mpsc::unbounded_channel();
        let (task_handle_tx, mut task_handle_rx) = tokio::sync::mpsc::unbounded_channel();
        let runner = EffectRunner::new(
            Adapters {
                command_runner: Arc::new(FastExitCommandRunner),
                metro: Arc::new(NoopMetro),
                port_probe: Arc::new(NoopProbe),
                worktrees: Arc::new(NoopWorktrees),
                devices: Arc::new(NoopDevices),
                native_cache: Arc::new(ScriptedNativeCache::new(NativeScript::LookupAndroid(Ok(
                    AndroidCacheLookup::Miss {
                        fingerprint: "unused".into(),
                    },
                )))),
                external_command: Arc::new(NoopExternalCommand),
                review: Arc::new(NoopReview),
                jira: None,
                multiplexer: None,
                mcp_server: None,
            },
            action_tx,
            handle_tx,
            task_handle_tx,
        );

        runner
            .run_effects(vec![crate::app::effect::Effect::SpawnTask {
                task_id: crate::domain::task::TaskId(42),
                worktree_id: WorktreeId("wt-1".into()),
                spec: CommandSpec::ShellCommand {
                    command: "true".into(),
                },
                cwd: PathBuf::from("/tmp/wt-1"),
                branch: "main".into(),
                repo_root: PathBuf::from("/tmp"),
            }])
            .await;

        enum FirstDelivery {
            Task,
            Exit(Box<Action>),
        }

        let first = tokio::time::timeout(Duration::from_secs(1), async {
            tokio::select! {
                biased;
                Some((_wt_id, _record)) = task_handle_rx.recv() => FirstDelivery::Task,
                Some(action) = action_rx.recv() => FirstDelivery::Exit(Box::new(action)),
            }
        })
        .await
        .expect("task record or exit action should be delivered");

        match first {
            FirstDelivery::Task => {}
            FirstDelivery::Exit(action) => {
                panic!("task record must be delivered before fast command exit; got {action:?}")
            }
        }
    }

    #[tokio::test]
    async fn native_cache_lookup_effects_map_success_and_failure() {
        let worktree_id = WorktreeId("wt-a".into());
        let (runner, mut action_rx) =
            runner_with_script(NativeScript::LookupIos(Ok(IosSimulatorCacheLookup::Miss {
                fingerprint: "ios-fp".into(),
            })));
        runner
            .run_effects(vec![Effect::LookupIosSimulatorCache {
                worktree_id: worktree_id.clone(),
                worktree_path: PathBuf::from("/tmp/wt-a"),
            }])
            .await;
        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::IosSimulatorCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Ok(IosSimulatorCacheLookup::Miss {
                    fingerprint: "ios-fp".into(),
                }),
            }
        );

        let (runner, mut action_rx) =
            runner_with_script(NativeScript::LookupAndroid(Err("lookup failed".into())));
        runner
            .run_effects(vec![Effect::LookupAndroidCache {
                worktree_id: worktree_id.clone(),
                worktree_path: PathBuf::from("/tmp/wt-a"),
            }])
            .await;
        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::AndroidCacheLookupFinished {
                worktree_id,
                result: Err("lookup failed".into()),
            }
        );
    }

    #[tokio::test]
    async fn native_cache_store_effects_map_success_and_failure() {
        let worktree_id = WorktreeId("wt-a".into());
        let hit = ios_hit();
        let (runner, mut action_rx) = runner_with_script(NativeScript::StoreIos(Ok(hit.clone())));
        runner
            .run_effects(vec![Effect::StoreIosSimulatorCache {
                worktree_id: worktree_id.clone(),
                request: IosSimulatorCacheStoreRequest {
                    worktree_path: PathBuf::from("/tmp/wt-a"),
                    variant: "Debug".into(),
                },
            }])
            .await;
        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::IosSimulatorCacheLookupFinished {
                worktree_id: worktree_id.clone(),
                result: Ok(IosSimulatorCacheLookup::Hit(Box::new(hit))),
            }
        );

        let (runner, mut action_rx) =
            runner_with_script(NativeScript::StoreAndroid(Err("store failed".into())));
        runner
            .run_effects(vec![Effect::StoreAndroidCache {
                worktree_id: worktree_id.clone(),
                request: AndroidCacheStoreRequest {
                    worktree_path: PathBuf::from("/tmp/wt-a"),
                    variant: "localDebugOptimized".into(),
                },
            }])
            .await;
        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::AndroidCacheLookupFinished {
                worktree_id,
                result: Err("store failed".into()),
            }
        );
    }

    #[tokio::test]
    async fn native_cache_launch_effects_map_success_and_failure() {
        let worktree_id = WorktreeId("wt-a".into());
        let (runner, mut action_rx) =
            runner_with_script(NativeScript::LaunchIos(Ok(vec!["launched".into()])));
        runner
            .run_effects(vec![Effect::InstallAndLaunchCachedIosSimulator {
                worktree_id: worktree_id.clone(),
                request: CachedIosLaunchRequest {
                    simulator_udid: "SIM-1".into(),
                    app_path: PathBuf::from("/tmp/app"),
                    bundle_id: "com.aljazeera.test".into(),
                    metro_port: 8081,
                    fingerprint: "fingerprint".into(),
                    variant: crate::domain::command::RunVariant::Local,
                    artifact_digest_algorithm: "sha256".into(),
                    artifact_digest: "digest".into(),
                },
            }])
            .await;
        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::CachedIosLaunchFinished {
                worktree_id: worktree_id.clone(),
                result: CachedIosLaunchResult::Success(vec!["launched".into()]),
            }
        );

        let (runner, mut action_rx) =
            runner_with_script(NativeScript::LaunchAndroid(Err("launch failed".into())));
        runner
            .run_effects(vec![Effect::InstallAndLaunchCachedAndroid {
                worktree_id: worktree_id.clone(),
                request: CachedAndroidLaunchRequest {
                    device_id: "emulator-5554".into(),
                    apk_path: PathBuf::from("/tmp/app.apk"),
                    application_id: "com.aljazeera.test".into(),
                    metro_port: 8081,
                    fingerprint: "fingerprint".into(),
                    variant: crate::domain::command::RunVariant::Prod,
                    artifact_digest_algorithm: "sha256".into(),
                    artifact_digest: "digest".into(),
                },
            }])
            .await;
        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::CachedAndroidLaunchFinished {
                worktree_id: worktree_id.clone(),
                result: CachedAndroidLaunchResult::Failure("launch failed".into()),
            }
        );

        let (runner, mut action_rx) = runner_with_script(
            NativeScript::LaunchAndroidValidationError("cached APK digest mismatch".into()),
        );
        runner
            .run_effects(vec![Effect::InstallAndLaunchCachedAndroid {
                worktree_id: worktree_id.clone(),
                request: CachedAndroidLaunchRequest {
                    device_id: "emulator-5554".into(),
                    apk_path: PathBuf::from("/tmp/app.apk"),
                    application_id: "com.aljazeera.test".into(),
                    metro_port: 8081,
                    fingerprint: "fingerprint".into(),
                    variant: crate::domain::command::RunVariant::Dev,
                    artifact_digest_algorithm: "sha256".into(),
                    artifact_digest: "digest".into(),
                },
            }])
            .await;
        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::CachedAndroidLaunchFinished {
                worktree_id,
                result: CachedAndroidLaunchResult::InvalidArtifact {
                    message: "cached APK digest mismatch".into(),
                    device_id: "emulator-5554".into(),
                    variant: crate::domain::command::RunVariant::Dev,
                },
            }
        );

        let worktree_id = WorktreeId("wt-a".into());
        let (runner, mut action_rx) = runner_with_script(NativeScript::LaunchIosValidationError(
            "cached .app digest mismatch".into(),
        ));
        runner
            .run_effects(vec![Effect::InstallAndLaunchCachedIosSimulator {
                worktree_id: worktree_id.clone(),
                request: CachedIosLaunchRequest {
                    simulator_udid: "SIM-1".into(),
                    app_path: PathBuf::from("/tmp/app"),
                    bundle_id: "com.aljazeera.test".into(),
                    metro_port: 8081,
                    fingerprint: "fingerprint".into(),
                    variant: crate::domain::command::RunVariant::Local,
                    artifact_digest_algorithm: "sha256".into(),
                    artifact_digest: "digest".into(),
                },
            }])
            .await;
        assert_eq!(
            receive_action(&mut action_rx).await,
            Action::CachedIosLaunchFinished {
                worktree_id,
                result: CachedIosLaunchResult::InvalidArtifact {
                    message: "cached .app digest mismatch".into(),
                    device_id: "SIM-1".into(),
                    variant: crate::domain::command::RunVariant::Local,
                },
            }
        );
    }
}
