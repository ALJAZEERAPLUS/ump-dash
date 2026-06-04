---
status: complete
quick_id: 260531-awm
slug: let-s-remove-the-lock-from-metro-now-whe
date: 2026-05-31
---

# Quick Task 260531-awm: Remove Metro lock and use next available port

## Goal

Starting Metro should no longer be blocked by another process already using port 8081. A new Metro launch should choose an explicit available port, starting at 8081 and incrementing until a free port is found.

## Tasks

1. Lock current behavior with failing tests
   - Update the Metro-start reducer tests so `Action::MetroStart` emits a spawn path directly instead of an external-port conflict probe.
   - Add/update infra process tests so the Metro command includes an explicit `--port <port>`.
   - Verify the tests fail before implementation.

2. Implement available-port startup
   - Remove the startup-time external Metro lock path for normal Metro launches.
   - Add available-port selection in the infra process adapter and pass the chosen port to the Metro command.
   - Carry the selected port through the Metro handle/status so reload/debug controls target the live Metro port.

3. Verify and record
   - Run focused Metro tests and the relevant full Rust test suite.
   - Write the quick-task summary and update `.planning/STATE.md`.

## Scope Added During Execution

- Metro lifecycle state is now per worktree. Starting Metro for one worktree no longer stops Metro already running for another worktree.
- The worktree UI shows the selected port for each running Metro instance.
