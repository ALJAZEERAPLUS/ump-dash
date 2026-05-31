//! Adapters — F-202 dependency-injection bundle (Plan 13-08).
//!
//! `Adapters` owns trait objects for every infra port the app layer needs.
//! `EffectRunner` receives an `Adapters` instance and dispatches every
//! `Effect` variant via `self.adapters.<port>.<method>()`. The hexagonal
//! direction is restored: `src/app/` depends only on
//! `crate::domain::ports::*` traits — concrete `infra::*` types are
//! constructed at the composition root (`src/main.rs`) and threaded in here.
//!
//! Two ports are `Option`-valued because their availability depends on
//! runtime context:
//! - `jira` is `None` when no `DashConfig` is loaded or no token is set.
//! - `multiplexer` is `None` when the process is not inside tmux, zellij, or Ghostty.
//!
//! `#[derive(Clone)]` is load-bearing: `EffectRunner::run_effect` clones
//! adapter handles into `tokio::spawn` closures.

#![allow(dead_code)]

use crate::domain::ports::command_runner_port::CommandRunnerPort;
use crate::domain::ports::device_port::DevicePort;
use crate::domain::ports::jira_port::JiraPort;
use crate::domain::ports::metro_port::MetroPort;
use crate::domain::ports::multiplexer_port::MultiplexerPort;
use crate::domain::ports::native_cache_port::NativeCachePort;
use crate::domain::ports::port_probe_port::PortProbePort;
use crate::domain::ports::worktree_port::WorktreePort;
use std::sync::Arc;

/// Dependency-injection bundle. Held by `EffectRunner`. Constructed in
/// `src/main.rs` (composition root). All concrete adapter types live in
/// `infra::*` and are injected as `Arc<dyn Port>` trait objects.
#[derive(Clone)]
pub struct Adapters {
    pub command_runner: Arc<dyn CommandRunnerPort>,
    pub metro: Arc<dyn MetroPort>,
    pub port_probe: Arc<dyn PortProbePort>,
    pub worktrees: Arc<dyn WorktreePort>,
    pub devices: Arc<dyn DevicePort>,
    pub native_cache: Arc<dyn NativeCachePort>,
    /// `None` when the dashboard config does not contain JIRA credentials.
    pub jira: Option<Arc<dyn JiraPort>>,
    /// `None` when the process is not running inside tmux, zellij, or Ghostty.
    pub multiplexer: Option<Arc<dyn MultiplexerPort>>,
}
