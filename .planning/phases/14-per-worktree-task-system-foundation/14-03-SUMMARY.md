---
phase: 14
plan: 03
subsystem: app-state
tags: [state, update, merge-slices, worktrees-field, per-worktree-task-map, tdd]
dependency_graph:
  requires:
    - domain::worktree_slice::WorktreeSlice    # Plan 14-01
    - domain::task::TaskRecord                  # Plan 14-01
    - domain::worktree::WorktreeId              # existing
    - infra::task_handle::TokioTaskHandle       # Plan 14-02
  provides:
    - app::state::AppState.worktrees (HashMap<WorktreeId, WorktreeSlice>)
    - app::state::task_for_worktree helper
    - app::state::merge_slices helper
    - app::update::WorktreesLoaded wired to merge_slices
  affects:
    - src/app/state.rs
    - src/app/update.rs
    - src/app/dispatch_tests.rs
tech_stack:
  added: []
  patterns:
    - TEA pure reducer (merge_slices is a pure AppState mutation, no I/O)
    - HashMap at AppState root (D-16: not inside a sub-struct)
    - Q4 short-circuit (O(n) HashSet comparison guards identity refreshes)
    - explicit abort on slice removal (P-6 fix: no accidental task drops on 60s refresh)
    - TDD Red-Green commit sequence per plan tdd="true"
key_files:
  created: []
  modified:
    - src/app/state.rs
    - src/app/update.rs
    - src/app/dispatch_tests.rs
decisions:
  - "[D-16] worktrees: HashMap at AppState root — not inside any sub-struct; keeps access path clear"
  - "[D-17] merge_slices preserves surviving slices, aborts removed task handles, inserts new default slices"
  - "[D-07] task_for_worktree reads via slice key — no worktree_id backref inside TaskRecord"
  - "[Q4] Short-circuit when loaded set equals current set — one HashSet comparison, zero slice mutations"
  - "[P-6] 60s refresh pitfall closed — merge_slices retains survivors; no task drop on identity refresh"
  - "Cloned Vec<Worktree> in WorktreesLoaded handler to satisfy borrow checker (&mut state + &[Worktree])"
metrics:
  duration: "8 minutes"
  completed: "2026-04-28"
  tasks: 2
  files: 3
---

# Phase 14 Plan 03: AppState Worktrees Field + merge_slices Summary

**One-liner:** AppState gains `worktrees: HashMap<WorktreeId, WorktreeSlice>` at root, with `task_for_worktree` + `merge_slices` helpers and WorktreesLoaded wiring; 4 merge contract tests + 1 integration test; RESEARCH Pitfall P-6 and Q4 short-circuit fully verified.

## Tasks Completed

| Task | Name | Commit(s) | Files |
|------|------|-----------|-------|
| 1 (RED) | Failing tests for worktrees field + merge_slices | 3c0beec | src/app/state.rs |
| 1 (GREEN) | Add worktrees field + task_for_worktree + merge_slices | 1707eaa | src/app/state.rs |
| 2 (RED) | Failing test for WorktreesLoaded populates slice map | 545a3bf | src/app/dispatch_tests.rs |
| 2 (GREEN) | Wire merge_slices into WorktreesLoaded handler | cca7824 | src/app/update.rs |

## AppState Shape Change

### Before (Plan 14-02 baseline)

```rust
pub struct AppState {
    pub focused_panel: FocusedPanel,
    pub show_help: bool,
    pub error_state: Option<ErrorState>,
    pub should_quit: bool,
    pub metro: crate::domain::metro::MetroManager,
    pub metro_state: MetroState,
    pub worktree_browser: WorktreeBrowserState,
    pub command_runner: CommandRunnerState,   // still alive — Plan 14-09 deletes it
    pub modal_stack: ModalStackState,
    pub jira: JiraState,
    pub app_config: AppConfigState,
}
```

### After (this plan)

```rust
pub struct AppState {
    // ... all above unchanged, PLUS: ...
    /// Phase 14 / D-16: per-worktree task slice map at AppState root.
    pub worktrees: std::collections::HashMap<
        crate::domain::worktree::WorktreeId,
        crate::domain::worktree_slice::WorktreeSlice,
    >,
}
```

`command_runner: CommandRunnerState` is explicitly NOT deleted — that belongs to Plan 14-09.

## Helpers Added

### `task_for_worktree` (D-07)

```rust
pub fn task_for_worktree<'a>(
    state: &'a AppState,
    id: &crate::domain::worktree::WorktreeId,
) -> Option<&'a crate::domain::task::TaskRecord> {
    state.worktrees.get(id).and_then(|s| s.task.as_ref())
}
```

No worktree_id backref inside `TaskRecord` — the slice key provides the identity.

### `merge_slices` (D-17 + Q4)

Three behaviors:
1. **Surviving ids** — existing slice kept with task + queue + output + post_drain intact.
2. **Removed ids** — slice dropped; `handle.abort()` called explicitly if a task is running.
3. **New ids** — default slice inserted with the worktree's `id` field set.

