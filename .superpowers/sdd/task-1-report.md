# Task 1: Golden Metadata Matrix Test

## Task Summary
Added `command_metadata_matrix` test to `src/domain/command.rs` to establish golden characterization of current `CommandSpec` behavior before refactoring.

## Test Execution

### Test Run Output
```
running 1 test
test domain::command::tests::command_metadata_matrix ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured
```

### Cargo Check
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.20s
```

Build successful with no errors.

## Test Coverage
The test validates all 19 `CommandSpec` variants against 5 predicate methods:
- `label()` — human-readable command name
- `is_destructive()` — requires explicit confirmation
- `is_cancellable()` — can be safely cancelled
- `collision_policy()` — BlockNew vs CancelPrevious per-variant behavior
- `refresh_needed()` — which background refreshes to trigger after command exits

### Variants Tested (19 total)
1. GitResetHard
2. GitPull
3. GitPush
4. GitFetch
5. GitResetHardFetch
6. RnCleanAndroid
7. RnCleanCocoapods
8. RmNodeModules
9. YarnInstall
10. YarnPodInstall
11. YarnUnitTests
12. YarnJest
13. YarnLint
14. YarnCheckTypes
15. UmpRunAndroid
16. UmpRunIos
17. RnReleaseBuild
18. AdbInstallApk
19. ShellCommand

## Commit Information
- Commit Hash: `1f87a7c`
- Branch: `remove-git-checkout-rebase-commands`
- Message: `test(command): golden metadata matrix as refactor safety net`
- Author: Claude Opus 4.8
- Files Changed: `src/domain/command.rs` (43 insertions)

## Key Facts
- Test is pure (no I/O, no tokio, no external dependencies)
- Test uses only types already in scope via `use super::*;`
- `RefreshSet` helpers (full, stale, none) reduce boilerplate
- Each assertion includes debug context for failure diagnosis
- Test passed immediately on current code (baseline established)
