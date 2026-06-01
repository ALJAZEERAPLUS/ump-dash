# iOS Native Build Cache Prototype Design

Date: 2026-05-31
Status: Ready for user review

## Goal

Prototype the riskiest part of native build caching inside `ump-dash`: install a previously built iOS simulator `.app` from a local cache and launch it against the selected worktree's Metro port, without running the native iOS build command again.

This is a feasibility prototype, not the full native build cache system. It should prove whether a cached simulator artifact can be reused across worktrees while still loading JavaScript from the active worktree's Metro instance.

## Decisions

- Cache scope is local machine only.
- First prototype is iOS simulator only.
- Cache hits skip native build entirely.
- Cached artifacts are installable `.app` bundles, not restored Xcode build directories.
- Metro port is runtime state and is not part of the artifact fingerprint.
- Fingerprint policy must be isolated behind a helper so it can change from narrow inputs such as `yarn.lock` to richer native-only globs later.
- Existing UMP run commands remain the only normal native run path.
- Legacy `react-native run-ios` / `run-android` command variants are not part of the design.

## User Experience

When the user opens the iOS palette on a selected worktree:

1. `ump-dash` computes the selected worktree's iOS native fingerprint.
2. It checks the local artifact cache for that fingerprint.
3. If a matching cached simulator artifact exists, the iOS palette shows a temporary `c` shortcut, labelled as cached run/install.
4. Pressing `i>c` opens the existing iOS simulator picker.
5. After the user selects a simulator, `ump-dash` ensures Metro is running for that worktree.
6. It installs the cached `.app` into the selected simulator.
7. It launches the app with the selected worktree's Metro port configured.

If no cache hit exists, the `c` shortcut is hidden. The normal `i>r` UMP run flow remains unchanged.

The worktree table shows a final `cache` column by default. It displays the first eight native fingerprint characters for both cache hits and cache misses, `...` while lookup is in progress, `err` for lookup errors, and `-` only for unknown state.

## Cache Shape

Cache root:

```text
~/.cache/ump-dash/native-builds/ios-simulator/<fingerprint>/
```

Each entry contains:

```text
artifact.app/
metadata.json
```

`metadata.json` contains:

```json
{
  "platform": "ios-simulator",
  "fingerprint": "sha256...",
  "bundle_id": "com.example.app",
  "variant": "local",
  "created_at": "2026-05-31T00:00:00Z",
  "source_worktree": "/absolute/path/to/worktree",
  "artifact_kind": "app-bundle"
}
```

After a successful normal iOS run, the dashboard stores the built simulator `.app` in the cache directory for the worktree's native fingerprint and writes `metadata.json`. Manual seeding remains useful for debugging, but is not required for the normal prototype flow.

## Fingerprint

The prototype should define an `IosNativeFingerprint` helper that owns all fingerprint rules. Callers should not know whether the fingerprint currently uses `yarn.lock`, native folders, or an explicit file list.

Initial inputs are intentionally conservative and easy to change:

- `yarn.lock`
- `package.json`
- `ios/Podfile`

The prototype deliberately excludes `ios/Podfile.lock` because it can create false misses while we are still proving install/launch feasibility. It also does not hash all of `ios/`. Once the launch path is proven, the helper can expand to native-only globs such as `ios/**/*.swift`, `ios/**/*.m`, `ios/**/*.mm`, `ios/**/*.h`, `ios/**/*.xcodeproj/**`, and config files, while excluding heavy/generated paths such as `ios/Pods`, `ios/build`, `xcuserdata`, and DerivedData.

JavaScript and TypeScript app source should not invalidate the cache by default, because Metro serves JS at runtime.

The helper must be deliberately small and covered by tests that make future policy changes cheap.

## Install And Launch

The install/launch logic should be isolated behind an iOS simulator artifact runner.

Expected command shape:

1. Install:

```text
xcrun simctl install <simulator-udid> <cached-app-path>
```

2. Launch:

```text
SIMCTL_CHILD_RCT_METRO_PORT=<port> xcrun simctl launch --terminate-running-process <simulator-udid> <bundle-id>
```

`SIMCTL_CHILD_RCT_METRO_PORT` is the first launch strategy because `simctl launch` passes `SIMCTL_CHILD_` variables into the app process, and React Native iOS uses `RCT_METRO_PORT` to derive the packager port. The command builder must keep this isolated so the strategy can be revised if the prototype proves that environment injection is not enough for UMP's current iOS app.

If launch succeeds but the app does not connect to the selected Metro port, the prototype should report that clearly in the worktree output. That result is useful: it tells us the full cache system needs another launch-time configuration strategy before production work.

## Architecture

The prototype should follow the existing ports-and-effects style:

- Domain owns cache key types, metadata structs, and command specs.
- App/update owns flow decisions: palette visibility, simulator selection, Metro prerequisite, and effect dispatch.
- Infra owns filesystem cache lookup, artifact install, and simulator launch commands.
- UI reads existing keybinding visibility hooks and should not inspect the filesystem directly.

Likely additions:

- A domain module for native build cache types.
- A domain port for native build cache lookup and artifact execution, or an iOS-specific prototype port if smaller.
- App effects for cache lookup and cached iOS launch.
- A temporary iOS palette keybinding for `c`, visible only when cache state says a hit exists.
- Worktree-slice state for cache-hit availability and lookup status.

## Error Handling

Errors should be appended to the selected worktree output:

- No cache entry found after a visible shortcut was selected.
- Cache metadata is malformed.
- `.app` path is missing.
- Bundle id is missing.
- Simulator install fails.
- Simulator launch fails.
- Metro is not running and cannot be started.

The prototype should fail closed: if anything is uncertain, do not run a native build as a fallback. The user can still use normal `i>r`.

## Testing

Unit tests should cover:

- Fingerprint helper produces stable keys for a controlled fixture.
- Cache metadata parse and validation.
- iOS palette `c` visibility depends on cache-hit state.
- `i>c` follows simulator selection and Metro prerequisite flow.
- Install/launch command builder includes simulator UDID, bundle id, app path, and Metro port.

Manual verification should cover:

1. Seed a cache entry for a simulator `.app`.
2. Start or select a worktree whose Metro uses a non-8081 port.
3. Press `i>c`.
4. Select a simulator.
5. Confirm the cached `.app` installs.
6. Confirm the launched app loads JavaScript from the selected worktree's Metro port.

## Out Of Scope

- Android caching.
- Physical iOS devices.
- Cache eviction or clear-cache UI.
- CI/shared cache behavior.
- Complete native-input fingerprint policy.
- Build-folder snapshot caching.

## Prototype Cache Population

The normal flow is:

1. Build the UMP iOS simulator app once through the normal `i>r` flow.
2. On successful command exit, the dashboard locates the newest matching simulator `.app` from `ios/build/Build/Products` or Xcode DerivedData.
3. The dashboard copies the app to:

```text
~/.cache/ump-dash/native-builds/ios-simulator/<fingerprint>/artifact.app
```

4. The dashboard writes `metadata.json` with the bundle id, variant, source worktree, and matching fingerprint.
5. Worktrees that already show the same fingerprint can use the same cache hit.

To inspect or manually seed a local iOS simulator cache entry, compute the fingerprint from the worktree root:

```bash
cargo test domain::native_cache::tests::print_current_worktree_ios_fingerprint -- --ignored --nocapture
```

Expected output includes one line shaped like:

```text
ios-simulator fingerprint for /absolute/path/to/worktree: <64-hex-character-sha256>
```
