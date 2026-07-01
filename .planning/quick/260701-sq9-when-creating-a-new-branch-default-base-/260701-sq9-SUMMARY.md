---
status: complete
quick_id: 260701-sq9
slug: when-creating-a-new-branch-default-base
completed: 2026-07-01
commit: 84f618c
---

# Quick Task 260701-sq9 Summary

## Result

New-branch worktree base selection now defaults to `origin/rc-trunk` when that branch is present, while the existing-branch checkout flow still defaults to the first loaded branch.

## Changes

- Added `PREFERRED_NEW_BRANCH_BASE` and `preferred_new_branch_base_index()` in `src/domain/command.rs` as pure domain policy.
- Updated `Action::BranchesLoaded` in `src/app/update.rs` to use that helper only when `pending_new_branch_worktree` is active.
- Added reducer regression coverage in `src/app/dispatch_tests.rs` for pressing Enter immediately in the new-branch base picker.

## TDD Evidence

- RED: `cargo test app::dispatch_tests::new_branch_base_picker_enter_defaults_to_origin_rc_trunk -- --exact` failed before production changes with `left: Some("origin/main")` and `right: Some("origin/rc-trunk")`.
- GREEN: the same focused test passed after the domain helper and reducer selection change.

## Verification

| Command | Result |
| --- | --- |
| `cargo test app::dispatch_tests::new_branch_base_picker_enter_defaults_to_origin_rc_trunk -- --exact` | Pass - 1 test passed |
| `cargo test app::dispatch_tests::branch_picker` | Pass - command completed, but matched 0 tests in this crate |
| `cargo test branch_picker` | Pass - 4 tests passed |
| `make arch-lint` | Pass - `arch-lint: PASS` with existing documented G-03 pending note |
| `cargo test` | Pass - 367 lib tests plus main, integration, and doc-test targets passed |
| `cargo clippy --all-targets -- -D warnings` | Pass |
| `make arch-report` | Pass - `arch_report_status=pass` |

## Notes

- No `ROADMAP.md` or `.planning/STATE.md` updates were made.
- Code/test changes were committed atomically in `84f618c`.
- The requested summary file is left uncommitted for the orchestrator to handle.
- An earlier pre-final `cargo test` run failed once in `infra::native_cache::tests::android_lookup_errors_when_cached_apk_missing`; that test passed when isolated, and the final full `cargo test` rerun passed.

## Self-Check: PASSED

- Summary file exists at `.planning/quick/260701-sq9-when-creating-a-new-branch-default-base-/260701-sq9-SUMMARY.md`.
- Commit `84f618c` exists in git history.
- Post-commit deletion check found no deleted files.
- Stub scan on changed source files found no task-introduced stubs.
