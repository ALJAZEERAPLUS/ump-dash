# Phase 16: Live UI Indicators - Context

**Gathered:** 2026-05-22
**Status:** Ready for planning

<domain>
## Phase Boundary

Render-only phase. Make the worktree table reflect live per-worktree task state that Phase 14/15 already track. No new state, no new domain types, no mutable tick counters.

**In scope (from REQUIREMENTS.md §UI):**
- UI-01 — Split the merged `Y/P` string into two independent cells. Each cell renders its letter OR a spinner independently.
- UI-02 — While a yarn-install task runs on a worktree, its `Y` cell shows a rotating 6-frame spinner; while a pod-install task runs, its `P` cell shows the spinner. Every other running task (jest, lint, check-types, unit-tests, run-android, run-ios, shell, clean, git) shows the same spinner + label + elapsed in a new rightmost "task" column. All indicators return to static letters / empty when idle.
- UI-03 — Each running worktree row shows a live elapsed counter computed in the render path from `task.started_at.elapsed()`. No mutable frame-counter or tick field added to `AppState`.

**Out of scope:**
- Any new `AppState` field for animation/time (the 250ms tick at `src/app/runtime.rs:31` already drives redraw — render reads `Instant::elapsed()` fresh each frame).
- Task-name-inline-in-row beyond the new task column (deferred per Phase 14 — "spinner + elapsed only").
- Task history / multiple concurrent tasks per worktree (collision rules guarantee ≤1 task per slice).
- Changing collision/cancellation behavior (Phase 15, complete).

**Hard preserved invariants:**
- `update()` purity — render reads `Instant`, but the pure reducer still gets no `Instant::now()`.
- `ui/` imports zero infra (G-02). Spinner frames + elapsed formatting are pure UI helpers.
- All `make arch-lint` shape guards stay green; existing tests pass.

</domain>

<decisions>
## Implementation Decisions

### Grid layout

- **D-01:** Worktree row gains ONE new rightmost column ("task"). New column layout: `[▶ icon][Y][P][branch][ticket+title][dir][task]`. The merged `Y/P` icon cell splits into independent `Y` and `P` glyphs.
- **D-02:** `Y` cell and `P` cell are independent — `Y` shows spinner only for a running **yarn-install** task, `P` shows spinner only for a running **pod-install** task. They never carry any other category's state.
- **D-03:** Y/P separator: **drop the slash**. Render as `Y P` (Y, single space, P) — 3 visual chars. Replaces today's `Y` + `/P` form. When yarn runs: `◐ P`; when pod runs: `Y ◐`.
- **D-04:** The new task column shows spinner + short label + elapsed for **every running task that is not yarn-install or pod-install** (jest, lint, check-types, unit-tests, run-android, run-ios, shell, clean-android, clean-cocoapods, rm-node-modules, and git ops). Example cells: `◐ jest 1:34`, `◓ run-ios 2:15`. Empty when no such task runs. Rationale: at most one task per slice (collision rules), so Y/P-cell and task-column are mutually exclusive per row by construction — a row shows a spinner in exactly one place.

### Spinner

- **D-05:** Glyph set = **half-circles** `◐ ◓ ◑ ◒ ◐ ◓` (6 frames, `const SPINNER_FRAMES: [&str; 6]`). Frame index = `task.started_at.elapsed().as_millis() / 150 % 6` (per UI-02 convention). **Width risk noted:** half-circles can render variable-width in some terminals — planner/executor must verify the column constraint accounts for this; fall back to a 1-cell-safe set if it breaks alignment.
- **D-06:** Spinner color = **yellow** (per UI-02). Color is constant while spinning — does not encode staleness.

### Task labels (task column)

- **D-07:** **Short codes:** `yarn` / `pods` / `jest` / `lint` / `types` / `unit-tests` / `run-ios` / `run-and` (run-android) / `shell` / clean variants. (`yarn`/`pods` only appear in the task column in the theoretical case they ever route there — normally they live in Y/P cells; the label table should cover every `CommandSpec` discriminant so no variant renders blank.) Git op labels are planner's discretion (short verb e.g. `pull`, `push`, `rebase`).

### Elapsed time format

- **D-08:** **Seconds under 60, then M:SS.** `5s`, `42s`, `1:34`, `12:03`. Minutes unpadded. Pure formatting helper in `ui/`.

### Staleness color (Y/P cells)

- **D-09:** While a spinner occupies the Y or P cell: glyph is **yellow** (D-06); staleness color hidden for that cell.
- **D-10:** After the task exits: **no special clear logic.** The Y/P cell recomputes its color from `wt.stale` / `wt.stale_pods` on the next render — same source as today. Staleness re-emerges naturally. No optimistic green-on-success, no forced refresh on exit.

### Claude's Discretion

- Exact git-op short labels in the task column (D-07).
- Whether the task column's `Constraint` is fixed `Length` or `Min` — planner picks based on longest label+time (`◐ unit-tests 12:03`).
- Where the spinner-frame helper + elapsed-format helper live in `ui/` (new `ui/indicators.rs` vs inline in `ui/panels.rs`) — planner picks; both are pure, unit-testable.
- Exact mapping of which `CommandSpec` discriminants count as "yarn-install" (Y) vs "pod-install" (P) vs task-column — default: only `YarnInstall`→Y, `YarnPodInstall`→P, everything else→task column. Confirm no other variant should claim Y/P.

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone & Requirements
- `.planning/REQUIREMENTS.md` §UI (lines 49-51) — UI-01, UI-02, UI-03 acceptance criteria (the locked spec for this phase)
- `.planning/ROADMAP.md` §Phase 16 (lines 164-173) — goal + 3 success criteria; depends on Phase 14 (task state) + Phase 15 (cancellation stable)
- `.planning/PROJECT.md` — v1.3 milestone target "UI live indicators" bullet

