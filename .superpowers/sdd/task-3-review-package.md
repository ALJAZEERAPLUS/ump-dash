# Task 3 review package — range 11ba416..79b15ab

## Commits
79b15ab refactor(refresh): refresh_needed reads from CommandMeta

## Diff stat
 src/domain/refresh.rs | 24 +-----------------------
 1 file changed, 1 insertion(+), 23 deletions(-)

## Full diff (-U15)
```diff
diff --git a/src/domain/refresh.rs b/src/domain/refresh.rs
index 2c2fca0..3ed7227 100644
--- a/src/domain/refresh.rs
+++ b/src/domain/refresh.rs
@@ -28,53 +28,31 @@ impl RefreshSet {
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
-    match cmd {
-        // Branch-changing git ops -> full reload + JIRA re-fetch
-        CommandSpec::GitResetHard
-        | CommandSpec::GitResetHardFetch => RefreshSet {
-            worktrees: true,
-            staleness: true,
-            jira_titles: true,
-        },
-        // Non-branch-changing git ops -> no refresh
-        CommandSpec::GitPull | CommandSpec::GitPush | CommandSpec::GitFetch => RefreshSet::none(),
-        // Install/clean -> staleness refresh only
-        CommandSpec::YarnInstall
-        | CommandSpec::YarnPodInstall
-        | CommandSpec::RmNodeModules
-        | CommandSpec::RnCleanAndroid
-        | CommandSpec::RnCleanCocoapods => RefreshSet {
-            worktrees: false,
-            staleness: true,
-            jira_titles: false,
-        },
-        // Everything else -> no refresh
-        _ => RefreshSet::none(),
-    }
+    cmd.meta().refresh
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
```
