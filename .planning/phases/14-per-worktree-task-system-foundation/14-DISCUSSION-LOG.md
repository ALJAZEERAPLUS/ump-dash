# Phase 14: Per-Worktree Task System Foundation - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in `14-CONTEXT.md` — this log preserves the alternatives considered.

**Date:** 2026-04-27
**Phase:** 14-per-worktree-task-system-foundation
**Areas discussed:** WorktreeSlice scope (F-500), TaskId + TaskRecord shape, Action routing payload, Queue strategy

---

## Area selection

| Option | Description | Selected |
|--------|-------------|----------|
| WorktreeSlice scope (F-500) | Full slice from day one vs narrow `HashMap<WorktreeId, TaskRecord>`. Addendum-flagged decision. | ✓ |
| TaskId + TaskRecord shape | Synthetic monotonic id vs `(CommandKind, WorktreeId)` vs hybrid. SC#3 vs SC#4 reconciliation. | ✓ |
| Action routing payload | How `CommandOutputLine` / `CommandExited` learn their worktree/task. | ✓ |
| Queue strategy | Per-worktree FIFO vs no-queue parallel vs hybrid. Recipe + sync-then-X chains. | ✓ |

**User's choice:** All four areas.

---

## Area 1 — WorktreeSlice scope (F-500)

### Q1.1 — What does the per-worktree value type hold from day one?

| Option | Description | Selected |
|--------|-------------|----------|
| Full WorktreeSlice (Recommended) | `WorktreeSlice { id, task, queue, output, output_scroll, post_drain }`. Migrate output now; Phase 15 just adds `cancel_token`. Matches Addendum sketch. | ✓ |
| Narrow TaskRecord only | `HashMap<WorktreeId, TaskRecord>`. Output/scroll/queue stay separate maps. Smaller diff this phase. | |
| Slice but keep output buffers separate | Slice owns task + queue + post_drain + future cancel_token; output buffers stay separate. | |

**User's choice:** Full WorktreeSlice (Recommended) → captured as **D-01** in CONTEXT.md.

### Q1.2 — Where does the slice type live?

| Option | Description | Selected |
|--------|-------------|----------|
| `src/domain/worktree_slice.rs` (Recommended) | Pure data type, no infra refs. Inline `#[cfg(test)]`. Matches `domain/refresh.rs` convention. | ✓ |
| `src/app/worktree_slice.rs` | App-layer only. Choose if slice ends up holding effect-runner-adjacent things. | |
| Inline in `src/app/state.rs` | Sub-struct alongside the other 6 (Plan 13-10 style). Cheapest. | |

**User's choice:** `src/domain/worktree_slice.rs` (Recommended) → captured as **D-02** in CONTEXT.md.

### Q1.3 — How does the domain-pure slice expose task cancellation? (follow-up because tokio types can't sit in `domain/`)

| Option | Description | Selected |
|--------|-------------|----------|
| Opaque trait object via domain port (Recommended) | `TaskRecord { ..., cancel: Box<dyn TaskHandle> }`; trait in `domain/ports/task_handle.rs`; tokio impl in infra/. Mirrors Phase 13/F-004 MetroPort/MetroHandle. | ✓ |
| JoinHandle in app/, not in slice | Slice stays pure data; separate `HashMap<TaskId, JoinHandle<()>>` lives in app/effect_runner. Cheapest but splits identity. | |
| TaskId-only slice + `Effect::AbortTask(TaskId)` | Slice holds only TaskId; effect_runner owns JoinHandle map. Domain knows nothing about cancellation mechanism. | |

**User's choice:** Opaque trait object via domain port → captured as **D-03** in CONTEXT.md.

---

## Area 2 — TaskId + TaskRecord shape

### Q2.1 — How is TaskId generated?

| Option | Description | Selected |
|--------|-------------|----------|
| Monotonic u64 from atomic counter (Recommended) | `TaskId(u64)` from `AtomicU64`. Cheap, debuggable, deterministic in tests. | ✓ |
| UUID v4 | `TaskId(Uuid)`. Globally unique, no shared counter. Adds `uuid` crate dep, harder to read. | |
| `(CommandKind, WorktreeId)` is the only id | No synthetic id — collision identity IS task identity. Simplest, but conflicts with SC#3 wording. | |

**User's choice:** Monotonic u64 from atomic counter → captured as **D-04** in CONTEXT.md.

### Q2.2 — What does CommandKind mean for collision identity?

| Option | Description | Selected |
|--------|-------------|----------|
| Reuse CommandSpec discriminant (Recommended) | `std::mem::discriminant(&spec)` is the kind. Yarn(Install)+Yarn(Install) collide; Jest{A}+Jest{B} also collide. Phase 15 leans on this. | ✓ |
| New CommandKind enum | Add parallel `enum CommandKind`. Coarser collision (all yarn ops collide). Adds `CommandSpec::kind()` mapping. | |
| Defer — Phase 15 decides | Phase 14 stores full `CommandSpec`; Phase 15 introduces collision concept. Keeps Phase 14 narrow. | |

**User's choice:** Reuse CommandSpec discriminant → captured as **D-05** in CONTEXT.md.

### Q2.3 — TaskRecord field set?

| Option | Description | Selected |
|--------|-------------|----------|
| id + spec + started_at + handle (Recommended) | `TaskRecord { id, spec, started_at, handle }`. `started_at` set up-front so Phase 16 MM:SS render works. | ✓ |
| Above + worktree backref | Add `worktree_id`. Redundant with HashMap key, but self-describing in logs. | |
| Minimal — id + spec + handle | No started_at this phase; Phase 16 adds it later. | |

