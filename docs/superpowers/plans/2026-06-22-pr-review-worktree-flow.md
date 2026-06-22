# PR Review Worktree Flow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `+ r` review flow that lists open GitHub PRs, lets the user filter/search them, creates a fresh worktree from the PR's exact branch head, and runs `yarn install`.

**Architecture:** Domain owns PR metadata, filter policy, and ports. App owns pure state transitions and returns effects. Infra owns `gh` and `git` command execution behind domain-owned ports.

**Tech Stack:** Rust 2024, Ratatui, Tokio, `gh`, `git worktree`, existing app `EffectRunner` and command task pipeline.

---

### Task 1: Domain And Infra Boundary

**Files:**
- Create: `src/domain/review.rs`
- Create: `src/domain/ports/review_port.rs`
- Modify: `src/domain/mod.rs`
- Modify: `src/domain/ports/mod.rs`
- Modify: `src/domain/ports/worktree_port.rs`
- Create: `src/infra/review.rs`
- Modify: `src/infra/mod.rs`
- Modify: `src/infra/worktrees.rs`
- Modify: `src/app/adapters.rs`
- Modify: `src/main.rs`

- [ ] **Step 1: Write failing tests**

Add tests for:
- `PullRequestFilter::next()` cycles `All -> NotReviewed -> Mine -> NotMine -> All`.
- `gh_pr_search_query(PullRequestFilter::NotReviewed)` returns `is:pr is:open user-review-requested:@me draft:false`.
- PR JSON parsing maps `author.login`, `headRefName`, and `headRefOid`.

- [ ] **Step 2: Verify red**

Run: `cargo test review --lib`
Expected: compile/test failure because the new review module and helpers do not exist.

- [ ] **Step 3: Implement minimal domain and infra**

Add `PullRequest`, `PullRequestFilter`, `ReviewPort`, `GitHubCliReviewAdapter`, and `WorktreePort::add_review_worktree`.

- [ ] **Step 4: Verify green**

Run: `cargo test review --lib`
Expected: review tests pass.

### Task 2: Pure App Flow

**Files:**
- Modify: `src/domain/action.rs`
- Modify: `src/domain/command.rs`
- Modify: `src/app/state.rs`
- Modify: `src/app/effect.rs`
- Modify: `src/app/effect_runner.rs`
- Modify: `src/app/update.rs`
- Modify: `src/app/handle_key.rs`
- Modify: `src/app/keybindings.rs`
- Test: `src/app/dispatch_tests.rs`

- [ ] **Step 1: Write failing reducer/key tests**

Add tests for:
- `+ r` maps to `Action::ReviewOpen`.
- `ReviewOpen` returns `Effect::ListPullRequests` for `PullRequestFilter::All`.
- `Tab` in the PR picker cycles filter and requests a new PR list.
- Selecting a PR whose `headRefName` matches an existing worktree branch opens an info modal with that path and emits no checkout effect.
- Selecting an available PR opens a text input prefilled with the PR `headRefName`.
- Submitting that text input returns `Effect::AddReviewWorktree`.
- `ReviewWorktreeCreated` inserts/selects the worktree and dispatches `YarnInstall`.

- [ ] **Step 2: Verify red**

Run: `cargo test app::dispatch_tests::review_flow --lib`
Expected: compile/test failure because actions, effects, and modals do not exist.

- [ ] **Step 3: Implement minimal reducer/key/effect wiring**

Add PR picker modal/action/effect cases and connect `EffectRunner` to the new ports.

- [ ] **Step 4: Verify green**

Run: `cargo test app::dispatch_tests::review_flow --lib`
Expected: review flow reducer tests pass.

### Task 3: UI Rendering And Architecture Check

**Files:**
- Modify: `src/ui/modals.rs`
- Test: existing app/domain tests plus architecture guard.

- [ ] **Step 1: Write failing UI-adjacent coverage where practical**

Use reducer/key tests for behavior; keep Ratatui rendering smoke coverage minimal because existing modal rendering is not heavily unit-tested.

- [ ] **Step 2: Implement PR picker and info modal rendering**

Render search/filter text, PR rows, selected row, empty/loading states, and keyboard hints.

- [ ] **Step 3: Verify**

Run:
- `cargo test review --lib`
- `cargo test app::dispatch_tests::review_flow --lib`
- `make arch-lint`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`

Expected: all commands exit 0.
