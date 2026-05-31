# iOS Native Build Cache Prototype Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an iOS-simulator-only prototype path in `ump-dash` that installs a manually seeded cached `.app` artifact and launches it against the selected worktree's Metro port without running the native build.

**Architecture:** Follow the existing TEA and ports/adapters structure. Domain owns cache metadata, fingerprint, and request/result types; app/update owns palette/device/Metro orchestration; infra owns cache filesystem lookup and `xcrun simctl` execution.

**Tech Stack:** Rust 2024, Ratatui keybinding registry, Tokio process execution, serde/serde_json, sha2 for stable fingerprints, existing `EffectRunner` and `Adapters` ports.

---

## File Structure

- Create `src/domain/native_cache.rs`: pure-ish data types plus the isolated iOS fingerprint helper. It may read files like existing `domain::staleness`.
- Create `src/domain/ports/native_cache_port.rs`: trait boundary for cache lookup and cached iOS install/launch.
- Create `src/infra/native_cache.rs`: local cache adapter, metadata parser, cache root helper, and `xcrun simctl` command builder/executor.
- Modify `src/domain/mod.rs`, `src/domain/ports/mod.rs`, `src/infra/mod.rs`: module registration.
- Modify `src/app/adapters.rs`, `src/main.rs`: inject `Arc<dyn NativeCachePort>`.
- Modify `src/app/effect.rs`, `src/app/effect_runner.rs`: add lookup and install/launch effects.
- Modify `src/domain/action.rs`: add cache lookup and cached launch actions.
- Modify `src/domain/worktree_slice.rs`: add per-worktree iOS cache state and pending cached launch state.
- Modify `src/app/state.rs`: add pending cached iOS run field to modal stack.
- Modify `src/app/keybindings.rs`: add temporary `i>c` cached shortcut, visible only for active cache hits.
- Modify `src/app/update.rs`: trigger lookup on iOS palette entry, route `i>c` through device picker, enforce Metro prerequisite, launch cached artifact when Metro is ready.
- Tests live inline with the files they cover, following the repo's existing pattern.

## Task 1: Domain Cache Types And Fingerprint

**Files:**
- Modify: `Cargo.toml`
- Create: `src/domain/native_cache.rs`
- Modify: `src/domain/mod.rs`

- [ ] **Step 1: Add fingerprint dependency**

Add `sha2` to `[dependencies]` in `Cargo.toml`:

```toml
sha2 = "0.10"
```

- [ ] **Step 2: Create failing domain tests**

Create `src/domain/native_cache.rs` with the test module first:

```rust
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const IOS_SIMULATOR_PLATFORM: &str = "ios-simulator";
pub const IOS_APP_ARTIFACT_KIND: &str = "app-bundle";

pub const IOS_FINGERPRINT_FILES: &[&str] = &[
    "yarn.lock",
    "package.json",
    "ios/Podfile",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IosSimulatorCacheMetadata {
    pub platform: String,
    pub fingerprint: String,
    pub bundle_id: String,
    pub variant: String,
    pub created_at: String,
    pub source_worktree: String,
    pub artifact_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IosSimulatorCacheHit {
    pub metadata: IosSimulatorCacheMetadata,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IosSimulatorCacheState {
    Unknown,
    Checking,
    Hit(IosSimulatorCacheHit),
    Miss,
    Error(String),
}

impl Default for IosSimulatorCacheState {
    fn default() -> Self {
        Self::Unknown
    }
}

impl IosSimulatorCacheState {
    pub fn hit(&self) -> Option<&IosSimulatorCacheHit> {
        match self {
            Self::Hit(hit) => Some(hit),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCachedIosLaunch {
    pub device_id: String,
    pub cache_hit: IosSimulatorCacheHit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedIosLaunchRequest {
    pub simulator_udid: String,
    pub app_path: PathBuf,
    pub bundle_id: String,
    pub metro_port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachedIosLaunchResult {
    Success(Vec<String>),
    Failure(String),
}

pub fn ios_native_fingerprint(worktree_path: &Path) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    for rel in IOS_FINGERPRINT_FILES {
        hasher.update(rel.as_bytes());
        hasher.update([0]);
        let path = worktree_path.join(rel);
        match std::fs::read(&path) {
            Ok(bytes) => {
                hasher.update(b"present");
                hasher.update((bytes.len() as u64).to_le_bytes());
                hasher.update(bytes);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                hasher.update(b"missing");
            }
            Err(e) => return Err(e.into()),
        }
        hasher.update([0xff]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_worktree() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "ump-dash-ios-cache-fingerprint-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(path.join("ios")).unwrap();
        path
    }

    #[test]
    fn ios_fingerprint_uses_declared_inputs_and_excludes_podfile_lock() {
        let dir = temp_worktree();
        fs::write(dir.join("yarn.lock"), "a").unwrap();
        fs::write(dir.join("package.json"), "{}").unwrap();
        fs::write(dir.join("ios/Podfile"), "platform :ios").unwrap();
        fs::write(dir.join("ios/Podfile.lock"), "lock-a").unwrap();

        let first = ios_native_fingerprint(&dir).unwrap();
        fs::write(dir.join("ios/Podfile.lock"), "lock-b").unwrap();
        let after_lock_change = ios_native_fingerprint(&dir).unwrap();
        fs::write(dir.join("ios/Podfile"), "platform :ios, '17.0'").unwrap();
        let after_podfile_change = ios_native_fingerprint(&dir).unwrap();

        assert_eq!(first, after_lock_change);
        assert_ne!(first, after_podfile_change);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn cache_state_hit_helper_returns_only_hits() {
        let hit = IosSimulatorCacheHit {
            metadata: IosSimulatorCacheMetadata {
                platform: IOS_SIMULATOR_PLATFORM.into(),
                fingerprint: "abc".into(),
                bundle_id: "com.example.app".into(),
                variant: "local".into(),
                created_at: "2026-05-31T00:00:00Z".into(),
                source_worktree: "/tmp/wt".into(),
                artifact_kind: IOS_APP_ARTIFACT_KIND.into(),
            },
            artifact_path: PathBuf::from("/tmp/artifact.app"),
        };
        assert!(IosSimulatorCacheState::Unknown.hit().is_none());
        assert_eq!(IosSimulatorCacheState::Hit(hit.clone()).hit(), Some(&hit));
    }

    #[test]
    #[ignore = "manual helper for seeding the prototype cache"]
    fn print_current_worktree_ios_fingerprint() {
        let cwd = std::env::current_dir().unwrap();
        let fingerprint = ios_native_fingerprint(&cwd).unwrap();
        println!("ios-simulator fingerprint for {}: {fingerprint}", cwd.display());
    }
}
```

