use crate::domain::command::RunVariant;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::path::PathBuf;

pub const IOS_SIMULATOR_PLATFORM: &str = "ios-simulator";
pub const IOS_APP_ARTIFACT_KIND: &str = "app-bundle";
pub const ANDROID_PLATFORM: &str = "android";
pub const ANDROID_APK_ARTIFACT_KIND: &str = "apk";

pub const IOS_FINGERPRINT_FILES: &[&str] = &["yarn.lock", "package.json", "ios/Podfile"];
pub const ANDROID_FINGERPRINT_FILES: &[&str] = &[
    "yarn.lock",
    "package.json",
    "android/settings.gradle",
    "android/build.gradle",
    "android/app/build.gradle",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeFingerprintInput {
    pub relative_path: String,
    pub contents: Option<Vec<u8>>,
}

impl NativeFingerprintInput {
    pub fn present(relative_path: impl Into<String>, contents: impl Into<Vec<u8>>) -> Self {
        Self {
            relative_path: relative_path.into(),
            contents: Some(contents.into()),
        }
    }

    pub fn missing(relative_path: impl Into<String>) -> Self {
        Self {
            relative_path: relative_path.into(),
            contents: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IosSimulatorCacheMetadata {
    pub platform: String,
    pub fingerprint: String,
    pub bundle_id: String,
    pub variant: String,
    pub created_at: String,
    pub source_worktree: String,
    pub artifact_kind: String,
    #[serde(default)]
    pub storage_mode: String,
    #[serde(default)]
    pub source_artifact_path: PathBuf,
    #[serde(default)]
    pub artifact_digest_algorithm: String,
    #[serde(default)]
    pub artifact_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IosSimulatorCacheHit {
    pub metadata: IosSimulatorCacheMetadata,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AndroidCacheMetadata {
    pub platform: String,
    pub fingerprint: String,
    pub application_id: String,
    pub variant: String,
    pub created_at: String,
    pub source_worktree: String,
    pub artifact_kind: String,
    #[serde(default)]
    pub storage_mode: String,
    #[serde(default)]
    pub source_artifact_path: PathBuf,
    #[serde(default)]
    pub artifact_digest_algorithm: String,
    #[serde(default)]
    pub artifact_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidCacheHit {
    pub metadata: AndroidCacheMetadata,
    pub artifact_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IosSimulatorCacheLookup {
    Hit(Box<IosSimulatorCacheHit>),
    Miss { fingerprint: String },
}

impl IosSimulatorCacheLookup {
    pub fn into_cache_state(self) -> IosSimulatorCacheState {
        match self {
            Self::Hit(hit) => IosSimulatorCacheState::Hit(hit),
            Self::Miss { fingerprint } => IosSimulatorCacheState::Miss { fingerprint },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AndroidCacheLookup {
    Hit(Box<AndroidCacheHit>),
    Miss { fingerprint: String },
}

impl AndroidCacheLookup {
    pub fn into_cache_state(self) -> AndroidCacheState {
        match self {
            Self::Hit(hit) => AndroidCacheState::Hit(hit),
            Self::Miss { fingerprint } => AndroidCacheState::Miss { fingerprint },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IosSimulatorCacheStoreRequest {
    pub worktree_path: PathBuf,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AndroidCacheStoreRequest {
    pub worktree_path: PathBuf,
    pub variant: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum IosSimulatorCacheState {
    #[default]
    Unknown,
    Checking,
    Hit(Box<IosSimulatorCacheHit>),
    Miss {
        fingerprint: String,
    },
    Error(String),
}

impl IosSimulatorCacheState {
    pub fn hit(&self) -> Option<&IosSimulatorCacheHit> {
        match self {
            Self::Hit(hit) => Some(hit.as_ref()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AndroidCacheState {
    #[default]
    Unknown,
    Checking,
    Hit(Box<AndroidCacheHit>),
    Miss {
        fingerprint: String,
    },
    Error(String),
}

impl AndroidCacheState {
    pub fn hit(&self) -> Option<&AndroidCacheHit> {
        match self {
            Self::Hit(hit) => Some(hit.as_ref()),
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
pub struct PendingCachedAndroidLaunch {
    pub device_id: String,
    pub cache_hit: AndroidCacheHit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedIosLaunchRequest {
    pub simulator_udid: String,
    pub app_path: PathBuf,
    pub bundle_id: String,
    pub metro_port: u16,
    pub fingerprint: String,
    pub variant: RunVariant,
    pub artifact_digest_algorithm: String,
    pub artifact_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedAndroidLaunchRequest {
    pub device_id: String,
    pub apk_path: PathBuf,
    pub application_id: String,
    pub metro_port: u16,
    pub fingerprint: String,
    pub variant: RunVariant,
    pub artifact_digest_algorithm: String,
    pub artifact_digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachedIosLaunchResult {
    Success(Vec<String>),
    Failure(String),
    InvalidArtifact {
        message: String,
        device_id: String,
        variant: RunVariant,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachedAndroidLaunchResult {
    Success(Vec<String>),
    Failure(String),
    InvalidArtifact {
        message: String,
        device_id: String,
        variant: RunVariant,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedArtifactValidationError {
    pub message: String,
}

impl fmt::Display for CachedArtifactValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for CachedArtifactValidationError {}

pub fn native_fingerprint_from_inputs(inputs: &[NativeFingerprintInput]) -> String {
    let mut hasher = Sha256::new();
    for input in inputs {
        hasher.update(input.relative_path.as_bytes());
        hasher.update([0]);
        match input.contents.as_ref() {
            Some(bytes) => {
                hasher.update(b"present");
                hasher.update((bytes.len() as u64).to_le_bytes());
                hasher.update(bytes);
            }
            None => {
                hasher.update(b"missing");
            }
        }
        hasher.update([0xff]);
    }
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn present(rel: &str, contents: &str) -> NativeFingerprintInput {
        NativeFingerprintInput::present(rel, contents.as_bytes().to_vec())
    }

    #[test]
    fn ios_fingerprint_uses_declared_inputs_and_excludes_podfile_lock() {
        let initial = native_fingerprint_from_inputs(&[
            present("yarn.lock", "left-pad@1.0.0\n"),
            present("package.json", "{\"dependencies\":{}}\n"),
            present("ios/Podfile", "platform :ios, '15.0'\n"),
        ]);

        let after_lock_change = native_fingerprint_from_inputs(&[
            present("yarn.lock", "left-pad@1.0.0\n"),
            present("package.json", "{\"dependencies\":{}}\n"),
            present("ios/Podfile", "platform :ios, '15.0'\n"),
        ]);

        assert_eq!(initial, after_lock_change);

        let after_podfile_change = native_fingerprint_from_inputs(&[
            present("yarn.lock", "left-pad@1.0.0\n"),
            present("package.json", "{\"dependencies\":{}}\n"),
            present("ios/Podfile", "platform :ios, '16.0'\n"),
        ]);

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
                storage_mode: "copy".to_string(),
                source_artifact_path: PathBuf::from("/tmp/worktree/build/app.app"),
                artifact_digest_algorithm: "sha256".to_string(),
                artifact_digest: "digest".to_string(),
            },
            artifact_path: PathBuf::from("/tmp/app"),
        };
        let state = IosSimulatorCacheState::Hit(Box::new(hit.clone()));

        assert_eq!(state.hit(), Some(&hit));
    }

    #[test]
    fn android_fingerprint_uses_declared_inputs() {
        let initial = native_fingerprint_from_inputs(&[
            present("yarn.lock", "yarn-a\n"),
            present("package.json", "{}\n"),
            present("android/settings.gradle", "settings-a\n"),
            present("android/build.gradle", "root-a\n"),
            present("android/app/build.gradle", "app-a\n"),
        ]);

        let after_untracked_native_source = native_fingerprint_from_inputs(&[
            present("yarn.lock", "yarn-a\n"),
            present("package.json", "{}\n"),
            present("android/settings.gradle", "settings-a\n"),
            present("android/build.gradle", "root-a\n"),
            present("android/app/build.gradle", "app-a\n"),
        ]);

        assert_eq!(initial, after_untracked_native_source);

        let after_declared_input_change = native_fingerprint_from_inputs(&[
            present("yarn.lock", "yarn-a\n"),
            present("package.json", "{}\n"),
            present("android/settings.gradle", "settings-a\n"),
            present("android/build.gradle", "root-a\n"),
            present("android/app/build.gradle", "app-b\n"),
        ]);

        assert_eq!(ANDROID_FINGERPRINT_FILES.len(), 5);
        assert_ne!(initial, after_declared_input_change);
    }

    #[test]
    fn fingerprint_distinguishes_missing_inputs_from_empty_files() {
        let missing =
            native_fingerprint_from_inputs(&[NativeFingerprintInput::missing("yarn.lock")]);
        let empty = native_fingerprint_from_inputs(&[present("yarn.lock", "")]);

        assert_ne!(missing, empty);
    }

    #[test]
    fn android_cache_state_hit_helper_returns_only_hits() {
        let hit = AndroidCacheHit {
            metadata: AndroidCacheMetadata {
                platform: ANDROID_PLATFORM.to_string(),
                fingerprint: "fingerprint".to_string(),
                application_id: "com.aljazeera.test".to_string(),
                variant: "localDebugOptimized".to_string(),
                created_at: "2026-06-04T00:00:00Z".to_string(),
                source_worktree: "/tmp/worktree".to_string(),
                artifact_kind: ANDROID_APK_ARTIFACT_KIND.to_string(),
                storage_mode: "copy".to_string(),
                source_artifact_path: PathBuf::from("/tmp/worktree/build/app.apk"),
                artifact_digest_algorithm: "sha256".to_string(),
                artifact_digest: "digest".to_string(),
            },
            artifact_path: PathBuf::from("/tmp/app.apk"),
        };
        let state = AndroidCacheState::Hit(Box::new(hit.clone()));

        assert_eq!(AndroidCacheState::Unknown.hit(), None);
        assert_eq!(
            AndroidCacheState::Miss {
                fingerprint: "fingerprint".to_string(),
            }
            .hit(),
            None
        );
        assert_eq!(AndroidCacheState::Checking.hit(), None);
        assert_eq!(AndroidCacheState::Error("bad".to_string()).hit(), None);
        assert_eq!(state.hit(), Some(&hit));
    }
}
