# Phase 2a final review — range e63ff01..f74ceb4 (deps + resolve + migration)

## Commits
f74ceb4 refactor(pipeline): drop dead Recipe::SyncThenRun, superseded by resolve()
0bf86c5 refactor(update): dispatch run/build prereqs via resolve() instead of SyncThenRun
22e7952 feat(pipeline): pure resolve() flattens command deps (matches SyncThenRun)
4310830 feat(command): declare prerequisite deps on the info-card

## Diff stat
 src/app/update.rs      |  18 +++----
 src/domain/command.rs  |  58 +++++++++++++--------
 src/domain/pipeline.rs | 135 ++++++++++++++++++++++++++++++-------------------
 3 files changed, 129 insertions(+), 82 deletions(-)

## Full diff (-U10)
```diff
diff --git a/src/app/update.rs b/src/app/update.rs
index 6d0cd42..057126c 100644
--- a/src/app/update.rs
+++ b/src/app/update.rs
@@ -17,38 +17,38 @@ use super::state::{
     PendingCachedIosRun, active_output, active_worktree_id,
 };
 use crate::domain::action::Action;
 use crate::domain::agent_protocol::{
     AgentOutcome, AgentRequest, BlockReason, MetroReport, TaskStatusReport, WorktreeStatusReport,
 };
 use crate::domain::command::{
     CleanOptions, CollisionPolicy, CommandSpec, ModalState, RunVariant, android_avd_name,
     android_boot_avd_command,
 };
-use crate::domain::pipeline::{DependencyState, Recipe};
+use crate::domain::pipeline::{resolve, DependencyState, Recipe};
 use crate::domain::ports::device_port::DeviceKind;
 use crate::domain::review::{PullRequest, PullRequestFilter};
 use crate::domain::task::ExitStatus;
 use crate::domain::worktree::{Worktree, WorktreeId, WorktreeMetroStatus};
 use crate::domain::worktree_slice::LastRunConfig;
 use std::path::{Path, PathBuf};

 // Plan 13-09 (F-204 consumer): the 11 inline prereq sites are rewritten to
 // build a `Recipe` and call `Recipe::expand(&deps)` — the dispatcher never
 // inlines prereq ordering. The boolean coordination flags
 // `pending_metro_run`, `pending_metro_after_sync`, and `pending_switch_path`
 // are deleted; their semantics are absorbed into the command_queue front-push
 // pattern (deferred run waits for metro Ready), `post_drain_action` (post-
 // queue-drain action), and direct `active_worktree_path` updates respectively.
 //
 // Site → Recipe variant mapping:
-//   SyncBeforeRunAccept (auto-sync fast path + modal accept) → Recipe::SyncThenRun
+//   SyncBeforeRunAccept (auto-sync fast path + modal accept) → resolve()
 //   SyncBeforeMetroAccept (modal accept) + WorktreeSwitch auto-sync → Recipe::SyncThenStartMetro
 //   CleanConfirm                                                    → Recipe::Clean
 //   RnReleaseBuild dispatch                                         → Recipe::ReleaseBuildAndInstall
 //   GitResetHardFetch dispatch                                      → Recipe::GitFetchThenReset
 //   needs_metro pre-dispatch (3 sites: CommandRun / CommandExited drain / SyncBeforeRunDecline)
 //                                                                   → command_queue.push_front + MetroStart

 const DEFAULT_METRO_PORT: u16 = 8081;

 fn metro_worktree_id_from_path(path: &Path) -> String {
@@ -548,21 +548,21 @@ fn dispatch_command_for_worktree(
         cwd: wt.path.clone(),
         branch: wt.branch.clone(),
         repo_root: state.app_config.repo_root.clone(),
     })
 }

 // ---------------------------------------------------------------------------
 // MCP agent request handling (Action::Agent).
 //
 // Pure — reuses the same dispatch primitives the keyboard path uses
-// (`dispatch_command_for_worktree` collision gate, `Recipe::SyncThenRun`,
+// (`dispatch_command_for_worktree` collision gate, `resolve()`,
 // `ensure_metro_for_worktree`) so every collision / dependency / lock decision
 // is shared. Unlike the keyboard path these are worktree-TARGETED (the agent's
 // own worktree, not the UI selection) and orphan-safe: a request that arrives
 // while a different task is running is queued, never dispatched over the live
 // task. See src/domain/agent_protocol.rs.
 // ---------------------------------------------------------------------------

 /// Resolve an agent's working directory to a known worktree. Pure: matches the
 /// cwd against the loaded worktree set (longest path prefix wins, mirroring the
 /// path-derived WorktreeId in src/infra/worktrees.rs).
@@ -933,21 +933,21 @@ fn handle_agent_request(
             let seq = Recipe::Clean(opts).expand(&DependencyState::new(false, false, false));
             if seq.is_empty() {
                 return AgentOutcome::Error {
                     message: "no clean targets selected".into(),
                 };
             }
             agent_enqueue(state, effects, wt_id, seq, false)
         }
         AgentRequest::Build { install } => {
             let deps = agent_deps_for(state, wt_id, false);
-            let mut seq = Recipe::SyncThenRun(CommandSpec::RnReleaseBuild).expand(&deps);
+            let mut seq = resolve(CommandSpec::RnReleaseBuild, &deps);
             if install {
                 seq.push(CommandSpec::AdbInstallApk);
             }
             agent_enqueue(state, effects, wt_id, seq, false)
         }
         AgentRequest::RunIos {
             device_id,
             variant,
         } => {
             if device_id.is_empty() {
@@ -1255,21 +1255,21 @@ fn dispatch_run(
                 spec.label()
             )],
         };
     }
     let (is_ios, device_id) = match &spec {
         CommandSpec::UmpRunIos { device_id, .. } => (true, device_id.clone()),
         CommandSpec::UmpRunAndroid { device_id, .. } => (false, device_id.clone()),
         _ => (false, String::new()),
     };
     let deps = agent_deps_for(state, wt_id, is_ios);
-    let mut seq = Recipe::SyncThenRun(spec).expand(&deps);
+    let mut seq = resolve(spec, &deps);
     // Cold-boot a stopped emulator before the run (parity with the UI).
     if !is_ios && let Some(avd) = android_avd_name(&device_id) {
         let boot = CommandSpec::ShellCommand {
             command: android_boot_avd_command(avd),
         };
         let run_idx = seq.len().saturating_sub(1);
         seq.insert(run_idx, boot);
     }
     agent_enqueue(state, effects, wt_id, seq, false)
 }
@@ -1961,25 +1961,25 @@ pub fn update(state: &mut AppState, action: Action) -> Vec<Effect> {
                     false
                 };

                 if *yarn_stale || pods_stale {
                     if state
                         .app_config
                         .config
                         .as_ref()
                         .is_some_and(|c| c.auto_sync)
                     {
-                        // Plan 13-09 (F-204 site 1): Recipe::SyncThenRun replaces
+                        // Plan 13-09 (F-204 site 1): resolve() replaces
                         // the inline yarn/pod sequencing. Auto-sync fast path —
                         // skip the modal, expand the recipe, queue + dispatch.
                         let deps = DependencyState::new(*yarn_stale, pods_stale, is_ios);
-                        let mut sequence = Recipe::SyncThenRun(spec).expand(&deps);
+                        let mut sequence = resolve(spec, &deps);
                         let first = sequence.remove(0);

                         // D-12: push to slice queue for the originating worktree.
                         let resolved_id = active_worktree_id(state);
                         if let Some(ref wt_id) = resolved_id {
                             let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                                 crate::domain::worktree_slice::WorktreeSlice {
                                     id: wt_id.clone(),
                                     ..Default::default()
                                 }
@@ -3460,29 +3460,29 @@ pub fn update(state: &mut AppState, action: Action) -> Vec<Effect> {
                 }
             }
         }
         Action::SyncBeforeRunAccept => {
             if let Some(ModalState::SyncBeforeRun {
                 run_command,
                 needs_yarn,
                 needs_pods,
             }) = state.modal_stack.modal.take()
             {
-                // Plan 13-09 (F-204 site 7): Recipe::SyncThenRun expansion.
+                // Plan 13-09 (F-204 site 7): resolve() expansion.
                 // The modal already encodes the staleness decision in
                 // (needs_yarn, needs_pods); rebuild a DependencyState that
                 // reproduces the same expansion. The is_ios_target flag is
                 // derived from needs_pods being meaningful — only iOS run
                 // commands ever set needs_pods=true at the modal-construction
                 // site (CommandRun stale check).
                 let deps = DependencyState::new(needs_yarn, needs_pods, needs_pods);
-                let mut sequence = Recipe::SyncThenRun(*run_command).expand(&deps);
+                let mut sequence = resolve(*run_command, &deps);

                 // Guaranteed non-empty: we only get here from the modal which only
                 // appears when needs_yarn || needs_pods, so sequence has ≥2 elements.
                 let first = sequence.remove(0);

                 // D-12: push to slice queue for the originating worktree.
                 let resolved_id = active_worktree_id(state);
                 if let Some(ref wt_id) = resolved_id {
                     let slice = state.worktrees.entry(wt_id.clone()).or_insert_with(|| {
                         crate::domain::worktree_slice::WorktreeSlice {
diff --git a/src/domain/command.rs b/src/domain/command.rs
index a2d5b30..dad3d8f 100644
--- a/src/domain/command.rs
+++ b/src/domain/command.rs
@@ -179,32 +179,34 @@ pub enum CollisionPolicy {
     BlockNew,
     /// The existing task is aborted, then the new task is dispatched. Used for
     /// builds, tests, and runs where the user intent is "run THIS version NOW"
     /// — re-running a test or app build should reflect the latest sources.
     CancelPrevious,
 }

 /// All of a command's *static* facts in one place — the single source of truth.
 /// Behaviour that depends on field values (argv, needs_text_input) or on runtime
 /// state stays as separate methods; this card holds only discriminant-pure facts.
-#[derive(Debug, Clone, Copy, PartialEq, Eq)]
+#[derive(Debug, Clone, Copy, PartialEq)]
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
+    /// Commands that must be satisfied before this one (the dependency graph).
+    pub deps: &'static [CommandSpec],
 }

 impl CommandSpec {
     /// Returns the argv that should be passed to `tokio::process::Command`.
     /// The first element is the program; the rest are arguments.
     pub fn to_argv(&self) -> Vec<String> {
         match self {
             CommandSpec::GitResetHard => {
                 vec!["git".into(), "reset".into(), "--hard".into(), "HEAD".into()]
             }
@@ -314,39 +316,39 @@ impl CommandSpec {

     /// The single source of truth for this command's static facts. The
     /// exhaustive match (no `_` arm) is the compile-time drift guard — adding a
     /// `CommandSpec` variant fails to compile here until its card is filled in.
     pub fn meta(&self) -> CommandMeta {
         let full = RefreshSet { worktrees: true, staleness: true, jira_titles: true };
         let stale = RefreshSet { worktrees: false, staleness: true, jira_titles: false };
         let none = RefreshSet::none();
         use CollisionPolicy::{BlockNew, CancelPrevious};
         match self {
-            CommandSpec::GitResetHard => CommandMeta { label: "git reset --hard HEAD", destructive: true, cancellable: false, refresh: full, collision: BlockNew },
-            CommandSpec::GitPull => CommandMeta { label: "git pull", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
-            CommandSpec::GitPush => CommandMeta { label: "git push", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
-            CommandSpec::GitFetch => CommandMeta { label: "git fetch --all --tags", destructive: false, cancellable: false, refresh: none, collision: BlockNew },
-            CommandSpec::GitResetHardFetch => CommandMeta { label: "git fetch + reset --hard origin/<branch>", destructive: true, cancellable: false, refresh: full, collision: BlockNew },
-            CommandSpec::RnCleanAndroid => CommandMeta { label: "Clean Android (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
-            CommandSpec::RnCleanCocoapods => CommandMeta { label: "Clean CocoaPods (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
-            CommandSpec::RmNodeModules => CommandMeta { label: "Remove node_modules", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious },
-            CommandSpec::YarnInstall => CommandMeta { label: "yarn install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew },
-            CommandSpec::YarnPodInstall => CommandMeta { label: "yarn pod-install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew },
-            CommandSpec::YarnUnitTests => CommandMeta { label: "yarn unit-tests", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
-            CommandSpec::YarnJest { .. } => CommandMeta { label: "yarn jest <filter>", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
-            CommandSpec::YarnLint => CommandMeta { label: "yarn lint --quiet --fix", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
-            CommandSpec::YarnCheckTypes => CommandMeta { label: "yarn check-types --incremental", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
-            CommandSpec::UmpRunAndroid { .. } => CommandMeta { label: "Run Android (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
-            CommandSpec::UmpRunIos { .. } => CommandMeta { label: "Run iOS (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
-            CommandSpec::RnReleaseBuild => CommandMeta { label: "gradlew assembleRelease", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
-            CommandSpec::AdbInstallApk => CommandMeta { label: "adb install release APK", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
-            CommandSpec::ShellCommand { .. } => CommandMeta { label: "shell command", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious },
+            CommandSpec::GitResetHard => CommandMeta { label: "git reset --hard HEAD", destructive: true, cancellable: false, refresh: full, collision: BlockNew, deps: &[] },
+            CommandSpec::GitPull => CommandMeta { label: "git pull", destructive: false, cancellable: false, refresh: none, collision: BlockNew, deps: &[] },
+            CommandSpec::GitPush => CommandMeta { label: "git push", destructive: false, cancellable: false, refresh: none, collision: BlockNew, deps: &[] },
+            CommandSpec::GitFetch => CommandMeta { label: "git fetch --all --tags", destructive: false, cancellable: false, refresh: none, collision: BlockNew, deps: &[] },
+            CommandSpec::GitResetHardFetch => CommandMeta { label: "git fetch + reset --hard origin/<branch>", destructive: true, cancellable: false, refresh: full, collision: BlockNew, deps: &[] },
+            CommandSpec::RnCleanAndroid => CommandMeta { label: "Clean Android (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious, deps: &[] },
+            CommandSpec::RnCleanCocoapods => CommandMeta { label: "Clean CocoaPods (react-native clean)", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious, deps: &[] },
+            CommandSpec::RmNodeModules => CommandMeta { label: "Remove node_modules", destructive: true, cancellable: true, refresh: stale, collision: CancelPrevious, deps: &[] },
+            CommandSpec::YarnInstall => CommandMeta { label: "yarn install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew, deps: &[] },
+            CommandSpec::YarnPodInstall => CommandMeta { label: "yarn pod-install", destructive: false, cancellable: true, refresh: stale, collision: BlockNew, deps: &[CommandSpec::YarnInstall] },
+            CommandSpec::YarnUnitTests => CommandMeta { label: "yarn unit-tests", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[] },
+            CommandSpec::YarnJest { .. } => CommandMeta { label: "yarn jest <filter>", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[] },
+            CommandSpec::YarnLint => CommandMeta { label: "yarn lint --quiet --fix", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[] },
+            CommandSpec::YarnCheckTypes => CommandMeta { label: "yarn check-types --incremental", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[] },
+            CommandSpec::UmpRunAndroid { .. } => CommandMeta { label: "Run Android (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[CommandSpec::YarnInstall] },
+            CommandSpec::UmpRunIos { .. } => CommandMeta { label: "Run iOS (UMP)", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[CommandSpec::YarnPodInstall] },
+            CommandSpec::RnReleaseBuild => CommandMeta { label: "gradlew assembleRelease", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[CommandSpec::YarnInstall] },
+            CommandSpec::AdbInstallApk => CommandMeta { label: "adb install release APK", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[] },
+            CommandSpec::ShellCommand { .. } => CommandMeta { label: "shell command", destructive: false, cancellable: true, refresh: none, collision: CancelPrevious, deps: &[] },
         }
     }

     /// Returns true for commands that cannot be undone and require explicit confirmation.
     pub fn is_destructive(&self) -> bool {
         self.meta().destructive
     }

     /// Returns false for git-porcelain commands (data-integrity risk on
     /// cancellation); true for all other commands (yarn, UMP runs, rm, adb,
@@ -785,20 +787,36 @@ mod tests {
             assert_eq!(
                 spec.collision_policy(),
                 CollisionPolicy::BlockNew,
                 "git variant {:?} must BlockNew (non-cancellable cannot CancelPrevious)",
                 spec
             );
         }
     }


+    #[test]
+    fn command_deps_graph() {
+        assert_eq!(CommandSpec::YarnInstall.meta().deps, &[]);
+        assert_eq!(CommandSpec::YarnPodInstall.meta().deps, &[CommandSpec::YarnInstall]);
+        assert_eq!(
+            CommandSpec::UmpRunIos { device_id: String::new(), variant: None }.meta().deps,
+            &[CommandSpec::YarnPodInstall]
+        );
+        assert_eq!(
+            CommandSpec::UmpRunAndroid { device_id: String::new(), variant: None }.meta().deps,
+            &[CommandSpec::YarnInstall]
+        );
+        assert_eq!(CommandSpec::RnReleaseBuild.meta().deps, &[CommandSpec::YarnInstall]);
+        assert_eq!(CommandSpec::GitPull.meta().deps, &[]);
+    }
+
     #[test]
     fn command_metadata_matrix() {
         use crate::domain::refresh::{refresh_needed, RefreshSet};
         use CollisionPolicy::{BlockNew, CancelPrevious};

         let full = RefreshSet { worktrees: true, staleness: true, jira_titles: true };
         let stale = RefreshSet { worktrees: false, staleness: true, jira_titles: false };
         let none = RefreshSet::none();

         // (spec, label, destructive, cancellable, collision, refresh)
diff --git a/src/domain/pipeline.rs b/src/domain/pipeline.rs
index b086cb7..d041fb6 100644
--- a/src/domain/pipeline.rs
+++ b/src/domain/pipeline.rs
@@ -30,41 +30,50 @@ pub enum Prerequisite {
 impl CommandSpec {
     /// Derive prerequisites from the variant. Replaces the per-site `needs_metro()`
     /// inline checks — callers should prefer `prerequisites()` going forward.
     ///
     /// `needs_metro()` stays as a thin wrapper for backward compatibility.
     pub fn prerequisites(&self) -> Vec<Prerequisite> {
         match self {
             CommandSpec::UmpRunAndroid { .. }
             | CommandSpec::UmpRunIos { .. }
             | CommandSpec::RnReleaseBuild => vec![Prerequisite::MetroRunning],
-            // Sync prerequisites come from Recipe::SyncThenRun — not per-variant.
+            // Sync prerequisites come from the dependency graph (`deps()` +
+            // `resolve()`), not from this metro-only prerequisite list.
             _ => vec![],
         }
     }
+
+    /// True when this command is already satisfied for the given worktree state,
+    /// so it can be skipped when it appears as a prerequisite. Goals (run/build)
+    /// are never "satisfied" — they always run.
+    pub fn is_satisfied(&self, ctx: &DependencyState) -> bool {
+        match self {
+            CommandSpec::YarnInstall => !ctx.stale_yarn,
+            CommandSpec::YarnPodInstall => !ctx.stale_pods,
+            _ => false,
+        }
+    }
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
-    /// Front-load yarn/pod sync steps if stale, then run the given command.
-    /// iOS-only for pods per F-204 rule.
-    SyncThenRun(CommandSpec),
     /// Front-load yarn if stale, then start metro.
     /// Native pods are handled by native run/build recipes, not Metro.
     SyncThenStartMetro,
     /// `RnReleaseBuild` followed by `AdbInstallApk`.
     ReleaseBuildAndInstall,
     /// `GitFetch` followed by `GitResetHard`.
     GitFetchThenReset,
 }

 /// Staleness snapshot used by `Recipe::expand` to decide which sync commands
@@ -113,48 +122,56 @@ impl Recipe {
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
-            Recipe::SyncThenRun(cmd) => {
-                let mut v = Vec::new();
-                if deps.stale_yarn {
-                    v.push(CommandSpec::YarnInstall);
-                }
-                if deps.stale_pods && deps.is_ios_target {
-                    v.push(CommandSpec::YarnPodInstall);
-                }
-                v.push(cmd.clone());
-                v
-            }
             Recipe::SyncThenStartMetro => {
                 let mut v = Vec::new();
                 if deps.stale_yarn {
                     v.push(CommandSpec::YarnInstall);
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

+/// Flatten `goal` and its unsatisfied transitive prerequisites into the order
+/// they must run: dependencies first (post-order), de-duplicated, with the goal
+/// last. Pure — no I/O. This is how run/build commands front-load their yarn/pod
+/// sync steps.
+pub fn resolve(goal: CommandSpec, ctx: &DependencyState) -> Vec<CommandSpec> {
+    fn collect(cmd: &CommandSpec, ctx: &DependencyState, out: &mut Vec<CommandSpec>) {
+        for dep in cmd.meta().deps {
+            collect(dep, ctx, out);
+            if !dep.is_satisfied(ctx) && !out.contains(dep) {
+                out.push(dep.clone());
+            }
+        }
+    }
+    let mut out = Vec::new();
+    collect(&goal, ctx, &mut out);
+    out.push(goal);
+    out
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;

     fn fresh_deps() -> DependencyState {
         DependencyState {
             stale_yarn: false,
             stale_pods: false,
             is_ios_target: false,
         }
@@ -214,58 +231,20 @@ mod tests {
     }

     #[test]
     fn test_clean_none_expands_to_empty() {
         assert_eq!(
             Recipe::Clean(CleanOptions::default()).expand(&fresh_deps()),
             Vec::<CommandSpec>::new()
         );
     }

-    #[test]
-    fn test_sync_then_run_stale_ios_adds_yarn_and_pods() {
-        let run_cmd = CommandSpec::UmpRunIos {
-            device_id: "udid-1".into(),
-            variant: Some(crate::domain::command::RunVariant::Local),
-        };
-        assert_eq!(
-            Recipe::SyncThenRun(run_cmd.clone()).expand(&stale_deps_ios()),
-            vec![
-                CommandSpec::YarnInstall,
-                CommandSpec::YarnPodInstall,
-                run_cmd
-            ]
-        );
-    }
-
-    #[test]
-    fn test_sync_then_run_stale_android_only_yarn() {
-        // Pods are skipped on Android (is_ios_target = false) — F-204 rule.
-        let run_cmd = CommandSpec::UmpRunAndroid {
-            device_id: "emulator-5554".into(),
-            variant: Some(crate::domain::command::RunVariant::Local),
-        };
-        assert_eq!(
-            Recipe::SyncThenRun(run_cmd.clone()).expand(&stale_deps_android()),
-            vec![CommandSpec::YarnInstall, run_cmd]
-        );
-    }
-
-    #[test]
-    fn test_sync_then_run_fresh_passes_through() {
-        let run_cmd = CommandSpec::YarnLint;
-        assert_eq!(
-            Recipe::SyncThenRun(run_cmd.clone()).expand(&fresh_deps()),
-            vec![run_cmd]
-        );
-    }
-
     #[test]
     fn test_sync_then_start_metro_stale_adds_only_yarn() {
         // Metro start path only needs JS dependencies. Native pods are handled
         // by native run/build recipes, not by Metro itself.
         assert_eq!(
             Recipe::SyncThenStartMetro.expand(&stale_deps_android()),
             vec![CommandSpec::YarnInstall]
         );
     }

@@ -327,11 +306,61 @@ mod tests {
         );
     }

     #[test]
     fn test_prerequisites_git_fetch_no_prereq() {
         assert_eq!(
             CommandSpec::GitFetch.prerequisites(),
             Vec::<Prerequisite>::new()
         );
     }
+
+    fn ios_spec() -> CommandSpec {
+        CommandSpec::UmpRunIos {
+            device_id: "d".into(),
+            variant: Some(crate::domain::command::RunVariant::Local),
+        }
+    }
+
+    fn android_spec() -> CommandSpec {
+        CommandSpec::UmpRunAndroid {
+            device_id: "e".into(),
+            variant: Some(crate::domain::command::RunVariant::Local),
+        }
+    }
+
+    #[test]
+    fn resolve_ios_stale_matches_sync_then_run() {
+        assert_eq!(
+            resolve(ios_spec(), &stale_deps_ios()),
+            vec![CommandSpec::YarnInstall, CommandSpec::YarnPodInstall, ios_spec()]
+        );
+    }
+
+    #[test]
+    fn resolve_android_stale_skips_pods() {
+        assert_eq!(
+            resolve(android_spec(), &stale_deps_android()),
+            vec![CommandSpec::YarnInstall, android_spec()]
+        );
+    }
+
+    #[test]
+    fn resolve_release_build_stale_adds_yarn() {
+        assert_eq!(
+            resolve(CommandSpec::RnReleaseBuild, &stale_deps_android()),
+            vec![CommandSpec::YarnInstall, CommandSpec::RnReleaseBuild]
+        );
+    }
+
+    #[test]
+    fn resolve_fresh_is_goal_only() {
+        assert_eq!(resolve(ios_spec(), &fresh_deps()), vec![ios_spec()]);
+    }
+
+    #[test]
+    fn resolve_ios_yarn_stale_pods_fresh_adds_only_yarn() {
+        let ctx = DependencyState::new(true, false, true);
+        assert_eq!(resolve(ios_spec(), &ctx), vec![CommandSpec::YarnInstall, ios_spec()]);
+    }
+
 }
```
