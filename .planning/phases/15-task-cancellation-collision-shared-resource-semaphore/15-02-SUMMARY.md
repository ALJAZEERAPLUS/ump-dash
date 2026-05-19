---
phase: 15-task-cancellation-collision-shared-resource-semaphore
plan: 02
subsystem: infra
tags: [task-handle, sigterm, sigkill, cancellation-token, infra, posix-signals]

# Dependency graph
requires:
  - phase: 15-task-cancellation-collision-shared-resource-semaphore
    plan: 01
    provides: "tokio-util direct dep + CommandEvent::ProcessStarted variant + .process_group(0) on run_command"
provides:
  - "TokioTaskHandle as a 3-field named struct: { join_handle, child_pid: u32, cancel_token: CancellationToken }"
  - "TaskHandle::abort() performing the full SIGTERM → 200ms grace → SIGKILL escalation ladder against the PGID"
  - "Signal-aware From<std::process::ExitStatus> for ExitStatus — SIGKILL → Killed, other signals → Cancelled, clean non-zero → Failure { code }"
  - "const CANCEL_GRACE_MS: u64 = 200 — module-private grace window constant"
affects: [15-03, 15-05, 15-06]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "POSIX SIGTERM→grace→SIGKILL escalation against PGID — mirrors infra/metro.rs:157-168 pattern"
    - "Pitfall-3 guard: refuse libc::kill if pid <= 1 (init / placeholder) — silent return (abort() is infallible)"
    - "Fire-and-forget tokio::spawn for grace-period escalation — JoinHandle intentionally discarded"
    - "Manual Debug impl for handle structs holding non-Debug fields (JoinHandle, CancellationToken) — print only diagnostically useful fields (pid)"

key-files:
  created: []
  modified:
    - "src/infra/task_handle.rs — TokioTaskHandle widened to 3-field struct; SIGTERM/SIGKILL/grace ladder in abort(); From<ExitStatus> distinguishes Killed/Cancelled; +6 inline tests"
    - "src/app/effect_runner.rs — SpawnTask construction site uses new 3-field struct shape with placeholder pid=0 (Plan 15-03 wires real pid via ProcessStarted oneshot)"

key-decisions:
  - "Manual Debug impl on TokioTaskHandle — omits JoinHandle<()> and CancellationToken internals, prints only child_pid via finish_non_exhaustive(). Satisfies the TaskHandle: Debug bound without leaking handle internals or compiling against tokio types lacking Debug."
  - "Pitfall 3 guard returns silently on pid <= 1 rather than panicking — abort() must be infallible per the domain trait contract. The placeholder pid=0 from effect_runner.rs (pre-15-03) and the init pid=1 are both refused. Tests assert cancel_token is NOT cancelled in those paths (proves early return before step 5)."
  - "Grace-period tokio::spawn is fire-and-forget (JoinHandle discarded). If the runtime exits before 200ms, the task is dropped — no leak. Worst case: one stray libc::kill(-pid, SIGKILL) ESRCH no-op after the process already died. Accepted per T-15-02-03."
  - "child_pid 0xDEAD_BEEF would have been a clearly-not-running value, but `as i32` wraps it negative (>i32::MAX) which triggers the pid <= 1 guard. Test uses 999_999 instead — a clearly-not-running pid that stays positive when cast."
  - "From<ExitStatus> uses ExitStatusExt::signal() hoisted to module-scope import (not inside fn body) for clarity alongside the other module-scope imports."
  - "Signal-mapping tests gated #[cfg(any(target_os = \"linux\", target_os = \"macos\"))] — same gate as COVER-02 (Windows is not a supported target)."
  - "Tests hard-timeout child.wait() at 3s via tokio::time::timeout — bounded CI even if the kill fails to deliver."

requirements-completed: [TASK-04]

# Metrics
duration: 8min
completed: 2026-05-19
---

# Phase 15 Plan 02: TokioTaskHandle SIGTERM/SIGKILL Ladder Summary

**TokioTaskHandle widened to {join_handle, child_pid, cancel_token}; abort() now broadcasts SIGTERM to the PGID, gives 200ms grace, then SIGKILL — with a Pitfall-3 guard refusing pid <= 1 — and From<ExitStatus> distinguishes Killed (SIGKILL) from Cancelled (other signals).**

## Performance

