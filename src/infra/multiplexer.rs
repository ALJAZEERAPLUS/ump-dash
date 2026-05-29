//! Terminal multiplexer adapters — concrete `TmuxAdapter`, `ZellijAdapter`,
//! and `GhosttyAdapter`
//! implementing `crate::domain::ports::multiplexer_port::MultiplexerPort`.
//!
//! Uses `std::process::Command` (same pattern as existing `tmux.rs`).
//! No new crates required.

use crate::domain::ports::multiplexer_port::MultiplexerPort;
use std::path::Path;
use std::process::Command;

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

#[derive(Debug)]
pub struct GhosttyAdapter;

impl MultiplexerPort for GhosttyAdapter {
    fn new_window(&self, path: &Path, name: &str, command: &str) -> anyhow::Result<()> {
        let status = ghostty_new_window_command(path, name, command).status()?;
        if !status.success() {
            anyhow::bail!("ghostty new window failed: exit code {:?}", status.code());
        }
        Ok(())
    }

    fn is_available(&self) -> bool {
        std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
    }
}

fn ghostty_new_window_command(path: &Path, name: &str, command: &str) -> Command {
    let (program, args) = ghostty_new_window_command_parts(path, name, command);
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd
}

#[cfg(target_os = "macos")]
fn ghostty_new_window_command_parts(
    path: &Path,
    name: &str,
    command: &str,
) -> (&'static str, Vec<String>) {
    let path_str = path.to_str().unwrap_or(".");
    (
        "osascript",
        vec![
            "-e".into(),
            "on run argv".into(),
            "-e".into(),
            "set projectDir to item 1 of argv".into(),
            "-e".into(),
            "set tabName to item 2 of argv".into(),
            "-e".into(),
            "set launchCommand to item 3 of argv".into(),
            "-e".into(),
            "tell application \"Ghostty\"".into(),
            "-e".into(),
            "activate".into(),
            "-e".into(),
            "set cfg to new surface configuration".into(),
            "-e".into(),
            "set initial working directory of cfg to projectDir".into(),
            "-e".into(),
            "set initial input of cfg to launchCommand & linefeed".into(),
            "-e".into(),
            "set win to new window with configuration cfg".into(),
            "-e".into(),
            "try".into(),
            "-e".into(),
            "set name of selected tab of win to tabName".into(),
            "-e".into(),
            "end try".into(),
            "-e".into(),
            "end tell".into(),
            "-e".into(),
            "end run".into(),
            path_str.into(),
            name.into(),
            command.into(),
        ],
    )
}

#[cfg(not(target_os = "macos"))]
fn ghostty_new_window_command_parts(
    path: &Path,
    name: &str,
    command: &str,
) -> (&'static str, Vec<String>) {
    let path_str = path.to_str().unwrap_or(".");
    (
        "ghostty",
        vec![
            "+new-window".into(),
            "--working-directory".into(),
            path_str.into(),
            "--title".into(),
            name.into(),
            "-e".into(),
            "sh".into(),
            "-lc".into(),
            command.into(),
        ],
    )
}

/// Auto-detect the available multiplexer. Checks $TMUX first, then $ZELLIJ,
/// then Ghostty. Returns None if no supported terminal surface is detected —
/// features that need it are disabled.
pub fn detect_multiplexer() -> Option<Box<dyn MultiplexerPort>> {
    if std::env::var("TMUX").is_ok() {
        return Some(Box::new(TmuxAdapter));
    }
    if std::env::var("ZELLIJ").is_ok() {
        return Some(Box::new(ZellijAdapter));
    }
    if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok() {
        return Some(Box::new(GhosttyAdapter));
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::sync::{Mutex, OnceLock};

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

    struct EnvRestore(Vec<(&'static str, Option<OsString>)>);

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                unsafe {
                    match value {
                        Some(value) => std::env::set_var(key, value),
                        None => std::env::remove_var(key),
                    }
                }
            }
        }
    }

    fn set_multiplexer_env(vars: &[(&'static str, &'static str)]) -> EnvRestore {
        let keys = ["TMUX", "ZELLIJ", "GHOSTTY_RESOURCES_DIR"];
        let saved = keys
            .into_iter()
            .map(|key| (key, std::env::var_os(key)))
            .collect();
        unsafe {
            for key in keys {
                std::env::remove_var(key);
            }
            for (key, value) in vars {
                std::env::set_var(key, value);
            }
        }
        EnvRestore(saved)
    }

    #[test]
    fn detects_ghostty_session_from_resources_dir() {
        let _guard = ENV_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
        let _restore = set_multiplexer_env(&[(
            "GHOSTTY_RESOURCES_DIR",
            "/Applications/Ghostty.app/Contents/Resources",
        )]);

        let mux = detect_multiplexer().expect("Ghostty sessions should be detected");

        assert_eq!(format!("{mux:?}"), "GhosttyAdapter");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn ghostty_macos_command_uses_applescript_with_context_args() {
        let (program, args) =
            ghostty_new_window_command_parts(Path::new("/repo/app"), "app-claude", "claude");

        assert_eq!(program, "osascript");
        assert!(args
            .iter()
            .any(|arg| arg.contains("new surface configuration")));
        assert!(args
            .iter()
            .any(|arg| arg.contains("new window with configuration cfg")));
        assert_eq!(
            &args[args.len() - 3..],
            ["/repo/app", "app-claude", "claude"]
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn ghostty_gtk_command_uses_new_window_with_cwd_title_and_shell_command() {
        let (program, args) =
            ghostty_new_window_command_parts(Path::new("/repo/app"), "app-claude", "claude");

        assert_eq!(program, "ghostty");
        assert_eq!(
            args,
            [
                "+new-window",
                "--working-directory",
                "/repo/app",
                "--title",
                "app-claude",
                "-e",
                "sh",
                "-lc",
                "claude"
            ]
        );
    }
}
