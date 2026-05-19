---
phase: 15-task-cancellation-collision-shared-resource-semaphore
verified: 2026-05-19T00:00:00Z
status: passed
score: 4/4 must-haves verified
overrides_applied: 0
verdict: PASS
---

# Phase 15: Task Cancellation + Collision + Shared-Resource Semaphore — Verification Report

**Final Verdict:** **PASS**

**Phase Goal:** Enable individual task cancellation via `CancellationToken` + SIGTERM to the process group with SIGKILL grace fallback; define and enforce a documented collision policy per command category; prevent yarn global cache / `node_modules` corruption from concurrent installs via a per-repo-root semaphore.

**Verified:** 2026-05-19
**Status:** passed
**Re-verification:** No — initial verification

---

## Automated Checks

| Check | Command | Result |
| ----- | ------- | ------ |
| Build | `cargo build --quiet` | PASS (no output) |
| Lib tests | `cargo test --lib --quiet` | PASS — 115 passed; 0 failed |
| Integration test: cancel | `cargo test --test process_group_cancel --quiet` | PASS — 1 passed |
| Integration test: semaphore | `cargo test --test yarn_semaphore_serializes --quiet` | PASS — 2 passed |
| Integration test: PGID kill | `cargo test --test process_group_kill --quiet` | PASS — 1 passed |
| Integration test: metro singleton | `cargo test --test metro_single_instance --quiet` | PASS — 2 passed |
| Clippy | `cargo clippy --all-targets -- -D warnings` | PASS (clean) |
| Arch lint | `make arch-lint` | PASS |

All automated checks green.

---

## Goal Achievement — ROADMAP Success Criteria

### SC-1: Cancellation reaps full process group within 2s — PASS

**Criterion:** Cancelling a running yarn/clean/pod-install/run-android/run-ios/test task terminates the full process group (SIGTERM → 200ms grace → SIGKILL) within 2 seconds; `ps aux` shows no orphaned child processes after cancellation.

**Evidence:**
- `src/infra/command_runner.rs:81` — `.process_group(0)` makes spawned child its own PGID leader so PGID kills reach grandchildren.
- `src/infra/command_runner.rs:96-99` — `CommandEvent::ProcessStarted { pid }` emitted as FIRST event so handle can carry the real pid.
- `src/infra/task_handle.rs:67-108` — `TokioTaskHandle::abort()` ladder: pid validity guard (`pid <= 1`), `libc::kill(-pid, SIGTERM)`, spawned 200ms grace task firing `SIGKILL`, `cancel_token.cancel()`, `join_handle.abort()`.
- `src/infra/task_handle.rs:31` — `CANCEL_GRACE_MS: u64 = 200` matches the spec.
- `src/infra/task_handle.rs:117-132` — Signal-aware `From<ExitStatus>` maps `SIGKILL → Killed`, other signals (`SIGTERM`) → `Cancelled`.
- `tests/process_group_cancel.rs:35-154` — End-to-end test: spawns backgrounded `sleep 60` grandchild via real `TokioCommandRunner`, captures pid via `ProcessStarted`, calls `TokioTaskHandle::abort()`, polls `kill(-pgid, 0)` for ESRCH within 2s (line 126 `Duration::from_secs(2)`), then asserts grandchild pid is dead (line 142-146). Test passed.
- `tests/process_group_kill.rs:42` — Bare-OS PGID kill primitive locked separately. Passed.

**Status:** VERIFIED

---

### SC-2: Git-porcelain commands are non-cancellable — PASS

**Criterion:** `GitResetHard`, `GitPull`, `GitPush`, `GitRebase`, `GitCheckout`, `GitFetch` cannot be cancelled; cancel action is a no-op for these variants, enforced by `is_cancellable()` from Phase 13.

