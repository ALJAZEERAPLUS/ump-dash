use crate::domain::command::{android_avd_name, android_boot_avd_command};
use crate::domain::native_cache::{
    ANDROID_APK_ARTIFACT_KIND, ANDROID_PLATFORM, AndroidCacheHit, AndroidCacheLookup,
    AndroidCacheMetadata, AndroidCacheStoreRequest, CachedAndroidLaunchRequest,
    CachedIosLaunchRequest, IOS_APP_ARTIFACT_KIND, IOS_SIMULATOR_PLATFORM, IosSimulatorCacheHit,
    IosSimulatorCacheLookup, IosSimulatorCacheMetadata, IosSimulatorCacheStoreRequest,
    android_native_fingerprint, ios_native_fingerprint,
};
use crate::domain::ports::native_cache_port::NativeCachePort;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Command as StdCommand, Output, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::process::Command as TokioCommand;

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

fn android_entry_dir(root: &Path, fingerprint: &str) -> PathBuf {
    root.join(ANDROID_PLATFORM).join(fingerprint)
}

fn default_derived_data_root() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|home| {
        PathBuf::from(home)
            .join("Library")
            .join("Developer")
            .join("Xcode")
            .join("DerivedData")
    })
}

fn created_at_string() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn ios_project_app_names(worktree_path: &Path) -> Vec<String> {
    let ios_dir = worktree_path.join("ios");
    let Ok(entries) = std::fs::read_dir(ios_dir) else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            (path.extension().and_then(|ext| ext.to_str()) == Some("xcodeproj"))
                .then(|| path.file_stem()?.to_str().map(str::to_string))
                .flatten()
        })
        .filter(|name| name != "Pods")
        .collect()
}

fn read_bundle_identifier_from_xml(contents: &str) -> Option<String> {
    let key_index = contents.find("<key>CFBundleIdentifier</key>")?;
    let after_key = &contents[key_index..];
    let string_start = after_key.find("<string>")? + "<string>".len();
    let after_string = &after_key[string_start..];
    let string_end = after_string.find("</string>")?;
    let value = after_string[..string_end].trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn read_bundle_identifier(app_path: &Path) -> anyhow::Result<String> {
    let info_plist = app_path.join("Info.plist");
    if let Ok(contents) = std::fs::read_to_string(&info_plist)
        && let Some(bundle_id) = read_bundle_identifier_from_xml(&contents)
    {
        return Ok(bundle_id);
    }

    let output = StdCommand::new("plutil")
        .args(["-extract", "CFBundleIdentifier", "raw"])
        .arg(&info_plist)
        .output()?;
    if !output.status.success() {
        anyhow::bail!("failed to read bundle id from {}", info_plist.display());
    }
    let bundle_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if bundle_id.is_empty() {
        anyhow::bail!("bundle id missing in {}", info_plist.display());
    }
    Ok(bundle_id)
}

fn is_simulator_app(path: &Path, allowed_app_names: &[String]) -> bool {
    if !path.is_dir() || path.extension().and_then(|ext| ext.to_str()) != Some("app") {
        return false;
    }
    if !path.join("Info.plist").is_file() {
        return false;
    }
    if !path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .map(|name| name.contains("iphonesimulator"))
        .unwrap_or(false)
    {
        return false;
    }
    if allowed_app_names.is_empty() {
        return true;
    }
    path.file_stem()
        .and_then(|name| name.to_str())
        .map(|name| allowed_app_names.iter().any(|allowed| allowed == name))
        .unwrap_or(false)
}

fn collect_app_candidates_from_products(
    products_dir: &Path,
    allowed_app_names: &[String],
    candidates: &mut Vec<PathBuf>,
) {
    let mut stack = vec![products_dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if is_simulator_app(&entry_path, allowed_app_names) {
                candidates.push(entry_path);
                continue;
            }
            if entry.file_type().map(|ty| ty.is_dir()).unwrap_or(false) {
                stack.push(entry_path);
            }
        }
    }
}

fn find_latest_ios_simulator_app(
    worktree_path: &Path,
    derived_data_root: Option<&Path>,
) -> anyhow::Result<PathBuf> {
    let allowed_app_names = ios_project_app_names(worktree_path);
    let mut candidates = Vec::new();
    collect_app_candidates_from_products(
        &worktree_path.join("ios/build/Build/Products"),
        &allowed_app_names,
        &mut candidates,
    );

    if let Some(derived_data_root) = derived_data_root
        && let Ok(entries) = std::fs::read_dir(derived_data_root)
    {
        for entry in entries.flatten() {
            collect_app_candidates_from_products(
                &entry.path().join("Build/Products"),
                &allowed_app_names,
                &mut candidates,
            );
        }
    }

    candidates
        .into_iter()
        .filter_map(|path| {
            let modified = std::fs::metadata(path.join("Info.plist"))
                .and_then(|metadata| metadata.modified())
                .ok()?;
            Some((modified, path))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, path)| path)
        .ok_or_else(|| anyhow::anyhow!("no built iOS simulator .app found"))
}

