# CommandSpec Info-Card (Phase 1 of 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Consolidate every command's *static* facts (label, destructive, cancellable, refresh, collision) into one `CommandMeta` info-card returned by `CommandSpec::meta()`, with the existing accessor methods becoming thin readers — a behavior-preserving refactor.

**Architecture:** Keep the `CommandSpec` enum and its value semantics. Introduce a passive `CommandMeta` data struct + one exhaustive `meta()` match (the new single drift-guard). Rewrite `label()`, `is_destructive()`, `is_cancellable()`, `collision_policy()` to read from `meta()`, and route `refresh_needed()` through `meta().refresh`. No behavior changes; a golden characterization test locks the current 19-command matrix first.

**Tech Stack:** Rust, domain layer (`src/domain/`). Tests are inline `#[cfg(test)]` modules. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-06-26-command-info-card-and-dependency-graph-design.md` (§4 Phase 1, §7 Testing).

## Global Constraints

- This is a **behavior-preserving refactor**. The `command_metadata_matrix` golden test (Task 1) MUST stay green and **unedited** through Tasks 2–4. If any task forces an edit to it, that is a behavior change — STOP and confirm.
- Keep `CommandSpec`'s `#[derive(Debug, Clone, PartialEq)]`, its value semantics, and `std::mem::discriminant`-based collision detection. Do NOT change them.
- Type-check with `CARGO_INCREMENTAL=1 cargo check`; run tests with `CARGO_INCREMENTAL=1 cargo test` (project convention is "check-types always uses --incremental"; raw cargo has no `--incremental` flag, so set the env var).
- Do NOT touch out-of-scope code: `to_argv()`, `needs_metro()`, `needs_text_input()`, `needs_device_selection()`, `needs_run_variant_selection()`, the UI `task_short_label()` (stays in `ui/indicators.rs`), `pipeline.rs`, or `update.rs`. Those belong to Phase 2 or are deliberately out of scope.
- Every commit must end with the trailer: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

---

## File Structure

- `src/domain/command.rs` — add `CommandMeta` struct + `meta()`; rewire `label`/`is_destructive`/`is_cancellable`/`collision_policy` as readers; add the golden matrix test; remove the redundant collision drift-guard.
- `src/domain/refresh.rs` — add `Copy` to `RefreshSet`; route `refresh_needed()` through `meta().refresh`.

No new files. No changes outside the domain layer.

---

## The authoritative 19-command matrix (current values)

This is the source of truth for the `meta()` rows and the golden test. Refresh sets: `full` = `{worktrees:true, staleness:true, jira_titles:true}`, `stale` = `{worktrees:false, staleness:true, jira_titles:false}`, `none` = `RefreshSet::none()`.

| Command | label | destructive | cancellable | collision | refresh |
|---|---|---|---|---|---|
| `GitResetHard` | `git reset --hard HEAD` | true | false | BlockNew | full |
| `GitPull` | `git pull` | false | false | BlockNew | none |
| `GitPush` | `git push` | false | false | BlockNew | none |
| `GitFetch` | `git fetch --all --tags` | false | false | BlockNew | none |
| `GitResetHardFetch` | `git fetch + reset --hard origin/<branch>` | true | false | BlockNew | full |
| `RnCleanAndroid` | `Clean Android (react-native clean)` | true | true | CancelPrevious | stale |
| `RnCleanCocoapods` | `Clean CocoaPods (react-native clean)` | true | true | CancelPrevious | stale |
| `RmNodeModules` | `Remove node_modules` | true | true | CancelPrevious | stale |
| `YarnInstall` | `yarn install` | false | true | BlockNew | stale |
| `YarnPodInstall` | `yarn pod-install` | false | true | BlockNew | stale |
| `YarnUnitTests` | `yarn unit-tests` | false | true | CancelPrevious | none |
| `YarnJest{..}` | `yarn jest <filter>` | false | true | CancelPrevious | none |
| `YarnLint` | `yarn lint --quiet --fix` | false | true | CancelPrevious | none |
| `YarnCheckTypes` | `yarn check-types --incremental` | false | true | CancelPrevious | none |
| `UmpRunAndroid{..}` | `Run Android (UMP)` | false | true | CancelPrevious | none |
| `UmpRunIos{..}` | `Run iOS (UMP)` | false | true | CancelPrevious | none |
| `RnReleaseBuild` | `gradlew assembleRelease` | false | true | CancelPrevious | none |
| `AdbInstallApk` | `adb install release APK` | false | true | CancelPrevious | none |
| `ShellCommand{..}` | `shell command` | false | true | CancelPrevious | none |

