# CommandSpec Dependency Resolver (Phase 2a of 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make commands self-describe their prerequisites (`deps` on the info-card) and introduce a pure `resolve()` that flattens a goal + its unsatisfied prerequisites into the same ordered `Vec<CommandSpec>` the hand-written `Recipe::SyncThenRun` produces today — then swap the 4 `SyncThenRun` call sites to `resolve()`.

**Architecture:** Keep the enum and value semantics (Phase 1's `CommandMeta` card already exists). Add a static `deps: &'static [CommandSpec]` field to `CommandMeta`, a runtime `is_satisfied(&self, ctx: &DependencyState) -> bool` method, and a pure `resolve(goal, ctx) -> Vec<CommandSpec>` in `pipeline.rs`. Migrate only the `SyncThenRun` sites. Metro (`needs_metro`, the async park/release-on-Ready gate), the fixed pipelines (`ReleaseBuildAndInstall`, `GitFetchThenReset`, `Clean`), and `SyncThenStartMetro` are all OUT of scope.

**Tech Stack:** Rust, domain layer (`src/domain/command.rs`, `src/domain/pipeline.rs`) + 4 call-site edits in `src/app/update.rs`.

**Spec:** `docs/superpowers/specs/2026-06-26-command-info-card-and-dependency-graph-design.md` (§5). This is the de-risked Phase 2a slice; metro-as-node (`EnsureMetro`, removing `needs_metro`) is explicitly deferred — it collides with the async-daemon task model.

## Global Constraints

- **Behavior-preserving.** `resolve(goal, ctx)` MUST equal `Recipe::SyncThenRun(goal).expand(ctx)` for `goal ∈ {UmpRunIos, UmpRunAndroid, RnReleaseBuild}`. The Phase-1 `command_metadata_matrix` golden test and the full existing `dispatch_tests.rs` suite must stay green and **unedited** (except where a task explicitly adds tests). A forced edit to an existing behavioral test = a behavior change → STOP and confirm.
- Type-check: `CARGO_INCREMENTAL=1 cargo check`; tests: `CARGO_INCREMENTAL=1 cargo test` (raw cargo has NO `--incremental` flag — set the env var).
- OUT OF SCOPE — do NOT touch: `needs_metro()` and any metro action/flow; `Recipe::SyncThenStartMetro`, `Recipe::Clean`, `Recipe::ReleaseBuildAndInstall`, `Recipe::GitFetchThenReset` and their call sites; the device/variant modal flow; `to_argv()`; the UI layer.
- Every commit ends with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

## The dependency graph (authoritative)

| Command | `deps` | `is_satisfied(ctx)` |
|---|---|---|
| `YarnInstall` | `&[]` | `!ctx.stale_yarn` |
| `YarnPodInstall` | `&[YarnInstall]` | `!ctx.stale_pods` |
| `UmpRunIos{..}` | `&[YarnPodInstall]` | `false` (goal, always runs) |
| `UmpRunAndroid{..}` | `&[YarnInstall]` | `false` |
| `RnReleaseBuild` | `&[YarnInstall]` | `false` |
| every other variant | `&[]` | `false` |

Why this reproduces `SyncThenRun.expand`: iOS reaches yarn *through* pods (`UmpRunIos → YarnPodInstall → YarnInstall`); Android/release reach yarn directly. The "pods only on iOS" rule is now structural (only `UmpRunIos` deps on pods), so `resolve()` never consults `is_ios_target`. `resolve(UmpRunIos, ctx) = [YarnInstall?, YarnPodInstall?, UmpRunIos]`; `resolve(UmpRunAndroid, ctx) = [YarnInstall?, UmpRunAndroid]`; `resolve(RnReleaseBuild, ctx) = [YarnInstall?, RnReleaseBuild]` — identical to today.

---

## File Structure

- `src/domain/command.rs` — add `deps` field to `CommandMeta`; populate the 19 `meta()` rows; add a `deps` unit test.
- `src/domain/pipeline.rs` — add `CommandSpec::is_satisfied()` + `resolve()` + their golden tests.
- `src/app/update.rs` — swap 4 `Recipe::SyncThenRun(X).expand(&deps)` → `resolve(X, &deps)`; add the `resolve` import.

---

## Task 1: Add `deps` to the info-card

**Files:**
- Modify: `src/domain/command.rs` (`CommandMeta` struct + 19 `meta()` rows + one test)

**Interfaces:**
- Produces: `CommandMeta.deps: &'static [CommandSpec]`. Existing `meta()` callers unaffected (additive field).

- [ ] **Step 1: Write the failing deps test**

Add to `#[cfg(test)] mod tests` in `command.rs`:

```rust
    #[test]
    fn command_deps_graph() {
        assert_eq!(CommandSpec::YarnInstall.meta().deps, &[]);
        assert_eq!(CommandSpec::YarnPodInstall.meta().deps, &[CommandSpec::YarnInstall]);
        assert_eq!(
            CommandSpec::UmpRunIos { device_id: String::new(), variant: None }.meta().deps,
            &[CommandSpec::YarnPodInstall]
        );
        assert_eq!(
            CommandSpec::UmpRunAndroid { device_id: String::new(), variant: None }.meta().deps,
            &[CommandSpec::YarnInstall]
        );
        assert_eq!(CommandSpec::RnReleaseBuild.meta().deps, &[CommandSpec::YarnInstall]);
        // A representative no-deps command:
        assert_eq!(CommandSpec::GitPull.meta().deps, &[]);
    }
```

- [ ] **Step 2: Run it — expect FAIL (no `deps` field yet)**

Run: `CARGO_INCREMENTAL=1 cargo test command_deps_graph`
Expected: compile error — `no field 'deps' on type 'CommandMeta'`.

- [ ] **Step 3: Add the `deps` field and populate the rows**

In `command.rs`, add the field to `CommandMeta` (after `collision`):

```rust
    /// What to do when a duplicate is dispatched while one is running.
    pub collision: CollisionPolicy,
    /// Commands that must be satisfied before this one (the dependency graph).
    pub deps: &'static [CommandSpec],
```

Then add `deps: …` to every `meta()` arm. For the four with real deps:
- `UmpRunIos { .. }` → `deps: &[CommandSpec::YarnPodInstall]`
- `UmpRunAndroid { .. }` → `deps: &[CommandSpec::YarnInstall]`
- `RnReleaseBuild` → `deps: &[CommandSpec::YarnInstall]`
- `YarnPodInstall` → `deps: &[CommandSpec::YarnInstall]`

Every other arm (15 of them) gets `deps: &[]`. Example (GitPull row becomes):

```rust
            CommandSpec::GitPull => CommandMeta { label: "git pull", destructive: false, cancellable: false, refresh: none, collision: BlockNew, deps: &[] },
```

- [ ] **Step 4: Run tests — expect PASS**

Run: `CARGO_INCREMENTAL=1 cargo test command_deps_graph` then `CARGO_INCREMENTAL=1 cargo test` — all pass (incl. the unedited `command_metadata_matrix`).

- [ ] **Step 5: Commit**

```bash
git add src/domain/command.rs
git commit -m "feat(command): declare prerequisite deps on the info-card" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: Add `is_satisfied()` + `resolve()`

**Files:**
- Modify: `src/domain/pipeline.rs` (add a method on `CommandSpec`, the `resolve` fn, and tests)

**Interfaces:**
- Consumes: `CommandSpec::meta().deps` (Task 1), `DependencyState` (existing, in this file).
- Produces: `CommandSpec::is_satisfied(&self, ctx: &DependencyState) -> bool`; `pub fn resolve(goal: CommandSpec, ctx: &DependencyState) -> Vec<CommandSpec>`.

- [ ] **Step 1: Write the failing tests**

Add to `#[cfg(test)] mod tests` in `pipeline.rs` (reuse the existing `fresh_deps`, `stale_deps_ios`, `stale_deps_android` helpers already defined there):

```rust
    #[test]
    fn resolve_ios_stale_matches_sync_then_run() {
        let ios = CommandSpec::UmpRunIos { device_id: "d".into(), variant: Some(RunVariant::Local) };
        assert_eq!(
            resolve(ios.clone(), &stale_deps_ios()),
            vec![CommandSpec::YarnInstall, CommandSpec::YarnPodInstall, ios]
        );
    }

    #[test]
    fn resolve_android_stale_skips_pods() {
        let android = CommandSpec::UmpRunAndroid { device_id: "e".into(), variant: Some(RunVariant::Local) };
        assert_eq!(
            resolve(android.clone(), &stale_deps_android()),
            vec![CommandSpec::YarnInstall, android]
        );
    }

    #[test]
    fn resolve_release_build_stale_adds_yarn() {
        assert_eq!(
            resolve(CommandSpec::RnReleaseBuild, &stale_deps_android()),
            vec![CommandSpec::YarnInstall, CommandSpec::RnReleaseBuild]
        );
    }

    #[test]
    fn resolve_fresh_is_goal_only() {
        let ios = CommandSpec::UmpRunIos { device_id: "d".into(), variant: Some(RunVariant::Local) };
        assert_eq!(resolve(ios.clone(), &fresh_deps()), vec![ios]);
    }

    #[test]
    fn resolve_ios_yarn_stale_pods_fresh_adds_only_yarn() {
        let ctx = DependencyState::new(true, false, true);
        let ios = CommandSpec::UmpRunIos { device_id: "d".into(), variant: Some(RunVariant::Local) };
        assert_eq!(resolve(ios.clone(), &ctx), vec![CommandSpec::YarnInstall, ios]);
    }

    #[test]
    fn resolve_equivalent_to_sync_then_run_across_combos() {
        let ios = CommandSpec::UmpRunIos { device_id: "d".into(), variant: Some(RunVariant::Local) };
        for (y, p) in [(false, false), (true, false), (false, true), (true, true)] {
            let ctx = DependencyState::new(y, p, true); // is_ios_target=true matches old SyncThenRun(ios) call sites
            assert_eq!(
                resolve(ios.clone(), &ctx),
                Recipe::SyncThenRun(ios.clone()).expand(&ctx),
                "ios mismatch at stale_yarn={y} stale_pods={p}"
            );
        }
        let android = CommandSpec::UmpRunAndroid { device_id: "e".into(), variant: Some(RunVariant::Local) };
        for (y, p) in [(false, false), (true, false), (false, true), (true, true)] {
            let ctx = DependencyState::new(y, p, false); // android call sites pass is_ios_target=false
            assert_eq!(
                resolve(android.clone(), &ctx),
                Recipe::SyncThenRun(android.clone()).expand(&ctx),
                "android mismatch at stale_yarn={y} stale_pods={p}"
            );
        }
    }
```

- [ ] **Step 2: Run — expect FAIL**

Run: `CARGO_INCREMENTAL=1 cargo test resolve_`
Expected: compile error — `resolve` / `is_satisfied` not found.

- [ ] **Step 3: Implement `is_satisfied()` and `resolve()`**

Add to `pipeline.rs` (after the existing `impl CommandSpec { fn prerequisites … }` block, or in a new `impl` block):

```rust
impl CommandSpec {
    /// True when this command is already satisfied for the given worktree state,
    /// so it can be skipped when it appears as a prerequisite. Goals (run/build)
    /// are never "satisfied" — they always run.
    pub fn is_satisfied(&self, ctx: &DependencyState) -> bool {
        match self {
            CommandSpec::YarnInstall => !ctx.stale_yarn,
            CommandSpec::YarnPodInstall => !ctx.stale_pods,
            _ => false,
        }
    }
}

/// Flatten `goal` and its unsatisfied transitive prerequisites into the order
/// they must run: dependencies first (post-order), de-duplicated, with the goal
/// last. Pure — no I/O. Replaces `Recipe::SyncThenRun` for run/build commands.
pub fn resolve(goal: CommandSpec, ctx: &DependencyState) -> Vec<CommandSpec> {
    fn collect(cmd: &CommandSpec, ctx: &DependencyState, out: &mut Vec<CommandSpec>) {
        for dep in cmd.meta().deps {
            collect(dep, ctx, out);
            if !dep.is_satisfied(ctx) && !out.contains(dep) {
                out.push(dep.clone());
            }
        }
    }
    let mut out = Vec::new();
    collect(&goal, ctx, &mut out);
    out.push(goal);
    out
}
```

- [ ] **Step 4: Run — expect PASS**

Run: `CARGO_INCREMENTAL=1 cargo test resolve_` then `CARGO_INCREMENTAL=1 cargo test` — all pass.

- [ ] **Step 5: Commit**

```bash
git add src/domain/pipeline.rs
git commit -m "feat(pipeline): pure resolve() flattens command deps (matches SyncThenRun)" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: Migrate the 4 `SyncThenRun` call sites to `resolve()`

The existing `dispatch_tests.rs` suite is the end-to-end regression net for this task — it must stay green and unedited. The transformation is uniform and behavior-identical (`resolve` returns the same `Vec`, consumed the same dispatch-first/queue-rest way).

**Files:**
- Modify: `src/app/update.rs` (4 lines + 1 import)

**Interfaces:**
- Consumes: `resolve` (Task 2).

- [ ] **Step 1: Add the import**

In `src/app/update.rs`, the existing import `use crate::domain::pipeline::{DependencyState, Recipe};` becomes:

```rust
use crate::domain::pipeline::{resolve, DependencyState, Recipe};
```

(`Recipe` stays — `SyncThenStartMetro`/`Clean`/`ReleaseBuildAndInstall`/`GitFetchThenReset` still use it.)

- [ ] **Step 2: Replace the 4 call sites**

Apply this exact substitution at each site (the surrounding dispatch/queue code stays unchanged):

| Line (approx) | Old | New |
|---|---|---|
| ~943 | `Recipe::SyncThenRun(CommandSpec::RnReleaseBuild).expand(&deps)` | `resolve(CommandSpec::RnReleaseBuild, &deps)` |
| ~1265 | `Recipe::SyncThenRun(spec).expand(&deps)` | `resolve(spec, &deps)` |
| ~1975 | `Recipe::SyncThenRun(spec).expand(&deps)` | `resolve(spec, &deps)` |
| ~3478 | `Recipe::SyncThenRun(*run_command).expand(&deps)` | `resolve(*run_command, &deps)` |

Use `rg -n "Recipe::SyncThenRun" src/app/update.rs` to confirm exactly these 4 code sites (ignore the comment lines at 44/558/1971/3470). After editing, that command should return only comment lines.

- [ ] **Step 3: Build and run the FULL suite — expect PASS, nothing edited**

Run: `CARGO_INCREMENTAL=1 cargo check` then `CARGO_INCREMENTAL=1 cargo test`
Expected: all green. In particular these existing characterization tests must pass unedited (they exercise the migrated paths): `run_android_without_metro_auto_starts_metro`, the `command_exited_with_nonempty_queue_pops_and_dispatches_front` drain test, and the sync-before-run tests. If any existing test fails, `resolve()` diverged from `SyncThenRun` — fix `resolve()`/deps (Task 1/2), NOT the test.

- [ ] **Step 4: Commit**

```bash
git add src/app/update.rs
git commit -m "refactor(update): dispatch run/build prereqs via resolve() instead of SyncThenRun" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: Remove the now-dead `Recipe::SyncThenRun`

After Task 3, `Recipe::SyncThenRun` has no production callers (verify). Remove it and its tests so the enum reflects reality.

**Files:**
- Modify: `src/domain/pipeline.rs` (remove the variant, its `expand` arm, and its 3 unit tests)

- [ ] **Step 1: Confirm no production callers remain**

Run: `rg -n "SyncThenRun" src/ ':!src/domain/pipeline.rs'`
Expected: only comment lines in `update.rs` (no `Recipe::SyncThenRun(...).expand` code). If a real call site remains, STOP — Task 3 was incomplete.

- [ ] **Step 2: Remove the variant, its expand arm, and its tests**

In `pipeline.rs`:
- Delete the `SyncThenRun(CommandSpec)` variant from the `Recipe` enum (and its doc-comment line).
- Delete its arm in `Recipe::expand` (the `Recipe::SyncThenRun(cmd) => { … }` block that front-loads yarn/pods).
- Delete the three tests `test_sync_then_run_stale_ios_adds_yarn_and_pods`, `test_sync_then_run_stale_android_only_yarn`, `test_sync_then_run_fresh_passes_through` (their behavior is now covered by the `resolve_*` tests). Also remove the stale comment-line references to `SyncThenRun` in `update.rs:44` and `update.rs:558` if they name it as a live recipe.

- [ ] **Step 3: Build and run the full suite — expect PASS**

Run: `CARGO_INCREMENTAL=1 cargo check` (clean, no unused-import/variant warnings) then `CARGO_INCREMENTAL=1 cargo test` — all green.

- [ ] **Step 4: Commit**

```bash
git add src/domain/pipeline.rs src/app/update.rs
git commit -m "refactor(pipeline): drop dead Recipe::SyncThenRun, superseded by resolve()" -m "Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**1. Spec coverage (§5):** `deps` on the card → Task 1. `is_satisfied` → Task 2. pure `resolve()` replacing `SyncThenRun` → Tasks 2–3. Dead-recipe cleanup → Task 4. Metro / `EnsureMetro` / removing `needs_metro` / fixed pipelines → explicitly deferred (Global Constraints), per the Phase-2a scoping decision. ✓

**2. Placeholder scan:** All new code (deps rows, `is_satisfied`, `resolve`, every test) is shown in full. The migration is an exact 4-row substitution table, not a "similar to" reference. ✓

**3. Type consistency:** `CommandMeta.deps: &'static [CommandSpec]` (Task 1) is read by `resolve()` via `cmd.meta().deps` (Task 2). `resolve(goal: CommandSpec, ctx: &DependencyState) -> Vec<CommandSpec>` is defined in Task 2 and called identically in Task 3. `is_satisfied(&self, ctx: &DependencyState) -> bool` consistent. ✓

**Behavior-preservation net:** Task 2's `resolve_equivalent_to_sync_then_run_across_combos` proves `resolve == SyncThenRun.expand` before migration; the full `dispatch_tests.rs` suite (unedited) guards the call-site swap; the Phase-1 `command_metadata_matrix` stays green. ✓

---

## Out of scope / next

Metro-as-a-node (`EnsureMetro`, removing `needs_metro()`, folding the metro gate into `resolve()`) is deferred — it requires reworking the daemon/queue completion model (metro completes on an async `MetroActivity::Ready`, not process exit). Revisit as its own effort if ever desired.
