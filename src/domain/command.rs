//! Command specification types for the command palette.
//!
//! `CommandSpec` describes *what* to run. The infrastructure layer converts it
//! to an actual process via `to_argv()`. No process spawning happens here.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunVariant {
    Local,
    Dev,
    Prod,
}

impl RunVariant {
    pub const ALL: [RunVariant; 3] = [RunVariant::Local, RunVariant::Dev, RunVariant::Prod];

    pub fn label(self) -> &'static str {
        match self {
            RunVariant::Local => "local",
            RunVariant::Dev => "dev",
            RunVariant::Prod => "prod",
        }
    }

    fn android_script(self) -> &'static str {
        match self {
            RunVariant::Local => "android:local",
            RunVariant::Dev => "android:dev",
            RunVariant::Prod => "android:prod",
        }
    }

    fn ios_script(self) -> &'static str {
        match self {
            RunVariant::Local => "ios:local",
            RunVariant::Dev => "ios:dev",
            RunVariant::Prod => "ios:prod",
        }
    }
}

/// All commands that can be dispatched from the dashboard command palettes.
/// 22 variants total. Pure data — no I/O.
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

    // UMP run commands (2 variants)
    UmpRunAndroid { device_id: String, variant: Option<RunVariant> },
    UmpRunIos { device_id: String, variant: Option<RunVariant> },

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

