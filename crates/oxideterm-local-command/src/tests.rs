// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

fn run_pre_connect_command(command: &str, duration: Duration) -> Result<(), LocalCommandFailure> {
    run_blocking(command, duration, || false)
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
