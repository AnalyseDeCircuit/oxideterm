// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Bounded local gate executed before a connection attempt.
//!
//! The command runs through the user's default shell so it can use the same
//! PATH and tooling as an interactive terminal, but it never inherits that
//! terminal: standard output and error are discarded so command output cannot
//! reach session recording, logs, diagnostics, or another shell surface.

use std::time::Duration;
use zeroize::Zeroizing;

pub use oxideterm_local_command::run_blocking as run_pre_connect_command_cancellable;
pub use oxideterm_local_command::{
    LocalCommandFailure, PRE_CONNECT_COMMAND_MAX_BYTES, PRE_CONNECT_COMMAND_TIMEOUT,
};

/// Trims a configured command and treats blank text as unset.
pub fn normalize_pre_connect_command(command: Option<&str>) -> Option<String> {
    let command = command.map(str::trim).filter(|value| !value.is_empty())?;
    (!command.is_empty()).then(|| command.to_owned())
}

pub async fn run_pre_connect_command_async(
    command: String,
    timeout: Duration,
) -> Result<(), LocalCommandFailure> {
    let command = Zeroizing::new(command);
    oxideterm_local_command::run(&command, timeout).await
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
        LocalCommandFailure::Cancelled => failure.to_string(),
        LocalCommandFailure::Exit(code) => translate(
            "connection.pre_connect.exit_code",
            &[("code", &code.to_string())],
            format!("Pre-connect command failed with exit code {code}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{LocalCommandFailure, normalize_pre_connect_command, pre_connect_failure_message};

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
    fn failure_text_reports_status_without_the_command() {
        let message = pre_connect_failure_message(None, LocalCommandFailure::Exit(3));
        assert!(message.contains('3'));
        assert!(!message.contains("gate-output"));
    }
}
