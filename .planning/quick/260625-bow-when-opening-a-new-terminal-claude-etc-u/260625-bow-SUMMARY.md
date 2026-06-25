---
phase: quick-260625-bow
plan: "01"
subsystem: domain/worktree
tags: [naming, worktree, display, refactor]
status: complete

dependency_graph:
  requires: []
  provides:
    - Directory-name-only preferred_prefix() and display_name() in Worktree domain type
  affects:
    - src/domain/worktree.rs
    - src/app/dispatch_tests.rs

tech_stack:
  added: []
  patterns:
    - Single source of truth: worktree directory name for all tab naming

key_files:
  modified:
    - src/domain/worktree.rs
    - src/app/dispatch_tests.rs

decisions:
  - preferred_prefix() and display_name() both return path.file_name() with "worktree" fallback — no jira_key/branch tiers
  - JIRA struct fields and all JIRA logic outside the two functions remain intact

metrics:
  duration: "~5 minutes"
  completed: "2026-06-25"
  tasks: 2
  files: 2
---

# Phase quick-260625-bow Plan 01: Directory-Name-Only Worktree Tab Naming Summary

**One-liner:** Simplified `preferred_prefix()` and `display_name()` to always return the worktree directory name, dropping jira_key/branch tiers so Claude/shell/editor tabs reflect the directory the user is working in.

## What Was Built

Both naming functions in `src/domain/worktree.rs` now return only `path.file_name()` (with `"worktree"` fallback). The old priority chains — jira_key > branch > directory for `preferred_prefix()` and jira_title > branch for `display_name()` — are removed. Two dispatch test assertions were updated to assert the new directory-name contract.

## Tasks Completed

| # | Task | Commit | Files |
|---|------|--------|-------|
| 1 | Rewrite preferred_prefix() and display_name() to directory name only | 8c3f3b7 | src/domain/worktree.rs |
| 2 | Update broken dispatch tests to assert directory-name contract | af41067 | src/app/dispatch_tests.rs |

## Verification

- `cargo check` — clean compile, no errors or new warnings
- `cargo test` — full suite passes
- `grep -n 'jira_key\|branch' src/domain/worktree.rs` — only appears in struct field declarations, not in `preferred_prefix()` or `display_name()` bodies
- `grep -rn 'preferred_prefix\|display_name' src/app/update.rs` — three unchanged consumer call sites (OpenClaudeCode, OpenShellTab, OpenEditor)
- No remaining `main-claude` / `main-editor` / `main-shell` assertions in test files

## Deviations from Plan

None — plan executed exactly as written.

Note: `cargo check --incremental` flag is not supported by this version of cargo; `cargo check` (without the flag) was used, which achieves the same verification goal.

## Known Stubs

None.

## Threat Flags

None — pure internal rename of a display-string derivation; no new trust boundaries or I/O surface introduced.

## Self-Check: PASSED

- src/domain/worktree.rs: modified and committed at 8c3f3b7
- src/app/dispatch_tests.rs: modified and committed at af41067
- Both commits present in git log
- All tests pass
