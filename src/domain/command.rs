//! Command specification types for the command palette.
//!
//! `CommandSpec` describes *what* to run. The infrastructure layer converts it
//! to an actual process via `to_argv()`. No process spawning happens here.

/// All commands that can be dispatched from the git or RN command palettes.
/// 23 variants total. Pure data — no I/O.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandSpec {
    // Git commands (6 variants)
    GitResetHard,
    GitPull,
    GitPush,
    GitRebase { target: String },
    GitCheckout { branch: String },
    GitCheckoutNew { branch: String },

    // React Native clean commands (3 variants)
    RnCleanAndroid,
    RnCleanCocoapods,
    RmNodeModules,

    // Yarn commands (2 variants)
    YarnInstall,
    YarnPodInstall,

    // RN run commands (3 variants)
    RnRunAndroid { device_id: String, mode: Option<String> },
    RnRunIos { device_id: String },
    RnRunIosDevice,                     // i>d: run-ios --device (auto-selects first physical device)

    // Test/quality commands (4 variants)
    YarnUnitTests,
    YarnJest { filter: String },
    YarnLint,
    YarnCheckTypes,

    // Phase 05.1 additions (5 variants)
    GitFetch,                           // g>f: git fetch --all --tags
    GitResetHardFetch,                  // g>X: fetch first, then reset to origin/<branch>
    RnReleaseBuild,                     // a>r: gradlew assembleRelease
    AdbInstallApk,                      // a>r continued: adb install of built APK
    ShellCommand { command: String },   // !: run arbitrary shell command in worktree dir
}

impl CommandSpec {
    /// Returns the argv that should be passed to `tokio::process::Command`.
    /// The first element is the program; the rest are arguments.
    pub fn to_argv(&self) -> Vec<String> {
        match self {
            CommandSpec::GitResetHard => vec!["git".into(), "reset".into(), "--hard".into(), "HEAD".into()],
            CommandSpec::GitPull => vec!["git".into(), "pull".into()],
            CommandSpec::GitPush => vec!["git".into(), "push".into()],
            CommandSpec::GitRebase { target } => vec!["git".into(), "rebase".into(), target.clone()],
            CommandSpec::GitCheckout { branch } => vec!["git".into(), "checkout".into(), branch.clone()],
            CommandSpec::GitCheckoutNew { branch } => vec!["git".into(), "checkout".into(), "-b".into(), branch.clone()],

            CommandSpec::RnCleanAndroid => vec!["npx".into(), "react-native".into(), "clean".into(), "--include".into(), "android".into()],
            CommandSpec::RnCleanCocoapods => vec!["npx".into(), "react-native".into(), "clean".into(), "--include".into(), "cocoapods".into()],
            CommandSpec::RmNodeModules => vec!["rm".into(), "-rf".into(), "node_modules".into()],

            CommandSpec::YarnInstall => vec!["yarn".into(), "install".into()],
            CommandSpec::YarnPodInstall => vec!["yarn".into(), "pod-install".into()],

            CommandSpec::RnRunAndroid { device_id, mode } => {
                let mut argv = vec!["npx".into(), "react-native".into(), "run-android".into()];
                if let Some(m) = mode {
                    argv.push("--mode".into());
                    argv.push(m.clone());
                }
                if !device_id.is_empty() {
                    argv.push("--device".into());
                    argv.push(device_id.clone());
                }
                argv
            }
            CommandSpec::RnRunIos { device_id } => {
                vec!["yarn".into(), "react-native".into(), "run-ios".into(), "--udid".into(), device_id.clone()]
            }
            CommandSpec::RnRunIosDevice => {
                vec!["yarn".into(), "react-native".into(), "run-ios".into(), "--device".into()]
            }

            CommandSpec::YarnUnitTests => vec!["yarn".into(), "unit-tests".into()],
            CommandSpec::YarnJest { filter } => vec!["yarn".into(), "jest".into(), filter.clone()],
            CommandSpec::YarnLint => vec!["yarn".into(), "lint".into(), "--quiet".into(), "--fix".into()],
            CommandSpec::YarnCheckTypes => vec!["yarn".into(), "check-types".into(), "--incremental".into()],

            CommandSpec::GitFetch => vec!["git".into(), "fetch".into(), "--all".into(), "--tags".into()],
            CommandSpec::GitResetHardFetch => {
                // Two-step operation handled by command_runner — fetch then reset.
                // to_argv returns the fetch step; the runner handles chaining.
                vec!["git".into(), "fetch".into(), "--all".into(), "--tags".into()]
            },
            CommandSpec::RnReleaseBuild => {
                vec!["./android/gradlew".into(), "-p".into(), "android".into(), "assembleRelease".into()]
            },
            CommandSpec::AdbInstallApk => {
                vec!["adb".into(), "install".into(), "-r".into(), "android/app/build/outputs/apk/release/app-release.apk".into()]
            },
            CommandSpec::ShellCommand { command } => {
                vec!["sh".into(), "-c".into(), command.clone()]
            },
        }
    }

