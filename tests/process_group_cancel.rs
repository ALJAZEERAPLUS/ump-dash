//! TASK-04 end-to-end characterization — proves that cancelling a task spawned
//! THROUGH `TokioCommandRunner` (Plan 15-01's `.process_group(0)` + Plan 15-02's
//! `TokioTaskHandle::abort()`) reaps the full process group within 2 seconds.
//!
//! Companion to but distinct from `tests/process_group_kill.rs` (COVER-02),
//! which spawns `tokio::process::Command` directly and was written before
//! Plan 15-01 added `.process_group(0)` to `command_runner.rs`. That test
//! locks the bare-OS PGID kill primitive; this test locks the full Phase 15
//! end-to-end path: `TokioCommandRunner::spawn` → `CommandEvent::ProcessStarted`
//! → `TokioTaskHandle { child_pid, .. }` → `abort()` (SIGTERM → 200ms grace →
//! SIGKILL ladder) → grandchildren reaped.
//!
//! GUARD ROLE: this test fails RED if `.process_group(0)` is removed from
//! `src/infra/command_runner.rs::run_command`. The grandchild sleep PID would
//! join the rn-dash process group, the PGID broadcast from `TokioTaskHandle::abort`
//! would fail to reach it, and the ESRCH polling loop would time out at 2s.
//! Also fails RED if `TokioTaskHandle::abort` no longer sends SIGTERM/SIGKILL
//! to the PGID (Plan 15-02 regression).

#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::time::Duration;

use rn_dash::domain::command::CommandSpec;
use rn_dash::domain::ports::command_runner_port::{CommandEvent, CommandRunnerPort};
use rn_dash::domain::ports::task_handle::TaskHandle;
use rn_dash::infra::command_runner::TokioCommandRunner;
use rn_dash::infra::task_handle::TokioTaskHandle;

use tokio::time::{sleep, timeout};

/// End-to-end: spawn `sleep 60` as a backgrounded grandchild through the
/// runner, abort the wrapping `TokioTaskHandle`, assert the process group is
/// empty within 2 seconds and the specific grandchild PID is dead.
#[tokio::test(flavor = "multi_thread")]
async fn cancel_via_task_handle_reaps_full_process_group() {
    // 1) Instantiate the production runner.
    let runner = TokioCommandRunner;

    // 2) Build a shell-command spec that backgrounds a long sleep and echoes
    //    the sleep's PID to stderr. `sh -c` (CommandSpec::ShellCommand expands
    //    to `sh -c <command>`) supports `&`, `$!`, and `wait` per POSIX, so
    //    this works on both macOS (/bin/sh -> bash) and Linux CI (/bin/sh -> dash).
    let spec = CommandSpec::ShellCommand {
        command: "sleep 60 & echo $! >&2; wait".into(),
    };

    // 3) Spawn through the runner. This exercises Plan 15-01's `.process_group(0)`.
    let mut rx = runner.spawn(spec, std::env::temp_dir(), "main".into());

    // 4) FIRST event must be ProcessStarted — locks the CommandEvent doc contract.
    let pgid: i32 = match rx.recv().await {
        Some(CommandEvent::ProcessStarted { pid }) => pid as i32,
        other => panic!("expected ProcessStarted first, got {other:?}"),
    };

    // 5) Pitfall 3 guard: never let kill(-1, ..) leak into the test path.
    assert!(pgid > 1, "refuse to kill(-1, ...): pgid = {pgid}");

    // 6) Drain OutputLine events until we capture the backgrounded sleep's PID
    //    (printed by `echo $! >&2`). Bound the drain at 500ms so a regression
    //    that drops stderr forwarding never hangs the test.
    let sleep_pid: i32 = timeout(Duration::from_millis(500), async {
        loop {
            match rx.recv().await {
                Some(CommandEvent::OutputLine(line)) => {
                    if let Ok(pid) = line.trim().parse::<i32>() {
                        break pid;
                    }
                }
                Some(CommandEvent::ProcessStarted { .. }) => {
                    panic!("ProcessStarted must be emitted exactly once");
                }
                Some(CommandEvent::Exited(status)) => {
                    panic!(
                        "command exited before we read the grandchild PID: {status:?}"
                    );
                }
                None => panic!("rx closed before grandchild PID line arrived"),
            }
        }
    })
    .await
    .expect("did not see backgrounded sleep PID line within 500ms");

    assert!(
        sleep_pid > 1,
        "refuse to probe kill(<=1, ...): sleep_pid = {sleep_pid}"
    );

    // 7) BEFORE assertion: the process group has at least one member (the
    //    grandchild sleep). kill(-pgid, 0) returns 0 if the group is non-empty,
    //    -1 with ESRCH otherwise.
    // SAFETY: pgid validated > 1; libc::kill with sig=0 is a POSIX existence
    // probe with no side effects.
    assert_eq!(
        unsafe { libc::kill(-pgid, 0) },
        0,
        "pgid {pgid} group should be live before we abort"
    );

    // 8) Construct a TokioTaskHandle wrapping the REAL child_pid. The
    //    join_handle slot gets a no-op forwarding task — we drive the receiver
    //    ourselves in the test, so the production forwarding loop is not in
    //    scope here. What matters is that abort() fires SIGTERM → 200ms →
    //    SIGKILL at the REAL pgid (Plan 15-02), proving the production path
    //    reaps grandchildren.
    let handle = TokioTaskHandle {
        join_handle: tokio::spawn(async {}),
        child_pid: pgid as u32,
        cancel_token: tokio_util::sync::CancellationToken::new(),
    };

    // 9) THE OPERATION: invoke abort() — the Plan 15-02 ladder.
    handle.abort();

    // 10) AFTER assertion: poll the PGID every 20ms up to 2s for ESRCH (group
    //     empty). 2s is the ROADMAP success-criterion-1 ceiling.
    let start = std::time::Instant::now();
    let reaped = loop {
        // SAFETY: pgid validated > 1; sig=0 existence probe.
        let probe = unsafe { libc::kill(-pgid, 0) };
        if probe == -1 {
            break true; // ESRCH — group is empty, end-to-end reap proven
        }
        if start.elapsed() > Duration::from_secs(2) {
            break false;
        }
        sleep(Duration::from_millis(20)).await;
    };
    assert!(
        reaped,
        "Phase 15 FAILED: pgid {pgid} still has live members 2 s after \
         TokioTaskHandle::abort() — process-group reap regressed (elapsed {:?})",
        start.elapsed()
    );

    // 11) Final assertion: the specific grandchild sleep PID is dead. Extra
    //     safety beyond the group probe — catches a hypothetical regression
    //     where the PGID is reaped but the sleep was somehow detached.
    // SAFETY: sleep_pid validated > 1; sig=0 existence probe.
    let sleep_alive = unsafe { libc::kill(sleep_pid, 0) };
    assert_eq!(
        sleep_alive, -1,
        "grandchild sleep_pid {sleep_pid} should be dead (ESRCH) after PGID reap"
    );

    // 12) Drain any remaining events so the runner's forwarding task can
    //     complete cleanly (no zombie warnings on test exit). Bounded.
    let _ = timeout(Duration::from_millis(500), async {
        while rx.recv().await.is_some() {}
    })
    .await;
}
