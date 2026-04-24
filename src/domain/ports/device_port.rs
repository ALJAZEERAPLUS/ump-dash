//! Device enumeration port (F-105).
//!
//! Wraps `list_android_devices` + `list_ios_simulators` from
//! `src/infra/devices.rs` behind a single `list(kind)` method dispatched by
//! `DeviceKind`. `AdbXcrunDevices` in `src/infra/devices.rs` is the production
//! impl.
//!
//! `DeviceKind` is the canonical enum for device families; Plan 13-03 added a
//! stub of the same name to `src/app/effect.rs` with a TODO to relocate — this
//! plan replaces that stub with an import of the canonical type here.

#![allow(dead_code)]

use crate::domain::command::DeviceInfo;

/// Device family selector for `DevicePort::list`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceKind {
    Android,
    Ios,
}

/// Trait boundary for device enumeration (adb + xcrun simctl).
#[async_trait::async_trait]
pub trait DevicePort: Send + Sync {
    /// List available devices for the given family. Running devices and
    /// emulator AVDs are merged for Android; available simulators are
    /// returned for iOS.
    async fn list(&self, kind: DeviceKind) -> anyhow::Result<Vec<DeviceInfo>>;
}