    /// Returns true for commands that cannot be undone and require explicit confirmation.
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            CommandSpec::GitResetHard
                | CommandSpec::GitResetHardFetch
                | CommandSpec::RnCleanAndroid
                | CommandSpec::RnCleanCocoapods
                | CommandSpec::RmNodeModules
        )
    }

    /// Returns false for git-porcelain commands (data-integrity risk on cancellation);
    /// true for all other commands (yarn, rn, rm, adb, shell).
    ///
    /// REFACTOR-02: Type-driven cancellability. Git variants are closed by construction —
    /// adding a new `Git*` variant requires explicit opt-in here (compile-error would be
    /// ideal; today this is a flat-enum predicate per AUDIT-ADDENDUM F-501 DEFERRED decision).
    pub fn is_cancellable(&self) -> bool {
        !matches!(
            self,
            CommandSpec::GitResetHard
                | CommandSpec::GitResetHardFetch
                | CommandSpec::GitPull
                | CommandSpec::GitPush
                | CommandSpec::GitRebase { .. }
                | CommandSpec::GitCheckout { .. }
                | CommandSpec::GitCheckoutNew { .. }
                | CommandSpec::GitFetch
        )
    }

    /// Returns true for commands that need a user-supplied text string before running.
    ///
    /// Plan 13-10 (F-006 Minor): the `_ => false` catch-all is intentional. New
    /// CommandSpec variants default to "no text input required" — which is the
    /// correct behavior unless they explicitly introduce a text-input
    /// requirement, at which point the maintainer adds an arm. Variant drift
    /// is additionally guarded by `is_cancellable`'s test fixture
    /// (every variant enumerated; new variants force a recompile + test
    /// review). Exhaustive conversion deferred to backlog per D-02.
    pub fn needs_text_input(&self) -> bool {
        match self {
            CommandSpec::GitRebase { .. }
            | CommandSpec::GitCheckout { .. }
            | CommandSpec::GitCheckoutNew { .. }
            | CommandSpec::YarnJest { .. } => true,
            CommandSpec::ShellCommand { command } => command.is_empty(),
            _ => false,
        }
    }

    /// Returns true for commands that require metro to be running before dispatch.
    pub fn needs_metro(&self) -> bool {
        matches!(self,
            CommandSpec::RnRunAndroid { .. }
            | CommandSpec::RnRunIos { .. }
            | CommandSpec::RnRunIosDevice
            | CommandSpec::RnReleaseBuild
        )
    }

    /// Returns true for commands that require the user to pick a connected device first.
    /// Only triggers when device_id is empty (not yet selected).
    pub fn needs_device_selection(&self) -> bool {
        matches!(self,
            CommandSpec::RnRunAndroid { device_id, .. } | CommandSpec::RnRunIos { device_id, .. }
            if device_id.is_empty()
        )
    }

    /// Human-readable label shown in the command palette and confirmation dialogs.
    pub fn label(&self) -> &'static str {
        match self {
            CommandSpec::GitResetHard => "git reset --hard HEAD",
            CommandSpec::GitPull => "git pull",
            CommandSpec::GitPush => "git push",
            CommandSpec::GitRebase { .. } => "git rebase <target>",
            CommandSpec::GitCheckout { .. } => "git checkout <branch>",
            CommandSpec::GitCheckoutNew { .. } => "git checkout -b <branch>",
            CommandSpec::RnCleanAndroid => "Clean Android (react-native clean)",
            CommandSpec::RnCleanCocoapods => "Clean CocoaPods (react-native clean)",
            CommandSpec::RmNodeModules => "Remove node_modules",
            CommandSpec::YarnInstall => "yarn install",
            CommandSpec::YarnPodInstall => "yarn pod-install",
            CommandSpec::RnRunAndroid { .. } => "Run on Android device",
            CommandSpec::RnRunIos { .. } => "Run on iOS device",
            CommandSpec::RnRunIosDevice => "Run on iOS device (auto)",
            CommandSpec::YarnUnitTests => "yarn unit-tests",
            CommandSpec::YarnJest { .. } => "yarn jest <filter>",
            CommandSpec::YarnLint => "yarn lint --quiet --fix",
            CommandSpec::YarnCheckTypes => "yarn check-types --incremental",
            CommandSpec::GitFetch => "git fetch --all --tags",
            CommandSpec::GitResetHardFetch => "git fetch + reset --hard origin/<branch>",
            CommandSpec::RnReleaseBuild => "gradlew assembleRelease",
            CommandSpec::AdbInstallApk => "adb install release APK",
            CommandSpec::ShellCommand { .. } => "shell command",
        }
    }
}