- **Duration:** ~8 min
- **Tasks:** 3
- **Files modified:** 2 (src/infra/task_handle.rs, src/app/effect_runner.rs)
- **Tests added:** +6 in `src/infra/task_handle.rs::tests` (construct_with_all_three_fields, abort_with_placeholder_pid_zero_is_noop, abort_with_placeholder_pid_one_is_noop, abort_with_dead_pid_does_not_panic, from_sigkill_status_maps_to_killed, from_sigterm_status_maps_to_cancelled)
- **Lib test count:** 98 → 104 (+6)

## Accomplishments

- **TokioTaskHandle is a 3-field named struct.** Replaced the Phase 14 tuple `TokioTaskHandle(JoinHandle)` with `TokioTaskHandle { join_handle, child_pid: u32, cancel_token: CancellationToken }`. The TaskHandle trait surface is unchanged (`fn abort(&self)`); all widening is behind the infra-private struct. Manual Debug impl prints only `child_pid` to avoid leaking JoinHandle/Token internals.
- **abort() performs the full Phase 15 escalation ladder.** Six steps in order: (1) capture pid as i32, (2) refuse pid <= 1 (Pitfall 3 guard), (3) `libc::kill(-pid, SIGTERM)`, (4) spawn a 200ms-delayed grace task that fires `libc::kill(-pid, SIGKILL)`, (5) `cancel_token.cancel()` (signals the forwarding loop — Plan 15-03 will wire `.cancelled()` into its select! arm), (6) `join_handle.abort()` (cooperative tokio belt-and-suspenders). All `libc::kill` blocks carry SAFETY comments documenting the own-PGID invariant and ESRCH safety.
- **From<ExitStatus> is signal-aware.** SIGKILL → `ExitStatus::Killed`; any other signal (SIGTERM, SIGINT, SIGHUP, etc.) → `ExitStatus::Cancelled`; clean non-zero exit → `ExitStatus::Failure { code: Some(N) }`. Two new tests spawn real `sleep 30` children in their own PGID, broadcast SIGKILL/SIGTERM, and assert the mapping end-to-end.
- **effect_runner.rs construction site updated.** The Plan 14 `Box::new(TokioTaskHandle(join_handle))` is replaced with the named-field constructor using `child_pid: 0` as a transitional placeholder. The Pitfall-3 guard in abort() ensures this placeholder is safely refused at cancel time — no kill is broadcast against the wrong pid. Plan 15-03 will wire the real `child_pid` via the `CommandEvent::ProcessStarted` oneshot from Plan 15-01.
- **+6 tests, zero regressions.** Lib test count moves 98 → 104. COVER-01 (`tests/metro_single_instance.rs`, 2 tests) and COVER-02 (`tests/process_group_kill.rs`, 1 test) pass unchanged — D-22 invariant preserved.

## Task Commits

1. **Task 1: Widen TokioTaskHandle to a 3-field named struct (child_pid + cancel_token)** — `7b639af` (feat)
2. **Task 2: Implement SIGTERM → 200ms grace → SIGKILL escalation in TaskHandle::abort()** — `6385bbb` (feat)
3. **Task 3: Widen From<std::process::ExitStatus> to distinguish Killed (SIGKILL) from Cancelled (other signals)** — `d6fc39c` (feat)

**Plan metadata commit:** this SUMMARY (final).

## Files Created/Modified

- `src/infra/task_handle.rs` — full file rewrite landing across all three tasks. Net +192 / -10 lines (final file ≈ 290 LOC). Module-scope imports now include `ExitStatusExt`; module-scope `const CANCEL_GRACE_MS: u64 = 200`. Struct widened to 3 named fields; manual Debug impl; abort() body is the 6-step ladder with adjacent SAFETY comments; `From<std::process::ExitStatus>` matches on `ExitStatusExt::signal()` to emit Killed/Cancelled. Tests module gains 4 new entries from Tasks 1-2 (3-field smoke + 3 abort-guard tests) and 2 new gated entries from Task 3 (real-child SIGKILL/SIGTERM mapping).
- `src/app/effect_runner.rs` — single-line change at the SpawnTask construction site: `Box::new(crate::infra::task_handle::TokioTaskHandle(join_handle))` becomes the named-field constructor with `child_pid: 0` and a fresh `cancel_token`. Comment notes Plan 15-03 will wire the real pid via the ProcessStarted oneshot.

## Decisions Made

