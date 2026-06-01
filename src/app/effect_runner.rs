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
//!   ListRemoteBranches { repo_root }               → adapters.worktrees.list_remote_branches(repo_root)
//!   FetchJiraTitles { keys }                       → adapters.jira.as_ref()?.fetch_title(...)
//!   SaveJiraCache(map)                             → spawn_blocking infra::jira_cache::save_jira_cache  (F-111 deferred)
//!   RecordSimUsed(udid)                            → spawn_blocking infra::sim_history::record_sim_used  (F-111 deferred)
//!   OpenInMultiplexer { worktree, name, command }  → adapters.multiplexer.as_ref()?.new_window(...)
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
                        .map(crate::domain::native_cache::IosSimulatorCacheLookup::Hit)
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Action::IosSimulatorCacheLookupFinished {
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
                    let result = match native_cache.install_and_launch_ios_simulator(request).await
                    {
                        Ok(lines) => {
                            crate::domain::native_cache::CachedIosLaunchResult::Success(lines)
                        }
                        Err(e) => crate::domain::native_cache::CachedIosLaunchResult::Failure(
                            e.to_string(),
                        ),
                    };
                    let _ = tx.send(Action::CachedIosLaunchFinished {
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
                                        if tx.send(Action::CommandOutputLine { task_id, line }).is_err() {
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
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
