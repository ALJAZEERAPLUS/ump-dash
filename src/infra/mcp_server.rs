//! Embedded MCP server (rmcp, streamable-HTTP) — the ONLY new tokio/socket/HTTP
//! site for the agent feature.
//!
//! Architecture: agents (Claude Code instances in worktrees) call MCP tools over
//! localhost HTTP. Each tool builds a domain `Action::Agent` and pushes it onto
//! the SAME `action_tx` channel the keyboard drives, then awaits the correlated
//! `AgentOutcome`. Correlation: a per-request `oneshot` is registered in
//! `AgentGateway.registry` keyed by `AgentRequestId`; `update()` emits
//! `Effect::AgentReply`, `effect_runner` forwards `(id, outcome)` on the
//! `agent_reply` mpsc, and the reply-drain task here resolves the oneshot.
//!
//! Because every request funnels through `update()`, all existing locks and
//! dependency decisions (collision policy, per-repo yarn semaphore, metro
//! single-instance, dependency recipes) are reused — this server adds no
//! orchestration of its own.
//!
//! Agent identity: each tool takes the agent's absolute `worktree` path (its
//! cwd) as an argument; `update()` resolves it against the live worktree set.

#![allow(dead_code)]

use crate::domain::action::Action;
use crate::domain::agent_protocol::{AgentOutcome, AgentRequest, AgentRequestId, DeviceTarget};
use crate::domain::ports::device_port::{DeviceKind, DevicePort};
use crate::domain::ports::mcp_server_port::McpServerPort;
use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::streamable_http_server::{
        StreamableHttpServerConfig, StreamableHttpService, session::local::LocalSessionManager,
    },
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

/// How long a tool call waits for the dashboard to return the immediate
/// lock/block decision. The decision is produced synchronously inside one
/// `update()` pass, so this only guards against a wedged event loop.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Shared bridge between the async MCP tools and the single-threaded `update()`
/// loop. Cloned (via `Arc`) into every per-session tool server.
struct AgentGateway {
    action_tx: UnboundedSender<Action>,
    registry: Mutex<HashMap<AgentRequestId, oneshot::Sender<AgentOutcome>>>,
    devices: Arc<dyn DevicePort>,
}

impl AgentGateway {
    /// Send an agent request into the dashboard and await its correlated outcome.
    async fn request(&self, cwd: String, request: AgentRequest) -> AgentOutcome {
        let request_id = AgentRequestId::next();
        let (tx, rx) = oneshot::channel();
        {
            let mut reg = self.registry.lock().unwrap();
            reg.insert(request_id, tx);
        }
        if self
            .action_tx
            .send(Action::Agent {
                request_id,
                cwd: PathBuf::from(cwd),
                request,
            })
            .is_err()
        {
            self.registry.lock().unwrap().remove(&request_id);
            return AgentOutcome::Error {
                message: "dashboard event loop is not running".into(),
            };
        }
        match tokio::time::timeout(REQUEST_TIMEOUT, rx).await {
            Ok(Ok(outcome)) => outcome,
            _ => {
                self.registry.lock().unwrap().remove(&request_id);
                AgentOutcome::Error {
                    message: "dashboard did not respond in time".into(),
                }
            }
        }
    }
}

fn json(outcome: &AgentOutcome) -> String {
    serde_json::to_string(outcome)
        .unwrap_or_else(|e| format!("{{\"error\":\"failed to serialize outcome: {e}\"}}"))
}

fn parse_variant(s: Option<String>) -> Option<crate::domain::command::RunVariant> {
    use crate::domain::command::RunVariant;
    match s.as_deref() {
        Some("local") => Some(RunVariant::Local),
        Some("dev") => Some(RunVariant::Dev),
        Some("prod") => Some(RunVariant::Prod),
        _ => None,
    }
}

