# Android Native Build Cache Prototype Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Android APK native build caching to `ump-dash`, letting a selected worktree install a cached debug APK and launch it against that worktree's Metro port without rebuilding native Android.

**Architecture:** Extend the existing iOS native cache path rather than creating a parallel subsystem. Domain owns Android fingerprint, metadata, state, and request/result types; infra owns APK discovery, cache storage, ADB command execution, and preference retargeting; app/update owns Android palette/device/Metro orchestration; UI reads per-worktree Android cache state through configurable columns.

**Tech Stack:** Rust 2024, existing TEA-style `Action`/`Effect` flow, Tokio process execution, serde/serde_json, sha2 fingerprints, Ratatui table columns, ADB.

---

## File Structure

- Modify `src/domain/native_cache.rs`: add Android constants, metadata, lookup state, pending launch state, request/result types, and fingerprint helper.
- Modify `src/domain/ports/native_cache_port.rs`: add Android lookup, store, and install/launch methods.
- Modify `src/infra/native_cache.rs`: add Android cache root helpers, APK discovery from `output-metadata.json`, metadata validation, ADB command builders, and cached launch executor.
- Modify `src/domain/worktree_slice.rs`: add Android cache state and pending cached launch.
- Modify `src/app/state.rs`: add pending cached Android run modal state.
- Modify `src/domain/action.rs`: add Android cache lookup/run/launch completion actions.
- Modify `src/app/effect.rs` and `src/app/effect_runner.rs`: add Android cache effects and runner branches.
- Modify `src/app/keybindings.rs`: add Android palette `c cached` shortcut and matching-hit selection.
- Modify `src/app/update.rs`: queue Android cache lookups, store APKs after successful Android runs, and orchestrate cached Android target selection plus Metro prerequisite.
- Modify `src/domain/dash_config.rs` and `src/ui/panels.rs`: add `android_cache` and `android_cache_status` columns.
- Update inline tests in each touched module.

## Task 1: Domain Android Cache Types

- [ ] Add tests in `src/domain/native_cache.rs` proving Android fingerprint uses `yarn.lock`, `package.json`, `android/settings.gradle`, `android/build.gradle`, and `android/app/build.gradle`, and that Android cache state exposes hits only.
- [ ] Run `cargo test domain::native_cache::tests::android_ -- --nocapture` and confirm the new tests fail to compile because Android types/helpers do not exist.
- [ ] Add `ANDROID_PLATFORM`, `ANDROID_APK_ARTIFACT_KIND`, `ANDROID_FINGERPRINT_FILES`, `AndroidCacheMetadata`, `AndroidCacheHit`, `AndroidCacheLookup`, `AndroidCacheState`, `AndroidCacheStoreRequest`, `PendingCachedAndroidLaunch`, `CachedAndroidLaunchRequest`, `CachedAndroidLaunchResult`, and `android_native_fingerprint`.
- [ ] Re-run `cargo test domain::native_cache::tests::android_ -- --nocapture` and confirm the tests pass.

## Task 2: Infra Android Cache Lookup And Store

- [ ] Add tests in `src/infra/native_cache.rs` for Android lookup hit/miss/missing APK and for storing the newest APK using nearby `output-metadata.json`.
- [ ] Run the targeted infra Android tests and confirm they fail before implementation.
- [ ] Implement `android_entry_dir`, metadata validation, Android lookup, APK output metadata parsing, APK discovery, and Android store copy/write logic.
- [ ] Extend `NativeCachePort` and `LocalNativeCache` with `lookup_android`, `store_android`, and `install_and_launch_android`.
- [ ] Re-run targeted infra tests and confirm they pass.

## Task 3: ADB Launch Helpers

- [ ] Add tests for pure Android command/helper behavior: install args, reverse args, activity parsing, `debug_http_host` XML insertion/update, and launch args.
- [ ] Run the targeted helper tests and confirm they fail before implementation.
- [ ] Implement pure helpers plus the async ADB launch sequence: `adb install -r -d`, preference XML pull/edit/push/copy through `run-as`, `adb reverse`, launcher resolution, and `am start`.
- [ ] Re-run targeted helper tests and confirm they pass.

## Task 4: App Flow And Effects

- [ ] Add dispatch tests mirroring iOS: worktrees load starts Android lookup, Android palette lookup, lookup result maps to slice state, matching miss uses another worktree hit, `a>c` loads Android targets, target selection starts or reuses Metro, deferred launch fires on Metro ready, Metro exit clears pending launch, successful Android run stores cache, failed Android run does not store cache.
- [ ] Run targeted dispatch tests and confirm they fail before implementation.
- [ ] Add Android cache fields/actions/effects and update `update.rs`, `effect.rs`, `effect_runner.rs`, `state.rs`, and `worktree_slice.rs` to satisfy the flow.
- [ ] Re-run targeted dispatch tests and confirm they pass.

## Task 5: Android Palette Shortcut And Columns

- [ ] Add tests in `src/app/dispatch_tests.rs`, `src/domain/dash_config.rs`, and `src/ui/panels.rs` for Android `c cached` visibility plus `android_cache` and `android_cache_status` config/rendering.
- [ ] Run targeted tests and confirm they fail before implementation.
- [ ] Add keybinding helper functions and column enum/rendering support while keeping existing `cache` and `cache_status` iOS-backed.
- [ ] Re-run targeted tests and confirm they pass.

## Task 6: Full Verification

- [ ] Run `cargo fmt`.
- [ ] Run `cargo test`.
- [ ] Inspect `git diff --check` and `git status --short`.
- [ ] Dispatch a subagent review of the complete implementation for spec compliance and likely runtime issues.
- [ ] Fix any review findings and re-run affected tests.
- [ ] Commit the implementation after verification passes.
