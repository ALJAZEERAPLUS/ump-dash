//! ump-dash library crate root. Mirrors the module layout of `main.rs` so that
//! integration tests (tests/*.rs) can `use ump_dash::domain::...` etc.
//!
//! The binary entrypoint (`src/main.rs`) re-exports via `use ump_dash::*` and
//! delegates its async-main body here so behavior is unchanged.
#![allow(dead_code)]

pub mod app;
pub mod domain;
pub mod event;
pub mod infra;
pub mod tui;
pub mod ui;
