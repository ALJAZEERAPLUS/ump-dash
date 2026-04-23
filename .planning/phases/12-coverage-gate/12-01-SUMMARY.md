---
phase: 12-coverage-gate
plan: 01
subsystem: testing
tags: [rust, tokio, characterization-test, metro, single-instance-invariant, should-panic, tea-integration-test, mpsc]

# Dependency graph
requires:
  - phase: 12-coverage-gate
    provides: "Plan 12-00 bin+lib scaffolding — src/lib.rs with pub mod action/app/domain/event/infra/tui/ui; Cargo.toml [dev-dependencies] tokio with macros/rt-multi-thread/process/time; tests/common/mod.rs with fake_metro_handle(pid, worktree) helper. Without these the integration test could not `use rn_dash::app::{update, AppState}` or get a runtime-ready MetroHandle for state.metro.register()."
provides:
  - "src/domain/metro.rs inline `#[cfg(test)] mod tests` — 3 characterization tests locking the MetroManager single-instance invariant at the type level (D-09 layer 1)"
  - "tests/metro_single_instance.rs — 2 characterization tests locking the update()-level MetroStart double-dispatch guard at the TEA boundary (D-09 layer 2)"
  - "Regression trip-wire for Phase 13 REFACTOR — any change that (a) drops the `assert!(self.handle.is_none(), …)` in MetroManager::register, or (b) removes the `if state.metro.is_running() { state.pending_restart = true; update(state, MetroStop, …); return; }` branch in Action::MetroStart, will fail one of the 5 tests in < 1 s"
  - "Worked example of the receiver-pinning idiom for update()-level integration tests: hold `_metro_rx` / `_handle_rx` in bindings for the life of the test to dodge the `channel closed` landmine (12-RESEARCH.md Pitfall 10)"
affects: [13-refactor, 14-task-system-rewrite, 12-03-dispatch-tests, 12-04-baseline-coverage]

# Tech tracking
tech-stack:
  added: []  # All deps already present from 12-00 (tokio dev-feature macros/rt-multi-thread, anyhow)
  patterns:
    - "Two-layer characterization: inline `#[cfg(test)] mod tests` for pure-struct invariants (D-06) + `tests/` integration test for cross-layer TEA behavior (D-07). Both layers cover the SAME requirement (COVER-01) at different abstraction levels so either a type-level refactor OR a state-machine refactor that drops the guard will be caught."
    - "`#[should_panic(expected = \"<prefix>\")]` substring-match pattern: lock on the load-bearing prefix `BUG: MetroManager::register() called with an existing handle` and deliberately OMIT the em-dash-suffix `— kill first` so that future punctuation tweaks (en-dash ↔ em-dash ↔ hyphen) don't destabilize the test."
    - "Negative-control test pattern: `metro_start_when_stopped_does_not_set_pending_restart` asserts the inverse of the positive test, catching the failure mode where a refactor accidentally makes `pending_restart` flip to `true` unconditionally (regressing the guard to a no-op)."
    - "`state.metro.register(fake_metro_handle(pid, wt))` is the canonical way to simulate 'metro is already running' in a TEA test without actually spawning a child process — establishes `is_running() == true` and a legal `MetroStatus::Running { pid, worktree_id }` without touching infra."
    - "Status matrix in assertions uses `matches!(state.metro.status, MetroStatus::Running { pid: 9999, .. } | MetroStatus::Stopping)` — acknowledges that `update(_, MetroStart, ..)` recursively calls `update(_, MetroStop, ..)` which may transition status synchronously, so the assertion accepts either the pre-stop OR the Stopping state but not any fresh `Running{pid: ≠9999}`."

key-files:
  created:
    - "tests/metro_single_instance.rs"
  modified:
    - "src/domain/metro.rs (appended `#[cfg(test)] mod tests` block — 3 inline tests)"

