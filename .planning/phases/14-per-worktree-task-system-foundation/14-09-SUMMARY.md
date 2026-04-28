---
phase: 14
plan: 09
subsystem: app/state, app/update, app/runtime, app/keybindings, ui/panels, app/effect, app/effect_runner, Makefile
tags: [deletion, migration-complete, arch-guard, per-worktree, cleanup]
dependency_graph:
  requires: [14-01, 14-02, 14-03, 14-04, 14-05, 14-06, 14-07, 14-08]
  provides: [TASK-01, TASK-02, TASK-03]
  affects: [src/app/state.rs, src/app/update.rs, src/app/runtime.rs, src/app/keybindings.rs, src/ui/panels.rs, src/app/effect.rs, src/app/effect_runner.rs, Makefile]
tech_stack:
  added: []
  patterns: [G-21 grep guard with field-access-specific regex, per-worktree slice-only read paths]
key_files:
  created: []
  modified:
    - src/app/state.rs
    - src/app/update.rs
    - src/app/runtime.rs
    - src/app/keybindings.rs
    - src/ui/panels.rs
    - src/app/effect.rs
    - src/app/effect_runner.rs
    - src/app/dispatch_tests.rs
    - Makefile
decisions:
  - "G-21 pattern uses field-access regex (`.field_name`) not bare string match to avoid false-positives on test module/function names and comment references"
  - "D-15 realized: CommandRunnerState deleted entirely; all 5 fields migrated to WorktreeSlice"
  - "D-23 step 7+8 realized: delete-and-guard land in same plan per CONTEXT specifics line 248-249"
metrics:
  duration: "~45 minutes"
  completed: "2026-04-28T06:15:03Z"
  tasks_completed: 7
  files_modified: 9
---

# Phase 14 Plan 09: Atomic Delete-and-Guard Summary

**One-liner:** Delete `CommandRunnerState` struct + `command_runner` field; flip all 8 call sites to per-worktree slice; add G-21 guard banning re-introduction of the 4 deleted field names.

## What Was Done

This final plan of Phase 14 executes the atomic delete-and-guard — D-23 steps 7+8. Per CONTEXT.md specifics line 248-249, G-21 MUST land in the same plan as the deletion so no partial regression can ship.

### Tasks Executed

| Task | Files | Commit | Change |
|------|-------|--------|--------|
| 1+2 | state.rs, update.rs | 132eb6a | Delete CommandRunnerState struct + field; flip active_output/active_output_scroll helpers; remove all ~55 transitional double-writes from update.rs |
| 3 | runtime.rs | 3a7adcc | Flip 60s refresh gate to walk slices; flip shutdown loop to abort all slice tasks |
| 4 | keybindings.rs | ee06224 | Flip command_running predicate to walk slices |
| 5 | panels.rs | 88620aa | Flip output panel title to use task_for_worktree + slice queue length |
| 6 | effect.rs, effect_runner.rs, dispatch_tests.rs | 8992b8a | Delete Effect::SpawnCommand variant + runner arm; remove transitional test lines |
| 7 | Makefile, update.rs | 1a64783 | Add G-21 guard; update G-20; fix clippy nested-if warnings |

### Files Modified (8)

| File | Change | LOC delta |
|------|--------|-----------|
| src/app/state.rs | Delete `CommandRunnerState` struct (26 LOC) + `command_runner` field; flip 2 helpers | -32 |
| src/app/update.rs | Remove ~55 transitional double-write references; fix clippy nested-if | -164 |
| src/app/runtime.rs | Flip 60s gate + shutdown loop | +5 |
| src/app/keybindings.rs | Flip command_running predicate | 0 |
| src/ui/panels.rs | Flip output panel title block | +7 |
| src/app/effect.rs | Delete SpawnCommand variant + update test | -12 |
| src/app/effect_runner.rs | Delete SpawnCommand arm | -30 |
| src/app/dispatch_tests.rs | Remove 3 transitional legacy seed lines | -4 |
| Makefile | G-20 update + G-21 addition | +12 |

**Net LOC delta: approximately -218 (net deletion).**

## G-21 Guard Details

The G-21 guard bans re-introduction of the 4 deleted field identifiers in `src/`:

```makefile
@! rg '\.(running_command|command_task|command_queue|post_drain_action)\b|pub (running_command|command_task|command_queue|post_drain_action):|^\s+(running_command|command_task|command_queue|post_drain_action):' src/ 2>/dev/null || (echo "G-21 FAIL: ..." && exit 1)
```

**Pattern explanation:**
- `\.(field_name)\b` — catches field accesses like `state.command_runner.running_command`
- `pub field_name:` — catches struct field declarations
- `^\s+field_name:` — catches indented struct field declarations

**Intentionally NOT matched (per RESEARCH §A4):**
- `command_runner` substring — the Adapters port name (`self.adapters.command_runner`) is distinct
- Comments (`// command_queue front-push`) — references in doc comments are historical, not code
- Test module names (`mod command_queue`) — naming convention, not field access
- Test function names (`fn command_queue_push_appends_to_back`) — same reason

