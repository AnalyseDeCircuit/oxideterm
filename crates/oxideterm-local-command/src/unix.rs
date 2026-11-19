// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    io,
    os::unix::process::CommandExt,
    path::PathBuf,
    process::{Child, Command, Stdio},
};

pub(super) struct RunningCommand {
    child: Child,
    succeeded: bool,
}

fn default_shell_path() -> PathBuf {
    std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| path.exists())
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                "/bin/zsh"
            } else {
                "/bin/bash"
            }
            .into()
        })
}

impl RunningCommand {
    pub(super) fn spawn(command: &str) -> io::Result<Self> {
        let child = Command::new(default_shell_path())
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0)
            .spawn()?;
        Ok(Self {
            child,
            succeeded: false,
        })
    }

    pub(super) fn try_wait(&mut self) -> io::Result<Option<i32>> {
        Ok(self.child.try_wait()?.map(|status| {
            self.succeeded = status.success();
            status.code().unwrap_or(-1)
        }))
    }
}

impl Drop for RunningCommand {
    fn drop(&mut self) {
        if !self.succeeded {
            // The shell gets its own process group before exec, so cancellation
            // also reaches ordinary descendants, without touching the caller.
            let pgid = self.child.id() as libc::pid_t;
            let _ = unsafe { libc::kill(-pgid, libc::SIGKILL) };
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
