//! App layer — TEA event loop, state mutation, effect interpretation.
//!
//! Split from a monolithic src/app.rs (2522 LOC) per F-200 in Plan 13-06. The
//! submodules have single-concern roles. Plans 13-07..13-10 apply the behavior-
//! changing refactors (F-201 TEA purity, F-202 hexagonal, F-203 metro extraction,
//! F-204 Recipe consumer, F-205 exhaustive modal arms, F-208 KEYBINDINGS, F-209
//! sub-struct grouping).
#![allow(dead_code)]

pub mod adapters;
pub mod effect;
pub mod effect_runner;
pub mod handle_key;
pub mod runtime;
pub mod state;
pub mod update;

// Re-exports — keeps rn_dash::app::* paths stable for tests and ui/
pub use handle_key::handle_key;
pub use runtime::run;
pub use state::{
    active_output, active_output_scroll, active_worktree_id, AppState, ErrorState,
    FocusedPanel, PaletteMode,
};
pub use update::update;

#[cfg(test)]
pub mod dispatch_tests;
