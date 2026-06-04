use std::{
    fs::File,
    path::PathBuf,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use std::os::fd::AsRawFd;

use anyhow::Result;
#[cfg(unix)]
use anyhow::Context;

const PROCESS_INFO_REFRESH_INTERVAL: Duration = Duration::from_millis(500);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalLifecycle {
    Running,
    Exited(Option<i32>),
    Closed,
}

impl TerminalLifecycle {
    pub fn is_running(&self) -> bool {
        matches!(self, Self::Running)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TerminalProcessInfo {
    pub shell_pid: Option<u32>,
    pub foreground_pid: Option<u32>,
    pub foreground_process_group_id: Option<u32>,
    pub command: Option<String>,
    pub cwd: Option<PathBuf>,
}

pub(crate) struct ProcessState {
    pub(crate) info: TerminalProcessInfo,
    pty_master: Option<File>,
    last_refresh: Instant,
}

impl ProcessState {
    pub(crate) fn new(
        shell_pid: Option<u32>,
        pty_master: Option<File>,
        cwd: Option<PathBuf>,
    ) -> Self {
        Self {
            info: TerminalProcessInfo {
                shell_pid,
                foreground_pid: shell_pid,
                foreground_process_group_id: shell_pid,
                command: shell_pid.and_then(process_command),
                cwd,
            },
            pty_master,
            last_refresh: Instant::now()
                .checked_sub(PROCESS_INFO_REFRESH_INTERVAL)
                .unwrap_or_else(Instant::now),
        }
    }

    pub(crate) fn mark_exited(&mut self) {
        self.info.foreground_pid = None;
        self.info.foreground_process_group_id = None;
        self.info.command = None;
    }

    pub(crate) fn refresh(&mut self) {
        if self.last_refresh.elapsed() < PROCESS_INFO_REFRESH_INTERVAL {
            return;
        }

        self.last_refresh = Instant::now();
        let foreground_group = self
            .pty_master
            .as_ref()
            .and_then(foreground_process_group_id)
            .or(self.info.shell_pid);
        let foreground_pid = foreground_group
            .and_then(active_process_in_group)
            .or(foreground_group);

        self.info.foreground_process_group_id = foreground_group;
        self.info.foreground_pid = foreground_pid;
        self.info.command = foreground_pid.and_then(process_command);
        if let Some(cwd) = foreground_pid
            .and_then(process_cwd)
            .or_else(|| self.info.shell_pid.and_then(process_cwd))
        {
            self.info.cwd = Some(cwd);
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum TerminalSignal {
    Terminate,
    Kill,
}

#[cfg(unix)]
pub(crate) fn signal_process_group(
    process_group_id: Option<u32>,
    signal: TerminalSignal,
) -> Result<()> {
    let Some(process_group_id) = process_group_id else {
        anyhow::bail!("no active terminal process group");
    };

    let signal = match signal {
        TerminalSignal::Terminate => libc::SIGTERM,
        TerminalSignal::Kill => libc::SIGKILL,
    };
    let target = -(process_group_id as libc::pid_t);
    let result = unsafe { libc::kill(target, signal) };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
            .with_context(|| format!("failed to signal process group {process_group_id}"))
    }
}

#[cfg(not(unix))]
pub(crate) fn signal_process_group(
    _process_group_id: Option<u32>,
    _signal: TerminalSignal,
) -> Result<()> {
    anyhow::bail!("active task signalling is not implemented for this platform")
}

#[cfg(unix)]
fn foreground_process_group_id(pty_master: &File) -> Option<u32> {
    let foreground = unsafe { libc::tcgetpgrp(pty_master.as_raw_fd()) };
    (foreground > 0).then_some(foreground as u32)
}

#[cfg(not(unix))]
fn foreground_process_group_id(_pty_master: &File) -> Option<u32> {
    None
}

#[cfg(target_os = "linux")]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(target_os = "macos")]
fn process_cwd(pid: u32) -> Option<PathBuf> {
    let output = Command::new("/usr/sbin/lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| parse_lsof_cwd(&String::from_utf8_lossy(&output.stdout)))?
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn process_cwd(_pid: u32) -> Option<PathBuf> {
    None
}

#[cfg(not(unix))]
fn process_cwd(_pid: u32) -> Option<PathBuf> {
    None
}

#[cfg(unix)]
fn active_process_in_group(process_group_id: u32) -> Option<u32> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,pgid=,stat="])
        .output()
        .ok()?;
    output.status.success().then(|| {
        parse_process_table_for_group(&String::from_utf8_lossy(&output.stdout), process_group_id)
    })?
}

#[cfg(not(unix))]
fn active_process_in_group(_process_group_id: u32) -> Option<u32> {
    None
}

#[cfg(target_os = "linux")]
fn process_command(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .ok()
        .map(|command| command.trim().to_string())
        .filter(|command| !command.is_empty())
}

#[cfg(all(unix, not(target_os = "linux")))]
fn process_command(pid: u32) -> Option<String> {
    let output = Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "comm="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let command = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!command.is_empty()).then_some(command)
}

#[cfg(not(unix))]
fn process_command(_pid: u32) -> Option<String> {
    None
}

pub(crate) fn parse_process_table_for_group(output: &str, process_group_id: u32) -> Option<u32> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse::<u32>().ok()?;
            let pgid = parts.next()?.parse::<u32>().ok()?;
            let stat = parts.next().unwrap_or_default();
            (pgid == process_group_id && !stat.contains('Z')).then_some(pid)
        })
        .max()
}

pub(crate) fn parse_lsof_cwd(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .find_map(|line| line.strip_prefix('n'))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}
