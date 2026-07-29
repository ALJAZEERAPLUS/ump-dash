# Task 2 review package — range 1f87a7c..11ba416

## Commits
11ba416 refactor(command): introduce CommandMeta info-card, readers delegate to meta()

## Diff stat
 src/domain/command.rs | 119 +++++++++++++++++++++++---------------------------
 src/domain/refresh.rs |   2 +-
 2 files changed, 55 insertions(+), 66 deletions(-)

## Full diff (-U12)
```diff
diff --git a/src/domain/command.rs b/src/domain/command.rs
index de83c15..5e1a91d 100644
--- a/src/domain/command.rs
+++ b/src/domain/command.rs
@@ -1,17 +1,19 @@
 //! Command specification types for the command palette.
 //!
 //! `CommandSpec` describes *what* to run. The infrastructure layer converts it
 //! to an actual process via `to_argv()`. No process spawning happens here.

+use super::refresh::RefreshSet;
+
 const ANDROID_AVD_PREFIX: &str = "avd:";

 pub fn android_avd_name(device_id: &str) -> Option<&str> {
     device_id
         .strip_prefix(ANDROID_AVD_PREFIX)
         .filter(|name| !name.is_empty())
 }

 pub fn android_avd_device_id(avd_name: &str) -> String {
     format!("{ANDROID_AVD_PREFIX}{avd_name}")
 }

@@ -172,24 +174,41 @@ pub enum CollisionPolicy {
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

+/// All of a command's *static* facts in one place — the single source of truth.
+/// Behaviour that depends on field values (argv, needs_text_input) or on runtime
+/// state stays as separate methods; this card holds only discriminant-pure facts.
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+pub struct CommandMeta {
+    /// Human-readable label for palette and confirmation dialogs.
+    pub label: &'static str,
+    /// Cannot be undone — dispatch shows a confirm prompt.
+    pub destructive: bool,
+    /// May be aborted mid-run (false for git plumbing — data-integrity).
+    pub cancellable: bool,
+    /// Background refreshes to run after this command completes.
+    pub refresh: RefreshSet,
+    /// What to do when a duplicate is dispatched while one is running.
+    pub collision: CollisionPolicy,
+}
+
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
@@ -284,96 +303,86 @@ impl CommandSpec {
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

+    /// The single source of truth for this command's static facts. The
+    /// exhaustive match (no `_` arm) is the compile-time drift guard — adding a
+    /// `CommandSpec` variant fails to compile here until its card is filled in.
+    pub fn meta(&self) -> CommandMeta {
+        let full = RefreshSet { worktrees: true, staleness: true, jira_titles: true };
+        let stale = RefreshSet { worktrees: false, staleness: true, jira_titles: false };
+        let none = RefreshSet::none();
+        use CollisionPolicy::{BlockNew, CancelPrevious};
+        match self {
+            CommandSpec::GitResetHard => CommandMeta { label: "git reset --hard HEAD", destructive: true, cancellable: false, refresh: full, collision: BlockNew },
+            CommandSpec::GitPull => CommandMeta { label: "git pull", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
+            CommandSpec::GitPush => CommandMeta { label: "git push", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
+            CommandSpec::GitFetch => CommandMeta { label: "git fetch --all --tags", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
+            CommandSpec::GitResetHardFetch => CommandMeta { label: "git fetch + reset --hard origin/<branch>", destructive: true, cancellable: false, refresh: full, collision: BlockNew },
+            CommandSpec::RnCleanAndroid => CommandMeta { label: "Clean Android (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
+            CommandSpec::RnCleanCocoapods => CommandMeta { label: "Clean CocoaPods (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
+            CommandSpec::RmNodeModules => CommandMeta { label: "Remove node_modules", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
+            CommandSpec::YarnInstall => CommandMeta { label: "yarn install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew },
+            CommandSpec::YarnPodInstall => CommandMeta { label: "yarn pod-install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew },
+            CommandSpec::YarnUnitTests => CommandMeta { label: "yarn unit-tests", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::YarnJest { .. } => CommandMeta { label: "yarn jest <filter>", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::YarnLint => CommandMeta { label: "yarn lint --quiet --fix", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::YarnCheckTypes => CommandMeta { label: "yarn check-types --incremental", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::UmpRunAndroid { .. } => CommandMeta { label: "Run Android (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::UmpRunIos { .. } => CommandMeta { label: "Run iOS (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::RnReleaseBuild => CommandMeta { label: "gradlew assembleRelease", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::AdbInstallApk => CommandMeta { label: "adb install release APK", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::ShellCommand { .. } => CommandMeta { label: "shell command", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+        }
+    }
+
     /// Returns true for commands that cannot be undone and require explicit confirmation.
     pub fn is_destructive(&self) -> bool {
-        matches!(
-            self,
-            CommandSpec::GitResetHard
-                | CommandSpec::GitResetHardFetch
-                | CommandSpec::RnCleanAndroid
-                | CommandSpec::RnCleanCocoapods
-                | CommandSpec::RmNodeModules
-        )
+        self.meta().destructive
     }

     /// Returns false for git-porcelain commands (data-integrity risk on cancellation);
     /// true for all other commands (yarn, UMP runs, rm, adb, shell).
     ///
     /// REFACTOR-02: Type-driven cancellability. Git variants are closed by construction —
     /// adding a new `Git*` variant requires explicit opt-in here (compile-error would be
     /// ideal; today this is a flat-enum predicate per AUDIT-ADDENDUM F-501 DEFERRED decision).
     pub fn is_cancellable(&self) -> bool {
-        !matches!(
-            self,
-            CommandSpec::GitResetHard
-                | CommandSpec::GitResetHardFetch
-                | CommandSpec::GitPull
-                | CommandSpec::GitPush
-                | CommandSpec::GitFetch
-        )
+        self.meta().cancellable
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
-        match self {
-            // Idempotent installs — running again while one is in progress
-            // produces the same result.
-            CommandSpec::YarnInstall | CommandSpec::YarnPodInstall => CollisionPolicy::BlockNew,
-
-            // Non-cancellable git porcelain (Q-4): cancel-previous is
-            // impossible for variants where `is_cancellable() == false`, so
-            // BlockNew is the only valid policy.
-            CommandSpec::GitResetHard
-            | CommandSpec::GitResetHardFetch
-            | CommandSpec::GitPull
-            | CommandSpec::GitPush
-            | CommandSpec::GitFetch => CollisionPolicy::BlockNew,
-
-            // Builds, tests, runs — "run THIS version NOW" semantics.
-            CommandSpec::YarnUnitTests
-            | CommandSpec::YarnJest { .. }
-            | CommandSpec::YarnLint
-            | CommandSpec::YarnCheckTypes
-            | CommandSpec::UmpRunAndroid { .. }
-            | CommandSpec::UmpRunIos { .. }
-            | CommandSpec::RnReleaseBuild
-            | CommandSpec::AdbInstallApk
-            | CommandSpec::ShellCommand { .. }
-            | CommandSpec::RnCleanAndroid
-            | CommandSpec::RnCleanCocoapods
-            | CommandSpec::RmNodeModules => CollisionPolicy::CancelPrevious,
-        }
+        self.meta().collision
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
@@ -405,45 +414,25 @@ impl CommandSpec {
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
-        match self {
-            CommandSpec::GitResetHard => "git reset --hard HEAD",
-            CommandSpec::GitPull => "git pull",
-            CommandSpec::GitPush => "git push",
-            CommandSpec::RnCleanAndroid => "Clean Android (react-native clean)",
-            CommandSpec::RnCleanCocoapods => "Clean CocoaPods (react-native clean)",
-            CommandSpec::RmNodeModules => "Remove node_modules",
-            CommandSpec::YarnInstall => "yarn install",
-            CommandSpec::YarnPodInstall => "yarn pod-install",
-            CommandSpec::UmpRunAndroid { .. } => "Run Android (UMP)",
-            CommandSpec::UmpRunIos { .. } => "Run iOS (UMP)",
-            CommandSpec::YarnUnitTests => "yarn unit-tests",
-            CommandSpec::YarnJest { .. } => "yarn jest <filter>",
-            CommandSpec::YarnLint => "yarn lint --quiet --fix",
-            CommandSpec::YarnCheckTypes => "yarn check-types --incremental",
-            CommandSpec::GitFetch => "git fetch --all --tags",
-            CommandSpec::GitResetHardFetch => "git fetch + reset --hard origin/<branch>",
-            CommandSpec::RnReleaseBuild => "gradlew assembleRelease",
-            CommandSpec::AdbInstallApk => "adb install release APK",
-            CommandSpec::ShellCommand { .. } => "shell command",
-        }
+        self.meta().label
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

diff --git a/src/domain/refresh.rs b/src/domain/refresh.rs
index 9f5dfbf..2c2fca0 100644
--- a/src/domain/refresh.rs
+++ b/src/domain/refresh.rs
@@ -1,22 +1,22 @@
 //! Data dependency model: maps completed commands to required refreshes.
 //!
 //! `refresh_needed()` is a pure domain function — no I/O, no side effects.
 //! The app layer calls it after a command exits to determine which
 //! background refresh tasks to dispatch.

 use super::command::CommandSpec;

 /// Which background refreshes a completed command requires.
-#[derive(Debug, Clone, PartialEq, Eq)]
+#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
```
