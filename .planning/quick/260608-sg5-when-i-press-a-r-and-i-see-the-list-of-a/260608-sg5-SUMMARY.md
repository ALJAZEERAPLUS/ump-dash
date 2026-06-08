---
status: complete
quick_id: 260608-sg5
date: 2026-06-08
commit: ad689e9
---

# Quick Task 260608-sg5: Android physical devices in run picker

## Summary

- Added a regression test for space-separated `adb devices -l` rows containing a physical Android device.
- Updated Android ADB parsing to split rows on any whitespace instead of requiring a tab between serial and state.
- Confirmed the connected device output on this machine uses the space-separated format that was previously skipped.

## Verification

- `cargo test infra::devices::tests`
- `cargo check`
- `adb devices -l`

## Code Commit

`ad689e9` — `fix(quick-260608-sg5): include physical Android adb devices`
