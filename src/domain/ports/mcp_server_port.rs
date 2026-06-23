//! MCP server port — the boundary the app layer uses to start the embedded
//! Model Context Protocol server without naming the concrete infra adapter.
//!
//! Like `command_runner_port` (which returns a `tokio::sync::mpsc::Receiver`),
//! this trait references tokio channel types: the bridge between the async MCP
//! transport (infra) and the single-threaded `update()` loop (app) IS a pair of
//! channels, so they are the natural vocabulary of the boundary. The concrete
//! `rmcp` server, the HTTP listener, and the correlation registry all live in
//! `src/infra/mcp_server.rs`.

#![allow(dead_code)]

use crate::domain::action::Action;
use crate::domain::agent_protocol::{AgentOutcome, AgentRequestId};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

/// Starts the embedded MCP server. Called exactly once at startup by
/// `runtime.rs`, on the tokio runtime.
///
/// - `action_tx` injects agent requests (`Action::Agent`) into the SAME channel
///   `update()` consumes, so every collision/dependency/lock decision is reused.
/// - `agent_reply_rx` delivers the correlated outcomes emitted by
///   `Effect::AgentReply` back to the waiting tool calls.
///
/// `serve` returns immediately; the implementation spawns the listener and the
/// reply-drain task internally (mirrors the "spawn inside, return sync"
/// convention of `CommandRunnerPort::spawn`).
pub trait McpServerPort: Send + Sync {
    fn serve(
        &self,
        action_tx: UnboundedSender<Action>,
        agent_reply_rx: UnboundedReceiver<(AgentRequestId, AgentOutcome)>,
    );
}