/// Toggle state for the clean submenu. Each field represents one cleanable target.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CleanOptions {
    pub node_modules: bool,
    pub pods: bool,
    pub android: bool,
    pub sync_after: bool, // if true, queue yarn install + pod-install after clean
}

/// State of a modal dialog overlaid on the main UI.
/// Only one modal can be active at a time.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum ModalState {
    /// User must confirm (Y) or cancel (N/Esc) a destructive action.
    Confirm {
        prompt: String,
        pending_command: CommandSpec,
    },
    /// User must type a string (e.g. branch name or jest filter) before the command can run.
    TextInput {
        prompt: String,
        buffer: String,
        /// Template command — the typed text fills the relevant field on submit.
        pending_template: Box<CommandSpec>,
    },
    /// User must pick a device from a list before a run command can be dispatched.
    DevicePicker {
        devices: Vec<DeviceInfo>,
        selected: usize,
        /// Template command — the chosen device_id fills the relevant field on confirm.
        pending_template: Box<CommandSpec>,
        /// Type-to-filter text — filters the device list by name (case-insensitive).
        filter: String,
    },
    /// Clean submenu with toggleable options. User checks items then confirms.
    CleanToggle {
        options: CleanOptions,
    },
    /// Sync-before-run prompt shown when stale worktree is about to run an app command.
    SyncBeforeRun {
        run_command: Box<CommandSpec>,
        needs_yarn: bool,
        needs_pods: bool,
    },
    /// Sync-before-metro prompt shown when stale worktree is about to start metro via Enter.
    SyncBeforeMetro {
        needs_yarn: bool,
        needs_pods: bool,
    },
    /// External metro conflict — another process occupies port 8081.
    ExternalMetroConflict {
        pid: u32,
        working_dir: String,
    },
    /// Branch picker for "create worktree with new branch" flow.
    BranchPicker {
        branches: Vec<String>,
        selected: usize,
        filter: String,
    },
}

/// Represents one connected device returned by `adb devices` or `xcrun simctl list`.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    /// Stable identifier: adb serial or iOS UDID.
    pub id: String,
    /// Human-readable display name (model name or simulator name).
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    // REFACTOR-02: `CommandSpec::is_cancellable()` returns false for git-porcelain
    // variants (data-integrity risk on cancellation) and true for all other commands.
    // One test per command family. Tests are pure — no tokio, no I/O.

    #[test]
    fn is_cancellable_git_variants_all_false() {
        let git_variants = [
            CommandSpec::GitResetHard,
            CommandSpec::GitResetHardFetch,
            CommandSpec::GitPull,
            CommandSpec::GitPush,
            CommandSpec::GitRebase { target: "main".into() },
            CommandSpec::GitCheckout { branch: "main".into() },
            CommandSpec::GitCheckoutNew { branch: "main".into() },
            CommandSpec::GitFetch,
        ];
        for spec in &git_variants {
            assert!(!spec.is_cancellable(), "git variant {:?} must NOT be cancellable", spec);
        }
    }

    #[test]
    fn is_cancellable_yarn_variants_all_true() {
        let yarn_variants = [
            CommandSpec::YarnInstall,
            CommandSpec::YarnPodInstall,
            CommandSpec::YarnUnitTests,
            CommandSpec::YarnCheckTypes,
            CommandSpec::YarnJest { filter: "".into() },
            CommandSpec::YarnLint,
        ];
        for spec in &yarn_variants {
            assert!(spec.is_cancellable(), "yarn variant {:?} must be cancellable", spec);
        }
    }

    #[test]
    fn is_cancellable_rn_run_variants_all_true() {
        let rn_run_variants = [
            CommandSpec::RnRunAndroid { device_id: "".into(), mode: None },
            CommandSpec::RnRunIos { device_id: "".into() },
            CommandSpec::RnRunIosDevice,
            CommandSpec::RnReleaseBuild,
        ];
        for spec in &rn_run_variants {
            assert!(spec.is_cancellable(), "rn-run variant {:?} must be cancellable", spec);
        }
    }

    #[test]
    fn is_cancellable_rn_clean_variants_all_true() {
        let clean_variants = [
            CommandSpec::RnCleanCocoapods,
            CommandSpec::RnCleanAndroid,
            CommandSpec::RmNodeModules,
        ];
        for spec in &clean_variants {
            assert!(spec.is_cancellable(), "clean variant {:?} must be cancellable", spec);
        }
    }

    #[test]
    fn is_cancellable_adb_install_true() {
        let spec = CommandSpec::AdbInstallApk;
        assert!(spec.is_cancellable(), "adb install must be cancellable");
    }

    #[test]
    fn is_cancellable_shell_true() {
        let spec = CommandSpec::ShellCommand { command: "echo hi".into() };
        assert!(spec.is_cancellable(), "shell command must be cancellable");
    }
}
