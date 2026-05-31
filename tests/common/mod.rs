//! Shared helpers for the integration-test crate (tests/*.rs).
//!
//! Convention: files under `tests/` are each a separate compiled binary. A
//! `tests/common/mod.rs` is the Rust-book-approved way to share code across
//! those files without creating an independent `tests/common.rs` binary.
//! (See https://doc.rust-lang.org/book/ch11-03-test-organization.html
//!  § "Submodules in Integration Tests".)
//!
//! Plan 13-03: `MetroHandle` is now a trait. `fake_metro_handle` returns a
//! `Box<dyn MetroHandle>` backed by a no-op `FakeMetroHandle` impl — no
//! tokio channels required, so the helper can be called from plain `#[test]`
//! functions (not just `#[tokio::test]`).

use ump_dash::domain::ports::metro_port::MetroHandle;

/// Minimal `MetroHandle` impl used by integration tests that only need
/// `MetroManager::register / is_running / take_handle` semantics — NOT for
/// tests that exercise stdin delivery or kill-path behavior.
#[derive(Debug)]
struct FakeMetroHandle {
    pid: u32,
    worktree_id: String,
    port: u16,
}

impl MetroHandle for FakeMetroHandle {
    fn pid(&self) -> u32 {
        self.pid
    }
    fn worktree_id(&self) -> &str {
        &self.worktree_id
    }
    fn port(&self) -> u16 {
        self.port
    }
    fn send_stdin(&self, _bytes: Vec<u8>) -> anyhow::Result<()> {
        Ok(())
    }
    fn kill(self: Box<Self>) -> anyhow::Result<()> {
        Ok(())
    }
}

/// Build a dummy `Box<dyn MetroHandle>` for tests that exercise the
/// `MetroManager::register` / `is_running` / `take_handle` invariant.
///
/// Synchronous — no tokio runtime required post-13-03.
pub fn fake_metro_handle(pid: u32, worktree: &str) -> Box<dyn MetroHandle> {
    Box::new(FakeMetroHandle {
        pid,
        worktree_id: worktree.to_string(),
        port: 8081,
    })
}