Q4 short-circuit: when loaded id-set equals current id-set, returns after two HashSet builds (O(n)) without touching any slice.

## WorktreesLoaded Integration

The existing `Action::WorktreesLoaded(...)` arm gains:

```rust
let loaded_for_merge: Vec<crate::domain::worktree::Worktree> =
    state.worktree_browser.worktrees.clone();
crate::app::state::merge_slices(state, &loaded_for_merge);
```

Clone rationale: borrow checker requires owned data when passing `&mut state` and `&[Worktree]` simultaneously; `n` is small (~5-50 worktrees) so the clone cost is negligible. Future optimization: change `merge_slices` signature to take `&[WorktreeId]` to avoid Worktree clone.

## Inline Tests Added (5 new tests)

| Test | Location | Description |
|------|----------|-------------|
| `merge_inserts_default_slices_for_new_worktrees` | state.rs | Empty state + 2 loaded worktrees → 2 slices inserted |
| `merge_preserves_surviving_slice_state` | state.rs | Seeded slice with 1-item queue + same id in loaded → queue preserved |
| `merge_drops_slice_for_removed_worktree` | state.rs | Slice for "wt-gone" + new loaded set → wt-gone dropped, survivor inserted |
| `merge_short_circuits_when_loaded_set_equals_current_set` | state.rs | Seeded queue + identity loaded set → queue untouched (Q4 verified) |
| `worktrees_loaded_populates_slice_map` | dispatch_tests.rs | WorktreesLoaded with 2 worktrees → state.worktrees has both keys |

## Test Count Delta

| Stage | lib tests | integration | total |
|-------|-----------|-------------|-------|
| Before Plan 14-03 (post-14-02) | 85 | 3 | 88 |
| After Plan 14-03 | 90 | 3 | 93 |
| Delta | +5 | 0 | +5 |

The plan projected total >= 90. Actual: 90 lib tests — target met.

## Arch-Lint Status

`make arch-lint` — all 20 guards G-01..G-20 pass.

G-04 specifically verified: no `tokio::spawn` or other async spawn primitives introduced in `update.rs`. G-05 verified: no HTTP client or process-launch imports in `src/app/`. Both remain unchanged.

## Old Fields Status

`pub command_runner: CommandRunnerState` is **still present** in AppState. Plan 14-09 is responsible for removing it (D-15). Both old and new state structures coexist after this plan, which satisfies the must_have truth: "Old CommandRunnerState fields stay alive — both old and new state coexist."

## Deviations from Plan

None — plan executed exactly as written. The Worktree struct lacked `Default` (as anticipated by the plan's note), so explicit field construction was used in test helpers, which matches the dispatch_tests.rs convention already established in the file's `seed_one_worktree` function.

## Known Stubs

None — `merge_slices` is a complete, production-ready implementation. The `worktrees` HashMap is wired and populated on every `WorktreesLoaded` event. No placeholder data flows to any UI rendering path.

## Threat Flags

None — this plan only adds pure state mutation helpers and wires them into the existing TEA reducer path. No new network endpoints, no auth paths, no file access patterns.

T-14-08 (DoS via 60s refresh dropping running tasks): **MITIGATED** by the retain-survivors logic + Q4 short-circuit. 4 inline tests verify the merge contract. P-6 pitfall from RESEARCH is closed.

T-14-09 (JoinHandle abort() panicking): **ACCEPTED** — `JoinHandle::abort()` does not panic per tokio docs; `take()` consumes the record so any future panic still drops the Box.

T-14-10 (lost output on slice drop): **ACCEPTED** — dropped worktree's UI panel is also gone; no consumer for the lost output.

## Self-Check: PASSED

- FOUND: `pub worktrees:` in src/app/state.rs
- FOUND: `pub fn task_for_worktree` in src/app/state.rs
- FOUND: `pub fn merge_slices` in src/app/state.rs
- FOUND: `pub command_runner: CommandRunnerState` in src/app/state.rs (NOT deleted)
- FOUND: `crate::app::state::merge_slices` in src/app/update.rs
- FOUND commit 3c0beec (test: RED for Task 1)
- FOUND commit 1707eaa (feat: GREEN for Task 1)
- FOUND commit 545a3bf (test: RED for Task 2)
- FOUND commit cca7824 (feat: GREEN for Task 2)
- 4/4 merge_slices inline tests pass: `cargo test --lib state::merge_slices_tests`
- 1/1 WorktreesLoaded integration test passes: `worktrees_loaded_populates_slice_map`
- Total: 90 lib + 2 metro + 1 process_group_kill = 93 tests
- `make arch-lint` PASS (all 20 guards green)
- `cargo clippy --all-targets -- -D warnings` PASS (0 warnings/errors)
- `cargo test --workspace` PASS (93 total)
