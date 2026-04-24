// src/infra/devices.rs
//
// Device enumeration for Android (adb) and iOS (xcrun simctl).
//
// Parsers are pure functions that take raw command output as &str and return
// Vec<DeviceInfo>. The async wrappers run the actual commands and delegate
// parsing to them — keeping the parsers unit-testable without real devices.

#![allow(dead_code)]

use crate::domain::command::DeviceInfo;

// ---------------------------------------------------------------------------
// Parsers (pure functions)
// ---------------------------------------------------------------------------

/// Parses `adb devices -l` output into a list of connected Android devices.
///
/// Sample output (`adb devices -l`):
/// ```text
/// List of devices attached
/// emulator-5552  device product:sdk_gphone64_arm64 model:sdk_gphone64_arm64 device:emu64a transport_id:3
/// R58MA1XR0XE    device product:a52sxq model:SM_A525F device:a52sxq transport_id:1
/// R58MB2YS1YF    offline
/// ```
///
/// Also handles the old `adb devices` format (no key:value pairs after state).
///
/// Only entries with state == "device" are included. "offline", "unauthorized",
/// "no permissions" entries are silently skipped.
/// The `model:` field (if present) is used as the display name with underscores replaced by spaces.
pub fn parse_adb_devices(output: &str) -> Vec<DeviceInfo> {
    output
        .lines()
        .skip(1) // Skip the "List of devices attached" header
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            // Fields are tab-separated: "<serial>\t<state>[extra]"
            // With -l flag, extra is space-separated key:value pairs like "model:sdk_gphone64_arm64"
            let mut parts = line.splitn(2, '\t');
            let serial = parts.next()?.trim();
            let rest = parts.next()?.trim();

            // Split on whitespace and take the first token as the state.
            let mut tokens = rest.split_whitespace();
            let state_token = tokens.next().unwrap_or("");

            if state_token != "device" {
                return None;
            }

            // Try to extract model name from key:value pairs (adb devices -l format)
            let model_name = tokens
                .find(|t| t.starts_with("model:"))
                .map(|t| {
                    // Strip "model:" prefix and replace underscores with spaces
                    t["model:".len()..].replace('_', " ")
                });

            let name = model_name.unwrap_or_else(|| serial.to_string());

            Some(DeviceInfo {
                id: serial.to_string(),
                name,
            })
        })
        .collect()
}

/// Parses `xcrun simctl list devices available --json` output into a list of iOS simulators.
///
/// The JSON structure is:
/// ```json
/// { "devices": { "com.apple.CoreSimulator.SimRuntime.iOS-17-0": [ { "udid": "...", "name": "...", "state": "Booted", "isAvailable": true }, ... ] } }
/// ```
///
/// Only simulators with `isAvailable == true` are returned.
/// Display name is formatted as "{name} ({state})" (e.g. "iPhone 15 Pro (Shutdown)").
pub fn parse_xcrun_simctl(json_output: &str) -> Vec<DeviceInfo> {
    let parsed: serde_json::Value = match serde_json::from_str(json_output) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let devices_map = match parsed.get("devices").and_then(|d| d.as_object()) {
        Some(m) => m,
        None => return Vec::new(),
    };

    let mut result = Vec::new();

    for runtime_devices in devices_map.values() {
        let device_list = match runtime_devices.as_array() {
            Some(arr) => arr,
            None => continue,
        };

        for device in device_list {
            // Only include available simulators
            let is_available = device
                .get("isAvailable")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_available {
                continue;
            }

            let udid = match device.get("udid").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => continue,
            };
            let name = device
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");
            let state = device
                .get("state")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown");

            result.push(DeviceInfo {
                id: udid.to_string(),
                name: format!("{name} ({state})"),
            });
        }
    }

    result
}

