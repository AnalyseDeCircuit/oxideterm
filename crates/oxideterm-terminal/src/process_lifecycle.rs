#[cfg(unix)]
use std::{
    collections::{HashMap, HashSet, VecDeque},
    thread,
    time::Duration,
};

#[cfg(all(unix, not(target_os = "linux")))]
use std::process::Command;

#[cfg(windows)]
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject,
        },
        Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
    },
};

#[cfg(unix)]
const LOCAL_PTY_TERMINATE_GRACE: Duration = Duration::from_millis(350);

#[cfg(unix)]
#[derive(Clone, Debug, Eq, PartialEq)]
struct UnixProcessEntry {
    pid: u32,
    ppid: u32,
    pgid: u32,
    session_id: u32,
    is_zombie: bool,
}

#[cfg(unix)]
pub(crate) fn cleanup_local_pty_process_tree(shell_pid: Option<u32>) {
    let Some(root_pid) = shell_pid else {
        return;
    };

    let first_snapshot = unix_process_snapshot();
    let mut targets = unix_related_processes(root_pid, &first_snapshot);
    if targets.is_empty() {
        return;
    }

    // Close semantics differ from the interactive "terminate foreground task"
    // command: closing a local PTY owns the whole shell subtree, including
    // background jobs that are no longer the foreground process group.
    signal_unix_targets(&targets, libc::SIGTERM);
    thread::sleep(LOCAL_PTY_TERMINATE_GRACE);

    let second_snapshot = unix_process_snapshot();
    targets.extend(unix_related_processes(root_pid, &second_snapshot));
    targets.retain(|entry| unix_pid_is_alive(entry.pid));
    if !targets.is_empty() {
        signal_unix_targets(&targets, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
pub(crate) fn cleanup_local_pty_process_tree(_shell_pid: Option<u32>) {}

#[cfg(unix)]
fn signal_unix_targets(entries: &[UnixProcessEntry], signal: libc::c_int) {
    let own_pid = std::process::id();
    let own_pgid = unsafe { libc::getpgrp() };
    let mut signaled_groups = HashSet::new();

    for entry in entries.iter().filter(|entry| !entry.is_zombie) {
        if entry.pgid > 0 && entry.pgid as libc::pid_t != own_pgid {
            if !signaled_groups.insert(entry.pgid) {
                continue;
            }
            let target = -(entry.pgid as libc::pid_t);
            // Process groups catch normal shells, foreground tasks, and
            // background jobs without racing every member PID individually.
            if unsafe { libc::kill(target, signal) } == 0 {
                continue;
            }
        }

        if entry.pid != own_pid {
            let _ = unsafe { libc::kill(entry.pid as libc::pid_t, signal) };
        }
    }
}

#[cfg(unix)]
fn unix_pid_is_alive(pid: u32) -> bool {
    let status = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if status == 0 {
        return true;
    }

    std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

#[cfg(unix)]
fn unix_related_processes(root_pid: u32, entries: &[UnixProcessEntry]) -> Vec<UnixProcessEntry> {
    let mut children_by_parent: HashMap<u32, Vec<&UnixProcessEntry>> = HashMap::new();
    for entry in entries {
        children_by_parent
            .entry(entry.ppid)
            .or_default()
            .push(entry);
    }

    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([root_pid]);
    let mut related = Vec::new();
    while let Some(pid) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        if let Some(entry) = entries.iter().find(|entry| entry.pid == pid) {
            related.push(entry.clone());
        }
        if let Some(children) = children_by_parent.get(&pid) {
            for child in children {
                queue.push_back(child.pid);
            }
        }
    }

    related
}

#[cfg(target_os = "linux")]
fn unix_process_snapshot() -> Vec<UnixProcessEntry> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };

    entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_string_lossy().parse::<u32>().ok())
        .filter_map(|pid| std::fs::read_to_string(format!("/proc/{pid}/stat")).ok())
        .filter_map(|stat| parse_linux_proc_stat(&stat))
        .collect()
}

#[cfg(all(unix, not(target_os = "linux")))]
fn unix_process_snapshot() -> Vec<UnixProcessEntry> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,pgid=,sess=,stat=,comm="])
        .output();
    let Ok(output) = output else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }

    parse_ps_process_snapshot(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "linux")]
