//! External command adapter for fire-and-forget GUI editor launches.

use crate::domain::ports::external_command_port::ExternalCommandPort;
use std::path::Path;

#[derive(Debug)]
pub struct ShellExternalCommand;

fn shell_command_parts(command: &str) -> (&'static str, [&str; 2]) {
    ("/bin/sh", ["-c", command])
}

#[cfg(target_os = "macos")]
fn finder_command_parts(path: &Path) -> (&'static str, [&std::ffi::OsStr; 1]) {
    ("open", [path.as_os_str()])
}

impl ExternalCommandPort for ShellExternalCommand {
    fn run_shell_command(&self, command: &str) -> anyhow::Result<()> {
        let (program, args) = shell_command_parts(command);
        let status = std::process::Command::new(program).args(args).status()?;
        if !status.success() {
            anyhow::bail!("external command failed: exit code {:?}", status.code());
        }
        Ok(())
    }

    fn open_in_finder(&self, path: &Path) -> anyhow::Result<()> {
        #[cfg(target_os = "macos")]
        {
            let (program, args) = finder_command_parts(path);
            let status = std::process::Command::new(program).args(args).status()?;
            if !status.success() {
                anyhow::bail!("Finder launch failed: exit code {:?}", status.code());
            }
            Ok(())
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            anyhow::bail!("Finder is only available on macOS")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_command_uses_non_login_shell() {
        let (program, args) = shell_command_parts("emacsclient -c -n '/tmp/ump'");

        assert_eq!(program, "/bin/sh");
        assert_eq!(args, ["-c", "emacsclient -c -n '/tmp/ump'"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn finder_command_passes_space_containing_path_as_one_argument() {
        let path = Path::new("/tmp/ump dash");

        let (program, args) = finder_command_parts(path);

        assert_eq!(program, "open");
        assert_eq!(args, [std::ffi::OsStr::new("/tmp/ump dash")]);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn finder_reports_unsupported_platform() {
        let error = ShellExternalCommand
            .open_in_finder(Path::new("/tmp/ump dash"))
            .expect_err("Finder should be unsupported outside macOS");

        assert_eq!(error.to_string(), "Finder is only available on macOS");
    }
}
