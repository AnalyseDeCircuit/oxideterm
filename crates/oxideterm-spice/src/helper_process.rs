// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    ops::{Deref, DerefMut},
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use crate::SpiceHelperCommand;

#[cfg(windows)]
const HELPER_CREATE_NO_WINDOW: u32 = 0x08000000;

pub(crate) struct OwnedHelper(Child);

impl Deref for OwnedHelper {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for OwnedHelper {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Drop for OwnedHelper {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            // Handshake and writer failures still own a live process until this guard reaps it.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

pub(crate) fn spawn_helper(command: &SpiceHelperCommand) -> Result<OwnedHelper, std::io::Error> {
    let mut process = Command::new(&command.executable);
    process
        .arg("--stdio")
        .current_dir(&command.working_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    process.creation_flags(HELPER_CREATE_NO_WINDOW);
    process.spawn().map(OwnedHelper)
}

pub(crate) fn wait_or_terminate(
    child: &mut Child,
    graceful_timeout: Duration,
    poll_interval: Duration,
) -> Option<ExitStatus> {
    let started_at = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) if started_at.elapsed() < graceful_timeout => {
                thread::sleep(
                    poll_interval.min(graceful_timeout.saturating_sub(started_at.elapsed())),
                );
            }
            Ok(None) | Err(_) => {
                // Child owns every SPICE integration task, so forced process teardown is the final
                // bounded cancellation path after cooperative Close or stdin EOF has failed.
                let _ = child.kill();
                return child.wait().ok();
            }
        }
    }
}
