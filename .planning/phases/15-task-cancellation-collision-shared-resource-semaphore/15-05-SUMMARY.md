---
phase: 15-task-cancellation-collision-shared-resource-semaphore
plan: 05
subsystem: app/update + app/dispatch_tests
tags: [update, collision-gate, is-cancellable-gate, dispatch, command-cancel, TASK-04, TASK-05]
requires: [15-03, 15-04]
provides:
  - "Action::CommandCancel handler honors CommandSpec::is_cancellable()"
  - "dispatch_command consults CollisionPolicy at (discriminant, WorktreeId) collision"
affects:
  - "Closes [ASSUMED] is_cancellable gate from Phase 14 CommandCancel"
  - "Closes F-501 deferred collision logic"
  - "ROADMAP success criterion 2 (git porcelain cannot be cancelled) at reducer level"
  - "ROADMAP success criterion 3 (collision per documented policy) at reducer level"
tech-stack:
  added: []
  patterns:
    - "Take-then-maybe-reinsert: take the Option, inspect owned value, either consume or put back — avoids borrow-checker conflict from holding &mut while inspecting"
    - "Two-pass collision gate: immutable read of discriminant first, then mutable take + abort — avoids holding mut borrow across the handle.abort() call"
key-files:
  created: []
  modified:
    - "src/app/update.rs (+60 LOC) — CollisionPolicy import; is_cancellable gate in CommandCancel; collision gate prefix in dispatch_command"
    - "src/app/dispatch_tests.rs (+251 LOC) — mod collision (4 tests) + mod cancellation_guard (2 tests)"
decisions:
  - "YarnJest collision test uses TextInputSubmit modal path — Action::CommandRun(YarnJest) routes through TextInput modal before reaching dispatch_command, so the test simulates the modal-submit path to hit the gate directly"
  - "Different-discriminant test uses YarnLint instead of YarnJest — YarnLint has no text-input pre-processing so Action::CommandRun reaches dispatch_command directly"
metrics:
  duration: "~25 min"
  completed: "2026-05-19"
  tasks: "3/3"
  files: "2 modified, 0 created"
  tests_added: 6
  lib_tests_total: 115
---

# Phase 15 Plan 05: CommandCancel is_cancellable Gate + dispatch_command Collision Gate Summary

is_cancellable() guard in Action::CommandCancel + collision_policy() gate in dispatch_command — the two TEA-reducer changes that consume Plan 15-04's CollisionPolicy and close TASK-04 + TASK-05 at the reducer level.

## What Shipped

Two TEA-level gates landed in `src/app/update.rs`:

1. **CommandCancel is_cancellable guard (Task 1)** — Phase 14's CommandCancel handler previously called `record.handle.abort()` unconditionally. Now the handler takes the record, checks `record.spec.is_cancellable()`, and either (a) aborts + clears queue + pushes `[cancelled]` (cancellable variants — yarn/rn/clean/shell/adb) OR (b) re-inserts the record without touching queue or output (non-cancellable — all 8 git porcelain variants). The take-then-maybe-reinsert pattern bypasses the borrow-checker complaint that would arise from inspecting `slice.task.as_ref()` mutably-or-immutably across the abort.

2. **dispatch_command collision gate (Task 2)** — Before allocating a new TaskId or writing the `$ argv` line, the function now checks `(std::mem::discriminant, WorktreeId)` against `slice.task`. On match, `spec.collision_policy()` decides:
   - `BlockNew` → immediate `return None`; slice.task, slice.queue, slice.output all untouched.
   - `CancelPrevious` → `record.handle.abort()` on the existing record, `slice.queue.clear()`, push `[cancelled by new dispatch]` to slice.output, then fall through to the normal dispatch path with the NEW task_id.

   The collision check uses a two-pass borrow (immutable read of `existing.spec` discriminant, then mutable take + abort) to avoid holding `slice` mutably while calling `record.handle.abort()`.

