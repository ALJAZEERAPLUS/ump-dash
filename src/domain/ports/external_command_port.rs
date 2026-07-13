//! ExternalCommandPort — domain-owned shell command launcher boundary.

#![allow(dead_code)]

use std::path::Path;

/// Runs a shell command outside the dashboard task system.
pub trait ExternalCommandPort: Send + Sync + std::fmt::Debug {
    fn run_shell_command(&self, command: &str) -> anyhow::Result<()>;
    fn open_in_finder(&self, path: &Path) -> anyhow::Result<()>;
}