- [ ] **Step 3: Register the module**

Add this line to `src/domain/mod.rs`:

```rust
pub mod native_cache;
```

- [ ] **Step 4: Run the domain tests**

Run:

```bash
cargo test domain::native_cache
```

Expected before implementation is complete: compile errors or failing tests. Expected after Steps 2-3: both tests pass.

- [ ] **Step 5: Commit Task 1**

```bash
git add Cargo.toml Cargo.lock src/domain/mod.rs src/domain/native_cache.rs
git commit -m "feat: add iOS cache fingerprint types"
```

## Task 2: Native Cache Port And Infra Adapter

**Files:**
- Create: `src/domain/ports/native_cache_port.rs`
- Modify: `src/domain/ports/mod.rs`
- Create: `src/infra/native_cache.rs`
- Modify: `src/infra/mod.rs`

- [ ] **Step 1: Add the domain port**

Create `src/domain/ports/native_cache_port.rs`:

```rust
use crate::domain::native_cache::{CachedIosLaunchRequest, IosSimulatorCacheHit};
use std::path::PathBuf;

#[async_trait::async_trait]
pub trait NativeCachePort: Send + Sync {
    async fn lookup_ios_simulator(
        &self,
        worktree_path: PathBuf,
    ) -> anyhow::Result<Option<IosSimulatorCacheHit>>;

    async fn install_and_launch_ios_simulator(
        &self,
        request: CachedIosLaunchRequest,
    ) -> anyhow::Result<Vec<String>>;
}
```

Register it in `src/domain/ports/mod.rs`:

```rust
pub mod native_cache_port;
```

- [ ] **Step 2: Write infra adapter and tests**

Create `src/infra/native_cache.rs`:

