//! External command adapter for fire-and-forget GUI editor launches.

use crate::domain::ports::external_command_port::ExternalCommandPort;

#[derive(Debug)]
pub struct ShellExternalCommand;

fn shell_command_parts(command: &str) -> (&'static str, [&str; 2]) {
    ("/bin/sh", ["-c", command])
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
}
