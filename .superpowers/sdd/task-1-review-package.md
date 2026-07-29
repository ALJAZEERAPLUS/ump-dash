# Task 1 review package — range 5a2a37a..1f87a7c

## Commits
1f87a7c test(command): golden metadata matrix as refactor safety net

## Diff stat
 src/domain/command.rs | 43 +++++++++++++++++++++++++++++++++++++++++++
 1 file changed, 43 insertions(+)

## Full diff (-U10)
```diff
diff --git a/src/domain/command.rs b/src/domain/command.rs
index 38cdb9e..de83c15 100644
--- a/src/domain/command.rs
+++ b/src/domain/command.rs
@@ -872,11 +872,54 @@ mod tests {
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
+
+    #[test]
+    fn command_metadata_matrix() {
+        use crate::domain::refresh::{refresh_needed, RefreshSet};
+        use CollisionPolicy::{BlockNew, CancelPrevious};
+
+        let full = RefreshSet { worktrees: true, staleness: true, jira_titles: true };
+        let stale = RefreshSet { worktrees: false, staleness: true, jira_titles: false };
+        let none = RefreshSet::none();
+
+        // (spec, label, destructive, cancellable, collision, refresh)
+        let cases: Vec<(CommandSpec, &str, bool, bool, CollisionPolicy, RefreshSet)> = vec![
+            (CommandSpec::GitResetHard, "git reset --hard HEAD", true, false, BlockNew, full.clone()),
+            (CommandSpec::GitPull, "git pull", false, false, BlockNew, none.clone()),
+            (CommandSpec::GitPush, "git push", false, false, BlockNew, none.clone()),
+            (CommandSpec::GitFetch, "git fetch --all --tags", false, false, BlockNew, none.clone()),
+            (CommandSpec::GitResetHardFetch, "git fetch + reset --hard origin/<branch>", true, false, BlockNew, full.clone()),
+            (CommandSpec::RnCleanAndroid, "Clean Android (react-native clean)", true, true, CancelPrevious, stale.clone()),
+            (CommandSpec::RnCleanCocoapods, "Clean CocoaPods (react-native clean)", true, true, CancelPrevious, stale.clone()),
+            (CommandSpec::RmNodeModules, "Remove node_modules", true, true, CancelPrevious, stale.clone()),
+            (CommandSpec::YarnInstall, "yarn install", false, true, BlockNew, stale.clone()),
+            (CommandSpec::YarnPodInstall, "yarn pod-install", false, true, BlockNew, stale.clone()),
+            (CommandSpec::YarnUnitTests, "yarn unit-tests", false, true, CancelPrevious, none.clone()),
+            (CommandSpec::YarnJest { filter: String::new() }, "yarn jest <filter>", false, true, CancelPrevious, none.clone()),
+            (CommandSpec::YarnLint, "yarn lint --quiet --fix", false, true, CancelPrevious, none.clone()),
+            (CommandSpec::YarnCheckTypes, "yarn check-types --incremental", false, true, CancelPrevious, none.clone()),
+            (CommandSpec::UmpRunAndroid { device_id: String::new(), variant: None }, "Run Android (UMP)", false, true, CancelPrevious, none.clone()),
+            (CommandSpec::UmpRunIos { device_id: String::new(), variant: None }, "Run iOS (UMP)", false, true, CancelPrevious, none.clone()),
+            (CommandSpec::RnReleaseBuild, "gradlew assembleRelease", false, true, CancelPrevious, none.clone()),
+            (CommandSpec::AdbInstallApk, "adb install release APK", false, true, CancelPrevious, none.clone()),
+            (CommandSpec::ShellCommand { command: String::new() }, "shell command", false, true, CancelPrevious, none.clone()),
+        ];
+
+        assert_eq!(cases.len(), 19, "matrix must cover all 19 CommandSpec variants");
+
+        for (spec, label, destructive, cancellable, collision, refresh) in &cases {
+            assert_eq!(spec.label(), *label, "label mismatch for {spec:?}");
+            assert_eq!(spec.is_destructive(), *destructive, "is_destructive mismatch for {spec:?}");
+            assert_eq!(spec.is_cancellable(), *cancellable, "is_cancellable mismatch for {spec:?}");
+            assert_eq!(spec.collision_policy(), *collision, "collision_policy mismatch for {spec:?}");
+            assert_eq!(refresh_needed(spec), *refresh, "refresh mismatch for {spec:?}");
+        }
+    }
 }
```
