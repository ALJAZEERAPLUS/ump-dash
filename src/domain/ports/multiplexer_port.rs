//! MultiplexerPort — domain-layer trait boundary for terminal multiplexer
//! operations (tmux + zellij).
//!
//! ARCH-02: Domain and app layers depend only on this trait; `infra::multiplexer`
//! supplies `TmuxAdapter` and `ZellijAdapter` concrete implementations plus the
//! `detect_multiplexer` auto-detection helper.

#![allow(dead_code)]

use std::path::Path;

/// Trait for terminal multiplexer operations.
/// Implementors must be `Send + Sync + Debug` for storage in `AppState`.
pub trait MultiplexerPort: Send + Sync + std::fmt::Debug {
    /// Creates a new window/tab at the given path with the given name, running
    /// the given command. The window should switch focus to the newly created tab.
    fn new_window(&self, path: &Path, name: &str, command: &str) -> anyhow::Result<()>;

    /// Returns true if this multiplexer is available in the current environment.
    fn is_available(&self) -> bool;
}
