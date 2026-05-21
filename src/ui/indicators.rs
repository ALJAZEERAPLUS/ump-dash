//! Pure display helpers for live task indicators.
//!
//! All functions are pure (no I/O, no mutable state) and safe to call from
//! the render path. Imports: `std::time::Duration` + `crate::domain::command::CommandSpec`.
//! Zero infra imports (G-02).

use std::time::Duration;

use crate::domain::command::CommandSpec;

/// Half-circle spinner frames. 6 frames, 150ms per frame → full rotation in 900ms.
///
/// This const is the single swap point: to use the braille fallback set
/// `["⠋", "⠙", "⠹", "⠸", "⠼", "⠴"]` (all narrow, east_asian_width=N) or
/// ASCII `["-", "\\", "|", "/", "-", "\\"]`, only this line needs changing —
/// no layout or call-site change required. The type annotation `[&str; 6]` is
/// exact so any swap is type-checked at compile time.
///
/// Glyph width: braille dots U+2800-block all have east_asian_width=N (Narrow),
/// so they render single-cell in every terminal and align with the width-1 Y/P
/// letters. (Half-circles ◐◑ are east_asian_width=A/Ambiguous and rendered fine
/// in tmux+iTerm2 but did not visually line up with the letters — braille does.)
pub const SPINNER_FRAMES: [&str; 6] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴"];

/// Returns the current spinner glyph for a running task.
///
/// Frame index = `elapsed.as_millis() / 150 % 6` (D-05 / UI-02).
/// No stored or incremented counter — the index is derived freshly from
/// the `elapsed` argument every call.
pub fn spinner_frame(elapsed: Duration) -> &'static str {
    let idx = (elapsed.as_millis() / 150 % 6) as usize;
    SPINNER_FRAMES[idx]
}

/// Formats elapsed duration for display in the task column (D-08 / UI-03).
///
/// - Under 60 seconds: `"42s"` (integer seconds, no padding)
/// - 60 seconds and above: `"M:SS"` (minutes unpadded, seconds zero-padded to 2)
///
/// Examples: `0s`, `59s`, `1:00`, `1:01`, `10:00`, `12:03`.
pub fn format_elapsed(elapsed: Duration) -> String {
    let secs = elapsed.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}:{s:02}")
    }
}

