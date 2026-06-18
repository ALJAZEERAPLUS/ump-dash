//! External command adapter for fire-and-forget GUI editor launches.

use crate::domain::ports::external_command_port::ExternalCommandPort;

#[derive(Debug)]
pub struct ShellExternalCommand;

impl ExternalCommandPort for ShellExternalCommand {
    fn run_shell_command(&self, command: &str) -> anyhow::Result<()> {
        let status = std::process::Command::new("/bin/sh")
            .args(["-lc", command])
            .status()?;
        if !status.success() {
            anyhow::bail!("external command failed: exit code {:?}", status.code());
        }
        Ok(())
    }
}
