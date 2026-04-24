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

// ==== Types filled in the feat commit ====

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
