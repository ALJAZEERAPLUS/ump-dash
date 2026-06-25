# CommandSpec: Info-Card + Dependency-Graph Refactor — Design

**Date:** 2026-06-26
**Status:** Approved design, ready for implementation planning
**Author:** Ali + Claude

---

## 1. Context & Problem

`CommandSpec` (`src/domain/command.rs`) is a 19-variant enum listing every action the
dashboard can run on a worktree (`git pull`, `yarn install`, `run iOS`, …). It is the
backbone of dispatch: it's stored in the task record, the per-worktree queue, and every
modal's `pending_template`; compared by `std::mem::discriminant` for collision detection;
and turned into a process via `to_argv()`.

The pain ("the smell") is narrower than "behaviour is scattered everywhere". Two specific
problems:

1. **Parallel exhaustive matches + twin drift-guards.** A command's static facts
   (`label`, `collision_policy`, `is_cancellable`, `is_destructive`, `needs_metro`, and
   the post-run `refresh_needed`) live in *separate* match blocks across
   `command.rs`, `refresh.rs`, and `ui/indicators.rs`. Adding/removing a command means
   editing several lists in lockstep, double-guarded by two drift-guard meta-tests
   (`collision_policy_covers_every_variant`, `task_short_label_covers_every_variant`).
   We hit this directly when removing the git checkout/rebase commands — ~7 edit sites.

2. **Prerequisite logic is hand-flattened.** "iOS run needs pods, which needs yarn; metro
   needs yarn" is encoded as hand-written `Recipe` variants (`SyncThenRun`,
   `SyncThenStartMetro`, `ReleaseBuildAndInstall`, `GitFetchThenReset`) in
   `pipeline.rs`, plus `prerequisites()` (flat, metro-only) and a `needs_metro()` bool.
   There is no declared dependency graph — the sequences are pre-computed by hand, and
   "needs metro" exists in two representations (`needs_metro()` and
   `Prerequisite::MetroRunning`).

### What we are NOT doing (rejected approach)

Full **trait objects / one-struct-per-command** (`Box<dyn Command>`) was considered and
rejected. `CommandSpec` is used pervasively as a *value*: cloned into the task record,
the queue, and modals; compared with `==` in ~131 dispatch tests; identified by
`std::mem::discriminant`. Trait objects lose `Clone`/`PartialEq`/discriminant for free,
forcing hand-rolled `clone_box` + a parallel `kind()` enum + rewriting the value-comparing
tests — and 15 of 19 commands carry no data, so they'd become empty unit structs. The
open-closed extensibility traits buy is not needed: the command set is closed and curated,
each wired to a specific keybinding. We keep the enum.

---

## 2. Goals & Non-Goals

**Goals**

- One source of truth for each command's **static facts** (the "info-card").
- A real **dependency graph** that flattens to a linear, de-duplicated task list, replacing
  the hand-written recipes.
- Each command **fully describes itself** — including its dependencies.
- Collapse the two drift-guard meta-tests into a single compiler-enforced exhaustive match.
- Preserve all current behaviour exactly (this is a refactor).

**Non-Goals (explicit out-of-scope)**

- The run-app **device/variant modal partial-application** logic (`command_with_device`,
  `command_with_run_variant`, `try_cached_launch`, etc.). That is a separate mess; leave it
  for a later follow-up.
- Changing **value semantics** or **discriminant-based collision**.
- Moving the UI **3-char status code** (`task_short_label`) out of the UI layer — it's a
  presentation detail and stays in `ui/indicators.rs`.

---

## 3. Design Overview — two layers that compose

| Layer | Answers | Owns |
|---|---|---|
| **Info-card** (`meta()` on `CommandSpec`) | "What *is* this command?" — static facts, incl. its declared dependencies | `command.rs` |
| **Dependency resolver** (`resolve()`) | "Given current worktree state, what ordered task list do I run to satisfy this command?" | `pipeline.rs` |

The card declares facts (including `deps`). The resolver *consumes* the cards to flatten a
goal into a linear `Vec<CommandSpec>`. Each command the resolver emits still has its own
card. They compose; neither owns the other.

This is rolled out in **two independent, separately-shippable phases.**

---

## 4. Phase 1 — The Info-Card (behaviour-preserving)

Introduce one descriptor holding every **static** per-command fact:

```rust
pub struct CommandMeta {
    pub label: &'static str,            // "git pull", "yarn jest <filter>"
    pub destructive: bool,              // → confirm prompt
    pub cancellable: bool,              // false for the 5 git plumbing ops
    pub refresh: RefreshSet,            // what to reload after it finishes
    pub collision: CollisionPolicy,     // BlockNew / CancelPrevious on a duplicate
    // `deps` is added in Phase 2 — Phase 1 holds only the static facts above.
}

impl CommandSpec {
    pub fn meta(&self) -> CommandMeta { match self { /* one row per command */ } }
}
```

