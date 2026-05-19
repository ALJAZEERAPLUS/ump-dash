---
phase: 15-task-cancellation-collision-shared-resource-semaphore
plan: 03
subsystem: app
tags: [effect-runner, spawn-task, semaphore, cancellation-token, repo-root, process-started, pid-oneshot]

# Dependency graph
requires:
  - phase: 15-task-cancellation-collision-shared-resource-semaphore
    plan: 01
    provides: "CommandEvent::ProcessStarted variant + .process_group(0) on run_command"
  - phase: 15-task-cancellation-collision-shared-resource-semaphore
    plan: 02
    provides: "TokioTaskHandle 3-field struct + SIGTERM→200ms→SIGKILL ladder in abort()"
provides:
  - "Effect::SpawnTask payload widened with `repo_root: PathBuf` field (TASK-06 semaphore key)"
  - "EffectRunner.yarn_semaphores field — per-repo-root Semaphore(1) HashMap"
  - "SpawnTask arm reads CommandEvent::ProcessStarted { pid } as the first event, delivers pid via tokio::sync::oneshot to TaskRecord assembly"
  - "SpawnTask arm constructs TokioTaskHandle { join_handle, child_pid: REAL, cancel_token } (no more placeholder pid=0)"
  - "SpawnTask arm forwarding loop uses tokio::select! with cancel_token.cancelled() arm — emits Action::CommandExited { Cancelled } on abort()"
  - "Yarn-family specs (YarnInstall, YarnPodInstall, RmNodeModules) serialize via OwnedSemaphorePermit BEFORE runner.spawn()"
  - "repo_root canonicalized via PathBuf::canonicalize() with raw-path fallback (15-RESEARCH §Pattern 4)"
affects: [15-05, 15-06]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "tokio::sync::oneshot for cross-task PID delivery — paired with tokio::time::timeout(5s) on the receiver to handle spawn-failure (T-15-03-05)"
    - "Per-resource Semaphore(1) keyed by canonicalized PathBuf — OwnedSemaphorePermit dropped on task exit/abort/panic (T-15-03-03)"
    - "std::sync::Mutex held only across HashMap entry().or_insert_with().clone() inside an explicit scope block — MutexGuard !Send compiler enforces drop-before-await (15-RESEARCH §Pitfall 4)"
    - "tokio::select! { rx.recv() / cancel_token.cancelled() } cooperative-cancel pattern — fires CommandExited { Cancelled } and breaks the loop without waiting for OS wait()"
    - "Two-task fan-out for the spawn boundary: inner forwarding task owns rx + permit; outer assembly task awaits pid_rx then builds TaskRecord (works around fn run_one being sync)"

key-files:
  created: []
  modified:
    - "src/app/effect.rs — Effect::SpawnTask widened with repo_root: PathBuf as 6th field; doc-comment updated; +1 inline test (spawn_task_carries_repo_root_for_semaphore_key) + existing test updated to populate/destructure repo_root"
    - "src/app/update.rs — dispatch_command populates repo_root from state.app_config.repo_root"
    - "src/app/effect_runner.rs — yarn_semaphores field + initializer in EffectRunner::new; SpawnTask arm fully rewritten with PID oneshot + cancel_token select! + yarn-family semaphore acquire/release"

key-decisions:
  - "state.app_config.repo_root (not state.repo_root) is the source of truth — AppState wraps the field in an AppConfigState sub-struct. The plan's literal text used `state.repo_root.clone()`; build failure surfaced the correct path immediately."
  - "Two-task fan-out for the spawn boundary: the inner forwarding task owns rx + permit + cancel select!; the outer assembly task awaits pid_rx (with 5s timeout) then builds TaskRecord. This is the workaround for `fn run_one(&self, eff: Effect)` being sync — we cannot `.await pid_rx` inline."
  - "Spawn-failure paths covered explicitly: (a) first event is Exited → forward the synthetic ExitStatus; (b) first event is OutputLine or channel closes → forward ExitStatus::Failure { code: None }; (c) pid_tx dropped → assembly task's 5s timeout fires → real_child_pid = 0 → abort()'s pid <= 1 guard makes the kill a no-op (Plan 15-02 / T-15-03-05)."
  - "Defensive cleanup on semaphore .close(): if sem.acquire_owned() returns Err (semaphore closed — we never call .close(), but defensive code matters) the task emits Action::CommandExited { Cancelled } and returns, rather than silently hanging."
  - "Yarn-family predicate inlined as `matches!(spec, ...)` — no named fn, no inline test. Plan 15-06's integration test will cover the runtime serialization behavior via a slow-fixture parallel SpawnTask test."
  - "OwnedSemaphorePermit (not SemaphorePermit) — required because the permit needs to live across the entire forwarding task body (which includes .await points). SemaphorePermit borrows the Semaphore and would tie us to a lifetime that doesn't span the async move closure."
  - "PathBuf canonicalize fallback to raw path on error covers the NFS / missing-dir edge case (15-RESEARCH §Pattern 4) — better to over-serialize (treating raw and canonical as distinct keys) than to crash."
  - "No EffectRunner inline tests added — building an EffectRunner requires constructing 7 trait-object adapters (the Adapters bundle has 5 mandatory + 2 optional). The plan explicitly authorizes skipping in favor of Plan 15-06's integration tests; doing so respects the cost/benefit tradeoff (15-RESEARCH § Pitfall 4 reinforces this)."

