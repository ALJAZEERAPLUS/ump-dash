# Phase 14: Per-Worktree Task System Foundation — Research

**Researched:** 2026-04-28
**Domain:** Rust + tokio TEA reducer; per-worktree task identity; hexagonal port introduction
**Confidence:** HIGH (CONTEXT.md locked 23 decisions; this research consolidates external verification + code-site reconnaissance only)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

The 23 locked decisions D-01..D-23 are reproduced verbatim below — the planner must respect every one. Quoted from `14-CONTEXT.md` `<decisions>` block.

**F-500 Scoping — WorktreeSlice shape**
- **D-01:** Per-worktree state lands as a **full `WorktreeSlice`** from day one (not a narrow `HashMap<WorktreeId, TaskRecord>`). The slice carries every per-worktree concern that exists today plus the Phase-15/16 anticipated fields. Sketch:
  ```rust
  pub struct WorktreeSlice {
      pub id: WorktreeId,
      pub task: Option<TaskRecord>,
      pub queue: VecDeque<CommandSpec>,
      pub output: VecDeque<String>,
      pub output_scroll: usize,
      pub post_drain: Option<Box<Action>>,
  }
  ```
- **D-02:** Slice lives at **`src/domain/worktree_slice.rs`** as a pure data type (no infra refs, no tokio types). Inline `#[cfg(test)] mod tests`.
- **D-03:** Cancellation handle is an **opaque trait object via a domain port**. Add `trait TaskHandle: Send + Sync + std::fmt::Debug { fn abort(&self); }` to `src/domain/ports/task_handle.rs` (joins the existing 8-port inventory as the 9th port). The tokio `JoinHandle` wrapper impls it in `src/infra/task_handle.rs` (or inside `infra/command_runner.rs`). Slice holds `Box<dyn TaskHandle>`.

**TaskId + TaskRecord**
- **D-04:** **`TaskId(u64)`** generated from a process-wide `AtomicU64` counter living in `src/domain/task.rs`. Counter starts at 1 (so `0` can be used as "no task" sentinel in tests). Test injection via `TaskId::next_for_test()` helper that takes an `&AtomicU64` argument; production calls `TaskId::next()` against a static `AtomicU64`.
- **D-05:** **CommandKind = `CommandSpec` discriminant** — no parallel `enum CommandKind`. Phase 15 collision identity will be `(std::mem::discriminant(&spec), worktree_id)`.
- **D-06:** **`TaskRecord { id: TaskId, spec: CommandSpec, started_at: Instant, handle: Box<dyn TaskHandle> }`**. `started_at` captured in `EffectRunner::run_spawn_task` at spawn moment (NOT in `update()` — pure reducer has no `Instant::now()`).
- **D-07:** No `worktree_id` backref inside `TaskRecord` — slice key already provides it. Helper `task_for_worktree(state, id) -> Option<&TaskRecord>` is `state.worktrees.get(&id).and_then(|s| s.task.as_ref())`.

**Action routing payload**
- **D-08:** `Action::CommandOutputLine` becomes **`CommandOutputLine { task_id: TaskId, line: String }`**. Routing: lookup the slice whose `task.as_ref().map(|t| t.id) == Some(task_id)`; if found, push to `slice.output`. Lines for tasks that no longer exist (cancelled, slice dropped) are silently dropped. Protects against fast cancel+respawn race where late stdout from dead task would contaminate new task's output.
- **D-09:** `Action::CommandExited` becomes **`CommandExited { task_id: TaskId, status: ExitStatus }`**. `ExitStatus` is a domain enum (`enum ExitStatus { Success, Failure { code: Option<i32> }, Cancelled, Killed }`) defined in `src/domain/task.rs` — NOT `std::process::ExitStatus`.
- **D-10:** `Effect::SpawnTask { task_id, worktree_id, spec }` is the **single chokepoint** for spawning. Replaces today's ad-hoc spawn paths in `effect_runner`. Runner constructs the tokio task with `task_id` + `worktree_id` captured by-move into the per-task closure; all `Action::CommandOutputLine` / `Action::CommandExited` sends from inside that closure carry the captured `task_id`.

**Queue strategy**
- **D-11:** Queue is **per-worktree FIFO inside the slice** (`WorktreeSlice.queue: VecDeque<CommandSpec>`). Global `CommandRunnerState.command_queue` deleted. Drain logic moves from "global CommandExited handler" to "slice-local: when `slice.task` clears, pop_front from `slice.queue`".
- **D-12:** **Recipe expansion targets the originating worktree's slice queue.** No cross-worktree recipe targeting.
- **D-13:** **Metro special-case stays metro-special.** `state.metro` (the `MetroManager` at AppState root) remains single-instance globally. The metro-needs-prereq drain step moves into the slice-local drain handler with same semantics — push back to head of `slice.queue`, dispatch `Action::MetroStart`, wait for `MetroActivityUpdate(Ready)` to drain head.
- **D-14:** **`post_drain` is per-slice** (`WorktreeSlice.post_drain: Option<Box<Action>>`). Sync-then-metro-on-A fires when `slice_A.queue` empties — never observes worktree B. `CommandRunnerState.post_drain_action` deleted.

**CommandRunnerState fate**
- **D-15:** **Default expectation: delete `CommandRunnerState` entirely** — its purpose is supplanted by per-slice fields, the JoinHandle map belongs in effect_runner, the TaskId counter is a static. (Alternative (b): repurpose for AtomicU64 + JoinHandle map. Planner picks.)

