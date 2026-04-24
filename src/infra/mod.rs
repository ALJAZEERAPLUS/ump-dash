//! Infrastructure layer — process spawning, git, JIRA, tmux, file I/O.
//! All concrete implementations are behind trait boundaries (ARCH-02).
//!
//! F-101 closed in Plan 13-05: `command_runner.rs` no longer imports
//! `crate::domain::action::Action`. The adapter emits the typed
//! `CommandEvent` defined in `crate::domain::ports::command_runner_port`
//! and the app layer (currently `src/app.rs::dispatch_command`; Plan 13-08
//! moves this to `effect_runner`) translates `CommandEvent → Action` at
//! the app-side boundary.

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
