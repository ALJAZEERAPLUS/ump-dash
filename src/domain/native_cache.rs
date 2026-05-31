use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

pub const IOS_SIMULATOR_PLATFORM: &str = "ios-simulator";
pub const IOS_APP_ARTIFACT_KIND: &str = "app-bundle";

pub const IOS_FINGERPRINT_FILES: &[&str] = &["yarn.lock", "package.json", "ios/Podfile"];

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
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempWorktree {
        path: PathBuf,
    }

    impl TempWorktree {
        fn new() -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "ump-dash-ios-native-cache-test-{}-{}",
                std::process::id(),
                nanos
            ));
            fs::create_dir_all(path.join("ios")).expect("temp worktree should be created");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn write(&self, rel: &str, contents: &str) {
            fs::write(self.path.join(rel), contents).expect("temp file should be written");
        }
    }

    impl Drop for TempWorktree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn ios_fingerprint_uses_declared_inputs_and_excludes_podfile_lock() {
        let worktree = TempWorktree::new();
        worktree.write("yarn.lock", "left-pad@1.0.0\n");
        worktree.write("package.json", "{\"dependencies\":{}}\n");
        worktree.write("ios/Podfile", "platform :ios, '15.0'\n");
        worktree.write("ios/Podfile.lock", "PODS:\n  - One\n");

        let initial = ios_native_fingerprint(worktree.path()).expect("fingerprint should hash");

        worktree.write("ios/Podfile.lock", "PODS:\n  - Different\n");
        let after_lock_change =
            ios_native_fingerprint(worktree.path()).expect("fingerprint should hash");

        assert_eq!(initial, after_lock_change);

        worktree.write("ios/Podfile", "platform :ios, '16.0'\n");
        let after_podfile_change =
            ios_native_fingerprint(worktree.path()).expect("fingerprint should hash");

        assert_ne!(initial, after_podfile_change);
    }

    #[test]
    fn cache_state_hit_helper_returns_only_hits() {
        assert_eq!(IosSimulatorCacheState::Unknown.hit(), None);

        let hit = IosSimulatorCacheHit {
            metadata: IosSimulatorCacheMetadata {
                platform: IOS_SIMULATOR_PLATFORM.to_string(),
                fingerprint: "fingerprint".to_string(),
                bundle_id: "com.aljazeera.test".to_string(),
                variant: "debug".to_string(),
                created_at: "2026-05-31T00:00:00Z".to_string(),
                source_worktree: "/tmp/worktree".to_string(),
                artifact_kind: IOS_APP_ARTIFACT_KIND.to_string(),
            },
            artifact_path: PathBuf::from("/tmp/app"),
        };
        let state = IosSimulatorCacheState::Hit(hit.clone());

        assert_eq!(state.hit(), Some(&hit));
    }

    #[test]
    #[ignore]
    fn print_current_worktree_ios_fingerprint() {
        let cwd = std::env::current_dir().expect("cwd should be available");
        let fingerprint = ios_native_fingerprint(&cwd).expect("fingerprint should hash");
        println!(
            "ios-simulator fingerprint for {}: {}",
            cwd.display(),
            fingerprint
        );
    }
}
