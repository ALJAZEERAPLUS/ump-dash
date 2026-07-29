# Task 4 review package — range 79b15ab..8e5ec3c

## Commits
8e5ec3c test(command): drop drift-guard subsumed by meta() exhaustiveness + matrix

## Diff stat
 src/domain/command.rs | 97 ++++++---------------------------------------------
 1 file changed, 10 insertions(+), 87 deletions(-)

## Full diff (-U6)
```diff
diff --git a/src/domain/command.rs b/src/domain/command.rs
index 5e1a91d..a2d5b30 100644
--- a/src/domain/command.rs
+++ b/src/domain/command.rs
@@ -345,36 +345,29 @@ impl CommandSpec {

     /// Returns true for commands that cannot be undone and require explicit confirmation.
     pub fn is_destructive(&self) -> bool {
         self.meta().destructive
     }

-    /// Returns false for git-porcelain commands (data-integrity risk on cancellation);
-    /// true for all other commands (yarn, UMP runs, rm, adb, shell).
-    ///
-    /// REFACTOR-02: Type-driven cancellability. Git variants are closed by construction —
-    /// adding a new `Git*` variant requires explicit opt-in here (compile-error would be
-    /// ideal; today this is a flat-enum predicate per AUDIT-ADDENDUM F-501 DEFERRED decision).
+    /// Returns false for git-porcelain commands (data-integrity risk on
+    /// cancellation); true for all other commands (yarn, UMP runs, rm, adb,
+    /// shell). The value comes from `meta()`; adding a new variant forces an
+    /// explicit decision there (exhaustive match, no `_` arm).
     pub fn is_cancellable(&self) -> bool {
         self.meta().cancellable
     }

     /// Returns the per-variant collision policy applied when a new dispatch
     /// matches a running task on the same `(discriminant, WorktreeId)` per
-    /// D-05.
-    ///
-    /// `BlockNew` for idempotent installs and non-cancellable git variants
-    /// (Q-4); `CancelPrevious` for builds, tests, runs, and clean operations
-    /// where "run THIS version NOW" is the intent.
+    /// D-05. `BlockNew` for idempotent installs and non-cancellable git
+    /// variants (Q-4); `CancelPrevious` for builds, tests, runs, and clean
+    /// operations where "run THIS version NOW" is the intent.
     ///
-    /// TASK-05 / 15-RESEARCH §F6. The match is intentionally exhaustive (NO
-    /// `_ =>` arm) so adding a new `CommandSpec` variant produces a compile
-    /// error here, forcing the maintainer to assign a policy explicitly
-    /// (T-15-04-01 mitigation). The drift-guard meta-test
-    /// `collision_policy_covers_every_variant` provides a second layer of
-    /// enforcement.
+    /// The value comes from `meta()`, whose exhaustive match (no `_` arm) is
+    /// the compile-time drift guard — a new variant cannot be added without
+    /// assigning it a policy there.
     pub fn collision_policy(&self) -> CollisionPolicy {
         self.meta().collision
     }

     /// Returns true for commands that need a user-supplied text string before running.
     ///
@@ -795,82 +788,12 @@ mod tests {
                 "git variant {:?} must BlockNew (non-cancellable cannot CancelPrevious)",
                 spec
             );
         }
     }

-    /// Drift-guard meta-test: mirrors the predicate body with an exhaustive
-    /// match (no `_` arm). Adding a new CommandSpec variant fails to compile here
-    /// AND in `collision_policy()` itself — two layers of compile-time enforcement
-    /// against silent default assignment (mitigates T-15-04-01).
-    #[test]
-    fn collision_policy_covers_every_variant() {
-        // One instance of every CommandSpec variant; if a variant is added in a
-        // future phase, this match becomes non-exhaustive and the build fails.
-        let variants = [
-            CommandSpec::GitResetHard,
-            CommandSpec::GitPull,
-            CommandSpec::GitPush,
-            CommandSpec::RnCleanAndroid,
-            CommandSpec::RnCleanCocoapods,
-            CommandSpec::RmNodeModules,
-            CommandSpec::YarnInstall,
-            CommandSpec::YarnPodInstall,
-            CommandSpec::UmpRunAndroid {
-                device_id: "".into(),
-                variant: Some(RunVariant::Local),
-            },
-            CommandSpec::UmpRunIos {
-                device_id: "".into(),
-                variant: Some(RunVariant::Dev),
-            },
-            CommandSpec::YarnUnitTests,
-            CommandSpec::YarnJest { filter: "".into() },
-            CommandSpec::YarnLint,
-            CommandSpec::YarnCheckTypes,
-            CommandSpec::GitFetch,
-            CommandSpec::GitResetHardFetch,
-            CommandSpec::RnReleaseBuild,
-            CommandSpec::AdbInstallApk,
-            CommandSpec::ShellCommand { command: "".into() },
-        ];
-        for v in &variants {
-            // Exhaustive match — no `_ =>` arm. Mirrors `collision_policy()` body.
-            let _policy: CollisionPolicy = match v {
-                CommandSpec::GitResetHard
-                | CommandSpec::GitResetHardFetch
-                | CommandSpec::GitPull
-                | CommandSpec::GitPush
-                | CommandSpec::GitFetch
-                | CommandSpec::YarnInstall
-                | CommandSpec::YarnPodInstall => CollisionPolicy::BlockNew,
-                CommandSpec::YarnUnitTests
-                | CommandSpec::YarnJest { .. }
-                | CommandSpec::YarnLint
-                | CommandSpec::YarnCheckTypes
-                | CommandSpec::UmpRunAndroid { .. }
-                | CommandSpec::UmpRunIos { .. }
-                | CommandSpec::RnReleaseBuild
-                | CommandSpec::AdbInstallApk
-                | CommandSpec::ShellCommand { .. }
-                | CommandSpec::RnCleanAndroid
-                | CommandSpec::RnCleanCocoapods
-                | CommandSpec::RmNodeModules => CollisionPolicy::CancelPrevious,
-            };
-            // Also assert the predicate agrees with the local mirror.
-            assert!(matches!(
-                v.collision_policy(),
-                CollisionPolicy::BlockNew | CollisionPolicy::CancelPrevious
-            ));
-        }
-        assert_eq!(
-            variants.len(),
-            19,
-            "must enumerate all 19 CommandSpec variants"
-        );
-    }

     #[test]
     fn command_metadata_matrix() {
         use crate::domain::refresh::{refresh_needed, RefreshSet};
         use CollisionPolicy::{BlockNew, CancelPrevious};

```
