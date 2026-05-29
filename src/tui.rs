//! Terminal lifecycle helpers.
//!
//! Canonical startup sequence (enforced in main.rs):
//!   1. color_eyre::install()    — install pretty panic hook first
//!   2. custom panic hook        — chains ratatui::restore() before original hook
//!   3. setup_logging()          — file appender, never stdout
//!   4. ratatui::init()          — raw mode + alternate screen
//!   5. app::run(terminal).await — event loop
//!   6. ratatui::restore()       — unconditional restore (Ok or Err path)
//!
//! Any deviation from this order causes hook chaining bugs or terminal corruption.

/// Sets up non-blocking file logging. Must be called before ratatui::init().
/// Returns the WorkerGuard — must be held alive for the duration of the program.
pub fn setup_logging() -> color_eyre::Result<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = dirs_or_home_config();
    let file_appender = tracing_appender::rolling::daily(log_dir, "ump-dash.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::DEBUG.into()),
        )
        .init();
    Ok(guard)
}

fn dirs_or_home_config() -> std::path::PathBuf {
    // Log to ~/.config/ump-dash/logs/ — create if missing
    let base = std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));
    let dir = base.join(".config").join("ump-dash").join("logs");
    let _ = std::fs::create_dir_all(&dir);
    dir
}
