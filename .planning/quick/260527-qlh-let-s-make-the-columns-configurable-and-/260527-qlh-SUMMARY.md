---
quick_id: 260527-qlh
status: complete
commit: c3eb089
date: 2026-05-27
files_changed: 6
---

# Quick Task 260527-qlh Summary

**Done.** Worktree table column widths are now configurable through a
`[columns]` table in `~/.config/rn-dash/config.toml`.

## Changes

- `src/domain/dash_config.rs`: added `WorktreeTableColumns` with serde
  defaults for `status`, `branch`, `ticket`, `dir`, and `task`.
- `src/ui/panels.rs`: reads loaded column widths before building the Ratatui
  table constraints. `ticket` remains the flexible `Constraint::Min` column.
- `README.md` and `config.example.toml`: documented the `[columns]` table and
  default values.

## Verification

- `cargo test columns_ --lib`: passed, including missing-config and
  partial-config fallback coverage.
- `cargo check`: passed.

## Commit

`c3eb089` feat(config): make columns configurable
