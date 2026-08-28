use std::{
    fs::File,
    path::PathBuf,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
use std::os::fd::AsRawFd;

#[cfg(unix)]
use anyhow::Context;
use anyhow::Result;

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

pub struct TerminalProcessProbe {
    shell_pid: Option<u32>,
    pty_master: Option<File>,
    previous_command: Option<String>,
    previous_cwd: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum TerminalProcessProbeScope {
    Full,
    ForegroundOnly,
    CurrentDirectory,
}

impl TerminalProcessProbe {
    pub fn collect(self) -> TerminalProcessInfo {
        self.collect_for_scope(TerminalProcessProbeScope::Full)
    }

    pub fn collect_foreground_only(self) -> TerminalProcessInfo {
        self.collect_for_scope(TerminalProcessProbeScope::ForegroundOnly)
    }

    pub fn collect_current_directory(self) -> TerminalProcessInfo {
        self.collect_for_scope(TerminalProcessProbeScope::CurrentDirectory)
    }

    fn collect_for_scope(self, scope: TerminalProcessProbeScope) -> TerminalProcessInfo {
        // Process table and cwd discovery may launch platform tools. Callers must run this
        // probe on a blocking/background thread rather than on the terminal paint loop.
        let foreground_process_group_id = self
            .pty_master
            .as_ref()
            .and_then(foreground_process_group_id)
            .or(self.shell_pid);
        let foreground_pid = foreground_process_group_id
            .and_then(active_process_in_group)
            .or(foreground_process_group_id);
        let cwd = if matches!(
            scope,
            TerminalProcessProbeScope::Full | TerminalProcessProbeScope::CurrentDirectory
        ) {
            foreground_pid
                .and_then(process_cwd)
                .or_else(|| self.shell_pid.and_then(process_cwd))
                .or(self.previous_cwd)
        } else {
            // Close confirmation only needs foreground ownership. Preserve the cached cwd and
            // avoid launching the platform cwd lookup on that latency-sensitive path.
            self.previous_cwd
        };

        TerminalProcessInfo {
            shell_pid: self.shell_pid,
            foreground_pid,
            foreground_process_group_id,
            command: if matches!(scope, TerminalProcessProbeScope::Full) {
                foreground_pid.and_then(process_command)
            } else {
                self.previous_command
            },
            cwd,
        }
    }
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

    pub(crate) fn probe(&self) -> Option<TerminalProcessProbe> {
        if self.info.shell_pid.is_none() && self.pty_master.is_none() {
            return None;
        }
        Some(TerminalProcessProbe {
            shell_pid: self.info.shell_pid,
            // A duplicated descriptor lets the probe query the PTY without borrowing session
            // state or holding the terminal mutex while platform commands execute.
            pty_master: self
                .pty_master
                .as_ref()
                .and_then(|pty_master| pty_master.try_clone().ok()),
            previous_command: self.info.command.clone(),
            previous_cwd: self.info.cwd.clone(),
        })
    }

    pub(crate) fn apply_probe_result(&mut self, info: TerminalProcessInfo) -> bool {
        // Ignore a late result if the session was replaced while the probe was running.
        if info.shell_pid != self.info.shell_pid {
            return false;
        }
        self.last_refresh = Instant::now();
        if self.info == info {
            return false;
        }
        self.info = info;
        true
    }

    pub(crate) fn refresh(&mut self) {
        if self.last_refresh.elapsed() < PROCESS_INFO_REFRESH_INTERVAL {
            return;
        }
        if let Some(probe) = self.probe() {
            let _ = self.apply_probe_result(probe.collect());
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

#[cfg(any(unix, test))]
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

#[cfg(any(target_os = "macos", test))]
pub(crate) fn parse_lsof_cwd(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .find_map(|line| line.strip_prefix('n'))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}
