---
status: complete
quick_id: 260528-fec
date: 2026-05-28
commit: b093803
---

# Quick Task 260528-fec: UMP run keychords

## Summary

- Added typed UMP Android/iOS run commands that invoke `yarn android:{local,dev,prod}` and `yarn ios:{local,dev,prod}` with selected target flags.
- Changed `a r` and `i r` to start a target-first run flow, followed by a run-type picker ordered `local`, `dev`, `prod`.
- Switched Metro process spawning to `yarn start:rozenite --reset-cache`.

## Verification

- `cargo test --lib ump_run`
- `cargo test`
- `make arch-lint`

## Code Commit

`b093803` — `feat(quick-260528-fec): add UMP run keychords`