/// Parses `xcrun xctrace list devices` output into a list of physical iOS devices.
///
/// Sample output:
/// ```text
/// == Devices ==
/// My iPhone (17.0) (00008120-XXXXXXXXXXXX)
/// iPad Pro (16.6) (00008103-YYYYYYYYYYYY)
///
/// == Simulators ==
/// iPhone 15 Pro Simulator (17.0) (XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX)
/// ```
///
/// Only entries before the "== Simulators ==" header are returned (physical devices).
pub fn parse_xctrace_devices(output: &str) -> Vec<DeviceInfo> {
    let mut result = Vec::new();
    let mut in_devices_section = false;

    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("== Devices ==") {
            in_devices_section = true;
            continue;
        }
        if trimmed.starts_with("== Simulators ==") {
            break; // stop at simulators
        }
        if !in_devices_section || trimmed.is_empty() {
            continue;
        }
        // Skip the local Mac entry (no parenthesized UDID at end, or contains "this computer")
        // Format: "Name (OS version) (UDID)"
        // We need at least two parenthesized groups
        if let Some(udid_start) = trimmed.rfind('(') {
            let udid_end = trimmed.len().saturating_sub(1); // last char should be ')'
            if udid_end > udid_start && trimmed.ends_with(')') {
                let udid = &trimmed[udid_start + 1..udid_end];
                // UDIDs for physical devices are hex strings (no dashes in the short form)
                // or UUID format. Skip if it looks like a version number only.
                if udid.len() >= 16 {
                    let name = trimmed[..udid_start].trim().trim_end_matches(' ');
                    result.push(DeviceInfo {
                        id: udid.to_string(),
                        name: name.to_string(),
                    });
                }
            }
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Async runners
// ---------------------------------------------------------------------------

/// Parses `emulator -list-avds` output into available (not necessarily running) emulator names.
///
/// Output is one AVD name per line, e.g.:
/// ```text
/// Pixel_7a
/// Pixel_8a
/// ```
pub fn parse_avd_list(output: &str) -> Vec<DeviceInfo> {
    output
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|name| DeviceInfo {
            id: name.to_string(),
            name: name.replace('_', " "),
        })
        .collect()
}

/// Runs `adb devices -l` + `emulator -list-avds` and returns the merged list.
/// Running devices appear first; available (not running) emulators are appended
/// with an "(available)" suffix so the user can distinguish them in the picker.
pub async fn list_android_devices() -> anyhow::Result<Vec<DeviceInfo>> {
    let (adb_output, avd_output) = tokio::join!(
        tokio::process::Command::new("adb")
            .args(["devices", "-l"])
            .output(),
        tokio::process::Command::new("emulator")
            .arg("-list-avds")
            .output(),
    );

    let mut devices = {
        let text = String::from_utf8_lossy(&adb_output?.stdout).to_string();
        parse_adb_devices(&text)
    };

    // Merge available AVDs that aren't already running
    if let Ok(output) = avd_output {
        let text = String::from_utf8_lossy(&output.stdout).to_string();
        let avds = parse_avd_list(&text);
        let running_ids: std::collections::HashSet<String> =
            devices.iter().map(|d| d.id.clone()).collect();
        for avd in avds {
            if !running_ids.contains(&avd.id) {
                devices.push(DeviceInfo {
                    name: format!("{} (available)", avd.name),
                    ..avd
                });
            }
        }
    }

    Ok(devices)
}

/// Runs `xcrun simctl list devices available --json` and returns available iOS simulators.
pub async fn list_ios_simulators() -> anyhow::Result<Vec<DeviceInfo>> {
    let output = tokio::process::Command::new("xcrun")
        .args(["simctl", "list", "devices", "available", "--json"])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("xcrun simctl failed: {stderr}");
    }

    let text = String::from_utf8(output.stdout)?;
    Ok(parse_xcrun_simctl(&text))
}

/// Runs `xcrun xctrace list devices` and returns connected physical iOS devices.
/// Falls back to empty list if xctrace is not available.
pub async fn list_ios_physical_devices() -> anyhow::Result<Vec<DeviceInfo>> {
    let output = tokio::process::Command::new("xcrun")
        .args(["xctrace", "list", "devices"])
        .output()
        .await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("xcrun xctrace failed: {stderr}");
    }

    let text = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(parse_xctrace_devices(&text))
}

/// F-105 adapter: dispatches by `DeviceKind` to the existing free fns
/// (`list_android_devices` / `list_ios_simulators`).
///
/// Physical iOS devices (`list_ios_physical_devices`) are not yet part of the
/// trait surface — the current app only consumes the simulator list for the
/// iOS family. A future extension can add a `DeviceKind::IosPhysical` variant
/// and dispatch accordingly without breaking existing consumers.
pub struct AdbXcrunDevices;

#[async_trait::async_trait]
impl crate::domain::ports::device_port::DevicePort for AdbXcrunDevices {
    async fn list(
        &self,
        kind: crate::domain::ports::device_port::DeviceKind,
    ) -> anyhow::Result<Vec<DeviceInfo>> {
        match kind {
            crate::domain::ports::device_port::DeviceKind::Android => {
                list_android_devices().await
            }
            crate::domain::ports::device_port::DeviceKind::Ios => list_ios_simulators().await,
        }
    }
}
