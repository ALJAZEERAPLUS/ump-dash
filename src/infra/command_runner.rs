// src/infra/command_runner.rs
//
// Generic command runner: spawns any CommandSpec as a child process and
// streams stdout/stderr lines back to the event loop via an unbounded mpsc
// channel as Action::CommandOutputLine values.
//
// The single public entry point is `spawn_command_task`, which returns a
// JoinHandle so the caller can abort the task if CommandCancel is dispatched.

#![allow(dead_code)]

use crate::domain::action::Action;
use crate::domain::command::CommandSpec;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::mpsc::UnboundedSender;

/// Spawns the given CommandSpec as a background task.
///
/// - Builds the argv via `build_argv(&spec, &current_branch)`
/// - Spawns via `tokio::process::Command` in `worktree_path`
/// - Streams stdout + stderr lines as `Action::CommandOutputLine`
/// - Sends `Action::CommandExited` when the process finishes (or fails to spawn)
///
/// Returns a `JoinHandle` so the caller can `.abort()` it on `CommandCancel`.
pub async fn spawn_command_task(
    spec: CommandSpec,
    worktree_path: PathBuf,
    current_branch: String,
    action_tx: UnboundedSender<Action>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let argv = build_argv(&spec, &current_branch);
        // argv is guaranteed non-empty by build_argv (CommandSpec variants always produce ≥1 element)
        let (program, args) = match argv.split_first() {
            Some((p, a)) => (p.clone(), a.to_vec()),
            None => {
                let _ = action_tx.send(Action::CommandOutputLine("[error] empty argv".into()));
                let _ = action_tx.send(Action::CommandExited);
                return;
            }
        };

        let mut child = match tokio::process::Command::new(&program)
            .args(&args)
            .current_dir(&worktree_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = action_tx
                    .send(Action::CommandOutputLine(format!("[error] failed to spawn: {e}")));
                let _ = action_tx.send(Action::CommandExited);
                return;
            }
        };

        // Take IO handles immediately before any wait/kill call.
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        // Stream stdout and stderr concurrently, then wait for process exit.
        stream_command_output(stdout, stderr, action_tx.clone()).await;
        let _ = child.wait().await;
        let _ = action_tx.send(Action::CommandExited);
    })
}

/// Reads stdout and stderr lines concurrently and sends each as `Action::CommandOutputLine`.
///
/// Returns when both streams are closed (process exited or task aborted).
/// Uses the same `tokio::select!` pattern as `stream_metro_logs` in `app.rs`.
async fn stream_command_output(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    tx: UnboundedSender<Action>,
) {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();

    // Track whether each stream has closed
    let mut stdout_done = false;
    let mut stderr_done = false;

    loop {
        if stdout_done && stderr_done {
            break;
        }

        tokio::select! {
            line = stdout_lines.next_line(), if !stdout_done => {
                match line {
                    Ok(Some(l)) => { let _ = tx.send(Action::CommandOutputLine(l)); }
                    _ => { stdout_done = true; }
                }
            }
            line = stderr_lines.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(l)) => { let _ = tx.send(Action::CommandOutputLine(l)); }
                    _ => { stderr_done = true; }
                }
            }
        }
    }
}

/// Builds the final argv for a command, injecting runtime context where needed.
///
/// For `GitResetHard`, overrides the target to `origin/{current_branch}` (hard-reset to
/// the remote tracking branch, not just HEAD). All other variants delegate to
/// `CommandSpec::to_argv()`.
fn build_argv(spec: &CommandSpec, current_branch: &str) -> Vec<String> {
    match spec {
        CommandSpec::GitResetHard => {
            vec![
                "git".into(),
                "reset".into(),
                "--hard".into(),
                format!("origin/{current_branch}"),
            ]
        }
        other => other.to_argv(),
    }
}