// --- Tool argument schemas -------------------------------------------------
// Every tool carries `worktree`: the agent's absolute working directory, used
// to resolve which worktree the request targets.

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct WorktreeArgs {
    /// Absolute path of your worktree (your current working directory).
    worktree: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct LogsArgs {
    /// Absolute path of your worktree (your current working directory).
    worktree: String,
    /// Number of trailing output lines to return (default: all retained).
    tail: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct RunArgs {
    /// Absolute path of your worktree (your current working directory).
    worktree: String,
    /// Target device/simulator id. Pass the `id` field from `list_devices`
    /// (the simulator/emulator UDID or device serial) — NOT the human-readable
    /// `name`. The build cache (fast prebuilt launch) only applies when this is
    /// a simulator UDID.
    device_id: String,
    /// Build variant: "local" (default), "dev", or "prod". A Local cache hit is
    /// used automatically for a fast prebuilt launch.
    variant: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct BuildArgs {
    /// Absolute path of your worktree (your current working directory).
    worktree: String,
    /// Also install the built artifact via adb when the build succeeds.
    install: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SyncArgs {
    /// Absolute path of your worktree (your current working directory).
    worktree: String,
    /// Also run `yarn pod-install` after `yarn install` (iOS).
    include_pods: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ShellArgs {
    /// Absolute path of your worktree (your current working directory).
    worktree: String,
    /// Shell command to run in the worktree directory.
    command: String,
    /// Must be true — running arbitrary shell is gated behind explicit confirmation.
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CleanArgs {
    /// Absolute path of your worktree (your current working directory).
    worktree: String,
    /// Remove node_modules.
    node_modules: Option<bool>,
    /// Clean CocoaPods.
    pods: Option<bool>,
    /// Clean the Android build.
    android: Option<bool>,
    /// Must be true — destructive clean is gated behind explicit confirmation.
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ConfirmArgs {
    /// Absolute path of your worktree (your current working directory).
    worktree: String,
    /// Must be true — destructive op is gated behind explicit confirmation.
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct CreateWorktreeArgs {
    /// Absolute path of your current worktree. Used to authorize the request
    /// against the dashboard's known worktree set.
    worktree: String,
    /// Branch to check out. If base_branch is set, this is the new branch name.
    branch: String,
    /// Optional base branch name for creating a new branch worktree. Use names
    /// like "main" or "rc-10.0.0", not "origin/main".
    base_branch: Option<String>,
    /// Must be true — worktree creation mutates local git state.
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DeleteWorktreeArgs {
    /// Absolute path of your current worktree. Used to authorize the request
    /// against the dashboard's known worktree set.
    worktree: String,
    /// Absolute path of the worktree to delete. The main repo root is refused.
    target_worktree: String,
    /// Must be true — deletion runs `git worktree remove --force`.
    confirm: Option<bool>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct DevicesArgs {
    /// "ios" or "android".
    platform: String,
}

/// Per-session MCP tool server. Holds the shared gateway + the generated tool
/// router. Cloned by the `StreamableHttpService` factory per session.
#[derive(Clone)]
struct McpToolServer {
    gateway: Arc<AgentGateway>,
    tool_router: ToolRouter<Self>,
}

impl McpToolServer {
    fn new(gateway: Arc<AgentGateway>) -> Self {
        Self {
            gateway,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl McpToolServer {
    #[tool(
        description = "Optional diagnostic: pre-flight status (dependency staleness, metro state + port, running task, queue). You do NOT need this before run_ios/run_android — they handle deps/metro/cache themselves."
    )]
    async fn get_worktree_status(&self, Parameters(a): Parameters<WorktreeArgs>) -> String {
        json(&self.gateway.request(a.worktree, AgentRequest::GetWorktreeStatus).await)
    }

    #[tool(
        description = "Running task + queue for a cold build. NOTE: a cached (prebuilt) launch is instant and has NO task — it will not appear here; check get_logs for its result instead."
    )]
    async fn get_task_status(&self, Parameters(a): Parameters<WorktreeArgs>) -> String {
        json(&self.gateway.request(a.worktree, AgentRequest::GetTaskStatus).await)
    }

    #[tool(description = "Tail of the retained command logs for your worktree (includes cached-launch results, e.g. '[cached-ios] installed and launched cached app').")]
    async fn get_logs(&self, Parameters(a): Parameters<LogsArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::GetLogs {
                        tail: a.tail.map(|n| n as usize),
                    },
                )
                .await,
        )
    }

    #[tool(
        description = "Optional: start Metro standalone (front-loads `yarn install` if stale); returns the Rozenite port. Rarely needed — run_ios/run_android start Metro for you."
    )]
    async fn start_metro(&self, Parameters(a): Parameters<WorktreeArgs>) -> String {
        json(&self.gateway.request(a.worktree, AgentRequest::StartMetro).await)
    }

    #[tool(
        description = "Run the app on an iOS simulator. Just call this — it syncs deps, starts Metro, and uses the prebuilt cache automatically; do NOT call start_metro/sync_deps/get_worktree_status first. device_id must be the `id` from list_devices (the simulator UDID), not the name. variant defaults to local. On a cache hit the launch is instant with no task to poll — its result shows in get_logs."
    )]
    async fn run_ios(&self, Parameters(a): Parameters<RunArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::RunIos {
                        device_id: a.device_id,
                        variant: parse_variant(a.variant),
                    },
                )
                .await,
        )
    }

    #[tool(
        description = "Run the app on an Android emulator/device. Just call this — it syncs deps, starts Metro, boots a stopped emulator, and uses the prebuilt cache automatically; do NOT call start_metro/sync_deps/get_worktree_status first. device_id must be the `id` from list_devices (the emulator/device serial), not the name. variant defaults to local. On a cache hit the launch is instant with no task to poll — its result shows in get_logs."
    )]
    async fn run_android(&self, Parameters(a): Parameters<RunArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::RunAndroid {
                        device_id: a.device_id,
                        variant: parse_variant(a.variant),
                    },
                )
                .await,
        )
    }

    #[tool(
        description = "Build the Android release artifact (ensures deps + Metro first). Set install=true to also adb-install it."
    )]
    async fn build(&self, Parameters(a): Parameters<BuildArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::Build {
                        install: a.install.unwrap_or(false),
                    },
                )
                .await,
        )
    }

    #[tool(
        description = "Sync JS dependencies (`yarn install`), optionally pods too. Blocked if an install is already running."
    )]
    async fn sync_deps(&self, Parameters(a): Parameters<SyncArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::SyncDeps {
                        include_pods: a.include_pods.unwrap_or(false),
                    },
                )
                .await,
        )
    }

    #[tool(description = "Cancel the running task on your worktree (git operations are protected and cannot be cancelled).")]
    async fn cancel(&self, Parameters(a): Parameters<WorktreeArgs>) -> String {
        json(&self.gateway.request(a.worktree, AgentRequest::Cancel).await)
    }

    #[tool(description = "Run an arbitrary shell command in your worktree. Requires confirm=true.")]
    async fn shell(&self, Parameters(a): Parameters<ShellArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::Shell {
                        command: a.command,
                        confirm: a.confirm.unwrap_or(false),
                    },
                )
                .await,
        )
    }

    #[tool(description = "Clean native build artifacts (node_modules/pods/android). Destructive — requires confirm=true.")]
    async fn clean(&self, Parameters(a): Parameters<CleanArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::Clean {
                        node_modules: a.node_modules.unwrap_or(false),
                        pods: a.pods.unwrap_or(false),
                        android: a.android.unwrap_or(false),
                        confirm: a.confirm.unwrap_or(false),
                    },
                )
                .await,
        )
    }

    #[tool(description = "`git reset --hard`. Destructive — requires confirm=true.")]
    async fn reset_hard(&self, Parameters(a): Parameters<ConfirmArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::ResetHard {
                        confirm: a.confirm.unwrap_or(false),
                    },
                )
                .await,
        )
    }

    #[tool(description = "`rm -rf node_modules`. Destructive — requires confirm=true.")]
    async fn rm_node_modules(&self, Parameters(a): Parameters<ConfirmArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::RmNodeModules {
                        confirm: a.confirm.unwrap_or(false),
                    },
                )
                .await,
        )
    }

    #[tool(
        description = "Create a git worktree. Pass branch to check out an existing branch; include base_branch (without origin/) to create branch from that base. Requires confirm=true."
    )]
    async fn create_worktree(&self, Parameters(a): Parameters<CreateWorktreeArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::CreateWorktree {
                        branch: a.branch,
                        base_branch: a.base_branch,
                        confirm: a.confirm.unwrap_or(false),
                    },
                )
                .await,
        )
    }

    #[tool(
        description = "Delete a git worktree by absolute path using `git worktree remove --force`. Requires target_worktree and confirm=true; refuses the main repo root."
    )]
    async fn delete_worktree(&self, Parameters(a): Parameters<DeleteWorktreeArgs>) -> String {
        json(
            &self
                .gateway
                .request(
                    a.worktree,
                    AgentRequest::DeleteWorktree {
                        target_worktree: a.target_worktree,
                        confirm: a.confirm.unwrap_or(false),
                    },
                )
                .await,
        )
    }

    #[tool(
        description = "List available run targets for a platform (\"ios\" or \"android\"), each tagged with whether it is currently running/booted."
    )]
    async fn list_devices(&self, Parameters(a): Parameters<DevicesArgs>) -> String {
        let kind = match a.platform.to_ascii_lowercase().as_str() {
            "ios" => DeviceKind::Ios,
            "android" => DeviceKind::Android,
            other => {
                return format!("{{\"error\":\"unknown platform: {other} (use ios|android)\"}}");
            }
        };
        match self.gateway.devices.list(kind).await {
            Ok(devices) => {
                let targets: Vec<DeviceTarget> = devices
                    .into_iter()
                    .map(|d| {
                        let running = match kind {
                            DeviceKind::Ios => d.name.contains("(Booted)"),
                            DeviceKind::Android => !d.name.contains("(available)"),
                        };
                        DeviceTarget {
                            id: d.id,
                            name: d.name,
                            running,
                        }
                    })
                    .collect();
                serde_json::to_string(&targets)
                    .unwrap_or_else(|e| format!("{{\"error\":\"serialize: {e}\"}}"))
            }
            Err(e) => format!("{{\"error\":\"device enumeration failed: {e}\"}}"),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for McpToolServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some(
            "Dashboard control for your git worktree. Pass your absolute worktree path \
             (your cwd) as `worktree` on every call. To run the app, just call run_ios or \
             run_android with a device_id from list_devices — nothing else. The dashboard \
             automatically syncs dependencies, starts Metro, and uses the build cache; you do \
             NOT need to check status, start Metro, or sync deps first. Action tools return \
             immediately with a decision + task_id; poll get_task_status / get_logs for \
             completion. start_metro, sync_deps, build, and get_worktree_status are optional \
             diagnostics that are rarely needed. Destructive tools require confirm=true."
                .into(),
        );
        info
    }
}