requirements-completed: [TASK-04, TASK-06]

# Metrics
duration: ~25min
completed: 2026-05-19
---

# Phase 15 Plan 03: EffectRunner SpawnTask Wiring Summary

**Effect::SpawnTask gained a `repo_root` field and the EffectRunner arm now reads CommandEvent::ProcessStarted { pid } via a oneshot, threads a CancellationToken into the forwarding loop's tokio::select! (so abort() can fire ExitStatus::Cancelled without waiting on OS wait()), and serializes yarn-family specs through a per-repo-root Semaphore(1) acquired BEFORE the subprocess spawns.**

## Performance

- **Duration:** ~25 min
- **Tasks:** 2 (both `type=auto`, both TDD)
- **Files modified:** 3 (src/app/effect.rs, src/app/update.rs, src/app/effect_runner.rs)
- **Lib tests:** 107 → 109 (+1 net new — `spawn_task_carries_repo_root_for_semaphore_key`)
- **Integration tests:** COVER-01 (metro_single_instance) + COVER-02 (process_group_kill) both unchanged and green
- **arch-lint:** 21 G-XX guards green; no Makefile whitelist edits required (tokio_util is an external crate and already in use)
- **clippy:** `cargo clippy --all-targets -- -D warnings` clean

## Task-by-Task

### Task 1 — Widen Effect::SpawnTask with repo_root + propagate from update.rs

- Added `repo_root: std::path::PathBuf` as the 6th field of `Effect::SpawnTask`
- Doc-comment block expanded to explain the field's role as the yarn semaphore key
- Updated `spawn_task_variant_constructs_and_matches` to populate and destructure `repo_root`
- New inline test `spawn_task_carries_repo_root_for_semaphore_key` pins round-trip identity with a distinctive path
- `dispatch_command` in update.rs now populates `repo_root: state.app_config.repo_root.clone()`
- effect_runner.rs's SpawnTask destructure widened with `repo_root` + transitional `let _ = repo_root;` (consumed by Task 2)
- **Commit:** `a38c4b6` — `feat(15-03): widen Effect::SpawnTask with repo_root field for yarn semaphore key`

### Task 2 — Extend EffectRunner SpawnTask arm with PID oneshot, CancellationToken, and yarn semaphore

- Added `pub yarn_semaphores: std::sync::Mutex<HashMap<PathBuf, Arc<tokio::sync::Semaphore>>>` field to EffectRunner
- Initialized empty in `EffectRunner::new` — no new constructor arg, so runtime.rs untouched
- Rewrote the entire `Effect::SpawnTask` arm body:
  - Cancellation token created and cloned into the forwarding loop
  - Yarn-family predicate via `matches!` (YarnInstall | YarnPodInstall | RmNodeModules)
  - Canonicalized repo_root for stable HashMap keying (with raw-path fallback)
  - MutexGuard scope-block to look up or insert the per-repo-root semaphore, then dropped before any `.await`
  - PID oneshot channel for cross-task delivery
  - Inner forwarding task: acquire permit → runner.spawn → read ProcessStarted → forward pid → tokio::select! { rx.recv() / cancel_token.cancelled() }
  - Outer assembly task: tokio::time::timeout(5s, pid_rx) → build TaskRecord with real pid → send via task_handle_tx
- Spawn-failure paths covered: synthetic Exited forwarded directly; contract-violation paths emit ExitStatus::Failure { code: None }
- The placeholder pid=0 from Plan 15-02 is GONE — child_pid now always carries the real PID (or 0 only on timeout, which abort() safely no-ops)
- **Commit:** `b69782d` — `feat(15-03): wire SpawnTask arm with PID + cancel token + yarn semaphore`

## Verification Results

| Check | Result |
|-------|--------|
| `cargo build --quiet` | clean |
| `cargo test --lib --quiet` | 109 passed; 0 failed (+1 net new test from Task 1) |
| `cargo test --test process_group_kill --quiet` | 1 passed (COVER-02 unchanged) |
| `cargo test --test metro_single_instance --quiet` | 2 passed (COVER-01 unchanged) |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `make arch-lint` | PASS (21 G-XX guards green) |
| `grep -c 'yarn_semaphores' src/app/effect_runner.rs` | 3 (field + init + use site) |
| `grep -c 'tokio::sync::oneshot' src/app/effect_runner.rs` | 2 (channel + send) |
| `grep -c 'CancellationToken::new()' src/app/effect_runner.rs` | 1 |
| `grep -c 'cancel_token.*cancelled()' src/app/effect_runner.rs` | 1 (the select! arm) |
| `grep -c 'canonicalize()' src/app/effect_runner.rs` | 1 |
| `grep -c 'acquire_owned()' src/app/effect_runner.rs` | 1 |
| `grep -c 'ExitStatus::Cancelled' src/app/effect_runner.rs` | 2 (cancel arm + defensive semaphore-closed path) |
| `rg -c 'CommandEvent::ProcessStarted' src/app/effect_runner.rs` | 4 (import + first-event match + straggler arm + comment) |
| `rg -c 'cancel_token_for_loop.cancelled' src/app/effect_runner.rs` | 1 |
| `grep -c 'repo_root: std::path::PathBuf' src/app/effect.rs` | 2 (field declaration + test) |
| `grep -c 'repo_root: state.app_config.repo_root.clone()' src/app/update.rs` | 10 (the new dispatch_command site + 9 pre-existing worktree-effect call sites — proves the pattern is consistent) |