```rust
use crate::domain::native_cache::{
    ios_native_fingerprint, CachedIosLaunchRequest, IosSimulatorCacheHit,
    IosSimulatorCacheMetadata, IOS_APP_ARTIFACT_KIND, IOS_SIMULATOR_PLATFORM,
};
use crate::domain::ports::native_cache_port::NativeCachePort;
use std::path::{Path, PathBuf};
use std::process::Stdio;

#[derive(Debug, Default)]
pub struct LocalNativeCache;

pub fn native_cache_root() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home)
        .join(".cache")
        .join("ump-dash")
        .join("native-builds")
}

fn ios_entry_dir(root: &Path, fingerprint: &str) -> PathBuf {
    root.join(IOS_SIMULATOR_PLATFORM).join(fingerprint)
}

fn validate_hit(
    metadata: IosSimulatorCacheMetadata,
    expected_fingerprint: &str,
    artifact_path: PathBuf,
) -> anyhow::Result<IosSimulatorCacheHit> {
    if metadata.platform != IOS_SIMULATOR_PLATFORM {
        anyhow::bail!("cache platform mismatch: {}", metadata.platform);
    }
    if metadata.artifact_kind != IOS_APP_ARTIFACT_KIND {
        anyhow::bail!("cache artifact kind mismatch: {}", metadata.artifact_kind);
    }
    if metadata.fingerprint != expected_fingerprint {
        anyhow::bail!("cache fingerprint mismatch: {}", metadata.fingerprint);
    }
    if metadata.bundle_id.trim().is_empty() {
        anyhow::bail!("cache metadata missing bundle_id");
    }
    if !artifact_path.is_dir() {
        anyhow::bail!("cached .app missing at {}", artifact_path.display());
    }
    Ok(IosSimulatorCacheHit {
        metadata,
        artifact_path,
    })
}

pub fn lookup_ios_simulator_in_root(
    root: &Path,
    worktree_path: &Path,
) -> anyhow::Result<Option<IosSimulatorCacheHit>> {
    let fingerprint = ios_native_fingerprint(worktree_path)?;
    let entry_dir = ios_entry_dir(root, &fingerprint);
    let metadata_path = entry_dir.join("metadata.json");
    if !metadata_path.exists() {
        return Ok(None);
    }
    let metadata: IosSimulatorCacheMetadata =
        serde_json::from_slice(&std::fs::read(&metadata_path)?)?;
    validate_hit(metadata, &fingerprint, entry_dir.join("artifact.app")).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimctlLaunchCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub fn simctl_install_args(simulator_udid: &str, app_path: &Path) -> Vec<String> {
    vec![
        "simctl".into(),
        "install".into(),
        simulator_udid.into(),
        app_path.to_string_lossy().to_string(),
    ]
}

pub fn simctl_launch_command(request: &CachedIosLaunchRequest) -> SimctlLaunchCommand {
    SimctlLaunchCommand {
        program: "xcrun".into(),
        args: vec![
            "simctl".into(),
            "launch".into(),
            "--terminate-running-process".into(),
            request.simulator_udid.clone(),
            request.bundle_id.clone(),
        ],
        env: vec![(
            "SIMCTL_CHILD_RCT_METRO_PORT".into(),
            request.metro_port.to_string(),
        )],
    }
}

#[async_trait::async_trait]
impl NativeCachePort for LocalNativeCache {
    async fn lookup_ios_simulator(
        &self,
        worktree_path: PathBuf,
    ) -> anyhow::Result<Option<IosSimulatorCacheHit>> {
        lookup_ios_simulator_in_root(&native_cache_root(), &worktree_path)
    }

    async fn install_and_launch_ios_simulator(
        &self,
        request: CachedIosLaunchRequest,
    ) -> anyhow::Result<Vec<String>> {
        let install_status = tokio::process::Command::new("xcrun")
            .args(simctl_install_args(&request.simulator_udid, &request.app_path))
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .status()
            .await?;
        if !install_status.success() {
            anyhow::bail!("xcrun simctl install failed with status {install_status}");
        }

        let launch = simctl_launch_command(&request);
        let mut cmd = tokio::process::Command::new(&launch.program);
        cmd.args(&launch.args);
        for (key, value) in &launch.env {
            cmd.env(key, value);
        }
        let launch_status = cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).status().await?;
        if !launch_status.success() {
            anyhow::bail!("xcrun simctl launch failed with status {launch_status}");
        }

        Ok(vec![
            format!("installed {}", request.app_path.display()),
            format!(
                "launched {} on {} with Metro port {}",
                request.bundle_id, request.simulator_udid, request.metro_port
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("ump-dash-{name}-{}-{unique}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn seed_worktree(path: &Path) -> String {
        fs::create_dir_all(path.join("ios")).unwrap();
        fs::write(path.join("yarn.lock"), "a").unwrap();
        fs::write(path.join("package.json"), "{}").unwrap();
        fs::write(path.join("ios/Podfile"), "platform :ios").unwrap();
        ios_native_fingerprint(path).unwrap()
    }

    #[test]
    fn lookup_returns_valid_cache_hit() {
        let root = temp_dir("native-cache-root");
        let wt = temp_dir("native-cache-wt");
        let fingerprint = seed_worktree(&wt);
        let entry = root.join(IOS_SIMULATOR_PLATFORM).join(&fingerprint);
        fs::create_dir_all(entry.join("artifact.app")).unwrap();
        let metadata = IosSimulatorCacheMetadata {
            platform: IOS_SIMULATOR_PLATFORM.into(),
            fingerprint: fingerprint.clone(),
            bundle_id: "com.example.app".into(),
            variant: "local".into(),
            created_at: "2026-05-31T00:00:00Z".into(),
            source_worktree: wt.display().to_string(),
            artifact_kind: IOS_APP_ARTIFACT_KIND.into(),
        };
        fs::write(entry.join("metadata.json"), serde_json::to_vec(&metadata).unwrap()).unwrap();

        let hit = lookup_ios_simulator_in_root(&root, &wt).unwrap().unwrap();
        assert_eq!(hit.metadata.bundle_id, "com.example.app");
        assert_eq!(hit.artifact_path, entry.join("artifact.app"));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(wt);
    }

    #[test]
    fn simctl_launch_command_sets_metro_port_env() {
        let request = CachedIosLaunchRequest {
            simulator_udid: "SIM-1".into(),
            app_path: PathBuf::from("/tmp/artifact.app"),
            bundle_id: "com.example.app".into(),
            metro_port: 8093,
        };
        let command = simctl_launch_command(&request);
        assert_eq!(command.program, "xcrun");
        assert!(command.args.contains(&"--terminate-running-process".into()));
        assert!(command.args.contains(&"SIM-1".into()));
        assert!(command.args.contains(&"com.example.app".into()));
        assert_eq!(
            command.env,
            vec![("SIMCTL_CHILD_RCT_METRO_PORT".into(), "8093".into())]
        );
    }
}
```

Register it in `src/infra/mod.rs`:

```rust
pub mod native_cache;
```

- [ ] **Step 3: Run adapter tests**

Run:

```bash
cargo test infra::native_cache
```

Expected: lookup and command-builder tests pass without invoking `xcrun`.

- [ ] **Step 4: Commit Task 2**

```bash
git add src/domain/ports/mod.rs src/domain/ports/native_cache_port.rs src/infra/mod.rs src/infra/native_cache.rs
git commit -m "feat: add native cache adapter"
```

## Task 3: Wire Adapter, Effects, And Lookup Result State

**Files:**
- Modify: `src/app/adapters.rs`
- Modify: `src/main.rs`
- Modify: `src/app/effect.rs`
- Modify: `src/app/effect_runner.rs`
- Modify: `src/domain/action.rs`
- Modify: `src/domain/worktree_slice.rs`

- [ ] **Step 1: Add adapter field**

In `src/app/adapters.rs`, import the port:

```rust
use crate::domain::ports::native_cache_port::NativeCachePort;
```

Add a field to `Adapters`:

```rust
pub native_cache: Arc<dyn NativeCachePort>,
```

In `src/main.rs`, add the concrete adapter:

```rust
native_cache: Arc::new(ump_dash::infra::native_cache::LocalNativeCache),
```

- [ ] **Step 2: Add action variants**

In `src/domain/action.rs`, add:

```rust
IosSimulatorCacheLookupFinished {
    worktree_id: crate::domain::worktree::WorktreeId,
    result: Result<Option<crate::domain::native_cache::IosSimulatorCacheHit>, String>,
},
CachedIosRun(crate::domain::native_cache::IosSimulatorCacheHit),
CachedIosLaunchFinished {
    worktree_id: crate::domain::worktree::WorktreeId,
    result: crate::domain::native_cache::CachedIosLaunchResult,
},
```

- [ ] **Step 3: Add effect variants**

In `src/app/effect.rs`, import cache types:

```rust
use crate::domain::native_cache::CachedIosLaunchRequest;
use crate::domain::worktree::WorktreeId;
```

Add effect variants:

```rust
LookupIosSimulatorCache {
    worktree_id: WorktreeId,
    worktree_path: PathBuf,
},
InstallAndLaunchCachedIosSimulator {
    worktree_id: WorktreeId,
    request: CachedIosLaunchRequest,
},
```

Update `effect_variants_compile` and `effect_has_at_least_fifteen_variants` so the new variants are constructed and matched.

- [ ] **Step 4: Add worktree-slice fields**

In `src/domain/worktree_slice.rs`, add imports:

```rust
use crate::domain::native_cache::{IosSimulatorCacheState, PendingCachedIosLaunch};
```

Add fields to `WorktreeSlice`:

```rust
pub ios_simulator_cache: IosSimulatorCacheState,
pub pending_cached_ios_launch: Option<PendingCachedIosLaunch>,
```

Update the existing default-slice test to assert:

```rust
assert_eq!(s.ios_simulator_cache, IosSimulatorCacheState::Unknown);
assert!(s.pending_cached_ios_launch.is_none());
```

- [ ] **Step 5: Handle lookup effects**

In `src/app/effect_runner.rs`, add an arm:

```rust
Effect::LookupIosSimulatorCache { worktree_id, worktree_path } => {
    let native_cache = self.adapters.native_cache.clone();
    let tx = self.action_tx.clone();
    tokio::spawn(async move {
        let result = native_cache
            .lookup_ios_simulator(worktree_path)
            .await
            .map_err(|e| e.to_string());
        let _ = tx.send(Action::IosSimulatorCacheLookupFinished { worktree_id, result });
    });
}
```

- [ ] **Step 6: Run compile-focused tests**

Run:

```bash
cargo test app::effect
cargo test domain::worktree_slice
```

Expected: all tests pass.

- [ ] **Step 7: Commit Task 3**

```bash
git add src/app/adapters.rs src/main.rs src/app/effect.rs src/app/effect_runner.rs src/domain/action.rs src/domain/worktree_slice.rs
git commit -m "feat: wire native cache lookup state"
```

## Task 4: iOS Palette Cache Lookup And Shortcut Visibility

**Files:**
- Modify: `src/app/update.rs`
- Modify: `src/app/keybindings.rs`
- Modify: `src/app/dispatch_tests.rs`

- [ ] **Step 1: Add update helper for selected worktree**

In `src/app/update.rs`, add a helper near `active_worktree_id` helpers:

```rust
fn active_worktree_snapshot(state: &AppState) -> Option<(crate::domain::worktree::WorktreeId, PathBuf)> {
    let idx = state.worktree_browser.worktree_table_state.selected().unwrap_or(0);
    let wt = state.worktree_browser.worktrees.get(idx.min(state.worktree_browser.worktrees.len().saturating_sub(1)))?;
    Some((wt.id.clone(), wt.path.clone()))
}
```

- [ ] **Step 2: Trigger lookup when entering iOS palette**

In the `Action::EnterIosPalette` arm, replace the current body with:

