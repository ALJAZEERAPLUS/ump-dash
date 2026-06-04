---
status: in-progress
date: 2026-06-03
slug: fix-android-run-target-pixel-9a-device-m
---

Fix Android run target handling when an AVD such as Pixel_9a is selected but React Native expects an adb serial such as emulator-5554.

## Plan

1. Reproduce the bad argv path with focused tests.
2. Mark stopped Android AVD picker targets distinctly from connected adb serials.
3. Resolve a booted AVD to its adb serial before running the Android package script.
4. Verify focused tests and the broader Rust test suite.