## Deviations from Plan

### Rule 1 — Bug (path-of-truth fix)

**1. [Rule 1 - Bug] `state.repo_root` → `state.app_config.repo_root`**
- **Found during:** Task 1, cargo build
- **Issue:** The plan instructed `repo_root: state.repo_root.clone()` but `AppState` no longer carries `repo_root` directly — it lives on the `AppConfigState` sub-struct (`src/app/state.rs:184`) which is wrapped under `state.app_config`. The compiler's E0609 surfaced the correct path with a helpful suggestion. All other Effect variants in update.rs that carry `repo_root` already use the `state.app_config.repo_root.clone()` form (verified with grep — 9 pre-existing sites).
- **Fix:** Use `state.app_config.repo_root.clone()` consistent with the existing pattern.
- **Files modified:** src/app/update.rs (line 84)
- **Commit:** a38c4b6

No Rule 2, Rule 3, or Rule 4 deviations. No authentication gates encountered. No architectural changes required.

## Threat Mitigations Honored

| Threat ID | Mitigation Implemented |
|-----------|------------------------|
| T-15-03-01 | Explicit scope block `let semaphore_opt = if is_yarn_family { let mut map = self.yarn_semaphores.lock().unwrap(); ... .clone() } else { None };` — MutexGuard dropped at the closing brace before any `.await`. Compiler enforces (`MutexGuard` is `!Send`). |
| T-15-03-02 | `repo_root.canonicalize().unwrap_or_else(|_| repo_root.clone())` — raw-path fallback on NFS / missing-dir errors. |
| T-15-03-03 | `OwnedSemaphorePermit` bound to `_permit` in the forwarding task — released on Drop (task exit, abort, panic). |
| T-15-03-04 | Accepted per plan disposition — the TOCTOU window is microseconds; PID-recycle does not realistically hit our PGID. |
| T-15-03-05 | `tokio::time::timeout(Duration::from_secs(5), pid_rx)` in the assembly task. Timeout → `real_child_pid = 0` → abort()'s `pid <= 1` guard noops the kill (Plan 15-02). TaskRecord still delivered so the slice gets the (effectively dead) handle; Phase 14 D-08 stale-drop handles any orphaned CommandExited. |

## Plan 15-05 Unblocked

- `dispatch_command` now reads `state.app_config.repo_root` and the `Effect::SpawnTask` field is in place — Plan 15-05's `CommandCancel` handler can call `record.handle.abort()`, which will (a) send SIGTERM to the full PGID, (b) escalate to SIGKILL after 200ms, AND (c) cause the forwarding loop's `tokio::select!` to emit `Action::CommandExited { Cancelled }` within milliseconds — independent of the OS-level `wait()`.

## Plan 15-06 Unblocked

- Two parallel `Effect::SpawnTask` on the same canonicalized `repo_root` will serialize via the `yarn_semaphores` map. Plan 15-06's slow-fixture integration test can now assert serialization end-to-end.

## D-22 Preserved

- COVER-01 (metro_single_instance) and COVER-02 (process_group_kill) unchanged and green. No regressions in either characterization layer.

## TDD Gate Compliance

Plan 15-03 plan frontmatter is `type: execute` (not `type: tdd`), so plan-level RED/GREEN/REFACTOR gate enforcement does not apply. Per-task TDD was honored: Task 1 added a failing test (`spawn_task_carries_repo_root_for_semaphore_key`) that could not compile without the new `repo_root` field, then the field was added to make it pass (RED → GREEN in a single commit, justified because the test and implementation are in the same file and the test would not compile in isolation). Task 2 had no new inline tests (skipped per plan authorization) — covered by Plan 15-06's integration tests.

## Self-Check: PASSED

Verified files exist:
- FOUND: src/app/effect.rs (modified with repo_root field + new test)
- FOUND: src/app/update.rs (modified with state.app_config.repo_root)
- FOUND: src/app/effect_runner.rs (yarn_semaphores field + rewritten SpawnTask arm)

Verified commits exist (`git log --oneline -3`):
- FOUND: a38c4b6 — feat(15-03): widen Effect::SpawnTask with repo_root field for yarn semaphore key
- FOUND: b69782d — feat(15-03): wire SpawnTask arm with PID + cancel token + yarn semaphore