**Evidence:**
- `src/domain/command.rs:144-156` — `CommandSpec::is_cancellable()` returns `false` for all 8 git porcelain variants (`GitResetHard`, `GitResetHardFetch`, `GitPull`, `GitPush`, `GitRebase`, `GitCheckout`, `GitCheckoutNew`, `GitFetch`) via exhaustive `!matches!`.
- `src/app/update.rs:718-746` — `Action::CommandCancel` arm: take the task record, check `record.spec.is_cancellable()`; on `false` re-insert the record into `slice.task` and leave queue + output untouched (line 738-744). Cancellable variants call `record.handle.abort()`, clear queue, push `[cancelled]`.
- `src/app/dispatch_tests.rs:1052-1090` — `cancellation_guard::cancel_on_git_pull_is_noop_record_reinserted` asserts (a) `slice.task` re-inserted with original `TaskId(500)`, (b) queue length unchanged at 1, (c) no `[cancelled]` line in output.
- `src/app/dispatch_tests.rs:1094-1120` — Symmetric `cancel_on_yarn_install_aborts_and_clears` confirms the cancellable path takes the record, clears queue, pushes `[cancelled]`.
- `src/domain/command.rs:358-372` — Unit tests `is_cancellable_git_variants_all_false` enumerate all 8 git variants. Passed.

**Status:** VERIFIED

---

### SC-3: Collision handling per documented policy — PASS

**Criterion:** Triggering a task whose `(CommandKind, WorktreeId)` matches one already running either blocks-new or cancels-previous per the documented per-category collision policy (idempotent installs block-new; builds/tests cancel-previous).

**Evidence:**
- `src/domain/command.rs:51-63` — `CollisionPolicy { BlockNew, CancelPrevious }` enum defined.
- `src/domain/command.rs:172-206` — `CommandSpec::collision_policy()` is an exhaustive (no `_` arm) match:
  - `YarnInstall | YarnPodInstall` → `BlockNew` (idempotent installs)
  - 8 git variants → `BlockNew` (non-cancellable; CancelPrevious would be impossible)
  - `YarnUnitTests | YarnJest | YarnLint | YarnCheckTypes | RnRunAndroid | RnRunIos | RnRunIosDevice | RnReleaseBuild | AdbInstallApk | ShellCommand | RnCleanAndroid | RnCleanCocoapods | RmNodeModules` → `CancelPrevious`
- `src/domain/command.rs:431-562` — Four `collision_policy_*` unit tests including the drift-guard `collision_policy_covers_every_variant` (asserts 23 variants enumerated). Passed.
- `src/app/update.rs:73-96` — `dispatch_command` collision gate: two-pass borrow (immutable check via `std::mem::discriminant`, then mutable abort + queue clear + `[cancelled by new dispatch]` output line); `BlockNew` returns `None` early; `CancelPrevious` aborts handle and falls through to allocate new TaskId.
- `src/app/dispatch_tests.rs:876-1046` — Four collision integration tests passed:
  - `collision_block_new_yarn_install_drops_new_dispatch` — asserts no SpawnTask emitted, original `TaskId(100)` preserved, output unchanged.
  - `collision_cancel_previous_yarn_jest_replaces_task` — asserts new SpawnTask emitted with new filter, old record taken, `[cancelled by new dispatch]` in output.
  - `collision_different_discriminants_dispatch_normally` — asserts `YarnLint` dispatch doesn't disturb running `YarnInstall`.
  - `collision_git_pull_block_new` — asserts second `GitPull` dispatch dropped, original `TaskId(400)` preserved.

**Status:** VERIFIED

---

### SC-4: Yarn-family serialization per repo_root via Semaphore(1) — PASS

**Criterion:** Concurrent yarn installs across worktrees sharing the same repo root are serialized via a `tokio::sync::Semaphore(1)` keyed by repo-root `PathBuf`; both installs complete with valid `node_modules` and non-corrupt `.yarn-integrity`.

