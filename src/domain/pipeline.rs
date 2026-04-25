//! Domain-level command orchestration (F-204 + REFACTOR-03, Plan 13-03).
//!
//! Pure domain types. No I/O, no tokio. The dispatcher reads `Recipe` and
//! trusts `Recipe::expand` to produce the correct `CommandSpec` sequence —
//! it never inlines prerequisite logic.
//!
//! Exemplary pattern: `src/domain/refresh.rs` (pure fn + inline tests).
//!
//! This module is TYPE-DEFINITION only — it has no consumers yet. Plan 13-09
//! rewires the 11 inline prereq sites in `update()` to construct a `Recipe`
//! and call `expand()`.

#![allow(dead_code)]

use crate::domain::command::{CleanOptions, CommandSpec};

/// Precondition that a `CommandSpec` requires before it can run.
///
/// Replaces the inline `needs_metro()` / `spec.needs_metro()` checks scattered
/// across `update()` at app.rs:890, 1014, 1713 (see AUDIT F-204 line list).
#[derive(Debug, Clone, PartialEq)]
pub enum Prerequisite {
    /// Metro bundler must be running before this command can dispatch.
    MetroRunning,
    /// Worktree dependencies must be fresh — `yarn` if true, `pod install` if true.
    /// Dispatcher inspects these flags to decide whether to front-load a sync step.
    DependenciesFresh { yarn: bool, pods: bool },
}

impl CommandSpec {
    /// Derive prerequisites from the variant. Replaces the per-site `needs_metro()`
    /// inline checks — callers should prefer `prerequisites()` going forward.
    ///
    /// `needs_metro()` stays as a thin wrapper for backward compatibility.
    pub fn prerequisites(&self) -> Vec<Prerequisite> {
        match self {
            CommandSpec::RnRunAndroid { .. }
            | CommandSpec::RnRunIos { .. }
            | CommandSpec::RnRunIosDevice
            | CommandSpec::RnReleaseBuild => vec![Prerequisite::MetroRunning],
            // Sync prerequisites come from Recipe::SyncThenRun — not per-variant.
            _ => vec![],
        }
    }
}

/// A single-command or multi-step dispatch unit. The dispatcher reads `Recipe`
/// and calls `expand()`; it NEVER branches on inline boolean flags.
///
/// Replaces the 11 inline prereq sites in `update()` enumerated by AUDIT F-204.
#[derive(Debug, Clone)]
pub enum Recipe {
    /// Dispatch one command, nothing before or after.
    Single(CommandSpec),
    /// Dispatch a fixed ordered list.
    Sequence(Vec<CommandSpec>),
    /// Clean palette expansion — honours CleanOptions toggles + optional sync_after.
    Clean(CleanOptions),
    /// Front-load yarn/pod sync steps if stale, then run the given command.
    /// iOS-only for pods per F-204 rule.
    SyncThenRun(CommandSpec),
    /// Front-load yarn/pod sync steps if stale, then start metro.
    /// Pods included when stale regardless of target (metro is platform-agnostic).
    SyncThenStartMetro,
    /// `RnReleaseBuild` followed by `AdbInstallApk`.
    ReleaseBuildAndInstall,
    /// `GitFetch` followed by `GitResetHard`.
    GitFetchThenReset,
}

/// Staleness snapshot used by `Recipe::expand` to decide which sync commands
/// front-load the dispatched sequence.
///
/// Copy-by-value — tiny struct, no heap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DependencyState {
    /// `yarn.lock` is newer than `node_modules/.yarn-integrity` (or missing).
    pub stale_yarn: bool,
    /// `Podfile.lock` is newer than `ios/Pods/Manifest.lock` (or missing).
    pub stale_pods: bool,
    /// True if the target about to run is iOS (affects whether pod install runs).
    pub is_ios_target: bool,
}

impl DependencyState {
    /// Convenience constructor for the call sites in `src/app/update.rs`.
    /// The data projection from `AppState` is small but repeated — this keeps
    /// the call-site noise low. Pure data — no I/O, callable from tests.
    pub fn new(stale_yarn: bool, stale_pods: bool, is_ios_target: bool) -> Self {
        Self { stale_yarn, stale_pods, is_ios_target }
    }
}