/// Production `McpServerPort` adapter: binds an rmcp streamable-HTTP server on
/// loopback and bridges tool calls into the dashboard event loop.
pub struct RmcpAgentServer {
    bind_port: u16,
    repo_root: PathBuf,
    devices: Arc<dyn DevicePort>,
}

impl RmcpAgentServer {
    pub fn new(bind_port: u16, repo_root: PathBuf, devices: Arc<dyn DevicePort>) -> Self {
        Self {
            bind_port,
            repo_root,
            devices,
        }
    }
}

impl McpServerPort for RmcpAgentServer {
    fn serve(
        &self,
        action_tx: UnboundedSender<Action>,
        mut agent_reply_rx: UnboundedReceiver<(AgentRequestId, AgentOutcome)>,
    ) {
        let gateway = Arc::new(AgentGateway {
            action_tx,
            registry: Mutex::new(HashMap::new()),
            devices: self.devices.clone(),
        });

        // Reply-drain task: resolve correlation oneshots as replies arrive.
        let drain_gateway = gateway.clone();
        tokio::spawn(async move {
            while let Some((request_id, outcome)) = agent_reply_rx.recv().await {
                if let Some(tx) = drain_gateway.registry.lock().unwrap().remove(&request_id) {
                    let _ = tx.send(outcome);
                }
            }
        });

        // HTTP listener task.
        let bind_port = self.bind_port;
        let repo_root = self.repo_root.clone();
        tokio::spawn(async move {
            let factory_gateway = gateway.clone();
            let service: StreamableHttpService<McpToolServer, LocalSessionManager> =
                StreamableHttpService::new(
                    move || Ok(McpToolServer::new(factory_gateway.clone())),
                    Default::default(),
                    // Plain JSON responses for simple request/response tools —
                    // no SSE framing needed for the poll-based protocol.
                    StreamableHttpServerConfig::default().with_json_response(true),
                );

            let router = axum::Router::new().nest_service("/mcp", service);
            let listener =
                match tokio::net::TcpListener::bind(("127.0.0.1", bind_port)).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::warn!("MCP server: failed to bind 127.0.0.1:{bind_port}: {e}");
                        return;
                    }
                };
            let port = listener.local_addr().map(|a| a.port()).unwrap_or(bind_port);
            write_discovery_file(&repo_root, port);
            tracing::info!("MCP server listening on http://127.0.0.1:{port}/mcp");
            if let Err(e) = axum::serve(listener, router).await {
                tracing::warn!("MCP server stopped: {e}");
            }
        });
    }
}