## G-20 Update

`CommandRunnerState` removed from G-20's alternation regex. Remaining 5 sub-structs
(MetroState, WorktreeBrowserState, ModalStackState, JiraState, AppConfigState) plus
PendingFlags satisfy the `>= 4` threshold.

## Final Verification Results

```
cargo test --workspace     — 99 tests pass (96 lib + 2 metro_single_instance + 1 process_group_kill)
cargo clippy -- -D warnings — clean (0 errors)
make arch-lint              — G-01..G-21 ALL green (21 guards)
rg state.command_runner src/ — 0 hits
rg Effect::SpawnCommand src/ — 0 hits (variant declaration gone; only in comment)
rg CommandRunnerState as code — 0 hits (only in doc comments)
G-21 field-access pattern in src/ — 0 hits
```

## TASK-01, TASK-02, TASK-03 Closure

**TASK-01** (replace global running_command/command_task/command_queue):
- `rg 'running_command' src/` as field access = 0
- `state.worktrees` map exists at AppState root (D-16)
- Verified: `grep -q 'pub worktrees' src/app/state.rs` matches

**TASK-02** (parallel execution across worktrees):
- Queue per slice (`WorktreeSlice.queue`) enables independent command queuing per worktree
- 60s refresh gate checks `state.worktrees.values().any(|s| s.task.is_some())` — does not block on single worktree
- Metro stays single-instance globally (COVER-01 passes unchanged)

**TASK-03** (task identity available to UI/cancel/collision):
- `task_for_worktree(state, id)` helper exists in state.rs (D-07)
- panels.rs uses `task_for_worktree` for output panel title
- Output routing by `task_id` in `CommandOutputLine` handler (D-08)
- Verified: Plan 14-08 test `output_routes_by_task_id_not_active_worktree` passes

## D-23 Step Completion

All 23 locked decisions (D-01..D-23) are now realized in code:
- D-01..D-07: Domain types (Plans 14-01..14-03)
- D-08..D-10: Action routing + Effect taxonomy (Plans 14-04..14-06)
- D-11..D-14: Queue/drain/post_drain strategy (Plans 14-05..14-07)
- D-15: CommandRunnerState deleted entirely (THIS PLAN — Plan 14-09)
- D-16: `state.worktrees` at AppState root (Plan 14-03)
- D-17..D-18: Slice lifecycle + merge (Plans 14-03, 14-08)
- D-19..D-20: Action/Effect taxonomy (Plans 14-04..14-09)
- D-21..D-22: Test strategy (Plans 14-03, 14-08)
- D-23: Migration sequencing complete (THIS PLAN — Plan 14-09)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] dispatch_tests.rs transitional seed lines referenced deleted state**
- **Found during:** Task 6 (cargo test run)
- **Issue:** 3 lines in `command_exited_drains_slice_queue_head` test seeded `state.command_runner.*` fields for the now-deleted transitional path
- **Fix:** Removed 3 seed lines (lines 642-644); slice-side seeds on lines 634-640 provide the necessary state
- **Files modified:** src/app/dispatch_tests.rs
- **Commit:** 8992b8a

**2. [Rule 1 - Bug] Clippy nested-if warnings from new guard pattern**
- **Found during:** Task 7 (clippy run)
- **Issue:** New `if let Some(id) { if let Some(slice) { ... } }` nested pattern triggered 9 clippy `-D warnings` errors
- **Fix:** Collapsed to `if let Some(id) && let Some(slice) { ... }` using Rust's `let` chain syntax
- **Files modified:** src/app/update.rs
- **Commit:** 1a64783

**3. [Rule 2 - Guard precision] G-21 pattern needed field-access specificity**
- **Found during:** Task 7 (pre-guard validation)
- **Issue:** Plain `! rg 'command_queue'` would false-positive on `mod command_queue` (test module), `fn command_queue_push_appends_to_back` (test function), and all doc comment references
- **Fix:** Used field-access-specific regex: `\.(field)\b|pub field:|^\s+field:` — only matches actual Rust field accesses and declarations
- **Files modified:** Makefile
- **Commit:** 1a64783

## Threat Surface Scan

No new network endpoints, auth paths, file access patterns, or schema changes introduced. This plan is net-deletion only — no new surface exposed.

## Self-Check: PASS

- src/app/state.rs: FOUND ✓
- src/app/update.rs: FOUND ✓
- src/app/runtime.rs: FOUND ✓
- src/app/keybindings.rs: FOUND ✓
- src/ui/panels.rs: FOUND ✓
- src/app/effect.rs: FOUND ✓
- src/app/effect_runner.rs: FOUND ✓
- Makefile: FOUND ✓
- Commits 132eb6a, 3a7adcc, ee06224, 88620aa, 8992b8a, 1a64783: ALL FOUND in git log ✓
- G-21 guard active: `make arch-lint` passes ✓
- 99 tests pass ✓
- clippy clean ✓
