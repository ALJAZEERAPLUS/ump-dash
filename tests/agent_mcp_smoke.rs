//! End-to-end smoke test for the embedded MCP server.
//!
//! Proves the public `RmcpAgentServer` actually binds a loopback socket, serves,
//! and publishes its endpoint via the discovery file — i.e. the rmcp + axum
//! wiring works at runtime, not just at compile time. Exercises initialization,
//! tool discovery, and a tool call across the HTTP transport. The per-tool
//! correlation and the `update()` round-trip are covered by unit tests.

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

async fn rpc_response(response: reqwest::Response) -> serde_json::Value {
    let response = response.error_for_status().unwrap();
    let content_type = response.headers()[reqwest::header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .to_owned();
    let body = response.text().await.unwrap();
    if content_type.starts_with("application/json") {
        return serde_json::from_str(&body).unwrap();
    }
    // Streamable HTTP permits SSE responses. Ignore the empty priming event.
    assert!(
        content_type.starts_with("text/event-stream"),
        "{content_type}: {body}"
    );
    let data = body
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim)
        .find(|data| !data.is_empty())
        .expect("response should contain a JSON-RPC data event");
    serde_json::from_str(data).unwrap()
}

#[tokio::test]
async fn server_binds_and_serves_mcp_tools() {
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

    let discovery: serde_json::Value = serde_json::from_str(&body).expect("valid discovery JSON");
    let url = discovery["url"].as_str().expect("discovery endpoint");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let response = client
        .post(url)
        .header("accept", "application/json, text/event-stream")
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 1, "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "ump-dash-smoke", "version": "1.0.0" }
            }
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap();
    let session = response
        .headers()
        .get("mcp-session-id")
        .expect("initialize should create a session")
        .clone();
    let initialized = rpc_response(response).await;
    assert_eq!(initialized["id"], 1);
    assert!(initialized["error"].is_null(), "{initialized}");
    assert!(initialized["result"]["capabilities"]["tools"].is_object());
    let protocol = initialized["result"]["protocolVersion"].as_str().unwrap();

    let request = || {
        client
            .post(url)
            .header("accept", "application/json, text/event-stream")
            .header("mcp-session-id", session.clone())
            .header("mcp-protocol-version", protocol)
    };

    let response = request()
        .json(&serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);

    let response = request()
        .json(&serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }))
        .send()
        .await
        .unwrap();
    let listed = rpc_response(response).await;
    assert_eq!(listed["id"], 2);
    let tools = listed["result"]["tools"].as_array().expect("tool list");
    for name in [
        "list_devices",
        "run_ios",
        "run_android",
        "get_worktree_status",
    ] {
        assert!(
            tools.iter().any(|tool| tool["name"] == name),
            "missing {name}"
        );
    }

    let response = request()
        .json(&serde_json::json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "list_devices", "arguments": { "platform": "ios" } }
        }))
        .send()
        .await
        .unwrap();
    let called = rpc_response(response).await;
    assert_eq!(called["id"], 3);
    assert!(called["error"].is_null(), "{called}");
    assert_ne!(called["result"]["isError"], true, "{called}");
    assert_eq!(called["result"]["content"][0]["type"], "text");
    assert_eq!(called["result"]["content"][0]["text"], "[]");

    let _ = std::fs::remove_dir_all(&repo_root);
}
