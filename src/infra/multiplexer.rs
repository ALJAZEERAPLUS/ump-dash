//! Terminal multiplexer adapters — concrete `TmuxAdapter` + `ZellijAdapter`
//! implementing `crate::domain::ports::multiplexer_port::MultiplexerPort`.
//!
//! Uses `std::process::Command` (same pattern as existing `tmux.rs`).
//! No new crates required.

use crate::domain::ports::multiplexer_port::MultiplexerPort;
use std::path::Path;

#[derive(Debug)]
pub struct TmuxAdapter;

impl MultiplexerPort for TmuxAdapter {
    fn new_window(&self, path: &Path, name: &str, command: &str) -> anyhow::Result<()> {
        let path_str = path.to_str().unwrap_or(".");
        let status = std::process::Command::new("tmux")
            .args(["new-window", "-c", path_str, "-n", name, command])
            .status()?;
        if !status.success() {
            anyhow::bail!("tmux new-window failed: exit code {:?}", status.code());
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        std::env::var("TMUX").is_ok()
    }
}

#[derive(Debug)]
pub struct ZellijAdapter;

impl MultiplexerPort for ZellijAdapter {
    fn new_window(&self, path: &Path, name: &str, command: &str) -> anyhow::Result<()> {
        // Zellij tab creation: create tab at CWD with name.
        // Zellij's new-tab does not support running an initial command directly
        // in the same way tmux does. We create the tab, then write the command.
        let path_str = path.to_str().unwrap_or(".");

        // Create tab at the given cwd
        let status = std::process::Command::new("zellij")
            .args(["action", "new-tab", "--name", name, "--cwd", path_str])
            .status()?;
        if !status.success() {
            anyhow::bail!("zellij new-tab failed: exit code {:?}", status.code());
        }

        // Write the command to the new tab's terminal
        // (zellij action write-chars sends keystrokes to the focused pane)
        let cmd_with_enter = format!("{command}\n");
        let write_status = std::process::Command::new("zellij")
            .args(["action", "write-chars", &cmd_with_enter])
            .status()?;
        if !write_status.success() {
            tracing::warn!("zellij write-chars failed — tab created but command not started");
        }

        Ok(())
    }

    fn is_available(&self) -> bool {
        std::env::var("ZELLIJ").is_ok()
    }
}

/// Auto-detect the available multiplexer. Checks $TMUX first, then $ZELLIJ.
/// Returns None if no multiplexer is detected — features that need it are disabled.
pub fn detect_multiplexer() -> Option<Box<dyn MultiplexerPort>> {
    if std::env::var("TMUX").is_ok() {
        return Some(Box::new(TmuxAdapter));
    }
    if std::env::var("ZELLIJ").is_ok() {
        return Some(Box::new(ZellijAdapter));
    }
    None
}

/// Returns `true` when the process is running inside a tmux session.
///
/// Tmux sets the `TMUX` environment variable to the path of the server socket,
/// so its presence is a reliable indicator of a tmux session.
///
/// Plan 13-10 (F-108): relocated from `infra::jira` — multiplexer concern, not
/// JIRA concern. Currently has no in-tree call sites; kept available behind
/// `#[allow(dead_code)]` for future detection logic that could route between
/// `TmuxAdapter::new_window` and a fall-back path.
#[allow(dead_code)]
pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}
