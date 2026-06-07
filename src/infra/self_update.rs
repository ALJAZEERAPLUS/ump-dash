use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use tar::Archive;

const REPO: &str = "ALJAZEERAPLUS/ump-dash";
const EXPECTED_ARCHIVE_BINARY: &str = "ump-dash";
const SHA256SUMS_ASSET: &str = "SHA256SUMS";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhReleaseSummary {
    tag_name: String,
    is_draft: bool,
    is_prerelease: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GhReleaseView {
    body: Option<String>,
    assets: Vec<GhAsset>,
}

#[derive(Debug, Deserialize)]
struct GhAsset {
    name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ReleaseCandidate {
    tag: String,
    version: Version,
}

#[derive(Debug, Eq, PartialEq)]
struct ReplacementPaths {
    temp_binary: PathBuf,
    backup: PathBuf,
}

#[derive(Debug)]
struct TempDir {
    path: PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn run() -> Result<()> {
    let installed_version = parse_version(env!("CARGO_PKG_VERSION"))
        .context("compiled CARGO_PKG_VERSION is not a semantic version")?;
    let exe_path = std::env::current_exe().context("could not resolve current executable path")?;
    let install_dir = exe_path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "could not determine parent directory for {}",
            exe_path.display()
        )
    })?;
    let asset_name =
        platform_asset_name(std::env::consts::OS, std::env::consts::ARCH).ok_or_else(|| {
            anyhow::anyhow!(
                "Unsupported platform: {} {}. No update asset is defined.",
                std::env::consts::OS,
                std::env::consts::ARCH
            )
        })?;

    if is_source_checkout_build_path(&exe_path) {
        bail!(
            "Refusing to update a source checkout build at {}.\nUpdate the checkout instead:\n  git pull && cargo build --release",
            exe_path.display()
        );
    }

    ensure_gh_available_and_authenticated()?;

    let releases = list_releases()?;
    let candidates = select_newer_releases(&releases, &installed_version);
    if candidates.is_empty() {
        let latest = latest_release_version(&releases).unwrap_or_else(|| installed_version.clone());
        println!("Installed: {}", installed_version);
        println!("Latest:    {}", latest);
        println!("ump-dash is current.");
        return Ok(());
    }
    ensure_parent_directory_writable(&exe_path)?;

    let mut release_notes = Vec::new();
    for candidate in &candidates {
        let view = view_release(&candidate.tag)?;
        release_notes.push((candidate.clone(), view));
    }

    let target = release_notes
        .last()
        .map(|(candidate, view)| (candidate, view))
        .expect("candidates is non-empty");
    let latest_version = &target.0.version;
    let target_tag = &target.0.tag;
    if !target.1.assets.iter().any(|asset| asset.name == asset_name) {
        bail!(
            "No matching release asset for platform {} {}.\nExpected asset: {}",
            std::env::consts::OS,
            std::env::consts::ARCH,
            asset_name
        );
    }
    if !target
        .1
        .assets
        .iter()
        .any(|asset| asset.name == SHA256SUMS_ASSET)
    {
        bail!("Release {target_tag} is missing required {SHA256SUMS_ASSET} asset.");
    }

    println!("Installed: {}", installed_version);
    println!("Latest:    {}", latest_version);
    println!();
    println!("Changes:");
    println!();
    for (candidate, view) in &release_notes {
        println!("## {}", candidate.version);
        let body = view.body.as_deref().unwrap_or("").trim();
        if body.is_empty() {
            println!("No release notes provided.");
        } else {
            println!("{body}");
        }
        println!();
    }

    println!("Updating to {latest_version}...");
    let temp_dir = TempDir::create_in(install_dir)?;
    let archive_path = download_asset(target_tag, asset_name, &temp_dir.path)?;
    if archive_path.file_name() != Some(OsStr::new(asset_name)) {
        bail!(
            "Downloaded asset filename mismatch: expected {}, got {}",
            asset_name,
            archive_path.display()
        );
    }
    let sums_path = download_asset(target_tag, SHA256SUMS_ASSET, &temp_dir.path)?;
    verify_archive_checksum(&archive_path, &sums_path, asset_name)?;

    let paths = replacement_paths_for(&exe_path, unique_suffix());
    extract_binary_from_archive(&archive_path, &paths.temp_binary)?;
    make_executable(&paths.temp_binary)?;
    replace_current_executable(&exe_path, &paths)?;

    println!("Updated ump-dash to {latest_version}.");
    Ok(())
}