#[derive(Debug, Deserialize)]
struct AndroidOutputMetadata {
    #[serde(rename = "applicationId")]
    application_id: String,
    #[serde(rename = "variantName")]
    variant_name: String,
    elements: Vec<AndroidOutputElement>,
}

#[derive(Debug, Deserialize)]
struct AndroidOutputElement {
    #[serde(rename = "outputFile")]
    output_file: String,
}

#[derive(Debug, Clone)]
struct AndroidApkCandidate {
    apk_path: PathBuf,
    application_id: String,
    variant_name: String,
}

fn android_apk_candidates_from_metadata(
    metadata_path: &Path,
) -> anyhow::Result<Vec<AndroidApkCandidate>> {
    let metadata: AndroidOutputMetadata = serde_json::from_slice(&std::fs::read(metadata_path)?)?;
    if metadata.application_id.trim().is_empty() {
        anyhow::bail!("Android output metadata applicationId missing");
    }
    if metadata.variant_name.trim().is_empty() {
        anyhow::bail!("Android output metadata variantName missing");
    }

    let output_dir = metadata_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Android output metadata has no parent directory"))?;
    Ok(metadata
        .elements
        .into_iter()
        .filter(|element| element.output_file.ends_with(".apk"))
        .map(|element| AndroidApkCandidate {
            apk_path: output_dir.join(element.output_file),
            application_id: metadata.application_id.clone(),
            variant_name: metadata.variant_name.clone(),
        })
        .collect())
}

fn collect_android_apk_candidates(
    root: &Path,
    candidates: &mut Vec<AndroidApkCandidate>,
) -> anyhow::Result<()> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_android_apk_candidates(&path, candidates)?;
        } else if path.file_name().and_then(|name| name.to_str()) == Some("output-metadata.json") {
            candidates.extend(android_apk_candidates_from_metadata(&path)?);
        }
    }

    Ok(())
}

fn find_latest_android_apk(worktree_path: &Path) -> anyhow::Result<AndroidApkCandidate> {
    let mut candidates = Vec::new();
    collect_android_apk_candidates(
        &worktree_path.join("android/app/build/outputs/apk"),
        &mut candidates,
    )?;

    candidates
        .into_iter()
        .filter(|candidate| candidate.apk_path.is_file())
        .filter_map(|candidate| {
            let modified = std::fs::metadata(&candidate.apk_path)
                .and_then(|metadata| metadata.modified())
                .ok()?;
            Some((modified, candidate))
        })
        .max_by_key(|(modified, _)| *modified)
        .map(|(_, candidate)| candidate)
        .ok_or_else(|| anyhow::anyhow!("no built Android APK found"))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&src_path, &dst_path)?;
        } else if file_type.is_symlink() {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(std::fs::read_link(&src_path)?, &dst_path)?;
            }
        }
    }
    Ok(())
}

pub fn store_ios_simulator_in_root(
    root: &Path,
    request: IosSimulatorCacheStoreRequest,
) -> anyhow::Result<IosSimulatorCacheHit> {
    store_ios_simulator_in_root_with_derived_data(
        root,
        request,
        default_derived_data_root().as_deref(),
    )
}

fn store_ios_simulator_in_root_with_derived_data(
    root: &Path,
    request: IosSimulatorCacheStoreRequest,
    derived_data_root: Option<&Path>,
) -> anyhow::Result<IosSimulatorCacheHit> {
    let fingerprint = ios_native_fingerprint(&request.worktree_path)?;
    let app_path = find_latest_ios_simulator_app(&request.worktree_path, derived_data_root)?;
    let bundle_id = read_bundle_identifier(&app_path)?;
    let entry = ios_entry_dir(root, &fingerprint);
    let artifact_path = entry.join("artifact.app");

    std::fs::create_dir_all(&entry)?;
    if artifact_path.exists() {
        if artifact_path.is_dir() {
            std::fs::remove_dir_all(&artifact_path)?;
        } else {
            std::fs::remove_file(&artifact_path)?;
        }
    }
    copy_dir_recursive(&app_path, &artifact_path)?;

    let metadata = IosSimulatorCacheMetadata {
        platform: IOS_SIMULATOR_PLATFORM.into(),
        fingerprint,
        bundle_id,
        variant: request.variant,
        created_at: created_at_string(),
        source_worktree: request.worktree_path.display().to_string(),
        artifact_kind: IOS_APP_ARTIFACT_KIND.into(),
    };
    std::fs::write(
        entry.join("metadata.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    Ok(IosSimulatorCacheHit {
        metadata,
        artifact_path,
    })
}

