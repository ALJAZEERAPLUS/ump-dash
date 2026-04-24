// src/infra/command_runner.rs
//
// Tokio-backed adapter implementing `CommandRunnerPort` (F-101).
//
// Pre-Phase-13 state: this file imported `crate::domain::action::Action` and
// sent `Action::CommandOutputLine` / `Action::CommandExited` directly on its
// output channel — the F-101 Fowler violation (Data-Source layer coupled to
// Service-layer messaging grammar).
//
// Post-Plan-13-05 state: the adapter emits the typed domain event
// `CommandEvent { OutputLine(String), Exited(ExitStatus) }` and knows nothing
// about `Action`. The `CommandEvent → Action` translation lives in the app
// layer (currently inline in `src/app.rs::dispatch_command`; Plan 13-08 will
// centralize it in `effect_runner`).
//
// Behavior preserved:
// - argv construction via `build_argv` (incl. `GitResetHard` → `origin/{branch}`)
// - `kill_on_drop(true)` on the spawned `tokio::process::Command`
// - concurrent stdout + stderr line streaming (tokio::select! loop)

#![allow(dead_code)]

use crate::domain::command::CommandSpec;
use crate::domain::ports::command_runner_port::{CommandEvent, CommandRunnerPort};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::sync::mpsc::{self, UnboundedSender};

/// Production adapter: spawns commands via `tokio::process::Command`.
///
/// Zero-sized type — no configuration required. Instantiate with
/// `TokioCommandRunner` (unit struct) and call `spawn` per command.
pub struct TokioCommandRunner;

impl CommandRunnerPort for TokioCommandRunner {
    fn spawn(
        &self,
        spec: CommandSpec,
        cwd: PathBuf,
        branch: String,
    ) -> mpsc::UnboundedReceiver<CommandEvent> {
        let (tx, rx) = mpsc::unbounded_channel::<CommandEvent>();
        tokio::spawn(async move {
            run_command(spec, cwd, branch, tx).await;
        });
        rx
    }
}

/// Inner body: builds argv, spawns the child, streams lines, waits for exit,
/// emits exactly one `CommandEvent::Exited(status)` on completion (including
/// the spawn-failure path — a synthetic failure status keeps downstream
/// consumers draining the receiver to completion).
async fn run_command(
    spec: CommandSpec,
    worktree_path: PathBuf,
    current_branch: String,
    tx: UnboundedSender<CommandEvent>,
) {
    let argv = build_argv(&spec, &current_branch);
    // argv is guaranteed non-empty by build_argv (CommandSpec variants always produce ≥1 element)
    let (program, args) = match argv.split_first() {
        Some((p, a)) => (p.clone(), a.to_vec()),
        None => {
            let _ = tx.send(CommandEvent::OutputLine("[error] empty argv".into()));
            let _ = tx.send(CommandEvent::Exited(synthetic_failure_status()));
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
            let _ = tx.send(CommandEvent::OutputLine(format!(
                "[error] failed to spawn: {e}"
            )));
            let _ = tx.send(CommandEvent::Exited(synthetic_failure_status()));
            return;
        }
    };

    // Take IO handles immediately before any wait/kill call.
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    // Stream stdout and stderr concurrently, then wait for process exit.
    stream_command_output(stdout, stderr, tx.clone()).await;
    let status = match child.wait().await {
        Ok(s) => s,
        Err(_) => synthetic_failure_status(),
    };
    let _ = tx.send(CommandEvent::Exited(status));
}

/// Reads stdout and stderr lines concurrently and sends each as
/// `CommandEvent::OutputLine`.
///
/// Returns when both streams are closed (process exited or task aborted).
/// Uses the same `tokio::select!` pattern as `stream_metro_logs` in `app.rs`.
async fn stream_command_output(
    stdout: tokio::process::ChildStdout,
    stderr: tokio::process::ChildStderr,
    tx: UnboundedSender<CommandEvent>,
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
                    Ok(Some(l)) => { let _ = tx.send(CommandEvent::OutputLine(l)); }
                    _ => { stdout_done = true; }
                }
            }
            line = stderr_lines.next_line(), if !stderr_done => {
                match line {
                    Ok(Some(l)) => { let _ = tx.send(CommandEvent::OutputLine(l)); }
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

/// Synthetic `ExitStatus` for failure paths (empty argv, spawn error, wait
/// error). Unix-only — matches the rest of the codebase which targets macOS
/// (primary) + Linux (CI).
#[cfg(unix)]
fn synthetic_failure_status() -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    // 1 << 8 ≈ "exited with code 1" on Unix (the shell convention for
    // generic failure). Consumers that only care about "did it exit?" will
    // treat this identically to a real non-zero exit.
    std::process::ExitStatus::from_raw(1 << 8)
}

#[cfg(not(unix))]
fn synthetic_failure_status() -> std::process::ExitStatus {
    // Non-Unix fallback — Default on ExitStatus is an impl detail across
    // std versions; use ExitStatusExt when available on the target.
    // This codebase does not ship on Windows; this path keeps the trait
    // implementation compilable under `cargo check --target x86_64-pc-windows-gnu`.
    std::process::ExitStatus::default()
}