- **`f.debug_struct(...).finish_non_exhaustive()` for the manual Debug impl** — preferred over `.finish()` because `JoinHandle<()>` and `CancellationToken` are intentionally omitted (neither has meaningful Debug output for diagnostics). `finish_non_exhaustive()` signals "more fields exist but aren't printed" to log readers — clearer than `.finish()` which would imply those are all the fields.
- **Guard `pid <= 1` (≤, not just == 0)** — covers placeholder 0, init's 1, AND any future negative-cast accident. Test `abort_with_placeholder_pid_one_is_noop` exists specifically to lock the init guard against future regressions (per T-15-02-02 disposition).
- **Test child_pid value: 999_999 (not 0xDEAD_BEEF)** — discovered during Task 2 verification: 0xDEAD_BEEF (3735928559) wraps to a NEGATIVE i32 (exceeds i32::MAX), which the `pid <= 1` guard would refuse. Switched to 999_999 — clearly not a running process, but positive when cast. The same cast behavior is documented inline so future contributors don't fall into the same trap.
- **Module-scope `use std::os::unix::process::ExitStatusExt;`** — could have lived inside `fn from`, but module-scope alongside the other imports is more discoverable. The trait is needed only on Unix; the module isn't currently `#[cfg(unix)]`-gated, but every existing use of `tokio::process` in the codebase implicitly assumes Unix and the integration tests are already cfg-gated.
- **Fire-and-forget grace task** — `tokio::spawn` returns a `JoinHandle` we intentionally drop. Discussed T-15-02-03 in the threat model: if the runtime exits before the 200ms sleep elapses, the task is dropped (no leak); if not, the worst case is one stray no-op SIGKILL after the process already died (acceptable).
- **`sleep 30` fixture (not a custom shell script)** — simplest possible child for the signal-mapping tests. POSIX `sleep` accepts any signal and dies; no custom signal handling, no script files to manage. `process_group(0)` makes the child its own PGID so `libc::kill(-pid, ...)` reaches just that child.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Test child_pid value 0xDEAD_BEEF wraps to negative i32 and triggers the pid<=1 guard**
- **Found during:** Task 2 verification — `abort_with_dead_pid_does_not_panic` failed because `0xDEAD_BEEF as i32 = -559038737` (≤ 1), so the guard returned early and `cancel_token.is_cancelled()` was false instead of true.
- **Issue:** The plan specified 0xDEAD_BEEF as "a clearly-not-running pid", but the literal exceeds i32::MAX (2^31 - 1 = 2147483647) so the `as i32` cast wraps to a negative number, which the guard treats as invalid.
- **Fix:** Switched to `child_pid: 999_999` — clearly-not-running but positive when cast. Added an inline code comment documenting the trap so future contributors don't fall into it.
- **Files modified:** `src/infra/task_handle.rs` (test fixture only, no production code change)
- **Commit:** Included in `6385bbb`

**2. [Rule 3 - Blocking] Unused `use std::os::unix::process::CommandExt` imports in Task 3 tests**
- **Found during:** Task 3 verification — both signal-mapping tests imported `CommandExt` (the std unix extension trait) thinking it was needed for `.process_group(0)`, but tokio's `tokio::process::Command::process_group(0)` is provided directly by tokio without needing the std trait. Compiler emitted `unused_imports` warning, which would fail clippy `-D warnings`.
- **Fix:** Removed both `use std::os::unix::process::CommandExt;` lines from inside the test functions.
- **Files modified:** `src/infra/task_handle.rs` (test imports only)
- **Commit:** Included in `d6fc39c`

No Rule 4 architectural deviations — plan executed exactly as designed.

## Issues Encountered

None beyond the two auto-fixes documented above. Both were discovered by the test gates immediately and fixed before commit.

## User Setup Required

None — pure code change with no external service configuration. POSIX-only signal handling is already standard on the supported targets (macOS, Linux CI).

## Threat Flags

None — the threat model in `15-02-PLAN.md` (T-15-02-01 through T-15-02-04 plus T-15-02-SC) covers every surface introduced by this plan. Specifically:
- T-15-02-02 (kill(-1) DoS): mitigated by the `pid <= 1` guard + `abort_with_placeholder_pid_one_is_noop` test.
- T-15-02-03 (grace-task leak): accepted; fire-and-forget pattern documented.
- T-15-02-04 (placeholder-pid repudiation): accepted; transient window between effect_runner spawn and Plan 15-03's oneshot delivery; cancel_token + join_handle.abort() still fire for cooperative cancel.

