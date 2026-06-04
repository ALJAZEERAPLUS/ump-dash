# Android Native Build Cache Prototype Design

Date: 2026-06-04
Status: Ready for user review

## Goal

Add the Android side of the native build cache prototype inside `ump-dash`: after a successful normal Android run, store the built debug APK under a native fingerprint, then allow another worktree with the same fingerprint to install that APK and launch it against that worktree's Metro port without rebuilding native Android.

This mirrors the iOS simulator cache prototype, but the artifact and launch mechanics are Android-specific.

## Prototype Result

The risky Android path has been tested manually before implementation:

1. Install an existing local debug APK with `adb install -r -d`.
2. Set React Native Android's `debug_http_host` preference to `localhost:<metro-port>` using `run-as <application-id>`.
3. Run `adb reverse tcp:<metro-port> tcp:<metro-port>`.
4. Launch the installed main activity.
5. Confirm logcat shows the app loading JavaScript from the selected non-default Metro port.

The prototype confirms that a cached APK can be reused across local worktrees while still loading JavaScript from the active worktree.

## Decisions

- Cache scope remains local machine only.
- Android cache is for debug APKs launched through ADB.
- Cache hits skip Gradle and React Native Android build steps entirely.
- Metro port is runtime state and is not part of the fingerprint.
- Android cache state is separate from iOS cache state.
- Existing normal Android run behavior remains unchanged.
- Existing `cache` and `cache_status` columns remain iOS-backed for compatibility.
- New configurable `android_cache` and `android_cache_status` columns expose Android cache keys and availability.

## User Experience

When the user opens the Android palette on a selected worktree:

1. `ump-dash` computes the selected worktree's Android native fingerprint.
2. It checks the local Android artifact cache for that fingerprint.
3. If a matching APK exists, the Android palette shows `c cached`.
4. Pressing `a>c` opens the existing Android target picker.
5. If the selected target is an available AVD, `ump-dash` boots it and resolves the running ADB serial.
6. `ump-dash` ensures Metro is running for the selected worktree.
7. It installs the cached APK into the selected device or emulator.
8. It sets React Native Android's `debug_http_host` preference to `localhost:<metro-port>`.
9. It runs `adb reverse tcp:<metro-port> tcp:<metro-port>`.
10. It launches the installed app's main activity.

If no matching Android cache hit exists, the `c` shortcut is hidden. Normal `a>r` Android runs remain unchanged and continue to populate `last_android_run`.

## Cache Shape

Cache root:

```text
~/.cache/ump-dash/native-builds/android/<fingerprint>/
```

Each entry contains:

```text
artifact.apk
metadata.json
```

`metadata.json` contains:

```json
{
  "platform": "android",
  "fingerprint": "sha256...",
  "application_id": "com.aljazeera.mobile.local",
  "variant": "localDebugOptimized",
  "created_at": "2026-06-04T00:00:00Z",
  "source_worktree": "/absolute/path/to/worktree",
  "artifact_kind": "apk"
}
```

`launch_activity` may be added when it is known, but the cached launch can also resolve the launcher activity after install with:

```text
adb -s <serial> shell cmd package resolve-activity --brief <application-id>
```

## Fingerprint

Android fingerprint rules are isolated behind an Android-specific helper so the policy can change later without touching UI or launch flow.

Initial inputs:

- `yarn.lock`
- `package.json`
- `android/settings.gradle`
- `android/build.gradle`
- `android/app/build.gradle`

This is intentionally narrow. It avoids hashing generated Gradle output and app JavaScript while still catching the core native dependency and Android build wiring changes. Future revisions can add Gradle version catalogs, lockfiles, native source globs, config plugin outputs, or explicit exclude rules if the MVP produces false hits or false misses.

## Cache Population

After a successful normal Android run, `ump-dash` stores the newest APK from:

```text
android/app/build/outputs/apk/**
```

