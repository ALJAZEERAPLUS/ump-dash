---
phase: 14
plan: 04
subsystem: app-effect
tags: [effect-grammar, spawn-task, per-worktree-task, q1-lock]
dependency_graph:
  requires:
    - domain::task::TaskId         # from Plan 14-01
    - domain::worktree::WorktreeId # from worktree.rs
    - domain::command::CommandSpec # from command.rs
  provides:
    - app::effect::Effect::SpawnTask
  affects:
    - src/app/effect_runner.rs     # stub arm added (Rule 3 deviation)
tech_stack:
  added: []
  patterns:
    - TEA effect grammar extension (pure data variant, no behavior change)
    - TDD RED/GREEN inline test
key_files:
  created: []
  modified:
    - src/app/effect.rs
    - src/app/effect_runner.rs
decisions:
  - Q1 locked: cwd+branch included in SpawnTask payload (mirrors SpawnCommand convention from Plan 13-08; runner needs context without querying state)
  - SpawnCommand preserved: no callers migrated in this plan; Plan 14-07 migrates dispatch sites; Plan 14-09 deletes SpawnCommand after all callers moved
  - effect_runner stub arm: unimplemented!() arm satisfies compiler exhaustiveness; Plan 14-06 replaces it with run_spawn_task implementation
metrics:
  duration: 2m 31s
  completed_date: 2026-04-28
  tasks_completed: 1
  tasks_total: 1
  files_modified: 2
---

# Phase 14 Plan 04: Effect::SpawnTask Variant Summary

**One-liner:** Added `Effect::SpawnTask { task_id, worktree_id, spec, cwd, branch }` variant with Q1-locked cwd+branch payload, adjacent to `SpawnCommand` in the effect grammar.

## What Was Built

Added the `Effect::SpawnTask` variant to `src/app/effect.rs` as the single chokepoint for per-worktree task spawning (D-10/D-20). This is pure data — no behavior change, no callers migrated. The variant carries:

- `task_id: crate::domain::task::TaskId` — correlates output/exit events to the task
- `worktree_id: crate::domain::worktree::WorktreeId` — identifies the target worktree slice
- `spec: crate::domain::command::CommandSpec` — what to run
- `cwd: std::path::PathBuf` — working directory (Q1 lock: included so runner needs no state lookup)
- `branch: String` — current branch (Q1 lock: same rationale)

## Q1 Lock

Open Question Q1 from RESEARCH.md is now locked: **cwd and branch are included in the SpawnTask payload**, consistent with the existing `SpawnCommand` convention (Plan 13-08) and Pitfall P-7 (avoiding runner needing to query AppState for context).

## Test Delta

- `spawn_task_variant_constructs_and_matches` — new inline test, constructs variant with TaskId(42) and WorktreeId("wt-test"), pattern-matches, asserts field equality. Passes.
- `effect_has_at_least_fifteen_variants` — updated with arm 17 for `SpawnTask { .. }`. Still passes.
- `effect_variants_compile` — untouched, still passes.
- Total effect tests: 3 (was 2 before this plan).
- `cargo test --workspace`: 86 tests pass (up from 85 with the new test).

## SpawnCommand Status

`Effect::SpawnCommand` is **still present and alive**. No callers were migrated. Plan 14-07 will migrate dispatch sites; Plan 14-09 will delete `SpawnCommand` once all callers have moved.

Verification: `rg 'SpawnCommand\s*\{' src/app/effect.rs` still matches.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Add stub arm for Effect::SpawnTask in effect_runner.rs**

- **Found during:** Task 1 (GREEN phase — cargo test --lib effect)
- **Issue:** `effect_runner.rs::run_one()` has a non-exhaustive `match effect` block. Adding a new `Effect` variant without a corresponding match arm causes a compile error (`E0004: patterns not exhaustive`). This blocked compilation and all tests.
- **Fix:** Added an `Effect::SpawnTask { .. } => unimplemented!("Plan 14-06")` stub arm at the end of the `run_one` match. The `unimplemented!()` macro is intentional — no caller emits `SpawnTask` before Plan 14-07, so this arm will never execute in practice. Plan 14-06 replaces it with the real `run_spawn_task` implementation.
- **Files modified:** `src/app/effect_runner.rs`
- **Commit:** 3a341ab (same commit as the variant addition — atomically bundled)

## Verification Results

| Check | Result |
|-------|--------|
| `cargo test --lib effect` | 3/3 pass |
| `cargo test --workspace` | 86/86 pass |
| `cargo check` | 0 errors, 0 unused warnings |
| `cargo clippy --all-targets -- -D warnings` | Clean |
| `make arch-lint` (G-01..G-20) | PASS |
| `rg 'SpawnTask\s*\{' src/app/effect.rs` | Match |
| `rg 'SpawnCommand\s*\{' src/app/effect.rs` | Match (still alive) |
| `rg 'task_id: crate::domain::task::TaskId' src/app/effect.rs` | Match |
| `rg 'worktree_id: crate::domain::worktree::WorktreeId' src/app/effect.rs` | Match |
| `rg -c 'cwd: std::path::PathBuf' src/app/effect.rs` | 2 (SpawnCommand + SpawnTask) |

## Commits

| Hash | Type | Description |
|------|------|-------------|
| 3a341ab | feat | Add Effect::SpawnTask variant + exhaustiveness stub in effect_runner |

## Self-Check: PASSED

| Item | Status |
|------|--------|
| `src/app/effect.rs` exists | FOUND |
| `src/app/effect_runner.rs` exists | FOUND |
| Commit 3a341ab exists | FOUND |
