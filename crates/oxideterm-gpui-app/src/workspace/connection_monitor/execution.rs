use std::time::Duration;

use oxideterm_connection_monitor::{
    ResourceSampleError, ResourceSampleShell, ResourceSampler, ResourceSamplerFuture,
};
use oxideterm_ssh::{SshCommandOutput, SshConnectionHandle};
use oxideterm_terminal::{LocalPtyConfig, ShellInfo};

pub(super) enum CommandTarget {
    Local,
    Ssh(SshConnectionHandle),
}

pub(super) fn local_os() -> &'static str {
    std::env::consts::OS
}

// Only commands constructed by the host-tool builders enter this module.
// In particular, a user's default terminal may be WSL, Fish or a container.
fn local_command_shell(command: &str) -> ShellInfo {
    #[cfg(windows)]
    {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let command = command.trim_end();
        let script = command
            .strip_prefix("powershell -NoProfile -ExecutionPolicy Bypass -Command \"")
            .and_then(|script| script.strip_suffix('"'))
            .unwrap_or(command);
        let script = format!(
            "$ProgressPreference='SilentlyContinue'; [Console]::OutputEncoding=[Text.UTF8Encoding]::new(); {script}"
        );
        let encoded = STANDARD.encode(
            script
                .encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>(),
        );
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        ShellInfo::new(
            "host-tool",
            "PowerShell",
            std::path::PathBuf::from(root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"),
        )
        .with_args(vec![
            "-NoLogo".into(),
            "-NoProfile".into(),
            "-EncodedCommand".into(),
            encoded,
        ])
    }
    #[cfg(not(windows))]
    {
        // Finder and other GUI launchers can omit the standard Homebrew paths.
        // Existing remote probes retry sudo; a local probe must keep the caller's privileges.
        ShellInfo::new("host-tool", "sh", "/bin/sh").with_args(vec![
            "-c".into(),
            format!("export LC_ALL=C PATH=\"$PATH:/usr/local/bin:/usr/local/sbin:/opt/homebrew/bin\"; sudo() {{ return 1; }}\n{command}"),
        ])
    }
}

pub(in crate::workspace) fn local_tool_terminal_config(command: &str) -> LocalPtyConfig {
    #[cfg(windows)]
    let shell = {
        let mut shell = local_command_shell(command);
        shell.args.insert(0, "-NoExit".into());
        shell
    };
    #[cfg(not(windows))]
    let shell = local_command_shell(&format!("(\n{command}\n)\nexec /bin/sh -i"));
    LocalPtyConfig {
        shell: Some(shell),
        ..Default::default()
    }
}

impl CommandTarget {
    pub(super) async fn run_command_capture(
        &self,
        command: &str,
        timeout: Duration,
        limit: usize,
    ) -> Result<SshCommandOutput, ()> {
        if let Self::Ssh(handle) = self {
            return handle
                .run_command_capture(command, timeout, limit)
                .await
                .map_err(|_| ());
        }
        let shell = local_command_shell(command);
        let mut command = tokio::process::Command::new(shell.path);
        command.args(shell.args);
        let child = oxideterm_ai::agent::AgentProcess::spawn(&mut command).map_err(|_| ())?;
        let (output, truncated) = tokio::time::timeout(timeout, child.limited_output(limit))
            .await
            .map_err(|_| ())?
            .map_err(|_| ())?;
        Ok(SshCommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            exit_code: output.status.code(),
            truncated,
        })
    }
}

pub(super) struct LocalSampler;

impl ResourceSampler for LocalSampler {
    fn open_shell<'a>(
        &'a self,
        _: &'a str,
        _: Duration,
    ) -> ResourceSamplerFuture<'a, Result<Box<dyn ResourceSampleShell>, String>> {
        Box::pin(async { Ok(Box::new(LocalSampler) as Box<dyn ResourceSampleShell>) })
    }
}

impl ResourceSampleShell for LocalSampler {
    fn sample_until<'a>(
        &'a mut self,
        command: &'a str,
        end_marker: &'a str,
        timeout: Duration,
        limit: usize,
    ) -> ResourceSamplerFuture<'a, Result<String, ResourceSampleError>> {
        Box::pin(async move {
            let output = CommandTarget::Local
                .run_command_capture(command, timeout, limit)
                .await
                .map_err(|_| ResourceSampleError::Failed("Local sample failed".into()))?;
            if output.truncated {
                return Err(ResourceSampleError::OutputLimit {
                    channel_reusable: true,
                });
            }
            if output.exit_code != Some(0) || !output.stdout.lines().any(|line| line == end_marker)
            {
                return Err(ResourceSampleError::Failed(
                    "Local sample did not complete".into(),
                ));
            }
            Ok(output.stdout)
        })
    }

    fn close<'a>(&'a mut self) -> ResourceSamplerFuture<'a, Result<(), String>> {
        Box::pin(async { Ok(()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn local_capture_preserves_exit_status_and_bounds_both_pipes() {
        #[cfg(windows)]
        let script = "[Console]::Out.Write('abcdef'); [Console]::Error.Write('123456'); exit 7";
        #[cfg(not(windows))]
        let script = "printf abcdef; printf 123456 >&2; exit 7";
        let output = CommandTarget::Local
            .run_command_capture(script, Duration::from_secs(10), 4)
            .await
            .unwrap();
        assert_eq!(
            (
                output.stdout.as_str(),
                output.stderr.as_str(),
                output.exit_code,
                output.truncated
            ),
            ("abcd", "1234", Some(7), true)
        );
    }

    #[tokio::test]
    async fn local_sampler_accepts_a_generated_host_probe() {
        let script = oxideterm_connection_monitor::build_sample_command_for(
            local_os(),
            oxideterm_connection_monitor::ResourceSamplingConfig {
                system: false,
                processes: false,
                docker: false,
                gpu: false,
            },
        );
        let output = LocalSampler
            .sample_until(&script, "===END===", Duration::from_secs(10), 4096)
            .await
            .unwrap();
        assert!(output.lines().any(|line| line == "===END==="));
    }
}
