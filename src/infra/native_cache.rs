use crate::domain::native_cache::{
    CachedIosLaunchRequest, IOS_APP_ARTIFACT_KIND, IOS_SIMULATOR_PLATFORM, IosSimulatorCacheHit,
    IosSimulatorCacheLookup, IosSimulatorCacheMetadata, ios_native_fingerprint,
};
use crate::domain::ports::native_cache_port::NativeCachePort;
use std::path::{Path, PathBuf};
use std::process::{Output, Stdio};
use tokio::process::Command;

#[derive(Debug, Default)]
pub struct LocalNativeCache;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimctlLaunchCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

pub fn native_cache_root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".cache")
        .join("ump-dash")
        .join("native-builds")
}

fn ios_entry_dir(root: &Path, fingerprint: &str) -> PathBuf {
    root.join("ios-simulator").join(fingerprint)
}

pub fn lookup_ios_simulator_in_root(
    root: &Path,
    worktree_path: PathBuf,
) -> anyhow::Result<IosSimulatorCacheLookup> {
    let fingerprint = ios_native_fingerprint(&worktree_path)?;
    let entry = ios_entry_dir(root, &fingerprint);
    let metadata_path = entry.join("metadata.json");
    if !metadata_path.exists() {
        return Ok(IosSimulatorCacheLookup::Miss { fingerprint });
    }

    let metadata: IosSimulatorCacheMetadata =
        serde_json::from_slice(&std::fs::read(&metadata_path)?)?;

    if metadata.platform != IOS_SIMULATOR_PLATFORM {
        anyhow::bail!("cached iOS metadata platform mismatch");
    }
    if metadata.artifact_kind != IOS_APP_ARTIFACT_KIND {
        anyhow::bail!("cached iOS metadata artifact kind mismatch");
    }
    if metadata.fingerprint != fingerprint {
        anyhow::bail!("cached iOS metadata fingerprint mismatch");
    }
    if metadata.bundle_id.trim().is_empty() {
        anyhow::bail!("cached iOS metadata bundle id missing");
    }

    let artifact_path = entry.join("artifact.app");
    if !artifact_path.is_dir() {
        anyhow::bail!("cached .app missing: {}", artifact_path.display());
    }

    Ok(IosSimulatorCacheLookup::Hit(IosSimulatorCacheHit {
        metadata,
        artifact_path,
    }))
}

pub fn simctl_install_args(simulator_udid: &str, app_path: &Path) -> Vec<String> {
    vec![
        "simctl".to_string(),
        "install".to_string(),
        simulator_udid.to_string(),
        app_path.to_string_lossy().into_owned(),
    ]
}

pub fn simctl_launch_command(request: &CachedIosLaunchRequest) -> SimctlLaunchCommand {
    SimctlLaunchCommand {
        program: "xcrun".to_string(),
        args: vec![
            "simctl".to_string(),
            "launch".to_string(),
            "--terminate-running-process".to_string(),
            request.simulator_udid.clone(),
            request.bundle_id.clone(),
        ],
        env: vec![(
            "SIMCTL_CHILD_RCT_METRO_PORT".to_string(),
            request.metro_port.to_string(),
        )],
    }
}

fn process_output_lines(stage: &str, output: &Output) -> Vec<String> {
    let mut lines = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let line = line.trim();
        if !line.is_empty() {
            lines.push(format!("{stage} stdout: {line}"));
        }
    }
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        let line = line.trim();
        if !line.is_empty() {
            lines.push(format!("{stage} stderr: {line}"));
        }
    }
    lines
}

fn process_failure_message(stage: &str, output: &Output) -> String {
    let mut parts = vec![format!("{stage} failed with status {}", output.status)];
    parts.extend(process_output_lines(stage, output));
    parts.join("; ")
}

#[async_trait::async_trait]
impl NativeCachePort for LocalNativeCache {
    async fn lookup_ios_simulator(
        &self,
        worktree_path: PathBuf,
    ) -> anyhow::Result<IosSimulatorCacheLookup> {
        lookup_ios_simulator_in_root(&native_cache_root(), worktree_path)
    }

