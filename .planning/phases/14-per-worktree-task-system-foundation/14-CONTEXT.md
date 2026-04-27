# Phase 14: Per-Worktree Task System Foundation - Context

**Gathered:** 2026-04-27
**Status:** Ready for planning

<domain>
## Phase Boundary

Replace the three global task fields in `AppState.command_runner` (`running_command`, `command_task`, `command_queue`) with a per-worktree task map keyed by `WorktreeId`. Add `TaskId` + `TaskRecord` domain types. Thread `TaskId`/`WorktreeId` through `Action::CommandOutputLine` / `Action::CommandExited` so output and exit events route to the correct worktree regardless of UI selection. Enable parallel command execution across worktrees while preserving the metro single-instance invariant unchanged.

**In scope (from REQUIREMENTS.md §TASK):**
- TASK-01 — Replace global `running_command` / `command_task` / `command_queue` with per-worktree task state keyed by `WorktreeId` (precondition: COVER-01..04 green — satisfied)
- TASK-02 — Parallel execution across worktrees; metro stays single-instance globally
- TASK-03 — Running task's identity is `(CommandKind, WorktreeId)`; identity available to UI, cancellation, and collision logic via `task_for_worktree(state, id)`

**Out of scope (explicit — belongs to Phase 15/16):**
- Cancellation wiring (`CancellationToken`, SIGTERM/SIGKILL escalation, `kill_on_drop`) — TASK-04, Phase 15
- Collision policy + per-`(CommandKind, WorktreeId)` block-vs-cancel rules — TASK-05, Phase 15
- Per-repo-root `tokio::sync::Semaphore(1)` for yarn install serialization — TASK-06, Phase 15
- Live UI indicators (split Y/P cells, 6-frame spinner, MM:SS elapsed render) — UI-01..03, Phase 16
- F-501 `Command` category split — DEFERRED to backlog per Phase 13/13-02 decision (flat-enum chosen)
- F-111 PersistencePort — DEFERRED to backlog per Phase 13/13-08 decision

**Hard preserved invariants (must not regress):**
- Metro single-instance — only one `MetroHandle` registered globally at any time (COVER-01 characterization test)
- POSIX process-group kill — full subprocess tree terminated on cancel (COVER-02 characterization test)
- TEA `update()` purity — zero `tokio::spawn` / `reqwest` / `tokio::process` in `src/app/update.rs` (G-04, G-05)
- Hexagonal — `app/` only imports infra via the F-111 persistence whitelist; `ui/` imports zero infra (G-01, G-02)
- All 20 shape guards in `make arch-lint` stay green — including a new G-2X guard introduced by this phase if a fresh invariant emerges
- 79 existing tests pass; `cargo clippy --all-targets -D warnings` clean

</domain>

<decisions>
## Implementation Decisions

### F-500 Scoping — WorktreeSlice shape

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
  Rationale: AUDIT-ADDENDUM F-500 explicitly recommends this against the narrow alternative; Phase 15 only adds `cancel_token: Option<CancellationToken>`; Phase 16 only reads `task.started_at`. Choosing narrow now means re-migrating the same data twice.
- **D-02:** Slice lives at **`src/domain/worktree_slice.rs`** as a pure data type (no infra refs, no tokio types). Inline `#[cfg(test)] mod tests` for slice methods, matching `domain/refresh.rs` and `domain/pipeline.rs` convention.
- **D-03:** Cancellation handle is an **opaque trait object via a domain port**. Add `trait TaskHandle: Send + Sync + std::fmt::Debug { fn abort(&self); }` to `src/domain/ports/task_handle.rs` (joins the existing 8-port inventory as the 9th port). The tokio `JoinHandle` wrapper impls it in `src/infra/task_handle.rs` (or inside `infra/command_runner.rs` if it stays a one-liner). The slice holds `Box<dyn TaskHandle>` so domain stays infra-free and G-05 stays green. Mirrors the Phase 13/F-004 `MetroPort` / `MetroHandle` pattern.

### TaskId + TaskRecord

