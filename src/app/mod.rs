//! App layer — TEA event loop, state mutation, effect interpretation.
//!
//! Plan 13-08: hexagonal injection (F-202) complete. The `Adapters` struct in
//! `adapters.rs` holds trait objects for every infra port; `EffectRunner`
//! dispatches every `Effect` variant via `self.adapters.<port>.<method>()`.
//! After this plan, `src/app/` contains zero direct `infra::*`
//! references except the three F-111-deferred persistence sites in
//! `effect_runner.rs` (whitelisted in `make arch-lint`'s G-01 guard).
#![allow(dead_code)]

pub mod adapters;
pub mod effect;
pub mod effect_runner;
pub mod handle_key;
pub mod keybindings;
pub mod runtime;
pub mod state;
pub mod update;

// Re-exports — keeps ump_dash::app::* paths stable for tests, ui/, and main.rs
pub use adapters::Adapters;
pub use handle_key::handle_key;
pub use runtime::run;
pub use state::{AppState, ErrorState, PaletteMode, active_worktree_id};
pub use update::update;

#[cfg(test)]
pub mod dispatch_tests;
