//! Metro lifecycle port — F-203 trait half + F-004 opaque handle (Plan 13-03).
//!
//! The `MetroHandle` trait replaces the concrete struct previously at
//! `src/domain/metro.rs:54-76` that leaked tokio types (`UnboundedSender`,
//! `JoinHandle`, `oneshot::Sender`) through its `pub` fields. With the trait
//! in place, the domain layer sees an opaque handle; tokio types remain on
//! the infra side only.
//!
//! Plan 13-07 implements `TokioMetroAdapter` in `src/infra/metro.rs` as the
//! sole production `MetroPort` impl. Until then, `src/app.rs` contains a
//! temporary `InAppMetroHandle` bridge (Plan 13-03 also introduces this) so
//! the existing `spawn_metro_task` keeps compiling.
//!
//! Decision (13-RESEARCH.md Pitfall 8 / Open Question Q3): `on_activity` is
//! a `Box<dyn Fn(MetroActivity)>` callback — hexagonally pure, no tokio leak
//! in the trait signature. The adapter converts internally to whatever
//! channel plumbing it uses.
//!
//! Decision (13-PATTERNS.md §Known gap): `detect_external` does NOT live on
//! `MetroPort`. External-port probing belongs to `PortProbePort` (introduced
//! in Plan 13-04) per audit F-102. The trait here stays minimal: `start` +
//! `http_post`.

#![allow(dead_code)]

use crate::domain::metro::MetroActivity;
use std::path::PathBuf;

/// Opaque handle to a live metro process. Implementations live infra-side.
///
/// The trait methods are the only surface the domain + app layers see; the
/// concrete type (tokio channels, task handles, kill-oneshot, etc.) is
/// private to the adapter.
pub trait MetroHandle: Send + Sync + std::fmt::Debug {
    /// OS process ID of the metro child — used for status display and
    /// external-kill bookkeeping.
    fn pid(&self) -> u32;

    /// Worktree identifier this instance was started from — displayed in
    /// the metro pane.
    fn worktree_id(&self) -> &str;

    /// TCP port selected for this metro instance.
    fn port(&self) -> u16;

    /// Send a raw byte sequence to metro's stdin (e.g. `r\n` for reload).
    ///
    /// No-op is acceptable if the underlying channel has closed — callers
    /// trigger stdin sends opportunistically and do not rely on delivery.
    fn send_stdin(&self, bytes: Vec<u8>) -> anyhow::Result<()>;

    /// Consuming kill. The adapter is responsible for process-group SIGKILL
    /// (metro spawns `yarn` which spawns `node` — both must die), aborting
    /// its internal tokio tasks, and waiting for the selected port to free.
    ///
    /// Takes `Box<Self>` so the trait stays object-safe — callers invoke
    /// `handle.kill()` on a `Box<dyn MetroHandle>` from
    /// `MetroManager::take_handle()`.
    fn kill(self: Box<Self>) -> anyhow::Result<()>;
}

/// Port trait for starting + controlling Metro instances. Plan 13-07's
/// `TokioMetroAdapter` in `src/infra/metro.rs` is the only concrete impl.
#[async_trait::async_trait]
pub trait MetroPort: Send + Sync {
    /// Spawn metro in the given worktree. `on_activity` is invoked for every
    /// `MetroActivity` parsed from stdout/stderr — the adapter converts
    /// channel plumbing internally.
    ///
    /// Returns an opaque handle; caller registers it via
    /// `MetroManager::register`.
    async fn start(
        &self,
        worktree: PathBuf,
        on_activity: Box<dyn Fn(MetroActivity) + Send + Sync>,
    ) -> anyhow::Result<Box<dyn MetroHandle>>;

    /// Fire-and-forget HTTP POST to metro's control endpoint
    /// (e.g. `/reload`, `/open-debugger`).
    async fn http_post(&self, path: &str, body: &str) -> anyhow::Result<()>;
}
