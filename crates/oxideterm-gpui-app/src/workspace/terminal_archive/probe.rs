use std::{process::Stdio, time::Duration};

use base64::{Engine, engine::general_purpose::STANDARD};
use oxideterm_environment::archive::{
    ArchiveCapabilities, ArchiveShell, POWERSHELL_ARCHIVE_PROBE, UNIX_ARCHIVE_PROBE,
};
use oxideterm_ssh::SshConnectionHandle;
use oxideterm_terminal::{ShellInfo, TerminalProcessInfo, TerminalProcessProbe};
use tokio::process::Command;
use zeroize::Zeroizing;

const PROBE_TIMEOUT: Duration = Duration::from_secs(6);

fn encoded_powershell_probe() -> String {
    let script = format!(
        "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new(); {POWERSHELL_ARCHIVE_PROBE}"
    );
    STANDARD.encode(
        script
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>(),
    )
}

pub(super) async fn probe_archive_environment(
    connection: Option<SshConnectionHandle>,
    launch_shell: Option<ShellInfo>,
    process_probe: Option<TerminalProcessProbe>,
) -> Result<
    (
        ArchiveCapabilities,
        Option<ArchiveShell>,
        Option<TerminalProcessInfo>,
    ),
    (),
> {
    if let Some(connection) = connection {
        let env = connection.remote_env().ok_or(())?;
        let shell = env.shell.as_deref().and_then(ArchiveShell::detect);
        let windows = env.os_type.to_ascii_lowercase().contains("windows");
        let command = if windows || shell == Some(ArchiveShell::PowerShell) {
            let executable = if windows { "powershell" } else { "pwsh" };
            format!(
                "{executable} -NoProfile -NonInteractive -EncodedCommand {}",
                encoded_powershell_probe()
            )
        } else {
            format!(
                "/bin/sh -lc {}",
                oxideterm_environment::shell_quote(UNIX_ARCHIVE_PROBE)
            )
        };
        let output = connection
            .run_command_capture(&command, PROBE_TIMEOUT, 16 * 1024)
            .await
            .map_err(|_| ())?;
        if output.exit_code != Some(0) {
            return Err(());
        }
        let text = Zeroizing::new(output.stdout);
        let capabilities = ArchiveCapabilities::parse(&text).ok_or(())?;
        let shell = shell.or(capabilities.shell);
        return Ok((capabilities, shell, None));
    }
    let mut shell_info = launch_shell.ok_or(())?;
    let process_info = if let Some(probe) = process_probe {
        Some(
            tokio::task::spawn_blocking(move || probe.collect())
                .await
                .map_err(|_| ())?,
        )
    } else {
        None
    };
    let wsl = shell_info.id.starts_with("wsl");
    let mut shell = ArchiveShell::detect(&shell_info.id)
        .or_else(|| shell_info.path.to_str().and_then(ArchiveShell::detect));
    if !wsl
        && let Some(command) = process_info
            .as_ref()
            .and_then(|info| info.command.as_deref())
    {
        // A foreground SSH client, container command or TUI is not the local shell environment.
        let foreground = ArchiveShell::detect(command).ok_or(())?;
        let foreground_name = command
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(command)
            .trim_start_matches('-');
        if shell_info.path.file_name().and_then(|name| name.to_str()) != Some(foreground_name) {
            shell_info.path = command.trim_start_matches('-').into();
        }
        shell = Some(foreground);
    }
    let mut command = if wsl {
        let mut command = Command::new(&shell_info.path);
        command
            .args(&shell_info.args)
            .args(["--exec", "/bin/sh", "-lc", UNIX_ARCHIVE_PROBE]);
        command
    } else {
        match shell.ok_or(())? {
            ArchiveShell::PowerShell | ArchiveShell::Cmd => {
                let mut command = if shell == Some(ArchiveShell::Cmd) {
                    Command::new("powershell.exe")
                } else {
                    Command::new(&shell_info.path)
                };
                command
                    .args(["-NoProfile", "-NonInteractive", "-EncodedCommand"])
                    .arg(encoded_powershell_probe());
                command
            }
            ArchiveShell::Posix => {
                let mut command = Command::new(&shell_info.path);
                command.args(["-lc", UNIX_ARCHIVE_PROBE]);
                command
            }
            ArchiveShell::Fish => {
                let mut command = Command::new(&shell_info.path);
                // Fish owns its PATH setup; the fixed capability script uses a POSIX parser.
                let script = format!(
                    "/bin/sh -c '{}'",
                    UNIX_ARCHIVE_PROBE
                        .replace('\\', "\\\\")
                        .replace('\'', "\\'")
                );
                command.args(["-lc", &script]);
                command
            }
        }
    };
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let output = tokio::time::timeout(PROBE_TIMEOUT, command.output())
        .await
        .map_err(|_| ())?
        .map_err(|_| ())?;
    if !output.status.success() || output.stdout.len() > 16 * 1024 {
        return Err(());
    }
    let bytes = Zeroizing::new(output.stdout);
    let text = std::str::from_utf8(&bytes).map_err(|_| ())?;
    let capabilities = ArchiveCapabilities::parse(text).ok_or(())?;
    if wsl {
        shell = capabilities.shell;
    }
    Ok((capabilities, shell, process_info))
}
