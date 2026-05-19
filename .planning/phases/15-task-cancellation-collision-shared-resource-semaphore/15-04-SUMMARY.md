---
phase: 15-task-cancellation-collision-shared-resource-semaphore
plan: 04
subsystem: domain
tags: [domain, collision-policy, predicate, command-spec, type-driven]
requires:
  - "Phase 13 / Plan 13-02 `is_cancellable()` (template / shape mirror)"
  - "Phase 14 D-05 collision identity = (CommandSpec discriminant, WorktreeId)"
provides:
  - "pub enum CollisionPolicy { BlockNew, CancelPrevious } in src/domain/command.rs"
  - "pub fn CommandSpec::collision_policy(&self) -> CollisionPolicy (exhaustive, no _ arm)"
affects:
  - "Plan 15-05 (update.rs collision gate) will consume this predicate via use crate::domain::command::CollisionPolicy"
tech-stack:
  added: []
  patterns:
    - "Exhaustive match (no _ arm) for type-driven drift guard — mirrors REFACTOR-02 is_cancellable shape"
    - "Per-family inline tests with for-loop assert pattern (matches is_cancellable_* test family)"
    - "Drift-guard meta-test enumerating every CommandSpec variant — two-layer compile-time enforcement"
key-files:
  created: []
  modified:
    - src/domain/command.rs
decisions:
  - "Match arms grouped by semantic intent (idempotent installs / non-cancellable git / builds-tests-runs) rather than by enum declaration order — clarifies Q-4 reasoning for reviewers."
  - "Drift-guard meta-test asserts variants.len() == 23 to catch the case where a future maintainer adds a CommandSpec variant but only updates the meta-test's match (not the array itself)."
metrics:
  duration_minutes: 7
  tasks_completed: 1
  files_modified: 1
  lines_added: 208
  lines_removed: 0
  tests_added: 4
  commits: 2
  completed_date: 2026-05-19
---

# Phase 15 Plan 04: Domain CollisionPolicy + collision_policy() Predicate Summary

CollisionPolicy enum and CommandSpec::collision_policy() predicate landed in src/domain/command.rs as a pure domain change — no infra, no app, no tokio — mirroring the Phase 13 is_cancellable() type-driven cancellability surface exactly.

## What Was Built

**Two API additions to `src/domain/command.rs`:**

1. `pub enum CollisionPolicy { BlockNew, CancelPrevious }` at module scope (immediately before `impl CommandSpec`), deriving `Debug, Clone, Copy, PartialEq, Eq`. Doc comments explain the two semantics:
   - `BlockNew` — existing task keeps running, new dispatch silently dropped. For idempotent installs + non-cancellable git porcelain.
   - `CancelPrevious` — existing task aborted, new task dispatched. For builds / tests / runs / clean operations where the user intent is "run THIS version NOW".

2. `pub fn collision_policy(&self) -> CollisionPolicy` on `impl CommandSpec`, placed immediately after `is_cancellable()` and before `needs_text_input()`. The match expression is exhaustive with NO `_ =>` arm — adding a new CommandSpec variant in any future phase produces a compile error here, forcing explicit policy assignment.

**Policy assignment per 15-RESEARCH §F6 + Q-4:**

| Group | Variants | Policy |
|-------|----------|--------|
| Idempotent installs | YarnInstall, YarnPodInstall | BlockNew |
| Non-cancellable git porcelain (Q-4) | GitResetHard, GitResetHardFetch, GitPull, GitPush, GitRebase, GitCheckout, GitCheckoutNew, GitFetch | BlockNew |
| Builds / tests / runs / clean | YarnUnitTests, YarnJest, YarnLint, YarnCheckTypes, RnRunAndroid, RnRunIos, RnRunIosDevice, RnReleaseBuild, AdbInstallApk, ShellCommand, RnCleanAndroid, RnCleanCocoapods, RmNodeModules | CancelPrevious |

23 variants total — matches `CommandSpec` doc comment ("23 variants total") and the drift-guard meta-test's `variants.len() == 23` assertion.

## Tests Added (+4)

| Test | Coverage |
|------|----------|
| `collision_policy_idempotent_installs_block_new` | YarnInstall, YarnPodInstall → BlockNew |
| `collision_policy_builds_tests_runs_cancel_previous` | 13 variants → CancelPrevious |
| `collision_policy_git_variants_all_block_new` | 8 git variants → BlockNew (mirrors `is_cancellable_git_variants_all_false`) |
| `collision_policy_covers_every_variant` | Drift-guard meta-test: exhaustive match (no `_ =>`) over all 23 variants — second layer of compile-time enforcement against silent default assignment (T-15-04-01). |

Lib test count: **102 (+4 vs baseline 98)**, zero regressions.

## TDD Cycle

