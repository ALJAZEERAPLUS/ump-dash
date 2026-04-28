---
phase: 14
plan: 05
subsystem: action-routing
tags: [action, task-id, exit-status, transitional, payload-widening]
dependency_graph:
  requires: [14-01, 14-02, 14-03, 14-04]
  provides: [widened-action-payloads, task-id-routed-output, transitional-legacy-fallback]
  affects: [src/domain/action.rs, src/app/effect_runner.rs, src/app/update.rs, src/app/dispatch_tests.rs]
tech_stack:
  added: []
  patterns: [transitional-dual-write, sentinel-taskid, task-id-routing-with-legacy-fallback]
key_files:
  created: []
  modified:
    - src/domain/action.rs
    - src/app/effect_runner.rs
    - src/app/update.rs
    - src/app/dispatch_tests.rs
decisions:
  - "Q2 lock: No Action::TaskSpawned variant — task record delivery uses dedicated task_handle_tx channel (mirrors MetroHandle pattern); Box<dyn TaskHandle> is not Clone+PartialEq"
  - "Transitional fallback in CommandOutputLine handler: routed_to_slice flag ensures legacy command_output_by_worktree write only runs when slice lookup fails; the two paths are mutually exclusive per T-14-14"
  - "TaskId(0) sentinel in tests: production TaskId::next() starts at 1; TaskId(0) never matches a real slice task, always falls through to legacy path"
metrics:
  duration: "~25 minutes"
  completed: "2026-04-28"
  tasks: 3
  files: 4
---

# Phase 14 Plan 05: Action Payload Widening (D-08, D-09) Summary

Widened three Action payload shapes across 4 files in a transitional migration that keeps all 91+ existing tests passing while adding task_id routing to command output and exit events.

## One-liner

Widened Action::CommandOutputLine to { task_id, line } and Action::CommandExited to { task_id, status: ExitStatus } per D-08/D-09, with transitional fallback routing in update.rs and TaskId(0) sentinel in dispatch tests.

## What Changed

### Task 1: src/domain/action.rs — Variant widening

`Action::CommandOutputLine(String)` widened to:
```rust
CommandOutputLine {
    task_id: crate::domain::task::TaskId,
    line: String,
}
```

`Action::CommandExited` (unit variant) widened to:
```rust
CommandExited {
    task_id: crate::domain::task::TaskId,
    status: crate::domain::task::ExitStatus,
}
```

Q2 lock: NO `Action::TaskSpawned` added. The dedicated `task_handle_tx: UnboundedSender<(WorktreeId, TaskRecord)>` channel (Plan 14-06) carries spawned records — mirrors the MetroHandle delivery pattern. `Box<dyn TaskHandle>` is not Clone+PartialEq and cannot be embedded in an Action.

Action's existing `#[derive(Debug, Clone, PartialEq)]` stays green — `TaskId` is `Copy+PartialEq` and `ExitStatus` is `Clone+PartialEq`.

### Task 2: src/app/effect_runner.rs — SpawnCommand arm rewrite

