//! Infrastructure layer — process spawning, git, JIRA, multiplexer, file I/O.
//!
//! All concrete implementations are behind domain-defined port traits
//! (F-101..F-110 resolved in Phase 13; see
//! `.planning/phases/11-architecture-audit/AUDIT.md`). The composition root
//! (`src/main.rs`) constructs concrete adapters and bundles them into
//! `crate::app::Adapters`; the app layer hops through that bundle, never
//! importing `crate::infra::*` directly (G-01 in `Makefile arch-lint`).
//!
//! Plan 13-10 (F-100, F-112): doc-comment updated to reflect Phase 13 end
//! state; the deprecated `tmux` module was deleted (its `TmuxAdapter`
//! replacement lives in `multiplexer.rs`).

pub mod port;
pub mod process;
pub mod worktrees;
pub mod command_runner;
pub mod devices;
pub mod config;
pub mod external_command;
pub mod jira;
pub mod jira_cache;
pub mod multiplexer;
pub mod self_update;
pub mod native_cache;
pub mod sim_history;
pub mod task_handle;
pub mod metro;