/// Write `<repo_root>/.git/ump-dash-mcp.json` so agents/worktrees can discover
/// the live endpoint. Best-effort — failures are logged, not fatal.
fn write_discovery_file(repo_root: &std::path::Path, port: u16) {
    let path = repo_root.join(".git").join("ump-dash-mcp.json");
    let pid = std::process::id();
    let body = format!(
        "{{\"port\":{port},\"pid\":{pid},\"url\":\"http://127.0.0.1:{port}/mcp\"}}\n"
    );
    if let Err(e) = std::fs::write(&path, body) {
        tracing::warn!("MCP server: failed to write discovery file {path:?}: {e}");
    }
}

#[cfg(test)]
mod tests {
    //! Correlation-layer tests for the MCP gateway: a tool's `request()` must
    //! resolve to the reply tagged with its own `AgentRequestId`, even under
    //! concurrency. The `Action::Agent` -> `Effect::AgentReply` round-trip
    //! through `update()` is covered by the pure dispatch tests; the HTTP
    //! transport itself is covered upstream by rmcp's own suite.

    use super::*;
    use crate::domain::command::DeviceInfo;

    struct NoopDevices;

    #[async_trait::async_trait]
    impl DevicePort for NoopDevices {
        async fn list(&self, _kind: DeviceKind) -> anyhow::Result<Vec<DeviceInfo>> {
            Ok(vec![])
        }
    }