```rust
state.modal_stack.palette_mode = Some(PaletteMode::Ios);
if let Some((worktree_id, worktree_path)) = active_worktree_snapshot(state) {
    let slice = state.worktrees.entry(worktree_id.clone()).or_insert_with(|| {
        crate::domain::worktree_slice::WorktreeSlice {
            id: worktree_id.clone(),
            ..Default::default()
        }
    });
    slice.ios_simulator_cache = crate::domain::native_cache::IosSimulatorCacheState::Checking;
    effects.push(Effect::LookupIosSimulatorCache {
        worktree_id,
        worktree_path,
    });
}
```

Add a handler for lookup completion:

```rust
Action::IosSimulatorCacheLookupFinished { worktree_id, result } => {
    if let Some(slice) = state.worktrees.get_mut(&worktree_id) {
        slice.ios_simulator_cache = match result {
            Ok(Some(hit)) => crate::domain::native_cache::IosSimulatorCacheState::Hit(hit),
            Ok(None) => crate::domain::native_cache::IosSimulatorCacheState::Miss,
            Err(message) => crate::domain::native_cache::IosSimulatorCacheState::Error(message),
        };
    }
}
```

- [ ] **Step 3: Add temporary `i>c` keybinding**

In `src/app/keybindings.rs`, add helpers:

```rust
fn cached_ios_hit(state: &AppState) -> Option<&crate::domain::native_cache::IosSimulatorCacheHit> {
    let id = active_worktree_id(state)?;
    state.worktrees.get(&id)?.ios_simulator_cache.hit()
}

fn has_cached_ios_hit(state: &AppState) -> bool {
    cached_ios_hit(state).is_some()
}

fn cached_ios_run(state: &AppState) -> Option<Action> {
    Some(cached_ios_hit(state).cloned().map_or(Action::ModalCancel, Action::CachedIosRun))
}
```

Add the binding inside the iOS palette section:

```rust
KeyBinding {
    key: KeyCode::Char('c'),
    label: "c", short_desc: "cached", long_desc: "Install cached iOS simulator build",
    context: BindingContext::Palette(PaletteMode::Ios),
    action: cached_ios_run,
    visible: has_cached_ios_hit,
},
```

- [ ] **Step 4: Add dispatch tests**

In `src/app/dispatch_tests.rs`, add a test in `palette_resolution`:

```rust
#[test]
fn ios_palette_cached_key_visible_only_with_cache_hit() {
    let mut state = base_state();
    seed_one_worktree(&mut state);
    state.modal_stack.palette_mode = Some(PaletteMode::Ios);

    assert_eq!(handle_key(&state, key('c')), Some(Action::ModalCancel));

    let id = WorktreeId("wt-1".into());
    let hit = crate::domain::native_cache::IosSimulatorCacheHit {
        metadata: crate::domain::native_cache::IosSimulatorCacheMetadata {
            platform: crate::domain::native_cache::IOS_SIMULATOR_PLATFORM.into(),
            fingerprint: "abc".into(),
            bundle_id: "com.example.app".into(),
            variant: "local".into(),
            created_at: "2026-05-31T00:00:00Z".into(),
            source_worktree: "/tmp/wt".into(),
            artifact_kind: crate::domain::native_cache::IOS_APP_ARTIFACT_KIND.into(),
        },
        artifact_path: std::path::PathBuf::from("/tmp/artifact.app"),
    };
    state.worktrees.get_mut(&id).unwrap().ios_simulator_cache =
        crate::domain::native_cache::IosSimulatorCacheState::Hit(hit.clone());

    assert_eq!(handle_key(&state, key('c')), Some(Action::CachedIosRun(hit)));
}
```

Add this test in `ump_run_dialog`:

```rust
#[test]
fn entering_ios_palette_starts_cache_lookup_for_selected_worktree() {
    let mut state = base_state();
    seed_one_worktree(&mut state);

    let effects = update(&mut state, Action::EnterIosPalette);

    assert!(matches!(state.modal_stack.palette_mode, Some(PaletteMode::Ios)));
    assert!(matches!(
        state.worktrees.get(&WorktreeId("wt-1".into())).unwrap().ios_simulator_cache,
        crate::domain::native_cache::IosSimulatorCacheState::Checking
    ));
    assert!(matches!(
        effects.as_slice(),
        [Effect::LookupIosSimulatorCache { worktree_id, .. }]
            if worktree_id == &WorktreeId("wt-1".into())
    ));
}
```

- [ ] **Step 5: Run dispatch tests**

Run:

```bash
cargo test app::dispatch_tests::palette_resolution
cargo test app::dispatch_tests::ump_run_dialog
```

Expected: tests pass.

- [ ] **Step 6: Commit Task 4**

```bash
git add src/app/update.rs src/app/keybindings.rs src/app/dispatch_tests.rs
git commit -m "feat: show cached iOS shortcut on cache hit"
```

## Task 5: Cached iOS Device Selection And Metro Prerequisite

**Files:**
- Modify: `src/app/state.rs`
- Modify: `src/app/update.rs`
- Modify: `src/app/dispatch_tests.rs`

- [ ] **Step 1: Add modal pending state**

In `src/app/state.rs`, add this field to `ModalStackState`:

```rust
pub pending_cached_ios_run: Option<crate::domain::native_cache::IosSimulatorCacheHit>,
```

Update `Action::ModalCancel` in `src/app/update.rs` to clear it:

```rust
state.modal_stack.pending_cached_ios_run = None;
```

