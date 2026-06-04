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

/// Inner body: builds argv, spawns the child, streams lines while waiting for exit,
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
        // CRITICAL: process_group(0) makes the spawned child its own process-group
        // leader (PID == PGID). Phase 15 SIGTERM-to-PGID then reaches grandchildren
        // (yarn-spawned node workers, gradle-spawned java, xcodebuild-spawned clang).
        // Without this, libc::kill(-pid, SIGTERM) targets a non-existent group (ESRCH)
        // and only the immediate child dies. 15-RESEARCH §F1 / §Pitfall 2.
        .process_group(0)
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

    // Phase 15 / Plan 15-01 Task 3: emit child PID as the FIRST event so
    // effect_runner can construct TokioTaskHandle { child_pid, .. } before
    // any OutputLine arrives. Spec: command_runner_port::CommandEvent doc.
    let child_pid = child
        .id()
        .expect("child pid available after successful spawn");
    let _ = tx.send(CommandEvent::ProcessStarted { pid: child_pid });

    // Take IO handles immediately before any wait/kill call.
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    // Stream stdout/stderr while waiting for process exit. Some commands
    // launch background descendants that inherit these pipes; waiting for EOF
    // before `child.wait()` would keep the dashboard task alive after the
    // launcher has already exited.
    let mut stream_task = tokio::spawn(stream_command_output(stdout, stderr, tx.clone()));
    let status = match child.wait().await {
        Ok(s) => s,
        Err(_) => synthetic_failure_status(),
    };

    match tokio::time::timeout(std::time::Duration::from_millis(250), &mut stream_task).await {
        Ok(Ok(())) => {}
        Ok(Err(_join_error)) => {}
        Err(_elapsed) => {
            stream_task.abort();
            let _ = stream_task.await;
        }
    }

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

#[cfg(test)]
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod tests {
    use super::*;

    /// Plan 15-01 Task 3 — locks the "ProcessStarted is the FIRST event"
    /// contract from `CommandEvent` doc string. Spawns `echo done` through
    /// the real `TokioCommandRunner` and asserts the event ordering
    /// `[ProcessStarted { pid > 1 }, OutputLine(_)+, Exited(_)]`.
    #[tokio::test(flavor = "multi_thread")]
    async fn run_command_emits_process_started_first() {
        let runner = TokioCommandRunner;
        let mut rx = runner.spawn(
            CommandSpec::ShellCommand {
                command: "echo done".into(),
            },
            std::env::temp_dir(),
            "main".into(),
        );

        // FIRST event must be ProcessStarted with a real OS pid.
        let first = rx.recv().await.expect("at least one event");
        match first {
            CommandEvent::ProcessStarted { pid } => {
                assert!(pid > 1, "expected real pid > 1, got {pid}");
            }
            other => panic!("expected ProcessStarted first, got {other:?}"),
        }

        // Drain the rest; require at least one OutputLine and exactly one Exited.
        let mut output_lines = 0usize;
        let mut exited = 0usize;
        while let Some(ev) = rx.recv().await {
            match ev {
                CommandEvent::ProcessStarted { .. } => {
                    panic!("ProcessStarted must be emitted exactly once");
                }
                CommandEvent::OutputLine(_) => output_lines += 1,
                CommandEvent::Exited(_) => exited += 1,
            }
        }
        assert!(output_lines >= 1, "expected at least one OutputLine");
        assert_eq!(exited, 1, "expected exactly one Exited event");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn run_command_exits_when_background_child_keeps_pipes_open() {
        let runner = TokioCommandRunner;
        let mut rx = runner.spawn(
            CommandSpec::ShellCommand {
                command: "sleep 5 & echo launched".into(),
            },
            std::env::temp_dir(),
            "main".into(),
        );

        let pid = match rx.recv().await.expect("at least one event") {
            CommandEvent::ProcessStarted { pid } => pid,
            other => panic!("expected ProcessStarted first, got {other:?}"),
        };

        let mut saw_output = false;
        let exited = tokio::time::timeout(std::time::Duration::from_millis(750), async {
            loop {
                match rx
                    .recv()
                    .await
                    .expect("channel should stay open until exit")
                {
                    CommandEvent::ProcessStarted { .. } => {
                        panic!("ProcessStarted must be emitted exactly once");
                    }
                    CommandEvent::OutputLine(line) => {
                        if line == "launched" {
                            saw_output = true;
                        }
                    }
                    CommandEvent::Exited(status) => return status,
                }
            }
        })
        .await;

        unsafe {
            let _ = libc::kill(-(pid as i32), libc::SIGKILL);
        }

        let status = exited.expect(
            "runner should emit Exited after the shell exits, even if a background child keeps stdout/stderr open",
        );
        assert!(status.success(), "expected shell success, got {status:?}");
        assert!(saw_output, "expected to stream command output before exit");
    }
}