---

## Task 1: Golden metadata characterization test (the safety net)

Lock current behavior via the *existing* public methods, before introducing `meta()`. This test passes immediately on current code and must stay green through every later task.

**Files:**
- Modify: `src/domain/command.rs` (add one test to the existing `#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes (existing): `CommandSpec::{label, is_destructive, is_cancellable, collision_policy}`, `crate::domain::refresh::refresh_needed`, `RefreshSet`, `CollisionPolicy`.
- Produces: the `command_metadata_matrix` regression net used by all later tasks.

- [ ] **Step 1: Write the golden matrix test**

Add to the `#[cfg(test)] mod tests` block in `src/domain/command.rs`:

```rust
    #[test]
    fn command_metadata_matrix() {
        use crate::domain::refresh::{refresh_needed, RefreshSet};
        use CollisionPolicy::{BlockNew, CancelPrevious};

        let full = RefreshSet { worktrees: true, staleness: true, jira_titles: true };
        let stale = RefreshSet { worktrees: false, staleness: true, jira_titles: false };
        let none = RefreshSet::none();

        // (spec, label, destructive, cancellable, collision, refresh)
        let cases: Vec<(CommandSpec, &str, bool, bool, CollisionPolicy, RefreshSet)> = vec![
            (CommandSpec::GitResetHard, "git reset --hard HEAD", true, false, BlockNew, full.clone()),
            (CommandSpec::GitPull, "git pull", false, false, BlockNew, none.clone()),
            (CommandSpec::GitPush, "git push", false, false, BlockNew, none.clone()),
            (CommandSpec::GitFetch, "git fetch --all --tags", false, false, BlockNew, none.clone()),
            (CommandSpec::GitResetHardFetch, "git fetch + reset --hard origin/<branch>", true, false, BlockNew, full.clone()),
            (CommandSpec::RnCleanAndroid, "Clean Android (react-native clean)", true, true, CancelPrevious, stale.clone()),
            (CommandSpec::RnCleanCocoapods, "Clean CocoaPods (react-native clean)", true, true, CancelPrevious, stale.clone()),
            (CommandSpec::RmNodeModules, "Remove node_modules", true, true, CancelPrevious, stale.clone()),
            (CommandSpec::YarnInstall, "yarn install", false, true, BlockNew, stale.clone()),
            (CommandSpec::YarnPodInstall, "yarn pod-install", false, true, BlockNew, stale.clone()),
            (CommandSpec::YarnUnitTests, "yarn unit-tests", false, true, CancelPrevious, none.clone()),
            (CommandSpec::YarnJest { filter: String::new() }, "yarn jest <filter>", false, true, CancelPrevious, none.clone()),
            (CommandSpec::YarnLint, "yarn lint --quiet --fix", false, true, CancelPrevious, none.clone()),
            (CommandSpec::YarnCheckTypes, "yarn check-types --incremental", false, true, CancelPrevious, none.clone()),
            (CommandSpec::UmpRunAndroid { device_id: String::new(), variant: None }, "Run Android (UMP)", false, true, CancelPrevious, none.clone()),
            (CommandSpec::UmpRunIos { device_id: String::new(), variant: None }, "Run iOS (UMP)", false, true, CancelPrevious, none.clone()),
            (CommandSpec::RnReleaseBuild, "gradlew assembleRelease", false, true, CancelPrevious, none.clone()),
            (CommandSpec::AdbInstallApk, "adb install release APK", false, true, CancelPrevious, none.clone()),
            (CommandSpec::ShellCommand { command: String::new() }, "shell command", false, true, CancelPrevious, none.clone()),
        ];

        assert_eq!(cases.len(), 19, "matrix must cover all 19 CommandSpec variants");

        for (spec, label, destructive, cancellable, collision, refresh) in &cases {
            assert_eq!(spec.label(), *label, "label mismatch for {spec:?}");
            assert_eq!(spec.is_destructive(), *destructive, "is_destructive mismatch for {spec:?}");
            assert_eq!(spec.is_cancellable(), *cancellable, "is_cancellable mismatch for {spec:?}");
            assert_eq!(spec.collision_policy(), *collision, "collision_policy mismatch for {spec:?}");
            assert_eq!(refresh_needed(spec), *refresh, "refresh mismatch for {spec:?}");
        }
    }
```

- [ ] **Step 2: Run the test — expect PASS (characterization, not red-green)**

Run: `CARGO_INCREMENTAL=1 cargo test command_metadata_matrix`
Expected: `test result: ok. 1 passed`. This test documents *current* behavior and is the regression net; it is green from the start and must stay green & unedited through Tasks 2–4.

- [ ] **Step 3: Commit**

```bash
git add src/domain/command.rs
git commit -m "test(command): golden metadata matrix as refactor safety net

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: Introduce `CommandMeta` + `meta()`, rewire the four readers

**Files:**
- Modify: `src/domain/refresh.rs:10` (add `Copy` to `RefreshSet` derive)
- Modify: `src/domain/command.rs` (add `CommandMeta` + `meta()`; rewrite `label`/`is_destructive`/`is_cancellable`/`collision_policy` bodies)

**Interfaces:**
- Consumes: `RefreshSet`, `CollisionPolicy`.
- Produces: `CommandSpec::meta(&self) -> CommandMeta`; `CommandMeta { label: &'static str, destructive: bool, cancellable: bool, refresh: RefreshSet, collision: CollisionPolicy }`. Method signatures of `label`/`is_destructive`/`is_cancellable`/`collision_policy` are unchanged.

- [ ] **Step 1: Make `RefreshSet` `Copy`**

In `src/domain/refresh.rs`, change the derive on `RefreshSet` (line 10) from:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshSet {
```

to:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefreshSet {
```

(Adding `Copy` to a 3-`bool` struct is additive and safe.)

- [ ] **Step 2: Add the `CommandMeta` struct and `meta()` to `command.rs`**

Add `use super::refresh::RefreshSet;` near the top of `src/domain/command.rs` (alongside the existing imports). Then add, inside `impl CommandSpec` (place it just above `label()`), the struct (above the `impl`) and method:

```rust
/// All of a command's *static* facts in one place — the single source of truth.
/// Behaviour that depends on field values (argv, needs_text_input) or on runtime
/// state stays as separate methods; this card holds only discriminant-pure facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMeta {
    /// Human-readable label for palette and confirmation dialogs.
    pub label: &'static str,
    /// Cannot be undone — dispatch shows a confirm prompt.
    pub destructive: bool,
    /// May be aborted mid-run (false for git plumbing — data-integrity).
    pub cancellable: bool,
    /// Background refreshes to run after this command completes.
    pub refresh: RefreshSet,
    /// What to do when a duplicate is dispatched while one is running.
    pub collision: CollisionPolicy,
}
```

```rust
    /// The single source of truth for this command's static facts. The
    /// exhaustive match (no `_` arm) is the compile-time drift guard — adding a
    /// `CommandSpec` variant fails to compile here until its card is filled in.
    pub fn meta(&self) -> CommandMeta {
        let full = RefreshSet { worktrees: true, staleness: true, jira_titles: true };
        let stale = RefreshSet { worktrees: false, staleness: true, jira_titles: false };
        let none = RefreshSet::none();
        use CollisionPolicy::{BlockNew, CancelPrevious};
        match self {
            CommandSpec::GitResetHard => CommandMeta { label: "git reset --hard HEAD", destructive: true, cancellable: false, refresh: full, collision: BlockNew },
            CommandSpec::GitPull => CommandMeta { label: "git pull", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
            CommandSpec::GitPush => CommandMeta { label: "git push", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
            CommandSpec::GitFetch => CommandMeta { label: "git fetch --all --tags", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
            CommandSpec::GitResetHardFetch => CommandMeta { label: "git fetch + reset --hard origin/<branch>", destructive: true, cancellable: false, refresh: full, collision: BlockNew },
            CommandSpec::RnCleanAndroid => CommandMeta { label: "Clean Android (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
            CommandSpec::RnCleanCocoapods => CommandMeta { label: "Clean CocoaPods (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
            CommandSpec::RmNodeModules => CommandMeta { label: "Remove node_modules", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
            CommandSpec::YarnInstall => CommandMeta { label: "yarn install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew },
            CommandSpec::YarnPodInstall => CommandMeta { label: "yarn pod-install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew },
            CommandSpec::YarnUnitTests => CommandMeta { label: "yarn unit-tests", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::YarnJest { .. } => CommandMeta { label: "yarn jest <filter>", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::YarnLint => CommandMeta { label: "yarn lint --quiet --fix", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::YarnCheckTypes => CommandMeta { label: "yarn check-types --incremental", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::UmpRunAndroid { .. } => CommandMeta { label: "Run Android (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::UmpRunIos { .. } => CommandMeta { label: "Run iOS (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::RnReleaseBuild => CommandMeta { label: "gradlew assembleRelease", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::AdbInstallApk => CommandMeta { label: "adb install release APK", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::ShellCommand { .. } => CommandMeta { label: "shell command", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
        }
    }
```

- [ ] **Step 3: Rewrite the four readers to delegate to `meta()`**

Replace the bodies of these four methods in `src/domain/command.rs` (keep their signatures and doc-comments):

```rust
    pub fn is_destructive(&self) -> bool {
        self.meta().destructive
    }
```

```rust
    pub fn is_cancellable(&self) -> bool {
        self.meta().cancellable
    }
```

```rust
    pub fn collision_policy(&self) -> CollisionPolicy {
        self.meta().collision
    }
```

```rust
    pub fn label(&self) -> &'static str {
        self.meta().label
    }
```

- [ ] **Step 4: Build and run the full suite — expect PASS**

Run: `CARGO_INCREMENTAL=1 cargo test`
Expected: all tests pass, including `command_metadata_matrix` (unedited), `collision_policy_*`, `is_cancellable_*`, and the existing `refresh.rs` tests. Behavior is unchanged.

- [ ] **Step 5: Commit**

```bash
git add src/domain/command.rs src/domain/refresh.rs
git commit -m "refactor(command): introduce CommandMeta info-card, readers delegate to meta()

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: Route `refresh_needed()` through `meta().refresh`

**Files:**
- Modify: `src/domain/refresh.rs:42-66` (`refresh_needed` body)

**Interfaces:**
- Consumes: `CommandSpec::meta()` (from Task 2).
- Produces: `refresh_needed(cmd)` returns `cmd.meta().refresh`; signature unchanged.

- [ ] **Step 1: Replace the `refresh_needed` match with a card read**

In `src/domain/refresh.rs`, replace the entire body of `refresh_needed` (the `match cmd { ... }` block, lines 43-65) with:

```rust
pub fn refresh_needed(cmd: &CommandSpec) -> RefreshSet {
    cmd.meta().refresh
}
```

Leave the doc-comment above it. The `RefreshSet` import and the existing `#[cfg(test)] mod tests` stay unchanged.

- [ ] **Step 2: Run refresh + matrix tests — expect PASS**

Run: `CARGO_INCREMENTAL=1 cargo test refresh_needed`
Then: `CARGO_INCREMENTAL=1 cargo test command_metadata_matrix`
Expected: both pass. The existing per-command refresh tests in `refresh.rs` still hold because `meta().refresh` returns the identical values.

- [ ] **Step 3: Run the full suite — expect PASS**

Run: `CARGO_INCREMENTAL=1 cargo test`
Expected: all green.

- [ ] **Step 4: Commit**

```bash
git add src/domain/refresh.rs
git commit -m "refactor(refresh): refresh_needed reads from CommandMeta

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: Retire the redundant collision drift-guard

`meta()`'s exhaustive match plus the `command_metadata_matrix` golden test now provide both compile-time and value-level drift protection. The old `collision_policy_covers_every_variant` meta-test is redundant. (Leave the UI-side `task_short_label_covers_every_variant` in `ui/indicators.rs` alone — it guards a separate match that stays in the UI layer.)

**Files:**
- Modify: `src/domain/command.rs` (delete the `collision_policy_covers_every_variant` test)

**Interfaces:**
- Consumes: nothing new.
- Produces: nothing; this is a test deletion.

- [ ] **Step 1: Delete the redundant drift-guard test**

In `src/domain/command.rs`, delete the entire `#[test] fn collision_policy_covers_every_variant() { ... }` function (it constructs all 19 variants, mirrors the policy match, and asserts `variants.len() == 19`).

- [ ] **Step 2: Run the full suite — expect PASS**

Run: `CARGO_INCREMENTAL=1 cargo test`
Expected: all green, with one fewer test. `command_metadata_matrix` (which enumerates all 19 with their collision values) plus `meta()`'s exhaustive match preserve the drift guarantee.

- [ ] **Step 3: Confirm `meta()` is the live drift guard (no code change — verification only)**

Reasoning check to record in the commit: adding a hypothetical new `CommandSpec` variant would fail to compile in `meta()` (exhaustive, no `_` arm) and fail `command_metadata_matrix` (count assertion `== 19`). Drift protection is intact.

- [ ] **Step 4: Commit**

```bash
git add src/domain/command.rs
git commit -m "test(command): drop drift-guard subsumed by meta() exhaustiveness + matrix

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

**1. Spec coverage (§4 Phase 1):**
- "One `CommandMeta` returned by `meta()`" → Task 2. ✓
- "Existing accessors become thin readers; signatures unchanged" → Task 2 Step 3. ✓
- "`refresh_needed` becomes `cmd.meta().refresh`" → Task 3. ✓
- "Single exhaustive `meta()` match is the drift guard; two drift-guards collapse to one coverage assertion" → Task 4 removes the command-side guard; `command_metadata_matrix` (Task 1) is the coverage assertion; the UI `task_short_label_covers_every_variant` is explicitly left in place (separate concern, §6). ✓
- "`needs_metro()` left untouched in Phase 1" → not in any task; Global Constraints forbid touching it. ✓
- "`deps` not introduced until Phase 2" → not in any task. ✓
- §7 "characterization test written first, from current values, stays green unedited" → Task 1 + Global Constraints. ✓

**2. Placeholder scan:** No TBD/TODO; every code step shows complete code; every run step gives an exact command and expected result. ✓

**3. Type consistency:** `CommandMeta` fields (`label: &'static str`, `destructive: bool`, `cancellable: bool`, `refresh: RefreshSet`, `collision: CollisionPolicy`) are defined in Task 2 Step 2 and read identically in Task 2 Step 3 and Task 3 Step 1. `meta()` signature `(&self) -> CommandMeta` is consistent across Tasks 2–3. The matrix tuple order (label, destructive, cancellable, collision, refresh) is internal to Task 1 and self-consistent. ✓

**Out-of-scope guard:** No task touches `to_argv`, `needs_metro`, the modal flow, `pipeline.rs`, `update.rs`, or the UI layer. ✓

---

## Next: Phase 2

Phase 2 (declared dependency graph + `resolve()` + `EnsureMetro`, removing `needs_metro()`/`Recipe`/`prerequisites()`) gets its **own plan**, written after Phase 1 merges and after a focused read of the metro lifecycle machinery (the spec's flagged risk). Phase 2's end-to-end characterization tests over `update()` are written first, against current behavior, per §7.