3. **6 new inline tests (Task 3)** — Two new sub-modules added at the end of `src/app/dispatch_tests.rs`:
   - `mod collision` (4 tests): BlockNew for `YarnInstall × YarnInstall`, CancelPrevious for `YarnJest × YarnJest` (via TextInputSubmit path), different-discriminant non-collision for `YarnInstall + YarnLint`, BlockNew for `GitPull × GitPull` (Q-4 honor).
   - `mod cancellation_guard` (2 tests): non-cancellable `GitPull` re-inserts record + preserves queue, cancellable `YarnInstall` aborts + clears queue + pushes `[cancelled]`.

## Tasks

| # | Name | Commit | Files |
| - | - | - | - |
| 1 | is_cancellable() guard in CommandCancel | `7ada3c3` | `src/app/update.rs` |
| 2 | collision_policy() gate in dispatch_command | `f5dfdac` | `src/app/update.rs` |
| 3 | mod collision + mod cancellation_guard tests | `80f8b1e` | `src/app/dispatch_tests.rs` |

## Verification

- `cargo build --quiet` — green
- `cargo test --lib --quiet` — **115 passed** (109 baseline + 6 new), 0 failed
- `cargo test --test process_group_kill --quiet` — 1 passed (COVER-02 unchanged)
- `cargo test --test metro_single_instance --quiet` — 2 passed (COVER-01 unchanged)
- `cargo clippy --all-targets -- -D warnings` — clean
- `make arch-lint` — 21 G-XX guards green (PASS)
- Acceptance grep checks all satisfied:
  - `grep -c 'record.spec.is_cancellable()' src/app/update.rs` = 1
  - `grep -c 'slice.task = Some(record);' src/app/update.rs` = 1
  - `grep -c 'CollisionPolicy::BlockNew => return None' src/app/update.rs` = 1
  - `grep -c 'std::mem::discriminant' src/app/update.rs` = 1
  - `grep -c 'mod collision' src/app/dispatch_tests.rs` = 1
  - `grep -c 'mod cancellation_guard' src/app/dispatch_tests.rs` = 1

## Deviations from Plan

### [Rule 1 - Bug] Fixed clippy::collapsible_if violation in CommandCancel handler

- **Found during:** Task 1 verification (`cargo clippy --all-targets -- -D warnings`)
- **Issue:** The plan's target shape used `if let Some(record) = slice.task.take() {` nested inside `if let Some(slice) = state.worktrees.get_mut(id) {`, but the outer block uses `if let .. && let ..` chained pattern. Clippy's `collapsible_if` flagged the nested `if let Some(record) = ...` as collapsible into the chain.
- **Fix:** Extended the let-chain to include `&& let Some(record) = slice.task.take()`; moved the `is_cancellable()` decision into the inner body.
- **Files modified:** `src/app/update.rs` (CommandCancel handler)
- **Commit:** `7ada3c3`

### [Rule 1 - Bug] Fixed doc-comment over-indented list items in dispatch_command

- **Found during:** Task 2 verification (`cargo clippy --all-targets -- -D warnings`)
- **Issue:** Initial doc-comment used `///   - BlockNew     → ...` (3-space indent on list items) which clippy's `doc_overindented_list_items` rejects.
- **Fix:** Flattened to single-space indent (`/// - BlockNew → ...`).
- **Files modified:** `src/app/update.rs` (dispatch_command doc-comment)
- **Commit:** `f5dfdac`

### [Rule 1 - Bug] CancelPrevious test must use TextInputSubmit path for YarnJest