impl Recipe {
    /// Expand the recipe into a linear sequence of `CommandSpec` — the dispatcher
    /// calls this once per recipe and enqueues every item in order.
    ///
    /// Pure — no I/O, no tokio. Testable without a runtime.
    pub fn expand(&self, deps: &DependencyState) -> Vec<CommandSpec> {
        match self {
            Recipe::Single(cmd) => vec![cmd.clone()],
            Recipe::Sequence(cmds) => cmds.clone(),
            Recipe::Clean(opts) => {
                let mut v = Vec::new();
                if opts.pods {
                    v.push(CommandSpec::RnCleanCocoapods);
                }
                if opts.android {
                    v.push(CommandSpec::RnCleanAndroid);
                }
                if opts.node_modules {
                    v.push(CommandSpec::RmNodeModules);
                }
                if opts.sync_after {
                    v.push(CommandSpec::YarnInstall);
                    v.push(CommandSpec::YarnPodInstall);
                }
                v
            }
            Recipe::SyncThenRun(cmd) => {
                let mut v = Vec::new();
                if deps.stale_yarn {
                    v.push(CommandSpec::YarnInstall);
                }
                if deps.stale_pods && deps.is_ios_target {
                    v.push(CommandSpec::YarnPodInstall);
                }
                v.push(cmd.clone());
                v
            }
            Recipe::SyncThenStartMetro => {
                let mut v = Vec::new();
                if deps.stale_yarn {
                    v.push(CommandSpec::YarnInstall);
                }
                if deps.stale_pods {
                    v.push(CommandSpec::YarnPodInstall);
                }
                v // dispatcher follows with MetroStart
            }
            Recipe::ReleaseBuildAndInstall => {
                vec![CommandSpec::RnReleaseBuild, CommandSpec::AdbInstallApk]
            }
            Recipe::GitFetchThenReset => {
                vec![CommandSpec::GitFetch, CommandSpec::GitResetHard]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_deps() -> DependencyState {
        DependencyState {
            stale_yarn: false,
            stale_pods: false,
            is_ios_target: false,
        }
    }

    fn stale_deps_ios() -> DependencyState {
        DependencyState {
            stale_yarn: true,
            stale_pods: true,
            is_ios_target: true,
        }
    }

    fn stale_deps_android() -> DependencyState {
        DependencyState {
            stale_yarn: true,
            stale_pods: true,
            is_ios_target: false,
        }
    }

    #[test]
    fn test_single_expands_to_one_spec() {
        assert_eq!(
            Recipe::Single(CommandSpec::YarnInstall).expand(&fresh_deps()),
            vec![CommandSpec::YarnInstall]
        );
    }

    #[test]
    fn test_sequence_preserves_order() {
        let recipe = Recipe::Sequence(vec![
            CommandSpec::GitFetch,
            CommandSpec::GitResetHard,
        ]);
        assert_eq!(
            recipe.expand(&fresh_deps()),
            vec![CommandSpec::GitFetch, CommandSpec::GitResetHard]
        );
    }

    #[test]
    fn test_clean_all_options_expands_to_four_plus_sync() {
        let opts = CleanOptions {
            pods: true,
            android: true,
            node_modules: true,
            sync_after: true,
        };
        assert_eq!(
            Recipe::Clean(opts).expand(&fresh_deps()),
            vec![
                CommandSpec::RnCleanCocoapods,
                CommandSpec::RnCleanAndroid,
                CommandSpec::RmNodeModules,
                CommandSpec::YarnInstall,
                CommandSpec::YarnPodInstall,
            ]
        );
    }

    #[test]
    fn test_clean_none_expands_to_empty() {
        assert_eq!(
            Recipe::Clean(CleanOptions::default()).expand(&fresh_deps()),
            Vec::<CommandSpec>::new()
        );
    }

    #[test]
    fn test_sync_then_run_stale_ios_adds_yarn_and_pods() {
        let run_cmd = CommandSpec::RnRunIos {
            device_id: "udid-1".into(),
        };
        assert_eq!(
            Recipe::SyncThenRun(run_cmd.clone()).expand(&stale_deps_ios()),
            vec![CommandSpec::YarnInstall, CommandSpec::YarnPodInstall, run_cmd]
        );
    }

    #[test]
    fn test_sync_then_run_stale_android_only_yarn() {
        // Pods are skipped on Android (is_ios_target = false) — F-204 rule.
        let run_cmd = CommandSpec::RnRunAndroid {
            device_id: "emulator-5554".into(),
            mode: None,
        };
        assert_eq!(
            Recipe::SyncThenRun(run_cmd.clone()).expand(&stale_deps_android()),
            vec![CommandSpec::YarnInstall, run_cmd]
        );
    }

    #[test]
    fn test_sync_then_run_fresh_passes_through() {
        let run_cmd = CommandSpec::YarnLint;
        assert_eq!(
            Recipe::SyncThenRun(run_cmd.clone()).expand(&fresh_deps()),
            vec![run_cmd]
        );
    }

    #[test]
    fn test_sync_then_start_metro_stale_adds_both() {
        // Metro start path: pods always included when stale, regardless of target.
        assert_eq!(
            Recipe::SyncThenStartMetro.expand(&stale_deps_android()),
            vec![CommandSpec::YarnInstall, CommandSpec::YarnPodInstall]
        );
    }

    #[test]
    fn test_sync_then_start_metro_fresh_is_empty() {
        assert_eq!(
            Recipe::SyncThenStartMetro.expand(&fresh_deps()),
            Vec::<CommandSpec>::new()
        );
    }

    #[test]
    fn test_release_build_and_install_expands_to_two() {
        assert_eq!(
            Recipe::ReleaseBuildAndInstall.expand(&fresh_deps()),
            vec![CommandSpec::RnReleaseBuild, CommandSpec::AdbInstallApk]
        );
    }

    #[test]
    fn test_git_fetch_then_reset_expands_to_two() {
        assert_eq!(
            Recipe::GitFetchThenReset.expand(&fresh_deps()),
            vec![CommandSpec::GitFetch, CommandSpec::GitResetHard]
        );
    }

    #[test]
    fn test_prerequisites_rn_run_android_needs_metro() {
        let spec = CommandSpec::RnRunAndroid {
            device_id: "emulator-5554".into(),
            mode: None,
        };
        assert_eq!(spec.prerequisites(), vec![Prerequisite::MetroRunning]);
    }

    #[test]
    fn test_prerequisites_rn_run_ios_needs_metro() {
        let spec = CommandSpec::RnRunIos {
            device_id: "udid-1".into(),
        };
        assert_eq!(spec.prerequisites(), vec![Prerequisite::MetroRunning]);
    }

    #[test]
    fn test_prerequisites_rn_release_build_needs_metro() {
        assert_eq!(
            CommandSpec::RnReleaseBuild.prerequisites(),
            vec![Prerequisite::MetroRunning]
        );
    }

    #[test]
    fn test_prerequisites_yarn_install_no_prereq() {
        assert_eq!(CommandSpec::YarnInstall.prerequisites(), Vec::<Prerequisite>::new());
    }

    #[test]
    fn test_prerequisites_git_fetch_no_prereq() {
        assert_eq!(CommandSpec::GitFetch.prerequisites(), Vec::<Prerequisite>::new());
    }
}