### Phase 14 outputs (the data this phase reads)
- `.planning/phases/14-per-worktree-task-system-foundation/14-CONTEXT.md` — D-06 (`TaskRecord { id, spec, started_at: Instant, handle }`), D-05 (CommandKind = discriminant), D-07 (`task_for_worktree(state, id)` helper). **Phase 16 reads `started_at.elapsed()` in render path — the explicit Phase-14 hand-off.**
- `.planning/phases/14-per-worktree-task-system-foundation/14-VERIFICATION.md` — confirms per-slice task state landed

### Phase 15 outputs (why spinner appear/disappear is deterministic)
- `.planning/phases/15-task-cancellation-collision-shared-resource-semaphore/15-VERIFICATION.md` — cancellation + collision stable; ≤1 task per slice; task clears on exit/cancel → spinner disappears

### Code reference points (sites this phase touches)
- `src/ui/panels.rs:82-138` — current worktree-row render: `▶ Y/P` icon spans, staleness coloring (`wt.stale` / `wt.stale_pods`), ticket+title merged column, metro detail-row insertion. **This is the file UI-01/02/03 modify.**
- `src/ui/panels.rs:156-167` — `Table::new` column `Constraint` array (8/20/Min/16). **New task column constraint appended here; icon col width adjusts for split Y P.**
- `src/app/runtime.rs:31` — existing 250ms `tokio::time::interval` tick; already triggers `terminal.draw` each tick. **No new tick needed — UI-03 satisfied by reading `elapsed()` per frame.**
- `src/domain/task.rs` — `TaskRecord` (`started_at: Instant`, `spec: CommandSpec`); read-only for this phase
- `src/domain/worktree_slice.rs` — `WorktreeSlice.task: Option<TaskRecord>`; the per-row task source
- `src/domain/command.rs:9-44` — `CommandSpec` variants (the label table in D-07 must cover all); `is_cancellable()` at :144 (git-only exclusion); `description()` exists for human-readable names if reused

### Architectural guards
- `Makefile` arch-lint — G-02 (`ui/` imports zero infra); spinner/elapsed helpers stay pure
- `CLAUDE.md` — `check-types --incremental`; YOLO mode; metro/branch-label notes

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`render_worktree_table` in `src/ui/panels.rs`** — the single render site. `icon_spans: Vec<Span>` already builds the `▶ Y /P` span sequence per row — split + spinner logic slots directly here.
- **`task_for_worktree(state, id)` (Phase 14/D-07)** — clean accessor for the row's running task; render calls it per row to get `Option<&TaskRecord>`.
- **`CommandSpec::description()` (`src/domain/command.rs:257+`)** — existing human-readable names; could feed labels, though D-07 picks tighter short codes for the narrow column.
- **250ms tick (`src/app/runtime.rs:31`)** — already drives redraw; the whole UI-03 "live counter with no mutable state" works because render recomputes `elapsed()` every frame for free.

### Established Patterns
- **Pure `ui/` helpers** — formatting/animation are deterministic functions of `(elapsed, spec)`; inline `#[cfg(test)] mod tests` (e.g. spinner frame index at boundary millis, elapsed format at 59s/60s/600s).
- **Ratatui `Cell` / `Span` / `Style` per-cell coloring** — existing `Span::styled` calls show the exact API for yellow spinner + staleness colors.
- **Table `Constraint` array** — adding a column = one entry + width tune; existing 4-column array is the template.

### Integration Points
- **`src/ui/panels.rs::render_worktree_table`** — split icon cell into Y + P cells, add task column, wire spinner + elapsed. Only file with required changes.
- **No `app/`, `domain/`, or `infra/` changes expected** — phase reads existing state. (If a label helper is judged domain-ish it could live in `domain/command.rs`, but default is a pure `ui/` helper to keep G-02 trivially satisfied.)

</code_context>

<specifics>
## Specific Ideas

- Selected mockup (locked visual target):
  ```
  ▶ Y P  feature-x      UMP-1234 Add login   ump-feature
    ◐ P  feature-y      UMP-1235 Fix nav     ump-feature-y  yarn 0:12
    Y ◐  feature-z      UMP-1236 Pod update  ump-feature-z  pods 0:08
    Y P  feature-w      UMP-1237 RN jest     ump-feature-w  ◐ jest 1:34
    Y P  feature-q      UMP-1238 Run iOS     ump-feature-q  ◐ run-ios 2:15
  ```
  (Spinner shown as static `◐` here — animates through `◐ ◓ ◑ ◒` live.)
- Half-circle width is the one real risk — verify column alignment in the target terminal (tmux + iTerm2/standard). If glyphs break alignment, swap `SPINNER_FRAMES` to a 1-cell-safe set without re-touching layout logic.
- "No mutable tick state" is non-negotiable (UI-03) — the counter MUST be `started_at.elapsed()` at render time, never a stored/incremented field.

</specifics>

<deferred>
## Deferred Ideas

- **Optimistic green-on-success for Y/P after install** — considered (D-10 alt), rejected for simplicity; staleness recomputes naturally next render. Revisit only if the post-install lag feels wrong in use.
- **Spinner color encoding staleness** — rejected; diverges from yellow-spinner req.
- **Inline full task name in row** — already deferred in Phase 14 ("spinner + elapsed only").
- **Task history per worktree** — deferred per REQUIREMENTS §Future.
- **Per-task progress (%) indicators** — not in scope; no task emits structured progress.

None — discussion stayed within phase scope.

</deferred>

---

*Phase: 16-live-ui-indicators*
*Context gathered: 2026-05-22*