The `Effect::SpawnCommand` arm now:
- Allocates `let task_id = TaskId::next()` at the runner (transitional — until Plan 14-06's dispatch flip moves allocation to `update.rs` at the `dispatch_command` call site)
- Emits `Action::CommandOutputLine { task_id, line }` instead of `Action::CommandOutputLine(line)`
- Emits `Action::CommandExited { task_id, status: ExitStatus::from(status) }` using the `From<std::process::ExitStatus>` impl from Plan 14-02 (infra/task_handle.rs)
- Binds `CommandEvent::Exited(status)` (was `_status`) to consume the OS exit code

### Task 3: src/app/update.rs + src/app/dispatch_tests.rs

**CommandOutputLine handler rewrite (D-08 PRIMARY + TRANSITIONAL):**

```
routed_to_slice = false
for slice in state.worktrees.values_mut():
    if slice.task.id == task_id:
        push line to slice.output
        routed_to_slice = true

if !routed_to_slice && active_worktree_id exists:
    push line to legacy command_output_by_worktree (transitional)
```

The `routed_to_slice` flag ensures the two paths are mutually exclusive (T-14-14 mitigation).

**CommandExited handler change:** Pattern only — `Action::CommandExited =>` becomes `Action::CommandExited { task_id: _, status: _ } =>`. Entire drain logic preserved verbatim. Plan 14-07 owns the slice-local drain rewrite.

**dispatch_tests.rs literal constructions rewritten:** 2 literal `Action::CommandExited` constructions replaced with `Action::CommandExited { task_id: crate::domain::task::TaskId(0), status: crate::domain::task::ExitStatus::Success }`. The `TaskId(0)` sentinel never matches a real slice task (production counter starts at 1) — falls through to legacy `command_output_by_worktree` path transparently.

No `Action::CommandOutputLine` literal constructions existed in update.rs or dispatch_tests.rs.

## Q2 Lock Recorded

**Decision (Q2):** Task record delivery uses a dedicated channel (`task_handle_tx: UnboundedSender<(WorktreeId, TaskRecord)>`), NOT an `Action::TaskSpawned` variant. Rationale:
- `Box<dyn TaskHandle>` inside `TaskRecord` does not implement `Clone` or `PartialEq`
- Action derives `Clone + PartialEq` — adding TaskRecord would require removing those derives, breaking all Action comparisons in tests and the channel clone semantics
- The dedicated channel pattern is already proven by `handle_tx: UnboundedSender<Box<dyn MetroHandle>>` (Plan 13-08 / F-004)
- Plan 14-06 implements the channel + `run_spawn_task()` method

## Transitional Fallback Strategy

Old global routing (`command_output_by_worktree`) stays alive through Plan 14-09. The dual-write pattern:

| TaskId | Scenario | Route |
|--------|----------|-------|
| TaskId(1+) with matching slice | Production new path | slice.output only |
| TaskId(1+) with no matching slice | Stale task (cancelled) | silently dropped |
| TaskId(0) | Test sentinel | legacy command_output_by_worktree |

This means existing dispatch tests need zero assertion rewrites — they use the legacy path transparently via the TaskId(0) sentinel.

## Test Count Delta

| Metric | Count |
|--------|-------|
| Tests before Plan 14-05 | 91 (lib) + 2 (COVER-01) + 1 (COVER-02) |
| Tests after Plan 14-05 | 91 (lib) + 2 (COVER-01) + 1 (COVER-02) |
| New tests | 0 (mechanical rewrite plan, no new behavior) |
| Failing tests | 0 |

Dispatch tests: 18 pass (17 original + 1 from Plan 14-03 `worktrees_loaded_populates_slice_map`).

## COVER-01 + COVER-02 Confirmation

- `cargo test --test metro_single_instance`: 2/2 pass — MetroManager single-instance invariant unchanged
- `cargo test --test process_group_kill`: 1/1 pass — POSIX process-group kill unchanged

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Clippy collapsible-if in CommandOutputLine handler**
- **Found during:** Task 3 (cargo clippy --all-targets -- -D warnings)
- **Issue:** Nested `if !routed_to_slice { if let Some(id) = ... { ... } }` triggers `clippy::collapsible_if`
- **Fix:** Collapsed to `if !routed_to_slice && let Some(id) = active_worktree_id(state) { ... }`
- **Files modified:** src/app/update.rs
- **Commit:** ffaa170 (included in Task 3 commit)

## Literal Action Constructions Rewritten

Total across update.rs + dispatch_tests.rs: **2**
- `dispatch_tests.rs:530` — `Action::CommandExited` → struct variant with TaskId(0)
- `dispatch_tests.rs:551` — `Action::CommandExited` → struct variant with TaskId(0)

Note: No `Action::CommandOutputLine(...)` literal constructions existed in these files — the action is only produced by the effect runner, never constructed inline in update.rs or dispatch_tests.rs.

## Threat Surface

No new network endpoints, auth paths, file access patterns, or schema changes introduced. The widened Action variants are internal message-passing types, not trust-boundary crossings.

T-14-14 mitigated by `routed_to_slice` flag (mutually exclusive dual-write paths verified).
T-14-13 accepted: TaskId(0) is a test-only sentinel; production TaskId::next() starts at 1.

## Self-Check: PASSED

Files created/modified:
- src/domain/action.rs: FOUND
- src/app/effect_runner.rs: FOUND
- src/app/update.rs: FOUND
- src/app/dispatch_tests.rs: FOUND

Commits:
- 67150d1: feat(14-05): widen Action variants in action.rs (Task 1)
- 275925f: feat(14-05): update SpawnCommand arm in effect_runner.rs (Task 2)
- ffaa170: feat(14-05): update update.rs + dispatch_tests.rs (Task 3)
