//! End-to-end smoke test for the embedded MCP server.
//!
//! Proves the public `RmcpAgentServer` actually binds a loopback socket, serves,
//! and publishes its endpoint via the discovery file — i.e. the rmcp + axum
//! wiring works at runtime, not just at compile time. The per-tool correlation
//! and the `update()` round-trip are covered by unit tests; here we only need to
//! know the transport comes up.

use std::sync::Arc;
use std::time::Duration;

use ump_dash::domain::action::Action;
use ump_dash::domain::agent_protocol::{AgentOutcome, AgentRequestId};
use ump_dash::domain::command::DeviceInfo;
use ump_dash::domain::ports::device_port::{DeviceKind, DevicePort};
use ump_dash::domain::ports::mcp_server_port::McpServerPort;
use ump_dash::infra::mcp_server::RmcpAgentServer;

struct NoopDevices;

#[async_trait::async_trait]
impl DevicePort for NoopDevices {
    async fn list(&self, _kind: DeviceKind) -> anyhow::Result<Vec<DeviceInfo>> {
        Ok(vec![])
    }
}

#[tokio::test]
async fn server_binds_and_publishes_discovery_file() {
    // Isolated repo root with a .git dir for the discovery file.
    let repo_root = std::env::temp_dir().join(format!("ump-mcp-smoke-{}", std::process::id()));
    let git_dir = repo_root.join(".git");
    std::fs::create_dir_all(&git_dir).expect("create temp .git");

    let (action_tx, _action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
    let (_reply_tx, reply_rx) =
        tokio::sync::mpsc::unbounded_channel::<(AgentRequestId, AgentOutcome)>();

    // Port 0 → ephemeral; the actual port is published to the discovery file.
    let server = RmcpAgentServer::new(0, repo_root.clone(), Arc::new(NoopDevices));
    server.serve(action_tx, reply_rx);

    // Poll for the discovery file (the server writes it only after a successful bind).
    let discovery = git_dir.join("ump-dash-mcp.json");
    let mut contents = None;
    for _ in 0..50 {
        if let Ok(body) = std::fs::read_to_string(&discovery) {
            contents = Some(body);
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let body = contents.expect("server should publish a discovery file after binding");

    // Extract the bound port and confirm something is actually listening on it.
    let port: u16 = body
        .split("\"port\":")
        .nth(1)
        .and_then(|rest| rest.split([',', '}']).next())
        .and_then(|n| n.trim().parse().ok())
        .expect("discovery file should contain a numeric port");
    assert!(port > 0, "ephemeral bind should yield a real port");

    std::net::TcpStream::connect(("127.0.0.1", port))
        .expect("MCP server should accept TCP connections on the published port");

    let _ = std::fs::remove_dir_all(&repo_root);
}