fn ensure_gh_available_and_authenticated() -> Result<()> {
    match Command::new("gh").arg("--version").output() {
        Ok(output) if output.status.success() => {}
        Ok(output) => bail!(
            "GitHub CLI (`gh`) is required for self-updates, but `gh --version` failed: {}",
            command_stderr(&output.stderr)
        ),
        Err(error) if error.kind() == io::ErrorKind::NotFound => bail!(
            "GitHub CLI (`gh`) is required for self-updates. Install GitHub CLI, then run `gh auth login`."
        ),
        Err(error) => return Err(error).context("failed to run GitHub CLI (`gh`)"),
    }

    let output = Command::new("gh")
        .args(["auth", "status", "--hostname", "github.com"])
        .output()
        .context("failed to check GitHub CLI authentication")?;
    if !output.status.success() {
        bail!(
            "GitHub CLI is not authenticated or cannot access GitHub.\nRun `gh auth login` and ensure access to {REPO}."
        );
    }
    Ok(())
}

fn list_releases() -> Result<Vec<GhReleaseSummary>> {
    let output = Command::new("gh")
        .args([
            "release",
            "list",
            "--repo",
            REPO,
            "--json",
            "tagName,isDraft,isPrerelease",
            "--limit",
            "100",
            "--order",
            "desc",
        ])
        .output()
        .context("failed to list GitHub releases with gh")?;
    if !output.status.success() {
        bail!(
            "Could not list releases for {REPO}.\nRun `gh auth login` and ensure access to {REPO}.\n{}",
            command_stderr(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout).context("failed to parse gh release list JSON")
}

fn view_release(tag: &str) -> Result<GhReleaseView> {
    let output = Command::new("gh")
        .args([
            "release",
            "view",
            tag,
            "--repo",
            REPO,
            "--json",
            "body,assets",
        ])
        .output()
        .with_context(|| format!("failed to view GitHub release {tag} with gh"))?;
    if !output.status.success() {
        bail!(
            "Could not read release {tag} for {REPO}.\nRun `gh auth login` and ensure access to {REPO}.\n{}",
            command_stderr(&output.stderr)
        );
    }
    serde_json::from_slice(&output.stdout)
        .with_context(|| format!("failed to parse gh release view JSON for {tag}"))
}

fn download_asset(tag: &str, asset_name: &str, dir: &Path) -> Result<PathBuf> {
    let before = sorted_file_names(dir)?;
    let output = Command::new("gh")
        .args([
            "release",
            "download",
            tag,
            "--repo",
            REPO,
            "--pattern",
            asset_name,
            "--dir",
        ])
        .arg(dir)
        .output()
        .with_context(|| format!("failed to download {asset_name} from release {tag}"))?;
    if !output.status.success() {
        bail!(
            "Could not download {asset_name} from release {tag}.\n{}",
            command_stderr(&output.stderr)
        );
    }

    let expected = dir.join(asset_name);
    if !expected.is_file() {
        let after = sorted_file_names(dir)?;
        bail!(
            "Release download did not produce expected asset {}.\nBefore: {:?}\nAfter: {:?}",
            asset_name,
            before,
            after
        );
    }
    Ok(expected)
}

fn select_newer_releases(
    releases: &[GhReleaseSummary],
    installed: &Version,
) -> Vec<ReleaseCandidate> {
    let mut candidates: Vec<_> = releases
        .iter()
        .filter(|release| !release.is_draft && !release.is_prerelease)
        .filter_map(|release| {
            let version = parse_version(&release.tag_name).ok()?;
            (version > *installed).then(|| ReleaseCandidate {
                tag: release.tag_name.clone(),
                version,
            })
        })
        .collect();
    candidates.sort_by(|a, b| a.version.cmp(&b.version));
    candidates
}

fn latest_release_version(releases: &[GhReleaseSummary]) -> Option<Version> {
    releases
        .iter()
        .filter(|release| !release.is_draft && !release.is_prerelease)
        .filter_map(|release| parse_version(&release.tag_name).ok())
        .max()
}

fn parse_version(tag_or_version: &str) -> Result<Version> {
    let normalized = tag_or_version
        .trim()
        .strip_prefix('v')
        .unwrap_or(tag_or_version.trim());
    Version::parse(normalized)
        .with_context(|| format!("invalid semantic version tag: {tag_or_version}"))
}

fn platform_asset_name(os: &str, arch: &str) -> Option<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Some("ump-dash-aarch64-apple-darwin.tar.gz"),
        ("macos", "x86_64") => Some("ump-dash-x86_64-apple-darwin.tar.gz"),
        ("linux", "x86_64") => Some("ump-dash-x86_64-unknown-linux-gnu.tar.gz"),
        _ => None,
    }
}