No new security-relevant surface beyond what the plan enumerated.

## Verification Results (Acceptance Gate)

- `cargo build --quiet` — green (exit 0)
- `cargo test --lib --quiet` — **104 passed, 0 failed** (98 → 104, +6 new in task_handle::tests)
- `cargo test --test process_group_kill --quiet` — **1 passed, 0 failed** (COVER-02 unchanged, D-22 preserved)
- `cargo test --test metro_single_instance --quiet` — **2 passed, 0 failed** (COVER-01 unchanged)
- `cargo clippy --all-targets -- -D warnings` — clean (no warnings, no output)
- `make arch-lint` — **PASS** (21 G-XX guards green)
- `grep -c 'pub child_pid: u32' src/infra/task_handle.rs` → 1
- `grep -c 'pub cancel_token: tokio_util::sync::CancellationToken' src/infra/task_handle.rs` → 1
- `grep -c 'pub struct TokioTaskHandle(' src/infra/task_handle.rs` → 0 (tuple form gone)
- `grep -c 'libc::SIGTERM' src/infra/task_handle.rs` → 1
- `grep -c 'libc::SIGKILL' src/infra/task_handle.rs` → 1
- `grep -c 'CANCEL_GRACE_MS: u64 = 200' src/infra/task_handle.rs` → 1
- `grep -c 'if pid <= 1' src/infra/task_handle.rs` → 2 (guard + comment reference)
- `grep -c 'ExitStatus::Killed' src/infra/task_handle.rs` → 2
- `grep -c 'ExitStatus::Cancelled' src/infra/task_handle.rs` → 6
- `grep -c 'cancel_token\.cancel()' src/infra/task_handle.rs` → 4

## Self-Check: PASSED

Files verified to exist:
- FOUND: src/infra/task_handle.rs (3-field struct, abort() ladder, signal-aware From impl, 9 tests in tests module)
- FOUND: src/app/effect_runner.rs (named-field constructor with placeholder pid=0)

Commits verified to exist:
- FOUND: 7b639af (feat(15-02): widen TokioTaskHandle to 3-field named struct)
- FOUND: 6385bbb (feat(15-02): implement SIGTERM → 200ms grace → SIGKILL escalation)
- FOUND: d6fc39c (feat(15-02): widen From<ExitStatus> to distinguish Killed/Cancelled)

## Next Phase Readiness

- **Plan 15-03 unblocked.** The TokioTaskHandle struct now accepts `{ join_handle, child_pid, cancel_token }` — Plan 15-03 can:
  1. Read `CommandEvent::ProcessStarted { pid }` from the runner stream (Plan 15-01 emits it first).
  2. Construct a fully-armed handle: `TokioTaskHandle { join_handle, child_pid: pid, cancel_token: token.clone() }`.
  3. Hand a clone of `cancel_token` to the forwarding loop's `tokio::select!` so the `.cancelled()` future short-circuits the recv loop and emits `Action::CommandExited { task_id, status: ExitStatus::Cancelled }`.
- **ROADMAP success criterion 1 mechanically achievable.** With Plan 15-01's `.process_group(0)` + this plan's SIGTERM-to-PGID ladder, the only remaining wiring is Plan 15-03's cancel_token clone + Plan 15-05's `CommandCancel` is_cancellable() gate. `ps aux` after cancel will show no orphans because the PGID broadcast reaches every grandchild.
- **Plan 15-05 (CommandCancel gate) still needed.** This plan's `abort()` does NOT check `is_cancellable()` — calling it on a git task would broadcast SIGTERM to git, which is wrong. The is_cancellable gate lives in `update.rs CommandCancel` (Plan 15-05). The data path is now ready; the policy gate is essential.
- **Plan 15-06 (subprocess integration test) is the proof point.** That plan will spawn a real `yarn install`-shaped tree and assert that abort() + 200ms grace actually leaves zero orphans. The unit tests in this plan cover the mapping logic + guard logic; the end-to-end subprocess proof is correctly deferred to 15-06.
- **D-22 invariant preserved.** COVER-01 and COVER-02 still pass unchanged.

---
*Phase: 15-task-cancellation-collision-shared-resource-semaphore*
*Completed: 2026-05-19*
