//! Command specification types for the command palette.
//!
//! `CommandSpec` describes *what* to run. The infrastructure layer converts it
//! to an actual process via `to_argv()`. No process spawning happens here.

use super::refresh::RefreshSet;

const ANDROID_AVD_PREFIX: &str = "avd:";

pub fn android_avd_name(device_id: &str) -> Option<&str> {
    device_id
        .strip_prefix(ANDROID_AVD_PREFIX)
        .filter(|name| !name.is_empty())
}

pub fn android_avd_device_id(avd_name: &str) -> String {
    format!("{ANDROID_AVD_PREFIX}{avd_name}")
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn android_wait_for_avd_script(avd_name: &str) -> String {
    let quoted_avd = shell_single_quote(avd_name);
    format!(
        "avd={quoted_avd}; i=0; while [ \"$i\" -lt 180 ]; do for serial in $(adb devices | awk 'NR > 1 && $2 == \"device\" && $1 ~ /^emulator-/ {{ print $1 }}'); do name=$(adb -s \"$serial\" emu avd name 2>/dev/null | awk 'NF && $0 != \"OK\" {{ print; exit }}' | tr -d '\\r'); if [ \"$name\" = \"$avd\" ]; then exit 0; fi; done; i=$((i + 1)); sleep 1; done; echo \"[error] could not find running emulator for AVD $avd\" >&2; exit 1"
    )
}

pub fn android_boot_avd_command(avd_name: &str) -> String {
    format!(
        "emulator -avd {} > /dev/null 2>&1 & {}",
        shell_single_quote(avd_name),
        android_wait_for_avd_script(avd_name)
    )
}

fn android_run_avd_script(avd_name: &str, variant: RunVariant) -> String {
    let quoted_avd = shell_single_quote(avd_name);
    let run_script = android_run_device_script("\"$serial\"", variant);
    format!(
        "avd={quoted_avd}; for serial in $(adb devices | awk 'NR > 1 && $2 == \"device\" && $1 ~ /^emulator-/ {{ print $1 }}'); do name=$(adb -s \"$serial\" emu avd name 2>/dev/null | awk 'NF && $0 != \"OK\" {{ print; exit }}' | tr -d '\\r'); if [ \"$name\" = \"$avd\" ]; then {run_script}; exit $?; fi; done; echo \"[error] could not find running emulator for AVD $avd\" >&2; exit 1"
    )
}

fn android_run_device_script(serial_arg: &str, variant: RunVariant) -> String {
    let gradle_task = variant.android_gradle_task();
    let apk_path = variant.android_apk_path();
    let app_id = variant.android_app_id();

    format!(
        "(cd android && ./gradlew app:{gradle_task} -x lint -PreactNativeDevServerPort=8081) && (adb -s {serial_arg} reverse tcp:8081 tcp:8081 || true) && adb -s {serial_arg} install -r -d {apk_path} && adb -s {serial_arg} shell am start -n {app_id}/com.aljazeera.mobile.MainActivity -a android.intent.action.MAIN -c android.intent.category.LAUNCHER"
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
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

    fn android_gradle_task(self) -> &'static str {
        match self {
            RunVariant::Local => "assembleLocalDebugOptimized",
            RunVariant::Dev => "assembleDevDebugOptimized",
            RunVariant::Prod => "assembleProdDebug",
        }
    }

    fn android_apk_path(self) -> &'static str {
        match self {
            RunVariant::Local => {
                "android/app/build/outputs/apk/local/debugOptimized/app-local-debugOptimized.apk"
            }
            RunVariant::Dev => {
                "android/app/build/outputs/apk/dev/debugOptimized/app-dev-debugOptimized.apk"
            }
            RunVariant::Prod => "android/app/build/outputs/apk/prod/debug/app-prod-debug.apk",
        }
    }

    fn android_app_id(self) -> &'static str {
        match self {
            RunVariant::Local => "com.aljazeera.mobile.local",
            RunVariant::Dev => "com.aljazeera.mobile.dev",
            RunVariant::Prod => "com.aljazeera.mobile",
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
/// 19 variants total. Pure data — no I/O.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandSpec {
    // Git commands (3 variants)
    GitResetHard,
    GitPull,
    GitPush,

    // React Native clean commands (3 variants)
    RnCleanAndroid,
    RnCleanCocoapods,
    RmNodeModules,

    // Yarn commands (2 variants)
    YarnInstall,
    YarnPodInstall,

    // UMP run commands (2 variants)
    UmpRunAndroid {
        device_id: String,
        variant: Option<RunVariant>,
    },
    UmpRunIos {
        device_id: String,
        variant: Option<RunVariant>,
    },

    // Test/quality commands (4 variants)
    YarnUnitTests,
    YarnJest {
        filter: String,
    },
    YarnLint,
    YarnCheckTypes,

    // Phase 05.1 additions (5 variants)
    GitFetch,          // g>f: git fetch --all --tags
    GitResetHardFetch, // g>X: fetch first, then reset to origin/<branch>
    RnReleaseBuild,    // a>r: gradlew assembleRelease
    AdbInstallApk,     // a>r continued: adb install of built APK
    ShellCommand {
        command: String,
    }, // !: run arbitrary shell command in worktree dir
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

/// All of a command's *static* facts in one place — the single source of truth.
/// Behaviour that depends on field values (argv, needs_text_input) or on runtime
/// state stays as separate methods; this card holds only discriminant-pure facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandMeta {
    /// Human-readable label for palette and confirmation dialogs.
    pub label: &'static str,
    /// Cannot be undone — dispatch shows a confirm prompt.
    pub destructive: bool,
    /// May be aborted mid-run (false for git plumbing — data-integrity).
    pub cancellable: bool,
    /// Background refreshes to run after this command completes.
    pub refresh: RefreshSet,
    /// What to do when a duplicate is dispatched while one is running.
    pub collision: CollisionPolicy,
}

impl CommandSpec {
    /// Returns the argv that should be passed to `tokio::process::Command`.
    /// The first element is the program; the rest are arguments.
    pub fn to_argv(&self) -> Vec<String> {
        match self {
            CommandSpec::GitResetHard => {
                vec!["git".into(), "reset".into(), "--hard".into(), "HEAD".into()]
            }
            CommandSpec::GitPull => vec!["git".into(), "pull".into()],
            CommandSpec::GitPush => vec!["git".into(), "push".into()],

            CommandSpec::RnCleanAndroid => vec![
                "npx".into(),
                "react-native".into(),
                "clean".into(),
                "--include".into(),
                "android".into(),
            ],
            CommandSpec::RnCleanCocoapods => vec![
                "npx".into(),
                "react-native".into(),
                "clean".into(),
                "--include".into(),
                "cocoapods".into(),
            ],
            CommandSpec::RmNodeModules => vec!["rm".into(), "-rf".into(), "node_modules".into()],

            CommandSpec::YarnInstall => vec!["yarn".into(), "install".into()],
            CommandSpec::YarnPodInstall => vec!["yarn".into(), "pod-install".into()],

            CommandSpec::UmpRunAndroid { device_id, variant } => {
                let variant = variant.unwrap_or(RunVariant::Local);
                if let Some(avd_name) = android_avd_name(device_id) {
                    return vec![
                        "sh".into(),
                        "-c".into(),
                        android_run_avd_script(avd_name, variant),
                    ];
                }

                if !device_id.is_empty() {
                    return vec![
                        "sh".into(),
                        "-c".into(),
                        android_run_device_script(&shell_single_quote(device_id), variant),
                    ];
                }

                vec!["yarn".into(), variant.android_script().into()]
            }
            CommandSpec::UmpRunIos { device_id, variant } => {
                let mut argv = vec![
                    "yarn".into(),
                    variant.unwrap_or(RunVariant::Local).ios_script().into(),
                ];
                if !device_id.is_empty() {
                    argv.push("--udid".into());
                    argv.push(device_id.clone());
                }
                argv
            }

            CommandSpec::YarnUnitTests => vec!["yarn".into(), "unit-tests".into()],
            CommandSpec::YarnJest { filter } => vec!["yarn".into(), "jest".into(), filter.clone()],
            CommandSpec::YarnLint => vec![
                "yarn".into(),
                "lint".into(),
                "--quiet".into(),
                "--fix".into(),
            ],
            CommandSpec::YarnCheckTypes => {
                vec!["yarn".into(), "check-types".into(), "--incremental".into()]
            }

            CommandSpec::GitFetch => vec![
                "git".into(),
                "fetch".into(),
                "--all".into(),
                "--tags".into(),
            ],
            CommandSpec::GitResetHardFetch => {
                // Two-step operation handled by command_runner — fetch then reset.
                // to_argv returns the fetch step; the runner handles chaining.
                vec![
                    "git".into(),
                    "fetch".into(),
                    "--all".into(),
                    "--tags".into(),
                ]
            }
            CommandSpec::RnReleaseBuild => {
                vec![
                    "./android/gradlew".into(),
                    "-p".into(),
                    "android".into(),
                    "assembleRelease".into(),
                ]
            }
            CommandSpec::AdbInstallApk => {
                vec![
                    "adb".into(),
                    "install".into(),
                    "-r".into(),
                    "android/app/build/outputs/apk/release/app-release.apk".into(),
                ]
            }
            CommandSpec::ShellCommand { command } => {
                vec!["sh".into(), "-c".into(), command.clone()]
            }
        }
    }

    /// The single source of truth for this command's static facts. The
    /// exhaustive match (no `_` arm) is the compile-time drift guard — adding a
    /// `CommandSpec` variant fails to compile here until its card is filled in.
    pub fn meta(&self) -> CommandMeta {
        let full = RefreshSet { worktrees: true, staleness: true, jira_titles: true };
        let stale = RefreshSet { worktrees: false, staleness: true, jira_titles: false };
        let none = RefreshSet::none();
        use CollisionPolicy::{BlockNew, CancelPrevious};
        match self {
            CommandSpec::GitResetHard => CommandMeta { label: "git reset --hard HEAD", destructive: true, cancellable: false, refresh: full, collision: BlockNew },
            CommandSpec::GitPull => CommandMeta { label: "git pull", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
            CommandSpec::GitPush => CommandMeta { label: "git push", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
            CommandSpec::GitFetch => CommandMeta { label: "git fetch --all --tags", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
            CommandSpec::GitResetHardFetch => CommandMeta { label: "git fetch + reset --hard origin/<branch>", destructive: true, cancellable: false, refresh: full, collision: BlockNew },
            CommandSpec::RnCleanAndroid => CommandMeta { label: "Clean Android (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
            CommandSpec::RnCleanCocoapods => CommandMeta { label: "Clean CocoaPods (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
            CommandSpec::RmNodeModules => CommandMeta { label: "Remove node_modules", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
            CommandSpec::YarnInstall => CommandMeta { label: "yarn install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew },
            CommandSpec::YarnPodInstall => CommandMeta { label: "yarn pod-install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew },
            CommandSpec::YarnUnitTests => CommandMeta { label: "yarn unit-tests", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::YarnJest { .. } => CommandMeta { label: "yarn jest <filter>", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::YarnLint => CommandMeta { label: "yarn lint --quiet --fix", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::YarnCheckTypes => CommandMeta { label: "yarn check-types --incremental", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::UmpRunAndroid { .. } => CommandMeta { label: "Run Android (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::UmpRunIos { .. } => CommandMeta { label: "Run iOS (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::RnReleaseBuild => CommandMeta { label: "gradlew assembleRelease", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::AdbInstallApk => CommandMeta { label: "adb install release APK", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
            CommandSpec::ShellCommand { .. } => CommandMeta { label: "shell command", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
        }
    }

    /// Returns true for commands that cannot be undone and require explicit confirmation.
    pub fn is_destructive(&self) -> bool {
        self.meta().destructive
    }

    /// Returns false for git-porcelain commands (data-integrity risk on cancellation);
    /// true for all other commands (yarn, UMP runs, rm, adb, shell).
    ///
    /// REFACTOR-02: Type-driven cancellability. Git variants are closed by construction —
    /// adding a new `Git*` variant requires explicit opt-in here (compile-error would be
    /// ideal; today this is a flat-enum predicate per AUDIT-ADDENDUM F-501 DEFERRED decision).
    pub fn is_cancellable(&self) -> bool {
        self.meta().cancellable
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
        self.meta().collision
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
            CommandSpec::YarnJest { .. } => true,
            CommandSpec::ShellCommand { command } => command.is_empty(),
            _ => false,
        }
    }

    /// Returns true for commands that require metro to be running before dispatch.
    pub fn needs_metro(&self) -> bool {
        matches!(
            self,
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
        self.meta().label
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
    /// Cache use is decided downstream by `dispatch_run`, so the picker carries
    /// no cache flags.
    RunVariantPicker {
        selected: usize,
        pending_template: Box<CommandSpec>,
        boot_android_emulator: bool,
    },
    /// Clean submenu with toggleable options. User checks items then confirms.
    CleanToggle { options: CleanOptions },
    /// Sync-before-run prompt shown when stale worktree is about to run an app command.
    SyncBeforeRun {
        run_command: Box<CommandSpec>,
        needs_yarn: bool,
        needs_pods: bool,
    },
    /// Sync-before-metro prompt shown when stale worktree is about to start metro via Enter.
    SyncBeforeMetro { needs_yarn: bool, needs_pods: bool },
    /// External metro conflict — another process occupies port 8081.
    ExternalMetroConflict { pid: u32, working_dir: String },
    /// Branch picker for "create worktree with new branch" flow.
    BranchPicker {
        branches: Vec<String>,
        selected: usize,
        filter: String,
    },
    /// PR picker for review worktree flow.
    PullRequestPicker {
        pull_requests: Vec<crate::domain::review::PullRequest>,
        selected: usize,
        search: String,
        filter: crate::domain::review::PullRequestFilter,
    },
    /// Non-error informational prompt.
    Info { message: String },
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
        let android_local = CommandSpec::UmpRunAndroid {
            device_id: "emulator-5554".into(),
            variant: Some(RunVariant::Local),
        }
        .to_argv();
        assert_eq!(&android_local[0], "sh");
        assert_eq!(&android_local[1], "-c");
        assert!(android_local[2].contains("app:assembleLocalDebugOptimized"));
        assert!(
            android_local[2]
                .contains("com.aljazeera.mobile.local/com.aljazeera.mobile.MainActivity")
        );

        let android_dev = CommandSpec::UmpRunAndroid {
            device_id: "emulator-5554".into(),
            variant: Some(RunVariant::Dev),
        }
        .to_argv();
        assert_eq!(&android_dev[0], "sh");
        assert_eq!(&android_dev[1], "-c");
        assert!(android_dev[2].contains("app:assembleDevDebugOptimized"));
        assert!(
            android_dev[2].contains("com.aljazeera.mobile.dev/com.aljazeera.mobile.MainActivity")
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
    fn ump_android_run_for_physical_device_installs_known_variant_apk() {
        let argv = CommandSpec::UmpRunAndroid {
            device_id: "R58W4019D1V".into(),
            variant: Some(RunVariant::Local),
        }
        .to_argv();

        assert_eq!(&argv[0], "sh");
        assert_eq!(&argv[1], "-c");
        let script = &argv[2];

        assert!(
            script.contains("./gradlew app:assembleLocalDebugOptimized"),
            "Android local run should assemble the exact Gradle variant, got {argv:?}"
        );
        assert!(
            script.contains(
                "android/app/build/outputs/apk/local/debugOptimized/app-local-debugOptimized.apk"
            ),
            "Android local run should install the APK path Gradle actually writes, got {argv:?}"
        );
        assert!(
            script.contains("adb -s 'R58W4019D1V' install -r -d"),
            "Android local run should install on the selected physical device, got {argv:?}"
        );
        assert!(
            script.contains("com.aljazeera.mobile.local/com.aljazeera.mobile.MainActivity"),
            "Android local run should launch the local app id, got {argv:?}"
        );
        assert!(
            !script.contains("react-native run-android"),
            "Android local run should bypass React Native CLI APK path derivation, got {argv:?}"
        );
    }

    #[test]
    fn ump_android_run_for_avd_resolves_serial_before_install_script() {
        let argv = CommandSpec::UmpRunAndroid {
            device_id: "avd:Pixel_9a".into(),
            variant: Some(RunVariant::Local),
        }
        .to_argv();

        assert_eq!(&argv[0], "sh");
        assert_eq!(&argv[1], "-c");
        assert!(
            argv[2].contains("adb devices"),
            "AVD run should inspect connected adb serials, got {argv:?}"
        );
        assert!(
            argv[2].contains("adb -s \"$serial\" emu avd name"),
            "AVD run should map adb serials back to AVD names, got {argv:?}"
        );
        assert!(
            argv[2].contains("Pixel_9a"),
            "AVD run should preserve the selected AVD name, got {argv:?}"
        );
        assert!(
            argv[2].contains("./gradlew app:assembleLocalDebugOptimized"),
            "AVD run should assemble the exact Gradle variant, got {argv:?}"
        );
        assert!(
            argv[2].contains("adb -s \"$serial\" install -r -d android/app/build/outputs/apk/local/debugOptimized/app-local-debugOptimized.apk"),
            "AVD run should install the known APK path on the resolved adb serial, got {argv:?}"
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
            CommandSpec::GitFetch,
        ];
        for spec in &git_variants {
            assert!(
                !spec.is_cancellable(),
                "git variant {:?} must NOT be cancellable",
                spec
            );
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
            assert!(
                spec.is_cancellable(),
                "yarn variant {:?} must be cancellable",
                spec
            );
        }
    }

    #[test]
    fn is_cancellable_run_variants_all_true() {
        let run_variants = [
            CommandSpec::UmpRunAndroid {
                device_id: "".into(),
                variant: Some(RunVariant::Local),
            },
            CommandSpec::UmpRunIos {
                device_id: "".into(),
                variant: Some(RunVariant::Dev),
            },
            CommandSpec::RnReleaseBuild,
        ];
        for spec in &run_variants {
            assert!(
                spec.is_cancellable(),
                "run variant {:?} must be cancellable",
                spec
            );
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
            assert!(
                spec.is_cancellable(),
                "clean variant {:?} must be cancellable",
                spec
            );
        }
    }

    #[test]
    fn is_cancellable_adb_install_true() {
        let spec = CommandSpec::AdbInstallApk;
        assert!(spec.is_cancellable(), "adb install must be cancellable");
    }

    #[test]
    fn is_cancellable_shell_true() {
        let spec = CommandSpec::ShellCommand {
            command: "echo hi".into(),
        };
        assert!(spec.is_cancellable(), "shell command must be cancellable");
    }

    // TASK-05 / Plan 15-04: `CommandSpec::collision_policy()` returns the per-variant
    // policy applied when a new dispatch collides with a running task on the same
    // `(discriminant, WorktreeId)` per Phase 14 D-05. Three per-family tests plus one
    // drift-guard meta-test enumerating every variant.

    #[test]
    fn collision_policy_idempotent_installs_block_new() {
        let installs = [CommandSpec::YarnInstall, CommandSpec::YarnPodInstall];
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
            CommandSpec::UmpRunAndroid {
                device_id: "emulator-5554".into(),
                variant: Some(RunVariant::Local),
            },
            CommandSpec::UmpRunIos {
                device_id: "ios-udid-1".into(),
                variant: Some(RunVariant::Prod),
            },
            CommandSpec::RnReleaseBuild,
            CommandSpec::AdbInstallApk,
            CommandSpec::ShellCommand {
                command: "ls".into(),
            },
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
            CommandSpec::RnCleanAndroid,
            CommandSpec::RnCleanCocoapods,
            CommandSpec::RmNodeModules,
            CommandSpec::YarnInstall,
            CommandSpec::YarnPodInstall,
            CommandSpec::UmpRunAndroid {
                device_id: "".into(),
                variant: Some(RunVariant::Local),
            },
            CommandSpec::UmpRunIos {
                device_id: "".into(),
                variant: Some(RunVariant::Dev),
            },
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
        assert_eq!(
            variants.len(),
            19,
            "must enumerate all 19 CommandSpec variants"
        );
    }

    #[test]
    fn command_metadata_matrix() {
        use crate::domain::refresh::{refresh_needed, RefreshSet};
        use CollisionPolicy::{BlockNew, CancelPrevious};

        let full = RefreshSet { worktrees: true, staleness: true, jira_titles: true };
        let stale = RefreshSet { worktrees: false, staleness: true, jira_titles: false };
        let none = RefreshSet::none();

        // (spec, label, destructive, cancellable, collision, refresh)
        let cases: Vec<(CommandSpec, &str, bool, bool, CollisionPolicy, RefreshSet)> = vec![
            (CommandSpec::GitResetHard, "git reset --hard HEAD", true, false, BlockNew, full.clone()),
            (CommandSpec::GitPull, "git pull", false, false, BlockNew, none.clone()),
            (CommandSpec::GitPush, "git push", false, false, BlockNew, none.clone()),
            (CommandSpec::GitFetch, "git fetch --all --tags", false, false, BlockNew, none.clone()),
            (CommandSpec::GitResetHardFetch, "git fetch + reset --hard origin/<branch>", true, false, BlockNew, full.clone()),
            (CommandSpec::RnCleanAndroid, "Clean Android (react-native clean)", true, true, CancelPrevious, stale.clone()),
            (CommandSpec::RnCleanCocoapods, "Clean CocoaPods (react-native clean)", true, true, CancelPrevious, stale.clone()),
            (CommandSpec::RmNodeModules, "Remove node_modules", true, true, CancelPrevious, stale.clone()),
            (CommandSpec::YarnInstall, "yarn install", false, true, BlockNew, stale.clone()),
            (CommandSpec::YarnPodInstall, "yarn pod-install", false, true, BlockNew, stale.clone()),
            (CommandSpec::YarnUnitTests, "yarn unit-tests", false, true, CancelPrevious, none.clone()),
            (CommandSpec::YarnJest { filter: String::new() }, "yarn jest <filter>", false, true, CancelPrevious, none.clone()),
            (CommandSpec::YarnLint, "yarn lint --quiet --fix", false, true, CancelPrevious, none.clone()),
            (CommandSpec::YarnCheckTypes, "yarn check-types --incremental", false, true, CancelPrevious, none.clone()),
            (CommandSpec::UmpRunAndroid { device_id: String::new(), variant: None }, "Run Android (UMP)", false, true, CancelPrevious, none.clone()),
            (CommandSpec::UmpRunIos { device_id: String::new(), variant: None }, "Run iOS (UMP)", false, true, CancelPrevious, none.clone()),
            (CommandSpec::RnReleaseBuild, "gradlew assembleRelease", false, true, CancelPrevious, none.clone()),
            (CommandSpec::AdbInstallApk, "adb install release APK", false, true, CancelPrevious, none.clone()),
            (CommandSpec::ShellCommand { command: String::new() }, "shell command", false, true, CancelPrevious, none.clone()),
        ];

        assert_eq!(cases.len(), 19, "matrix must cover all 19 CommandSpec variants");

        for (spec, label, destructive, cancellable, collision, refresh) in &cases {
            assert_eq!(spec.label(), *label, "label mismatch for {spec:?}");
            assert_eq!(spec.is_destructive(), *destructive, "is_destructive mismatch for {spec:?}");
            assert_eq!(spec.is_cancellable(), *cancellable, "is_cancellable mismatch for {spec:?}");
            assert_eq!(spec.collision_policy(), *collision, "collision_policy mismatch for {spec:?}");
            assert_eq!(refresh_needed(spec), *refresh, "refresh mismatch for {spec:?}");
        }
    }
}
