---
quick_id: 260622-n7p
status: complete
completed: 2026-06-22
commit: 00dd78d
---

# Quick Task 260622-n7p: Android build/run success without install

## Outcome

Fixed a race where very fast command exits could arrive at `update()` before the runtime had installed the matching `TaskRecord` into the worktree slice. When that happened, `CommandExited` could not find the owner slice, so queued follow-up commands did not drain. For Android AVD runs, this could leave the successful emulator boot/wait command as the visible terminal success while the queued Android install/launch command never ran.

## Changes

- Added an `effect_runner` handshake so output/exit forwarding waits until the task record has been queued for the main runtime.
- Prioritized task-record delivery in the runtime event loop and drained pending task records before pending actions.
- Added a regression test for a fast command stream that emits `ProcessStarted` followed immediately by `Exited`.

## Verification

- `cargo test spawn_task_delivers_task_record_before_fast_command_exit` - passed
- `cargo test app::effect_runner::tests` - passed
- `cargo test available_android_avd_boot_waits_for_selected_avd_before_queued_run` - passed
- `cargo test command_exited_with_nonempty_queue_pops_and_dispatches_front` - passed
- `cargo test command_exited_drains_slice_local_queue_not_other` - passed
- `make arch-lint` - passed
- `cargo clippy --all-targets -- -D warnings` - passed
- `cargo test` - passed

## Notes

I did not run a real Android emulator/install locally. The fix is based on the reproduced runtime ordering race that matches the symptom: a successful preliminary command can otherwise prevent the queued install/launch command from draining.