fn is_source_checkout_build_path(exe_path: &Path) -> bool {
    let Some(profile_dir) = exe_path.parent() else {
        return false;
    };
    let Some(profile_name) = profile_dir.file_name().and_then(OsStr::to_str) else {
        return false;
    };
    if !matches!(profile_name, "debug" | "release") {
        return false;
    }

    let Some(parent) = profile_dir.parent() else {
        return false;
    };
    if parent.file_name() == Some(OsStr::new("target")) {
        return true;
    }

    parent.parent().and_then(Path::file_name) == Some(OsStr::new("target"))
}

fn ensure_parent_directory_writable(exe_path: &Path) -> Result<()> {
    let parent = exe_path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "could not determine parent directory for {}",
            exe_path.display()
        )
    })?;
    let probe = parent.join(format!(".ump-dash-write-test-{}", unique_suffix()));
    match OpenOptions::new().create_new(true).write(true).open(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            Ok(())
        }
        Err(error) => bail!(
            "Install directory is not writable for executable {}.\nMove or reinstall ump-dash manually.\n{}",
            exe_path.display(),
            error
        ),
    }
}

fn verify_archive_checksum(archive_path: &Path, sums_path: &Path, asset_name: &str) -> Result<()> {
    let expected = checksum_for_asset(sums_path, asset_name)?;
    let actual = sha256_hex(archive_path)?;
    if !actual.eq_ignore_ascii_case(&expected) {
        bail!(
            "Checksum mismatch for {}.\nExpected: {}\nActual:   {}",
            asset_name,
            expected,
            actual
        );
    }
    Ok(())
}

fn checksum_for_asset(sums_path: &Path, asset_name: &str) -> Result<String> {
    let file =
        File::open(sums_path).with_context(|| format!("failed to open {}", sums_path.display()))?;
    parse_sha256sums(BufReader::new(file), asset_name)
}

fn parse_sha256sums(reader: impl BufRead, asset_name: &str) -> Result<String> {
    let mut matches = Vec::new();
    for line in reader.lines() {
        let line = line.context("failed to read SHA256SUMS")?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(checksum) = parts.next() else {
            continue;
        };
        let Some(filename) = parts.next() else {
            continue;
        };
        if parts.next().is_some() {
            continue;
        }
        let filename = filename.strip_prefix('*').unwrap_or(filename);
        if filename == asset_name {
            matches.push(checksum.to_ascii_lowercase());
        }
    }

    match matches.len() {
        0 => bail!("SHA256SUMS does not contain a checksum row for {asset_name}."),
        1 => {
            let checksum = matches.remove(0);
            if checksum.len() != 64 || !checksum.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                bail!("SHA256SUMS checksum for {asset_name} is not a valid SHA-256 hex digest.");
            }
            Ok(checksum)
        }
        _ => bail!("SHA256SUMS contains duplicate checksum rows for {asset_name}."),
    }
}

