//! ExternalCommandPort — domain-owned shell command launcher boundary.

#![allow(dead_code)]

/// Runs a shell command outside the dashboard task system.
pub trait ExternalCommandPort: Send + Sync + std::fmt::Debug {
    fn run_shell_command(&self, command: &str) -> anyhow::Result<()>;
}
