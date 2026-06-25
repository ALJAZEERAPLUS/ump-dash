---
phase: quick-260625-bow
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - src/domain/worktree.rs
  - src/app/dispatch_tests.rs
autonomous: true
requirements: []

must_haves:
  truths:
    - "Opening Claude/shell/editor tabs names the tab after the worktree DIRECTORY name, never the JIRA key or branch"
    - "preferred_prefix() returns the directory name for every worktree, regardless of jira_key or branch"
    - "display_name() returns the directory name, agreeing with preferred_prefix()"
    - "JIRA fields (jira_key, jira_title) and all JIRA logic remain present and untouched outside the two naming functions"
    - "cargo test passes (updated assertions lock in directory-name naming)"
  artifacts:
    - path: "src/domain/worktree.rs"
      provides: "Directory-name-only preferred_prefix() and display_name()"
      contains: "file_name"
    - path: "src/app/dispatch_tests.rs"
      provides: "Updated tab-name assertions asserting the directory name"
      contains: "ump-dash-claude"
  key_links:
    - from: "src/app/update.rs"
      to: "src/domain/worktree.rs"
      via: "OpenClaudeCode/OpenShellTab/OpenEditor call wt.preferred_prefix() (unchanged consumers)"
      pattern: "preferred_prefix"
---

<objective>
Make every worktree-derived name use the worktree DIRECTORY name (`path.file_name()`) as the single source of truth. Drop the `jira_key` tier and the `branch` tier from BOTH naming functions in the domain layer, so Claude/shell/editor tabs (and any modal title / status message) are named after the directory the user is actually in.

Purpose: When opening a new terminal/Claude/editor, the user wants the tab to reflect the directory they are working in, not a JIRA key or branch that may differ. Locked decision: ALWAYS USE THE DIRECTORY NAME.

Output: `Worktree::preferred_prefix()` and `Worktree::display_name()` both return the directory name; broken dispatch tests updated to assert the directory-name contract.
</objective>

<execution_context>
@$HOME/.claude/gsd-core/workflows/execute-plan.md
@$HOME/.claude/gsd-core/templates/summary.md
</execution_context>

<context>
@.planning/STATE.md
@./CLAUDE.md
@src/domain/worktree.rs

# Consumers of preferred_prefix() — read for confirmation only, DO NOT edit:
#   src/app/update.rs ~line 3009 (OpenClaudeCode), ~3037 (OpenShellTab), ~3072 (OpenEditor)
# Each builds `format!("{}-claude" | "{}-shell" | "{}-editor", wt.preferred_prefix())`.
# These stay untouched — only the function they call changes.

# Tests that WILL break and must be fixed (in this plan):
#   src/app/dispatch_tests.rs:3655 asserts name == "main-claude"  (branch tier)
#   src/app/dispatch_tests.rs:3681 asserts name == "main-editor"  (branch tier)
# Both fixtures use seed_one_worktree_id("ump-dash") -> path /tmp/ump-dash, branch "main",
# jira_key None. New directory-name behavior makes these "ump-dash-claude" / "ump-dash-editor".

# NOT affected (do NOT touch):
#   src/infra/multiplexer.rs:253-285 — passes literal "app-claude" directly to
#     ghostty_new_surface_command_parts; not derived from preferred_prefix().
#   src/infra/worktrees.rs:342 — `display_name: "Run UMP App"` is a different struct's
#     field literal, unrelated to Worktree::display_name().
</context>

<tasks>

<task type="auto">
  <name>Task 1: Make both naming functions return the directory name only</name>
  <files>src/domain/worktree.rs</files>
  <action>
In `src/domain/worktree.rs`, rewrite the bodies of `preferred_prefix()` (currently lines ~65-77) and `display_name()` (currently lines ~52-57) so each returns the worktree directory name derived from `self.path.file_name()`, with the existing "worktree" fallback when the file name is unavailable.

preferred_prefix(): DELETE the `jira_key` branch (the `if let Some(key) = &self.jira_key` block) and the `branch` branch (the `if !self.branch.is_empty() && self.branch != "(unknown)"` block). The body becomes only the existing directory-name expression: `self.path.file_name().and_then(|n| n.to_str()).unwrap_or("worktree").to_string()`. Keep the `-> String` signature.

display_name(): DELETE the `jira_title` branch (the `if let Some(title) = &self.jira_title` block) and the `self.branch.as_str()` return. Return the directory name as a `&str` borrowed from `self.path`: `self.path.file_name().and_then(|n| n.to_str()).unwrap_or("worktree")`. This compiles as `-> &str` because the borrow lives as long as `&self` and the `"worktree"` fallback is `&'static str`; keep the `-> &str` signature and the `#[allow(dead_code)]` attribute. Do NOT change the signature unless the borrow checker forces it — it does not here.