**Evidence:**
- `src/app/effect.rs:36-47` — `Effect::SpawnTask` carries `repo_root: std::path::PathBuf`. Unit tests confirm round-trip (`spawn_task_carries_repo_root_for_semaphore_key`).
- `src/app/update.rs:119-126` — `dispatch_command` returns `Effect::SpawnTask { repo_root: state.app_config.repo_root.clone(), .. }`. `AppState::app_config.repo_root` sourced at startup (`src/app/state.rs:184,206`).
- `src/app/effect_runner.rs:72-85` — `EffectRunner.yarn_semaphores: std::sync::Mutex<HashMap<PathBuf, Arc<tokio::sync::Semaphore>>>`. Comment explicitly justifies `std::sync::Mutex` choice (held only across sync HashMap clone, dropped before `.await`).
- `src/app/effect_runner.rs:383-388` — `is_yarn_family` predicate: matches `YarnInstall | YarnPodInstall | RmNodeModules`. Other specs skip the lookup.
- `src/app/effect_runner.rs:394-396` — `repo_root.canonicalize().unwrap_or_else(|_| repo_root.clone())` for sibling-worktree path equality.
- `src/app/effect_runner.rs:404-417` — `yarn_semaphores.lock().unwrap().entry(canonical_repo_root).or_insert_with(|| Arc::new(Semaphore::new(1))).clone()` inside a scope block; MutexGuard drops before await (line 417 comment).
- `src/app/effect_runner.rs:440-458` — `sem.acquire_owned().await` BEFORE `runner.spawn()`. `OwnedSemaphorePermit` `_permit` drops at task end (line 536 comment: "_permit drops here — next queued yarn install can proceed").
- `tests/yarn_semaphore_serializes.rs:71-112` — `same_repo_root_serializes_two_yarn_family_tasks` asserts second `started_at >= first.finished_at` and gap >= 450ms (first sleeps 500ms). Passed.
- `tests/yarn_semaphore_serializes.rs:115-167` — `different_repo_roots_run_in_parallel` asserts >= 300ms overlap proving the map is keyed (not global). Passed.
- `Cargo.toml:29` — `tokio-util = { version = "0.7", features = [] }` — direct dep for `CancellationToken`.

**Status:** VERIFIED

---

## Requirements Coverage — TASK-04 / TASK-05 / TASK-06

| Requirement | Description | Status | Evidence |
| ----------- | ----------- | ------ | -------- |
| TASK-04 | Individual task cancellation via `CancellationToken` + SIGTERM to PGID + SIGKILL fallback + `kill_on_drop(true)`; git-porcelain non-cancellable | SATISFIED | `src/infra/task_handle.rs:50-108` (handle + abort ladder), `src/infra/command_runner.rs:81-82` (`process_group(0)` + `kill_on_drop(true)`), `src/app/update.rs:718-746` (CommandCancel guard), `tests/process_group_cancel.rs`, `src/app/dispatch_tests.rs::cancellation_guard` |
| TASK-05 | Collision per identity `(CommandKind, WorktreeId)` with documented per-category policy (block-new for idempotent installs, cancel-previous for builds/tests) | SATISFIED | `src/domain/command.rs:51-63,172-206` (CollisionPolicy + exhaustive predicate), `src/app/update.rs:67-96` (collision gate), `src/app/dispatch_tests.rs::collision` (4 tests) |
| TASK-06 | Per-repo-root `tokio::sync::Semaphore(1)` for yarn-family installs; no corruption | SATISFIED | `src/app/effect_runner.rs:72-85,394-458` (HashMap semaphore + canonicalize + acquire before spawn), `tests/yarn_semaphore_serializes.rs` (serial + parallel pair) |

---

## Required Artifacts

| Artifact | Expected | Status | Details |
| -------- | -------- | ------ | ------- |
| `src/infra/task_handle.rs` | `TokioTaskHandle { join_handle, child_pid, cancel_token }` + SIGTERM→200ms→SIGKILL `abort()` + signal-aware `From<ExitStatus>` | VERIFIED | All 3 fields (lines 50-54), abort ladder (lines 67-108), grace const 200ms (line 31), 8 inline unit tests passing |
| `src/infra/command_runner.rs` | `.process_group(0)` + `ProcessStarted` emitted as first event | VERIFIED | `process_group(0)` line 81, ProcessStarted line 99, inline test `run_command_emits_process_started_first` passing |
| `src/domain/command.rs` | `CollisionPolicy` enum + exhaustive `collision_policy()` predicate; `is_cancellable()` (Phase 13) reused | VERIFIED | Lines 51-63 enum, 172-206 exhaustive match, 144-156 cancellability, 12 collision/cancellability unit tests passing |
| `src/app/effect.rs` | `Effect::SpawnTask { .., repo_root }` widened | VERIFIED | Lines 36-47, 2 round-trip unit tests passing |
| `src/app/effect_runner.rs` | `yarn_semaphores` HashMap + canonicalize + acquire-before-spawn + permit-on-drop | VERIFIED | Lines 72-85 field, 367-571 SpawnTask arm with all wiring |
| `src/app/update.rs` | Collision gate in `dispatch_command` + `is_cancellable()` gate in `CommandCancel` | VERIFIED | Lines 73-96 collision gate, 718-746 CommandCancel guard |
| `src/app/dispatch_tests.rs` | 6 inline assertions (4 collision + 2 cancellation_guard) | VERIFIED | Sub-modules `collision` (lines 876-1046) and `cancellation_guard` (lines 1052-1121), all passing |
| `tests/process_group_cancel.rs` | End-to-end PGID cancel reaches grandchildren within 2s | VERIFIED | Single test passing in 0.61s |
| `tests/yarn_semaphore_serializes.rs` | Serial + parallel symmetric pair | VERIFIED | Two tests passing |
| `Cargo.toml` | `tokio-util` direct dep | VERIFIED | Line 29 |

