//! Hexagonal ports — trait boundaries owned by the domain layer.
//!
//! Every concrete adapter in `crate::infra` implements one of these traits.
//! The domain + app layers depend only on the trait — never on the infra
//! adapter type — so swapping a real HTTP/process/tmux client for a fake
//! in tests happens without touching domain or app code.
//!
//! Other port modules added in 13-03 (metro), 13-04 (worktree/device/port_probe),
//! 13-05 (command_runner).
pub mod device_port;
pub mod jira_port;
pub mod metro_port;
pub mod multiplexer_port;
pub mod port_probe_port;
pub mod process_port;
pub mod worktree_port;