- `to_argv()` is `Vec<String>`/argv built per-variant (and interpolates data for `YarnJest`,
  `ShellCommand`). It is **logic, not static data** → stays its own method, unchanged.
- The existing accessors become thin readers so **their signatures don't change** and
  callers/tests are untouched:
  `collision_policy()` → `self.meta().collision`, `label()` → `self.meta().label`,
  `is_cancellable()` → `self.meta().cancellable`, `is_destructive()` → `self.meta().destructive`.
- `refresh_needed(cmd)` in `refresh.rs` becomes `cmd.meta().refresh` (or a thin wrapper).
- The single exhaustive `meta()` match is now the compile-time drift guard; the two
  drift-guard meta-tests collapse to one coverage assertion.
- `needs_metro()` is **left untouched in Phase 1** (it's removed in Phase 2 — no add-then-remove).
- The `deps` field and dependency machinery are **not introduced until Phase 2**.

Phase 1 changes no behaviour and (by keeping signatures) touches almost no test except the
drift-guards.

---

## 5. Phase 2 — The Dependency Graph & Resolver (behaviour-touching)

### 5.1 Dependencies on the card; satisfaction as a method

Add a `deps: &'static [CommandSpec]` field to `CommandMeta`. Each command declares the
commands it depends on, in its card's `deps`:

```rust
// in meta() rows:
UmpRunIos { .. }     => CommandMeta { /* … */, deps: &[EnsureMetro, YarnPodInstall] },
UmpRunAndroid { .. } => CommandMeta { /* … */, deps: &[EnsureMetro] },          // no pods on android
YarnPodInstall       => CommandMeta { /* … */, deps: &[YarnInstall] },
EnsureMetro          => CommandMeta { /* … */, deps: &[YarnInstall] },
YarnInstall          => CommandMeta { /* … */, deps: &[] },
```

A node is **complete when its condition is true** — so each command answers "am I already
satisfied?" against live state. This can't be static (depends on staleness / metro status),
so it's a method, not a card field:

```rust
impl CommandSpec {
    /// Already done? If true, skip me when I appear as a dependency.
    pub fn is_satisfied(&self, ctx: &DependencyState) -> bool {
        match self {
            CommandSpec::YarnInstall    => !ctx.stale_yarn,
            CommandSpec::YarnPodInstall => !ctx.stale_pods,
            CommandSpec::EnsureMetro    => ctx.metro_running,
            _                           => false,   // everything else always runs
        }
    }
}
```

### 5.2 `EnsureMetro` — metro as a first-class node

Per the agreed model: **"if metro is not running, it's not complete."** Metro becomes a
real `CommandSpec` variant (`EnsureMetro`) so it sits in `deps` uniformly:

- `is_satisfied` = `metro_running`; if false → **auto-start** metro.
- "complete" = metro signals **ready** (not "process exited"). The app already detects
  metro-ready and holds dependent runs until then; the runner special-cases `EnsureMetro`'s
  completion trigger and bridges to the existing one-Metro-per-worktree machinery.
- This **removes `needs_metro()`** and `Prerequisite::MetroRunning`: "does X need metro?"
  becomes "does resolving X reach `EnsureMetro`?". Single source of truth = the graph edge.

> ⚠️ This bridge (a daemon node flowing through machinery built for run-to-completion tasks)
> is the **highest-risk piece** of the whole refactor. It is implemented last, behind the
> metro characterization tests (§7), and behaviour must match today's "defer run until
> metro-ready" exactly.

### 5.3 The resolver

```rust
/// Pure. Walk deps depth-first, keep each command only if NOT already satisfied,
/// order dependencies before dependents, dedup (shared YarnInstall lands once).
/// The root goal always runs.
pub fn resolve(goal: CommandSpec, ctx: &DependencyState) -> Vec<CommandSpec>;
```

- Replaces the hand-written `Recipe` variants and `prerequisites()`.
- The same `deps` + `is_satisfied` mechanism covers **two kinds** of ordering: *skippable
  prerequisites* (yarn/pods/metro — `is_satisfied` may be true) and *unconditional pipelines*
  (`is_satisfied` always false, so both steps always run, in order). The old pipeline recipes
  map directly: `AdbInstallApk.deps = &[RnReleaseBuild]` and `GitResetHard` reached via
  `GitResetHardFetch.deps = &[GitFetch]` (or equivalent), with the pipeline steps' `is_satisfied`
  returning false. The resolver must reproduce these exact sequences.
- Pure and unit-testable, exactly like the current `Recipe::expand` tests.
- `DependencyState` (already exists: `{ stale_yarn, stale_pods, is_ios_target }`) gains
  `metro_running` and serves as the `ctx`.

### 5.4 Removed in Phase 2

`Recipe` enum, `Prerequisite` enum, `prerequisites()`, `needs_metro()` — all replaced by
`deps` + `is_satisfied` + `resolve`. `staleness.rs` stays (it computes the ctx inputs).

---

## 6. What explicitly stays unchanged

- The `CommandSpec` enum, its `Debug/Clone/PartialEq`, value semantics.
- `std::mem::discriminant`-based collision detection and `dispatch_command_for_worktree`.
- `to_argv()` and the infra `build_argv()` branch-substitution for `GitResetHard`.
- The UI `task_short_label()` 3-char code (stays in `ui/indicators.rs`).
- The device/variant modal partial-application flow (out of scope; later follow-up).

---

## 7. Testing Strategy — characterization-first, high-level (PRIMARY REQUIREMENT)

**Core rule: lock current observable behaviour with high-level tests BEFORE refactoring.
The refactor must keep them green *without editing them*. If a characterization test needs
to change, that is a behaviour change — stop and confirm before proceeding.**

We test at the highest stable seams, not the internals being moved:

**Seam A — the reducer `update(state, action) -> effects`** (primary; this is the
"don't break stuff" net). Drive an action, assert the resulting effects / modal / queue.

**Seam B — the pure resolver `resolve(goal, ctx) -> Vec<CommandSpec>`** — exhaustive
scenario tests (cheap, like today's `Recipe::expand` tests).

**Seam C — a metadata matrix** — one table-driven test over all 19 commands.

### Phase 1 test set (write first, from CURRENT values)

- `command_metadata_matrix`: a single table listing all 19 commands with their expected
  `(label, cancellable, destructive, refresh, collision)`, asserted against `meta()`. This
  is the golden snapshot — any drift fails the build. Authored from current behaviour
  *before* the card exists.
- Keep all existing collision / cancellable / label tests green, unedited.
- Replace the two drift-guards with one coverage/count assertion over `meta()`.

### Phase 2 test set (write first, as end-to-end characterization over `update()`)

Captured against *today's* Recipe-based behaviour, must stay green after `resolve()` replaces it:

- Run iOS, `{yarn stale, pods stale}` → spawns `[YarnInstall, YarnPodInstall, UmpRunIos]`, in order.
- Run iOS, fresh → `[UmpRunIos]`.
- Run Android, `{yarn stale, pods stale}` → `[YarnInstall, UmpRunAndroid]` (no pods).
- Run iOS, metro **not** running → metro auto-started and the run **held until metro-ready**,
  then dispatched (assert the deferral + ready→dispatch).
- Run iOS, metro running, fresh → `[UmpRunIos]` directly.
- Shared-dep dedup → a goal reaching `YarnInstall` via two paths includes it **once**.
- `RnReleaseBuild` → `[RnReleaseBuild, AdbInstallApk]`; `GitResetHardFetch` →
  `[GitFetch, GitResetHard]`.
- Collision (regression): dispatching a duplicate of a running command →
  `BlockNew` drops / `CancelPrevious` aborts+replaces, unchanged.
- `is_satisfied` matrix: per-command, across `{stale_yarn, stale_pods, metro_running}`.
- Pure `resolve()` unit tests mirroring the above with explicit `ctx`.

### Sequencing of tests vs. code

1. Write the Phase-1 metadata matrix → refactor to `meta()` → matrix stays green.
2. Write the Phase-2 end-to-end resolution tests against current code → confirm green on the
   *old* implementation → build `resolve()`/graph → swap → tests stay green.
3. Metro bridge last, behind its characterization tests.

---

## 8. Rollout

- **Phase 1** (info-card) ships independently. No behaviour change; near-zero test churn.
- **Phase 2** (dep graph + `EnsureMetro` + resolver) ships after, gated by the
  characterization suite. Metro bridge implemented last within Phase 2.
- Each phase is a normal atomic-commit GSD-style unit; Phase 1 can merge before Phase 2 starts.

---

## 9. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| Card migration silently changes a metadata value | `command_metadata_matrix` golden test written from current values, first |
| Resolver produces a different/wrong sequence | End-to-end `update()` characterization tests written against current behaviour, first |
| Metro daemon bridge breaks "defer until ready" | Implement last; metro characterization tests; behaviour must match today exactly |
| Dependency cycle | Graph is static & curated; resolver asserts acyclic (debug) — add a cycle guard test |
| Scope creep into device/variant modal flow | Explicitly out of scope; do not touch in this work |

---

## 10. Open Questions

None blocking. Resolved during design:

- Trait-objects vs info-card → **info-card** (keep enum/value semantics).
- Who owns dependencies → **the card** (`deps: &'static [CommandSpec]`).
- Metro daemon → **`EnsureMetro` node**, satisfied = running, auto-start, complete = ready.

## 11. Future follow-ups (not this work)

- Refactor the run-app device/variant modal partial-application (`command_with_device`,
  `command_with_run_variant`, cache helpers) into a cohesive type.