key-decisions:
  - "Used `#[tokio::test]` (not `#[test]`) for both `register_twice_panics` and `register_once_then_clear_allows_second_register` inline tests, because `dummy_handle()` calls `tokio::spawn(async {})` to produce the `JoinHandle` fields of `MetroHandle`. `tokio::spawn` requires a runtime context — a plain `#[test]` would panic with 'there is no reactor running'. The smoke test `new_manager_is_stopped_not_running` stays `#[test]` because it builds no `MetroHandle`."
  - "Used substring-match `#[should_panic(expected = \"BUG: MetroManager::register() called with an existing handle\")]` (NOT the full message with the em-dash suffix `— kill first`). Rationale: the prefix is load-bearing semantic content; the em-dash is purely stylistic. Future punctuation tweaks (e.g., en-dash, hyphen, comma) MUST not destabilize the characterization."
  - "In the update()-level test, the assertion on `state.metro.status` accepts EITHER the original `Running { pid: 9999, .. }` or `Stopping` — explicitly NOT a fresh `Running { pid: ≠ 9999 }`. This acknowledges the handler's recursive `update(state, Action::MetroStop, ...)` call which sets status to Stopping synchronously; the test's real characterization target is 'metro is NOT a fresh second Running state', not 'metro has any specific status'."
  - "Held receivers in `_metro_rx` and `_handle_rx` bindings for the life of each #[tokio::test], not discarded with `_` alone. `Action::MetroStart` handler at `src/app.rs:600-608` does `tokio::spawn` a follow-up task that sends to `metro_tx`; if the receiver is dropped first, the spawned task panics with 'channel closed' and the test flakes (12-RESEARCH.md Pitfall 10)."

patterns-established:
  - "D-09 two-layer characterization: COVER-01 is locked at BOTH the type boundary (MetroManager::register assert) AND the TEA boundary (update() dispatch branch). A single refactor that drops either layer is caught independently. Future cross-cutting invariants that span domain + app should use the same two-layer pattern."
  - "Negative-control test pattern: for every assertion of the form 'X must happen when Y', add a sibling assertion of the form 'X must NOT happen when NOT Y'. Catches the refactor failure mode where the guard becomes a no-op (always-fires)."
  - "Receiver-pinning for TEA integration tests: when the action handler spawns followup tokio tasks that write to tx, hold the corresponding rx in a `let _rx = ...;` binding for the entire test body — do not rely on `_ = chan_rx` or immediate drop."

requirements-completed: [COVER-01]

# Metrics
duration: 5min
completed: 2026-04-23
---

# Phase 12 Plan 01: Metro Single-Instance Characterization Summary

**5 characterization tests locking the metro single-instance invariant at two layers: 3 inline in `src/domain/metro.rs` (MetroManager::register panic) + 2 in `tests/metro_single_instance.rs` (update()-level MetroStart double-dispatch → pending_restart flip). All pass in < 0.01 s of test time, clippy clean.**

## Performance

- **Duration:** ~5 min (inspect salvaged unit tests, write integration test, verify, summarize)
- **Started:** 2026-04-23T18:42:06Z
- **Completed:** 2026-04-23T18:43:29Z (plus ~3 min summary / state update — wall-time ~5 min)
- **Tasks:** 2 (01.1 inline metro tests inherited as uncommitted draft; 01.2 integration test written from scratch)
- **Files modified:** 1 modified (`src/domain/metro.rs`), 1 created (`tests/metro_single_instance.rs`)

## Accomplishments

- **3 inline tests** at `src/domain/metro.rs`:
  - `register_twice_panics` — `#[should_panic]` characterization of the load-bearing `assert!` in `MetroManager::register` (D-09 layer 1).
  - `register_once_then_clear_allows_second_register` — positive-case safety net for the legitimate register → clear → register sequence.
  - `new_manager_is_stopped_not_running` — smallest possible smoke test (no runtime, no handle).
- **2 integration tests** at `tests/metro_single_instance.rs`:
  - `metro_start_while_running_triggers_restart_not_double_spawn` — dispatches `Action::MetroStart` through `rn_dash::app::update` with a pre-registered fake handle, asserts `pending_restart` flipped true and `metro.status` is either `Running { pid: 9999, .. }` or `Stopping` (never a fresh second Running) (D-09 layer 2).
  - `metro_start_when_stopped_does_not_set_pending_restart` — negative control that catches the refactor failure mode where `pending_restart` is always flipped.
- **Wall-clock test time:** both test binaries finish in < 0.01 s (29 lib tests in `finished in 0.00s`, 2 integration tests in `finished in 0.00s`). Well under the plan's < 1 s budget.
- **Full suite:** `cargo test --quiet` → 32/32 passing (29 lib + 2 metro_single_instance + 1 process_group_kill).
- **Clippy:** `cargo clippy --all-targets -- -D warnings` → clean.

## Task Commits

1. **Task 01.1: Inline `#[cfg(test)] mod tests` for MetroManager register invariant** — `a9ddd3b` (test)
2. **Task 01.2: `tests/metro_single_instance.rs` update()-level double-dispatch integration test** — `49e69f6` (test)

**Plan metadata commit:** [this SUMMARY + STATE + ROADMAP + REQUIREMENTS commit — hash recorded after it lands]

## Files Created/Modified

