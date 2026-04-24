//! App-tier effect grammar (F-201 type half — Plan 13-03).
//!
//! `Effect` is plain data — every variant is constructable without closures or
//! tokio handles. Plan 13-07 rewrites update() to return `Vec<Effect>`; the
//! eventual effect_runner.rs interprets them into tokio::spawn calls at a
//! single boundary.
//!
//! This module is TYPE-DEFINITION only — it has no consumers yet.

#![allow(dead_code)]

use crate::domain::command::CommandSpec;
use std::collections::HashMap;
use std::path::PathBuf;

/// Device family enum for the LoadDevices effect.
///
/// Stub — Plan 13-04 moves this to `crate::domain::ports::device_port::DeviceKind`.
/// When 13-04 lands, delete this definition and replace with an import.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DeviceKind {
    Android,
    Ios,
}

/// App-tier effect grammar — describes every side-effecting operation that
/// update() can dispatch. Each variant maps to one or more current
/// `tokio::spawn` call sites (see AUDIT.md F-201, lines 379-420).
///
/// Invariant: every variant is plain data. No closures, no `Box<dyn Fn>`,
/// no tokio handles. The effect_runner interprets variants into spawns.
#[derive(Debug)]
pub enum Effect {
    // Metro lifecycle
    /// Replaces the detect-external spawn at app.rs:602.
    DetectExternalMetro { port: u16 },
    /// Replaces the spawn_metro_task call at app.rs:619.
    SpawnMetro { worktree: PathBuf },
    /// Replaces the http_post calls at app.rs:636, 649 (reload, open debugger).
    MetroHttpPost { url: String, body: String },
    /// Replaces the kill-external-metro spawn at app.rs:709.
    KillProcess { pid: u32 },

    // Commands
    /// Replaces the command spawn at app.rs:524 (dispatch_command helper body).
    SpawnCommand {
        spec: CommandSpec,
        cwd: PathBuf,
        branch: String,
    },
    /// Replaces the device-load spawn at app.rs:929.
    LoadDevices { kind: DeviceKind },

    // Worktrees
    /// Replaces the list-worktrees spawns at app.rs:817, 993, 1863, 1903, 2042, 2107.
    ListWorktrees,
    /// Replaces the remove-worktree spawn at app.rs:1101.
    RemoveWorktree { path: PathBuf },
    /// Replaces the add-worktree spawn at app.rs:1205.
    AddWorktree { branch: String },
    /// Replaces the add-worktree-new-branch spawn at app.rs:1186.
    AddWorktreeNewBranch { new: String, base: String },
    /// Replaces the list-remote-branches spawn at app.rs:1928.
    ListRemoteBranches,

    // Persistence (spawn_blocking sites)
    /// Replaces the save-jira-cache spawn at app.rs:1564.
    SaveJiraCache(HashMap<String, String>),
    /// Replaces the save-android-mode spawns at app.rs:1170, 1339, 1362, 1392, 1413.
    SaveAndroidMode(String),
    /// Replaces the record-sim-used spawn at app.rs:1678.
    RecordSimUsed(String),

    // External processes
    /// Replaces the open-in-multiplexer spawns at app.rs:1236, 1548.
    OpenInMultiplexer {
        worktree: PathBuf,
        name: String,
        command: String,
    },

    // JIRA
    /// Replaces the fetch-jira-titles spawns at app.rs:708, 794.
    FetchJiraTitles { keys: Vec<String> },

    // Recursive self-dispatch (absorbs F-206 — the 7+ `update(state, next_action, ..)`
    // recursive call sites collapse into Effect::ScheduleAction post-F-201).
    ScheduleAction(crate::domain::action::Action),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_variants_compile() {
        // Compilation-only: ensures every variant has a valid data shape.
        // A handful of representative variants are exercised; the full set is
        // implicitly validated by the match-arm shape below.
        let _ = Effect::DetectExternalMetro { port: 8081 };
        let _ = Effect::ListWorktrees;
        let _ = Effect::ListRemoteBranches;
        let _ = Effect::SaveAndroidMode("release".into());
        let _ = Effect::KillProcess { pid: 999 };
        let _ = Effect::LoadDevices {
            kind: DeviceKind::Android,
        };
        let _ = Effect::MetroHttpPost {
            url: "http://localhost:8081/reload".into(),
            body: "{}".into(),
        };
        let _ = Effect::FetchJiraTitles {
            keys: vec!["ABC-1".into()],
        };
    }

    #[test]
    fn effect_has_at_least_fifteen_variants() {
        // G-09 shape guard: ensure the enum has ≥15 variants by exhaustively
        // matching — if a variant is added/removed the compiler forces an
        // update here, and the branch count is visibly ≥15.
        fn variant_index(e: &Effect) -> u32 {
            match e {
                Effect::DetectExternalMetro { .. } => 0,
                Effect::SpawnMetro { .. } => 1,
                Effect::MetroHttpPost { .. } => 2,
                Effect::KillProcess { .. } => 3,
                Effect::SpawnCommand { .. } => 4,
                Effect::LoadDevices { .. } => 5,
                Effect::ListWorktrees => 6,
                Effect::RemoveWorktree { .. } => 7,
                Effect::AddWorktree { .. } => 8,
                Effect::AddWorktreeNewBranch { .. } => 9,
                Effect::ListRemoteBranches => 10,
                Effect::SaveJiraCache(_) => 11,
                Effect::SaveAndroidMode(_) => 12,
                Effect::RecordSimUsed(_) => 13,
                Effect::OpenInMultiplexer { .. } => 14,
                Effect::FetchJiraTitles { .. } => 15,
                Effect::ScheduleAction(_) => 16,
            }
        }
        let e = Effect::ListWorktrees;
        assert_eq!(variant_index(&e), 6);
    }
}
