# Task 2: CommandMeta Info-Card Refactor — Report

## Summary
Introduced `CommandMeta` struct and `meta()` method as the single source of truth for command static facts. All four accessors (`label()`, `is_destructive()`, `is_cancellable()`, `collision_policy()`) now delegate to `meta()`. Behavior preserved exactly — no signature changes, all tests pass including unmodified golden test.

## Edits Completed

### Edit 1: Make `RefreshSet` Copy
- File: `src/domain/refresh.rs`
- Changed derive from `#[derive(Debug, Clone, PartialEq, Eq)]` to `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`
- Rationale: `RefreshSet` is all-bool (3 x bool = 12 bytes); Copy is more ergonomic for passing through `CommandMeta`.

### Edit 2: Add `CommandMeta` struct and `meta()` method
- File: `src/domain/command.rs`
- Added import: `use super::refresh::RefreshSet;`
- Added `CommandMeta` struct (lines 186–201):
  ```rust
  pub struct CommandMeta {
      pub label: &'static str,
      pub destructive: bool,
      pub cancellable: bool,
      pub refresh: RefreshSet,
      pub collision: CollisionPolicy,
  }
  ```
- Added exhaustive `meta()` method (lines 313–336):
  - 19 match arms, one per `CommandSpec` variant
  - Uses local bindings (`full`, `stale`, `none`) for RefreshSet patterns
  - Uses `use CollisionPolicy::{BlockNew, CancelPrevious}` for brevity
  - Exhaustive match (no `_` arm) ensures compile-error on new variants

### Edit 3: Rewire four accessors to read from `meta()`
- File: `src/domain/command.rs`
- Replaced `is_destructive()` body: `self.meta().destructive`
- Replaced `is_cancellable()` body: `self.meta().cancellable`
- Replaced `collision_policy()` body: `self.meta().collision`
- Replaced `label()` body: `self.meta().label`
- All doc-comments preserved; only match bodies removed

## Testing

### Type Check
```
CARGO_INCREMENTAL=1 cargo check
→ Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
```
✓ Clean build, no warnings

### Tests
```
CARGO_INCREMENTAL=1 cargo test
→ test result: ok. 356 passed; 0 failed (domain crate)
→ test result: ok. 3 passed; 0 failed (main.rs)
→ test result: ok. 1 passed; 0 failed (agent_mcp_smoke)
→ test result: ok. 3 passed; 0 failed (metro_single_instance)
→ test result: ok. 1 passed; 0 failed (process_group_cancel)
→ test result: ok. 1 passed; 0 failed (process_group_kill)
→ test result: ok. 2 passed; 0 failed (worktree_slice_metro)
→ test result: ok. 2 passed; 0 failed (yarn_semaphore_serializes)
→ Test total: 369 passed, 0 failed
```

### Golden Test
```
CARGO_INCREMENTAL=1 cargo test command_metadata_matrix -- --nocapture
→ test domain::command::tests::command_metadata_matrix ... ok
```
✓ Unmodified test passes, no value transcription errors

### Collision Policy Tests
- `collision_policy_idempotent_installs_block_new` ✓
- `collision_policy_builds_tests_runs_cancel_previous` ✓
- `collision_policy_git_variants_all_block_new` ✓
- `collision_policy_covers_every_variant` ✓ (still valid, though now uses `meta()` as source)

### Cancellable Tests
- `is_cancellable_git_variants_all_false` ✓
- `is_cancellable_yarn_variants_all_true` ✓
- `is_cancellable_run_variants_all_true` ✓
- `is_cancellable_rn_clean_variants_all_true` ✓
- `is_cancellable_adb_install_true` ✓
- `is_cancellable_shell_true` ✓

### Refresh Tests (from `refresh.rs`)
All 13 refresh tests pass unchanged (test at layer below `meta()`).

## Import Notes
- No unused imports. `RefreshSet` was previously imported in tests; now imported at module level for `meta()`.
- `CollisionPolicy` already in scope; only added `use CollisionPolicy::*` inside `meta()` for ergonomics.

## Commit
```
commit 11ba416 (HEAD -> remove-git-checkout-rebase-commands)
Author: Ali Monemian <cubicme@hey.com>

refactor(command): introduce CommandMeta info-card, readers delegate to meta()

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
```

## Notable Decisions
1. **Exhaustive match in `meta()`**: No `_` arm. Adding a new `CommandSpec` variant will fail to compile here, enforcing the fill-in-the-card pattern.
2. **Copy trait on RefreshSet**: Safe and ergonomic (small POD struct). Aids pass-through in CommandMeta.
3. **Preserved doc-comments**: All four accessor methods retain their original documentation (e.g., REFACTOR-02 note on `is_cancellable`).
4. **Preserved `collision_policy_covers_every_variant` test**: Still passes; now becomes a sanity check that `meta()` covers every variant (even though the test's own match still mirrors the dispatch logic).

## Behavior Verification
- Behavior unchanged: same label, destructive, cancellable, collision, and refresh values flow through.
- Method signatures unchanged: no caller impact.
- Golden test unmodified and passing: evidence of accurate value transcription.