The store path prefers an APK with nearby `output-metadata.json`, because that file exposes the `applicationId`, `variantName`, and `outputFile` for the produced artifact. If metadata is missing or malformed, storage should fail clearly in the worktree output rather than storing an ambiguous APK.

For the current UMP local build, the expected artifact shape is:

```text
android/app/build/outputs/apk/local/debugOptimized/app-local-debugOptimized.apk
android/app/build/outputs/apk/local/debugOptimized/output-metadata.json
```

## Install And Launch

Cached Android launch is owned by infra behind the native cache port.

Expected sequence:

1. If the target is an AVD id, boot it and resolve its running ADB serial.
2. Install:

```text
adb -s <serial> install -r -d <artifact.apk>
```

3. Update React Native Android's debug host preference:

```text
shared_prefs/<application-id>_preferences.xml
debug_http_host = localhost:<metro-port>
```

The reliable implementation is host-side XML edit plus `adb push` and `run-as <application-id> cp`, because direct shell redirection into the app sandbox is not reliable.

4. Reverse Metro:

```text
adb -s <serial> reverse tcp:<metro-port> tcp:<metro-port>
```

5. Resolve launcher activity:

```text
adb -s <serial> shell cmd package resolve-activity --brief <application-id>
```

6. Launch:

```text
adb -s <serial> shell am start -n <component-name>
```

If `run-as` fails, cached Android launch should fail closed with a clear `[cached-android error]` line. Debug APKs are expected to support `run-as`.

## Architecture

Follow the existing ports-and-effects style used by the iOS native cache:

- Domain owns Android cache metadata, lookup state, pending launch state, and request/result types.
- App/update owns flow decisions: palette visibility, target selection, AVD boot handling, Metro prerequisite, and effect dispatch.
- Infra owns filesystem cache lookup, APK discovery, metadata validation, ADB install, preference update, reverse, and launch.
- UI reads worktree slice cache state and should not inspect the filesystem directly.

Likely additions:

- Android cache types in `src/domain/native_cache.rs`.
- Android cache methods on `NativeCachePort`.
- Android cache lookup, store, and install/launch effects.
- Worktree slice fields for Android cache state and pending cached Android launch.
- Android palette `c` shortcut visible only when an Android cache hit exists for the selected fingerprint.
- Configurable `android_cache` and `android_cache_status` table columns.

## Error Handling

Errors should be appended to the origin worktree output:

- Cache metadata is malformed.
- Cached APK is missing.
- Application id is missing.
- Selected AVD cannot be booted or resolved to an ADB serial.
- APK install fails.
- `run-as` preference update fails.
- `adb reverse` fails.
- Launcher activity cannot be resolved.
- Activity launch fails.
- Metro is not running and cannot be started.

The cached path must not fall back to a normal Android build. The user can still run `a>r`.

## Testing

Unit tests should cover:

- Android fingerprint helper uses the declared inputs.
- Android metadata parse and validation.
- Android cache lookup returns hit, miss, and missing-artifact errors.
- Store copies the expected APK and writes metadata from `output-metadata.json`.
- Android palette `c` visibility depends on Android cache-hit state.
- Cached Android run follows target selection and Metro prerequisite flow.
- Cached Android launch uses an existing Metro process even if ready activity is not available.
- Android cache columns render hit, miss, checking, error, and unknown states.
- ADB command builders include serial, APK path, application id, port, and component name.

Manual verification should cover:

1. Build Android once through normal `a>r`.
2. Confirm the APK is stored in the Android cache.
3. Select another worktree with the same Android fingerprint.
4. Press `a>c`.
5. Select a running emulator or available AVD.
6. Confirm the cached APK installs and launches.
7. Confirm the launched app connects to that worktree's Metro port.

## Out Of Scope

- Android release APK caching.
- Android App Bundle caching.
- Physical device differences beyond normal ADB serial support.
- Shared CI cache.
- Cache eviction or clear-cache UI.
- Complete native-input fingerprint policy.
- General build directory snapshot caching.