fn sha256_hex(path: &Path) -> Result<String> {
    let mut file =
        File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn extract_binary_from_archive(archive_path: &Path, dest_path: &Path) -> Result<()> {
    let archive_file = File::open(archive_path)
        .with_context(|| format!("failed to open {}", archive_path.display()))?;
    let decoder = GzDecoder::new(archive_file);
    let mut archive = Archive::new(decoder);
    let mut extracted = false;

    for entry_result in archive.entries().context("failed to read tar archive")? {
        let mut entry = entry_result.context("failed to read tar archive entry")?;
        let path = entry
            .path()
            .context("failed to read tar archive entry path")?
            .into_owned();
        validate_archive_path(&path)?;
        if !entry.header().entry_type().is_file() {
            bail!(
                "Archive contains unsupported entry type for {}.",
                path.display()
            );
        }
        if path != Path::new(EXPECTED_ARCHIVE_BINARY) {
            bail!(
                "Archive contains unexpected file {}. Expected only {}.",
                path.display(),
                EXPECTED_ARCHIVE_BINARY
            );
        }
        if extracted {
            bail!("Archive contains duplicate {EXPECTED_ARCHIVE_BINARY} entries.");
        }

        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(dest_path)
            .with_context(|| format!("failed to create {}", dest_path.display()))?;
        let bytes = io::copy(&mut entry, &mut output).context("failed to extract update binary")?;
        output.flush().context("failed to flush update binary")?;
        if bytes == 0 {
            bail!("Extracted update binary is empty.");
        }
        extracted = true;
    }

    if !extracted {
        bail!("Archive contains no valid {EXPECTED_ARCHIVE_BINARY} binary entry.");
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<()> {
    let mut components = path.components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(name)), None) if name == OsStr::new(EXPECTED_ARCHIVE_BINARY) => {
            Ok(())
        }
        _ => bail!(
            "Archive contains unsafe or unexpected path {}. Expected only {}.",
            path.display(),
            EXPECTED_ARCHIVE_BINARY
        ),
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to stat {}", path.display()))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to set executable permissions on {}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn replacement_paths_for(exe_path: &Path, suffix: String) -> ReplacementPaths {
    let parent = exe_path.parent().unwrap_or_else(|| Path::new("."));
    let filename = exe_path
        .file_name()
        .unwrap_or_else(|| OsStr::new("ump-dash"));
    let filename = filename.to_string_lossy();
    ReplacementPaths {
        temp_binary: parent.join(format!(".{filename}.update-{suffix}")),
        backup: parent.join(format!(".{filename}.backup-{suffix}")),
    }
}

fn replace_current_executable(exe_path: &Path, paths: &ReplacementPaths) -> Result<()> {
    fs::rename(exe_path, &paths.backup).with_context(|| {
        format!(
            "failed to move current executable {} to backup {}",
            exe_path.display(),
            paths.backup.display()
        )
    })?;

    match fs::rename(&paths.temp_binary, exe_path) {
        Ok(()) => {
            if let Err(error) = fs::remove_file(&paths.backup) {
                eprintln!(
                    "Updated, but could not remove backup {}: {}",
                    paths.backup.display(),
                    error
                );
            }
            Ok(())
        }
        Err(install_error) => match fs::rename(&paths.backup, exe_path) {
            Ok(()) => bail!(
                "Replacement failed after backup was created, and the previous executable was restored.\nOriginal: {}\nBackup:   {}\nError:    {}",
                exe_path.display(),
                paths.backup.display(),
                install_error
            ),
            Err(restore_error) => bail!(
                "Replacement failed after backup was created, and automatic restore also failed.\nMove the backup back manually:\n  mv {} {}\nInstall error: {}\nRestore error: {}",
                paths.backup.display(),
                exe_path.display(),
                install_error,
                restore_error
            ),
        },
    }
}

fn sorted_file_names(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))? {
        let entry = entry?;
        names.push(entry.file_name().to_string_lossy().into_owned());
    }
    names.sort();
    Ok(names)
}

