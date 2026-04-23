//! Shared helpers for the integration-test crate (tests/*.rs).
//!
//! Convention: files under `tests/` are each a separate compiled binary. A
//! `tests/common/mod.rs` is the Rust-book-approved way to share code across
//! those files without creating an independent `tests/common.rs` binary.
//! (See https://doc.rust-lang.org/book/ch11-03-test-organization.html
//!  § "Submodules in Integration Tests".)

use rn_dash::domain::metro::MetroHandle;

/// Build a dummy `MetroHandle` suitable for tests that only care about the
/// `MetroManager::register() / is_running() / take_handle()` invariant —
/// NOT for tests that interact with the stream_task or stdin_task.
///
/// SAFETY: this MUST run inside a tokio runtime (i.e. called from a
/// `#[tokio::test]` function), because `tokio::spawn` requires one.
pub fn fake_metro_handle(pid: u32, worktree: &str) -> MetroHandle {
    let (stdin_tx, _stdin_rx) = tokio::sync::mpsc::unbounded_channel();
    let (kill_tx, _kill_rx) = tokio::sync::oneshot::channel();
    MetroHandle {
        pid,
        worktree_id: worktree.to_string(),
        stdin_tx,
        stream_task: tokio::spawn(async {}),
        stdin_task: tokio::spawn(async {}),
        kill_tx: Some(kill_tx),
    }
}