**User's choice:** id + spec + started_at + handle → captured as **D-06** in CONTEXT.md (D-07 derives the no-backref helper from this).

---

## Area 3 — Action routing payload

### Q3.1 — What does CommandOutputLine carry?

| Option | Description | Selected |
|--------|-------------|----------|
| TaskId only (Recommended) | `CommandOutputLine { task_id, line }`. Slice lookup via `task.id == task_id`. Stale lines from cancelled tasks dropped silently. | ✓ |
| WorktreeId only | `CommandOutputLine { worktree, line }`. Cheaper lookup; loses stale-task detection. | |
| Both | `{ worktree, task_id, line }`. Direct routing + stale detection. Largest payload. | |

**User's choice:** TaskId only → captured as **D-08** in CONTEXT.md.

### Q3.2 — What does CommandExited carry?

| Option | Description | Selected |
|--------|-------------|----------|
| TaskId + ExitStatus (Recommended) | `CommandExited { task_id, status }`. Parity with Output. ExitStatus distinguishes cancelled-vs-failed. | ✓ |
| TaskId only, no status | `{ task_id }`. Status looked up separately. | |
| WorktreeId + status | `{ worktree, status }`. Mirror routing-by-worktree. Same stale-task risk. | |

**User's choice:** TaskId + ExitStatus → captured as **D-09** in CONTEXT.md.

### Q3.3 — How are TaskId/WorktreeId threaded into the spawn site?

| Option | Description | Selected |
|--------|-------------|----------|
| New `Effect::SpawnTask` payload (Recommended) | `Effect::SpawnTask { task_id, worktree_id, spec }`. effect_runner constructs the tokio task with these in scope. Single chokepoint. | ✓ |
| Closures in `CommandRunnerPort` | `run(spec, on_line, on_exit)` with closures capturing IDs. Clean but heavier dyn overhead. | |
| Sender wrapper per spawn | effect_runner builds a per-spawn `mpsc::Sender` wrapper that injects IDs. Per-task indirection layer. | |

**User's choice:** New `Effect::SpawnTask` payload → captured as **D-10** in CONTEXT.md.

---

## Area 4 — Queue strategy

### Q4.1 — Queue topology after Phase 14?

| Option | Description | Selected |
|--------|-------------|----------|
| Per-worktree FIFO inside slice (Recommended) | `WorktreeSlice.queue: VecDeque<CommandSpec>` + `WorktreeSlice.post_drain`. Recipe expansion targets the slice's queue. Metro stays single-instance. | ✓ |
| No queue — immediate dispatch | Recipe expansion fans out into N back-to-back `Effect::SpawnTask` immediately; per-worktree collision (Phase 15) prevents the second from running. Couples Phase 14 to Phase 15's policy. | |
| Hybrid — per-slice + global metro pre-queue | Per-worktree queue plus separate `metro_pending_queue` for cross-worktree metro waits. More state. | |

**User's choice:** Per-worktree FIFO inside slice → captured as **D-11** in CONTEXT.md.

### Q4.2 — `Recipe::expand` result destination?

| Option | Description | Selected |
|--------|-------------|----------|
| All steps to originating worktree's queue (Recommended) | Recipe issued in worktree A → every expanded `CommandSpec` lands in `slice_A.queue`. Metro special-case via push_front + MetroStart unchanged. | ✓ |
| Cross-worktree expansion supported | `Recipe::TargetWorktree(id) | Recipe::TargetCurrent`. Adds plumbing for a non-existent use case. | |
| Inline dispatch — no queue handoff | `Recipe::expand` returns `Vec<Effect>` directly. Closer to Phase 13 ethos but loses FIFO semantics. | |

**User's choice:** All steps to originating worktree's queue → captured as **D-12** in CONTEXT.md.

### Q4.3 — `post_drain_action` scope?

| Option | Description | Selected |
|--------|-------------|----------|
| Per-worktree slot inside slice (Recommended) | `WorktreeSlice.post_drain`. Sync-then-metro-on-A fires when slice_A's queue empties. Preserves Plan 13-09 semantics for single-worktree flows. | ✓ |
| Global slot survives | Keep `CommandRunnerState.post_drain_action` global. Fires when ALL queues empty. Simplest; breaks per-worktree coordination. | |

**User's choice:** Per-worktree slot inside slice → captured as **D-14** in CONTEXT.md.

---

## Closing prompt

| Option | Description | Selected |
|--------|-------------|----------|
| Write context | All decisions captured. | ✓ |
| One more area: WorktreeId stability | Slice lifecycle on WorktreesLoaded merge. | |
| One more area: dispatch_command refactor surface | Helper signature decision. | |
| One more area: test strategy | Existing 17 dispatch tests adaptation strategy. | |

**User's choice:** Write context. (The three deferred areas were folded into CONTEXT.md as D-17/D-18 (lifecycle/merge), D-15/D-19/D-20 (dispatch surface), and D-21/D-22 (test strategy) without re-asking — Claude's discretion based on the patterns already locked above.)

---

## Claude's Discretion

- Exact `tests/` vs inline split for new parallelism tests — Phase 12/D-07 rule applies (subprocess tests in `tests/`, pure update() tests inline).
- Whether `EffectRunner` holds the JoinHandle map directly or via a small `TaskHandleRegistry` helper struct.
- Whether the `merge_slices` helper lives in `domain/worktree_slice.rs` (pure data merge) or in `app/state.rs` (uses `Default` for new slices).

## Deferred Ideas

(Captured in CONTEXT.md `<deferred>` section — not duplicated here.)