- [ ] **Step 2: Add helper to emit cached launch effects**

In `src/app/update.rs`, add:

```rust
fn cached_ios_launch_request(
    cache_hit: &crate::domain::native_cache::IosSimulatorCacheHit,
    device_id: String,
    metro_port: u16,
) -> crate::domain::native_cache::CachedIosLaunchRequest {
    crate::domain::native_cache::CachedIosLaunchRequest {
        simulator_udid: device_id,
        app_path: cache_hit.artifact_path.clone(),
        bundle_id: cache_hit.metadata.bundle_id.clone(),
        metro_port,
    }
}

fn begin_cached_ios_launch(
    state: &mut AppState,
    effects: &mut Vec<Effect>,
    device_id: String,
    cache_hit: crate::domain::native_cache::IosSimulatorCacheHit,
) {
    effects.push(Effect::ScheduleAction(Action::SimulatorUsed(device_id.clone())));
    let Some(wt_id) = active_worktree_id(state) else {
        return;
    };
    let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
        crate::domain::worktree_slice::WorktreeSlice {
            id: wt_id.clone(),
            ..Default::default()
        }
    });
    if let Some(port) = slice.metro.running_port() {
        effects.push(Effect::InstallAndLaunchCachedIosSimulator {
            worktree_id: wt_id,
            request: cached_ios_launch_request(&cache_hit, device_id, port),
        });
    } else {
        slice.pending_cached_ios_launch = Some(crate::domain::native_cache::PendingCachedIosLaunch {
            device_id,
            cache_hit,
        });
        effects.extend(update(state, Action::MetroStart));
    }
}
```

- [ ] **Step 3: Handle `Action::CachedIosRun`**

In `src/app/update.rs`, add an action arm before normal command dispatch:

```rust
Action::CachedIosRun(cache_hit) => {
    state.modal_stack.palette_mode = None;
    state.modal_stack.pending_cached_ios_run = Some(cache_hit);
    state.modal_stack.pending_device_command = Some(CommandSpec::UmpRunIos {
        device_id: String::new(),
        variant: Some(RunVariant::Local),
    });
    effects.push(Effect::LoadDevices {
        kind: crate::domain::ports::device_port::DeviceKind::Ios,
    });
}
```

- [ ] **Step 4: Route device selection through cached launch**

At the start of `Action::ModalDeviceConfirm` after selecting `device_id`, before normal `CommandSpec` handling, add:

```rust
if let Some(cache_hit) = state.modal_stack.pending_cached_ios_run.take() {
    begin_cached_ios_launch(state, &mut effects, device_id, cache_hit);
    return effects;
}
```

In `Action::DevicesEnumerated`, before the existing `match devices.len()` flow uses `spec`, branch when `pending_cached_ios_run` is set:

```rust
if let Some(cache_hit) = state.modal_stack.pending_cached_ios_run.clone() {
    match devices.len() {
        0 => {
            state.modal_stack.pending_cached_ios_run = None;
            if let Some(id) = active_worktree_id(state)
                && let Some(slice) = state.worktrees.get_mut(&id)
            {
                slice.output.push_back("[error] no iOS simulators found for cached run".into());
            }
        }
        1 => {
            state.modal_stack.pending_cached_ios_run = None;
            begin_cached_ios_launch(state, &mut effects, devices[0].id.clone(), cache_hit);
        }
        _ => {
            let mut sorted_devices = devices;
            let history = &state.app_config.sim_history;
            sorted_devices.sort_by_key(|d| {
                history.iter().position(|h| h == &d.id).unwrap_or(usize::MAX)
            });
            state.modal_stack.modal = Some(ModalState::DevicePicker {
                devices: sorted_devices,
                selected: 0,
                pending_template: Box::new(CommandSpec::UmpRunIos {
                    device_id: String::new(),
                    variant: Some(RunVariant::Local),
                }),
                filter: String::new(),
            });
        }
    }
    return effects;
}
```

- [ ] **Step 5: Launch after Metro becomes ready and clear on Metro failure**

In `Action::MetroActivityUpdate`, inside the `Ready` branch after existing command queue handling, take pending cached launch for the matching slice:

```rust
let pending_cached = slice_id_for_metro_worktree_id(state, &worktree_id)
    .and_then(|id| {
        let slice = state.worktrees.get_mut(&id)?;
        let port = slice.metro.running_port()?;
        let pending = slice.pending_cached_ios_launch.take()?;
        Some((id, pending, port))
    });

if let Some((id, pending, port)) = pending_cached {
    effects.push(Effect::InstallAndLaunchCachedIosSimulator {
        worktree_id: id,
        request: cached_ios_launch_request(&pending.cache_hit, pending.device_id, port),
    });
}
```

In `Action::MetroSpawnFailed`, after the existing queue/post-drain cleanup, clear a pending cached launch for the same slice and append a worktree-output error:

```rust
if let Some(slice_id) = slice_id_for_metro_worktree_id(state, &worktree_id)
    && let Some(slice) = state.worktrees.get_mut(&slice_id)
{
    if slice.pending_cached_ios_launch.take().is_some() {
        slice.output.push_back(format!("[cached-ios error] Metro failed to start: {message}"));
        while slice.output.len() > MAX_COMMAND_LINES {
            slice.output.pop_front();
        }
    }
}
```

