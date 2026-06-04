---
status: complete
quick_id: 260531-awm
slug: let-s-remove-the-lock-from-metro-now-whe
date: 2026-05-31
---

# Summary

Removed the Metro port lock and changed Metro lifecycle tracking from one global handle to per-worktree handles.

## Completed

- Normal Metro start no longer probes/blocks on external ownership of port 8081.
- New Metro launches choose the next available port from 8081 and pass it to `yarn start:rozenite --reset-cache --port <port>`.
- Metro handles carry their selected port, and reload/debug HTTP actions target the selected worktree's running Metro port.
- Starting Metro on worktree A leaves an existing Metro on worktree B running.
- Worktree table status/detail rows are derived per worktree and display the active port.
- Background Metro activity, exit, and spawn failure actions now carry `worktree_id` so one Metro process does not clear all Metro state.

## Verification

- `cargo check`
- `cargo test`
- `make arch-lint`