/// Short display code for the task column (D-07).
///
/// Exhaustive match with NO `_ =>` catch-all arm — mirrors `collision_policy()`
/// at `src/domain/command.rs:172-206`. Adding a new `CommandSpec` variant will
/// fail to compile here (and in `collision_policy`) simultaneously, enforcing
/// variant coverage at compile time. The drift-guard meta-test
/// `task_short_label_covers_every_variant` provides a second layer of
/// enforcement.
///
/// Every code is non-empty so no variant ever renders blank.
pub fn task_short_label(spec: &CommandSpec) -> &'static str {
    match spec {
        CommandSpec::YarnInstall           => "yarn",
        CommandSpec::YarnPodInstall        => "pods",
        CommandSpec::YarnJest { .. }       => "jest",
        CommandSpec::YarnLint              => "lint",
        CommandSpec::YarnCheckTypes        => "types",
        CommandSpec::YarnUnitTests         => "unit-tests",
        CommandSpec::RnRunAndroid { .. }   => "run-and",
        CommandSpec::RnRunIos { .. }       => "run-ios",
        CommandSpec::RnRunIosDevice        => "run-ios",
        CommandSpec::RnReleaseBuild        => "release",
        CommandSpec::AdbInstallApk         => "adb",
        CommandSpec::ShellCommand { .. }   => "shell",
        CommandSpec::RnCleanAndroid        => "clean-and",
        CommandSpec::RnCleanCocoapods      => "clean-pod",
        CommandSpec::RmNodeModules         => "rm-mods",
        CommandSpec::GitPull               => "pull",
        CommandSpec::GitPush               => "push",
        CommandSpec::GitFetch              => "fetch",
        CommandSpec::GitRebase { .. }      => "rebase",
        CommandSpec::GitResetHard          => "reset",
        CommandSpec::GitResetHardFetch     => "reset+f",
        CommandSpec::GitCheckout { .. }    => "co",
        CommandSpec::GitCheckoutNew { .. } => "co -b",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // --- spinner_frame: boundary cases (D-05) ---

    #[test]
    fn frame_at_0ms() {
        assert_eq!(spinner_frame(Duration::from_millis(0)), "⠋");
    }

    #[test]
    fn frame_at_149ms() {
        assert_eq!(spinner_frame(Duration::from_millis(149)), "⠋");
    }

    #[test]
    fn frame_at_150ms() {
        assert_eq!(spinner_frame(Duration::from_millis(150)), "⠙");
    }

    #[test]
    fn frame_at_749ms() {
        assert_eq!(spinner_frame(Duration::from_millis(749)), "⠼");
    }

    #[test]
    fn frame_at_750ms() {
        assert_eq!(spinner_frame(Duration::from_millis(750)), "⠴");
    }

    #[test]
    fn frame_wraps_at_900ms() {
        assert_eq!(spinner_frame(Duration::from_millis(900)), "⠋");
    }

    // --- format_elapsed: boundary cases (D-08) ---

    #[test]
    fn elapsed_0s() {
        assert_eq!(format_elapsed(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn elapsed_42s() {
        assert_eq!(format_elapsed(Duration::from_secs(42)), "42s");
    }

    #[test]
    fn elapsed_59s() {
        assert_eq!(format_elapsed(Duration::from_secs(59)), "59s");
    }

    #[test]
    fn elapsed_60s() {
        assert_eq!(format_elapsed(Duration::from_secs(60)), "1:00");
    }

    #[test]
    fn elapsed_61s() {
        assert_eq!(format_elapsed(Duration::from_secs(61)), "1:01");
    }

    #[test]
    fn elapsed_600s() {
        assert_eq!(format_elapsed(Duration::from_secs(600)), "10:00");
    }

    #[test]
    fn elapsed_723s() {
        assert_eq!(format_elapsed(Duration::from_secs(723)), "12:03");
    }

    // --- task_short_label: spot-checks ---

    #[test]
    fn yarn_install_label() {
        assert_eq!(task_short_label(&CommandSpec::YarnInstall), "yarn");
    }

    #[test]
    fn unit_tests_label() {
        assert_eq!(task_short_label(&CommandSpec::YarnUnitTests), "unit-tests");
    }

    #[test]
    fn git_checkout_new_label() {
        assert_eq!(
            task_short_label(&CommandSpec::GitCheckoutNew {
                branch: "x".into()
            }),
            "co -b"
        );
    }

    #[test]
    fn reset_hard_fetch_label() {
        assert_eq!(task_short_label(&CommandSpec::GitResetHardFetch), "reset+f");
    }

    #[test]
    fn shell_label() {
        assert_eq!(
            task_short_label(&CommandSpec::ShellCommand {
                command: "".into()
            }),
            "shell"
        );
    }

    // --- task_short_label: drift-guard meta-test ---

    /// Constructs one instance of all 23 CommandSpec variants, asserts count == 23,
    /// and asserts every label is non-empty. Mirrors `collision_policy_covers_every_variant`
    /// at src/domain/command.rs:501-562.
    #[test]
    fn task_short_label_covers_every_variant() {
        let variants: Vec<CommandSpec> = vec![
            CommandSpec::GitResetHard,
            CommandSpec::GitPull,
            CommandSpec::GitPush,
            CommandSpec::GitRebase {
                target: "main".into(),
            },
            CommandSpec::GitCheckout {
                branch: "feat".into(),
            },
            CommandSpec::GitCheckoutNew {
                branch: "new-feat".into(),
            },
            CommandSpec::RnCleanAndroid,
            CommandSpec::RnCleanCocoapods,
            CommandSpec::RmNodeModules,
            CommandSpec::YarnInstall,
            CommandSpec::YarnPodInstall,
            CommandSpec::RnRunAndroid {
                device_id: "emulator-5554".into(),
                mode: None,
            },
            CommandSpec::RnRunIos {
                device_id: "simulator-uuid".into(),
            },
            CommandSpec::RnRunIosDevice,
            CommandSpec::YarnUnitTests,
            CommandSpec::YarnJest {
                filter: "MyTest".into(),
            },
            CommandSpec::YarnLint,
            CommandSpec::YarnCheckTypes,
            CommandSpec::GitFetch,
            CommandSpec::GitResetHardFetch,
            CommandSpec::RnReleaseBuild,
            CommandSpec::AdbInstallApk,
            CommandSpec::ShellCommand {
                command: "echo hi".into(),
            },
        ];

        assert_eq!(
            variants.len(),
            23,
            "Expected 23 CommandSpec variants — update this test when adding a new variant"
        );

        for v in &variants {
            let label = task_short_label(v);
            assert!(
                !label.is_empty(),
                "task_short_label returned empty string for {:?}",
                v
            );
        }
    }
}
