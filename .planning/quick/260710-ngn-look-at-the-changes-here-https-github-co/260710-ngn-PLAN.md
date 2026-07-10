---
status: ready
quick_id: 260710-ngn
slug: look-at-the-changes-here-https-github-co
description: Preserve dashboard MCP provisioning when UMP worktrees already contain the team-shared MCP configs introduced by PR 3992
created: 2026-07-10
files_modified:
  - src/infra/worktrees.rs
---

# Quick Task 260710-ngn: Make MCP provisioning compatible with UMP PR #3992

## Goal

When a dashboard-created UMP worktree already contains the tracked `.mcp.json` and `.codex/config.toml` introduced by `ALJAZEERAPLUS/ump#3992`, provision the local `ump-dash` endpoint alongside the six team servers without losing any server settings or the explanatory Codex TOML comments. Preserve the current behavior for fresh configs, existing personal configs, invalid configs, endpoint refreshes, disabled MCP provisioning, and non-clobbered run-app skills.

## Evidence

- PR #3992 adds tracked `.mcp.json` and `.codex/config.toml` files with the same six servers: `sauce-api-mcp-core`, `sauce-api-mcp-rdc`, `atlassian`, `bugsnag`, `figma`, and `amplitude`. The Codex config also carries Sauce startup timeouts, `figma.enabled = false`, and setup comments that must survive provisioning.
- `src/infra/worktrees.rs:197-224` already merges `ump-dash` semantically into JSON, and `src/infra/worktrees.rs:270-300` does the same for TOML. The existing tests cover only a single generic `other` server.
- `src/infra/worktrees.rs:390-423` reads and rewrites both config files after every new-worktree path. JSON values survive, but `toml::to_string_pretty` reconstructs the whole TOML document and drops PR #3992's comments, so a tracked team config is not preserved as authored.
- Baseline `cargo test infra::worktrees::tests::agent_ -- --nocapture` passes five existing fresh/generic/invalid merge tests. The compatibility gap therefore needs a PR-shaped regression, not a new app/domain abstraction.

## Architecture Notes

- Keep the change in `src/infra/worktrees.rs`: parsing and writing concrete Claude/Codex config files is an infra concern. Do not move file IO into `domain`, `app::update()`, or `ui`.
- Keep provisioning best-effort and non-fatal. A config merge/write failure must continue to log and leave worktree creation successful.
- Do not add a new parsing dependency for this focused compatibility change. Continue using the existing `serde_json` and `toml` validation paths.

## Tasks

### Task 1 - Add PR #3992 compatibility regressions first

Files:
- `src/infra/worktrees.rs`

Action:
- Add focused inline tests beside the existing `agent_mcp_json_*`, `agent_codex_config_*`, and provisioning tests. Use stable fixtures that reproduce the two PR #3992 configs observed on 2026-07-10 rather than reaching GitHub during the test.
- For `.mcp.json`, include all six PR servers and their relevant transport fields. After `agent_mcp_json(..., 8790)`, assert the document has those six unchanged plus `mcpServers.ump-dash` with `type = "http"` and `url = "http://127.0.0.1:8790/mcp"`. In particular, protect the wrapper command/args for both Sauce servers and BugSnag and the three remote URLs.
- For `.codex/config.toml`, include all six PR servers, the two `startup_timeout_sec = 60` values, `figma.enabled = false`, and representative leading/per-server comments from the PR. After `agent_codex_config(..., 8790)`, parse and assert every team server setting remains unchanged, the dashboard block is present, and the representative comments remain in the returned text.
- Add refresh regressions for both formats: when an existing `ump-dash` entry points at an old port, only that entry is refreshed and sibling servers/sections survive. Add explicit TOML recovery coverage for invalid input and a non-table `mcp_servers` value. Retain the existing fresh, invalid-JSON replacement, generic merge, provisioning-disabled, and run-app-skill non-clobber tests unchanged.
- Run the targeted tests before production changes. The new JSON semantic assertions may already pass; the PR-shaped TOML comment-preservation assertion must fail against the current whole-document serializer, providing the red case.

Verify:
- `cargo test infra::worktrees::tests::agent_ -- --nocapture` fails specifically because PR #3992's TOML comments are removed.

Done:
- Tests precisely encode PR #3992's six-server shapes and fail on the destructive TOML rewrite while locking all previously supported merge/refresh cases.

### Task 2 - Preserve authored team config while upserting ump-dash

Files:
- `src/infra/worktrees.rs`

Action:
- Refactor `agent_codex_config` so valid existing TOML is not round-tripped wholesale through `toml::to_string_pretty`. Validate and inspect it with the existing `toml` parser, then append a canonical `[mcp_servers.ump-dash]` block when the dashboard server is absent. This preserves PR #3992's comments, ordering, commands, args, timeouts, disabled flag, and unrelated sections byte-for-byte.
- When the canonical dashboard section already exists, replace only that section through the next TOML table header so a changed dashboard port refreshes `url`, `enabled`, `required`, `startup_timeout_sec`, and `tool_timeout_sec` without touching sibling sections. Accept the quoted/unquoted canonical `ump-dash` header forms. If a valid document expresses the existing dashboard entry in a non-canonical form that cannot be safely isolated, fall back to the current parsed-table replacement rather than appending a duplicate table.
- Preserve the current recovery contract: `None` or invalid TOML produces a fresh valid config containing the canonical dashboard block; a non-table `mcp_servers` value is normalized as today; output ends with a newline; no panic or worktree-creation failure is introduced.
- Keep `agent_mcp_json`'s validated semantic merge, now protected with the exact PR fixture. Update `write_merged` to skip `std::fs::write` when the merged bytes equal the existing bytes, while retaining best-effort logging/error behavior when a real change is required.
- Update nearby comments to state the actual guarantee: PR/team server semantics are preserved for JSON, while valid Codex TOML also preserves unrelated authored text/comments. Do not change app/domain ports or the three `GitWorktreeAdapter::add*` provisioning call sites.

Verify:
- `cargo test infra::worktrees::tests::agent_ -- --nocapture`
- `cargo test infra::worktrees::tests::provision_ -- --nocapture`
- `make arch-lint`

Done:
- A PR #3992-shaped worktree retains all six team MCP servers and Codex guidance while gaining/refreshing the local `ump-dash` endpoint.
- Fresh and invalid inputs still recover to valid dashboard configs; existing dashboard endpoints refresh; MCP-disabled worktrees remain untouched; existing run-app skills remain non-clobbered.
- The change remains entirely inside the infra adapter and repeated provisioning avoids an unnecessary identical write.

## Verification

Run all repository-required checks:

- `make arch-lint`
- `make arch-report`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`

## Success Criteria

- Both exact PR #3992 config shapes coexist with the injected `ump-dash` server.
- No Sauce Labs, Atlassian, BugSnag, Figma, or Amplitude field changes during provisioning; Figma remains disabled in Codex.
- PR #3992's Codex TOML comments survive provisioning.
- All prior fresh/generic/invalid/refresh/disabled/non-clobber cases stay green.
- No architecture guard regression is reported.

## Out of Scope

- No changes to UMP PR #3992 itself or to its server inventory/auth wrapper.
- No changes to MCP transport/tool behavior in `src/infra/mcp_server.rs`.
- No `ROADMAP.md` or `.planning/STATE.md` edits.
