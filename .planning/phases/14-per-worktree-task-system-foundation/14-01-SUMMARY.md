---
phase: 14
plan: 01
subsystem: domain
tags: [domain-types, task-system, port, worktree-slice]
dependency_graph:
  requires: []
  provides:
    - domain::ports::task_handle::TaskHandle
    - domain::task::TaskId
    - domain::task::TaskRecord
    - domain::task::ExitStatus
    - domain::worktree_slice::WorktreeSlice
  affects:
    - src/domain/ports/mod.rs
    - src/domain/mod.rs
    - src/domain/worktree.rs
tech_stack:
  added: []
  patterns:
    - opaque-port-trait (mirrors MetroHandle/MetroPort from Plan 13-03)
    - pure-domain-data + inline-tests (mirrors domain/refresh.rs)
    - AtomicU64 monotonic counter with test-injection hook
key_files:
  created:
    - src/domain/ports/task_handle.rs
    - src/domain/task.rs
    - src/domain/worktree_slice.rs
  modified:
    - src/domain/ports/mod.rs
    - src/domain/mod.rs
    - src/domain/worktree.rs
decisions:
  - "[D-03] TaskHandle is an opaque domain port trait with abort(&self); JoinHandle wrapper lives infra-side in Plan 14-02"
  - "[D-04] TaskId(u64) uses AtomicU64 counter starting at 1; zero is reserved as no-task sentinel"
  - "[D-06] TaskRecord carries id, spec, started_at, handle — no worktree_id backref per D-07"
  - "[D-09] ExitStatus is a domain enum (not std::process::ExitStatus) so Phase 15 can emit Cancelled cleanly"
  - "[D-01/D-02] WorktreeSlice has all 6 fields from the CONTEXT.md D-01 sketch, derives Default"
  - "[Rule 2 deviation] Added Default derive to WorktreeId to enable WorktreeSlice::default()"
metrics:
  duration: "6 minutes"
  completed: "2026-04-28"
  tasks: 3
  files: 6
---

# Phase 14 Plan 01: Domain Type Foundation Summary

**One-liner:** Pure-domain WorktreeSlice + TaskId/TaskRecord/ExitStatus types + opaque TaskHandle port (9th port) with 6 inline tests; zero infra imports.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Create TaskHandle port (9th port) | 85b4274 | src/domain/ports/task_handle.rs, src/domain/ports/mod.rs |
| 2 | Create task.rs (TaskId, TaskRecord, ExitStatus) | d08392c | src/domain/task.rs, src/domain/mod.rs |
| 3 | Create worktree_slice.rs (6-field slice + Default) | a9b1ba8 | src/domain/worktree_slice.rs, src/domain/mod.rs, src/domain/worktree.rs |

## Artifacts Created

### `src/domain/ports/task_handle.rs` (9th port)

Opaque `trait TaskHandle: Send + Sync + std::fmt::Debug` with single `fn abort(&self)` method. Mirrors `MetroHandle` pattern from Plan 13-03/F-004. The `abort` method takes `&self` (not `self: Box<Self>`) because `tokio::task::JoinHandle::abort` is a shared-reference method.

### `src/domain/task.rs`

- `TaskId(pub u64)`: `Copy + Clone + Debug + PartialEq + Eq + Hash` newtype with static `AtomicU64` counter starting at 1 (zero reserved as sentinel)
- `TaskId::next()`: production allocator
- `TaskId::next_for_test(counter: &AtomicU64)`: test-injection hook for counter isolation
- `TaskRecord { id, spec, started_at, handle }`: live task bag; no `worktree_id` backref (D-07)
- `ExitStatus { Success, Failure { code: Option<i32> }, Cancelled, Killed }`: domain enum, not `std::process::ExitStatus`

### `src/domain/worktree_slice.rs`

`WorktreeSlice` with all 6 D-01 fields: `id: WorktreeId`, `task: Option<TaskRecord>`, `queue: VecDeque<CommandSpec>`, `output: VecDeque<String>`, `output_scroll: usize`, `post_drain: Option<Box<Action>>`. Derives `Debug + Default`.

## Inline Tests Added (6 new tests)

| Test | Module |
|------|--------|
| `next_for_test_is_monotonic` | `domain::task::tests` |
| `task_id_zero_unused_by_default_counter` | `domain::task::tests` |
| `exit_status_variants_are_constructible` | `domain::task::tests` |
| `default_slice_has_no_task_and_empty_queue` | `domain::worktree_slice::tests` |
| `default_slice_has_empty_output_and_zero_scroll` | `domain::worktree_slice::tests` |
| `slice_with_explicit_id_preserves_id` | `domain::worktree_slice::tests` |

## Test Count Delta

- Before: 79 total workspace tests (76 lib + 2 metro_single_instance + 1 process_group_kill)
- After: 85 total workspace tests (82 lib + 2 metro_single_instance + 1 process_group_kill)
- Delta: +6 lib tests

## App Files Touched

None — all changes are in `src/domain/`. No `app/`, `infra/`, or `ui/` files modified.

## Arch-Lint Status

`make arch-lint` — all 20 guards G-01..G-20 pass. G-10 (domain::ports module index) specifically verified: `grep -c '^pub mod' src/domain/ports/mod.rs` outputs `9`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical Functionality] Added Default derive to WorktreeId**

- **Found during:** Task 3
- **Issue:** `WorktreeSlice` needs `#[derive(Default)]` which requires all fields to implement `Default`. `WorktreeId(pub String)` did not derive `Default`, making `WorktreeSlice::default()` impossible to compile.
- **Fix:** Added `Default` to `WorktreeId`'s derive list in `src/domain/worktree.rs:8`. `WorktreeId::default()` = `WorktreeId("")`, consistent with `String::default()`.
- **Files modified:** `src/domain/worktree.rs` (line 8: add `Default` to derive)
- **Commit:** a9b1ba8
- **PATTERNS.md note:** The note at line 65 said "verify before relying on `WorktreeSlice::default()`" — this confirms the issue was anticipated but not pre-resolved in the plan.

## Known Stubs

None — all 6 fields are properly typed and connected to real domain types. No placeholder values.

## Threat Flags

None — these are pure in-process domain data types with no I/O surface, no network endpoints, no auth paths. T-14-01 through T-14-04 from the plan's threat model are accepted (no mitigations required for this plan).

## Self-Check

Checking files exist and commits are present.
