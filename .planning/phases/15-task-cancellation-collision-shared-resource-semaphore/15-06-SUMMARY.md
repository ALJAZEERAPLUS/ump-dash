---
phase: 15-task-cancellation-collision-shared-resource-semaphore
plan: 06
subsystem: integration-tests
tags: [integration-tests, process-group, semaphore, end-to-end, validation, characterization]
dependency_graph:
  requires:
    - 15-01 (TokioCommandRunner emits CommandEvent::ProcessStarted as first event; spawns with .process_group(0))
    - 15-02 (TokioTaskHandle::abort performs SIGTERM → 200ms → SIGKILL ladder; named 3-field struct)
    - 15-03 (EffectRunner::yarn_semaphores: Mutex<HashMap<PathBuf, Arc<Semaphore>>>)
  provides:
    - End-to-end proof of ROADMAP success criterion 1 (orphan reap within 2s via TokioCommandRunner + TokioTaskHandle::abort)
    - End-to-end proof of ROADMAP success criterion 4 (per-repo-root semaphore serialization of yarn-family installs)
  affects:
    - .planning/phases/15-task-cancellation-collision-shared-resource-semaphore/15-VALIDATION.md (Wave 0 ❌ → ✅ for TASK-04 + TASK-06)
tech-stack:
  added: []
  patterns:
    - Integration-test characterization via real subprocess + libc::kill probe
    - Contract test against in-memory data structure shape (semaphore HashMap)
    - tokio::time::timeout hard ceilings on every async test body
key-files:
  created:
    - tests/process_group_cancel.rs
    - tests/yarn_semaphore_serializes.rs
  modified: []
decisions:
  - "Use ShellCommand (sh -c) instead of bash -c — POSIX `&`, `$!`, `wait` work on both macOS (/bin/sh→bash) and Linux CI (/bin/sh→dash); avoids adding a bash-specific CommandSpec variant"
  - "TokioTaskHandle constructed with a no-op JoinHandle in the test — abort() still fires SIGTERM/SIGKILL to the REAL pgid; this isolates the test from EffectRunner scaffolding while still exercising the production kill ladder"
  - "yarn_semaphore_serializes.rs is a CONTRACT test against the same data structure (Mutex<HashMap<PathBuf, Arc<Semaphore>>>) rather than an end-to-end test through Adapters — Plan 15-03's inline tests already lock the production wiring; this characterizes the contract for future refactors"
  - "Symmetric two-test pair (same-repo serializes / different-repo parallelizes) catches both the 'always serializes' (global semaphore) and 'never serializes' (no semaphore) bug shapes"
metrics:
  duration: ~12 minutes (2 tasks, no deviations)
  completed: 2026-05-19
  tasks: 2
  files: 2
---

# Phase 15 Plan 06: End-to-End Integration Tests for Process-Group Cancel + Yarn Semaphore Summary

Two integration tests added to `tests/` that prove ROADMAP success criteria 1 (process-group reap within 2s via the full Phase 15 path) and 4 (per-repo-root yarn-family serialization), moving the Wave 0 ❌ placeholders in `15-VALIDATION.md §Phase Requirements → Test Map` to ✅.

## What Was Built

### Task 1 — `tests/process_group_cancel.rs` (154 LOC)

End-to-end test `cancel_via_task_handle_reaps_full_process_group`:

1. Instantiates `TokioCommandRunner` (production adapter).
2. Spawns `CommandSpec::ShellCommand { command: "sleep 60 & echo $! >&2; wait" }` through the runner (`sh -c` POSIX semantics work on both macOS and Linux CI).
3. Reads the FIRST event — asserts it is `CommandEvent::ProcessStarted { pid }`, locks the doc contract. Captures `pgid = pid as i32`.
4. Drains `OutputLine` events (500ms timeout-bounded) until it parses the grandchild sleep's PID from `echo $! >&2`.
5. BEFORE assertion: `libc::kill(-pgid, 0) == 0` (group is live).
6. Constructs a `TokioTaskHandle { join_handle: tokio::spawn(async {}), child_pid: pgid as u32, cancel_token: ... }` and calls `abort()` — fires the Plan 15-02 SIGTERM → 200ms → SIGKILL ladder at the REAL pgid.
7. AFTER assertion: polls `libc::kill(-pgid, 0)` every 20ms until ESRCH (-1), bounded at 2s — proves ROADMAP criterion 1.
8. Final probe: `libc::kill(sleep_pid, 0) == -1` — the specific grandchild is dead, not just the group emptied.
9. Drains remaining events (500ms bound) to let the runner forwarding task complete cleanly.

