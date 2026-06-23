//! Data dependency model: maps completed commands to required refreshes.
//!
//! `refresh_needed()` is a pure domain function — no I/O, no side effects.
//! The app layer calls it after a command exits to determine which
//! background refresh tasks to dispatch.

use super::command::CommandSpec;

/// Which background refreshes a completed command requires.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshSet {
    /// Re-enumerate worktrees (branch may have changed).
    pub worktrees: bool,
    /// Re-check staleness of node_modules / pods.
    pub staleness: bool,
    /// Re-fetch JIRA titles (branch names may have changed).
    pub jira_titles: bool,
}

impl RefreshSet {
    /// No refreshes needed.
    pub fn none() -> Self {
        Self {
            worktrees: false,
            staleness: false,
            jira_titles: false,
        }
    }

    /// Returns true if any refresh is needed.
    #[allow(dead_code)]
    pub fn any(&self) -> bool {
        self.worktrees || self.staleness || self.jira_titles
    }
}

/// Determines which refreshes are needed after a command completes.
///
/// Single source of truth for the command-to-refresh mapping.
/// The CommandExited handler calls this instead of scattering refresh
/// logic across individual match arms.
pub fn refresh_needed(cmd: &CommandSpec) -> RefreshSet {
    match cmd {
        // Branch-changing git ops -> full reload + JIRA re-fetch
        CommandSpec::GitCheckout { .. }
        | CommandSpec::GitCheckoutNew { .. }
        | CommandSpec::GitRebase { .. }
        | CommandSpec::GitResetHard
        | CommandSpec::GitResetHardFetch => RefreshSet {
            worktrees: true,
            staleness: true,
            jira_titles: true,
        },
        // Non-branch-changing git ops -> no refresh
        CommandSpec::GitPull | CommandSpec::GitPush | CommandSpec::GitFetch => RefreshSet::none(),
        // Install/clean -> staleness refresh only
        CommandSpec::YarnInstall
        | CommandSpec::YarnPodInstall
        | CommandSpec::RmNodeModules
        | CommandSpec::RnCleanAndroid
        | CommandSpec::RnCleanCocoapods => RefreshSet {
            worktrees: false,
            staleness: true,
            jira_titles: false,
        },
        // Everything else -> no refresh
        _ => RefreshSet::none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn full_refresh() -> RefreshSet {
        RefreshSet {
            worktrees: true,
            staleness: true,
            jira_titles: true,
        }
    }

    fn staleness_only() -> RefreshSet {
        RefreshSet {
            worktrees: false,
            staleness: true,
            jira_titles: false,
        }
    }

    // --- Branch-changing git ops -> full refresh ---

    #[test]
    fn git_reset_hard_triggers_full_refresh() {
        assert_eq!(refresh_needed(&CommandSpec::GitResetHard), full_refresh());
    }

    #[test]
    fn git_checkout_triggers_full_refresh() {
        assert_eq!(
            refresh_needed(&CommandSpec::GitCheckout {
                branch: "main".into()
            }),
            full_refresh()
        );
    }

    #[test]
    fn git_checkout_new_triggers_full_refresh() {
        assert_eq!(
            refresh_needed(&CommandSpec::GitCheckoutNew {
                branch: "feat".into()
            }),
            full_refresh()
        );
    }

    #[test]
    fn git_rebase_triggers_full_refresh() {
        assert_eq!(
            refresh_needed(&CommandSpec::GitRebase {
                target: "main".into()
            }),
            full_refresh()
        );
    }

    #[test]
    fn git_reset_hard_fetch_triggers_full_refresh() {
        assert_eq!(
            refresh_needed(&CommandSpec::GitResetHardFetch),
            full_refresh()
        );
    }

    // --- Non-branch-changing git ops -> no refresh ---

    #[test]
    fn git_pull_triggers_no_refresh() {
        assert_eq!(refresh_needed(&CommandSpec::GitPull), RefreshSet::none());
    }

    #[test]
    fn git_push_triggers_no_refresh() {
        assert_eq!(refresh_needed(&CommandSpec::GitPush), RefreshSet::none());
    }

    #[test]
    fn git_fetch_triggers_no_refresh() {
        assert_eq!(refresh_needed(&CommandSpec::GitFetch), RefreshSet::none());
    }

    // --- Install/clean -> staleness only ---

    #[test]
    fn yarn_install_triggers_staleness_only() {
        assert_eq!(refresh_needed(&CommandSpec::YarnInstall), staleness_only());
    }

    #[test]
    fn yarn_pod_install_triggers_staleness_only() {
        assert_eq!(
            refresh_needed(&CommandSpec::YarnPodInstall),
            staleness_only()
        );
    }

    #[test]
    fn rm_node_modules_triggers_staleness_only() {
        assert_eq!(
            refresh_needed(&CommandSpec::RmNodeModules),
            staleness_only()
        );
    }

    #[test]
    fn rn_clean_android_triggers_staleness_only() {
        assert_eq!(
            refresh_needed(&CommandSpec::RnCleanAndroid),
            staleness_only()
        );
    }

    #[test]
    fn rn_clean_cocoapods_triggers_staleness_only() {
        assert_eq!(
            refresh_needed(&CommandSpec::RnCleanCocoapods),
            staleness_only()
        );
    }

    // --- Everything else -> no refresh ---

    #[test]
    fn yarn_lint_triggers_no_refresh() {
        assert_eq!(refresh_needed(&CommandSpec::YarnLint), RefreshSet::none());
    }

    #[test]
    fn shell_command_triggers_no_refresh() {
        assert_eq!(
            refresh_needed(&CommandSpec::ShellCommand {
                command: "echo hi".into()
            }),
            RefreshSet::none()
        );
    }

    #[test]
    fn yarn_unit_tests_triggers_no_refresh() {
        assert_eq!(
            refresh_needed(&CommandSpec::YarnUnitTests),
            RefreshSet::none()
        );
    }

    #[test]
    fn yarn_check_types_triggers_no_refresh() {
        assert_eq!(
            refresh_needed(&CommandSpec::YarnCheckTypes),
            RefreshSet::none()
        );
    }

    // --- RefreshSet helpers ---

    #[test]
    fn none_has_no_flags() {
        let r = RefreshSet::none();
        assert!(!r.any());
    }

    #[test]
    fn any_returns_true_when_worktrees_set() {
        let r = RefreshSet {
            worktrees: true,
            staleness: false,
            jira_titles: false,
        };
        assert!(r.any());
    }

    #[test]
    fn any_returns_true_when_staleness_set() {
        let r = staleness_only();
        assert!(r.any());
    }
}
