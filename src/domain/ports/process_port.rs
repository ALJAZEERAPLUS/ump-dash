//! ProcessPort — domain-layer trait boundary for metro process spawning.
//!
//! ARCH-02: Domain and app layers depend only on this trait; infra provides
//! the concrete `TokioProcessClient` implementation. Tests may supply a fake.
//!
//! Pragmatic exception: `tokio::process::Child` in the return type is kept
//! here rather than modeled as a domain-owned handle type — mirrors the
//! existing exception comment at `domain/metro.rs:5-13` (MetroHandle). The
//! child handle is opaque infrastructure passed through the domain boundary,
//! never inspected inside the domain layer itself.

#![allow(dead_code)]

use std::path::PathBuf;
use tokio::process::Child;

/// Trait boundary for metro process spawning.
///
/// The domain and app layers depend only on this trait. `TokioProcessClient`
/// in `infra::process` is the production implementation; tests may supply a
/// fake.
#[async_trait::async_trait]
pub trait ProcessPort: Send + Sync {
    /// Spawn a metro dev server in the given worktree directory.
    ///
    /// Returns the `Child` handle with stdout, stderr, and stdin all piped.
    /// The caller is responsible for taking those handles before any kill call
    /// (see research pitfall 5).
    ///
    /// Pipes stdout/stderr/stdin for capture by drain_metro_output.
    async fn spawn_metro(&self, worktree_path: PathBuf) -> anyhow::Result<Child>;
}
