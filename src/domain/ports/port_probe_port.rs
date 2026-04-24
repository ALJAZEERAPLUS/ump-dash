//! Port probe — lsof-based external metro detection + port occupancy + kill (F-102).
//!
//! Domain-layer trait boundary for the three lsof/kill free functions currently
//! living in `src/infra/port.rs`. The adapter shell (`LsofPortProbe` in
//! `src/infra/port.rs`) wraps those free fns so consumers after Plan 13-08 can
//! depend only on the trait.
//!
//! `ExternalProcessInfo` is the domain-canonical name for the struct previously
//! called `ExternalMetroInfo` — the payload is process-generic (pid + working
//! dir), not metro-specific, so the domain type drops the "Metro" prefix.
//! `src/infra/port.rs` keeps the old name for backwards compat with existing
//! callers until Plan 13-08 routes everything through the port.

#![allow(dead_code)]

/// Information about an external (non-dashboard) process occupying a port.
///
/// Returned by `PortProbePort::detect_external`; carried by
/// `Action::ExternalMetroDetected` so the app layer never touches the
/// infra-side `ExternalMetroInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalProcessInfo {
    pub pid: u32,
    pub working_dir: String,
}

/// Trait boundary for port-availability probing + external process kill.
///
/// The domain + app layers depend only on this trait. `LsofPortProbe` in
/// `infra::port` is the production implementation; tests may supply a fake.
#[async_trait::async_trait]
pub trait PortProbePort: Send + Sync {
    /// Returns true if no process is currently bound to `port` on 127.0.0.1.
    fn port_is_free(&self, port: u16) -> bool;

    /// Detect if an external (non-dashboard) process is listening on the given
    /// port. Fast path returns `None` if the port is free without invoking
    /// lsof (per research pattern).
    async fn detect_external(&self, port: u16) -> Option<ExternalProcessInfo>;

    /// Kill a process by PID using SIGKILL.
    async fn kill_process(&self, pid: u32) -> anyhow::Result<()>;
}