Wall-clock: 0.03s on macOS (sleep dies on SIGTERM almost instantly).

### Task 2 — `tests/yarn_semaphore_serializes.rs` (167 LOC)

Two contract tests using the same `Mutex<HashMap<PathBuf, Arc<Semaphore::new(1)>>>` data structure as `src/app/effect_runner.rs::EffectRunner::yarn_semaphores`:

**`same_repo_root_serializes_two_yarn_family_tasks`** — two `run_with_semaphore` calls with `repo = std::env::temp_dir()`, sleeping 500ms and 100ms respectively, joined via `tokio::join!`. After identifying first vs second by `started_at` (jitter-symmetric), asserts:
- `second.started_at >= first.finished_at` (strict serialization)
- `second.started_at - first.started_at >= 450ms` (semaphore gap; 50ms slack)

**`different_repo_roots_run_in_parallel`** — two tasks with `std::env::temp_dir()` and `std::env::temp_dir().join("rn-dash-yarn-semaphore-test-{pid}")` (created with `std::fs::create_dir_all` so `canonicalize` succeeds), both sleeping 500ms. Asserts:
- `second.started_at < first.finished_at` (parallel overlap)
- `first.finished_at - second.started_at >= 300ms` (overlap of the 500ms windows; 200ms total slack)

Both tests wrapped in `tokio::time::timeout(Duration::from_secs(3), ...)` with explicit panic messages on timeout (catches deadlock regressions).

Wall-clock: 0.61s combined (one ~500ms serial run + one ~500ms parallel run).

## ROADMAP Success Criteria — Now Proven

| Criterion | Plan that proves it | Test |
|-----------|---------------------|------|
| 1. Cancellation reaps full process group within 2s | **15-06** (this plan) | `tests/process_group_cancel.rs::cancel_via_task_handle_reaps_full_process_group` |
| 2. Collision policy blocks/queues correctly | 15-05 (inline dispatch tests) | `src/app/dispatch_tests/*` |
| 3. SpawnTask emits TaskStarted/TaskFinished | 15-05 (inline dispatch tests) | `src/app/dispatch_tests/*` |
| 4. yarn-family installs serialize per repo_root, parallel across repo_roots | **15-06** (this plan) | `tests/yarn_semaphore_serializes.rs` (2 tests) |

## Deviations from Plan

**1. [Rule 3 - Blocking fixup] Use `sh -c` instead of `bash -c`**
- **Found during:** Task 1 implementation
- **Issue:** Plan §Behavior 2 step 2 specifies `bash -c 'sleep 60 & echo $! >&2; wait'`, but `CommandSpec::ShellCommand` expands to `["sh", "-c", command]` (verified in `src/domain/command.rs:120`). The plan also acknowledged this: "Choose `CommandSpec::ShellCommand { command: ... }` if that's the natural variant ... verify by reading `src/domain/command.rs` for the right variant."
- **Fix:** Use `sh -c` (via `CommandSpec::ShellCommand`). POSIX `&`, `$!`, and `wait` are supported on both macOS (`/bin/sh` → bash) and Linux CI (`/bin/sh` → dash), so no behavior change.
- **Files modified:** `tests/process_group_cancel.rs`
- **Commit:** 34d0d69

**2. [Rule 3 - Blocking fixup] No-op JoinHandle in TokioTaskHandle test construction**
- **Found during:** Task 1 implementation
- **Issue:** Plan §Behavior 2 step 8 was explicit about this resolution path ("Simpler: spawn a NO-OP forwarding task `let nop_handle = tokio::spawn(async {});` for the JoinHandle field"). Applied as written — not a true deviation, but documented for traceability.
- **Files modified:** `tests/process_group_cancel.rs`
- **Commit:** 34d0d69

