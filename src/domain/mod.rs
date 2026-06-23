//! Domain layer — pure Rust. Zero dependencies on ratatui, crossterm, or infra.
//! Note: metro.rs references tokio types for the MetroHandle bridge type — see that
//! file's architectural note for the rationale. mod.rs itself imports nothing from infra.
pub mod action;
pub mod agent_protocol;
pub mod command;
pub mod dash_config;
pub mod jira;
pub mod metro;
pub mod native_cache;
pub mod pipeline;
pub mod ports;
pub mod refresh;
pub mod review;
pub mod staleness;
pub mod task;
pub mod worktree;
pub mod worktree_slice;
