// src/main.rs — composition root (Plan 13-08).
//
// All adapter construction happens here, plus startup-time config / cache /
// preferences loading. The library crate (`ump_dash`) is now infra-free at the
// boundary `ump_dash::app::run(terminal, adapters, state)`.

use ump_dash::app::{run, Adapters, AppState};
use ump_dash::domain::ports::jira_port::JiraPort;
use std::sync::Arc;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    // Step 1: Install color-eyre hooks FIRST so ratatui's hook chains after it.
    color_eyre::install()?;

    // Step 2: Install panic hook that restores terminal before the panic
    // message prints. This must happen BEFORE ratatui::init().
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Ignore errors — we're already in a panic, best-effort restore.
        ratatui::restore();
        original_hook(panic_info);
    }));

    // Step 3: Set up file-based logging — NEVER use println!/eprintln! in a
    // TUI (corrupts rendering).
    let _log_guard = ump_dash::tui::setup_logging()?;
    tracing::info!("ump-dash starting");

    // Step 4: Load config + persisted preferences (synchronous I/O at startup).
    let config = match ump_dash::infra::config::load_config() {
        Ok(config) => config,
        Err(e) => {
            tracing::warn!("config load failed: {e}");
            None
        }
    };
    let jira_title_cache = ump_dash::infra::jira_cache::load_jira_cache().unwrap_or_default();
    let sim_history = ump_dash::infra::sim_history::load_sim_history();

    // Step 5: Construct the Adapters bundle. Concrete `crate::infra::*` types
    // are referenced ONLY here — `src/app/` stays infra-free (G-01).
    let jira_port: Option<Arc<dyn JiraPort>> = config.as_ref().and_then(|cfg| {
        match ump_dash::infra::jira::HttpJiraClient::new(cfg) {
            Ok(client) => Some(Arc::new(client) as Arc<dyn JiraPort>),
            Err(e) => {
                tracing::warn!("JIRA client init failed: {e}");
                None
            }
        }
    });
    let multiplexer_port = ump_dash::infra::multiplexer::detect_multiplexer().map(Arc::from);

    // Worktree seed-file list comes from config; fall back to the domain default
    // when no config is loaded so worktree creation still seeds sane defaults.
    let seed_files = config
        .as_ref()
        .map(|cfg| cfg.seed_files.clone())
        .unwrap_or_else(ump_dash::domain::dash_config::default_seed_files);

    let adapters = Adapters {
        command_runner: Arc::new(ump_dash::infra::command_runner::TokioCommandRunner),
        metro: Arc::new(ump_dash::infra::metro::TokioMetroAdapter::new()),
        port_probe: Arc::new(ump_dash::infra::port::LsofPortProbe),
        worktrees: Arc::new(ump_dash::infra::worktrees::GitWorktreeAdapter::new(seed_files)),
        devices: Arc::new(ump_dash::infra::devices::AdbXcrunDevices),
        jira: jira_port.clone(),
        multiplexer: multiplexer_port.clone(),
    };

    // Step 6: Build the pre-populated AppState. The flat fields here mirror
    // the pre-13-08 `runtime::run` startup logic, minus the adapter
    // construction (now in step 5). `field_reassign_with_default` would
    // suggest a struct-literal `..Default::default()` form, but AppState has
    // ~30 fields and only a handful change here — sequential assignment is
    // clearer at the composition root.
    let state = build_state(
        config,
        jira_title_cache,
        sim_history,
        jira_port.is_some(),
        multiplexer_port.is_some(),
    );


    // Step 7: Initialize terminal (raw mode + alternate screen) and hand off.
    let terminal = ratatui::init();
    let result = run(terminal, adapters, state).await;

    // Step 8: Restore terminal unconditionally — runs on both Ok and Err.
    ratatui::restore();

    tracing::info!(
        "ump-dash exiting: {:?}",
        result.as_ref().map(|_| "ok").unwrap_or("err")
    );

    result
}

#[allow(clippy::field_reassign_with_default)]
fn build_state(
    config: Option<ump_dash::domain::dash_config::DashConfig>,
    jira_title_cache: std::collections::HashMap<String, String>,
    sim_history: Vec<String>,
    jira_available: bool,
    multiplexer_available: bool,
) -> AppState {
    let mut state = AppState::default();
    state.jira.title_cache = jira_title_cache;
    state.app_config.sim_history = sim_history;
    state.jira.available = jira_available;
    state.app_config.multiplexer_available = multiplexer_available;
    if let Some(cfg) = config {
        state.app_config.claude_flags = cfg.claude_flags.clone();
        state.jira.project_prefix = cfg.jira_project_prefix.clone();
        if let Some(path) = cfg.repo_root_path() {
            state.app_config.repo_root = path;
        }
        state.app_config.config = Some(cfg);
    }
    state
}
