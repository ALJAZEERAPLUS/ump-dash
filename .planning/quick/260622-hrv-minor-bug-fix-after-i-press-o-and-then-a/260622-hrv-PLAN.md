---
quick_id: 260622-hrv
status: in_progress
---

# Quick Task 260622-hrv: Return to root after submenu option selection

## Goal

After pressing `o` and choosing any open submenu option, the palette returns to the root. Apply the same invariant to submenu option selections generally.

## Tasks

1. Add a focused dispatch regression test covering submenu actions clearing `state.modal_stack.palette_mode`.
2. Fix the open submenu action handlers that currently leave `PaletteMode::Open` active.
3. Run focused Rust tests, architecture guard, and inspect git status before committing.