fn command_stderr(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr).trim().to_string();
    if text.is_empty() {
        "no stderr output".to_string()
    } else {
        text
    }
}

fn unique_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}", std::process::id())
}

impl TempDir {
    fn create_in(parent: &Path) -> Result<Self> {
        for _ in 0..10 {
            let path = parent.join(format!(".ump-dash-update-{}", unique_suffix()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "failed to create temporary update directory in {}",
                            parent.display()
                        )
                    });
                }
            }
        }
        bail!(
            "failed to create a unique temporary update directory in {}",
            parent.display()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use std::io::Cursor;
    use tar::{Builder, EntryType, Header};

    const ASSET: &str = "ump-dash-aarch64-apple-darwin.tar.gz";

    #[test]
    fn platform_asset_mapping_supports_known_targets() {
        assert_eq!(
            platform_asset_name("macos", "aarch64"),
            Some("ump-dash-aarch64-apple-darwin.tar.gz")
        );
        assert_eq!(
            platform_asset_name("macos", "x86_64"),
            Some("ump-dash-x86_64-apple-darwin.tar.gz")
        );
        assert_eq!(
            platform_asset_name("linux", "x86_64"),
            Some("ump-dash-x86_64-unknown-linux-gnu.tar.gz")
        );
        assert_eq!(platform_asset_name("linux", "aarch64"), None);
        assert_eq!(platform_asset_name("windows", "x86_64"), None);
    }

    #[test]
    fn parses_v_prefixed_semver_tags() {
        assert_eq!(
            parse_version("v1.2.3").unwrap(),
            Version::parse("1.2.3").unwrap()
        );
        assert_eq!(
            parse_version("1.2.3").unwrap(),
            Version::parse("1.2.3").unwrap()
        );
        assert!(parse_version("release-1.2.3").is_err());
    }

    #[test]
    fn selects_newer_releases_oldest_to_newest_and_filters_drafts_prereleases() {
        let releases = vec![
            summary("v1.5.0", false, false),
            summary("v1.4.0", false, false),
            summary("v1.6.0", true, false),
            summary("v1.7.0", false, true),
            summary("not-semver", false, false),
            summary("v1.3.0", false, false),
        ];
        let installed = Version::parse("1.3.0").unwrap();
        let selected = select_newer_releases(&releases, &installed);
        let versions: Vec<_> = selected
            .iter()
            .map(|release| release.version.to_string())
            .collect();
        assert_eq!(versions, vec!["1.4.0", "1.5.0"]);
    }

    #[test]
    fn detects_source_checkout_build_paths() {
        assert!(is_source_checkout_build_path(Path::new(
            "/repo/target/debug/ump-dash"
        )));
        assert!(is_source_checkout_build_path(Path::new(
            "/repo/target/release/renamed"
        )));
        assert!(is_source_checkout_build_path(Path::new(
            "/repo/target/aarch64-apple-darwin/release/ump-dash"
        )));
        assert!(!is_source_checkout_build_path(Path::new(
            "/usr/local/bin/ump-dash"
        )));
    }

    #[test]
    fn parses_sha256sums_exactly_one_row() {
        let sums = format!(
            "{}  {}\n{}  other.tar.gz\n",
            "a".repeat(64),
            ASSET,
            "b".repeat(64)
        );
        assert_eq!(
            parse_sha256sums(Cursor::new(sums), ASSET).unwrap(),
            "a".repeat(64)
        );
    }

    #[test]
    fn rejects_missing_duplicate_and_invalid_checksum_rows() {
        let missing = format!("{}  other.tar.gz\n", "a".repeat(64));
        assert!(parse_sha256sums(Cursor::new(missing), ASSET).is_err());

        let duplicate = format!(
            "{}  {}\n{}  {}\n",
            "a".repeat(64),
            ASSET,
            "b".repeat(64),
            ASSET
        );
        assert!(parse_sha256sums(Cursor::new(duplicate), ASSET).is_err());

        let invalid = format!("not-a-sha  {}\n", ASSET);
        assert!(parse_sha256sums(Cursor::new(invalid), ASSET).is_err());
    }

    #[test]
    fn checksum_verification_accepts_matching_hash_and_rejects_mismatch() {
        let dir = test_dir("checksum");
        fs::create_dir_all(&dir).unwrap();
        let archive = dir.join(ASSET);
        fs::write(&archive, b"archive bytes").unwrap();
        let digest = sha256_hex(&archive).unwrap();
        let sums = dir.join(SHA256SUMS_ASSET);
        fs::write(&sums, format!("{digest}  {ASSET}\n")).unwrap();
        verify_archive_checksum(&archive, &sums, ASSET).unwrap();

        fs::write(&sums, format!("{}  {ASSET}\n", "0".repeat(64))).unwrap();
        assert!(verify_archive_checksum(&archive, &sums, ASSET).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_validation_extracts_single_binary() {
        let dir = test_dir("valid-archive");
        fs::create_dir_all(&dir).unwrap();
        let archive = dir.join("valid.tar.gz");
        write_archive(&archive, &[(EXPECTED_ARCHIVE_BINARY, b"binary".as_slice())]).unwrap();
        let dest = dir.join("new-binary");

        extract_binary_from_archive(&archive, &dest).unwrap();
        assert_eq!(fs::read(&dest).unwrap(), b"binary");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn archive_validation_rejects_path_traversal_and_unexpected_entries() {
        let dir = test_dir("invalid-archive");
        fs::create_dir_all(&dir).unwrap();

        let traversal = dir.join("traversal.tar.gz");
        write_archive(&traversal, &[("../ump-dash", b"binary".as_slice())]).unwrap();
        assert!(extract_binary_from_archive(&traversal, &dir.join("out1")).is_err());

        let unexpected = dir.join("unexpected.tar.gz");
        write_archive(&unexpected, &[("other", b"binary".as_slice())]).unwrap();
        assert!(extract_binary_from_archive(&unexpected, &dir.join("out2")).is_err());

        let empty = dir.join("empty.tar.gz");
        write_archive(&empty, &[(EXPECTED_ARCHIVE_BINARY, b"".as_slice())]).unwrap();
        assert!(extract_binary_from_archive(&empty, &dir.join("out3")).is_err());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn replacement_paths_preserve_local_filename_and_parent() {
        let paths = replacement_paths_for(Path::new("/opt/tools/renamed-dash"), "abc".to_string());
        assert_eq!(
            paths.temp_binary,
            PathBuf::from("/opt/tools/.renamed-dash.update-abc")
        );
        assert_eq!(
            paths.backup,
            PathBuf::from("/opt/tools/.renamed-dash.backup-abc")
        );
    }

    fn summary(tag_name: &str, is_draft: bool, is_prerelease: bool) -> GhReleaseSummary {
        GhReleaseSummary {
            tag_name: tag_name.to_string(),
            is_draft,
            is_prerelease,
        }
    }

    fn test_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ump-dash-self-update-{name}-{}", unique_suffix()))
    }

    fn write_archive(path: &Path, entries: &[(&str, &[u8])]) -> Result<()> {
        let file = File::create(path)?;
        let encoder = GzEncoder::new(file, Compression::default());
        let mut builder = Builder::new(encoder);
        for (name, bytes) in entries {
            let mut header = Header::new_gnu();
            header.as_mut_bytes()[..name.len()].copy_from_slice(name.as_bytes());
            header.set_entry_type(EntryType::Regular);
            header.set_mode(0o755);
            header.set_size(bytes.len() as u64);
            header.set_cksum();
            builder.append(&header, *bytes)?;
        }
        builder.finish()?;
        Ok(())
    }
}