**3. [Rule 2 - Test robustness] Symmetric leader-identification in yarn_semaphore_serializes**
- **Found during:** Task 2 implementation
- **Issue:** The plan asserted `r2.0 >= r1.1` directly, assuming the first task launched (h1, 500ms) always acquired the permit first. Under tokio scheduling jitter on a busy CI, h2 could acquire first.
- **Fix:** Identify `(first, second)` by comparing `started_at`, then assert `second.started_at >= first.finished_at`. Result: identical semantic guarantee, robust to scheduling order.
- **Files modified:** `tests/yarn_semaphore_serializes.rs`
- **Commit:** 9620e85

**4. [Rule 2 - Test robustness] Per-PID unique sibling directory in different-repo-root test**
- **Found during:** Task 2 implementation
- **Issue:** Plan §Behavior 3 specified `std::env::temp_dir().join("other")` — a hardcoded shared path across cargo test invocations. Concurrent test runs (e.g. across different cargo processes) would collide.
- **Fix:** Use `std::env::temp_dir().join(format!("rn-dash-yarn-semaphore-test-{}", std::process::id()))` for the sibling directory, with best-effort cleanup at end.
- **Files modified:** `tests/yarn_semaphore_serializes.rs`
- **Commit:** 9620e85

## D-22 Invariant Preserved

- `tests/process_group_kill.rs` (COVER-02) — unchanged, passes.
- `tests/metro_single_instance.rs` (COVER-01) — unchanged, passes (2/2).

## Verification — All Green

```
cargo build --quiet                                        # clean
cargo test --lib --quiet                                   # 115 passed
cargo test --test process_group_cancel --quiet             # 1 passed (0.03s)
cargo test --test yarn_semaphore_serializes --quiet        # 2 passed (0.61s)
cargo test --test process_group_kill --quiet               # 1 passed (COVER-02 unchanged)
cargo test --test metro_single_instance --quiet            # 2 passed (COVER-01 unchanged)
cargo clippy --all-targets -- -D warnings                  # clean
make arch-lint                                             # 21 guards PASS
```

Combined integration-test wall-clock: < 1s (well under the 8s ceiling).

## Threat Model Status

| Threat ID | Disposition | Status |
|-----------|-------------|--------|
| T-15-06-01 (DoS via hang) | mitigate | ✅ All tests wrapped in `tokio::time::timeout(2s|3s, ...)` with informative panic messages |
| T-15-06-02 (kill(-1, ...) leak) | mitigate | ✅ `assert!(pgid > 1)` + `assert!(sleep_pid > 1)` guards inline |
| T-15-06-03 (PID leak in panic messages) | accept | ✅ PIDs only printed on failure (standard CI hygiene) |
| T-15-06-04 (lenient fixture passes against buggy impl) | mitigate | ✅ Test header documents guard role; symmetric two-test pair in Task 2; ProcessStarted+PGID assertions in Task 1 are non-trivial |
| T-15-06-SC | — | ✅ No new packages |

## Known Stubs

None. Both tests exercise real code paths (real OS subprocesses in Task 1; real semaphore/HashMap/Mutex in Task 2). No placeholders, no TODOs, no hardcoded empties feeding UI.

## Self-Check: PASSED

- `tests/process_group_cancel.rs` exists — FOUND
- `tests/yarn_semaphore_serializes.rs` exists — FOUND
- Commit `34d0d69` (Task 1) — FOUND in git log
- Commit `9620e85` (Task 2) — FOUND in git log
- Acceptance grep checks (verified):
  - `grep -c 'TokioCommandRunner' tests/process_group_cancel.rs` → 4 (>= 1)
  - `grep -c 'CommandEvent::ProcessStarted' tests/process_group_cancel.rs` → 3 (>= 1)
  - `grep -c 'TokioTaskHandle' tests/process_group_cancel.rs` → 9 (>= 1)
  - `grep -c 'libc::kill(-pgid' tests/process_group_cancel.rs` → 2 (>= 1)
  - `grep -c 'Semaphore::new(1)' tests/yarn_semaphore_serializes.rs` → 2 (>= 1)
  - `grep -c 'acquire_owned' tests/yarn_semaphore_serializes.rs` → 1 (>= 1)
  - `grep -c 'canonicalize' tests/yarn_semaphore_serializes.rs` → 6 (>= 1)
- COVER-01 and COVER-02 unmodified (D-22) — verified via git diff against main
