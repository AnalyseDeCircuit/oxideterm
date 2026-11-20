// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

//! Runs user-authored local connection prerequisites with a bounded lifetime.

use std::{
    fmt,
    time::{Duration, Instant},
};

#[cfg(unix)]
#[path = "unix.rs"]
mod process;
#[cfg(windows)]
#[path = "windows.rs"]
mod process;
#[cfg(test)]
mod tests;

#[cfg(windows)]
mod batch_path;
#[cfg(windows)]
pub use batch_path::windows_batch_launch_path;

pub const PRE_CONNECT_COMMAND_MAX_BYTES: usize = 8192;
pub const PRE_CONNECT_COMMAND_TIMEOUT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalCommandFailure {
    TooLong,
    Spawn,
    Timeout,
    Cancelled,
    Exit(i32),
}

impl fmt::Display for LocalCommandFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong => write!(
                f,
                "Pre-connect command is too long (max {PRE_CONNECT_COMMAND_MAX_BYTES} bytes)"
            ),
            Self::Spawn => f.write_str("Pre-connect command could not start"),
            Self::Timeout => write!(
                f,
                "Pre-connect command timed out after {} seconds",
                PRE_CONNECT_COMMAND_TIMEOUT.as_secs()
            ),
            Self::Cancelled => f.write_str("Pre-connect command cancelled"),
            Self::Exit(code) => write!(f, "Pre-connect command failed with exit code {code}"),
        }
    }
}

fn spawn(command: &str) -> Result<Option<process::RunningCommand>, LocalCommandFailure> {
    let command = command.trim();
    if command.is_empty() {
        return Ok(None);
    }
    if command.len() > PRE_CONNECT_COMMAND_MAX_BYTES {
        return Err(LocalCommandFailure::TooLong);
    }
    process::RunningCommand::spawn(command)
        .map(Some)
        .map_err(|_| LocalCommandFailure::Spawn)
}

fn poll(
    child: &mut process::RunningCommand,
    deadline: Instant,
) -> Result<Option<()>, LocalCommandFailure> {
    match child.try_wait().map_err(|_| LocalCommandFailure::Spawn)? {
        Some(0) => Ok(Some(())),
        Some(code) => Err(LocalCommandFailure::Exit(code)),
        None if Instant::now() >= deadline => Err(LocalCommandFailure::Timeout),
        None => Ok(None),
    }
}

pub fn run_blocking(
    command: &str,
    timeout: Duration,
    is_cancelled: impl Fn() -> bool,
) -> Result<(), LocalCommandFailure> {
    if is_cancelled() {
        return Err(LocalCommandFailure::Cancelled);
    }
    let Some(mut child) = spawn(command)? else {
        return Ok(());
    };
    let deadline = Instant::now() + timeout;
    loop {
        if is_cancelled() {
            return Err(LocalCommandFailure::Cancelled);
        }
        if poll(&mut child, deadline)?.is_some() {
            return Ok(());
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

pub async fn run(command: &str, timeout: Duration) -> Result<(), LocalCommandFailure> {
    // The waiting future directly owns the process group/job. Cancelling the
    // owning connection attempt drops it, rather than detaching a blocking task.
    let Some(mut child) = spawn(command)? else {
        return Ok(());
    };
    let deadline = Instant::now() + timeout;
    loop {
        if poll(&mut child, deadline)?.is_some() {
            return Ok(());
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}