- `src/domain/metro.rs` — appended `// --- Tests --- ` comment bar + `#[cfg(test)] mod tests { … }` block (~59 lines; 3 tests + `dummy_handle` helper)
- `tests/metro_single_instance.rs` (72 lines, newly created) — module-level doc explaining the D-09 layer-2 characterization target, 2 `#[tokio::test]` functions, receiver-pinning pattern

## Decisions Made

- **`#[tokio::test]` for MetroHandle-constructing tests, `#[test]` for the smoke test** — `dummy_handle()` needs a tokio runtime for `tokio::spawn`.
- **Substring-match `#[should_panic(expected = …)]`** — use a stable load-bearing prefix, not the full message with stylistic em-dash. Future punctuation tweaks must not destabilize.
- **Status assertion accepts `Running { pid: 9999, .. } | Stopping`** — the handler recursively calls `update(_, MetroStop, _, _)` which may set status to Stopping synchronously; either pre-stop OR Stopping is acceptable, NEVER a fresh `Running{pid: ≠9999}`.
- **Receivers held in `_metro_rx` / `_handle_rx` bindings** — dropping the receivers before the test body completes would break the spawned task at `src/app.rs:600-608` with 'channel closed' and flake CI.

## Deviations from Plan

None — plan executed exactly as written.

The Task 01.1 inline tests were salvaged verbatim from a prior agent's partial work (matching the plan's `<action>` block byte-for-byte). Task 01.2 was written fresh following the plan's `<action>` block verbatim. Verified:

- `grep -c '^#\[cfg(test)\]$' src/domain/metro.rs` → 1 ✓
- `grep -c '#\[should_panic' src/domain/metro.rs` → 1 ✓
- `grep -q 'fn register_twice_panics' src/domain/metro.rs` → ✓
- `grep -q 'fn register_once_then_clear_allows_second_register' src/domain/metro.rs` → ✓
- `grep -q 'fn new_manager_is_stopped_not_running' src/domain/metro.rs` → ✓
- `grep -q 'mod common;' tests/metro_single_instance.rs` → ✓
- `grep -q 'use common::fake_metro_handle;'` → ✓
- `grep -q 'use rn_dash::app::{update, AppState};'` → ✓
- `grep -q 'use rn_dash::action::Action;'` → ✓
- `grep -c '#\[tokio::test\]' tests/metro_single_instance.rs` → 2 ✓
- `grep -q 'fn metro_start_while_running_triggers_restart_not_double_spawn'` → ✓
- `grep -q 'fn metro_start_when_stopped_does_not_set_pending_restart'` → ✓

## Issues Encountered

- **Partial work on main working tree:** `src/domain/metro.rs` had an uncommitted `#[cfg(test)] mod tests` block appended by a prior agent. Diagnosis confirmed the block matched the plan's `<action>` verbatim and all 3 tests passed — no rewrite needed, just commit.
- **No other issues.** `tests/metro_single_instance.rs` compiled on first try; both tests passed on first run.

## User Setup Required

None.

## Next Phase Readiness

- **COVER-01 requirement done.** Phase 12 wave 2 progress: 12-01 + 12-02 complete; 12-03 (dispatch tests) is next within wave 2; 12-04 (baseline coverage + thresholds) is wave 3.
- **For Phase 13 REFACTOR:** Two independent trip-wires:
  1. Any refactor that drops the `assert!(self.handle.is_none(), …)` in `MetroManager::register` will fail `register_twice_panics`.
  2. Any refactor that changes the `Action::MetroStart` handler to spawn a second MetroHandle without routing through the `pending_restart = true; update(…, MetroStop, …); return;` sequence will fail `metro_start_while_running_triggers_restart_not_double_spawn`.
- **For Phase 14+ TASK-SYSTEM rewrite:** The TEA `update()` entry point is characterized at the MetroStart boundary — future task-system changes that relocate this logic must preserve the same state transition (pending_restart flip + metro not-fresh-running).

## Self-Check: PASSED

- File exists: `src/domain/metro.rs` (modified) → FOUND
- File exists: `tests/metro_single_instance.rs` (created) → FOUND
- Commit exists: `a9ddd3b` → FOUND (`git log --oneline | grep a9ddd3b`)
- Commit exists: `49e69f6` → FOUND (`git log --oneline | grep 49e69f6`)
- `cargo test --lib domain::metro::tests --quiet` → 3 passed in 0.00s
- `cargo test --test metro_single_instance --quiet` → 2 passed in 0.00s
- `cargo test --quiet` (full suite) → 32/32 passed
- `cargo clippy --all-targets -- -D warnings` → clean
- Plan success criteria satisfied: 5 new tests total, 0 regressions, both layers of D-09 covered.

---
*Phase: 12-coverage-gate*
*Completed: 2026-04-23*