    fn test_gateway(action_tx: UnboundedSender<Action>) -> Arc<AgentGateway> {
        Arc::new(AgentGateway {
            action_tx,
            registry: Mutex::new(HashMap::new()),
            devices: Arc::new(NoopDevices),
        })
    }

    fn spawn_drain(
        gateway: Arc<AgentGateway>,
        mut rx: UnboundedReceiver<(AgentRequestId, AgentOutcome)>,
    ) {
        tokio::spawn(async move {
            while let Some((id, outcome)) = rx.recv().await {
                if let Some(tx) = gateway.registry.lock().unwrap().remove(&id) {
                    let _ = tx.send(outcome);
                }
            }
        });
    }

    #[tokio::test]
    async fn request_resolves_to_its_own_reply() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
        let (reply_tx, reply_rx) = tokio::sync::mpsc::unbounded_channel();
        let gateway = test_gateway(action_tx);
        spawn_drain(gateway.clone(), reply_rx);

        // Fake dashboard: reply to each Action::Agent with the SAME request_id.
        tokio::spawn(async move {
            while let Some(Action::Agent { request_id, .. }) = action_rx.recv().await {
                let _ = reply_tx.send((request_id, AgentOutcome::NothingToCancel));
            }
        });

        let outcome = gateway
            .request("/tmp/wt".into(), AgentRequest::Cancel)
            .await;
        assert!(matches!(outcome, AgentOutcome::NothingToCancel));
    }

    #[tokio::test]
    async fn concurrent_requests_are_not_crossed() {
        let (action_tx, mut action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
        let (reply_tx, reply_rx) = tokio::sync::mpsc::unbounded_channel();
        let gateway = test_gateway(action_tx);
        spawn_drain(gateway.clone(), reply_rx);

        // Encode each request's id into its reply so crossing is detectable.
        tokio::spawn(async move {
            while let Some(Action::Agent { request_id, .. }) = action_rx.recv().await {
                let _ = reply_tx.send((
                    request_id,
                    AgentOutcome::MetroAlready {
                        port: request_id.0 as u16,
                    },
                ));
            }
        });

        let g1 = gateway.clone();
        let g2 = gateway.clone();
        let (a, b) = tokio::join!(
            g1.request("/tmp/a".into(), AgentRequest::StartMetro),
            g2.request("/tmp/b".into(), AgentRequest::StartMetro),
        );
        let pa = match a {
            AgentOutcome::MetroAlready { port } => port,
            other => panic!("expected MetroAlready, got {other:?}"),
        };
        let pb = match b {
            AgentOutcome::MetroAlready { port } => port,
            other => panic!("expected MetroAlready, got {other:?}"),
        };
        assert_ne!(pa, pb, "concurrent requests must receive distinct replies");
    }

    #[tokio::test]
    async fn request_errors_when_event_loop_is_gone() {
        let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
        let gateway = test_gateway(action_tx);
        // Drop the receiver — the dashboard loop is "not running".
        drop(action_rx);

        let outcome = gateway
            .request("/tmp/wt".into(), AgentRequest::GetTaskStatus)
            .await;
        assert!(matches!(outcome, AgentOutcome::Error { .. }));
    }
}
