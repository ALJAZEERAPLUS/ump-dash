---
status: complete
quick_id: 260630-de7
slug: fix-dashboard-worktree-creation-when-bas
commit: 8442487
---

# Quick Task 260630-de7 Summary

Fixed dashboard worktree creation when the requested base branch exists only as a local branch and has no `origin/<branch>` ref.

## Changes

- Added `resolve_worktree_base_ref` in `src/infra/worktrees.rs`.
- New-branch worktree creation now prefers `refs/remotes/origin/<base>` when present and falls back to `refs/heads/<base>`.
- Added a real-git regression test for `base_branch = UMP-6831` with no remote-tracking ref.
- Updated the `WorktreePort` contract comment to document local-only base branch support.

## Verification

- Red test first reproduced `fatal: invalid reference: origin/UMP-6831`.
- `cargo test infra::worktrees::tests::add_new_branch_uses_local_base_branch_when_no_origin_ref -- --nocapture`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `make arch-lint`

`rustfmt --edition 2024 --check src/infra/worktrees.rs src/domain/ports/worktree_port.rs` still reports pre-existing formatting drift in older `src/infra/worktrees.rs` test blocks that were not part of this fix.
