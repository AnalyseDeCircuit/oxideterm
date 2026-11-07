use crate::error::{CliError, CliResult};
use clap::Args;
use oxideterm_ssh_launch::{LocalTerminalLaunch, MAX_LOCAL_LAUNCH_BYTES};
use std::{
    fmt,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use zeroize::{Zeroize, Zeroizing};

#[derive(Args)]
#[command(
    after_help = "Examples:\n  oxideterm terminal --cwd ~/project\n  oxideterm terminal --cwd ~/project -- claude --resume SESSION_ID\n  oxideterm terminal --cwd ~/project -- /bin/zsh -lc 'echo hello'\n\nArguments after -- are passed literally. Shell syntax requires an explicit shell."
)]
pub struct TerminalArgs {
    #[arg(
        long,
        value_name = "DIRECTORY",
        help = "Working directory (defaults to the caller's directory)"
    )]
    pub cwd: Option<PathBuf>,
    #[arg(last = true, value_name = "PROGRAM [ARG...]")]
    pub command: Vec<String>,
}

impl Drop for TerminalArgs {
    fn drop(&mut self) {
        self.command.zeroize();
    }
}

impl fmt::Debug for TerminalArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TerminalArgs")
            .field("argument_count", &self.command.len())
            .finish_non_exhaustive()
    }
}

pub fn run(mut args: TerminalArgs) -> CliResult<i32> {
    if crate::paths::has_cli_path_override() {
        return Err(CliError::new(
            "native_terminal_profile_unsupported",
            "terminal targets the native application's active data directory; remove --config-dir and --profile",
            false,
        ));
    }
    let launch = LocalTerminalLaunch::resolve(
        args.cwd.take(),
        Zeroizing::new(std::mem::take(&mut args.command)),
    )
    .map_err(|message| CliError::new("invalid_local_launch", message, false))?;
    let bytes = Zeroizing::new(serde_json::to_vec(&launch).map_err(|_| {
        CliError::new(
            "local_launch_encode_failed",
            "Could not encode the local terminal request",
            false,
        )
    })?);
    if bytes.len() > MAX_LOCAL_LAUNCH_BYTES {
        return Err(CliError::new(
            "local_launch_too_large",
            "Local terminal request exceeds 60 KiB",
            false,
        ));
    }
    let mut candidates: Vec<PathBuf> = crate::ssh::sibling_native_binary().into_iter().collect();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(binary) = bundled_native_binary(&exe) {
            candidates.push(binary);
        }
    }
    candidates.push(PathBuf::from(crate::ssh::native_binary_name()));
    #[cfg(target_os = "macos")]
    {
        candidates.push("/Applications/OxideTerm.app/Contents/MacOS/oxideterm-native".into());
        if let Some(home) = std::env::var_os("HOME") {
            candidates.push(
                PathBuf::from(home)
                    .join("Applications/OxideTerm.app/Contents/MacOS/oxideterm-native"),
            );
        }
    }
    #[cfg(windows)]
    {
        let product = if oxideterm_settings::is_prerelease_version(env!("CARGO_PKG_VERSION")) {
            "OxideTerm Preview"
        } else {
            "OxideTerm"
        };
        if let Ok(output) = Command::new("reg")
            .args([
                "query",
                &format!("HKCU\\Software\\{product}"),
                "/v",
                "InstallDir",
            ])
            .output()
        {
            if output.status.success() {
                if let Some(path) =
                    String::from_utf8_lossy(&output.stdout)
                        .lines()
                        .find_map(|line| {
                            line.split_once("REG_SZ")
                                .map(|(_, value)| value.trim().to_string())
                        })
                {
                    candidates.push(PathBuf::from(path).join(crate::ssh::native_binary_name()));
                }
            }
        }
    }
    for binary in candidates {
        match Command::new(binary)
            .arg("--local-launch-stdin")
            .stdin(Stdio::piped())
            .spawn()
        {
            Ok(mut child) => {
                // Use a one-shot pipe: argv and temporary files must not retain the payload.
                let sent = child.stdin.take().expect("piped stdin").write_all(&bytes);
                if sent.is_err() {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(CliError::new(
                        "local_launch_send_failed",
                        "Could not deliver the local terminal request",
                        false,
                    ));
                }
                println!("Local terminal launch requested.");
                return Ok(0);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => {
                return Err(CliError::new(
                    "native_gui_launch_failed",
                    "Could not start the native application",
                    false,
                ));
            }
        }
    }
    Err(CliError::new(
        "native_gui_not_found",
        "Could not find oxideterm-native next to the CLI, in PATH, or in an installed application bundle",
        false,
    ))
}

fn bundled_native_binary(exe: &Path) -> Option<PathBuf> {
    let cli_dir = exe
        .ancestors()
        .find(|path| path.file_name().is_some_and(|name| name == "cli-bin"))?;
    let parent = cli_dir.parent()?;
    let directory = if parent.file_name().is_some_and(|name| name == "Resources")
        && parent
            .parent()?
            .file_name()
            .is_some_and(|name| name == "Contents")
    {
        parent.parent()?.join("MacOS")
    } else if parent.file_name().is_some_and(|name| name == "resources") {
        parent.parent()?.to_path_buf()
    } else {
        parent.to_path_buf()
    };
    Some(directory.join(crate::ssh::native_binary_name()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::{Cli, Command};
    use clap::Parser;

    #[test]
    fn terminal_command_keeps_flags_empty_arguments_and_shell_metacharacters_literal() {
        let cli = Cli::try_parse_from([
            "oxideterm",
            "terminal",
            "--cwd",
            "a directory",
            "--",
            "tool",
            "--profile",
            "literal profile",
            "",
            "a;$(no)",
        ])
        .unwrap();
        let Command::Terminal(args) = cli.command else {
            panic!("terminal command")
        };
        assert_eq!(args.cwd, Some(PathBuf::from("a directory")));
        assert_eq!(
            args.command,
            ["tool", "--profile", "literal profile", "", "a;$(no)"]
        );
        assert!(!format!("{args:?}").contains("literal profile"));
    }

    #[cfg(unix)]
    #[test]
    fn bundled_cli_finds_its_own_application_before_path() {
        assert_eq!(
            bundled_native_binary(Path::new(
                "/custom/OxideTerm.app/Contents/Resources/cli-bin/aarch64-apple-darwin/oxideterm"
            )),
            Some(PathBuf::from(
                "/custom/OxideTerm.app/Contents/MacOS/oxideterm-native"
            ))
        );
        assert_eq!(
            bundled_native_binary(Path::new(
                "/opt/oxideterm/resources/cli-bin/linux/oxideterm"
            )),
            Some(PathBuf::from("/opt/oxideterm/oxideterm-native"))
        );
    }
}