    async fn install_and_launch_ios_simulator(
        &self,
        request: CachedIosLaunchRequest,
    ) -> anyhow::Result<Vec<String>> {
        let install_status = Command::new("xcrun")
            .args(simctl_install_args(
                &request.simulator_udid,
                request.app_path.as_path(),
            ))
            .stdin(Stdio::null())
            .output()
            .await?;
        if !install_status.status.success() {
            anyhow::bail!(process_failure_message("install", &install_status));
        }

        let launch = simctl_launch_command(&request);
        let mut command = Command::new(&launch.program);
        command.args(&launch.args);
        for (key, value) in &launch.env {
            command.env(key, value);
        }
        let launch_status = command.stdin(Stdio::null()).output().await?;
        if !launch_status.status.success() {
            anyhow::bail!(process_failure_message("launch", &launch_status));
        }

        let mut lines = vec![
            format!("installed {}", request.app_path.display()),
            format!(
                "launched {} on {} with Metro port {}",
                request.bundle_id, request.simulator_udid, request.metro_port
            ),
        ];
        lines.extend(process_output_lines("install", &install_status));
        lines.extend(process_output_lines("launch", &launch_status));
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::native_cache::{
        CachedIosLaunchRequest, IOS_APP_ARTIFACT_KIND, IOS_SIMULATOR_PLATFORM,
        IosSimulatorCacheMetadata, ios_native_fingerprint,
    };
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::process::ExitStatusExt;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempTree {
        path: PathBuf,
    }

    impl TempTree {
        fn new(name: &str) -> anyhow::Result<Self> {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            let path = std::env::temp_dir().join(format!(
                "ump-dash-native-cache-{name}-{}-{stamp}",
                std::process::id()
            ));
            fs::create_dir_all(&path)?;
            Ok(Self { path })
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempTree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn seed_fingerprint_files(worktree: &Path) -> anyhow::Result<()> {
        fs::create_dir_all(worktree.join("ios"))?;
        fs::write(worktree.join("yarn.lock"), "yarn lock")?;
        fs::write(worktree.join("package.json"), "{}")?;
        fs::write(worktree.join("ios/Podfile"), "platform :ios, '15.1'")?;
        Ok(())
    }

    fn write_metadata(
        entry: &Path,
        fingerprint: &str,
        bundle_id: &str,
        worktree: &Path,
    ) -> anyhow::Result<()> {
        let metadata = IosSimulatorCacheMetadata {
            platform: IOS_SIMULATOR_PLATFORM.to_string(),
            fingerprint: fingerprint.to_string(),
            bundle_id: bundle_id.to_string(),
            variant: "debug".to_string(),
            created_at: "2026-05-31T00:00:00Z".to_string(),
            source_worktree: worktree.display().to_string(),
            artifact_kind: IOS_APP_ARTIFACT_KIND.to_string(),
        };
        fs::write(
            entry.join("metadata.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
        Ok(())
    }

    #[test]
    fn lookup_returns_valid_cache_hit() -> anyhow::Result<()> {
        let root = TempTree::new("root")?;
        let worktree = TempTree::new("worktree")?;
        seed_fingerprint_files(worktree.path())?;
        let fingerprint = ios_native_fingerprint(worktree.path())?;
        let entry = root.path().join("ios-simulator").join(&fingerprint);
        fs::create_dir_all(entry.join("artifact.app"))?;
        write_metadata(
            &entry,
            &fingerprint,
            "com.aljazeera.dashboard",
            worktree.path(),
        )?;

        let hit = match lookup_ios_simulator_in_root(root.path(), worktree.path().to_path_buf())? {
            IosSimulatorCacheLookup::Hit(hit) => hit,
            IosSimulatorCacheLookup::Miss { fingerprint } => {
                panic!("expected cache hit, got miss for {fingerprint}")
            }
        };

        assert_eq!(hit.metadata.bundle_id, "com.aljazeera.dashboard");
        assert_eq!(hit.artifact_path, entry.join("artifact.app"));
        Ok(())
    }

    #[test]
    fn lookup_returns_miss_with_fingerprint_when_artifact_absent() -> anyhow::Result<()> {
        let root = TempTree::new("miss-root")?;
        let worktree = TempTree::new("miss-worktree")?;
        seed_fingerprint_files(worktree.path())?;
        let fingerprint = ios_native_fingerprint(worktree.path())?;

        let lookup = lookup_ios_simulator_in_root(root.path(), worktree.path().to_path_buf())?;

        assert_eq!(lookup, IosSimulatorCacheLookup::Miss { fingerprint });
        Ok(())
    }

    #[test]
    fn simctl_launch_command_sets_metro_port_env() {
        let request = CachedIosLaunchRequest {
            simulator_udid: "SIM-123".to_string(),
            app_path: PathBuf::from("/tmp/App.app"),
            bundle_id: "com.aljazeera.dashboard".to_string(),
            metro_port: 19001,
        };

        let command = simctl_launch_command(&request);

        assert_eq!(command.program, "xcrun");
        assert_eq!(
            command.args,
            vec![
                "simctl",
                "launch",
                "--terminate-running-process",
                "SIM-123",
                "com.aljazeera.dashboard"
            ]
        );
        assert_eq!(
            command.env,
            vec![(
                "SIMCTL_CHILD_RCT_METRO_PORT".to_string(),
                "19001".to_string()
            )]
        );
    }

    #[test]
    fn lookup_errors_when_cached_app_missing() -> anyhow::Result<()> {
        let root = TempTree::new("missing-app-root")?;
        let worktree = TempTree::new("missing-app-worktree")?;
        seed_fingerprint_files(worktree.path())?;
        let fingerprint = ios_native_fingerprint(worktree.path())?;
        let entry = root.path().join("ios-simulator").join(&fingerprint);
        fs::create_dir_all(&entry)?;
        write_metadata(
            &entry,
            &fingerprint,
            "com.aljazeera.dashboard",
            worktree.path(),
        )?;

        let err = lookup_ios_simulator_in_root(root.path(), worktree.path().to_path_buf())
            .expect_err("missing app should fail validation");

        assert!(
            err.to_string().contains("cached .app missing"),
            "unexpected error: {err}"
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn process_output_lines_include_non_empty_stdout_and_stderr() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: b"installed ok\n\nnext line\n".to_vec(),
            stderr: b"warning line\n".to_vec(),
        };

        assert_eq!(
            process_output_lines("install", &output),
            vec![
                "install stdout: installed ok".to_string(),
                "install stdout: next line".to_string(),
                "install stderr: warning line".to_string(),
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_failure_message_includes_stage_status_stdout_and_stderr() {
        let output = std::process::Output {
            status: std::process::ExitStatus::from_raw(1),
            stdout: b"launch stdout detail\n".to_vec(),
            stderr: b"launch stderr detail\n".to_vec(),
        };

        let message = process_failure_message("launch", &output);

        assert!(
            message.contains("launch failed with status"),
            "unexpected message: {message}"
        );
        assert!(
            message.contains("launch stdout: launch stdout detail"),
            "unexpected message: {message}"
        );
        assert!(
            message.contains("launch stderr: launch stderr detail"),
            "unexpected message: {message}"
        );
    }
}
