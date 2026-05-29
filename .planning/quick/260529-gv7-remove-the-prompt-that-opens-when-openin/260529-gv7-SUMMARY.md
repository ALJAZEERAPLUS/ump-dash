---
status: complete
quick_id: 260529-gv7
completed: 2026-05-29
commit: a2b79bf
---

# Quick Task 260529-gv7 Summary

## Result

Opening Claude Code now immediately opens a multiplexer surface named `<worktree-prefix>-claude`. The custom suffix text-input prompt and its pending state were removed.

## Changed Files

- `src/app/update.rs` - `Action::OpenClaudeCode` now emits `Effect::OpenInMultiplexer` directly.
- `src/app/state.rs` - removed the unused `pending_claude_open` modal handoff field.
- `src/app/dispatch_tests.rs` - added a regression test for direct Claude tab opening without a suffix prompt.

## Verification

- `cargo test open_claude_code_opens_default_tab_without_suffix_prompt` - passed.
- `cargo test` - passed: 159 lib tests, 6 integration tests, 0 doctests.

## Commit

- `a2b79bf` - `fix(quick-260529-gv7): skip Claude suffix prompt`