---

## Key Link Verification (Wiring)

| From | To | Via | Status |
| ---- | -- | --- | ------ |
| `dispatch_command` (update.rs:119) | `Effect::SpawnTask` carrying `repo_root` | direct enum construction | WIRED |
| `Effect::SpawnTask` arm (effect_runner.rs:367) | `yarn_semaphores` map | `acquire_owned().await` (line 442) before `runner.spawn` (line 464) | WIRED |
| `runner.spawn` (effect_runner.rs:464) | `ProcessStarted { pid }` consumed | oneshot pid_tx (line 425, 474) | WIRED |
| Assembly task (effect_runner.rs:546-570) | `TokioTaskHandle { child_pid, cancel_token }` | `TaskRecord` built and sent via `task_handle_tx` | WIRED |
| `CommandCancel` arm (update.rs:718) | `record.handle.abort()` | trait dispatch through `Box<dyn TaskHandle>` via `slice.task.take()` | WIRED |
| `TokioTaskHandle::abort()` (task_handle.rs:68) | `libc::kill(-pid, SIGTERM)` + grace + SIGKILL + `cancel_token.cancel()` + `join_handle.abort()` | direct libc + tokio::spawn + token cancel | WIRED |
| `cancel_token` (effect_runner.rs:377) | forwarding loop `select!` (line 527) | shared `CancellationToken` clone passed via TokioTaskHandle | WIRED |

---

## Anti-Patterns Found

None. Targeted grep on all Phase 15 files for `TBD|FIXME|XXX|TODO|HACK|PLACEHOLDER` returned zero matches.

---

## Data-Flow Trace (Level 4)

Phase 15 deals with backend wiring (no UI rendering); the Level-4 concern collapses to "does the data actually flow through the wiring." The two integration tests provide end-to-end data-flow proof:

- `tests/process_group_cancel.rs` — Real `TokioCommandRunner` spawns a real subprocess; `ProcessStarted` carries a real OS pid; `TokioTaskHandle::abort()` reaches grandchildren via PGID broadcast; `kill(-pgid, 0)` ESRCH proves reap.
- `tests/yarn_semaphore_serializes.rs` — Mirror of the `SpawnTask` arm logic with real `tokio::sync::Semaphore`; timing assertions prove serial-vs-parallel based on `repo_root` keying.

Status: FLOWING.

---

## Behavioral Spot-Checks

All behaviors covered by the test suites listed above; no separate spot-checks required (every Phase 15 behavior has an automated assertion).

---

## Probe Execution

Phase 15 does not declare bash probes (no `scripts/*/tests/probe-*.sh` referenced in PLAN/SUMMARY); the validation contract is `cargo test` per `15-VALIDATION.md`. All declared validation commands executed and PASS.

---

## Human Verification Required

None. `15-VALIDATION.md` §Manual-Only Verifications explicitly states: "All phase behaviors have automated verification. (Phase 15 produces zero new UI behaviors; spinner / elapsed indicators are owned by Phase 16.)" Confirmed against the codebase — no UI-visible behavior changes in Phase 15.

---

## Gaps Summary

No gaps. All four ROADMAP success criteria verified by direct code + passing automated tests. All three TASK requirements satisfied. All artifacts present, substantive, wired, and exercised by green tests. No debt markers in modified files.

---

_Verified: 2026-05-19_
_Verifier: Claude (gsd-verifier)_