- [ ] **Step 6: Add tests**

Add tests to `src/app/dispatch_tests.rs`:

```rust
#[test]
fn cached_ios_run_loads_ios_devices() {
    let mut state = base_state();
    seed_one_worktree(&mut state);
    let hit = cached_ios_hit_fixture();

    let effects = update(&mut state, Action::CachedIosRun(hit.clone()));

    assert_eq!(state.modal_stack.pending_cached_ios_run, Some(hit));
    assert!(matches!(
        effects.as_slice(),
        [Effect::LoadDevices { kind: crate::domain::ports::device_port::DeviceKind::Ios }]
    ));
}

#[test]
fn cached_ios_device_selection_starts_metro_when_needed() {
    let mut state = base_state();
    seed_one_worktree(&mut state);
    state.modal_stack.pending_cached_ios_run = Some(cached_ios_hit_fixture());
    state.modal_stack.modal = Some(ModalState::DevicePicker {
        devices: vec![crate::domain::command::DeviceInfo {
            id: "SIM-1".into(),
            name: "iPhone 16".into(),
        }],
        selected: 0,
        pending_template: Box::new(CommandSpec::UmpRunIos {
            device_id: String::new(),
            variant: Some(RunVariant::Local),
        }),
        filter: String::new(),
    });

    let effects = update(&mut state, Action::ModalDeviceConfirm);

    assert!(state
        .worktrees
        .get(&WorktreeId("wt-1".into()))
        .unwrap()
        .pending_cached_ios_launch
        .is_some());
    assert!(effects.iter().any(|e| matches!(e, Effect::SpawnMetro { .. })));
}
```

Add a local test helper:

```rust
fn cached_ios_hit_fixture() -> crate::domain::native_cache::IosSimulatorCacheHit {
    crate::domain::native_cache::IosSimulatorCacheHit {
        metadata: crate::domain::native_cache::IosSimulatorCacheMetadata {
            platform: crate::domain::native_cache::IOS_SIMULATOR_PLATFORM.into(),
            fingerprint: "abc".into(),
            bundle_id: "com.example.app".into(),
            variant: "local".into(),
            created_at: "2026-05-31T00:00:00Z".into(),
            source_worktree: "/tmp/wt".into(),
            artifact_kind: crate::domain::native_cache::IOS_APP_ARTIFACT_KIND.into(),
        },
        artifact_path: std::path::PathBuf::from("/tmp/artifact.app"),
    }
}
```

- [ ] **Step 7: Run tests**

Run:

```bash
cargo test app::dispatch_tests
```

Expected: all dispatch tests pass.

- [ ] **Step 8: Commit Task 5**

```bash
git add src/app/state.rs src/app/update.rs src/app/dispatch_tests.rs
git commit -m "feat: route cached iOS run through simulator picker"
```

## Task 6: Install/Launch Effect And Output Reporting

**Files:**
- Modify: `src/app/effect_runner.rs`
- Modify: `src/app/update.rs`
- Modify: `src/app/effect.rs`

- [ ] **Step 1: Run cached install/launch effect**

In `src/app/effect_runner.rs`, add:

```rust
Effect::InstallAndLaunchCachedIosSimulator { worktree_id, request } => {
    let native_cache = self.adapters.native_cache.clone();
    let tx = self.action_tx.clone();
    tokio::spawn(async move {
        let result = match native_cache.install_and_launch_ios_simulator(request).await {
            Ok(lines) => crate::domain::native_cache::CachedIosLaunchResult::Success(lines),
            Err(e) => crate::domain::native_cache::CachedIosLaunchResult::Failure(e.to_string()),
        };
        let _ = tx.send(Action::CachedIosLaunchFinished { worktree_id, result });
    });
}
```

- [ ] **Step 2: Append result output to the worktree**

In `src/app/update.rs`, add:

```rust
Action::CachedIosLaunchFinished { worktree_id, result } => {
    if let Some(slice) = state.worktrees.get_mut(&worktree_id) {
        match result {
            crate::domain::native_cache::CachedIosLaunchResult::Success(lines) => {
                slice.output.push_back("[cached-ios] installed and launched cached app".into());
                for line in lines {
                    slice.output.push_back(format!("[cached-ios] {line}"));
                }
            }
            crate::domain::native_cache::CachedIosLaunchResult::Failure(message) => {
                slice.output.push_back(format!("[cached-ios error] {message}"));
            }
        }
        while slice.output.len() > MAX_COMMAND_LINES {
            slice.output.pop_front();
        }
        slice.output_scroll = 0;
    }
}
```

- [ ] **Step 3: Add result-output test**

In `src/app/dispatch_tests.rs`, add:

```rust
#[test]
fn cached_ios_launch_result_appends_to_selected_slice_output() {
    let mut state = base_state();
    seed_one_worktree(&mut state);

    let _ = update(
        &mut state,
        Action::CachedIosLaunchFinished {
            worktree_id: WorktreeId("wt-1".into()),
            result: crate::domain::native_cache::CachedIosLaunchResult::Success(vec![
                "launched com.example.app on SIM-1 with Metro port 8093".into(),
            ]),
        },
    );

    let output = slice_output(&state, "wt-1");
    assert!(output.iter().any(|line| line.contains("installed and launched")));
    assert!(output.iter().any(|line| line.contains("Metro port 8093")));
}
```

