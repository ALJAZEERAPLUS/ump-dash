---
phase: 14-per-worktree-task-system-foundation
verified: 2026-04-28T07:30:00Z
status: human_needed
score: 3/4 must-haves verified
overrides_applied: 0
human_verification:
  - test: "Run `cargo run`, open two worktrees, trigger `Y` (yarn install) on one while triggering a test command on another. Confirm both worktree rows show independent output in the output panel and that one worktree's output does not appear in the other's panel."
    expected: "Each worktree displays its own command output. Switching the UI selection to the other worktree shows that worktree's output. No cross-contamination between worktree output buffers."
    why_human: "SC#1 requires actual concurrent subprocess execution + interactive TUI observation. The data model and routing are verified in code; VALIDATION.md explicitly designates this as manual-only because it needs real subprocesses + interactive TUI."
---

# Phase 14: Per-Worktree Task System Foundation — Verification Report

**Phase Goal:** Replace the three global task fields (`running_command`, `command_task`, `command_queue`) in `AppState` with a per-worktree task map, add `TaskId` and `TaskRecord` domain types, update `Action` routing so output lines and exit events carry `WorktreeId`/`TaskId`, and enable parallel command execution across worktrees.
**Verified:** 2026-04-28T07:30:00Z
**Status:** human_needed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Dispatching `yarn install` in worktree A while worktree B is running a test causes both to execute concurrently — each worktree's output appears in its own output panel with no cross-contamination | ? HUMAN NEEDED | Data model, routing, and dispatch all verified in code. `yarn_install_on_a_while_jest_on_b_both_have_tasks` unit test passes. Actual concurrent TUI observation requires live run per VALIDATION.md §Manual-Only Verifications. |
| 2 | The metro single-instance invariant is preserved — starting metro in any worktree while metro is already running goes through the existing conflict flow unchanged | ✓ VERIFIED | `cargo test --test metro_single_instance` passes (2 tests: stop+restart, double-spawn conflict). `metro_start_on_a_while_metro_running_on_b_keeps_single_instance` dispatch test passes. `state.metro` stays at AppState root (D-13). G-17/G-16 guards still green. |
| 3 | `CommandOutputLine` and `CommandExited` actions carry `WorktreeId`/`TaskId` and are routed to the correct worktree's output buffer regardless of which worktree is currently selected in the UI | ✓ VERIFIED | `Action::CommandOutputLine { task_id, line }` and `Action::CommandExited { task_id, status }` confirmed in `src/domain/action.rs:51-61`. Routing by `task_id` in `update.rs:566-578` via `values_mut().find(|s| s.task.as_ref().map(|t| t.id) == Some(task_id))`. `command_output_line_routes_to_correct_slice_regardless_of_active_worktree` test passes. `late_command_output_line_for_cancelled_task_is_silently_dropped` test passes. |
| 4 | A running task's identity `(CommandKind, WorktreeId)` is accessible to UI, cancellation, and collision logic via `task_for_worktree(state, id)` | ✓ VERIFIED | `pub fn task_for_worktree<'a>(state: &'a AppState, id: &WorktreeId) -> Option<&'a TaskRecord>` exists in `src/app/state.rs:290-294`. Used in `src/ui/panels.rs:210` for output panel title. `TaskRecord.spec: CommandSpec` provides CommandKind via variant discriminant (D-05). Cancel path uses `slice.task.take().handle.abort()` in `update.rs:680-681`. |

**Score:** 3/4 truths automated-verified; 1 human-needed

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `src/domain/worktree_slice.rs` | WorktreeSlice struct with 6 D-01 fields | ✓ VERIFIED | 6 fields present: `id`, `task`, `queue`, `output`, `output_scroll`, `post_drain`. `#[derive(Debug, Default)]`. 3 inline tests pass. |
| `src/domain/task.rs` | TaskId, TaskRecord, ExitStatus, AtomicU64 counter | ✓ VERIFIED | All 4 types present. `TaskId(u64)` with `next()` and `next_for_test()`. `TaskRecord` has `id`, `spec`, `started_at`, `handle`. `ExitStatus` has 4 variants. 3 inline tests pass. |
| `src/domain/ports/task_handle.rs` | `trait TaskHandle: Send + Sync + Debug` with `fn abort(&self)` | ✓ VERIFIED | Trait present, 9th port in `ports/mod.rs`. No tokio types in domain. |
| `src/domain/ports/mod.rs` | 9-port index including `pub mod task_handle` | ✓ VERIFIED | 9 entries confirmed. |
| `src/infra/task_handle.rs` | `TokioTaskHandle` + `From<std::process::ExitStatus> for ExitStatus` | ✓ VERIFIED | Struct, trait impl, From impl all present. 3 inline tests pass. |
| `src/app/state.rs` | `pub worktrees: HashMap<WorktreeId, WorktreeSlice>` at AppState root + `task_for_worktree` + `merge_slices` | ✓ VERIFIED | Field at root (not in sub-struct). Both helpers present. 4 merge_slices inline tests pass. |
| `src/app/update.rs` | Slice-local routing, drain, dispatch; `Effect::SpawnTask` emission from `dispatch_command` | ✓ VERIFIED | `dispatch_command` emits `Effect::SpawnTask`. `CommandOutputLine` routes by task_id. `CommandExited` does slice-local drain. `slice.queue.pop_front`, `slice.post_drain.take()`, `record.handle.abort()` all present. Zero `state.command_runner` references. |
| `src/app/effect.rs` | `Effect::SpawnTask { task_id, worktree_id, spec, cwd, branch }` variant; `SpawnCommand` deleted | ✓ VERIFIED | `SpawnTask` variant present. `SpawnCommand` has 0 matches in `src/`. `spawn_task_variant_constructs_and_matches` test passes. |
| `src/app/effect_runner.rs` | `SpawnTask` arm + `task_handle_tx` field on `EffectRunner` | ✓ VERIFIED | `pub task_handle_tx` field, `Effect::SpawnTask` arm, `Instant::now()` at runner, `task_handle_tx.send` all present. `SpawnCommand` arm deleted. |
| `src/app/runtime.rs` | `task_handle_tx`/`task_handle_rx` channel + `slice.task = Some(record)` in both select-arm and drain-pass | ✓ VERIFIED | Channel construction, `EffectRunner::new` 4-arg call, `task_handle_rx.recv()`, `task_handle_rx.try_recv()`, `slice.task = Some(record)` all confirmed. |
| `src/app/keybindings.rs` | `command_running` predicate walks slices | ✓ VERIFIED | `state.worktrees.values().any(|s| s.task.is_some())` at line 974. Zero `command_runner` references. |
| `src/ui/panels.rs` | Output panel title uses `task_for_worktree` + slice queue length | ✓ VERIFIED | `task_for_worktree` called at line 210. `state.worktrees.get(id)` for queue count at line 212. |
| `src/app/dispatch_tests.rs` | 23 dispatch tests (including 5 new parallelism/routing/stale-drop tests) | ✓ VERIFIED | 23 dispatch tests pass. New tests: `yarn_install_on_a_while_jest_on_b_both_have_tasks`, `metro_start_on_a_while_metro_running_on_b_keeps_single_instance`, `command_output_line_routes_to_correct_slice_regardless_of_active_worktree`, `command_exited_drains_slice_local_queue_not_other`, `late_command_output_line_for_cancelled_task_is_silently_dropped`. Helpers `assert_running_in`, `slice_queue_len`, `slice_output`, `synthetic_task_record` present. |
| `Makefile` | G-21 guard active; G-20 updated to remove `CommandRunnerState` | ✓ VERIFIED | G-21 at line 119 with field-access regex pattern. G-20 no longer includes `CommandRunnerState`. `make arch-lint` passes all 21 guards (G-01..G-21). |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|-----|-----|--------|---------|
| `src/app/update.rs::dispatch_command` | `src/app/effect.rs::Effect::SpawnTask` | `return Some(Effect::SpawnTask{...})` | ✓ WIRED | Confirmed at `update.rs:75` |
| `src/domain/action.rs::CommandOutputLine` | `src/app/update.rs` routing by task_id | `values_mut().find(|s| s.task.as_ref().map(|t| t.id) == Some(task_id))` | ✓ WIRED | Confirmed at `update.rs:566-578` |
| `src/app/effect_runner.rs::SpawnTask arm` | `src/app/runtime.rs::task_handle_rx` | `task_handle_tx.send((worktree_id, record))` | ✓ WIRED | Confirmed at `effect_runner.rs:371` and `runtime.rs:102` |
| `src/app/runtime.rs::task_handle_rx.recv` | `src/app/state.rs::AppState.worktrees[id].task` | `slice.task = Some(record)` | ✓ WIRED | Confirmed at `runtime.rs:107` and `runtime.rs:129` |
| `src/app/state.rs::merge_slices` | `src/app/update.rs::WorktreesLoaded handler` | `crate::app::state::merge_slices(state, ...)` call | ✓ WIRED | Confirmed; `worktrees_loaded_populates_slice_map` test passes |
| `src/domain/worktree_slice.rs::WorktreeSlice` | `src/app/state.rs::AppState.worktrees` | `HashMap<WorktreeId, WorktreeSlice>` | ✓ WIRED | Field at AppState root, not in sub-struct (D-16) |
| `src/domain/task.rs::TaskRecord` | `src/domain/worktree_slice.rs::WorktreeSlice.task` | `task: Option<TaskRecord>` | ✓ WIRED | Confirmed in worktree_slice.rs |
| `src/infra/task_handle.rs::TokioTaskHandle` | `src/domain/ports/task_handle.rs::TaskHandle` | `impl TaskHandle for TokioTaskHandle` | ✓ WIRED | Confirmed in infra/task_handle.rs |
| `src/app/state.rs::task_for_worktree` | `src/ui/panels.rs` | `.and_then(|id| crate::app::state::task_for_worktree(state, id))` | ✓ WIRED | Confirmed at `panels.rs:210` |
| `src/app/keybindings.rs::command_running` | `src/app/state.rs::AppState.worktrees` | `state.worktrees.values().any(|s| s.task.is_some())` | ✓ WIRED | Confirmed at `keybindings.rs:974` |

---

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|--------------|--------|--------------------|--------|
| `src/app/update.rs::CommandOutputLine handler` | `slice.output` | `task_id` routes from `effect_runner`'s tokio task closure → `action_tx.send(Action::CommandOutputLine{task_id, line})` → `update()` | Yes — live subprocess stdout | ✓ FLOWING |
| `src/app/update.rs::CommandExited handler` | `slice.task.take()` | `effect_runner`'s `CommandEvent::Exited(status)` → `ExitStatus::from(status)` → `Action::CommandExited{task_id, status}` | Yes — real process exit | ✓ FLOWING |
| `src/app/runtime.rs::task_handle_rx` | `slice.task = Some(record)` | `effect_runner::SpawnTask arm` → `task_handle_tx.send((worktree_id, TaskRecord{...}))` | Yes — real `JoinHandle` wrapped in `TokioTaskHandle` | ✓ FLOWING |
| `src/ui/panels.rs::output panel title` | `active_task: Option<&TaskRecord>` | `task_for_worktree(state, id)` reads `state.worktrees.get(id).and_then(|s| s.task.as_ref())` | Yes — populated by runtime channel | ✓ FLOWING |
| `src/app/state.rs::active_output` | `&VecDeque<String>` | `state.worktrees.get(&id).map(|s| &s.output)` | Yes — filled by CommandOutputLine handler | ✓ FLOWING |

---

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| All 96 lib unit tests pass | `cargo test --lib 2>&1 \| grep "test result"` | `96 passed; 0 failed` | ✓ PASS |
| COVER-01 metro single-instance | `cargo test --test metro_single_instance` | `2 passed; 0 failed` | ✓ PASS |
| COVER-02 process group kill | `cargo test --test process_group_kill` | `1 passed; 0 failed` | ✓ PASS |
| All 23 dispatch tests pass | `cargo test --lib dispatch_tests` | `23 passed; 0 failed` | ✓ PASS |
| G-21 + 20 other guards pass | `make arch-lint` | `arch-lint: PASS` (all 21 guards) | ✓ PASS |
| clippy clean | `cargo clippy --all-targets -- -D warnings` | No errors or warnings | ✓ PASS |
| Zero banned field accesses | G-21 pattern match against `src/` | 0 hits | ✓ PASS |
| `CommandRunnerState` struct gone | `rg 'pub struct CommandRunnerState' src/` | 0 matches (only doc comment references) | ✓ PASS |
| `Effect::SpawnCommand` variant gone | `rg 'Effect::SpawnCommand' src/` | 0 matches | ✓ PASS |

---

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| TASK-01 | Plans 14-01 through 14-09 | Global `running_command`/`command_task`/`command_queue` replaced with per-worktree `HashMap<WorktreeId, WorktreeSlice>` | ✓ SATISFIED | `state.worktrees` at AppState root; `CommandRunnerState` deleted; G-21 guard active; zero field-access matches |
| TASK-02 | Plans 14-04, 14-06, 14-07, 14-08 | Parallel execution across worktrees; metro single-instance preserved | ✓ SATISFIED | Each worktree has independent `WorktreeSlice` with own task/queue; COVER-01 unchanged; `yarn_install_on_a_while_jest_on_b_both_have_tasks` test passes |
| TASK-03 | Plans 14-01, 14-03, 14-05, 14-08 | Running task identity `(CommandKind, WorktreeId)` accessible to UI/cancel/collision via `task_for_worktree` | ✓ SATISFIED | Helper exists in `state.rs:290-294`; used in `panels.rs`; cancel uses `record.handle.abort()`; routing tests pass |

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `src/domain/worktree_slice.rs` | 1 | `#![allow(dead_code)]` | ℹ️ Info | Suppress dead-code warnings during migration; acceptable for Phase 14 (fields will be used by Phase 15/16) |
| `src/domain/task.rs` | 1 | `#![allow(dead_code)]` | ℹ️ Info | Same rationale — `ExitStatus::Cancelled`/`Killed` variants unused until Phase 15 |
| `src/domain/ports/task_handle.rs` | 1 | `#![allow(dead_code)]` | ℹ️ Info | Trait used; allow is belt-and-suspenders |
| `src/app/update.rs` | 35+ | Doc comments referencing `command_queue`/`post_drain_action` | ℹ️ Info | Historical doc comments; not field accesses; G-21 regex correctly excludes these |
| `src/app/dispatch_tests.rs` | multiple | `mod command_queue { fn command_queue_push_appends_to_back }` | ℹ️ Info | Test module/function names from before Phase 14 that still test queue behavior via slice; G-21 regex correctly excludes function/module names |

No blockers or warnings found. All `#![allow(dead_code)]` usages are appropriate for the transitional domain types that will be consumed by Phases 15 and 16.

---

### Human Verification Required

#### 1. Concurrent Output Panel Display (SC#1)

**Test:** Run `cargo run` against a real multi-worktree repo. Select worktree A and trigger `Y` (yarn install). Immediately select worktree B and trigger a test command (`y` then `t`). Switch back and forth between the two worktrees using `j`/`k`.

**Expected:** Each worktree's output panel shows only that worktree's command output. Lines from worktree A's yarn install do not appear in worktree B's output panel, and vice versa. Both commands are running concurrently (both worktree rows show a running indicator).

**Why human:** Requires real subprocess execution with actual yarn and test runners. The data model (per-slice output buffers) and routing (by task_id, not active_worktree_id) are verified programmatically. The end-to-end integration requires interactive TUI observation. VALIDATION.md §Manual-Only Verifications explicitly designates this as the sole manual verification required for Phase 14.

---

### Gaps Summary

No gaps found. All automated must-haves are verified. The sole open item is SC#1's interactive TUI observation, which is explicitly classified as manual-only in VALIDATION.md and cannot be automated without real subprocess execution + TUI interaction.

The implementation is structurally complete:
- `CommandRunnerState` struct fully deleted; 4 global fields gone (G-21 enforced)
- Per-worktree `HashMap<WorktreeId, WorktreeSlice>` at AppState root
- `TaskId` + `TaskRecord` + `ExitStatus` domain types exist
- `TaskHandle` opaque port (9th) + `TokioTaskHandle` infra adapter
- `Action::CommandOutputLine { task_id, line }` and `Action::CommandExited { task_id, status }` with task_id routing
- `Effect::SpawnTask` as the sole spawn chokepoint
- `task_for_worktree(state, id)` helper used by UI, cancellation, and collision path
- `merge_slices` preserves running tasks across 60s worktree refreshes
- 21 arch-lint guards (G-01..G-21) all green
- 99 total tests passing (96 lib + 2 COVER-01 + 1 COVER-02)

---

_Verified: 2026-04-28T07:30:00Z_
_Verifier: Claude (gsd-verifier)_
