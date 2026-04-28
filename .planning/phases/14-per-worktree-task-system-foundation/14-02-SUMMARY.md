---
phase: 14
plan: 02
subsystem: infra
tags: [infra-adapter, task-handle, exit-status, tokio, port-impl]
dependency_graph:
  requires:
    - domain::ports::task_handle::TaskHandle  # Plan 14-01
    - domain::task::ExitStatus                 # Plan 14-01
  provides:
    - infra::task_handle::TokioTaskHandle
    - infra::task_handle::From<std::process::ExitStatus> for ExitStatus
  affects:
    - src/infra/mod.rs
tech_stack:
  added: []
  patterns:
    - opaque-infra-adapter (mirrors TokioMetroHandle in metro.rs / Plan 13-07)
    - From-impl-in-infra (D-09: ExitStatus From conversion lives infra-side)
key_files:
  created:
    - src/infra/task_handle.rs
  modified:
    - src/infra/mod.rs
    - src/domain/worktree_slice.rs
decisions:
  - "[D-03] TokioTaskHandle wraps tokio::task::JoinHandle<()> and impls TaskHandle; JoinHandle stays private to infra module"
  - "[D-09] From<std::process::ExitStatus> for ExitStatus lives in infra/task_handle.rs, adjacent to the adapter; maps success -> Success, failure -> Failure { code }"
  - "[Rule 3 deviation] Fixed pre-existing doc list continuation lint in worktree_slice.rs to unblock cargo clippy -D warnings acceptance criteria"
metrics:
  duration: "4 minutes"
  completed: "2026-04-28"
  tasks: 1
  files: 3
---

# Phase 14 Plan 02: Infra Adapter (TokioTaskHandle) Summary

**One-liner:** TokioTaskHandle infra adapter wrapping JoinHandle<()> behind the TaskHandle port + From<std::process::ExitStatus> conversion; 3 inline tests; G-05 stays green.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Create src/infra/task_handle.rs (TokioTaskHandle + ExitStatus From impl) | 50afd4a | src/infra/task_handle.rs, src/infra/mod.rs, src/domain/worktree_slice.rs |

## Artifacts Created

### `src/infra/task_handle.rs` (~100 LOC including tests)

`TokioTaskHandle(pub tokio::task::JoinHandle<()>)` — public newtype struct, derives `Debug`.

`impl TaskHandle for TokioTaskHandle`: `abort(&self)` delegates to `self.0.abort()`. Method signature matches `JoinHandle::abort` which is `&self`, synchronous, non-async.

`impl From<std::process::ExitStatus> for ExitStatus`: maps `status.success()` -> `ExitStatus::Success`, otherwise `ExitStatus::Failure { code: status.code() }`. Signal-killed processes (code=None) are classified as `Failure { code: None }` per Phase 14 contract; Phase 15 will widen via `ExitStatusExt::signal()`.

## Inline Tests Added (3 new tests)

| Test | Description |
|------|-------------|
| `infra::task_handle::tests::from_success_status_maps_to_success` | Spawns `true`, verifies `-> ExitStatus::Success` |
| `infra::task_handle::tests::from_failure_status_with_code_maps_to_failure_code_some` | Spawns `sh -c "exit 7"`, verifies `-> ExitStatus::Failure { code: Some(7) }` |
| `infra::task_handle::tests::boxed_task_handle_dispatches_through_trait_object` | Boxes `TokioTaskHandle` as `Box<dyn TaskHandle>`, calls `abort()` — smoke tests trait dispatch |

## Test Count Delta

- Before: 85 lib tests (82 from Plan 14-01 + 6 from Plan 14-01's domain tests, total with integration = 85+3)
- After: 88 lib tests (85 lib + 2 metro_single_instance + 1 process_group_kill = 88 total)
- Delta: +3 lib tests (the 3 inline tests above)

## Arch-Lint Status

`make arch-lint` — all 20 guards G-01..G-20 pass. G-05 specifically verified: tokio::process and JoinHandle types are confined to `src/infra/task_handle.rs`; `src/domain/` has no new tokio imports introduced by this plan.

## Module Registration

`pub mod task_handle;` added to `src/infra/mod.rs` in alphabetical position between `sim_history` and `android_prefs`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking Issue] Fixed pre-existing doc list continuation lint in worktree_slice.rs**

- **Found during:** Task 1 (running `cargo clippy --all-targets -- -D warnings` per acceptance criteria)
- **Issue:** Two `error: doc list item without indentation` errors in `src/domain/worktree_slice.rs:12-13` (introduced by Plan 14-01). The `//! Plus...` line immediately followed a doc list item without a blank line separator, violating clippy's `doc_lazy_continuation` lint. This was a pre-existing issue from Plan 14-01 that was not caught there.
- **Fix:** Added a blank `//!` line between the list items and the continuation sentence at line 12.
- **Files modified:** `src/domain/worktree_slice.rs` (+1 blank doc line)
- **Commit:** 50afd4a (included in the main task commit)

## Known Stubs

None — `TokioTaskHandle` is a complete, production-ready implementation. The `From` impl covers all Phase 14 cases (success + failure with code). Signal-kill mapping is intentionally deferred to Phase 15 as documented in the code comment.

## Threat Flags

None — this plan only adds infra-side code behind a domain trait. No new network endpoints, no auth paths, no file access patterns, no schema changes. T-14-05 through T-14-07 from the plan's threat model are all accepted per the STRIDE register.

## Self-Check: PASSED

- FOUND: src/infra/task_handle.rs
- FOUND: `pub mod task_handle` in src/infra/mod.rs
- FOUND commit 50afd4a (feat: TokioTaskHandle adapter)
- `rg -q 'pub struct TokioTaskHandle' src/infra/task_handle.rs` matches
- `rg -q 'impl TaskHandle for TokioTaskHandle' src/infra/task_handle.rs` matches
- `rg -q 'impl From<std::process::ExitStatus> for ExitStatus' src/infra/task_handle.rs` matches
- `rg -q 'pub mod task_handle' src/infra/mod.rs` matches
- 3/3 inline tests pass: `cargo test --lib infra::task_handle::tests`
- `make arch-lint` PASS (all 20 guards green)
- `cargo clippy --all-targets -- -D warnings` PASS (0 warnings/errors)
- `cargo test --workspace` PASS (88 total: 85 lib + 2 metro + 1 process_group_kill)
