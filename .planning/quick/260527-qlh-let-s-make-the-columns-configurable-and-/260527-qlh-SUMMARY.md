---
quick_id: 260527-qlh
status: complete
commit: 0c28a1c
date: 2026-05-27
files_changed: 6
---

# Quick Task 260527-qlh Summary

**Done.** Worktree table column order and visibility are configurable through a
`columns = [...]` array in `~/.config/rn-dash/config.toml`. Widths remain fixed
by rn-dash.

## Changes

- `src/domain/dash_config.rs`: added `WorktreeTableColumn` with a default
  ordered list of `status`, `branch`, `ticket`, `dir`, and `task`.
- `src/ui/panels.rs`: renders cells and constraints from the configured column
  order. Omitted names hide columns; widths remain built-in.
- `README.md` and `config.example.toml`: documented the `columns` array and
  accepted values.

## Verification

- `cargo test column --lib`: passed, including default order, custom order,
  hidden columns, unknown names, and duplicate-name rejection.
- `cargo check`: passed.

## Commit

`0c28a1c` feat(config): configure column order
