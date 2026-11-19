// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Bounded local gate executed before a connection attempt.
//!
//! The command runs through the user's default shell so it can use the same
//! PATH and tooling as an interactive terminal, but it never inherits that
//! terminal: standard output and error are discarded so command output cannot
//! reach session recording, logs, diagnostics, or another shell surface.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Upper bound for both the configured text and the payload handed to a shell.
pub const PRE_CONNECT_COMMAND_MAX_BYTES: usize = 8192;
/// One gate attempt is bounded so a hung command cannot pin the reconnect loop
/// or a pending connection trace indefinitely.
pub const PRE_CONNECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const PRE_CONNECT_COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Failure detail for one gate attempt.
///
/// The variants deliberately carry no command text or process output: callers
/// surface this through user-visible errors that must never echo the command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalCommandFailure {
    TooLong,
    Spawn,
    Timeout,
    Exit(i32),
}

/// Trims a configured command and treats blank text as unset.
pub fn normalize_pre_connect_command(command: Option<&str>) -> Option<String> {
    let command = command.map(str::trim).filter(|value| !value.is_empty())?;
    (!command.is_empty()).then(|| command.to_owned())
}

/// Runs one configured command and waits for its exit status.
///
/// Blocking by design: every caller already owns a background thread or a
/// blocking task, and the wait must finish before the transport is created.
pub fn run_pre_connect_command(
    command: &str,
    timeout: Duration,
) -> Result<(), LocalCommandFailure> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(());
    }
    if command.len() > PRE_CONNECT_COMMAND_MAX_BYTES {
        return Err(LocalCommandFailure::TooLong);
    }
    let (program, args) = shell_invocation(command);
    let mut process = Command::new(program);
    process
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // A gate command is never interactive, so it must not flash a console
        // window over the application.
        process.creation_flags(0x0800_0000);
    }
    let mut child = process.spawn().map_err(|_| LocalCommandFailure::Spawn)?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(LocalCommandFailure::Exit(status.code().unwrap_or(-1)))
                };
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(LocalCommandFailure::Timeout);
                }
                std::thread::sleep(PRE_CONNECT_COMMAND_POLL_INTERVAL);
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(LocalCommandFailure::Spawn);
            }
        }
    }
}

/// Off-thread wrapper for callers that live inside a tokio runtime.
pub async fn run_pre_connect_command_async(
    command: String,
    timeout: Duration,
) -> Result<(), LocalCommandFailure> {
    tokio::task::spawn_blocking(move || run_pre_connect_command(&command, timeout))
        .await
        .unwrap_or(Err(LocalCommandFailure::Spawn))
}

/// Builds the user-visible failure text without exposing the command itself.
pub fn pre_connect_failure_message(
    i18n: Option<&oxideterm_i18n::I18n>,
    failure: LocalCommandFailure,
) -> String {
    let limit = PRE_CONNECT_COMMAND_MAX_BYTES.to_string();
    let seconds = PRE_CONNECT_COMMAND_TIMEOUT.as_secs().to_string();
    let translate = |key: &str, args: &[(&str, &str)], fallback: String| match i18n {
        Some(i18n) => i18n.t_with(key, args),
        None => fallback,
    };
    match failure {
        LocalCommandFailure::TooLong => translate(
            "connection.pre_connect.too_long",
            &[("limit", limit.as_str())],
            format!("Pre-connect command is too long (max {limit} bytes)"),
        ),
        LocalCommandFailure::Spawn => translate(
            "connection.pre_connect.spawn_failed",
            &[],
            "Pre-connect command could not start".to_string(),
        ),
        LocalCommandFailure::Timeout => translate(
            "connection.pre_connect.timeout",
            &[("seconds", seconds.as_str())],
            format!("Pre-connect command timed out after {seconds} seconds"),
        ),
        LocalCommandFailure::Exit(code) => translate(
            "connection.pre_connect.exit_code",
            &[("code", &code.to_string())],
            format!("Pre-connect command failed with exit code {code}"),
        ),
    }
}

fn shell_invocation(command: &str) -> (PathBuf, Vec<String>) {
    let shell = crate::local_shell::default_shell();
    let shell_id = shell.id.to_ascii_lowercase();
    #[cfg(target_os = "windows")]
    {
        // Windows shells disagree on the switch that carries a command string,
        // so mirror the invocation each one documents instead of assuming cmd.
        if shell_id.contains("powershell") || shell_id.contains("pwsh") {
            return (
                shell.path,
                vec![
                    "-NoProfile".to_string(),
                    "-NonInteractive".to_string(),
                    "-Command".to_string(),
                    command.to_string(),
                ],
            );
        }
        if shell_id == "nu" || shell_id == "nu.exe" {
            return (shell.path, vec!["-c".to_string(), command.to_string()]);
        }
        // Git Bash and MSYS shells are POSIX even on Windows.
        if shell_id.contains("bash") || shell_id.contains("zsh") || shell_id.contains("sh") {
            return (shell.path, vec!["-c".to_string(), command.to_string()]);
        }
        return (shell.path, vec!["/C".to_string(), command.to_string()]);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = shell_id;
        (shell.path, vec!["-c".to_string(), command.to_string()])
    }
}

#[cfg(test)]
mod tests {
    use super::{
        LocalCommandFailure, PRE_CONNECT_COMMAND_MAX_BYTES, normalize_pre_connect_command,
        pre_connect_failure_message, run_pre_connect_command,
    };
    use std::time::Duration;

    #[test]
    fn normalize_treats_blank_and_whitespace_as_unset() {
        assert_eq!(normalize_pre_connect_command(None), None);
        assert_eq!(normalize_pre_connect_command(Some("   ")), None);
        assert_eq!(
            normalize_pre_connect_command(Some("  ~/.config/vpn/up.sh  ")).as_deref(),
            Some("~/.config/vpn/up.sh")
        );
    }

    #[test]
    #[cfg(unix)]
    fn exit_status_decides_success_and_reports_the_code() {
        assert_eq!(
            run_pre_connect_command("true", Duration::from_secs(5)),
            Ok(())
        );
        assert_eq!(
            run_pre_connect_command("exit 7", Duration::from_secs(5)),
            Err(LocalCommandFailure::Exit(7))
        );
    }

    #[test]
    #[cfg(unix)]
    fn command_output_never_reaches_the_caller() {
        // The gate's only observable result is its status; output must be
        // discarded instead of becoming terminal or diagnostic content.
        let secret = "gate-output-must-not-surface";
        let command = format!("printf '{secret}'");
        assert_eq!(
            run_pre_connect_command(&command, Duration::from_secs(5)),
            Ok(())
        );
    }

    #[test]
    #[cfg(unix)]
    fn timeout_stops_a_hung_command() {
        assert_eq!(
            run_pre_connect_command("sleep 30", Duration::from_millis(100)),
            Err(LocalCommandFailure::Timeout)
        );
    }

    #[test]
    fn oversized_command_is_rejected_before_spawning() {
        let command = "x".repeat(PRE_CONNECT_COMMAND_MAX_BYTES + 1);
        assert_eq!(
            run_pre_connect_command(&command, Duration::from_secs(5)),
            Err(LocalCommandFailure::TooLong)
        );
    }

    #[test]
    fn failure_text_reports_status_without_the_command() {
        let message = pre_connect_failure_message(None, LocalCommandFailure::Exit(3));
        assert!(message.contains('3'));
        assert!(!message.contains("gate-output"));
    }
}