pub fn store_android_in_root(
    root: &Path,
    request: AndroidCacheStoreRequest,
) -> anyhow::Result<AndroidCacheHit> {
    let fingerprint = android_native_fingerprint(&request.worktree_path)?;
    let apk = find_latest_android_apk(&request.worktree_path)?;
    let entry = android_entry_dir(root, &fingerprint);
    let artifact_path = entry.join("artifact.apk");

    std::fs::create_dir_all(&entry)?;
    if artifact_path.exists() {
        if artifact_path.is_dir() {
            std::fs::remove_dir_all(&artifact_path)?;
        } else {
            std::fs::remove_file(&artifact_path)?;
        }
    }
    std::fs::copy(&apk.apk_path, &artifact_path)?;

    let metadata = AndroidCacheMetadata {
        platform: ANDROID_PLATFORM.into(),
        fingerprint,
        application_id: apk.application_id,
        variant: apk.variant_name,
        created_at: created_at_string(),
        source_worktree: request.worktree_path.display().to_string(),
        artifact_kind: ANDROID_APK_ARTIFACT_KIND.into(),
    };
    std::fs::write(
        entry.join("metadata.json"),
        serde_json::to_string_pretty(&metadata)?,
    )?;

    Ok(AndroidCacheHit {
        metadata,
        artifact_path,
    })
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

pub fn lookup_android_in_root(
    root: &Path,
    worktree_path: PathBuf,
) -> anyhow::Result<AndroidCacheLookup> {
    let fingerprint = android_native_fingerprint(&worktree_path)?;
    let entry = android_entry_dir(root, &fingerprint);
    let metadata_path = entry.join("metadata.json");
    if !metadata_path.exists() {
        return Ok(AndroidCacheLookup::Miss { fingerprint });
    }

    let metadata: AndroidCacheMetadata = serde_json::from_slice(&std::fs::read(&metadata_path)?)?;

    if metadata.platform != ANDROID_PLATFORM {
        anyhow::bail!("cached Android metadata platform mismatch");
    }
    if metadata.artifact_kind != ANDROID_APK_ARTIFACT_KIND {
        anyhow::bail!("cached Android metadata artifact kind mismatch");
    }
    if metadata.fingerprint != fingerprint {
        anyhow::bail!("cached Android metadata fingerprint mismatch");
    }
    if metadata.application_id.trim().is_empty() {
        anyhow::bail!("cached Android metadata application id missing");
    }

    let artifact_path = entry.join("artifact.apk");
    if !artifact_path.is_file() {
        anyhow::bail!("cached APK missing: {}", artifact_path.display());
    }

    Ok(AndroidCacheLookup::Hit(AndroidCacheHit {
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

pub fn simctl_bootstatus_command(simulator_udid: &str) -> SimctlLaunchCommand {
    SimctlLaunchCommand {
        program: "xcrun".to_string(),
        args: vec![
            "simctl".to_string(),
            "bootstatus".to_string(),
            simulator_udid.to_string(),
            "-b".to_string(),
        ],
        env: Vec::new(),
    }
}

pub fn simctl_set_js_location_command(request: &CachedIosLaunchRequest) -> SimctlLaunchCommand {
    SimctlLaunchCommand {
        program: "xcrun".to_string(),
        args: vec![
            "simctl".to_string(),
            "spawn".to_string(),
            request.simulator_udid.clone(),
            "defaults".to_string(),
            "write".to_string(),
            request.bundle_id.clone(),
            "RCT_jsLocation".to_string(),
            "-string".to_string(),
            format!("localhost:{}", request.metro_port),
        ],
        env: Vec::new(),
    }
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

pub fn adb_install_args(device_id: &str, apk_path: &Path) -> Vec<String> {
    vec![
        "-s".to_string(),
        device_id.to_string(),
        "install".to_string(),
        "-r".to_string(),
        "-d".to_string(),
        apk_path.to_string_lossy().into_owned(),
    ]
}

pub fn adb_reverse_args(device_id: &str, metro_port: u16) -> Vec<String> {
    let tcp = format!("tcp:{metro_port}");
    vec![
        "-s".to_string(),
        device_id.to_string(),
        "reverse".to_string(),
        tcp.clone(),
        tcp,
    ]
}

fn adb_force_stop_args(device_id: &str, application_id: &str) -> Vec<String> {
    vec![
        "-s".to_string(),
        device_id.to_string(),
        "shell".to_string(),
        "am".to_string(),
        "force-stop".to_string(),
        application_id.to_string(),
    ]
}

fn adb_resolve_activity_args(device_id: &str, application_id: &str) -> Vec<String> {
    vec![
        "-s".to_string(),
        device_id.to_string(),
        "shell".to_string(),
        "cmd".to_string(),
        "package".to_string(),
        "resolve-activity".to_string(),
        "--brief".to_string(),
        application_id.to_string(),
    ]
}

fn adb_launch_args(device_id: &str, component_name: &str) -> Vec<String> {
    vec![
        "-s".to_string(),
        device_id.to_string(),
        "shell".to_string(),
        "am".to_string(),
        "start".to_string(),
        "-n".to_string(),
        component_name.to_string(),
    ]
}

pub fn parse_android_launch_activity(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| line.contains('/'))
        .filter(|line| !line.is_empty())
        .next_back()
        .map(str::to_string)
}

pub fn parse_running_emulator_serials(output: &str) -> Vec<String> {
    output
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let serial = fields.next()?;
            let state = fields.next()?;
            (serial.starts_with("emulator-") && state == "device").then(|| serial.to_string())
        })
        .collect()
}

pub fn parse_adb_emu_avd_name(output: &str) -> Option<String> {
    output
        .lines()
        .map(|line| line.trim().trim_end_matches('\r'))
        .find(|line| !line.is_empty() && *line != "OK")
        .map(str::to_string)
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn upsert_debug_http_host_preference(contents: &str, host: &str) -> String {
    let entry = format!(
        "<string name=\"debug_http_host\">{}</string>",
        xml_escape(host)
    );
    let start_tag = "<string name=\"debug_http_host\">";
    if let Some(start) = contents.find(start_tag)
        && let Some(end_offset) = contents[start..].find("</string>")
    {
        let end = start + end_offset + "</string>".len();
        let mut updated = contents.to_string();
        updated.replace_range(start..end, &entry);
        return updated;
    }

    if let Some(end) = contents.rfind("</map>") {
        let mut updated = contents.to_string();
        updated.insert_str(end, &format!("{entry}\n"));
        return updated;
    }

    format!("<?xml version='1.0' encoding='utf-8' standalone='yes' ?>\n<map>\n{entry}\n</map>\n")
}

async fn run_adb_stage(stage: &str, args: Vec<String>) -> anyhow::Result<Output> {
    let output = TokioCommand::new("adb")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!(process_failure_message(stage, &output));
    }
    Ok(output)
}

async fn resolve_android_adb_device_id(device_id: &str) -> anyhow::Result<String> {
    let Some(avd_name) = android_avd_name(device_id) else {
        return Ok(device_id.to_string());
    };

    let boot_output = TokioCommand::new("sh")
        .args(["-c", &android_boot_avd_command(avd_name)])
        .stdin(Stdio::null())
        .output()
        .await?;
    if !boot_output.status.success() {
        anyhow::bail!(process_failure_message("avd-boot", &boot_output));
    }

    let devices_output = TokioCommand::new("adb")
        .arg("devices")
        .stdin(Stdio::null())
        .output()
        .await?;
    if !devices_output.status.success() {
        anyhow::bail!(process_failure_message("adb-devices", &devices_output));
    }
    let devices_stdout = String::from_utf8_lossy(&devices_output.stdout);
    for serial in parse_running_emulator_serials(&devices_stdout) {
        let name_output = TokioCommand::new("adb")
            .args(["-s", &serial, "emu", "avd", "name"])
            .stdin(Stdio::null())
            .output()
            .await?;
        if !name_output.status.success() {
            continue;
        }
        let name_stdout = String::from_utf8_lossy(&name_output.stdout);
        if parse_adb_emu_avd_name(&name_stdout).as_deref() == Some(avd_name) {
            return Ok(serial);
        }
    }

    anyhow::bail!("could not resolve AVD {avd_name} to a running adb serial")
}

async fn update_android_debug_http_host(
    device_id: &str,
    application_id: &str,
    metro_port: u16,
) -> anyhow::Result<Output> {
    let prefs_name = format!("{application_id}_preferences.xml");
    let prefs_path = format!("shared_prefs/{prefs_name}");
    let read_output = TokioCommand::new("adb")
        .args([
            "-s",
            device_id,
            "exec-out",
            "run-as",
            application_id,
            "cat",
            &prefs_path,
        ])
        .stdin(Stdio::null())
        .output()
        .await?;
    let existing = if read_output.status.success() {
        String::from_utf8_lossy(&read_output.stdout).into_owned()
    } else {
        "<?xml version='1.0' encoding='utf-8' standalone='yes' ?>\n<map>\n</map>\n".to_string()
    };
    let updated = upsert_debug_http_host_preference(&existing, &format!("localhost:{metro_port}"));

    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let local_path = std::env::temp_dir().join(format!(
        "ump-dash-android-prefs-{}-{stamp}.xml",
        std::process::id()
    ));
    let remote_path = format!(
        "/data/local/tmp/ump-dash-android-prefs-{}-{stamp}.xml",
        std::process::id()
    );
    std::fs::write(&local_path, updated)?;

    let push_output = run_adb_stage(
        "prefs-push",
        vec![
            "-s".to_string(),
            device_id.to_string(),
            "push".to_string(),
            local_path.to_string_lossy().into_owned(),
            remote_path.clone(),
        ],
    )
    .await;
    let _ = std::fs::remove_file(&local_path);
    push_output?;

    run_adb_stage(
        "prefs-chmod",
        vec![
            "-s".to_string(),
            device_id.to_string(),
            "shell".to_string(),
            "chmod".to_string(),
            "644".to_string(),
            remote_path.clone(),
        ],
    )
    .await?;
    run_adb_stage(
        "prefs-mkdir",
        vec![
            "-s".to_string(),
            device_id.to_string(),
            "shell".to_string(),
            "run-as".to_string(),
            application_id.to_string(),
            "mkdir".to_string(),
            "-p".to_string(),
            "shared_prefs".to_string(),
        ],
    )
    .await?;
    run_adb_stage(
        "prefs-copy",
        vec![
            "-s".to_string(),
            device_id.to_string(),
            "shell".to_string(),
            "run-as".to_string(),
            application_id.to_string(),
            "cp".to_string(),
            remote_path,
            prefs_path,
        ],
    )
    .await
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

    async fn store_ios_simulator(
        &self,
        request: IosSimulatorCacheStoreRequest,
    ) -> anyhow::Result<IosSimulatorCacheHit> {
        store_ios_simulator_in_root(&native_cache_root(), request)
    }

    async fn lookup_android(&self, worktree_path: PathBuf) -> anyhow::Result<AndroidCacheLookup> {
        lookup_android_in_root(&native_cache_root(), worktree_path)
    }

    async fn store_android(
        &self,
        request: AndroidCacheStoreRequest,
    ) -> anyhow::Result<AndroidCacheHit> {
        store_android_in_root(&native_cache_root(), request)
    }

    async fn install_and_launch_ios_simulator(
        &self,
        request: CachedIosLaunchRequest,
    ) -> anyhow::Result<Vec<String>> {
        let bootstatus = simctl_bootstatus_command(&request.simulator_udid);
        let mut boot_command = TokioCommand::new(&bootstatus.program);
        boot_command.args(&bootstatus.args);
        let boot_status = boot_command.stdin(Stdio::null()).output().await?;
        if !boot_status.status.success() {
            anyhow::bail!(process_failure_message("bootstatus", &boot_status));
        }

        let install_status = TokioCommand::new("xcrun")
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

        let js_location = simctl_set_js_location_command(&request);
        let mut defaults_command = TokioCommand::new(&js_location.program);
        defaults_command.args(&js_location.args);
        let defaults_status = defaults_command.stdin(Stdio::null()).output().await?;
        if !defaults_status.status.success() {
            anyhow::bail!(process_failure_message("defaults", &defaults_status));
        }

        let launch = simctl_launch_command(&request);
        let mut command = TokioCommand::new(&launch.program);
        command.args(&launch.args);
        for (key, value) in &launch.env {
            command.env(key, value);
        }
        let launch_status = command.stdin(Stdio::null()).output().await?;
        if !launch_status.status.success() {
            anyhow::bail!(process_failure_message("launch", &launch_status));
        }

        let mut lines = vec![
            format!("booted simulator {}", request.simulator_udid),
            format!("installed {}", request.app_path.display()),
            format!("configured Metro location localhost:{}", request.metro_port),
            format!(
                "launched {} on {} with Metro port {}",
                request.bundle_id, request.simulator_udid, request.metro_port
            ),
        ];
        lines.extend(process_output_lines("bootstatus", &boot_status));
        lines.extend(process_output_lines("install", &install_status));
        lines.extend(process_output_lines("defaults", &defaults_status));
        lines.extend(process_output_lines("launch", &launch_status));
        Ok(lines)
    }

    async fn install_and_launch_android(
        &self,
        request: CachedAndroidLaunchRequest,
    ) -> anyhow::Result<Vec<String>> {
        let adb_device_id = resolve_android_adb_device_id(&request.device_id).await?;

        let install_status = run_adb_stage(
            "install",
            adb_install_args(&adb_device_id, request.apk_path.as_path()),
        )
        .await?;

        let force_stop_before_prefs = run_adb_stage(
            "force-stop",
            adb_force_stop_args(&adb_device_id, &request.application_id),
        )
        .await?;

        let prefs_status = update_android_debug_http_host(
            &adb_device_id,
            &request.application_id,
            request.metro_port,
        )
        .await?;

        let reverse_status = run_adb_stage(
            "reverse",
            adb_reverse_args(&adb_device_id, request.metro_port),
        )
        .await?;

        let resolve_status = run_adb_stage(
            "resolve-activity",
            adb_resolve_activity_args(&adb_device_id, &request.application_id),
        )
        .await?;
        let resolve_stdout = String::from_utf8_lossy(&resolve_status.stdout);
        let component_name = parse_android_launch_activity(&resolve_stdout).ok_or_else(|| {
            anyhow::anyhow!(
                "could not resolve Android launcher activity for {}",
                request.application_id
            )
        })?;

        let launch_status =
            run_adb_stage("launch", adb_launch_args(&adb_device_id, &component_name)).await?;

        let mut lines = vec![
            format!("installed {}", request.apk_path.display()),
            format!(
                "configured Android debug host localhost:{}",
                request.metro_port
            ),
            format!(
                "reversed tcp:{} to device {}",
                request.metro_port, adb_device_id
            ),
            format!(
                "launched {} on {} with Metro port {}",
                request.application_id, adb_device_id, request.metro_port
            ),
        ];
        lines.extend(process_output_lines("install", &install_status));
        lines.extend(process_output_lines("force-stop", &force_stop_before_prefs));
        lines.extend(process_output_lines("prefs-copy", &prefs_status));
        lines.extend(process_output_lines("reverse", &reverse_status));
        lines.extend(process_output_lines("resolve-activity", &resolve_status));
        lines.extend(process_output_lines("launch", &launch_status));
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::native_cache::{
        ANDROID_APK_ARTIFACT_KIND, ANDROID_PLATFORM, AndroidCacheLookup, AndroidCacheMetadata,
        AndroidCacheStoreRequest, CachedIosLaunchRequest, IOS_APP_ARTIFACT_KIND,
        IOS_SIMULATOR_PLATFORM, IosSimulatorCacheMetadata, IosSimulatorCacheStoreRequest,
        android_native_fingerprint, ios_native_fingerprint,
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

    fn seed_xcode_project(worktree: &Path, name: &str) -> anyhow::Result<()> {
        fs::create_dir_all(worktree.join("ios").join(format!("{name}.xcodeproj")))?;
        Ok(())
    }

    fn seed_simulator_app(app: &Path, bundle_id: &str) -> anyhow::Result<()> {
        fs::create_dir_all(app)?;
        fs::write(
            app.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>{bundle_id}</string>
</dict>
</plist>
"#
            ),
        )?;
        fs::write(app.join("AppBinary"), "binary")?;
        Ok(())
    }

    fn seed_android_fingerprint_files(worktree: &Path) -> anyhow::Result<()> {
        fs::create_dir_all(worktree.join("android/app"))?;
        fs::write(worktree.join("yarn.lock"), "yarn lock")?;
        fs::write(worktree.join("package.json"), "{}")?;
        fs::write(
            worktree.join("android/settings.gradle"),
            "pluginManagement {}",
        )?;
        fs::write(worktree.join("android/build.gradle"), "plugins {}")?;
        fs::write(worktree.join("android/app/build.gradle"), "android {}")?;
        Ok(())
    }

    fn write_android_metadata(
        entry: &Path,
        fingerprint: &str,
        application_id: &str,
        worktree: &Path,
    ) -> anyhow::Result<()> {
        let metadata = AndroidCacheMetadata {
            platform: ANDROID_PLATFORM.to_string(),
            fingerprint: fingerprint.to_string(),
            application_id: application_id.to_string(),
            variant: "localDebugOptimized".to_string(),
            created_at: "2026-06-04T00:00:00Z".to_string(),
            source_worktree: worktree.display().to_string(),
            artifact_kind: ANDROID_APK_ARTIFACT_KIND.to_string(),
        };
        fs::write(
            entry.join("metadata.json"),
            serde_json::to_string_pretty(&metadata)?,
        )?;
        Ok(())
    }

    fn seed_android_apk_output(
        worktree: &Path,
        application_id: &str,
        variant_name: &str,
        output_file: &str,
        apk_contents: &str,
    ) -> anyhow::Result<()> {
        let output_dir = worktree.join("android/app/build/outputs/apk/local/debugOptimized");
        fs::create_dir_all(&output_dir)?;
        fs::write(output_dir.join(output_file), apk_contents)?;
        fs::write(
            output_dir.join("output-metadata.json"),
            format!(
                r#"{{
  "version": 3,
  "artifactType": {{"type": "APK", "kind": "Directory"}},
  "applicationId": "{application_id}",
  "variantName": "{variant_name}",
  "elements": [
    {{
      "type": "SINGLE",
      "filters": [],
      "attributes": [],
      "versionCode": 1,
      "versionName": "1.0",
      "outputFile": "{output_file}"
    }}
  ],
  "elementType": "File"
}}"#
            ),
        )?;
        Ok(())
    }

    #[test]
    fn android_lookup_returns_valid_cache_hit() -> anyhow::Result<()> {
        let root = TempTree::new("android-root")?;
        let worktree = TempTree::new("android-worktree")?;
        seed_android_fingerprint_files(worktree.path())?;
        let fingerprint = android_native_fingerprint(worktree.path())?;
        let entry = root.path().join("android").join(&fingerprint);
        fs::create_dir_all(&entry)?;
        fs::write(entry.join("artifact.apk"), "apk")?;
        write_android_metadata(
            &entry,
            &fingerprint,
            "com.aljazeera.mobile.local",
            worktree.path(),
        )?;

        let lookup = lookup_android_in_root(root.path(), worktree.path().to_path_buf())?;

        match lookup {
            AndroidCacheLookup::Hit(hit) => {
                assert_eq!(hit.metadata.application_id, "com.aljazeera.mobile.local");
                assert_eq!(hit.metadata.variant, "localDebugOptimized");
                assert_eq!(hit.artifact_path, entry.join("artifact.apk"));
            }
            AndroidCacheLookup::Miss { fingerprint } => {
                panic!("expected Android cache hit, got miss for {fingerprint}")
            }
        }
        Ok(())
    }

    #[test]
    fn android_lookup_returns_miss_with_fingerprint_when_artifact_absent() -> anyhow::Result<()> {
        let root = TempTree::new("android-root")?;
        let worktree = TempTree::new("android-worktree")?;
        seed_android_fingerprint_files(worktree.path())?;
        let expected = android_native_fingerprint(worktree.path())?;

        let lookup = lookup_android_in_root(root.path(), worktree.path().to_path_buf())?;

        assert_eq!(
            lookup,
            AndroidCacheLookup::Miss {
                fingerprint: expected
            }
        );
        Ok(())
    }

    #[test]
    fn android_lookup_errors_when_cached_apk_missing() -> anyhow::Result<()> {
        let root = TempTree::new("android-root")?;
        let worktree = TempTree::new("android-worktree")?;
        seed_android_fingerprint_files(worktree.path())?;
        let fingerprint = android_native_fingerprint(worktree.path())?;
        let entry = root.path().join("android").join(&fingerprint);
        fs::create_dir_all(&entry)?;
        write_android_metadata(
            &entry,
            &fingerprint,
            "com.aljazeera.mobile.local",
            worktree.path(),
        )?;

        let err = lookup_android_in_root(root.path(), worktree.path().to_path_buf())
            .expect_err("missing cached APK should error");

        assert!(
            err.to_string().contains("cached APK missing"),
            "unexpected error: {err}"
        );
        Ok(())
    }

    #[test]
    fn android_store_copies_apk_and_writes_metadata() -> anyhow::Result<()> {
        let root = TempTree::new("android-root")?;
        let worktree = TempTree::new("android-worktree")?;
        seed_android_fingerprint_files(worktree.path())?;
        seed_android_apk_output(
            worktree.path(),
            "com.aljazeera.mobile.local",
            "localDebugOptimized",
            "app-local-debugOptimized.apk",
            "apk-binary",
        )?;

        let hit = store_android_in_root(
            root.path(),
            AndroidCacheStoreRequest {
                worktree_path: worktree.path().to_path_buf(),
                variant: "local".to_string(),
            },
        )?;

        let fingerprint = android_native_fingerprint(worktree.path())?;
        let entry = root.path().join("android").join(&fingerprint);
        assert_eq!(hit.artifact_path, entry.join("artifact.apk"));
        assert_eq!(
            fs::read_to_string(entry.join("artifact.apk"))?,
            "apk-binary"
        );
        assert_eq!(hit.metadata.fingerprint, fingerprint);
        assert_eq!(hit.metadata.application_id, "com.aljazeera.mobile.local");
        assert_eq!(hit.metadata.variant, "localDebugOptimized");
        assert_eq!(hit.metadata.artifact_kind, ANDROID_APK_ARTIFACT_KIND);
        Ok(())
    }

    #[test]
    fn adb_install_args_include_device_and_cached_apk() {
        assert_eq!(
            adb_install_args("emulator-5554", Path::new("/tmp/cached.apk")),
            vec![
                "-s",
                "emulator-5554",
                "install",
                "-r",
                "-d",
                "/tmp/cached.apk"
            ]
        );
    }

    #[test]
    fn adb_reverse_args_targets_selected_metro_port() {
        assert_eq!(
            adb_reverse_args("emulator-5554", 8099),
            vec!["-s", "emulator-5554", "reverse", "tcp:8099", "tcp:8099"]
        );
    }

    #[test]
    fn android_launch_activity_parser_uses_last_component_line() {
        assert_eq!(
            parse_android_launch_activity(
                "priority=0 preferredOrder=0 match=0x108000 specificIndex=-1 isDefault=true\ncom.example/.MainActivity\n"
            ),
            Some("com.example/.MainActivity".to_string())
        );
    }

    #[test]
    fn adb_devices_parser_returns_running_emulator_serials_only() {
        assert_eq!(
            parse_running_emulator_serials(
                "List of devices attached\nemulator-5554\tdevice product:sdk\nabc123\tdevice\nemulator-5556\toffline\n"
            ),
            vec!["emulator-5554".to_string()]
        );
    }

    #[test]
    fn adb_emu_avd_name_parser_ignores_ok_line() {
        assert_eq!(
            parse_adb_emu_avd_name("Pixel_9a\r\nOK\r\n"),
            Some("Pixel_9a".to_string())
        );
    }

    #[test]
    fn debug_http_host_preference_xml_is_inserted_or_updated() {
        let inserted = upsert_debug_http_host_preference(
            "<?xml version='1.0' encoding='utf-8' standalone='yes' ?>\n<map>\n</map>\n",
            "localhost:8099",
        );
        assert!(inserted.contains("<string name=\"debug_http_host\">localhost:8099</string>"));

        let updated = upsert_debug_http_host_preference(
            "<map>\n<string name=\"debug_http_host\">localhost:8081</string>\n</map>\n",
            "localhost:9000",
        );
        assert!(updated.contains("<string name=\"debug_http_host\">localhost:9000</string>"));
        assert!(!updated.contains("localhost:8081"));
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
    fn store_copies_worktree_build_app_and_writes_metadata() -> anyhow::Result<()> {
        let root = TempTree::new("store-root")?;
        let worktree = TempTree::new("store-worktree")?;
        seed_fingerprint_files(worktree.path())?;
        seed_xcode_project(worktree.path(), "AlJazeeraMobile")?;
        let app = worktree
            .path()
            .join("ios/build/Build/Products/Debug-iphonesimulator/AlJazeeraMobile.app");
        seed_simulator_app(&app, "com.aljazeera.mobile.local")?;
        let fingerprint = ios_native_fingerprint(worktree.path())?;

        let hit = store_ios_simulator_in_root_with_derived_data(
            root.path(),
            IosSimulatorCacheStoreRequest {
                worktree_path: worktree.path().to_path_buf(),
                variant: "dev".into(),
            },
            None,
        )?;

        assert_eq!(hit.metadata.fingerprint, fingerprint);
        assert_eq!(hit.metadata.bundle_id, "com.aljazeera.mobile.local");
        assert_eq!(hit.metadata.variant, "dev");
        assert!(hit.artifact_path.join("Info.plist").is_file());
        assert!(hit.artifact_path.join("AppBinary").is_file());
        assert!(
            root.path()
                .join("ios-simulator")
                .join(&fingerprint)
                .join("metadata.json")
                .is_file()
        );
        Ok(())
    }

    #[test]
    fn store_uses_derived_data_when_worktree_build_app_is_empty() -> anyhow::Result<()> {
        let root = TempTree::new("store-derived-root")?;
        let worktree = TempTree::new("store-derived-worktree")?;
        let derived = TempTree::new("derived-data")?;
        seed_fingerprint_files(worktree.path())?;
        seed_xcode_project(worktree.path(), "AlJazeeraMobile")?;
        fs::create_dir_all(
            worktree
                .path()
                .join("ios/build/Build/Products/Debug-iphonesimulator/AlJazeeraMobile.app"),
        )?;
        let derived_app = derived
            .path()
            .join("AlJazeeraMobile-abc/Build/Products/Debug-iphonesimulator/AlJazeeraMobile.app");
        seed_simulator_app(&derived_app, "com.aljazeera.mobile.local")?;

        let hit = store_ios_simulator_in_root_with_derived_data(
            root.path(),
            IosSimulatorCacheStoreRequest {
                worktree_path: worktree.path().to_path_buf(),
                variant: "local".into(),
            },
            Some(derived.path()),
        )?;

        assert_eq!(hit.metadata.bundle_id, "com.aljazeera.mobile.local");
        assert!(hit.artifact_path.join("AppBinary").is_file());
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
    fn simctl_bootstatus_command_boots_and_waits_for_simulator() {
        let command = simctl_bootstatus_command("SIM-123");

        assert_eq!(command.program, "xcrun");
        assert_eq!(command.args, vec!["simctl", "bootstatus", "SIM-123", "-b"]);
        assert!(command.env.is_empty());
    }

    #[test]
    fn simctl_js_location_command_targets_requested_metro_port() {
        let request = CachedIosLaunchRequest {
            simulator_udid: "SIM-123".to_string(),
            app_path: PathBuf::from("/tmp/App.app"),
            bundle_id: "com.aljazeera.dashboard".to_string(),
            metro_port: 19001,
        };

        let command = simctl_set_js_location_command(&request);

        assert_eq!(command.program, "xcrun");
        assert_eq!(
            command.args,
            vec![
                "simctl",
                "spawn",
                "SIM-123",
                "defaults",
                "write",
                "com.aljazeera.dashboard",
                "RCT_jsLocation",
                "-string",
                "localhost:19001"
            ]
        );
        assert!(command.env.is_empty());
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
