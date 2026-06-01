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
use crate::domain::native_cache::CachedIosLaunchRequest;
use crate::domain::ports::device_port::DeviceKind;
use std::collections::HashMap;
use std::path::PathBuf;

/// App-tier effect grammar — describes every side-effecting operation that
/// update() can dispatch.
#[derive(Debug)]
pub enum Effect {
    // Metro lifecycle
    DetectExternalMetro {
        port: u16,
    },
    SpawnMetro {
        worktree: PathBuf,
        port: u16,
    },
    MetroHttpPost {
        url: String,
        body: String,
    },
    KillProcess {
        pid: u32,
    },

    // Commands
    /// Phase 14 / D-10, D-20, Q1: single chokepoint for spawning per-worktree tasks.
    ///
    /// Payload includes `cwd` and `branch` (Q1 lock — RESEARCH §Open Q1 +
    /// §Pitfall P-7) so the runner does not need to look them up against state.
    ///
    /// Plan 15-03 / TASK-06: `repo_root` is the canonicalization key for the
    /// yarn install semaphore. `effect_runner` canonicalizes it and uses it
    /// to look up (or create) the per-repo-root `Semaphore(1)` that serializes
    /// `YarnInstall` / `YarnPodInstall` / `RmNodeModules` across worktrees
    /// that share the same upstream repo. Non-yarn specs ignore the field.
    SpawnTask {
        task_id: crate::domain::task::TaskId,
        worktree_id: crate::domain::worktree::WorktreeId,
        spec: CommandSpec,
        cwd: std::path::PathBuf,
        branch: String,
        repo_root: std::path::PathBuf,
    },
    LoadDevices {
        kind: DeviceKind,
        request_id: Option<u64>,
    },
    LookupIosSimulatorCache {
        worktree_id: crate::domain::worktree::WorktreeId,
        worktree_path: PathBuf,
    },
    InstallAndLaunchCachedIosSimulator {
        worktree_id: crate::domain::worktree::WorktreeId,
        request: CachedIosLaunchRequest,
    },

    // Worktrees — Plan 13-08: variants now carry repo_root from update().
    ListWorktrees {
        repo_root: PathBuf,
    },
    RemoveWorktree {
        repo_root: PathBuf,
        path: PathBuf,
    },
    AddWorktree {
        repo_root: PathBuf,
        branch: String,
    },
    AddWorktreeNewBranch {
        repo_root: PathBuf,
        new: String,
        base: String,
    },
    ListRemoteBranches {
        repo_root: PathBuf,
    },

    // Persistence (spawn_blocking sites — F-111 PersistencePort deferred)
    SaveJiraCache(HashMap<String, String>),
    RecordSimUsed(String),

    // External processes
    OpenInMultiplexer {
        worktree: PathBuf,
        name: String,
        command: String,
    },

    // JIRA
    FetchJiraTitles {
        keys: Vec<String>,
    },

    // Recursive self-dispatch
    ScheduleAction(crate::domain::action::Action),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_task_variant_constructs_and_matches() {
        let eff = Effect::SpawnTask {
            task_id: crate::domain::task::TaskId(42),
            worktree_id: crate::domain::worktree::WorktreeId("wt-test".into()),
            spec: crate::domain::command::CommandSpec::YarnInstall,
            cwd: std::path::PathBuf::from("/tmp/wt-test"),
            branch: "main".into(),
            repo_root: std::path::PathBuf::from("/tmp/repo-root-test"),
        };
        match eff {
            Effect::SpawnTask {
                task_id,
                worktree_id,
                repo_root,
                ..
            } => {
                assert_eq!(task_id, crate::domain::task::TaskId(42));
                assert_eq!(
                    worktree_id,
                    crate::domain::worktree::WorktreeId("wt-test".into())
                );
                assert_eq!(repo_root, std::path::PathBuf::from("/tmp/repo-root-test"));
            }
            _ => panic!("expected SpawnTask"),
        }
    }

    /// Plan 15-03 / TASK-06: `repo_root` is the key the runner uses to look up
    /// the per-repo-root yarn install semaphore. This test pins the field's
    /// presence and round-trip identity — a refactor that drops the field or
    /// renames it will break this characterization.
    #[test]
    fn spawn_task_carries_repo_root_for_semaphore_key() {
        let key = std::path::PathBuf::from("/Users/test/repo-A");
        let eff = Effect::SpawnTask {
            task_id: crate::domain::task::TaskId(7),
            worktree_id: crate::domain::worktree::WorktreeId("wt-A-1".into()),
            spec: crate::domain::command::CommandSpec::YarnInstall,
            cwd: std::path::PathBuf::from("/Users/test/repo-A/wt-A-1"),
            branch: "feat".into(),
            repo_root: key.clone(),
        };
        match eff {
            Effect::SpawnTask { repo_root, .. } => {
                assert_eq!(
                    repo_root, key,
                    "repo_root must round-trip — it is the semaphore HashMap key"
                );
            }
            _ => panic!("expected SpawnTask"),
        }
    }

    #[test]
    fn effect_variants_compile() {
        let _ = Effect::DetectExternalMetro { port: 8081 };
        let worktree_id = crate::domain::worktree::WorktreeId("wt-a".into());
        let _ = Effect::LookupIosSimulatorCache {
            worktree_id: worktree_id.clone(),
            worktree_path: PathBuf::from("."),
        };
        let _ = Effect::InstallAndLaunchCachedIosSimulator {
            worktree_id,
            request: crate::domain::native_cache::CachedIosLaunchRequest {
                simulator_udid: "SIM-1".into(),
                app_path: PathBuf::from("build/app.app"),
                bundle_id: "com.aljazeera.test".into(),
                metro_port: 8081,
            },
        };
        let _ = Effect::ListWorktrees {
            repo_root: PathBuf::from("."),
        };
        let _ = Effect::ListRemoteBranches {
            repo_root: PathBuf::from("."),
        };
        let _ = Effect::KillProcess { pid: 999 };
        let _ = Effect::LoadDevices {
            kind: DeviceKind::Android,
            request_id: None,
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
        // Plan 14-09: SpawnCommand removed; SpawnTask is the sole spawn chokepoint.
        fn variant_index(e: &Effect) -> u32 {
            match e {
                Effect::DetectExternalMetro { .. } => 0,
                Effect::SpawnMetro { .. } => 1,
                Effect::MetroHttpPost { .. } => 2,
                Effect::KillProcess { .. } => 3,
                Effect::SpawnTask { .. } => 4,
                Effect::LoadDevices { .. } => 5,
                Effect::ListWorktrees { .. } => 6,
                Effect::RemoveWorktree { .. } => 7,
                Effect::AddWorktree { .. } => 8,
                Effect::AddWorktreeNewBranch { .. } => 9,
                Effect::ListRemoteBranches { .. } => 10,
                Effect::SaveJiraCache(_) => 11,
                Effect::RecordSimUsed(_) => 12,
                Effect::OpenInMultiplexer { .. } => 13,
                Effect::FetchJiraTitles { .. } => 14,
                Effect::ScheduleAction(_) => 15,
                Effect::LookupIosSimulatorCache { .. } => 16,
                Effect::InstallAndLaunchCachedIosSimulator { .. } => 17,
            }
        }
        let e = Effect::ListWorktrees {
            repo_root: PathBuf::from("."),
        };
        assert_eq!(variant_index(&e), 6);
    }
}