- **RED** (`1a04787`): All 4 new tests fail to compile (E0433 / E0599 — `CollisionPolicy` and `collision_policy()` undeclared). Confirmed before implementation.
- **GREEN** (`359f9cb`): Enum + predicate added. 4/4 new tests pass. 102/102 lib tests green. Cargo build green. Clippy `-D warnings` clean. `make arch-lint` 21/21 green.
- **REFACTOR**: Not needed. The implementation already follows the established `is_cancellable()` pattern verbatim — no cleanup opportunity.

## Verification Run

| Check | Result |
|-------|--------|
| `cargo build --quiet` | green (no output) |
| `cargo test --lib --quiet` | 102 passed; 0 failed; 0 ignored |
| `cargo test --lib --quiet command::tests::collision_policy*` | 4 passed; 0 failed |
| `cargo test --lib --quiet command::tests::is_cancellable*` | All 6 pre-existing tests still pass (no Phase 13 regressions) |
| `cargo clippy --all-targets -- -D warnings` | clean |
| `make arch-lint` | PASS — 21/21 G-XX guards green |
| `rg 'collision_policy' src/` | hits ONLY in `src/domain/command.rs` (no infra/app coupling) |

## Acceptance-Criteria Grep Verification

```
pub enum CollisionPolicy: 1
pub fn collision_policy: 1
CollisionPolicy::BlockNew: 6   (≥ 3 required)
CollisionPolicy::CancelPrevious: 4   (≥ 3 required)
_ => match arms (real, not in doc text): 1   (unchanged from baseline — predicate body has zero wildcards)
collision_policy referenced in src/: only src/domain/command.rs
```

## Threat Model Mitigations

| Threat ID | Mitigation Status |
|-----------|-------------------|
| T-15-04-01 (Tampering — future variant silently defaults) | Mitigated: exhaustive match in predicate body (Rust compile-error on missing variant) + drift-guard meta-test mirroring the match shape (second layer — compile-fail even if a maintainer adds a wildcard to the predicate body) |
| T-15-04-02 (Repudiation — wrong policy slips through review) | Mitigated: three per-family tests assert every variant's policy by name; misassignment fails `assert_eq!` with debug-printed spec |
| T-15-04-SC (Tampering — supply chain) | N/A: zero `Cargo.toml` edits, no new dependencies |

## Plan-vs-Reality Variant Reconciliation

The plan's `<interfaces>` section listed some variants with slightly different shapes than what currently lives in `src/domain/command.rs`:

| Plan said | File actually has | Impact |
|-----------|-------------------|--------|
| `GitRebase { onto: String }` | `GitRebase { target: String }` | None — destructured as `{ .. }` in match arm |
| `RnRunAndroid { mode: String }` | `RnRunAndroid { device_id: String, mode: Option<String> }` | None — destructured as `{ .. }` in match arm; test constructs `Some("release".into())` |
| `RnRunIos { device: String }` | `RnRunIos { device_id: String }` | None — destructured as `{ .. }` in match arm |

Match arms use `{ .. }` destructuring (not field-name binding) so the field-name divergence does not affect behavior. Tests construct variants using the actual file's field names. No deviation — implementation followed the **file**, not the plan's stale interface description.

## Deviations from Plan

None — plan executed exactly as written. The plan called out the predicate body must use exhaustive matching with no `_ =>` arm; implementation does. The plan called out the drift-guard meta-test must mirror the predicate shape; implementation does. The plan called out four new inline tests; four added. The plan called out placement after `is_cancellable()` and before `needs_text_input()`; that's exactly where the method sits.

## Downstream Impact

**Plan 15-05** (update.rs collision gate) is now unblocked. It can:

```rust
use crate::domain::command::{CommandSpec, CollisionPolicy};

match new_spec.collision_policy() {
    CollisionPolicy::BlockNew => { /* drop new dispatch */ }
    CollisionPolicy::CancelPrevious => { /* abort existing, dispatch new */ }
}
```

The two-variant exhaustive match in the consumer is the symmetric guard — if Phase 15 (or any later phase) ever adds a third `CollisionPolicy` variant, the consumer will compile-fail until the new branch is handled. Type-driven authority chain established.

## Commits

| Phase | Hash | Message |
|-------|------|---------|
| RED | `1a04787` | test(15-04): add failing tests for CollisionPolicy + collision_policy() |
| GREEN | `359f9cb` | feat(15-04): add CollisionPolicy enum + collision_policy() predicate |

## TDD Gate Compliance

- RED commit (`test(...)`) present: ✓ `1a04787`
- GREEN commit (`feat(...)`) present after RED: ✓ `359f9cb`
- REFACTOR commit: intentionally absent (no cleanup opportunity — implementation matched the established `is_cancellable()` template verbatim on first pass)

## Self-Check: PASSED

- `src/domain/command.rs`: FOUND
- Commit `1a04787` (RED): FOUND
- Commit `359f9cb` (GREEN): FOUND
- All acceptance criteria verified via grep
- All verification commands green (build, test, clippy, arch-lint)
