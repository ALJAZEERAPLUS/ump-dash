# Task 4 Report: Drift-Guard Test & Doc-Comment Cleanup

## Summary
Completed all three edits to `src/domain/command.rs` for Phase 1 refactor cleanup. Removed redundant drift-guard test subsumed by `CommandSpec::meta()` exhaustiveness and golden test. Updated two stale doc-comments to reflect current design.

## Changes Made

### Edit 1: Deleted `collision_policy_covers_every_variant()` test
- Removed entire function (lines 805-870 in pre-edit state)
- This test mirrored exhaustive match logic that is now the sole responsibility of `meta()`'s exhaustive match + `command_metadata_matrix` golden test
- No longer needed as compile-time enforcement is handled by `meta()` exhaustiveness

### Edit 2: Updated `collision_policy()` doc-comment
- Replaced stale comment referencing "intentionally exhaustive" match and deleted `collision_policy_covers_every_variant` test
- New comment correctly states: "The value comes from `meta()`, whose exhaustive match (no `_` arm) is the compile-time drift guard"

### Edit 3: Updated `is_cancellable()` doc-comment
- Replaced stale REFACTOR-02 reference and flat-enum predicate description
- New comment clearly states: "The value comes from `meta()`; adding a new variant forces an explicit decision there (exhaustive match, no `_` arm)"

## Verification

### Type Check
```
CARGO_INCREMENTAL=1 cargo check
✓ Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.22s
```

### Tests
```
CARGO_INCREMENTAL=1 cargo test --lib
test result: ok. 355 passed; 0 failed; 0 ignored
```

- **Before**: 356 tests (includes deleted `collision_policy_covers_every_variant`)
- **After**: 355 tests (one fewer, as expected)
- `command_metadata_matrix` test still passes
- All other domain::command tests pass

### Commit
```
commit: 8e5ec3c
message: test(command): drop drift-guard subsumed by meta() exhaustiveness + matrix
Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>
```

## Notes
- No production logic touched
- No other tests modified or deleted
- `command_metadata_matrix` remains as golden test for 19-variant coverage
- `task_short_label_covers_every_variant` test in UI layer stays intact
- All edits preserve behavior; this is pure cleanup/documentation improvement
