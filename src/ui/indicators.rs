//! Pure display helpers for live task indicators.
//!
//! All functions are pure (no I/O, no mutable state) and safe to call from
//! the render path. Imports: `std::time::Duration` + `crate::domain::command::CommandSpec`.
//! Zero infra imports (G-02).

use std::time::Duration;

use crate::domain::command::CommandSpec;

/// Half-circle spinner frames (the default). 6 frames, 150ms each → full
/// rotation in 900ms. `◐◑` are east_asian_width=A (Ambiguous); they render fine
/// in tmux+iTerm2 but may not sit flush under the width-1 Y/P letters in every
/// terminal — use [`SpinnerStyle::Braille`] there.
pub const SPINNER_FRAMES_CIRCLES: [&str; 6] = ["◐", "◓", "◑", "◒", "◐", "◓"];

/// Braille spinner frames. All glyphs are east_asian_width=N (Narrow), so they
/// render single-cell in every terminal and align with the width-1 Y/P letters.
pub const SPINNER_FRAMES_BRAILLE: [&str; 6] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴"];

/// Selectable spinner glyph set (config key `spinner_style` in config.toml).
/// Defaults to [`SpinnerStyle::Circles`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SpinnerStyle {
    /// Half-circles `◐◓◑◒` — the default.
    #[default]
    Circles,
    /// Braille dots `⠋⠙⠹⠸⠼⠴` — guaranteed single-cell width.
    Braille,
}

impl SpinnerStyle {
    /// Maps the config string to a style. `"braille"`/`"dots"` → Braille;
    /// everything else (including `"circles"`, empty, or unknown) → the
    /// default Circles. Case- and whitespace-insensitive.
    pub fn from_config(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "braille" | "dots" => SpinnerStyle::Braille,
            _ => SpinnerStyle::Circles,
        }
    }

    /// The 6-frame glyph set for this style.
    fn frames(self) -> &'static [&'static str; 6] {
        match self {
            SpinnerStyle::Circles => &SPINNER_FRAMES_CIRCLES,
            SpinnerStyle::Braille => &SPINNER_FRAMES_BRAILLE,
        }
    }
}

/// Returns the current spinner glyph for a running task in the given style.
///
/// Frame index = `elapsed.as_millis() / 150 % 6` (D-05 / UI-02).
/// No stored or incremented counter — the index is derived freshly from
/// the `elapsed` argument every call.
pub fn spinner_frame(elapsed: Duration, style: SpinnerStyle) -> &'static str {
    let idx = (elapsed.as_millis() / 150 % 6) as usize;
    style.frames()[idx]
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
        CommandSpec::UmpRunAndroid { .. }  => "run-and",
        CommandSpec::UmpRunIos { .. }      => "run-ios",
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

    // --- spinner_frame: boundary cases (D-05), default style = Circles ---

    #[test]
    fn frame_at_0ms() {
        assert_eq!(spinner_frame(Duration::from_millis(0), SpinnerStyle::Circles), "◐");
    }

    #[test]
    fn frame_at_149ms() {
        assert_eq!(spinner_frame(Duration::from_millis(149), SpinnerStyle::Circles), "◐");
    }

    #[test]
    fn frame_at_150ms() {
        assert_eq!(spinner_frame(Duration::from_millis(150), SpinnerStyle::Circles), "◓");
    }

    #[test]
    fn frame_at_749ms() {
        assert_eq!(spinner_frame(Duration::from_millis(749), SpinnerStyle::Circles), "◐");
    }

    #[test]
    fn frame_at_750ms() {
        assert_eq!(spinner_frame(Duration::from_millis(750), SpinnerStyle::Circles), "◓");
    }

    #[test]
    fn frame_wraps_at_900ms() {
        assert_eq!(spinner_frame(Duration::from_millis(900), SpinnerStyle::Circles), "◐");
    }

    // --- spinner_frame: braille style at the same boundaries ---

    #[test]
    fn braille_frames_at_boundaries() {
        assert_eq!(spinner_frame(Duration::from_millis(0), SpinnerStyle::Braille), "⠋");
        assert_eq!(spinner_frame(Duration::from_millis(149), SpinnerStyle::Braille), "⠋");
        assert_eq!(spinner_frame(Duration::from_millis(150), SpinnerStyle::Braille), "⠙");
        assert_eq!(spinner_frame(Duration::from_millis(749), SpinnerStyle::Braille), "⠼");
        assert_eq!(spinner_frame(Duration::from_millis(750), SpinnerStyle::Braille), "⠴");
        assert_eq!(spinner_frame(Duration::from_millis(900), SpinnerStyle::Braille), "⠋");
    }

    // --- SpinnerStyle: config mapping + default ---

    #[test]
    fn style_default_is_circles() {
        assert_eq!(SpinnerStyle::default(), SpinnerStyle::Circles);
    }

    #[test]
    fn style_from_config_maps_braille_and_defaults_to_circles() {
        assert_eq!(SpinnerStyle::from_config("braille"), SpinnerStyle::Braille);
        assert_eq!(SpinnerStyle::from_config("dots"), SpinnerStyle::Braille);
        assert_eq!(SpinnerStyle::from_config("  BRAILLE "), SpinnerStyle::Braille);
        assert_eq!(SpinnerStyle::from_config("circles"), SpinnerStyle::Circles);
        assert_eq!(SpinnerStyle::from_config(""), SpinnerStyle::Circles);
        assert_eq!(SpinnerStyle::from_config("nonsense"), SpinnerStyle::Circles);
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

    /// Constructs one instance of all 22 CommandSpec variants, asserts count == 22,
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
            CommandSpec::UmpRunAndroid {
                device_id: "emulator-5554".into(),
                variant: Some(crate::domain::command::RunVariant::Local),
            },
            CommandSpec::UmpRunIos {
                device_id: "simulator-uuid".into(),
                variant: Some(crate::domain::command::RunVariant::Dev),
            },
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
            22,
            "Expected 22 CommandSpec variants — update this test when adding a new variant"
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