- **Found during:** Task 3 first test run (`collision_cancel_previous_yarn_jest_replaces_task` panicked: "got []")
- **Issue:** The plan instructed: "call `update(&mut state, Action::CommandRun(CommandSpec::YarnJest { filter: "second" }))`". But `CommandSpec::YarnJest` has `needs_text_input() == true`, so `Action::CommandRun(YarnJest { .. })` routes to the TextInput modal — never reaching `dispatch_command`. No SpawnTask Effect is emitted.
- **Fix:** Test now stages a `ModalState::TextInput { template: YarnJest, buffer: "second" }` and submits via `Action::ModalInputSubmit`, which routes through the actual TextInput-submit path that composes the real `YarnJest { filter: "second" }` and calls `dispatch_command`. This is the only path where YarnJest reaches the gate, and it correctly exercises the CancelPrevious + replacement semantics. Test comment documents the rationale.
- **Files modified:** `src/app/dispatch_tests.rs` (`collision_cancel_previous_yarn_jest_replaces_task`)
- **Commit:** `80f8b1e`

### [Rule 1 - Bug] Different-discriminant test must use no-text-input spec

- **Found during:** Implementing Task 3 (anticipating the same failure mode)
- **Issue:** The plan suggested dispatching `YarnJest { filter: "x".into() }` for the no-collision test. Same problem: routes through TextInput modal, never reaches `dispatch_command`. Cannot assert that the gate is bypassed if the action never reaches the gate at all.
- **Fix:** Swapped to `CommandSpec::YarnLint` — same family (CancelPrevious), no text-input, no device-picker, reaches `dispatch_command` directly. Added an assertion that the new dispatch emits `Effect::SpawnTask` for the new spec, strengthening the test.
- **Files modified:** `src/app/dispatch_tests.rs` (`collision_different_discriminants_dispatch_normally`)
- **Commit:** `80f8b1e`

No auth gates. No architectural changes (Rule 4 not triggered). No stubs introduced.

## Decisions Made

- **YarnJest cannot be tested via direct `CommandRun` dispatch** — it always opens the TextInput modal first. Tests that want to exercise YarnJest's path through `dispatch_command` MUST simulate the TextInput modal + submit sequence. Documented inline in the test comment for future contributors.
- **Two-pass collision gate** chosen over single-pass mutable borrow — clearer borrow story, no `unsafe`, no NLL surprises.
- **`[cancelled by new dispatch]` distinct from `[cancelled]`** — the two output lines disambiguate user-initiated cancel vs. collision-induced replacement.

## Known Stubs

None. All claimed behavior is wired and covered by tests.

## Threat Surface Scan

No new threat surface beyond what was documented in 15-05-PLAN.md's threat register. Both T-15-05-01 (SIGTERM at running git mid-flight) and T-15-05-03 (yarn install DoS) are now mitigated and verified by tests. T-15-05-02 (silent no-op) and T-15-05-04 (race window) remain accepted per plan.

## Phase Impact

- **ROADMAP success criterion 2** (git porcelain cannot be cancelled): VERIFIED at reducer level by `cancel_on_git_pull_is_noop_record_reinserted`.
- **ROADMAP success criterion 3** (collision per documented policy): VERIFIED at reducer level by all 4 `mod collision` tests.
- **ROADMAP success criterion 1** (timing within 2s) and **4** (semaphore serialization) remain Plan 15-06's integration-test scope — these require real subprocess spawning, which the synthetic `NoopHandle` cannot exercise.
- **Wave 5 unblocked**: Plan 15-06 (integration tests with real subprocesses) can now assert end-to-end timing and serialization with confidence that the underlying TEA gates are correct.

## Self-Check: PASSED

Files verified to exist:
- `src/app/update.rs` — FOUND (contains is_cancellable + CollisionPolicy gate)
- `src/app/dispatch_tests.rs` — FOUND (contains `mod collision` + `mod cancellation_guard`)
- `.planning/phases/15-task-cancellation-collision-shared-resource-semaphore/15-05-SUMMARY.md` — FOUND (this file)

Commits verified in git log:
- `7ada3c3` — FOUND (feat(15-05): add is_cancellable() guard to Action::CommandCancel)
- `f5dfdac` — FOUND (feat(15-05): add collision_policy() gate to dispatch_command)
- `80f8b1e` — FOUND (test(15-05): add collision + cancellation_guard sub-modules)
