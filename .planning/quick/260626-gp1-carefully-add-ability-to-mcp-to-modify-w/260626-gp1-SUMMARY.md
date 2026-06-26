---
status: complete
quick_id: 260626-gp1
date: 2026-06-26
commit: 5abd72d
---

# Quick Task 260626-gp1 Summary

Added MCP worktree mutation support without bypassing the app reducer:

- Added `AgentRequest::CreateWorktree` and `AgentRequest::DeleteWorktree`, plus `AgentOutcome::WorktreeOperationStarted`.
- Added `create_worktree` and `delete_worktree` MCP tools in `src/infra/mcp_server.rs`.
- Routed create/delete through `src/app/update.rs` so the dashboard still owns worktree effects, in-flight operation guards, confirmation checks, and main-worktree deletion refusal.
- Updated generated run-app skill text to mention the new worktree-management tools.
- Added dispatch and protocol round-trip coverage.
- Fixed a small pre-existing clippy test-table lint in `src/domain/command.rs` so the required clippy gate passes.

Verification:

- `cargo test --lib agent_requests`
- `cargo test --lib agent_protocol::tests`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings`
- `make arch-lint`
- `make arch-report`
