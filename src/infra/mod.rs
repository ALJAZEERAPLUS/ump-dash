//! Infrastructure layer — process spawning, git, JIRA, tmux, file I/O.
//! All concrete implementations are behind trait boundaries (ARCH-02).
//!
//! NOTE: command_runner.rs still imports `crate::domain::action::Action` —
//! Plan 13-05 removes this via CommandEvent per AUDIT F-101.

pub mod port;
pub mod process;
pub mod worktrees;
pub mod command_runner;
pub mod devices;
pub mod config;
pub mod jira;
pub mod jira_cache;
pub mod tmux;
pub mod multiplexer;
pub mod sim_history;
pub mod android_prefs;