- [ ] **Step 4: Run focused tests**

Run:

```bash
cargo test app::dispatch_tests::cached_ios_launch_result_appends_to_selected_slice_output
cargo test app::effect::tests::effect_variants_compile
```

Expected: both pass.

- [ ] **Step 5: Commit Task 6**

```bash
git add src/app/effect_runner.rs src/app/update.rs src/app/effect.rs src/app/dispatch_tests.rs
git commit -m "feat: launch cached iOS simulator artifacts"
```

## Task 7: Manual Seeding Instructions And Full Verification

**Files:**
- Modify: `docs/superpowers/specs/2026-05-31-ios-native-build-cache-prototype-design.md`

- [ ] **Step 1: Add manual seed command to the spec**

Append this section to the spec:

````markdown
## Prototype Cache Seeding

To seed a local iOS simulator cache entry:

1. Build the UMP iOS simulator app once through the normal `i>r` flow.
2. Compute the fingerprint from the worktree root:

```bash
cargo test domain::native_cache::tests::print_current_worktree_ios_fingerprint -- --ignored --nocapture
```

Expected output includes one line shaped like:

```text
ios-simulator fingerprint for /absolute/path/to/worktree: <64-hex-character-sha256>
```

3. Create:

```text
~/.cache/ump-dash/native-builds/ios-simulator/<fingerprint>/artifact.app
~/.cache/ump-dash/native-builds/ios-simulator/<fingerprint>/metadata.json
```

4. Copy the built `.app` directory to `artifact.app`.
5. Write `metadata.json` with the bundle id and the matching fingerprint.

The prototype intentionally does not auto-populate the cache after misses.
````

- [ ] **Step 2: Run static search**

Run:

```bash
rg -n 'react-native run-ios|react-native run-android|RnRunIos|RnRunAndroid|RnRunIosDevice' src
rg -n 'IOS_FINGERPRINT_FILES|Podfile.lock' src/domain/native_cache.rs
```

Expected: the first command prints no output. The second command shows `IOS_FINGERPRINT_FILES` and the exclusion test's `Podfile.lock` writes, but no `Podfile.lock` entry inside `IOS_FINGERPRINT_FILES`.

- [ ] **Step 3: Run full verification**

Run:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
make arch-lint
git diff --check
```

Expected:

- `cargo test`: all tests pass.
- `cargo clippy --all-targets -- -D warnings`: exit 0.
- `make arch-lint`: exits 0 with `arch-lint: PASS`.
- `git diff --check`: no output.

- [ ] **Step 4: Manual feasibility run**

Run this manually on a machine with an available iOS simulator and seeded cache:

```bash
cargo run
```

Then in `ump-dash`:

1. Select a worktree with a seeded cache entry.
2. Press `i`.
3. Confirm `c cached` appears.
4. Press `c`.
5. Select an iOS simulator.
6. Confirm the worktree output shows cached install and launch lines.
7. Confirm the app loads JavaScript from that worktree's Metro port.

If the app launches but does not load from the selected Metro port, record that result in the spec under a `Prototype Result` section before continuing to production cache design.

- [ ] **Step 5: Commit Task 7**

```bash
git add docs/superpowers/specs/2026-05-31-ios-native-build-cache-prototype-design.md
git commit -m "docs: document iOS cache prototype seeding"
```

## Self-Review

Spec coverage:

- Local machine cache: Tasks 1-2 use `~/.cache/ump-dash/native-builds`.
- iOS simulator only: all types and effects are named for iOS simulator; no Android path is introduced.
- Cache hit skips native build: Tasks 5-6 install and launch `.app` directly; no build command is dispatched.
- Artifact cache, not build folder cache: Task 2 validates `artifact.app`.
- Metro port not in fingerprint: Task 1 hashes only `yarn.lock`, `package.json`, and `ios/Podfile`; Task 5 adds Metro port only to launch request.
- `Podfile.lock` excluded: Task 1 test asserts changing it does not change the fingerprint.
- Temporary `i>c` shortcut: Task 4 adds keybinding visible only for cache hits.
- Existing UMP run path unchanged: plan adds a separate cached action, not a `CommandSpec` replacement.
- Error reporting: Task 6 appends success/failure to selected worktree output.
- Full verification: Task 7 includes tests, clippy, arch-lint, and manual feasibility run.

Placeholder scan:

- The implementation steps contain no unresolved fill-in markers.
- Angle-bracket examples appear only in documentation paths and shell command examples where they mean user-substituted values.

Type consistency:

- `IosSimulatorCacheHit`, `IosSimulatorCacheState`, `PendingCachedIosLaunch`, `CachedIosLaunchRequest`, and `CachedIosLaunchResult` are defined in Task 1 and reused consistently.
- `NativeCachePort` is defined in Task 2 and injected through `Adapters` in Task 3.
- `Action` and `Effect` variants added in Task 3 are consumed in Tasks 4-6.
