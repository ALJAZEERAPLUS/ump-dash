---
quick_id: 260622-bvn
slug: add-pr-review-worktree-flow-from-the-men
status: planned
created: 2026-06-22
---

# Quick Task 260622-bvn: Add PR review worktree flow from the + menu

Add a review flow accessible from the Worktree `+` palette with key `r`.

## Task 1: Domain, Ports, And Infra

Files:
- `src/domain/review.rs`
- `src/domain/ports/review_port.rs`
- `src/domain/ports/worktree_port.rs`
- `src/infra/review.rs`
- `src/infra/worktrees.rs`
- `src/app/adapters.rs`
- `src/main.rs`

Actions:
- Add domain PR metadata and review filter types.
- Add a `ReviewPort` for GitHub PR listing through `gh pr list`.
- Add `WorktreePort::add_review_worktree` to fetch `refs/pull/<n>/head`, force-update local `headRefName`, create the chosen worktree directory, and seed local files.

Verify:
- Unit tests cover filter queries and PR JSON parsing.
- Worktree port testable helper builds the guarded git commands without app/UI infra leaks.

## Task 2: App State, Actions, Effects, And Keybindings

Files:
- `src/domain/action.rs`
- `src/domain/command.rs`
- `src/app/state.rs`
- `src/app/effect.rs`
- `src/app/effect_runner.rs`
- `src/app/update.rs`
- `src/app/handle_key.rs`
- `src/app/keybindings.rs`

Actions:
- Add PR picker modal with search text, selected row, and `All` / `Not reviewed` / `Mine` / `Not mine` filter.
- Add `+ r` binding to load PRs.
- Use current worktree state, already sourced from `WorktreePort::list`, to block selecting a PR whose exact `headRefName` is checked out anywhere.
- Use existing text input flow for editable worktree directory name, prefilled with the exact PR `headRefName`.
- After review worktree creation, insert/select the new worktree and dispatch `YarnInstall` for that worktree.

Verify:
- App reducer tests cover filter cycling, local search, checked-out branch blocking, prefilled worktree name, and `YarnInstall` dispatch.
- Keybinding tests cover `+ r`.

## Task 3: UI And Final Verification

Files:
- `src/ui/modals.rs`
- docs/config only if user-facing configuration changes are added.

Actions:
- Render the PR picker as a conventional search/list modal.
- Render a generic info modal for "branch already checked out" and other non-error notices.

Verify:
- `cargo test` focused reducer/domain/infra tests.
- `make arch-lint`.
- Broader `cargo test` / `cargo clippy --all-targets -- -D warnings` if focused checks pass and time permits.
