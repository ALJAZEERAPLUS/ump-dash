---
quick_id: 260527-qlh
description: Make worktree table columns configurable and update docs/examples
date: 2026-05-27
mode: quick
---

# Quick Task 260527-qlh: Configurable Worktree Table Columns

## Goal

Allow users to configure the worktree table column widths from
`~/.config/rn-dash/config.toml` while preserving the current layout by default.

## Tasks

### 1. Add column-width config defaults

- **Files:** `src/domain/dash_config.rs`
- **Action:** Add a nested `[columns]` config type with defaults matching the
  current table constraints: status `4`, branch `20`, ticket `20`, dir `16`,
  task `20`.
- **Verify:** Missing and partial config keeps defaults.

### 2. Use config in the table renderer

- **Files:** `src/ui/panels.rs`
- **Action:** Replace hardcoded worktree table constraints with values from the
  loaded config; keep the ticket column as `Constraint::Min`.
- **Verify:** No config produces identical constraints to the current layout.

### 3. Update user-facing docs and example config

- **Files:** `README.md`, `config.example.toml`
- **Action:** Document the `[columns]` table and show default values in the
  example config.

## must_haves

- truths:
  - "Default column widths remain status=4, branch=20, ticket=20, dir=16, task=20"
  - "`ticket` remains the flexible minimum-width column"
  - "Partial `[columns]` config falls back per-field"
- artifacts:
  - "`src/domain/dash_config.rs` defines the nested column config"
  - "`src/ui/panels.rs` reads column widths from loaded config"
  - "`README.md` and `config.example.toml` document the new settings"
