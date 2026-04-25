//! App-tier effect grammar (F-201 type half — Plan 13-03; Plan 13-08 expansion).
//!
//! `Effect` is plain data — every variant is constructable without closures or
//! tokio handles. Plan 13-07 rewrote update() to return `Vec<Effect>`; Plan
//! 13-08 routes every variant through trait objects on the `Adapters` struct
//! (no more direct `infra::*` calls in `effect_runner`).
//!
//! Plan 13-08: variants that need repo_root context now carry it explicitly.
//! Pre-13-08 effect_runner.rs grabbed `std::env::current_dir()` as a fallback;
//! after 13-08 the caller (update()) supplies the right path from
//! `state.repo_root`.

#![allow(dead_code)]

use crate::domain::command::CommandSpec;
use crate::domain::ports::device_port::DeviceKind;
use std::collections::HashMap;
use std::path::PathBuf;

/// App-tier effect grammar — describes every side-effecting operation that
/// update() can dispatch.
#[derive(Debug)]
pub enum Effect {
    // Metro lifecycle
    DetectExternalMetro { port: u16 },
    SpawnMetro { worktree: PathBuf },
    MetroHttpPost { url: String, body: String },
    KillProcess { pid: u32 },

    // Commands
    SpawnCommand {
        spec: CommandSpec,
        cwd: PathBuf,
        branch: String,
    },
    LoadDevices { kind: DeviceKind },

    // Worktrees — Plan 13-08: variants now carry repo_root from update().
    ListWorktrees { repo_root: PathBuf },
    RemoveWorktree { repo_root: PathBuf, path: PathBuf },
    AddWorktree { repo_root: PathBuf, branch: String },
    AddWorktreeNewBranch { repo_root: PathBuf, new: String, base: String },
    ListRemoteBranches { repo_root: PathBuf },

    // Persistence (spawn_blocking sites — F-111 PersistencePort deferred)
    SaveJiraCache(HashMap<String, String>),
    SaveAndroidMode(String),
    RecordSimUsed(String),

    // External processes
    OpenInMultiplexer {
        worktree: PathBuf,
        name: String,
        command: String,
    },

    // JIRA
    FetchJiraTitles { keys: Vec<String> },

    // Recursive self-dispatch
    ScheduleAction(crate::domain::action::Action),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_variants_compile() {
        let _ = Effect::DetectExternalMetro { port: 8081 };
        let _ = Effect::ListWorktrees { repo_root: PathBuf::from(".") };
        let _ = Effect::ListRemoteBranches { repo_root: PathBuf::from(".") };
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
        fn variant_index(e: &Effect) -> u32 {
            match e {
                Effect::DetectExternalMetro { .. } => 0,
                Effect::SpawnMetro { .. } => 1,
                Effect::MetroHttpPost { .. } => 2,
                Effect::KillProcess { .. } => 3,
                Effect::SpawnCommand { .. } => 4,
                Effect::LoadDevices { .. } => 5,
                Effect::ListWorktrees { .. } => 6,
                Effect::RemoveWorktree { .. } => 7,
                Effect::AddWorktree { .. } => 8,
                Effect::AddWorktreeNewBranch { .. } => 9,
                Effect::ListRemoteBranches { .. } => 10,
                Effect::SaveJiraCache(_) => 11,
                Effect::SaveAndroidMode(_) => 12,
                Effect::RecordSimUsed(_) => 13,
                Effect::OpenInMultiplexer { .. } => 14,
                Effect::FetchJiraTitles { .. } => 15,
                Effect::ScheduleAction(_) => 16,
            }
        }
        let e = Effect::ListWorktrees { repo_root: PathBuf::from(".") };
        assert_eq!(variant_index(&e), 6);
    }
}
