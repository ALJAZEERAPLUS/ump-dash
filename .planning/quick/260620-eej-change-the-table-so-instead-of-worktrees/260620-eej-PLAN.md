---
quick_id: 260620-eej
status: planned
created: 2026-06-20
---

# Quick Task 260620-eej: Worktree Table Column Headers

## Goal

Change the worktree table so the columns themselves have headers instead of the table block title saying `Worktrees`.

## Tasks

1. Add column header rendering to the worktree table.
   - Files: `src/ui/panels.rs`
   - Action: Map each configured `WorktreeTableColumn` to a short visible label, remove the `Worktrees` block title, and pass the labels to the Ratatui table as a styled header row.
   - Verify: Existing worktree table behavior remains state-driven and UI-only.

2. Add or update focused UI coverage.
   - Files: `src/ui/panels.rs`
   - Action: Add a small test for the column label mapping so configured columns have stable headers.
   - Verify: Run focused UI tests.

3. Run guards and record summary.
   - Files: `.planning/STATE.md`, quick summary.
   - Action: Run focused tests and `make arch-lint`; run broader Rust checks as practical for the scope.
   - Verify: Commit code changes atomically, then commit GSD artifacts.
