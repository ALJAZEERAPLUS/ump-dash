---
phase: 14
plan: "07"
subsystem: app/update
tags: [per-worktree, slice-queue, dispatch, drain, D-11, D-12, D-13, D-14, D-03]
dependency_graph:
  requires: ["14-01", "14-02", "14-03", "14-04", "14-05", "14-06"]
  provides: ["slice-local-queue-dispatch", "slice-local-drain", "slice-local-cancel", "metro-ready-slice-walk"]
  affects: ["src/app/update.rs"]
tech_stack:
  added: []
  patterns:
    - "SliceDrainResult enum for borrow-split in CommandExited handler"
    - "Transitional double-write: slice-primary + legacy-global preserved"
    - "D-12 all Recipe::expand push sites write to both slice.queue and command_runner.command_queue"
key_files:
  created: []
  modified:
    - src/app/update.rs
decisions:
  - "Used SliceDrainResult enum to resolve borrow-split issue in CommandExited; avoids unsafe indexing"
  - "MetroActivityUpdate transitional fallback checks candidate_id first; only falls through to legacy if no slice candidate"
  - "CommandQueuePush action also gets slice-local double-write for symmetry with Recipe sites"
  - "WorktreeSwitchToSelected auto-sync + SyncBeforeMetroAccept also set slice.post_drain in addition to legacy post_drain_action"
metrics:
  duration_seconds: 361
  completed_date: "2026-04-28"
  tasks_completed: 3
  files_modified: 1
---

# Phase 14 Plan 07: Slice-Local Queue Dispatch + Drain Summary

**One-liner:** Migrated all 11 Recipe::expand push sites + CommandExited drain + CommandCancel + MetroActivityUpdate Ready to slice-local (per-worktree) queues with transitional legacy double-write.

## What Was Done

### Task 1 — D-12: Recipe::expand push sites migrated (commit aa782b5)

All Recipe::expand push_back sites now write to BOTH `slice.queue` (primary) and `state.command_runner.command_queue` (transitional). Both push_front sites (metro prereq deferral) are similarly double-written.

**Inventory of 11 push sites migrated:**

| Line (approx) | Site | Type | Action |
|---|---|---|---|
| ~430 | F-204 site 1: Recipe::SyncThenRun auto-sync in CommandRun | push_back | Sequence remainder to slice.queue |
| ~455 | F-204 site 2: metro prereq in CommandRun | push_front | Spec head to slice.queue.push_front |
| ~503 | F-204 site 3: Recipe::ReleaseBuildAndInstall in CommandRun | push_back | Sequence remainder to slice.queue |
| ~522 | F-204 site 4: Recipe::GitFetchThenReset in CommandRun | push_back | Sequence remainder to slice.queue |
| ~643 | CommandQueuePush action | push_back | Single spec to slice.queue |
| ~1078 | F-204 site 10a: Recipe::SyncThenStartMetro in WorktreeSwitchToSelected | push_back + post_drain | Sequence + MetroStart post_drain |
| ~1248 | F-204 site 6: Recipe::Clean in CleanConfirm | push_back | Sequence remainder to slice.queue |
| ~1302 | F-204 site 7: Recipe::SyncThenRun in SyncBeforeRunAccept | push_back | Sequence remainder to slice.queue |
| ~1318 | F-204 site 8: SyncBeforeRunDecline metro deferral | push_front | Spec head to slice.queue.push_front |
| ~1356 | F-204 site 9: Recipe::SyncThenStartMetro in SyncBeforeMetroAccept | push_back + post_drain | Sequence + MetroStart post_drain |

All push sites are guarded by `if let Some(ref wt_id) = active_worktree_id(state)` — sites where no worktree is active gracefully skip the slice push (legacy global push still happens for back-compat).

### Task 2 — D-11 + D-14: CommandExited handler rewrite (commit c5644ef)

**New handler shape:**
1. Find originating slice: `state.worktrees.iter().find(|(_, s)| s.task.as_ref().map(|t| t.id) == Some(task_id))`
2. Take slice.task (clearing it), extract spec for refresh classification
3. Legacy global state cleanup (`running_command.take()`, `command_task = None`) preserved transitionally
4. Refresh classification logic preserved verbatim
5. **Slice-local drain** via `SliceDrainResult` enum (eliminates borrow conflicts):
   - `Dispatch(spec)` — dispatch via `dispatch_command(state, spec)`
   - `NeedsMetro(spec)` — push_front back to slice + `Action::MetroStart`
   - `PostDrain(action)` — D-14: `slice.post_drain.take()` consumed
   - `Empty` — no-op
6. **Transitional legacy drain** — pops `command_queue.pop_front()` for bookkeeping but does NOT re-dispatch when slice already dispatched

