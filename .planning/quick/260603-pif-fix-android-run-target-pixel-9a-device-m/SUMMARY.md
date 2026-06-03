---
status: complete
quick_id: 260603-pif
slug: fix-android-run-target-pixel-9a-device-m
date: 2026-06-03
---

# Summary

Fixed Android AVD runs so selecting `Pixel_9a` no longer passes that AVD name directly to the React Native command when the CLI expects an adb serial such as `emulator-5554`.

## Completed

- Stopped Android AVD picker targets are stored as `avd:<name>` so they are distinct from connected adb serials.
- Running emulator entries are annotated with their AVD name and matching stopped AVD entries are no longer duplicated as available.
- Booting a stopped AVD now waits until that specific AVD is visible through adb before draining the queued run.
- A queued Android run for `avd:<name>` resolves the live emulator serial through `adb -s <serial> emu avd name` before invoking `yarn android:<variant> --device <serial>`.
- Added focused tests for AVD target marking, adb AVD-name parsing, dynamic serial resolution, and the reducer boot queue path.

## Verification

- `cargo test --all-targets`
- `cargo check`
- `make arch-lint` (PASS; emitted existing G-19 coverage warning)
- `adb devices -l`
- `emulator -list-avds`
- `adb -s emulator-5554 emu avd name`