- **D-04:** **`TaskId(u64)`** generated from a process-wide `AtomicU64` counter living in `src/domain/task.rs`. Counter starts at 1 (so `0` can be used as "no task" sentinel in tests if needed). Test injection via a `TaskId::next_for_test()` helper that takes an `&AtomicU64` argument; production calls `TaskId::next()` against a static `AtomicU64`.
- **D-05:** **CommandKind = `CommandSpec` discriminant** — no parallel `enum CommandKind`. The Phase 15 collision identity will be `(std::mem::discriminant(&spec), worktree_id)`. Two `Yarn(Install)` invocations collide; `Jest { filter: A }` and `Jest { filter: B }` collide because they share the variant tag — this is the intended coarseness for "block second jest run on the same worktree".
- **D-06:** **`TaskRecord { id: TaskId, spec: CommandSpec, started_at: Instant, handle: Box<dyn TaskHandle> }`** — final field set for this phase. `started_at` is captured in `EffectRunner::run_spawn_task` at the moment the tokio task is spawned (NOT in `update()` — the pure reducer has no `Instant::now()`). Phase 16 reads `started_at.elapsed()` directly in the render path with zero mutable tick state.
- **D-07:** No `worktree_id` backref inside `TaskRecord` — the slice key already provides it. Helper `task_for_worktree(state, id) -> Option<&TaskRecord>` (SC#4) is `state.worktrees.get(&id).and_then(|s| s.task.as_ref())`.

### Action routing payload

- **D-08:** `Action::CommandOutputLine` becomes **`CommandOutputLine { task_id: TaskId, line: String }`**. Routing: lookup the slice whose `task.as_ref().map(|t| t.id) == Some(task_id)`; if found, push to `slice.output`. Lines arriving for a task that no longer exists (cancelled, slice dropped) are silently dropped. This protects against the fast cancel+respawn race where late stdout from the dead task would otherwise contaminate the new task's output.
- **D-09:** `Action::CommandExited` becomes **`CommandExited { task_id: TaskId, status: ExitStatus }`**. `ExitStatus` is a domain enum (`enum ExitStatus { Success, Failure { code: Option<i32> }, Cancelled, Killed }`) defined in `src/domain/task.rs` — not `std::process::ExitStatus` (that's an infra type). Phase 15 cancellation will emit `Cancelled`; today everything except the cancel path emits `Success` or `Failure { code }`.
- **D-10:** `Effect::SpawnTask { task_id, worktree_id, spec }` is the **single chokepoint** for spawning. Replaces today's ad-hoc spawn paths in `effect_runner`. The runner constructs the tokio task with `task_id` + `worktree_id` captured by-move into the per-task closure; all `Action::CommandOutputLine` / `Action::CommandExited` sends from inside that closure carry the captured `task_id`. No closure-based ports, no per-task sender wrapper.

### Queue strategy

- **D-11:** Queue is **per-worktree FIFO inside the slice** (`WorktreeSlice.queue: VecDeque<CommandSpec>`). The global `CommandRunnerState.command_queue` field is deleted. Drain logic moves from "global CommandExited handler" to "slice-local: when `slice.task` clears, pop_front from `slice.queue`".
- **D-12:** **Recipe expansion targets the originating worktree's slice queue.** When `update()` handles a Recipe-issuing action for worktree A, every expanded `CommandSpec` from `Recipe::expand(&deps)` lands in `slice_A.queue` in order. No cross-worktree recipe targeting (`Recipe::TargetWorktree`) — every existing Recipe variant is single-worktree by construction.
- **D-13:** **Metro special-case stays metro-special.** `state.metro` (the `MetroManager` at AppState root) remains single-instance globally. The metro-needs-prereq drain step (current `update.rs` lines ~530-540: "spec.needs_metro() && !metro.is_running() → push_front + MetroStart") moves into the slice-local drain handler with the same semantics — push back to the head of `slice.queue`, dispatch `Action::MetroStart`, wait for `MetroActivityUpdate(Ready)` to drain the head. No new metro queue.
- **D-14:** **`post_drain` is per-slice** (`WorktreeSlice.post_drain: Option<Box<Action>>`). Sync-then-metro-on-A fires when `slice_A.queue` empties — never observes worktree B's queue state. `CommandRunnerState.post_drain_action` (Plan 13-09 introduced it as a global slot) is deleted along with the other 3 global fields.

### CommandRunnerState fate

- **D-15:** `CommandRunnerState` is **not deleted wholesale** — but loses 4 of its 5 fields. The remaining field (none — all 5 move out: `command_queue`, `running_command`, `command_task`, `post_drain_action` per slice, plus `command_output_by_worktree` + `command_output_scroll_by_worktree` move INTO each slice). After this phase, `CommandRunnerState` either:
  - (a) is removed entirely from `AppState` along with its module,
  - (b) is repurposed to hold per-worktree-task-system globals such as the `AtomicU64` task id counter and the JoinHandle map effect_runner needs (effect_runner is the obvious owner — pick at planning time).
  Planner picks based on what naturally clusters. **Default expectation: delete `CommandRunnerState` entirely** — its purpose is supplanted by the per-slice fields, the JoinHandle map belongs in effect_runner, and the TaskId counter is a static.

### Worktree slice lifecycle

- **D-16:** Slice map is **`HashMap<WorktreeId, WorktreeSlice>` on AppState root** (alongside `MetroManager`, the existing `worktree_browser`, etc.) — NOT inside any sub-struct. Reason: the slice IS the per-worktree replacement for what used to be 4 fields of `CommandRunnerState` plus 2 fields of `WorktreeBrowserState` — wrapping it inside another struct just re-introduces the access-path noise Plan 13-10 avoided.
- **D-17:** **Merge strategy on `WorktreesLoaded`:** existing slices for surviving `WorktreeId`s are **kept** (their tasks + queues + output buffers survive a worktree-list refresh); slices for removed `WorktreeId`s are dropped (the running task's `handle.abort()` is called as part of slice removal — pre-Phase-15 it's just JoinHandle abort, Phase 15 will widen to SIGTERM/SIGKILL); slices for new `WorktreeId`s are inserted with `Default::default()`. Implemented as a small `merge_slices(state, loaded_worktrees)` helper called from the existing `WorktreesLoaded` handler.
- **D-18:** **Worktree removal mid-task** (`pending_worktree_removal` flow): the `merge_slices` helper handles task abort on slice drop — no separate cancel pass needed in the worktree-remove handler. Phase 15 will tighten this if needed.

### Action taxonomy

- **D-19:** Three Actions get a payload change:
  - `CommandOutputLine(String)` → `CommandOutputLine { task_id: TaskId, line: String }`
  - `CommandExited` (no payload) → `CommandExited { task_id: TaskId, status: ExitStatus }`
  - `CommandRun(CommandSpec)` stays the same — but `update()` MUST resolve its target `WorktreeId` from `active_worktree_id(state)` at the dispatch site (not later). Recipe-issuing actions follow the same rule.
  Other Actions touched only structurally (read from per-slice queue instead of global): `CommandQueued`, `CommandCancel`, `CommandOutputClear`.

### Effect taxonomy

- **D-20:** One new Effect:
  - `Effect::SpawnTask { task_id: TaskId, worktree_id: WorktreeId, spec: CommandSpec }`
  Today's command-spawn effects collapse into this. Effect surface for Phase 14: `+1 variant`, `0 deletions` (existing variants stay until effect_runner is fully ported).

### Test strategy

- **D-21:** Existing 17 dispatch tests in `src/app/dispatch_tests.rs` must pass after migration. Approach: rewrite assertions from `assert!(state.command_runner.running_command.is_some())` to `assert!(state.worktrees.get(&id).and_then(|s| s.task.as_ref()).is_some())`. Add new per-worktree parallelism tests:
  - "yarn install on A while jest on B → both slices have `task.is_some()` simultaneously"
  - "MetroStart on A while metro running on B → existing conflict path triggers; `state.metro` retains single registration"
  - "CommandOutputLine routes to correct slice regardless of `active_worktree_id`"
  - "CommandExited drains slice-local queue, not the other slice's queue"
- **D-22:** Existing 2 characterization tests (`tests/metro_single_instance.rs`, `tests/process_group_kill.rs`) MUST pass unchanged. If they need to change, that's a regression signal — investigate before editing the test.

### Migration sequencing

- **D-23:** Plans land in this order (planner refines exact split):
  1. Domain types: `WorktreeSlice`, `TaskId`, `TaskRecord`, `ExitStatus`, `TaskHandle` port, `task_for_worktree` helper. No app/ changes yet.
  2. AppState shape: add `worktrees: HashMap<WorktreeId, WorktreeSlice>` root field; `WorktreesLoaded` merge logic. Old `CommandRunnerState` fields stay alive in parallel.
  3. Action payload widening: `CommandOutputLine { task_id, line }`, `CommandExited { task_id, status }` — update every match site, route through both old and new state during transition.
  4. `Effect::SpawnTask` + effect_runner port: new spawn path uses per-slice routing; old path stays alive for unmigrated call sites.
  5. Dispatch migration: every `dispatch_command` / Recipe-expansion site flips from "push to global queue" to "push to slice queue".
  6. Drain migration: `CommandExited` handler reads slice-local queue; `post_drain` per-slice.
  7. Delete the 4 global fields + `CommandRunnerState` (or reduce to its leftover); flip `active_output` / `active_output_scroll` helpers to read from the slice.
  8. New shape guard added to `make arch-lint`: G-21 — "no `running_command` / `command_task` / `command_queue` field references anywhere in `src/`".

### Claude's Discretion

- Exact `tests/` vs inline split for new parallelism tests — planner picks based on whether they need real subprocesses (Phase 12/D-07 rule: subprocess tests live in `tests/`, pure update() tests stay inline).
- Whether `EffectRunner` holds the JoinHandle map directly or via a small `TaskHandleRegistry` helper struct — planner picks based on whether the Phase 15 cancellation hook reads cleaner one way or the other.
- Whether the `merge_slices` helper lives in `domain/worktree_slice.rs` (pure data merge) or in `app/state.rs` (uses Default for new slices, knows about app concerns) — planner picks based on whether the merge needs anything app-layer.

### Folded Todos

None — no pending todos matched Phase 14 scope. The `todo.org` at repo root is unread context (per CLAUDE.md it's not part of GSD's todo system).

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone & Requirements
- `.planning/PROJECT.md` — Core value (per-worktree everything), Ousterhout constraint, 23 of 25 AUDIT findings closed in Phase 13, Phase 14 unblocked
- `.planning/REQUIREMENTS.md` §TASK (lines 38-46) — TASK-01..06 acceptance criteria; **only TASK-01..03 are in scope this phase**; TASK-04..06 belong to Phase 15
- `.planning/ROADMAP.md` §Phase 14 — goal, 4 ROADMAP success criteria, depends on Phase 12 + Phase 13 (both complete)

### Phase 13 Outputs (the structural baseline this phase builds on)
- `.planning/phases/13-audit-driven-refactors/13-VERIFICATION.md` — confirms `update()` pure, hexagonal Adapters, 20 shape guards green, 79 tests pass — the locked baseline this phase must not break
- `.planning/phases/13-audit-driven-refactors/13-PATTERNS.md:741-793` — sub-struct regroup pattern (D-16 follows it for placement of `worktrees` HashMap at AppState root)
- `.planning/phases/13-audit-driven-refactors/13-09-SUMMARY.md` — Plan 13-09 introduced `post_drain_action` as a global slot; **D-14 makes it per-slice** (regression-watch)
- `.planning/phases/13-audit-driven-refactors/13-10-SUMMARY.md` — Plan 13-10 sub-struct regroup; this phase ADDS `worktrees: HashMap<WorktreeId, WorktreeSlice>` and DELETES (or hollows) `CommandRunnerState`

### Audit Findings Driving This Phase
- `.planning/phases/11-architecture-audit/AUDIT-ADDENDUM.md` §F-500 — **the explicit Phase 14 scoping decision**; D-01 follows the Addendum's `WorktreeSlice` recommendation; Addendum text: "Per-worktree task map should be `HashMap<WorktreeId, WorktreeSlice>`, not `HashMap<WorktreeId, TaskRecord>`"
- `.planning/phases/11-architecture-audit/AUDIT-ADDENDUM.md` §F-501 — flat-enum `is_cancellable` chosen in Phase 13/13-02; D-05 reuses `CommandSpec` discriminant for the same reason (no category split)
- `.planning/phases/11-architecture-audit/AUDIT.md` — base audit findings; F-200..F-209 inform what app/ layout must stay coherent

### Code Reference Points (sites this phase touches)

**State layer — what gets restructured:**
- `src/app/state.rs:111-136` — `CommandRunnerState` and its 5 fields; **D-15 either deletes the struct or hollows it**
- `src/app/state.rs:243-262` — `AppState` struct; **D-16 adds `pub worktrees: HashMap<WorktreeId, WorktreeSlice>` root field**
- `src/app/state.rs:269-308` — `active_worktree_id` / `active_output` / `active_output_scroll` helpers; flip to read slice-local

**Update layer — drain + dispatch sites that move to per-slice:**
- `src/app/update.rs:42-69` — `dispatch_command` helper that sets `running_command` and stores `command_task`; **D-10 funnels through `Effect::SpawnTask`**
- `src/app/update.rs:480-560` — `CommandOutputLine` + `CommandExited` handlers; **D-08, D-09 widen the payloads; D-11 reads slice-local queue**
- `src/app/update.rs:200-240` — queue clear + sync-then-X `post_drain_action` consumer — moves to per-slice
- `src/app/update.rs:556-577` — `CommandCancel` path — pre-Phase-15 just calls `slice.task.handle.abort()`
- All 11 `Recipe::expand` consumer sites in `src/app/update.rs` (search comments for `F-204 site`) — D-12 retargets to slice queue

**Effect/runner layer:**
- `src/app/effect.rs:23+` — `pub enum Effect`; **D-20 adds `SpawnTask { task_id, worktree_id, spec }`**
- `src/app/effect_runner.rs:1-338` — `EffectRunner` impl; new `run_spawn_task` method captures `task_id` + `worktree_id` into the per-task closure (D-10) and owns the JoinHandle map for Phase 15 cancellation (D-15 alt-(b))
- `src/app/adapters.rs:33-43` — `Adapters` struct holding 7 `Arc<dyn Port>` references; **may grow to 8 if `TaskHandle` factory is injected as a port** (planner decides)

**Domain types this phase introduces:**
- `src/domain/worktree_slice.rs` — **NEW**, per D-02 (`WorktreeSlice`)
- `src/domain/task.rs` — **NEW** (`TaskId`, `TaskRecord`, `ExitStatus`, atomic counter)
- `src/domain/ports/task_handle.rs` — **NEW**, per D-03 (`trait TaskHandle: Send + Sync + Debug`)
- `src/domain/ports/mod.rs` — index gets a 9th port entry
- `src/infra/task_handle.rs` (or extend `src/infra/command_runner.rs`) — `impl TaskHandle for tokio::task::JoinHandle<()>` wrapper

**Domain invariants that must not move:**
- `src/domain/metro.rs:64+` — `MetroManager` stays at `AppState` root, single-instance enforced by `register()`/`take_handle()`. **D-13: metro stays metro-special**.
- `src/domain/command.rs:125-137` — `is_cancellable()` predicate; D-05 reuses the discriminant for kind, does not modify the predicate
- `src/domain/pipeline.rs` — `Recipe`, `Prerequisite`, `DependencyState`; D-12 reuses `Recipe::expand(&DependencyState) -> Vec<CommandSpec>` unchanged
- `src/domain/worktree.rs:9` — `pub struct WorktreeId(pub String)`; the HashMap key for D-16

### Coverage Gate (the Phase-12 safety net)
- `tests/metro_single_instance.rs` — COVER-01; **MUST pass unchanged** (regression signal if it fails)
- `tests/process_group_kill.rs` — COVER-02; **MUST pass unchanged**
- `src/app/dispatch_tests.rs` (570 LOC, 17 inline tests) — COVER-03; per D-21 these get rewritten field-by-field; assertion shape changes, behavior coverage stays

### Architectural Guards (`make arch-lint`)
- `Makefile` lines 41-76+ — 20 active shape guards (G-01..G-20); **all 20 must stay green after this phase**
- **New guard expected (G-21):** "no `running_command` / `command_task` / `command_queue` references anywhere in `src/`" — replaces those identifiers with the per-slice equivalents; concrete grep TBD by planner

### Project Conventions
- `CLAUDE.md` — `check-types` always uses `--incremental`; YOLO mode no confirmations at workflow gates; phase loop continues until verified
- 13-PATTERNS.md mapping convention: each new file gets a closest-analog reference (planner-phase will produce `14-PATTERNS.md`)
- Inline `#[cfg(test)] mod tests` for pure domain logic (per D-02)
- No `mockall` / `rstest` / `proptest` (Phase 12/D-13 holds — no new dev-deps unless absolutely necessary)

### External / Tool Docs
- `tokio::task::JoinHandle` — used opaquely behind `TaskHandle` trait; consulted by planner for the `impl TaskHandle for JoinHandle` shape
- `tokio::sync::CancellationToken` — Phase 15 surface; **NOT used in Phase 14**, but mentioned because the slice's future `cancel_token` field is the obvious next-phase landing site

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`MetroManager` / `MetroPort` / `MetroHandle` (Phase 13/F-004 / 13-03)** — exact pattern to mirror for `TaskHandle` port + opaque trait object (D-03). The trait stays in `domain/ports/`; the tokio impl lives in `infra/`.
- **`Adapters` struct (Phase 13/F-202 / 13-08)** — composition root for ports; if `TaskHandle` needs a factory port (planner decides), it gets injected here. Otherwise plain `Box::new(JoinHandleWrapper(handle))` at the spawn site is fine.
- **`Recipe::expand(&DependencyState) -> Vec<CommandSpec>` (Phase 13/F-204 / 13-09)** — produces an ordered command list; D-12 just retargets the destination from the global queue to a slice queue. Zero changes needed inside the Recipe API.
- **`is_cancellable()` predicate (Phase 13/REFACTOR-02)** — already type-driven; Phase 15 will consume it; no change in Phase 14 but referenced by D-05 (kind = discriminant works for both predicate and collision identity).
- **`CommandRunnerState.post_drain_action` (Plan 13-09)** — the generalized post-drain coordination slot is the design template for D-14's per-slice version.
- **Sub-struct convention (Plan 13-10 / F-209)** — `WorktreeBrowserState`, `MetroState`, `JiraState`, etc.; `WorktreeSlice` is a sibling concept but lives at AppState root rather than inside a sub-struct because it IS the replacement for what used to be cross-cutting global fields (D-16).
- **`active_worktree_id` / `active_output` / `active_output_scroll` (state.rs:269-308)** — UI hot-loop entry points; D-15 flips their internals to slice-local lookup. Signatures stay identical so `ui/panels.rs` callsites don't change.

### Established Patterns
- **TEA `update(state, action) -> Vec<Effect>`** — D-10/D-20 stay inside this purity; `Effect::SpawnTask` is data, not a spawn. effect_runner does the actual `tokio::spawn`.
- **`Adapters` injection at composition root (`src/main.rs`)** — `JoinHandleWrapper`-style adapter constructed where `tokio::process::Command` already lives.
- **Inline `#[cfg(test)] mod tests` for pure domain logic** — `domain/worktree_slice.rs`, `domain/task.rs` follow this (slice merge logic, TaskId counter, ExitStatus mapping all unit-testable without tokio).
- **Subprocess characterization tests live in `tests/`** — Phase 12/D-07. New parallelism integration tests (if any need real subprocesses) go here; pure update() tests stay in `dispatch_tests.rs`.
- **Shape guards in `Makefile arch-lint`** — every architectural invariant gets a `! rg` line. New G-21 follows the established G-04/G-05 form (`! rg 'pattern' src/...`).

### Integration Points
- **`src/app/state.rs::AppState`** — gains `pub worktrees: HashMap<WorktreeId, WorktreeSlice>`; loses `pub command_runner: CommandRunnerState` (or sees `command_runner` reduced to leftover globals per D-15).
- **`src/app/effect_runner.rs::EffectRunner`** — gains a `run_spawn_task` method, owns the JoinHandle map (or `TaskHandleRegistry` helper, planner's choice).
- **`src/app/effect.rs::Effect`** — `+SpawnTask { task_id, worktree_id, spec }` variant.
- **`src/app/update.rs`** — every site that today reads/writes `state.command_runner.{running_command, command_task, command_queue, post_drain_action}` flips to `state.worktrees.get_mut(&worktree_id).map(|s| &mut s.{task, queue, post_drain})`.
- **`src/app/handle_key.rs`** — unchanged (KEYBINDINGS dispatch is per-Action, not per-target-state); regression-check that the dispatch tests still pass with new payload shapes.
- **`src/ui/panels.rs`** — unchanged (uses `active_output` / `active_output_scroll` helpers whose signatures don't change).
- **`src/domain/ports/mod.rs`** — `+pub mod task_handle;` line (G-10 stays satisfied).
- **`Makefile`** — new G-21 grep guard appended to the existing 20 (echo line, fail-on-match command).
- **`src/main.rs`** — composition root may construct a `TaskHandleFactory` adapter if the planner picks the port-injected route (alternative: spawn-site uses concrete `JoinHandleWrapper(jh)` with no factory).
- **`src/infra/command_runner.rs`** — already spawns with `.process_group(0)` + `kill_on_drop(true)` (Phase 12/D-10 characterized this); the new spawn path inherits both. The JoinHandle returned by `tokio::spawn` is wrapped into `Box<dyn TaskHandle>` here or in the effect_runner.

</code_context>

<specifics>
## Specific Ideas

- The Addendum's `WorktreeSlice` sketch is the literal blueprint — same field names, same semantics. Deviations from the sketch must justify themselves. (D-01 follows it precisely except `pending: PendingFlags` is replaced by `post_drain: Option<Box<Action>>` because Plan 13-09 already collapsed the 5 prereq flags into the post_drain slot — the Addendum was written before 13-09 landed.)
- `TaskId(u64)` from a static `AtomicU64` is the cheapest possible identity — exactly what's needed for log correlation and the SC#3/SC#4 contract. Don't reach for UUID.
- `ExitStatus` as a domain enum (not `std::process::ExitStatus`) is the only way Phase 15 can emit `Cancelled` cleanly. The `From<std::process::ExitStatus>` impl lives in infra (`infra/command_runner.rs` or `infra/task_handle.rs`).
- Stale-task line drop (D-08) is non-negotiable: if a user fast-cancels yarn install and immediately re-runs it, the dead process's late stdout MUST NOT contaminate the new task's output buffer. Routing by `task_id` (not `worktree_id`) makes this a one-line check.
- The new G-21 shape guard MUST land in the same plan that deletes the global fields — otherwise a partial regression could ship "we deleted `running_command` from state but didn't notice we still reference it in some test helper".
- Migration sequencing (D-23) is a planner suggestion, not a hard order — but step 8 (G-21 + delete) MUST be the last step. If G-21 fails, the deletion is not done.

</specifics>

<deferred>
## Deferred Ideas

- **Cancellation wiring** (`CancellationToken`, SIGTERM/SIGKILL escalation, `kill_on_drop`) — TASK-04, Phase 15. The `TaskHandle::abort()` in Phase 14 is just `JoinHandle::abort()` (cooperative tokio cancel); Phase 15 widens the trait to include OS-level kill via the existing process-group path.
- **Collision policy** — TASK-05, Phase 15. Phase 14 lays the identity foundation (`(discriminant, WorktreeId)`); Phase 15 decides per-category block-vs-cancel rules.
- **Per-repo-root `tokio::sync::Semaphore(1)` for yarn install serialization** — TASK-06, Phase 15. Lives near the spawn site, keyed by `slice_for_repo_root(slice).repo_root` (or equivalent helper).
- **Live UI indicators** — UI-01..03, Phase 16. Reads `slice.task.as_ref().map(|t| t.started_at.elapsed())` directly in the render path. Phase 14 sets up the data; Phase 16 reads it.
- **F-501 `Command` category split** — DEFERRED to backlog per Phase 13/13-02. Flat-enum + discriminant-as-kind (D-05) satisfies Phase 14 without re-opening the category-split debate.
- **`WorktreeSlice.pending: PendingFlags` field** (from the Addendum sketch) — not added; Plan 13-09 already eliminated the 5 prereq flags by absorbing them into Recipe variants + the post_drain slot. If a per-worktree pending flag is genuinely needed in Phase 15, it goes onto the slice then.
- **Cross-worktree Recipe targeting** (`Recipe::TargetWorktree(id) | Recipe::TargetCurrent`) — D-12 explicitly says no; every existing Recipe is single-worktree. Revisit if a cross-worktree workflow ever appears.
- **`task_history: Vec<TaskRecord>` per slice** — explicitly deferred per REQUIREMENTS §Future Requirements ("Task history persistence per worktree (not this milestone)").
- **`current task name displayed inline in the worktree row`** — explicitly deferred per REQUIREMENTS §Future Requirements ("user opted for spinner + elapsed only").
- **CI integration of `make arch-lint`** — post-milestone, same as Phase 12 coverage tooling.
- **`cargo-deny` / `cargo-modules` / `cargo-depgraph` static graphs** — post-milestone per REQUIREMENTS §Future Requirements.

</deferred>

---

*Phase: 14-per-worktree-task-system-foundation*
*Context gathered: 2026-04-27*