fn parse_linux_proc_stat(stat: &str) -> Option<UnixProcessEntry> {
    let pid_end = stat.find(" (")?;
    let pid = stat[..pid_end].trim().parse::<u32>().ok()?;
    let after_command = stat.rsplit_once(") ")?.1;
    let mut parts = after_command.split_whitespace();
    let state = parts.next()?;
    let ppid = parts.next()?.parse::<u32>().ok()?;
    let pgid = parts.next()?.parse::<u32>().ok()?;
    let session_id = parts.next()?.parse::<u32>().ok()?;

    Some(UnixProcessEntry {
        pid,
        ppid,
        pgid,
        session_id,
        is_zombie: state == "Z",
    })
}

#[cfg(all(unix, not(target_os = "linux")))]
fn parse_ps_process_snapshot(output: &str) -> Vec<UnixProcessEntry> {
    output
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let pid = parts.next()?.parse::<u32>().ok()?;
            let ppid = parts.next()?.parse::<u32>().ok()?;
            let pgid = parts.next()?.parse::<u32>().ok()?;
            let session_id = parts.next()?.parse::<u32>().ok()?;
            let stat = parts.next().unwrap_or_default();
            Some(UnixProcessEntry {
                pid,
                ppid,
                pgid,
                session_id,
                is_zombie: stat.contains('Z'),
            })
        })
        .collect()
}

#[cfg(windows)]
pub(crate) struct WindowsTerminalJob {
    handle: HANDLE,
}

#[cfg(windows)]
// Windows kernel HANDLE values are process-wide references. This type owns a
// job handle and only closes or terminates it through Win32 APIs, so moving
// ownership to the terminal session's worker boundary is safe.
unsafe impl Send for WindowsTerminalJob {}

#[cfg(windows)]
impl WindowsTerminalJob {
    pub(crate) fn for_shell(shell_pid: Option<u32>) -> Option<Self> {
        let pid = shell_pid?;
        unsafe {
            let job = CreateJobObjectW(None, None).ok()?;
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let info_size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
            if SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const _,
                info_size,
            )
            .is_err()
            {
                let _ = CloseHandle(job);
                return None;
            }

            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid).ok()?;
            if AssignProcessToJobObject(job, process).is_err() {
                let _ = CloseHandle(process);
                let _ = CloseHandle(job);
                return None;
            }
            let _ = CloseHandle(process);
            Some(Self { handle: job })
        }
    }

    pub(crate) fn terminate(&self) {
        let _ = unsafe { TerminateJobObject(self.handle, 1) };
    }
}

#[cfg(windows)]
impl Drop for WindowsTerminalJob {
    fn drop(&mut self) {
        // The job is created with KILL_ON_JOB_CLOSE so app exit cleans up the
        // shell subtree even if the normal close path is bypassed.
        let _ = unsafe { CloseHandle(self.handle) };
    }
}

#[cfg(test)]
#[cfg(unix)]
mod tests {
    use super::*;

    #[test]
    fn unix_related_processes_walks_descendants_only() {
        let entries = vec![
            entry(10, 1, 10, 10, false),
            entry(11, 10, 10, 10, false),
            entry(12, 11, 12, 10, false),
            entry(20, 1, 20, 20, false),
        ];

        let related = unix_related_processes(10, &entries)
            .into_iter()
            .map(|entry| entry.pid)
            .collect::<Vec<_>>();

        assert_eq!(related, vec![10, 11, 12]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_proc_stat_parser_handles_command_names_with_spaces() {
        let entry = parse_linux_proc_stat("1234 (sleep worker) S 100 1234 1234 0 -1 0").unwrap();

        assert_eq!(entry.pid, 1234);
        assert_eq!(entry.ppid, 100);
        assert_eq!(entry.pgid, 1234);
        assert_eq!(entry.session_id, 1234);
        assert!(!entry.is_zombie);
    }

    #[cfg(all(unix, not(target_os = "linux")))]
    #[test]
    fn ps_snapshot_parser_marks_zombies() {
        let entries = parse_ps_process_snapshot("10 1 10 10 Ss sh\n11 10 10 10 Z sleep\n");

        assert_eq!(entries[0], entry(10, 1, 10, 10, false));
        assert_eq!(entries[1], entry(11, 10, 10, 10, true));
    }

    fn entry(pid: u32, ppid: u32, pgid: u32, session_id: u32, is_zombie: bool) -> UnixProcessEntry {
        UnixProcessEntry {
            pid,
            ppid,
            pgid,
            session_id,
            is_zombie,
        }
    }
}
