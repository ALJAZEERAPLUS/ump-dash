---
quick_id: 260623-k7w
status: complete
completed: 2026-06-23
commit: aee6519
---

# Quick Task 260623-k7w Summary

Implemented the checkout-new branch picker flow for `g>c`.

## Changes

- `CommandSpec::GitCheckoutNew` now carries an optional base branch.
- `g>c` requests remote branches and opens the existing `BranchPicker` before asking for the new branch name.
- Confirming a picker entry for checkout-new stores the selected base branch in the pending command template.
- Submitting the new branch name now dispatches `git checkout -b <new> <base>`.
- The new-worktree branch picker flow still uses the same picker and keeps its existing base-branch handling.

## Verification

- `cargo test app::dispatch_tests::`
- `cargo test`
- `make arch-lint`
- `cargo clippy --all-targets -- -D warnings`
