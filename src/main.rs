// src/main.rs — thin binary entrypoint. All module declarations now live in
// src/lib.rs so integration tests (tests/*.rs) can `use rn_dash::...`. Only
// the items actually referenced in `main()` are imported here to keep
// `cargo clippy -D warnings` clean.
use rn_dash::{app, tui};

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
    // Step 1: Install color-eyre hooks FIRST so ratatui's hook chains after it
    color_eyre::install()?;

    // Step 2: Install panic hook that restores terminal before the panic message prints
    // This must happen BEFORE ratatui::init() — init() docs: "ensure this is called after
    // your app installs any other panic hooks."
    let original_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        // Ignore errors — we're already in a panic, best-effort restore
        ratatui::restore();
        original_hook(panic_info);
    }));

    // Step 3: Set up file-based logging — NEVER use println!/eprintln! in a TUI (corrupts rendering)
    let _log_guard = tui::setup_logging()?;
    tracing::info!("rn-dash starting");

    // Step 4: Initialize terminal (enables raw mode + alternate screen)
    let terminal = ratatui::init();

    // Step 5: Run the application event loop
    let result = app::run(terminal).await;

    // Step 6: Restore terminal unconditionally — runs on both Ok and Err exit from run()
    ratatui::restore();

    tracing::info!("rn-dash exiting: {:?}", result.as_ref().map(|_| "ok").unwrap_or("err"));

    result
}
