---
quick_id: 260623-k7w
status: complete
completed: 2026-06-23
commit: ffa5c48
---

# Quick Task 260623-k7w Summary

Implemented the existing-branch picker flow for worktree checkout (`+>c`) and removed the incorrect picker behavior from git checkout-new (`g>c`).

## Changes

- `+>c` now requests remote branches and opens the existing `BranchPicker`.
- Confirming a picker entry for `+>c` dispatches `Effect::AddWorktree` for the selected branch.
- `g>c` is restored to the direct new-branch-name text input and dispatches `git checkout -b <new>`.
- The new-branch worktree flow (`+>n`) still uses the same picker and keeps its base-branch handling.

## Verification

- `cargo test app::dispatch_tests::`
- `cargo test`
- `make arch-lint`
- `cargo clippy --all-targets -- -D warnings`