### Task 3 — D-03 + D-13: CommandCancel + MetroActivityUpdate Ready slice-walk (commit 240cda1)

**CommandCancel:**
- Slice-local primary: `slice.task.take()` → `record.handle.abort()` (D-03 opaque `TaskHandle` trait)
- Clears `slice.queue`, sets `slice.post_drain = None`, pushes `"[cancelled]"` to `slice.output`
- Transitional: legacy global clear preserved (`command_task.take().abort()`, `running_command = None`, `command_queue.clear()`, `post_drain_action = None`, legacy output push)

**MetroActivityUpdate(Ready) drain (D-13):**
- PRIMARY: `state.worktrees.iter().find(|(_, s)| s.task.is_none() && s.queue.front().map(|c| c.needs_metro()).unwrap_or(false))`
- Winner: pop slice.queue.front, re-enter via `update(state, Action::CommandRun(spec))`
- TRANSITIONAL fallback: if no slice candidate and `running_command.is_none()`, drain legacy `command_queue` head (preserves pre-14 behavior for global-queue tests)
- Single-instance metro: at most one slice wins per Ready event

## Test Count

All 91 inline tests + 2 metro_single_instance tests + 1 process_group_kill test = **94 tests pass** (unchanged from pre-plan count).

## Coverage Gate Confirmation

- COVER-01 (`tests/metro_single_instance.rs`): PASS — metro single-instance invariant unchanged
- COVER-02 (`tests/process_group_kill.rs`): PASS — process-group kill behavior unchanged
- 17 dispatch tests in `src/app/dispatch_tests.rs`: PASS (Plan 14-08 will rewrite assertions)

## Key Verification Results

```
rg -c 'slice\.queue\.push' src/app/update.rs       → 11
rg -c 'state\.command_runner\.command_queue\.push' src/app/update.rs → 11
rg -q 'slice\.queue\.pop_front'                    → FOUND
rg -q 'slice\.post_drain\.take'                    → FOUND
rg -q 'task\.as_ref()\.map\(|t| t\.id\) == Some\(task_id\)' → FOUND
rg -q 'slice\.task\.take\(\)'                      → FOUND
rg -q 'record\.handle\.abort\(\)'                  → FOUND
rg -q 's\.queue\.front\(\)\.map\(\|c\| c\.needs_metro\(\)\)' → FOUND
cargo clippy --all-targets -- -D warnings           → PASS (0 warnings)
make arch-lint                                      → arch-lint: PASS (all 20 G-XX guards green)
```

## Deviations from Plan

### Minor: SliceDrainResult enum instead of nested if-let

**Found during:** Task 2 implementation

**Issue:** The plan's PATTERNS.md sketch used nested `if let ... && let Some(slice) = ...` chains that would have created borrow conflicts when calling `dispatch_command(state, spec)` after popping from the slice.

**Fix:** Introduced a local `SliceDrainResult` enum inside the `CommandExited` arm. This extracts the drain decision in one borrow block, drops the borrow, then acts on the result in a fresh borrow. No behavior change vs. the plan's description.

**Files modified:** `src/app/update.rs` only

### Minor: CommandQueuePush included in D-12 migration

**Found during:** Task 1 review

**Issue:** The plan's 10-site inventory focuses on Recipe::expand sites but `Action::CommandQueuePush` (line 626) is also a direct spec push to `command_queue`. For symmetry and correctness, it should also double-write.

**Fix:** Added slice-local double-write to `CommandQueuePush` handler. Legacy single-write preserved.

### Minor: post_drain set at WorktreeSwitchToSelected and SyncBeforeMetroAccept

**Found during:** Task 1 (D-14 partial implementation)

**Observation:** These two sites not only push to the queue but also set `command_runner.post_drain_action = Some(Box::new(Action::MetroStart))`. Per D-14, the per-slice equivalent should be set at the same call sites.

**Fix:** Both sites now also set `slice.post_drain = Some(Box::new(Action::MetroStart))` alongside the legacy global write. This is additive and safe.

## Known Stubs

None — all slice writes are real (not hardcoded empty values). The legacy global fields remain alive as designed (Plan 14-09 deletes them).

## Threat Flags

None — no new network endpoints, auth paths, file access patterns, or schema changes introduced. The `slice.queue.push_front` usage in the metro-prereq path follows the existing `command_queue.push_front` semantics unchanged.

## Self-Check: PASSED

| Check | Result |
|-------|--------|
| src/app/update.rs exists | FOUND |
| 14-07-SUMMARY.md exists | FOUND |
| commit aa782b5 exists | FOUND |
| commit c5644ef exists | FOUND |
| commit 240cda1 exists | FOUND |
| cargo test --workspace (94 tests) | PASS |
| cargo clippy --all-targets | PASS (0 warnings) |
| make arch-lint | PASS (all 20 G-XX guards green) |