**Worktree slice lifecycle**
- **D-16:** Slice map is **`HashMap<WorktreeId, WorktreeSlice>` on AppState root** — NOT inside any sub-struct.
- **D-17:** **Merge strategy on `WorktreesLoaded`:** existing slices for surviving `WorktreeId`s are **kept**; slices for removed `WorktreeId`s are dropped (running task's `handle.abort()` is called as part of slice removal); slices for new `WorktreeId`s are inserted with `Default::default()`. Implemented as `merge_slices(state, loaded_worktrees)` helper called from `WorktreesLoaded` handler.
- **D-18:** **Worktree removal mid-task** (`pending_worktree_removal` flow): the `merge_slices` helper handles task abort on slice drop — no separate cancel pass needed.

**Action taxonomy**
- **D-19:** Three Actions get a payload change:
  - `CommandOutputLine(String)` → `CommandOutputLine { task_id: TaskId, line: String }`
  - `CommandExited` (no payload) → `CommandExited { task_id: TaskId, status: ExitStatus }`
  - `CommandRun(CommandSpec)` stays the same — but `update()` MUST resolve target `WorktreeId` from `active_worktree_id(state)` at the dispatch site.
  Other Actions touched only structurally: `CommandQueued`, `CommandCancel`, `CommandOutputClear`.

**Effect taxonomy**
- **D-20:** One new Effect: `Effect::SpawnTask { task_id: TaskId, worktree_id: WorktreeId, spec: CommandSpec }`. Phase 14: +1 variant, 0 deletions (existing variants stay until effect_runner is fully ported).

**Test strategy**
- **D-21:** Existing 17 dispatch tests in `src/app/dispatch_tests.rs` must pass after migration. Rewrite assertions from `state.command_runner.running_command.is_some()` to `state.worktrees.get(&id).and_then(|s| s.task.as_ref()).is_some()`. Add new per-worktree parallelism tests:
  - "yarn install on A while jest on B → both slices have `task.is_some()` simultaneously"
  - "MetroStart on A while metro running on B → existing conflict path triggers; `state.metro` retains single registration"
  - "CommandOutputLine routes to correct slice regardless of `active_worktree_id`"
  - "CommandExited drains slice-local queue, not the other slice's queue"
- **D-22:** Existing 2 characterization tests (`tests/metro_single_instance.rs`, `tests/process_group_kill.rs`) MUST pass unchanged.

**Migration sequencing**
- **D-23:** Plans land in this order:
  1. Domain types: `WorktreeSlice`, `TaskId`, `TaskRecord`, `ExitStatus`, `TaskHandle` port, `task_for_worktree` helper. No app/ changes.
  2. AppState shape: add `worktrees` root field; `WorktreesLoaded` merge logic. Old fields stay alive in parallel.
  3. Action payload widening: `CommandOutputLine { task_id, line }`, `CommandExited { task_id, status }` — update every match site.
  4. `Effect::SpawnTask` + effect_runner port: new spawn path uses per-slice routing; old path stays alive for unmigrated call sites.
  5. Dispatch migration: every `dispatch_command` / Recipe-expansion site flips to slice queue.
  6. Drain migration: `CommandExited` handler reads slice-local queue; `post_drain` per-slice.
  7. Delete the 4 global fields + `CommandRunnerState` (or reduce); flip `active_output` / `active_output_scroll` helpers to read from the slice.
  8. New shape guard added to `make arch-lint`: G-21 — "no `running_command` / `command_task` / `command_queue` field references anywhere in `src/`".

### Claude's Discretion

Reproduced verbatim from CONTEXT.md `<decisions>` block:

- Exact `tests/` vs inline split for new parallelism tests — planner picks based on whether they need real subprocesses (Phase 12/D-07 rule).
- Whether `EffectRunner` holds the JoinHandle map directly or via a small `TaskHandleRegistry` helper struct — planner picks based on whether the Phase 15 cancellation hook reads cleaner one way or the other.
- Whether the `merge_slices` helper lives in `domain/worktree_slice.rs` (pure data merge) or in `app/state.rs` (uses Default for new slices, knows about app concerns) — planner picks based on whether the merge needs anything app-layer.

### Deferred Ideas (OUT OF SCOPE)

Reproduced verbatim from CONTEXT.md `<deferred>` block:

- **Cancellation wiring** (`CancellationToken`, SIGTERM/SIGKILL escalation, `kill_on_drop`) — TASK-04, Phase 15. The `TaskHandle::abort()` in Phase 14 is just `JoinHandle::abort()` (cooperative tokio cancel); Phase 15 widens the trait to OS-level kill via the existing process-group path.
- **Collision policy** — TASK-05, Phase 15. Phase 14 lays the identity foundation (`(discriminant, WorktreeId)`); Phase 15 decides per-category block-vs-cancel rules.
- **Per-repo-root `tokio::sync::Semaphore(1)` for yarn install serialization** — TASK-06, Phase 15.
- **Live UI indicators** — UI-01..03, Phase 16. Reads `slice.task.as_ref().map(|t| t.started_at.elapsed())` directly in render path. Phase 14 sets up the data; Phase 16 reads it.
- **F-501 `Command` category split** — DEFERRED to backlog.
- **`WorktreeSlice.pending: PendingFlags` field** — not added; Plan 13-09 already absorbed prereq flags into Recipe + post_drain.
- **Cross-worktree Recipe targeting** — D-12 explicitly says no.
- **`task_history: Vec<TaskRecord>` per slice** — explicitly deferred.
- **Current task name displayed inline** — explicitly deferred.
- **CI integration of `make arch-lint`** — post-milestone.
- **`cargo-deny` / `cargo-modules` / `cargo-depgraph`** — post-milestone.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| TASK-01 | Replace global `running_command` / `command_task` / `command_queue` in `AppState` with per-worktree task state keyed by `WorktreeId`. | Section "Architectural Responsibility Map" maps fields → tier (domain `WorktreeSlice` + app `state.worktrees` root). Section "Standard Stack" specifies `HashMap<WorktreeId, WorktreeSlice>`. Section "Existing Field Inventory" enumerates 6 file × 72 touch sites the migration must rewrite. |
| TASK-02 | Commands execute in parallel across different worktrees; metro stays single-instance. | "Architecture Patterns / Parallel-spawn-without-shared-mutex" describes the pattern. "Common Pitfalls / P-3 Late stdout from cancelled task" + "P-5 Metro registration race" cover the failure modes. |
| TASK-03 | A running task's identity is `(CommandKind, WorktreeId)`; available via `task_for_worktree(state, id)`. | Section "Standard Stack / Identity types" specifies `TaskId(u64)` from `AtomicU64`; "Code Examples / TaskRecord lookup helper" shows the function shape. CommandKind = `std::mem::discriminant(&CommandSpec)` per D-05. |
</phase_requirements>

## Summary

Phase 14 is a **structural migration** inside an already-decided design space. The user has locked 23 implementation decisions; this research's job is NOT to re-litigate alternatives but to surface the technical investigation needed to PLAN their implementation correctly:

1. **Verify the tokio `JoinHandle::abort()` semantics** that D-03 + D-17 (slice drop = abort) rely on — confirmed: `abort()` is synchronous, non-async, returns immediately; the task itself completes asynchronously with a `JoinError`. `kill_on_drop(true)` on `tokio::process::Command` triggers when the `Child` is dropped — and the `Child` is dropped when the task body returns (or unwinds from abort). So `JoinHandle::abort()` on a task that owns a `Child` configured with `kill_on_drop(true)` triggers the kill signal as a side effect of the task tearing down. This validates D-17's slice-drop-aborts-task design.

2. **Verify atomic counter best practices** for `TaskId` (D-04) — confirmed: `AtomicU64::fetch_add(1, Ordering::Relaxed)` is the canonical pattern for a process-wide monotonic ID counter. Wraparound is well-defined (two's complement); for our use case (one task per few-hundred-ms human action) wraparound is multi-decade unreachable.

3. **Map every existing call site** the planner needs to touch — 6 files, 72 grep hits across `running_command` / `command_task` / `command_queue` / `post_drain_action`, with concrete file:line ranges + Recipe::expand site catalogue (10 sites in update.rs).

4. **Surface the 13-PATTERNS.md sub-struct regroup pattern** (D-16 mirrors it for placement of `worktrees` HashMap at AppState root).

**Primary recommendation:** Follow D-23's 8-step migration sequencing literally. Step 8 (G-21 + delete) MUST be the last plan in the phase — partial G-21 land creates a window where a deleted-but-still-referenced field could ship.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| `WorktreeSlice` data type | `src/domain/worktree_slice.rs` | — | Pure data — no I/O, no tokio. D-02 places it in domain. |
| `TaskId(u64)` + `TaskRecord` + `ExitStatus` | `src/domain/task.rs` | — | Pure data + a `static AtomicU64`. No tokio types in the public surface. D-04 + D-06 + D-09. |
| `TaskHandle` port (trait) | `src/domain/ports/task_handle.rs` | — | 9th port. Domain owns the trait; infra owns the impl. D-03 mirrors `MetroPort` / `MetroHandle` (Phase 13/F-004). |
| `impl TaskHandle for tokio::task::JoinHandle<()>` (or wrapper newtype) | `src/infra/task_handle.rs` (or `infra/command_runner.rs`) | — | Tokio types stay infra-side. G-05 (no `tokio::process` in `src/app/`) + G-04 (no `tokio::spawn` in `update.rs`) stay green. |
| `state.worktrees: HashMap<WorktreeId, WorktreeSlice>` (the per-worktree map) | `src/app/state.rs` (AppState root) | — | App layer — owned by AppState. D-16 places at root, NOT inside a sub-struct. |
| `merge_slices(state, loaded_worktrees)` helper | Planner's discretion: `domain/worktree_slice.rs` (pure) OR `app/state.rs` (uses Default + app concerns) | — | If merge needs only `WorktreeId` + slice fields, it's pure-domain. If it needs `Default::default()` from app types, it lives app-side. |
| `task_for_worktree(state, id)` helper | `src/app/state.rs` | — | Used by app-tier code (`update.rs`, `effect_runner.rs`) and UI (`panels.rs`). One-liner: `state.worktrees.get(&id).and_then(|s| s.task.as_ref())`. |
| `Effect::SpawnTask { task_id, worktree_id, spec }` | `src/app/effect.rs` | — | App-tier effect grammar — pure data. D-20. |
| `EffectRunner::run_spawn_task` | `src/app/effect_runner.rs` | `src/infra/command_runner.rs` (existing adapter) | Effect runner does the `tokio::spawn`; existing `TokioCommandRunner` (already configured with `kill_on_drop(true)`) keeps emitting `CommandEvent`s. The runner translates `CommandEvent` → `Action::CommandOutputLine{ task_id, ... }` / `Action::CommandExited{ task_id, status }` with `task_id` captured by-move. |
| `WorktreesLoaded` slice merge integration | `src/app/update.rs` | — | The existing `WorktreesLoaded` handler at `update.rs:286-338` calls the `merge_slices` helper. |
| Action payload widening (`CommandOutputLine`, `CommandExited`) | `src/domain/action.rs` | All consumers in `src/app/` | Action enum is pure-domain since Plan 13-01 (G-15 active). |
| `G-21` shape guard | `Makefile` arch-lint target | — | Local-only grep guard — no CI integration this phase. |

## Standard Stack

### Core (in-tree types this phase introduces or modifies)

| Type | Location | Purpose | Why Standard |
|------|----------|---------|--------------|
| `WorktreeSlice` | `src/domain/worktree_slice.rs` (NEW) | Per-worktree task + queue + output bag | D-01: full slice from day one (vs. narrow `HashMap<WorktreeId, TaskRecord>`); avoids re-migration in Phase 15/16 |
| `TaskId(u64)` | `src/domain/task.rs` (NEW) | Process-wide monotonic task identity | D-04: cheapest possible identity for log correlation |
| `TaskRecord` | `src/domain/task.rs` (NEW) | `{ id, spec, started_at, handle: Box<dyn TaskHandle> }` | D-06: minimum fields for SC#3, SC#4, and Phase-16 elapsed render |
| `ExitStatus` (domain enum) | `src/domain/task.rs` (NEW) | `Success` / `Failure { code }` / `Cancelled` / `Killed` | D-09: domain-pure; Phase 15 emits `Cancelled` cleanly without infra type |
| `TaskHandle` trait | `src/domain/ports/task_handle.rs` (NEW, 9th port) | `trait TaskHandle: Send + Sync + Debug { fn abort(&self); }` | D-03: opaque cancellation handle; mirrors `MetroPort` / `MetroHandle` (F-004) |
| `Effect::SpawnTask` | `src/app/effect.rs` (variant added to existing enum) | Single spawn chokepoint | D-10 + D-20: replaces ad-hoc spawn paths in effect_runner |
| `state.worktrees: HashMap<WorktreeId, WorktreeSlice>` | `src/app/state.rs` (root field, NEW) | The per-worktree task map | D-16: at AppState root, not inside a sub-struct |

### Supporting (existing primitives consumed unchanged)

| Type | Location | Used For |
|------|----------|----------|
| `WorktreeId(pub String)` | `src/domain/worktree.rs:9` | HashMap key for `state.worktrees` (`#[derive(Eq, Hash)]` already in place) [VERIFIED: read] |
| `CommandSpec` | `src/domain/command.rs` | Slice queue element type; `std::mem::discriminant(&spec)` is the kind for D-05 |
| `Recipe::expand(&DependencyState) -> Vec<CommandSpec>` | `src/domain/pipeline.rs` | Unchanged — D-12 just re-targets the destination from global queue to slice queue |
| `tokio::task::JoinHandle<()>` | `tokio` (existing dep) | Wrapped in `Box<dyn TaskHandle>` — `JoinHandle::abort()` is synchronous [CITED: docs.rs/tokio/1.49.0/tokio/task/struct.JoinHandle.html] |
| `std::sync::atomic::AtomicU64` | `std` | Process-wide TaskId counter — `Relaxed` ordering is canonical [CITED: doc.rust-lang.org/std/sync/atomic/struct.AtomicU64.html] |
| `std::time::Instant` | `std` | `started_at` field captured in `EffectRunner::run_spawn_task` (NOT in update — pure reducer has no `Instant::now()`) |
| `std::mem::discriminant<T>(&T) -> Discriminant<T>` | `std` | Phase 15 collision identity `(discriminant, WorktreeId)`. `Discriminant<T>: PartialEq + Eq + Hash` ([CITED: doc.rust-lang.org/std/mem/fn.discriminant.html] — `Hash` impl available since 1.21.0). Phase 14 only needs to ensure CommandSpec stays a flat enum (it does). |

### Alternatives Considered

| Instead of | Could Use | Tradeoff (and why CONTEXT.md picked otherwise) |
|------------|-----------|------------------------------------------------|
| `HashMap<WorktreeId, WorktreeSlice>` (D-01) | `HashMap<WorktreeId, TaskRecord>` | Narrow alternative re-migrates the same data twice (Phase 15 adds `cancel_token`, Phase 16 reads `started_at`, both go on the slice). Addendum F-500 explicitly recommends slice. |
| `TaskId(u64)` from `AtomicU64` (D-04) | UUID v4 | UUID overkill: log correlation needs uniqueness within a process lifetime, not globally. Atomic counter is one machine word + one atomic op per spawn. |
| Domain enum `ExitStatus` (D-09) | `std::process::ExitStatus` | std type can't represent `Cancelled` or `Killed` cleanly; would force an infra type into Action's payload (G-15 violation — Action lives in domain). |
| Trait object `Box<dyn TaskHandle>` (D-03) | Concrete `JoinHandle<()>` field | Concrete tokio type in domain breaks G-05 (`src/app/` may not import tokio::process / spawn primitives) and G-16 pattern (MetroHandle stays opaque trait). |
| Global FIFO queue (status quo) | Per-slice FIFO (D-11) | Global queue is the entire reason TASK-02 fails today: a yarn install on A blocks a jest run on B even though they share no resource. |
| `running_command: Option<CommandSpec>` global (status quo) | `slice.task: Option<TaskRecord>` (D-01) | Identity is `(CommandKind, WorktreeId)` per TASK-03; without WorktreeId in the key the running-status lookup is ambiguous when two worktrees both have a yarn install. |

**Installation:** No new dev/runtime dependencies. All required types are already in `std` and `tokio = "1.49"` (existing) [VERIFIED: `Cargo.toml:28,61`]. Phase 12/D-13 hold: no new dev-deps unless absolutely necessary.

**Version verification:**
- `tokio = "1.49"` confirmed in `Cargo.toml` [VERIFIED: read].
- `cargo 1.94.1` / `rustc 1.94.1` confirmed [VERIFIED: `cargo --version` + `rustc --version`].

## Architecture Patterns

### System Architecture Diagram

```
                                     ┌──────────────────────────────────────┐
                                     │                AppState              │
   user keypress                     │  (single source of truth)            │
   ──────────►  handle_key  ─────►   │                                      │
                                     │  worktrees: HashMap<                 │
                                     │    WorktreeId,                       │
                                     │    WorktreeSlice {                   │
                                     │      task: Option<TaskRecord>,       │
                                     │      queue: VecDeque<CommandSpec>,   │
                                     │      output: VecDeque<String>,       │
                                     │      output_scroll: usize,           │
                                     │      post_drain: Option<Box<Action>>,│
                                     │    }                                 │
                                     │  >                                   │
                                     │                                      │
                                     │  metro: MetroManager  ─── single     │
                                     └──────────────────────────────────────┘
                                              │
   Action ──►  update(state, action) ─────────┴────► Vec<Effect>
                       │
                       ├── CommandRun(spec)                     ╮
                       │     resolves WorktreeId from           │
                       │     active_worktree_id(state)          │  Effect::SpawnTask
                       │     pushes new TaskRecord into         │   { task_id,
                       │     slice.task                         │     worktree_id,
                       │     emits Effect::SpawnTask            │     spec }
                       │                                        │
                       ├── CommandOutputLine{ task_id, line }   │
                       │     looks up slice via                 │
                       │     find_slice_with_task(state,        │
                       │       task_id) ─► slice.output         │
                       │                                        │
                       ├── CommandExited{ task_id, status }     │
                       │     clears slice.task,                 │
                       │     drains slice.queue front,          │
                       │     consumes slice.post_drain          │
                       │                                        │
                       └── CommandCancel                        │
                             slice.task.take()                  │
                             handle.abort()                     │
                                                                ▼
                                                  ┌──────────────────────────┐
                                                  │      EffectRunner        │
                                                  │  (single tokio boundary) │
                                                  │                          │
   ←─── Action::CommandOutputLine{ task_id, … } ──┤  run_spawn_task:         │
        Action::CommandExited{ task_id, status } ─┤    captures task_id +    │
                                                  │    worktree_id by-move,  │
                                                  │    JoinHandle returned   │
                                                  │    by tokio::spawn       │
                                                  │    wrapped in            │
                                                  │    Box<dyn TaskHandle>   │
                                                  └──────────────────────────┘
                                                              │
                                                              ▼
                                                  ┌──────────────────────────┐
                                                  │  TokioCommandRunner      │
                                                  │  (infra adapter, existing│
                                                  │   from Plan 13-05)       │
                                                  │  - spawn child           │
                                                  │  - kill_on_drop(true)    │
                                                  │  - emits CommandEvent    │
                                                  └──────────────────────────┘
```

**Data flow** (yarn install on worktree A):
1. User presses `y`, `i` → `handle_key` returns `Action::CommandRun(CommandSpec::YarnInstall)`.
2. `update()` resolves WorktreeId-A from `active_worktree_id(state)`, looks up `slice_A`, sets `slice_A.task = Some(TaskRecord { id: TaskId(N), spec, started_at, handle })`, returns `Effect::SpawnTask { task_id: TaskId(N), worktree_id: WorktreeId-A, spec }`.
   - Wrinkle: `started_at` and `handle` cannot be set from `update()` (no `Instant::now()`, no `tokio::spawn`). Two valid shapes:
     - (a) `update()` constructs a partial `TaskRecord` with placeholder values and the runner backfills later via a second Action (`TaskSpawned { task_id, started_at, handle }`).
     - (b) `update()` does NOT populate `slice.task` — the runner does, via `Action::TaskSpawned { task_id, worktree_id, spec, started_at, handle }`. This keeps `update()` purer at the cost of an extra Action.
   - **Planner must pick** between (a) and (b). Planner-phase recommendation: option (b) is closer to the existing pattern (handle delivery already uses a dedicated channel for `Box<dyn MetroHandle>` — see `runtime.rs:38-40` and `effect_runner.rs:99-138`). Treat as planner discretion.
3. Concurrently, user presses `y`, `j` on worktree B → `Action::CommandRun(CommandSpec::YarnJest { … })` → same flow, lands in `slice_B.task`. **No global mutex contended.**
4. yarn-install stdout streams in: `Action::CommandOutputLine { task_id: TaskId(N), line }` → routes to `slice_A.output` regardless of which worktree the UI is currently showing.
5. yarn-install exits: `Action::CommandExited { task_id: TaskId(N), status: ExitStatus::Success }` → clears `slice_A.task`, pops `slice_A.queue.front()` if non-empty, never touches `slice_B`.

### Recommended Project Structure (post-Phase-14 deltas only)

```
src/
├── domain/
│   ├── worktree_slice.rs    # NEW (D-02) — pure data + inline tests
│   ├── task.rs              # NEW (D-04, D-06, D-09) — TaskId, TaskRecord, ExitStatus, AtomicU64 counter
│   ├── ports/
│   │   ├── mod.rs           # +pub mod task_handle;  (9th port — keeps G-10 satisfied)
│   │   └── task_handle.rs   # NEW (D-03) — trait TaskHandle
│   └── ... (existing)
├── infra/
│   ├── task_handle.rs       # NEW (or extend command_runner.rs) — impl TaskHandle for tokio::task::JoinHandle<()>
│   └── ... (existing)
├── app/
│   ├── state.rs             # +pub worktrees: HashMap<WorktreeId, WorktreeSlice>; -pub command_runner (or hollow)
│   ├── update.rs            # all 41 touch sites of running_command/command_task/command_queue → slice access
│   ├── effect.rs            # +Effect::SpawnTask { task_id, worktree_id, spec }
│   ├── effect_runner.rs     # +run_spawn_task; gains JoinHandle map (or TaskHandleRegistry helper)
│   ├── adapters.rs          # may grow to 8 fields if planner picks port-injected TaskHandle factory (Claude's Discretion)
│   ├── runtime.rs           # 2 touch sites (runtime.rs:65, runtime.rs:129) → slice access
│   ├── keybindings.rs       # 1 touch site (line 974: "is any task running?") → walk slices
│   └── dispatch_tests.rs    # 21 assertion sites rewritten per D-21
└── ui/
    └── panels.rs            # 3 touch sites (panels.rs:207, 209, 217) → slice access via active_worktree_id

tests/
├── metro_single_instance.rs # MUST pass unchanged (D-22)
└── process_group_kill.rs    # MUST pass unchanged (D-22)
```

### Pattern 1: Sub-Struct Regroup (mirrored from Plan 13-10 / F-209)

**What:** Wrap cohesive cross-cutting fields in a struct under `AppState`. Plan 13-10 took ~30 flat AppState fields and grouped them into 6 sub-structs (`MetroState`, `WorktreeBrowserState`, `CommandRunnerState`, `ModalStackState`, `JiraState`, `AppConfigState`).

**When to use:** When fields share a single domain concern AND the access path doesn't compound. **D-16 deliberately does NOT mirror this for `worktrees`** — instead the slice map sits at AppState root because it IS the per-worktree replacement for what used to be cross-cutting global fields. Wrapping it would re-introduce the access-path noise Plan 13-10 was trying to avoid.

**Example (post-Phase-14 AppState root):**
```rust
// Source: src/app/state.rs:243-262 [VERIFIED: read] — annotated with Phase 14 deltas
pub struct AppState {
    // Cross-cutting roots (unchanged)
    pub focused_panel: FocusedPanel,
    pub show_help: bool,
    pub error_state: Option<ErrorState>,
    pub should_quit: bool,
    pub metro: crate::domain::metro::MetroManager,    // single-instance, D-13 unchanged

    // Sub-structs (Plan 13-10)
    pub metro_state: MetroState,
    pub worktree_browser: WorktreeBrowserState,
    // pub command_runner: CommandRunnerState,         // DELETED in Phase 14 step 7 (D-15 default expectation)
    pub modal_stack: ModalStackState,
    pub jira: JiraState,
    pub app_config: AppConfigState,

    // NEW in Phase 14 (D-16: at root, not inside any sub-struct)
    pub worktrees: HashMap<WorktreeId, WorktreeSlice>,
}
```

### Pattern 2: Opaque Domain Port + Infra Adapter (mirrored from Plan 13-03 / F-004)

**What:** Domain defines a trait; infra defines a concrete struct that impls the trait; app holds `Box<dyn Trait>` and never sees the concrete type.

**When to use:** When tokio types (or any infra concern) would otherwise leak into domain. The MetroHandle pattern is the literal blueprint.

**Example (mirroring `MetroHandle` for `TaskHandle`):**
```rust
// Source: src/domain/ports/metro_port.rs:34-57 [VERIFIED: read] — adapted for D-03
// In src/domain/ports/task_handle.rs (NEW):
pub trait TaskHandle: Send + Sync + std::fmt::Debug {
    /// Cooperative cancel. Phase 14: just `JoinHandle::abort()` —
    /// the task body unwinds, the inner Child's `kill_on_drop(true)`
    /// handles process termination as a side effect.
    /// Phase 15 will widen this trait to add SIGTERM/SIGKILL escalation.
    fn abort(&self);
}

// In src/infra/task_handle.rs (NEW):
#[derive(Debug)]
pub struct TokioTaskHandle(pub tokio::task::JoinHandle<()>);

impl TaskHandle for TokioTaskHandle {
    fn abort(&self) {
        self.0.abort();
    }
}
```

### Pattern 3: Effect-as-Data (Plan 13-07 / F-201)

**What:** `update()` returns `Vec<Effect>`; the EffectRunner does the spawn. **Update() is pure — no `tokio::spawn`, no `Instant::now()`.** Plan 13-07 enforces this with G-04 (`! rg 'tokio::spawn|spawn_blocking' src/app/update.rs`) [VERIFIED: `Makefile:51`].

**When to use:** Always for new dispatch paths. Phase 14's `Effect::SpawnTask` follows this pattern exactly.

**Implication for D-06 (`started_at: Instant`):** `Instant::now()` is not allowed in `update()`. The runner must capture `started_at` either at spawn time (option (b) above, the recommended path) or via a separate "task spawned" callback Action. The slice's `task` field can either:
- Be populated synchronously in `update()` with a placeholder `started_at` (e.g., a sentinel) and overwritten by the runner — **NOT recommended**, leaks "needs a real Instant" obligation to a non-obvious site.
- Stay `None` until the runner sends `Action::TaskSpawned { task_id, worktree_id, spec, started_at, handle: Box<dyn TaskHandle> }`, and `update()` populates `slice.task` then. **Recommended path** — mirrors how `Box<dyn MetroHandle>` is delivered today via `handle_tx` channel and `runtime.rs:88-92` calls `state.metro.register(handle)`.

### Pattern 4: Per-Task Closure Capture (D-10)

**What:** When `EffectRunner::run_spawn_task` calls `tokio::spawn`, the closure captures `task_id` and `worktree_id` by-move. Every `Action::CommandOutputLine` and `Action::CommandExited` sent from inside that closure carries the captured `task_id`. No closure-based ports, no per-task sender wrapper.

**Example (Phase 14 sketch):**
```rust
// Source: src/app/effect_runner.rs:160-178 (existing SpawnCommand) [VERIFIED: read]
//         — adapted per D-10
Effect::SpawnTask { task_id, worktree_id: _, spec } => {
    use crate::domain::ports::command_runner_port::CommandEvent;
    let runner = self.adapters.command_runner.clone();
    // cwd + branch resolved by update() and embedded in the Effect — NOT shown
    // in D-20's payload sketch but they will be needed; planner may either
    // add cwd/branch to SpawnTask or look them up from worktree_id at the runner site.
    let mut rx = runner.spawn(spec.clone(), cwd, branch);
    let tx = self.action_tx.clone();
    tokio::spawn(async move {
        while let Some(ev) = rx.recv().await {
            let action = match ev {
                CommandEvent::OutputLine(line) => Action::CommandOutputLine { task_id, line },
                CommandEvent::Exited(status) => Action::CommandExited {
                    task_id,
                    status: ExitStatus::from(status),  // From<std::process::ExitStatus> impl in infra
                },
            };
            if tx.send(action).is_err() { break; }
        }
    });
    // The JoinHandle from the spawn must be wrapped in Box<dyn TaskHandle>
    // and delivered to update() via either an Action carrying handle
    // (Pattern 3 option (b)) or a dedicated handle channel (mirroring metro).
}
```

**Open detail (planner decides):** D-20's `Effect::SpawnTask { task_id, worktree_id, spec }` does NOT mention `cwd` and `branch`. The existing `Effect::SpawnCommand` carries them (`effect.rs:31-35`) [VERIFIED: read]. Planner must either:
1. Widen `SpawnTask` payload to `{ task_id, worktree_id, spec, cwd, branch }`, OR
2. Have `EffectRunner::run_spawn_task` look up cwd/branch from `worktree_id` against an injected snapshot.

Option 1 is closer to the Plan-13-08 convention (effects carry their context — `state.repo_root` is embedded in `Effect::ListWorktrees { repo_root }` per `effect.rs:38-39`).

### Anti-Patterns to Avoid

- **`tokio::spawn` inside `update()`:** Hard-blocked by G-04 grep guard. New code must return `Effect::SpawnTask` instead.
- **`Instant::now()` inside `update()`:** Not yet guarded by a grep, but breaks `update()` purity (test determinism). The runner captures `started_at`.
- **`std::process::ExitStatus` in `Action`:** Action lives in `src/domain/action.rs` (G-15); pulling in `std::process` couples domain to OS-process semantics. Use the domain `ExitStatus` enum (D-09) — infra has the `From<std::process::ExitStatus>` impl.
- **`worktree_id` backref inside `TaskRecord`:** D-07 forbids it. The slice key is the source of truth; storing it in the value invites desync. The `task_for_worktree(state, id)` helper is one line.
- **Routing `CommandOutputLine` by `active_worktree_id` instead of `task_id`:** Today's update.rs:493-501 [VERIFIED: read] does exactly this. After D-08 the lookup is by `task_id` so late stdout from a cancelled task is silently dropped (the right behavior — see Pitfall P-3).
- **Wrapping `worktrees` HashMap inside a sub-struct:** D-16 forbids it. The slice IS the per-worktree replacement; wrapping re-introduces the access-path noise Plan 13-10 avoided.
- **Cross-worktree Recipe targeting:** D-12 forbids `Recipe::TargetWorktree(id) | Recipe::TargetCurrent`. Every existing Recipe variant is single-worktree by construction. Recipe expansion implicitly targets the originating worktree's slice queue.
- **Per-task `mpsc::Sender` cloning:** D-10 says "no per-task sender wrapper." The single `action_tx` from `runtime.rs:38` is shared across every spawned task; closures clone it once.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Process-wide monotonic ID counter | Mutex<u64> + manual lock dance | `static COUNTER: AtomicU64 = AtomicU64::new(1); COUNTER.fetch_add(1, Relaxed)` | Lockless, wraparound-safe, ~1 ns per call. [CITED: doc.rust-lang.org/std/sync/atomic/struct.AtomicU64.html] |
| Process group kill on cancellation | Bespoke libc::kill + zombie reaping | `tokio::process::Command` with `kill_on_drop(true)` (already configured in `infra/command_runner.rs:76`) | Already in place since Phase 13/REFACTOR-02 + characterized in COVER-02. Phase 14 abort path inherits this — `JoinHandle::abort()` drops the task body, which drops the Child, which kills the process group. [VERIFIED: read `infra/command_runner.rs:71-87`] |
| Variant-discriminant equality + hashing | Manual `match (a, b) { (CommandSpec::YarnInstall, CommandSpec::YarnInstall) => true, ... }` for every collision check | `std::mem::discriminant(&spec_a) == std::mem::discriminant(&spec_b)`; `Discriminant<T>: Hash + Eq` since std 1.21.0 | Phase 15 collision identity is `(Discriminant<CommandSpec>, WorktreeId)`. Phase 14 only requires this stays available — no code yet. [CITED: doc.rust-lang.org/std/mem/struct.Discriminant.html] |
| JoinHandle aliasing across slice removals | `Arc<Mutex<JoinHandle<()>>>` | `Box<dyn TaskHandle>` owned by exactly one place (the slice's TaskRecord) | Single ownership matches D-17's slice-drop = abort lifecycle. `JoinHandle::abort` is `&self` (shared-reference safe), so the trait method `fn abort(&self)` works directly. [CITED: docs.rs/tokio/1.49.0/tokio/task/struct.JoinHandle.html] |
| Re-implementing the per-task action stream | New `mpsc::channel` per spawn | Reuse the single `action_tx` from `runtime.rs:38`, clone into each spawned closure | Already the established pattern (`effect_runner.rs:80,87,...`). Per-task channels would require a separate aggregator task and extra indirection. [VERIFIED: read] |
| ExitStatus → variant mapping in domain | Manual `if status.success() { ExitStatus::Success } else if status.signal() { ExitStatus::Killed } else { ExitStatus::Failure { ... } }` everywhere | Single `From<std::process::ExitStatus> for crate::domain::task::ExitStatus` impl in infra | One translation site → one test. Per `effect_runner.rs:170-172` the existing translation already lives in the runner; Phase 14 widens it. [VERIFIED: read] |

**Key insight:** This phase has **zero net-new dependencies and zero net-new infra primitives**. Every required capability already exists in std (`AtomicU64`, `mem::discriminant`, `Instant`, `HashMap`), tokio (`task::JoinHandle`, the existing `kill_on_drop(true)` config), or the codebase's existing port pattern. The work is structural rearrangement guarded by an existing test corpus.

## Runtime State Inventory

> Phase 14 is a structural refactor — not a rename. Most categories below are not applicable, but stating that explicitly per the Step 2.5 protocol.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — no databases, no persistent stores carry the names of the deleted fields. The 3 persistence sites (jira_cache, android_prefs, sim_history) write semantic data not field-name-keyed [VERIFIED: `effect_runner.rs:298-324`]. | None |
| Live service config | None — no external services (n8n, Datadog, etc.) reference these field names. | None |
| OS-registered state | None — no Task Scheduler, launchd, pm2 process names embed `running_command` / `command_task` / `command_queue`. | None |
| Secrets/env vars | None — env var names in `Cargo.toml` and the codebase don't reference these symbols. | None |
| Build artifacts | None expected — no compiled binaries are field-name-keyed. **One nuance:** if a downstream consumer pinned to the old `AppState` shape via a dev-dep that re-exports `rn_dash::AppState`, that compile would break. There are none today (no other crates depend on rn_dash) [VERIFIED: bin+lib only]. | None |

**Summary:** This is a pure source-tree refactor. No data migration, no OS state, no external service touch. The G-21 grep guard suffices to detect leftover references in `src/`.

## Existing Field Inventory (the migration's grep target)

The 4 fields (`running_command`, `command_task`, `command_queue`, `post_drain_action`) appear at **72 sites across 6 files** [VERIFIED: `grep -rn ... src/`]:

| File | Touch Count | Why It's There |
|------|-------------|----------------|
| `src/app/update.rs` | 41 | Every dispatch / drain / cancel / queue manipulation. The bulk of the migration. |
| `src/app/dispatch_tests.rs` | 21 | Test assertions on the global state shape. D-21 rewrites these. |
| `src/app/state.rs` | 4 | Field declarations on `CommandRunnerState`. Step 7 deletes the struct (or hollows it). |
| `src/ui/panels.rs` | 3 | Lines 207, 209, 217 — output panel title rendering ("running command label" + "queue count"). D-21 helper `task_for_worktree` + per-slice queue lookup. |
| `src/app/runtime.rs` | 2 | Line 65 (60s refresh gate: "skip if a task is running") + line 129 (shutdown task abort). Step 7 rewrites: line 65 → walk slices for `task.is_some()`; line 129 → walk slices, abort each. |
| `src/app/keybindings.rs` | 1 | Line 974 — single helper that returns "is anything running?". Step 7 rewrites to walk slices. |

**Recipe::expand call sites in `update.rs`** (D-12 retargets all of these from global queue to slice queue) [VERIFIED: `grep -n 'F-204 site' src/app/update.rs`]:

| Line | Site | Recipe |
|------|------|--------|
| ~382 | site 1 | `Recipe::SyncThenRun` (auto-sync fast path) |
| ~407 | site 2 | metro prereq (push_front + MetroStart) |
| ~457 | site 3 | `Recipe::ReleaseBuildAndInstall` |
| ~471 | site 4 | `Recipe::GitFetchThenReset` |
| ~527 | site 5 | needs_metro drain — push_front + MetroStart |
| ~999 | site 10a | auto-sync fast path |
| ~1027 | site 10b | active_worktree_path update |
| ~1166 | site 6 | `Recipe::Clean` |
| ~1220 | site 7 | `Recipe::SyncThenRun` |
| ~1243 | site 8 | skip-sync metro deferral |
| ~1269 | site 9 | `Recipe::SyncThenStartMetro` |

10 distinct sites across `update.rs`. Each requires the same transformation: `state.command_runner.command_queue.push_back(cmd)` → `state.worktrees.get_mut(&wt_id).unwrap().queue.push_back(cmd)` (or via a helper like `slice_mut(state, wt_id)` per Claude's Discretion). The `wt_id` is already in scope at every site because update.rs already resolves it for branch/path lookup.

## Common Pitfalls

### P-1: `update()` accidentally calls `Instant::now()` for `started_at`

**What goes wrong:** A reasonable-looking implementation populates `slice.task = Some(TaskRecord { id, spec, started_at: Instant::now(), handle: ??? })` inside `update()`. Compiles. Tests pass (Instant deltas are non-deterministic but the assertion `task.is_some()` doesn't check the timestamp). Ships.

**Why it happens:** D-06 says `started_at` is captured "in `EffectRunner::run_spawn_task` at the moment the tokio task is spawned (NOT in `update()`)" — but `Instant::now()` is not yet guarded by a grep.

**How to avoid:** Either (a) add a G-22 grep guard `! rg 'Instant::now\\(\\)' src/app/update.rs`, or (b) populate `slice.task` only after the runner sends `Action::TaskSpawned { task_id, started_at, handle }` (Pattern 3 option (b)). Option (b) makes the type system enforce the discipline — `update()` literally has no `Instant` in scope.

**Warning signs:** Test snapshots that include `started_at` show non-zero variance run-to-run.

### P-2: Closure capture races on `task_id`

**What goes wrong:** The runner clones `task_id` into the closure but accidentally captures a reference to a counter that changes between spawn and the closure's first send.

**Why it happens:** Misreading "capture by-move" — `move` only matters for owned data. `TaskId(u64)` is `Copy`; closures default-copy it. The actual risk is forgetting `move` and accidentally borrowing.

**How to avoid:** `tokio::spawn(async move { … })` — the existing `effect_runner.rs:167` pattern. `TaskId` derives `Copy` for free (one u64). Let the compiler enforce.

**Warning signs:** A test where two concurrent spawns interleave their output: if `task_id` was captured by ref against a per-spawn local, both closures would see whichever value happened to land last. With `Copy + move`, impossible.

### P-3: Late stdout from a cancelled task contaminates a respawned task's output

**What goes wrong:** User starts yarn install on worktree A (TaskId=5). yarn forks node, which buffers a few KB of stdout. User hits cancel — `slice_A.task.take()` + `handle.abort()`. tokio aborts the task; the task drops its `Child`; `kill_on_drop(true)` triggers SIGKILL on the process group. **But:** between abort() and process death, the kernel may flush a final batch of buffered stdout. That flush turns into `Action::CommandOutputLine { task_id: 5, line }` after the user has already started a NEW yarn install (TaskId=6). If routing is by `worktree_id` (the pre-D-08 design), those lines appear in the NEW run's output buffer.

**Why it happens:** Routing by `worktree_id` confuses identity ("the worktree where yarn is running now") with attribution ("the task that produced this stdout line"). D-08 routes by `task_id` so stale lines route to a now-deleted slice — the lookup fails, and the line is silently dropped.

**How to avoid:** D-08 is mandatory. The routing helper:
```rust
// In a slice, find the one whose current task matches task_id.
fn slice_for_task<'a>(state: &'a mut AppState, task_id: TaskId) -> Option<&'a mut WorktreeSlice> {
    state.worktrees.values_mut().find(|s| s.task.as_ref().map(|t| t.id) == Some(task_id))
}
```
If the task was cancelled, `s.task` is None; the find returns None; the line is dropped. Correct behavior.

**Warning signs:** Add a parallelism test (D-21): spawn yarn on A, immediately cancel, immediately respawn yarn on A; assert that the second run's output buffer doesn't contain stdout lines that arrived from the first run's tail.

### P-4: `kill_on_drop(true)` semantics with `JoinHandle::abort()`

**What goes wrong:** Plan author assumes "abort the JoinHandle" means "kill the child process now." It doesn't — directly. `abort()` schedules cancellation; the task body unwinds at the next await point; on unwind, the `Child` is dropped; `kill_on_drop(true)` then sends a kill signal. There's a race window between `abort()` and the actual SIGKILL where the child is still alive.

**Why it happens:** Misreading the abort semantics. The tokio docs are explicit: `"Awaiting a cancelled task might complete as usual if the task was already completed at the time it was cancelled, but most likely it will fail with a cancelled JoinError."` [CITED: docs.rs/tokio/1.49.0/tokio/task/struct.JoinHandle.html]

Importantly: `"Although issuing a kill signal to the child process is a synchronous operation, the resulting zombie process cannot be .await'ed inside of the destructor"` and `"The tokio runtime will, on a best-effort basis, attempt to reap and clean up such processes in the background, but no additional guarantees are made with regard to how quickly or how often this procedure will take place."` [CITED: docs.rs/tokio/1.49.0/tokio/process/struct.Command.html]

**How to avoid:** For Phase 14, this is acceptable — D-22 + COVER-02 already characterize the process-group kill behavior unchanged. **Phase 15 will widen `TaskHandle::abort()` to do the SIGTERM + grace + SIGKILL escalation explicitly** (TASK-04). Phase 14's abort path is "best-effort cooperative" by design. The risk to flag: don't write a Phase 14 test that asserts "the child process is dead within X ms of abort" — there's no guarantee.

**Warning signs:** A Phase 14 test that hangs because it waits for the child PID to disappear after `handle.abort()`. Don't write that test in Phase 14; it's a Phase 15 assertion.

### P-5: Metro registration race with parallel commands

**What goes wrong:** Both worktree A and B simultaneously dispatch `Action::MetroStart`. Both update() calls land in the same single-threaded event loop (good — no real race), but the second one sees `state.metro.is_running() == false` (because the first one only emitted `Effect::DetectExternalMetro`, hasn't actually spawned yet) and proceeds to also emit `Effect::DetectExternalMetro`. Now two tokio tasks are racing to register a metro handle — `MetroManager::register()` panics on the second.

**Why it happens:** Multiplexing CommandRun on per-slice queues makes the developer think "metro should be parallel too." It's not. D-13 keeps metro special.

**How to avoid:** D-13 is mandatory. The metro special-case stays — `state.metro` (the `MetroManager` at AppState root) remains single-instance globally. The metro-needs-prereq drain step moves into the slice-local drain handler with **the same semantics**: push back to the head of `slice.queue`, dispatch `Action::MetroStart`, wait for `MetroActivityUpdate(Ready)` to drain the head. No new metro queue.

**Warning signs:** A new test introduced in Phase 14 that asserts two metro instances run concurrently. Reject. COVER-01 (`tests/metro_single_instance.rs`) MUST pass unchanged (D-22). [VERIFIED: read]

### P-6: Slice-merge accidentally drops the running task's slice

**What goes wrong:** Periodic `RefreshWorktrees` (every 60s, see `runtime.rs:32` [VERIFIED: read]) re-reads `git worktree list --porcelain` and dispatches `Action::WorktreesLoaded(Vec<Worktree>)`. The naive merge implementation `state.worktrees = HashMap::from_iter(loaded.iter().map(|w| (w.id.clone(), WorktreeSlice::default())))` would obliterate every running task on every refresh.

**Why it happens:** Forgetting D-17. The `merge_slices` helper must:
- For each surviving WorktreeId in `loaded`: keep the existing slice (preserve task + queue + output).
- For each removed WorktreeId (in current map but not in loaded): drop the slice; dropping calls `handle.abort()` on the task if any.
- For each new WorktreeId (in loaded but not in map): insert `WorktreeSlice::default()`.

**How to avoid:** Implement `merge_slices` as the first PR in Step 2 (D-23). Test it directly with an inline test: seed a slice with `task = Some(...)`, simulate `WorktreesLoaded` containing that worktree, assert task survives. Add a second test with the worktree absent, assert slice drops.

**Warning signs:** UI shows the spinner disappear and reappear every 60s, or task output buffers reset every 60s.

### P-7: Forgetting `worktree_id` in `Effect::SpawnTask` (a payload-completeness gotcha)

**What goes wrong:** D-20 lists `Effect::SpawnTask { task_id, worktree_id, spec }` — but the existing `Effect::SpawnCommand { spec, cwd, branch }` shows that `cwd` and `branch` are required to actually spawn the process. Forgetting them means the runner has to look them up by `worktree_id` against a snapshot of `state.worktrees`, which `EffectRunner` doesn't currently hold.

**Why it happens:** D-20 sketches the minimum identity payload. The planner must decide whether to widen the payload to include `cwd` + `branch` (mirroring `SpawnCommand`) or to inject a `WorktreeLookupPort` of some kind.

**How to avoid:** Widen the Effect payload at planning time. Concretely: `Effect::SpawnTask { task_id, worktree_id, spec, cwd, branch }`. This matches the Plan-13-08 convention (effects carry their own context).

**Warning signs:** The runner's `run_spawn_task` body has to clone `state` or the slice map to read `cwd` / `branch`.

## Code Examples

Verified patterns from existing code + adapted for Phase 14.

### Atomic counter for `TaskId::next()` [CITED: doc.rust-lang.org/std/sync/atomic/struct.AtomicU64.html]

```rust
// In src/domain/task.rs (NEW)
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(pub u64);

static NEXT_TASK_ID: AtomicU64 = AtomicU64::new(1);  // start at 1; 0 reserved as "no task" sentinel

impl TaskId {
    pub fn next() -> Self {
        TaskId(NEXT_TASK_ID.fetch_add(1, Ordering::Relaxed))
    }

    /// Test injection — fixture passes its own counter so tests stay isolated.
    pub fn next_for_test(counter: &AtomicU64) -> Self {
        TaskId(counter.fetch_add(1, Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_for_test_is_monotonic() {
        let counter = AtomicU64::new(100);
        let a = TaskId::next_for_test(&counter);
        let b = TaskId::next_for_test(&counter);
        assert_eq!(a, TaskId(100));
        assert_eq!(b, TaskId(101));
    }
}
```

### Slice-routed `CommandOutputLine` (D-08) [VERIFIED: pre-image read at update.rs:493-501]

```rust
// In src/app/update.rs (Phase 14 step 3 rewrite)
Action::CommandOutputLine { task_id, line } => {
    // D-08: route by task_id, not by active_worktree_id. Late stdout from a
    // cancelled task lands here with no matching slice; silently dropped.
    if let Some(slice) = state.worktrees
        .values_mut()
        .find(|s| s.task.as_ref().map(|t| t.id) == Some(task_id))
    {
        slice.output.push_back(line);
        if slice.output.len() > MAX_COMMAND_LINES {
            slice.output.pop_front();
        }
    }
}
```

### `task_for_worktree` helper (SC#4)

```rust
// In src/app/state.rs (Phase 14 step 1 / 2)
pub fn task_for_worktree(
    state: &AppState,
    id: &crate::domain::worktree::WorktreeId,
) -> Option<&crate::domain::task::TaskRecord> {
    state.worktrees.get(id).and_then(|s| s.task.as_ref())
}
```

### `merge_slices` helper (D-17)

```rust
// In src/app/state.rs (or src/domain/worktree_slice.rs — Claude's Discretion)
pub fn merge_slices(
    state: &mut AppState,
    loaded: &[crate::domain::worktree::Worktree],
) {
    let loaded_ids: std::collections::HashSet<_> =
        loaded.iter().map(|w| w.id.clone()).collect();

    // Drop slices for worktrees that disappeared (running task aborted via Drop).
    state.worktrees.retain(|id, slice| {
        if !loaded_ids.contains(id) {
            // Pre-Phase-15: explicit abort on the way out. Once Phase 15
            // lands SIGTERM/SIGKILL escalation on `TaskHandle::abort()`, this
            // line gets the same treatment.
            if let Some(record) = slice.task.take() {
                record.handle.abort();
            }
            false
        } else {
            true
        }
    });

    // Insert default slices for new worktrees.
    for wt in loaded {
        state.worktrees
            .entry(wt.id.clone())
            .or_insert_with(|| WorktreeSlice {
                id: wt.id.clone(),
                ..Default::default()
            });
    }
}
```

### `TaskHandle` port + tokio adapter [VERIFIED: pattern mirrors metro_port.rs:34-57]

```rust
// In src/domain/ports/task_handle.rs (NEW)
pub trait TaskHandle: Send + Sync + std::fmt::Debug {
    fn abort(&self);
}

// In src/infra/task_handle.rs (NEW; alternative: extend infra/command_runner.rs)
use crate::domain::ports::task_handle::TaskHandle;

#[derive(Debug)]
pub struct TokioTaskHandle(pub tokio::task::JoinHandle<()>);

impl TaskHandle for TokioTaskHandle {
    fn abort(&self) { self.0.abort(); }
}
```

### G-21 grep guard in Makefile (Step 8)

```makefile
# In Makefile arch-lint target (appended after G-20)
@echo "=== G-21: pre-Phase-14 global task fields removed ==="
@! rg 'running_command|command_task|command_queue|post_drain_action' src/ \
    2>/dev/null \
    || (echo "G-21 FAIL: pre-Phase-14 global task field references remain in src/"; exit 1)
```

**Caveat:** Run G-21 against `src/` only — the planner must verify `command_runner` (the variable name in `effect_runner.rs:160-178` for `self.adapters.command_runner`) is allowed; it's an Adapters port name, not a deleted field. The grep target is the four exact identifiers, not the `command_runner` substring. The example above hits exact word boundaries via `\b` if needed; the planner can refine.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Global `command_runner.command_queue: VecDeque<CommandSpec>` (single FIFO) | Per-slice `WorktreeSlice.queue: VecDeque<CommandSpec>` (per-worktree FIFO) | This phase (D-11) | TASK-02: parallel command execution across worktrees becomes possible |
| Global `command_runner.running_command: Option<CommandSpec>` | Per-slice `WorktreeSlice.task: Option<TaskRecord>` | This phase (D-01) | TASK-03: identity is `(CommandKind, WorktreeId)`, not "the one running command" |
| `Action::CommandOutputLine(String)` routed by `active_worktree_id` | `Action::CommandOutputLine { task_id, line }` routed by `task_id` lookup | This phase (D-08) | Eliminates fast cancel+respawn race; lines never contaminate the wrong slice |
| `Action::CommandExited` (no payload) | `Action::CommandExited { task_id, status: ExitStatus }` | This phase (D-09) | `ExitStatus` is a domain enum (Success / Failure / Cancelled / Killed) — Phase 15 can emit Cancelled cleanly |
| `Effect::SpawnCommand { spec, cwd, branch }` ad-hoc spawn | `Effect::SpawnTask { task_id, worktree_id, spec, [cwd, branch] }` single chokepoint | This phase (D-10, D-20) | Phase 15's cancel hook reads exactly one site; Phase 16's elapsed render reads exactly one site |
| `CommandRunnerState` sub-struct holding 5 fields | Either deleted entirely (D-15 default) or hollowed | Step 7 of D-23 | Removes a sub-struct that no longer corresponds to a coherent concern |

**Deprecated/outdated:**
- The 5 prereq-coordination flags (`pending_metro_run`, `pending_metro_after_sync`, `pending_switch_path`) — already deleted by Plan 13-09; Phase 14 does NOT reintroduce equivalents.
- `command_runner.post_drain_action: Option<Box<Action>>` global slot — moves per-slice (D-14).

## Assumptions Log

> Claims marked `[ASSUMED]` in the body of this research that the planner / discuss-phase may need to confirm with the user.

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `Effect::SpawnTask` payload should include `cwd` and `branch` (matching `SpawnCommand`) rather than have the runner look them up | "Pattern 4" + "Pitfall P-7" | Wrong choice forces effect_runner to clone state slices — minor, easily fixed in PR review |
| A2 | The "TaskSpawned" Action option (Pattern 3 option (b)) is preferable to populating `slice.task` synchronously in `update()` with placeholder values | "Pattern 3" | Wrong choice means a second migration when someone realizes `Instant::now()` was being called from update() |
| A3 | `merge_slices` belongs in `src/app/state.rs` (not `src/domain/worktree_slice.rs`) because it must call `handle.abort()` on slices being removed | "Code Examples / merge_slices" | If it lives in domain it would need a generic over `H: TaskHandle` parameter or move to `app/`; small refactor |
| A4 | The new G-21 guard is most safely written as `! rg 'running_command\|command_task\|command_queue\|post_drain_action' src/` (4 exact identifiers, not "command_runner" which is an Adapters port name) | "Code Examples / G-21" | Wrong pattern would false-positive on `self.adapters.command_runner` and fail the build |

**If A1..A4 are uncomfortable for the planner to assume:** the discuss-phase can lock them. None block research; all are "should be obvious from code review" but worth flagging.

## Open Questions

1. **Should `Effect::SpawnTask` carry `cwd` and `branch`, or should the runner look them up?** [Assumption A1]
   - What we know: existing `Effect::SpawnCommand` carries them; D-20 sketches `{ task_id, worktree_id, spec }` without them.
   - What's unclear: D-20's sketch is intentionally incomplete vs. accidentally so.
   - Recommendation: planner adds `cwd` + `branch` to the payload; consistent with Plan 13-08 convention.

2. **`update()` writes `slice.task` synchronously vs. via `Action::TaskSpawned`?** [Assumption A2]
   - What we know: `update()` cannot call `Instant::now()` (purity); cannot call `tokio::spawn` (G-04); cannot construct a `TokioTaskHandle` (no JoinHandle yet).
   - What's unclear: whether to mirror the metro pattern (handle delivered via dedicated channel + `state.metro.register()` on the main thread) or invent something new.
   - Recommendation: mirror metro. Add `Action::TaskSpawned { task_id, worktree_id, spec, started_at, handle }` to action.rs. The runner constructs the TaskRecord and ships it via this Action; `update()` populates `slice.task` from there.

3. **Does `EffectRunner` hold a JoinHandle map, or do TaskRecords own their handle exclusively?** [Claude's Discretion per CONTEXT.md]
   - What we know: D-15 alt-(b) suggests the runner could own a JoinHandle map. D-23 step 7 says "JoinHandle map belongs in effect_runner."
   - What's unclear: if TaskRecords own the only handle (slice.task.handle), Phase 15 cancellation works through the slice; if effect_runner duplicates, two abort paths exist.
   - Recommendation: single ownership in slice.task.handle. The runner needs no map — it dispatches one tokio::spawn per Effect::SpawnTask, the resulting JoinHandle goes straight into the TaskRecord. Phase 15's `Action::CommandCancel` calls `slice.task.take().handle.abort()`. Cleaner.

4. **What happens to in-flight tasks on `WorktreesLoaded` when the loaded set is identical to the current set (the 60s refresh case)?**
   - What we know: D-17 says existing slices "are kept" — implies no-op on identity.
   - What's unclear: whether the merge helper iterates and creates spurious work (cloning, cap checks).
   - Recommendation: short-circuit when `loaded_ids == current_ids`. Inline test: refresh with no changes, assert task survives, no allocations beyond the HashSet build.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo` | Build, test, clippy | ✓ | 1.94.1 | — |
| `rustc` | Compile | ✓ | 1.94.1 | — |
| `tokio` (crate, full features) | Existing async runtime | ✓ | 1.49 [VERIFIED: `Cargo.toml:28`] | — |
| `cargo-llvm-cov` | `make cov-check` (G-19) | Per Phase 12, installed locally | 0.8.5 baseline | — |
| `rg` (ripgrep) | `make arch-lint` greps (G-01..G-21) | ✓ (assumed — Phase 13 used it) | n/a | grep is not a viable fallback (different regex syntax) |
| `make` | Makefile target invocation | ✓ | n/a | — |

**No new external dependencies for Phase 14.** All required crates / binaries / runtimes already in place.

## Validation Architecture

> Phase 14 has `nyquist_validation` enabled (config absent → default-on). This section is required.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | `cargo test` (rustc 1.94.1) — `#[test]` + `#[tokio::test]` |
| Config file | `Cargo.toml` (`[lib]` + `[[bin]]` + dev-deps) [VERIFIED: read] |
| Quick run command | `cargo test --lib` (76 tests in 0.00s — pure-update tests) |
| Full suite command | `cargo test --workspace` (76 lib + 2 metro_single_instance + 1 process_group_kill = 79 tests) [VERIFIED: ran] |
| Shape guards | `make arch-lint` (20 active G-01..G-20; G-21 added by this phase) [VERIFIED: read Makefile] |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| TASK-01 | Per-worktree slice replaces 4 globals | unit (inline in `state.rs` + `worktree_slice.rs`) | `cargo test --lib worktree_slice` | ❌ Wave 0 — new file |
| TASK-01 | All 17 existing dispatch_tests still pass with rewritten assertions | unit (inline) | `cargo test --lib dispatch_tests` | ✓ (existing 570 LOC; D-21 rewrite) |
| TASK-01 | G-21 forbids re-introduction of deleted field names | shape guard | `make arch-lint` (G-21) | ❌ Wave 0 — Makefile addition (last plan in D-23) |
| TASK-01 | All 20 existing G-XX guards stay green | shape guard | `make arch-lint` | ✓ (existing) |
| TASK-02 | Parallel yarn-on-A + jest-on-B both have `task.is_some()` simultaneously | unit (inline in `dispatch_tests.rs`) | `cargo test --lib parallel_yarn_jest` | ❌ Wave 0 — new test (D-21) |
| TASK-02 | Metro single-instance preserved (COVER-01 unchanged) | integration | `cargo test --test metro_single_instance` | ✓ (must pass unchanged per D-22) |
| TASK-02 | Process group kill preserved (COVER-02 unchanged) | integration | `cargo test --test process_group_kill` | ✓ (must pass unchanged per D-22) |
| TASK-03 | `task_for_worktree(state, id)` returns the running TaskRecord | unit (inline) | `cargo test --lib task_for_worktree` | ❌ Wave 0 — new helper test |
| TASK-03 | `CommandOutputLine` routes to correct slice regardless of `active_worktree_id` | unit (inline in `dispatch_tests.rs`) | `cargo test --lib output_line_routing` | ❌ Wave 0 — new test (D-21) |
| TASK-03 | `CommandExited` drains slice-local queue, not other slice's queue | unit (inline in `dispatch_tests.rs`) | `cargo test --lib exit_drains_slice_local` | ❌ Wave 0 — new test (D-21) |
| Cross-cut | TaskId monotonicity + injection helper | unit (inline in `task.rs`) | `cargo test --lib task::tests` | ❌ Wave 0 — new file |
| Cross-cut | `merge_slices` keep-running-task semantics | unit (inline) | `cargo test --lib merge_slices` | ❌ Wave 0 — new helper |
| Cross-cut | Late `CommandOutputLine` for cancelled task is silently dropped | unit (inline) | `cargo test --lib stale_output_drop` | ❌ Wave 0 — new test |

### Sampling Rate

- **Per task commit:** `cargo test --lib` (≤ 1s on dev box; covers all inline + dispatch_tests)
- **Per wave merge:** `cargo test --workspace && make arch-lint` (≤ 5s; includes 2 integration tests + 21 grep guards)
- **Phase gate (before `/gsd-verify-work`):** `cargo test --workspace && cargo clippy --all-targets -- -D warnings && make arch-lint && make cov-check`. The 79+ test count must be ≥ 79 (Phase 13 baseline) and ideally higher (we add 5+ new tests per D-21).

### Wave 0 Gaps

These artifacts must be created by Wave 0 of the phase (or first wave that needs them):

- [ ] `src/domain/worktree_slice.rs` — module + inline `#[cfg(test)] mod tests`
- [ ] `src/domain/task.rs` — module + inline `#[cfg(test)] mod tests` for `TaskId::next_for_test`, `ExitStatus` mapping
- [ ] `src/domain/ports/task_handle.rs` — `trait TaskHandle` (no inline test — pure trait def)
- [ ] `src/domain/ports/mod.rs` — `+pub mod task_handle;`
- [ ] `src/infra/task_handle.rs` (or extension to `infra/command_runner.rs`) — `impl TaskHandle for TokioTaskHandle`
- [ ] `Makefile` arch-lint target — `+G-21` echo line + grep
- [ ] New parallelism tests in `src/app/dispatch_tests.rs` (4+ per D-21):
  - parallel yarn-on-A + jest-on-B
  - metro-conflict on A while metro running on B
  - CommandOutputLine routing by task_id (not active_worktree_id)
  - CommandExited slice-local drain
  - stale-task line drop

**Coverage threshold:** Per-file ratchet from Phase 12's BASELINE-COVERAGE.json — `floor(baseline, 5)` applies. New files (`worktree_slice.rs`, `task.rs`) start at whatever the wave-end coverage is; the threshold becomes `floor(initial, 5)` going forward. No regression on existing files.

## Project Constraints (from CLAUDE.md)

- **YOLO mode** — no confirmation gates; auto-approve research, plans, verification unless something is clearly wrong.
- **`check-types` always uses `--incremental`** — already encoded in `CommandSpec::YarnCheckTypes` argv [VERIFIED: `domain/command.rs:87`].
- **Branch labels are per-branch, not per-worktree** — irrelevant to Phase 14 (no label work).
- **Continuous build loop** — phase verifies green before next phase.
- **Architecture:** Rust + Ratatui, domain/infra/app/ui separation, Ousterhout philosophy. Phase 14 must preserve all 20 shape guards (G-01..G-20) green and add G-21.
- **79 tests must pass** — Phase 13 baseline. Phase 14 adds 5+ new tests; total stays ≥ 79.

## Sources

### Primary (HIGH confidence)
- `tokio` 1.49 rustdoc — `JoinHandle::abort()` semantics: synchronous, non-async; awaiting cancelled task yields `JoinError`. [CITED: docs.rs/tokio/1.49.0/tokio/task/struct.JoinHandle.html]
- `tokio` 1.49 rustdoc — `Command::kill_on_drop(true)` triggers when `Child` is dropped; zombie reaping is best-effort. [CITED: docs.rs/tokio/1.49.0/tokio/process/struct.Command.html]
- `std::sync::atomic::AtomicU64` rustdoc — `fetch_add(1, Ordering::Relaxed)` is canonical for monotonic counters; wraparound is two's complement well-defined. [CITED: doc.rust-lang.org/std/sync/atomic/struct.AtomicU64.html]
- `std::mem::discriminant` rustdoc — `Discriminant<T>: Hash + Eq + Copy` since 1.21.0; the planner-supported identity for D-05's collision check. [CITED: doc.rust-lang.org/std/mem/struct.Discriminant.html]
- `.planning/phases/14-per-worktree-task-system-foundation/14-CONTEXT.md` — 23 locked decisions [VERIFIED: read].
- `.planning/REQUIREMENTS.md` §TASK lines 38-46 — TASK-01..06 acceptance criteria [VERIFIED: read].
- `.planning/ROADMAP.md` §Phase 14 — goal, 4 success criteria, dependencies (Phase 12 + Phase 13 complete) [VERIFIED: read].
- `.planning/STATE.md` — Phase 13 closed; Phase 14 unblocked; CONTEXT gathered 2026-04-27 [VERIFIED: read].

### Secondary (MEDIUM confidence)
- `.planning/phases/13-audit-driven-refactors/13-PATTERNS.md:741-793` — sub-struct regroup pattern (D-16 placement reasoning) [VERIFIED: read].
- `src/app/state.rs:111-262` — current `CommandRunnerState` + `AppState` shape [VERIFIED: read].
- `src/app/update.rs:42-577` — `dispatch_command`, `CommandOutputLine`, `CommandExited`, `CommandCancel`, queue manipulation sites [VERIFIED: read].
- `src/app/effect.rs:23-62` — current Effect grammar (15 variants) [VERIFIED: read].
- `src/app/effect_runner.rs:160-178` — `Effect::SpawnCommand` impl, the template Phase 14's `run_spawn_task` mirrors [VERIFIED: read].
- `src/app/runtime.rs:30-134` — event loop, dual-channel pattern (action_tx + handle_tx for `Box<dyn MetroHandle>`) [VERIFIED: read].
- `src/domain/ports/metro_port.rs:34-78` — `MetroHandle` + `MetroPort` template for D-03's `TaskHandle` [VERIFIED: read].
- `src/infra/metro.rs:33-60` — `TokioMetroHandle` impl template for `TokioTaskHandle` [VERIFIED: read].
- `src/infra/command_runner.rs:71-100` — `kill_on_drop(true)` already configured [VERIFIED: read].
- `src/domain/command.rs:9-44, 125-137` — `CommandSpec` flat enum + `is_cancellable()` [VERIFIED: read].
- `src/domain/worktree.rs:9, 22-42` — `WorktreeId(pub String)` + `Worktree` struct [VERIFIED: read].
- `src/domain/pipeline.rs:22-80` — `Recipe`, `Prerequisite`, `DependencyState` (all unchanged in Phase 14) [VERIFIED: read].
- `Makefile:40-115` — 20 active G-XX shape guards [VERIFIED: read].

### Tertiary (LOW confidence — cross-checked)
- 41+21+4+3+2+1 = 72 grep hits for the 4 deleted fields across 6 files [VERIFIED: `grep -rn ... src/`].
- 17 inline tests in `dispatch_tests.rs` (570 LOC) [VERIFIED: `grep -c` and line read].
- 79-test total: 76 lib + 2 metro_single_instance + 1 process_group_kill [VERIFIED: `cargo test`].
- 10 Recipe::expand call sites in update.rs [VERIFIED: `grep -n 'F-204 site' src/app/update.rs`].
- 8 existing domain ports → 9 with `task_handle` [VERIFIED: `wc -l src/domain/ports/mod.rs`].

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — every type already exists in std/tokio; no new dependencies; no API uncertainty.
- Architecture: HIGH — patterns are mirrored from Plan 13-03 (MetroPort) and Plan 13-10 (sub-struct regroup), both shipped + verified.
- Pitfalls: HIGH — P-1 through P-7 each grounded in either a verified rustdoc passage, an existing code site, or a CONTEXT.md decision rationale.
- Validation: HIGH — Nyquist test map cross-references existing 79-test baseline + 20-guard arch-lint corpus.
- Open Questions: MEDIUM — Q1..Q4 are real ambiguities the planner must resolve. None block research; all are decision-points the planner-phase will lock.

**Research date:** 2026-04-28
**Valid until:** 2026-05-28 (30 days — stable Rust + tokio 1.49 + frozen architecture decisions; Phase 15 work may invalidate the "abort = JoinHandle::abort only" simplification)