Update the doc-comments on BOTH functions to describe the new directory-only behavior. The current comments describe the old jira_key/branch/title priority chains and are now wrong. Each comment should state plainly that the function returns the worktree directory name (with a "worktree" fallback), and keep the existing "used for ... tab name / modal titles" usage notes.

CONSTRAINT — do NOT touch any JIRA logic: keep the `jira_key` and `jira_title` struct fields, `extract_jira_key`, the JIRA column, and ticket lookups in update.rs exactly as they are. Only these two function bodies + their doc-comments change. After this change `jira_key`/`jira_title` are still read elsewhere (e.g. the JIRA column), so removing them from these two functions does NOT make them unused.

Do NOT edit src/app/update.rs — the three consumers keep calling `wt.preferred_prefix()` and automatically pick up the new behavior.
  </action>
  <verify>
    <automated>cargo check --incremental 2>&1 | tail -5</automated>
  </verify>
  <done>Both functions return only the directory name (no jira_key/branch/jira_title branches remain in either body); doc-comments describe directory-only behavior; `cargo check --incremental` compiles clean; JIRA struct fields and extract_jira_key still present.</done>
</task>

<task type="auto">
  <name>Task 2: Update broken dispatch tests to assert the directory-name contract</name>
  <files>src/app/dispatch_tests.rs</files>
  <action>
Two assertions in `src/app/dispatch_tests.rs` assert the OLD branch-tier behavior and now break, because the fixture `seed_one_worktree_id("ump-dash")` produces path `/tmp/ump-dash` (directory name `ump-dash`) with branch `"main"`:

1. In `open_claude_code_opens_default_tab_without_suffix_prompt` (~line 3655): change `assert_eq!(name, "main-claude");` to `assert_eq!(name, "ump-dash-claude");`.
2. In `open_editor_terminal_mode_opens_configured_editor_in_multiplexer` (~line 3681): change `assert_eq!(name, "main-editor");` to `assert_eq!(name, "ump-dash-editor");`.

These now lock in the new contract: the tab name derives from the worktree directory name (`ump-dash`), not the branch (`main`).

Run a repo-wide check for any OTHER test that asserts a `-claude` / `-shell` / `-editor` name, or asserts directly on `preferred_prefix()` / `display_name()` output, and update any that encode the old jira_key/branch priority to assert the directory name instead. Scope: `grep -rn '\-claude\|\-shell\|\-editor\|preferred_prefix\|display_name' src/ --include='*.rs'`. Known non-targets to LEAVE UNCHANGED: `src/infra/multiplexer.rs` (literal `"app-claude"` passed in directly, not from preferred_prefix), and `src/infra/worktrees.rs:342` (`display_name: "Run UMP App"` field literal on an unrelated struct). If a worktree.rs inline unit test exists asserting the old priority, update it to assert the directory name too.
  </action>
  <verify>
    <automated>cargo test 2>&1 | tail -15</automated>
  </verify>
  <done>`cargo test` passes; the two dispatch assertions read `ump-dash-claude` / `ump-dash-editor`; no remaining test asserts the old jira_key/branch naming priority; multiplexer.rs and worktrees.rs:342 left untouched.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| (none new) | Pure internal rename of a display-string derivation; no new inputs cross any boundary. The directory name was already a value flowing into tab names via the fallback tier. |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-bow-01 | Information disclosure | preferred_prefix() / display_name() tab name | accept | Directory name is already user-visible (it is the path the user created); switching from jira_key/branch to dir name discloses nothing the user did not author. No PII surface change. |
| T-bow-02 | Tampering | npm/pip/cargo installs | mitigate | No new dependencies added; no install tasks in this plan. Vacuously satisfied. |
</threat_model>

<verification>
- `cargo check --incremental` compiles with no errors or new warnings.
- `cargo test` passes (full suite), including the updated `claude_tab` module assertions.
- `grep -n 'jira_key\|branch' src/domain/worktree.rs` shows `jira_key`/`branch` ONLY in the struct field declarations and their doc-comments — NOT inside `preferred_prefix()` or `display_name()` bodies.
- `grep -rn 'preferred_prefix\|display_name' src/app/update.rs` still shows the three unchanged consumer call sites.
</verification>

<success_criteria>
- Opening Claude/shell/editor on a worktree names the tab `<dir>-claude` / `<dir>-shell` / `<dir>-editor`, where `<dir>` is the worktree directory name, for every worktree regardless of jira_key or branch.
- `display_name()` and `preferred_prefix()` agree (both return the directory name).
- All JIRA fields and logic outside the two functions remain intact.
- Full test suite green.
</success_criteria>

<output>
Create `.planning/quick/260625-bow-when-opening-a-new-terminal-claude-etc-u/260625-bow-SUMMARY.md` when done.
</output>
