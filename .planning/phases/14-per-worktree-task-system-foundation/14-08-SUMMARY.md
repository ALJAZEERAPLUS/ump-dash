---
phase: 14
plan: "08"
subsystem: testing
tags: [dispatch-tests, per-worktree, slice-assertions, parallelism, routing, stale-drop]
dependency_graph:
  requires: ["14-06", "14-07"]
  provides: ["D-21 test assertions", "TASK-02 contract tests", "TASK-03 contract tests", "P-3 stale-drop test"]
  affects: ["src/app/dispatch_tests.rs"]
tech_stack:
  added: []
  patterns:
    - "Slice-side test assertions via assert_running_in / assert_no_running_task_anywhere helpers"
    - "synthetic_task_record fixture for no-runtime task seeding"
    - "NoopHandle: test-only TaskHandle impl gated behind cfg(test) context"
key_files:
  created: []
  modified:
    - src/app/dispatch_tests.rs
decisions:
  - "assert_running_in asserts slice.task.is_some() — forward-compatible with Plan 14-09 global deletion"
  - "command_exited_with_nonempty_queue test asserts queue drained not slice.task set — SpawnTask effect not processed in pure update() unit test"
  - "metro_start test uses inline FakeMetroHandle struct instead of exposing common/mod.rs — keeps unit test self-contained"
  - "seed_two_worktrees helper keeps selection on index 0 (wt-A) to ensure active_worktree_id returns A by default"
metrics:
  duration: "~10 minutes"
  completed: "2026-04-28T05:59:24Z"
  tasks_completed: 2
  files_changed: 1
---

# Phase 14 Plan 08: Dispatch Tests — Slice Assertion Rewrite + New Tests

Rewrites all `command_queue` module assertions to read from `state.worktrees` (per D-21), adds 4 helpers plus 1 fixture, and introduces 5 new tests proving TASK-02 + TASK-03 contracts and the P-3 stale-drop mitigation.

## What Was Built

### Helpers Added (4 + 1 fixture)

| Helper | Location | Purpose |
|--------|----------|---------|
| `assert_running_in(state, id)` | dispatch_tests.rs top | Assert slice for `id` has `task.is_some()` |
| `assert_no_running_task_anywhere(state)` | dispatch_tests.rs top | Assert every slice has `task.is_none()` |
| `slice_queue_len(state, id) -> usize` | dispatch_tests.rs top | Queue length for named slice |
| `slice_output(state, id) -> Vec<String>` | dispatch_tests.rs top | Output snapshot for named slice |
| `synthetic_task_record(id_value, spec) -> TaskRecord` | dispatch_tests.rs top | Build a TaskRecord with NoopHandle for seeding slice.task in tests |
| `NoopHandle` (struct) | dispatch_tests.rs top | Test-only TaskHandle impl; abort() is a no-op |
| `seed_one_worktree_id(state, id)` | dispatch_tests.rs top | Seeds a named worktree + slice |
| `seed_two_worktrees(state, id_a, id_b)` | dispatch_tests.rs top | Seeds two worktrees + slices |

### 3 Existing Tests Rewritten (D-21)

All 3 tests in `mod command_queue` now assert against `state.worktrees` (slice-side):

| Test | Old assertion | New assertion |
|------|--------------|--------------|
| `command_queue_push_appends_to_back` | `command_runner.command_queue.len()` | `slice_queue_len(&state, "wt-1")` |
| `command_exited_with_empty_queue_clears_running_command` | `command_runner.running_command.is_none()` | `assert_no_running_task_anywhere(&state)` |
| `command_exited_with_nonempty_queue_pops_and_dispatches_front` | `command_runner.running_command == YarnInstall` | `slice_queue_len(&state, "wt-1") == 1` + `Effect::SpawnTask` |

The remaining 15 tests (palette_resolution, modal_dismissal, worktrees_loaded) had no `command_runner` references and required no changes.

### 5 New Tests Added

| Module | Test Name | Contract Proven |
|--------|-----------|----------------|
| `parallelism` | `yarn_install_on_a_while_jest_on_b_both_have_tasks` | TASK-02: parallel tasks across worktrees |
| `parallelism` | `metro_start_on_a_while_metro_running_on_b_keeps_single_instance` | COVER-01 / D-13: single metro instance |
| `routing` | `command_output_line_routes_to_correct_slice_regardless_of_active_worktree` | TASK-03 / D-08: routing by task_id |
| `routing` | `command_exited_drains_slice_local_queue_not_other` | TASK-03 / D-11: slice-local drain |
| `stale_drop` | `late_command_output_line_for_cancelled_task_is_silently_dropped` | P-3: stale line dropped |

## Test Count Delta

| Metric | Before | After |
|--------|--------|-------|
| dispatch tests | 18 | 23 |
| lib tests total | 91 | 96 |
| integration tests | 3 | 3 |
| **total** | **94** | **99** |

## Verification Results

- `cargo test --lib dispatch_tests` — 23 passed, 0 failed
- `cargo test --workspace` — 99 passed total (96 lib + 2 metro_single_instance + 1 process_group_kill)
- `cargo clippy --all-targets -- -D warnings` — clean, no warnings
- `make arch-lint` — PASS (all 20 G-XX guards green)
- `rg -c 'state\.worktrees\.get' src/app/dispatch_tests.rs` — 14 (>= 8 required)
- COVER-01 (`tests/metro_single_instance.rs`) — 2 tests pass unchanged
- COVER-02 (`tests/process_group_kill.rs`) — 1 test passes unchanged

## Forward Compatibility

All new tests use slice-side assertions only. They are designed to continue passing after Plan 14-09 deletes the legacy `CommandRunnerState` global fields:

- No new test asserts `state.command_runner.*`
- `command_exited_with_nonempty_queue` asserts `slice_queue_len` and `Effect::SpawnTask` — both will still hold after legacy deletion
- The `NoopHandle` is `#[cfg(test)]`-scoped — not visible in production builds

## Deviations from Plan

### Auto-deviation: seed_one_worktree refactored to delegate

The plan assumed a single `seed_one_worktree` helper targeting `"wt-1"`. The implementation extracted `seed_one_worktree_id(id)` as the general form, with `seed_one_worktree` delegating to it. This supports the 5 new tests that need different worktree IDs without duplicating worktree/slice construction code.

### Auto-deviation: command_exited_with_nonempty_queue assertion strategy

The plan's action template included `assert_running_in(&state, "wt-1")` after drain. However, `dispatch_command` only emits `Effect::SpawnTask` — it does NOT write to `slice.task` (the runtime does that when processing the effect). A pure unit test cannot see `slice.task` populated post-drain. The assertion was changed to `slice_queue_len == 1` (queue drained) + `Effect::SpawnTask emitted`, which correctly proves the drain happened without requiring a runtime. Comment added explaining why `slice.task` is not asserted here.

## Self-Check

### Created files

None (plan modifies only `src/app/dispatch_tests.rs`).

### Commits

- `bd279de` — Task 1: helpers + rewrite command_queue assertions
- `92a93c9` — Task 2: 5 new parallelism / routing / stale-drop tests
