//! COVER-02 — characterization of POSIX process-group kill.
//!
//! This test spawns `bash -c 'trap : TERM; sleep 30 & wait'` via
//! `tokio::process::Command::process_group(0)`, then sends SIGTERM to the
//! PGID (not the PID). The bash parent INSTALLS A TRAP HANDLER that does
//! nothing (`:` is the shell no-op builtin), which means bash actively
//! catches SIGTERM without exiting from it. The sleep(30) child receives
//! SIGTERM via the PGID broadcast, dies with the default disposition,
//! bash's `wait` returns, and bash exits. If `.process_group(0)` were absent
//! or broken, `kill(-child_pid, TERM)` would fail with ESRCH (no process
//! group with that PGID exists) and the test would assert-fail on
//! `kill_rc == 0`; if delivery to the child were somehow blocked, the
//! 2-second `timeout(child.wait())` would fail instead.
//!
//! This adversarial fixture is the entire point: it can only pass if the
//! PGID mechanism is working. A signal sent to the bash PID alone (with the
//! trap installed) would be caught-and-ignored by the handler and sleep
//! would live for 30 s — only the PGID broadcast reaches sleep directly.
//!
//! NOTE on the original `trap "" SIGTERM` fixture in 12-RESEARCH.md §Pattern 3:
//! that fixture is BROKEN on macOS (and arguably on every POSIX) because
//! `trap "" SIG` sets disposition to SIG_IGN, which — unlike a trap handler
//! — is INHERITED by forked children (POSIX spec: execve resets handlers,
//! but SIG_IGN survives fork+exec). So the sleep child also ignored SIGTERM
//! and the PGID broadcast reached nobody. `trap : TERM` (no-op handler)
//! avoids this: the handler is reset to SIG_DFL in the child, so sleep
//! dies normally. Deviation from plan truth-statement documented in
//! 12-02-SUMMARY.md.
//!
//! NOTE: this test spawns `tokio::process::Command` directly — NOT through
//! `infra::command_runner::spawn_command_task`, because `command_runner.rs`
//! does not currently call `.process_group(0)` (Pitfall 6 in 12-RESEARCH.md;
//! flagged for Phase 13 refactor, not fixed here).

#![cfg(any(target_os = "linux", target_os = "macos"))]

use std::time::Duration;
use tokio::process::Command;
use tokio::time::{sleep, timeout};

#[tokio::test(flavor = "multi_thread")]
async fn killing_pgid_reaps_child_tree() -> anyhow::Result<()> {
    // Spawn the fixture. `trap : TERM` installs a no-op SIGTERM handler on
    // bash — bash will catch SIGTERM and keep running. Unlike `trap "" TERM`
    // (which sets SIG_IGN and IS inherited by forked children), an explicit
    // handler is reset to SIG_DFL in the sleep child. So a PGID-targeted
    // SIGTERM is handled-and-ignored by bash but kills sleep outright; bash's
    // `wait` then returns, and bash exits.
    let mut child = Command::new("bash")
        .args(["-c", "trap : TERM; sleep 30 & wait"])
        .process_group(0)       // THE invariant under test
        .kill_on_drop(true)     // defense-in-depth — reaps on test-body panic
        .spawn()?;

    let pgid: i32 = child
        .id()
        .expect("tokio::process::Child must expose pid before wait()")
        .try_into()
        .expect("child pid must fit in i32 on Linux/macOS");

    // Pitfall 3: kill(-1, ...) is a BROADCAST to every process of our UID.
    // Catastrophic on a dev machine. Assert we never ask libc to do that.
    assert!(pgid > 1, "refuse to kill(-1, ...): pgid = {pgid}");

    // Give bash ~100 ms to fork its sleep(30) child and join the PGID.
    // Without this, we might send SIGTERM BEFORE sleep has joined the group,
    // failing to exercise the full invariant.
    sleep(Duration::from_millis(100)).await;

    // Probe: pgid group must have at least one member at this point.
    // kill(pgid_negated, 0) returns 0 if group non-empty, -1 with ESRCH otherwise.
    // SAFETY: libc::kill is a POSIX syscall with well-defined behavior; we
    // validated pgid > 1 above.
    assert_eq!(
        unsafe { libc::kill(-pgid, 0) },
        0,
        "pgid {pgid} group should be live before we send SIGTERM"
    );

    // THE OPERATION: SIGTERM to the entire PGID. Bash runs its no-op trap
    // handler (`:`) and keeps waiting; sleep (with SIG_DFL) dies; bash's
    // wait returns, bash exits cleanly.
    // SAFETY: same as above; pgid is our own child's pgid, owned by this UID.
    let kill_rc = unsafe { libc::kill(-pgid, libc::SIGTERM) };
    assert_eq!(
        kill_rc, 0,
        "libc::kill(-pgid, SIGTERM) should succeed; errno would be in *__errno_location()"
    );

    // Wait with a hard ceiling. If PGID kill is broken, this hits 2 s and
    // fails the test loudly with a clear message.
    let status = timeout(Duration::from_secs(2), child.wait())
        .await
        .map_err(|_| anyhow::anyhow!(
            "COVER-02 FAILED: bash parent + sleep child did not exit within 2 s \
             of SIGTERM to PGID {pgid} — PGID-kill invariant is broken"
        ))??;

    // Parent bash typically exits with status 143 (128 + SIGTERM) because
    // its `wait` builtin returns the sleep child's death-by-signal status,
    // but exact shape varies across bash versions and OSes. The LOAD-BEARING
    // assertion is that it exited at all within 2 s — not the exit code.
    let _ = status;

    // Final invariant: the PGID group is empty (ESRCH). Poll up to 500 ms
    // for the reaper to complete — faster than a fixed sleep on macOS where
    // waitpid-cascade ordering is not wall-clock bounded.
    let start = std::time::Instant::now();
    loop {
        // SAFETY: same as above.
        let probe = unsafe { libc::kill(-pgid, 0) };
        if probe == -1 {
            break; // ESRCH — group is empty, invariant holds
        }
        if start.elapsed() > Duration::from_millis(500) {
            panic!(
                "COVER-02 FAILED: pgid {pgid} still has live members 500 ms after \
                 child.wait() returned — orphaned descendant detected"
            );
        }
        sleep(Duration::from_millis(20)).await;
    }

    Ok(())
}