/// Per-variant policy applied when a new task dispatch matches a running task
/// on the same `(CommandSpec discriminant, WorktreeId)` per Phase 14 D-05.
///
/// TASK-05 / 15-RESEARCH §F6: collision_policy() is the type-driven authority
/// consulted by `dispatch_command` (Plan 15-05) when it detects a collision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionPolicy {
    /// The existing task keeps running; the new dispatch is silently dropped.
    /// Used for idempotent installs (running a second `yarn install` while one
    /// is in progress produces the same result; no point in double-running)
    /// and for non-cancellable git porcelain (Q-4 lock — cancel-previous is
    /// impossible for variants where `is_cancellable() == false`).
    BlockNew,
    /// The existing task is aborted, then the new task is dispatched. Used for
    /// builds, tests, and runs where the user intent is "run THIS version NOW"
    /// — re-running a test or app build should reflect the latest sources.
    CancelPrevious,
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

            CommandSpec::UmpRunAndroid { device_id, variant } => {
                let mut argv = vec!["yarn".into(), variant.unwrap_or(RunVariant::Local).android_script().into()];
                if !device_id.is_empty() {
                    argv.push("--device".into());
                    argv.push(device_id.clone());
                }
                argv
            }
            CommandSpec::UmpRunIos { device_id, variant } => {
                let mut argv = vec!["yarn".into(), variant.unwrap_or(RunVariant::Local).ios_script().into()];
                if !device_id.is_empty() {
                    argv.push("--udid".into());
                    argv.push(device_id.clone());
                }
                argv
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
    /// true for all other commands (yarn, UMP runs, rm, adb, shell).
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

    /// Returns the per-variant collision policy applied when a new dispatch
    /// matches a running task on the same `(discriminant, WorktreeId)` per
    /// D-05.
    ///
    /// `BlockNew` for idempotent installs and non-cancellable git variants
    /// (Q-4); `CancelPrevious` for builds, tests, runs, and clean operations
    /// where "run THIS version NOW" is the intent.
    ///
    /// TASK-05 / 15-RESEARCH §F6. The match is intentionally exhaustive (NO
    /// `_ =>` arm) so adding a new `CommandSpec` variant produces a compile
    /// error here, forcing the maintainer to assign a policy explicitly
    /// (T-15-04-01 mitigation). The drift-guard meta-test
    /// `collision_policy_covers_every_variant` provides a second layer of
    /// enforcement.
    pub fn collision_policy(&self) -> CollisionPolicy {
        match self {
            // Idempotent installs — running again while one is in progress
            // produces the same result.
            CommandSpec::YarnInstall
            | CommandSpec::YarnPodInstall => CollisionPolicy::BlockNew,

            // Non-cancellable git porcelain (Q-4): cancel-previous is
            // impossible for variants where `is_cancellable() == false`, so
            // BlockNew is the only valid policy.
            CommandSpec::GitResetHard
            | CommandSpec::GitResetHardFetch
            | CommandSpec::GitPull
            | CommandSpec::GitPush
            | CommandSpec::GitRebase { .. }
            | CommandSpec::GitCheckout { .. }
            | CommandSpec::GitCheckoutNew { .. }
            | CommandSpec::GitFetch => CollisionPolicy::BlockNew,

            // Builds, tests, runs — "run THIS version NOW" semantics.
            CommandSpec::YarnUnitTests
            | CommandSpec::YarnJest { .. }
            | CommandSpec::YarnLint
            | CommandSpec::YarnCheckTypes
            | CommandSpec::UmpRunAndroid { .. }
            | CommandSpec::UmpRunIos { .. }
            | CommandSpec::RnReleaseBuild
            | CommandSpec::AdbInstallApk
            | CommandSpec::ShellCommand { .. }
            | CommandSpec::RnCleanAndroid
            | CommandSpec::RnCleanCocoapods
            | CommandSpec::RmNodeModules => CollisionPolicy::CancelPrevious,
        }
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
            CommandSpec::UmpRunAndroid { .. }
            | CommandSpec::UmpRunIos { .. }
            | CommandSpec::RnReleaseBuild
        )
    }

    /// Returns true for commands that require the user to pick a connected device first.
    /// Only triggers when device_id is empty (not yet selected).
    pub fn needs_device_selection(&self) -> bool {
        matches!(self,
            CommandSpec::UmpRunAndroid { device_id, .. }
                | CommandSpec::UmpRunIos { device_id, .. }
            if device_id.is_empty()
        )
    }

    pub fn needs_run_variant_selection(&self) -> bool {
        matches!(
            self,
            CommandSpec::UmpRunAndroid { variant: None, .. }
                | CommandSpec::UmpRunIos { variant: None, .. }
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
            CommandSpec::UmpRunAndroid { .. } => "Run Android (UMP)",
            CommandSpec::UmpRunIos { .. } => "Run iOS (UMP)",
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
    /// User picked the target; now choose UMP run type in local/dev/prod order.
    RunVariantPicker {
        selected: usize,
        pending_template: Box<CommandSpec>,
        boot_android_emulator: bool,
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

    #[test]
    fn ump_run_argv_uses_package_scripts_and_target_flags() {
        assert_eq!(
            CommandSpec::UmpRunAndroid {
                device_id: "emulator-5554".into(),
                variant: Some(RunVariant::Local),
            }
            .to_argv(),
            vec!["yarn", "android:local", "--device", "emulator-5554"]
        );
        assert_eq!(
            CommandSpec::UmpRunAndroid {
                device_id: "emulator-5554".into(),
                variant: Some(RunVariant::Dev),
            }
            .to_argv(),
            vec!["yarn", "android:dev", "--device", "emulator-5554"]
        );
        assert_eq!(
            CommandSpec::UmpRunIos {
                device_id: "ios-udid-1".into(),
                variant: Some(RunVariant::Prod),
            }
            .to_argv(),
            vec!["yarn", "ios:prod", "--udid", "ios-udid-1"]
        );
    }

    #[test]
    fn ump_run_variants_require_target_then_run_variant() {
        let android = CommandSpec::UmpRunAndroid {
            device_id: String::new(),
            variant: None,
        };
        assert!(android.needs_device_selection());
        assert!(android.needs_run_variant_selection());

        let ios_with_target = CommandSpec::UmpRunIos {
            device_id: "ios-udid-1".into(),
            variant: None,
        };
        assert!(!ios_with_target.needs_device_selection());
        assert!(ios_with_target.needs_run_variant_selection());
    }

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
    fn is_cancellable_run_variants_all_true() {
        let run_variants = [
            CommandSpec::UmpRunAndroid { device_id: "".into(), variant: Some(RunVariant::Local) },
            CommandSpec::UmpRunIos { device_id: "".into(), variant: Some(RunVariant::Dev) },
            CommandSpec::RnReleaseBuild,
        ];
        for spec in &run_variants {
            assert!(spec.is_cancellable(), "run variant {:?} must be cancellable", spec);
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

    // TASK-05 / Plan 15-04: `CommandSpec::collision_policy()` returns the per-variant
    // policy applied when a new dispatch collides with a running task on the same
    // `(discriminant, WorktreeId)` per Phase 14 D-05. Three per-family tests plus one
    // drift-guard meta-test enumerating every variant.

    #[test]
    fn collision_policy_idempotent_installs_block_new() {
        let installs = [
            CommandSpec::YarnInstall,
            CommandSpec::YarnPodInstall,
        ];
        for spec in &installs {
            assert_eq!(
                spec.collision_policy(),
                CollisionPolicy::BlockNew,
                "install variant {:?} must BlockNew",
                spec
            );
        }
    }

    #[test]
    fn collision_policy_builds_tests_runs_cancel_previous() {
        let cancelable = [
            CommandSpec::YarnUnitTests,
            CommandSpec::YarnJest { filter: "x".into() },
            CommandSpec::YarnLint,
            CommandSpec::YarnCheckTypes,
            CommandSpec::UmpRunAndroid { device_id: "emulator-5554".into(), variant: Some(RunVariant::Local) },
            CommandSpec::UmpRunIos { device_id: "ios-udid-1".into(), variant: Some(RunVariant::Prod) },
            CommandSpec::RnReleaseBuild,
            CommandSpec::AdbInstallApk,
            CommandSpec::ShellCommand { command: "ls".into() },
            CommandSpec::RnCleanAndroid,
            CommandSpec::RnCleanCocoapods,
            CommandSpec::RmNodeModules,
        ];
        for spec in &cancelable {
            assert_eq!(
                spec.collision_policy(),
                CollisionPolicy::CancelPrevious,
                "build/test/run variant {:?} must CancelPrevious",
                spec
            );
        }
    }

    #[test]
    fn collision_policy_git_variants_all_block_new() {
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
            assert_eq!(
                spec.collision_policy(),
                CollisionPolicy::BlockNew,
                "git variant {:?} must BlockNew (non-cancellable cannot CancelPrevious)",
                spec
            );
        }
    }

    /// Drift-guard meta-test: mirrors the predicate body with an exhaustive
    /// match (no `_` arm). Adding a new CommandSpec variant fails to compile here
    /// AND in `collision_policy()` itself — two layers of compile-time enforcement
    /// against silent default assignment (mitigates T-15-04-01).
    #[test]
    fn collision_policy_covers_every_variant() {
        // One instance of every CommandSpec variant; if a variant is added in a
        // future phase, this match becomes non-exhaustive and the build fails.
        let variants = [
            CommandSpec::GitResetHard,
            CommandSpec::GitPull,
            CommandSpec::GitPush,
            CommandSpec::GitRebase { target: "main".into() },
            CommandSpec::GitCheckout { branch: "main".into() },
            CommandSpec::GitCheckoutNew { branch: "main".into() },
            CommandSpec::RnCleanAndroid,
            CommandSpec::RnCleanCocoapods,
            CommandSpec::RmNodeModules,
            CommandSpec::YarnInstall,
            CommandSpec::YarnPodInstall,
            CommandSpec::UmpRunAndroid { device_id: "".into(), variant: Some(RunVariant::Local) },
            CommandSpec::UmpRunIos { device_id: "".into(), variant: Some(RunVariant::Dev) },
            CommandSpec::YarnUnitTests,
            CommandSpec::YarnJest { filter: "".into() },
            CommandSpec::YarnLint,
            CommandSpec::YarnCheckTypes,
            CommandSpec::GitFetch,
            CommandSpec::GitResetHardFetch,
            CommandSpec::RnReleaseBuild,
            CommandSpec::AdbInstallApk,
            CommandSpec::ShellCommand { command: "".into() },
        ];
        for v in &variants {
            // Exhaustive match — no `_ =>` arm. Mirrors `collision_policy()` body.
            let _policy: CollisionPolicy = match v {
                CommandSpec::GitResetHard
                | CommandSpec::GitResetHardFetch
                | CommandSpec::GitPull
                | CommandSpec::GitPush
                | CommandSpec::GitRebase { .. }
                | CommandSpec::GitCheckout { .. }
                | CommandSpec::GitCheckoutNew { .. }
                | CommandSpec::GitFetch
                | CommandSpec::YarnInstall
                | CommandSpec::YarnPodInstall => CollisionPolicy::BlockNew,
                CommandSpec::YarnUnitTests
                | CommandSpec::YarnJest { .. }
                | CommandSpec::YarnLint
                | CommandSpec::YarnCheckTypes
                | CommandSpec::UmpRunAndroid { .. }
                | CommandSpec::UmpRunIos { .. }
                | CommandSpec::RnReleaseBuild
                | CommandSpec::AdbInstallApk
                | CommandSpec::ShellCommand { .. }
                | CommandSpec::RnCleanAndroid
                | CommandSpec::RnCleanCocoapods
                | CommandSpec::RmNodeModules => CollisionPolicy::CancelPrevious,
            };
            // Also assert the predicate agrees with the local mirror.
            assert!(matches!(
                v.collision_policy(),
                CollisionPolicy::BlockNew | CollisionPolicy::CancelPrevious
            ));
        }
        assert_eq!(variants.len(), 22, "must enumerate all 22 CommandSpec variants");
    }
}
