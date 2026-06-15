use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::metrics::ResourceTopProcess;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProcessActionKind {
    Term,
    Kill,
    Stop,
    Cont,
    Renice { nice: i32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessCommandCapability {
    Full,
    Partial,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessActionCommand {
    pub command: String,
    pub capability: ProcessCommandCapability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ProcessFilter {
    All,
    Running,
    HighCpu,
    HighMemory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ProcessSort {
    Cpu,
    Memory,
    Pid,
    Command,
    User,
}

const PROCESS_HIGH_CPU_THRESHOLD: f64 = 10.0;
const PROCESS_HIGH_MEMORY_THRESHOLD: f64 = 5.0;

/// Builds the remote command for one process action while preserving platform capability semantics.
pub fn build_process_action_command(
    os_type: &str,
    pid: &str,
    action: ProcessActionKind,
) -> Result<ProcessActionCommand, String> {
    let pid = validated_pid(pid)?;
    validate_process_action(&action)?;
    match normalized_process_os(os_type) {
        ProcessOs::LinuxLike => Ok(ProcessActionCommand {
            command: build_unix_process_action_command(pid, &action, true),
            capability: ProcessCommandCapability::Full,
        }),
        ProcessOs::MacOs | ProcessOs::Bsd => Ok(ProcessActionCommand {
            command: build_unix_process_action_command(pid, &action, false),
            capability: ProcessCommandCapability::Partial,
        }),
        ProcessOs::Windows => build_windows_process_action_command(pid, &action),
        ProcessOs::Unsupported => Err(format!(
            "Process management is not supported for remote OS {os_type}."
        )),
    }
}

/// Applies process search, filter, and ordering before the GPUI layer renders the table.
pub fn visible_process_rows(
    processes: &[ResourceTopProcess],
    query: &str,
    filter: ProcessFilter,
    sort: ProcessSort,
    descending: bool,
) -> Vec<ResourceTopProcess> {
    let mut rows = processes
        .iter()
        .filter(|process| process_matches_filter(process, filter))
        .filter(|process| process_matches_query(process, query))
        .cloned()
        .collect::<Vec<_>>();
    sort_process_rows(&mut rows, sort, descending);
    rows
}

/// Produces a stable row signature so virtual lists refresh only when process data changes.
pub fn process_row_signature(process: &ResourceTopProcess) -> u64 {
    let mut hasher = DefaultHasher::new();
    process.pid.hash(&mut hasher);
    process.ppid.hash(&mut hasher);
    process.user.hash(&mut hasher);
    process.state.hash(&mut hasher);
    process
        .cpu_percent
        .map(|value| value.to_bits())
        .hash(&mut hasher);
    process.memory_percent.to_bits().hash(&mut hasher);
    process.rss_bytes.hash(&mut hasher);
    process.vsz_bytes.hash(&mut hasher);
    process.elapsed.hash(&mut hasher);
    process.command.hash(&mut hasher);
    process.full_command.hash(&mut hasher);
    hasher.finish()
}

/// Prefers the full command line when the sampler captured it, falling back to the short command.
pub fn process_display_command(process: &ResourceTopProcess) -> String {
    process_usable_command(process.full_command.as_deref())
        .or_else(|| process_usable_command(Some(process.command.as_str())))
        .unwrap_or(process.command.as_str())
        .to_string()
}

/// Uses the short executable name for table rows so narrow sidebars do not hide process identity.
pub fn process_display_name(process: &ResourceTopProcess) -> String {
    process_usable_command(Some(process.command.as_str()))
        .or_else(|| process_usable_command(process.full_command.as_deref()))
        .unwrap_or(process.pid.as_str())
        .to_string()
}

/// Maps platform process states to UI translation keys without making GPUI own state semantics.
pub fn process_state_label_key(state: &str) -> &'static str {
    match state.chars().next() {
        Some('R') => "sidebar.host_processes.states.running",
        Some('S') => "sidebar.host_processes.states.sleeping",
        Some('D') => "sidebar.host_processes.states.waiting",
        Some('T') | Some('t') => "sidebar.host_processes.states.stopped",
        Some('Z') => "sidebar.host_processes.states.zombie",
        Some('I') => "sidebar.host_processes.states.idle",
        _ => "sidebar.host_processes.states.unknown",
    }
}

/// SSH command runners may omit exit status; absence is treated as success for compatibility.
pub fn process_action_succeeded(exit_code: Option<i32>) -> bool {
    exit_code.unwrap_or(0) == 0
}

/// Summarizes successful remote command output without exposing a multiline terminal dump in toasts.
pub fn process_action_success_message(stdout: &str, stderr: &str) -> String {
    compact_process_command_message(stdout)
        .or_else(|| compact_process_command_message(stderr))
        .unwrap_or_else(|| "Process action completed.".to_string())
}

/// Summarizes the remote failure reason, preferring stderr because process tools report there.
pub fn process_action_failure_message(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> String {
    compact_process_command_message(stderr)
        .or_else(|| compact_process_command_message(stdout))
        .unwrap_or_else(|| {
            exit_code
                .map(|code| format!("Process action failed with exit code {code}."))
                .unwrap_or_else(|| "Process action failed.".to_string())
        })
}

/// Evaluates semantic process filters from the captured snapshot fields.
pub fn process_matches_filter(process: &ResourceTopProcess, filter: ProcessFilter) -> bool {
    match filter {
        ProcessFilter::All => true,
        ProcessFilter::Running => process
            .state
            .as_deref()
            .is_some_and(|state| state.starts_with('R')),
        ProcessFilter::HighCpu => process
            .cpu_percent
            .is_some_and(|value| value >= PROCESS_HIGH_CPU_THRESHOLD),
        ProcessFilter::HighMemory => process.memory_percent >= PROCESS_HIGH_MEMORY_THRESHOLD,
    }
}

/// Checks the user query against PID, user, short command, and full command.
pub fn process_matches_query(process: &ResourceTopProcess, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    process.pid.to_lowercase().contains(&query)
        || process
            .user
            .as_deref()
            .is_some_and(|user| user.to_lowercase().contains(&query))
        || process.command.to_lowercase().contains(&query)
        || process
            .full_command
            .as_deref()
            .is_some_and(|command| command.to_lowercase().contains(&query))
}

/// Orders process rows using monitor-owned semantics instead of app-local table helpers.
pub fn sort_process_rows(rows: &mut [ResourceTopProcess], sort: ProcessSort, descending: bool) {
    rows.sort_by(|left, right| {
        let ordering = match sort {
            ProcessSort::Cpu => compare_optional_f64(left.cpu_percent, right.cpu_percent),
            ProcessSort::Memory => left
                .memory_percent
                .partial_cmp(&right.memory_percent)
                .unwrap_or(std::cmp::Ordering::Equal),
            ProcessSort::Pid => {
                process_pid_sort_value(&left.pid).cmp(&process_pid_sort_value(&right.pid))
            }
            ProcessSort::Command => left
                .command
                .to_lowercase()
                .cmp(&right.command.to_lowercase()),
            ProcessSort::User => left
                .user
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .cmp(&right.user.as_deref().unwrap_or("").to_lowercase()),
        };
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
}

fn validated_pid(pid: &str) -> Result<&str, String> {
    let pid = pid.trim();
    if pid.is_empty() || !pid.chars().all(|character| character.is_ascii_digit()) {
        return Err("PID must be a positive integer.".to_string());
    }
    if pid == "0" {
        return Err("PID 0 cannot be managed from the process table.".to_string());
    }
    Ok(pid)
}

fn validate_process_action(action: &ProcessActionKind) -> Result<(), String> {
    if let ProcessActionKind::Renice { nice } = action
        && !(-20..=19).contains(nice)
    {
        return Err("nice value must be between -20 and 19.".to_string());
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProcessOs {
    LinuxLike,
    MacOs,
    Bsd,
    Windows,
    Unsupported,
}

fn normalized_process_os(os_type: &str) -> ProcessOs {
    match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
            ProcessOs::LinuxLike
        }
        "macOS" | "macos" | "Darwin" => ProcessOs::MacOs,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => ProcessOs::Bsd,
        "Windows" | "windows" => ProcessOs::Windows,
        _ => ProcessOs::Unsupported,
    }
}

fn build_unix_process_action_command(
    pid: &str,
    action: &ProcessActionKind,
    prefer_proc: bool,
) -> String {
    let existence_check = if prefer_proc {
        format!(
            "if [ ! -d /proc/{pid} ] && ! ps -p {pid} >/dev/null 2>&1; then echo 'Process {pid} not found'; exit 3; fi;"
        )
    } else {
        format!(
            "if ! ps -p {pid} >/dev/null 2>&1; then echo 'Process {pid} not found'; exit 3; fi;"
        )
    };
    let action_command = match action {
        ProcessActionKind::Term => format!("kill -TERM -- {pid}"),
        ProcessActionKind::Kill => format!("kill -KILL -- {pid}"),
        ProcessActionKind::Stop => format!("kill -STOP -- {pid}"),
        ProcessActionKind::Cont => format!("kill -CONT -- {pid}"),
        ProcessActionKind::Renice { nice } => format!("renice -n {nice} -p {pid}"),
    };
    let success = match action {
        ProcessActionKind::Term => format!("Sent TERM to PID {pid}"),
        ProcessActionKind::Kill => format!("Sent KILL to PID {pid}"),
        ProcessActionKind::Stop => format!("Sent STOP to PID {pid}"),
        ProcessActionKind::Cont => format!("Sent CONT to PID {pid}"),
        ProcessActionKind::Renice { nice } => format!("Set PID {pid} nice value to {nice}"),
    };
    format!(
        "{existence_check} if {action_command}; then echo '{success}'; else status=$?; echo 'Process action failed' >&2; exit $status; fi"
    )
}

fn build_windows_process_action_command(
    pid: &str,
    action: &ProcessActionKind,
) -> Result<ProcessActionCommand, String> {
    let (operation, success) = match action {
        ProcessActionKind::Term => (
            format!("Stop-Process -Id {pid} -ErrorAction Stop"),
            format!("Stopped PID {pid}"),
        ),
        ProcessActionKind::Kill => (
            format!("Stop-Process -Id {pid} -Force -ErrorAction Stop"),
            format!("Force stopped PID {pid}"),
        ),
        ProcessActionKind::Stop | ProcessActionKind::Cont | ProcessActionKind::Renice { .. } => {
            return Err("This process action is not supported on Windows OpenSSH yet.".to_string());
        }
    };
    let script = format!(
        "$ErrorActionPreference='Stop'; try {{ Get-Process -Id {pid} -ErrorAction Stop | Out-Null; {operation}; Write-Output '{success}'; exit 0 }} catch {{ Write-Error $_.Exception.Message; exit 1 }}"
    );
    Ok(ProcessActionCommand {
        command: format!(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"{}\"",
            script.replace('"', "`\"")
        ),
        capability: ProcessCommandCapability::Partial,
    })
}

fn compare_optional_f64(left: Option<f64>, right: Option<f64>) -> std::cmp::Ordering {
    left.unwrap_or(f64::NEG_INFINITY)
        .partial_cmp(&right.unwrap_or(f64::NEG_INFINITY))
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn process_pid_sort_value(pid: &str) -> u64 {
    pid.parse::<u64>().unwrap_or(0)
}

fn compact_process_command_message(value: &str) -> Option<String> {
    let summary = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .chars()
        .take(180)
        .collect::<String>();
    Some(summary)
}

fn process_usable_command(value: Option<&str>) -> Option<&str> {
    let command = value?.trim();
    (!command.is_empty() && command != "..." && command != "…").then_some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_process_actions_prefer_proc_and_are_full() {
        let command =
            build_process_action_command("Linux", "123", ProcessActionKind::Kill).unwrap();
        assert_eq!(command.capability, ProcessCommandCapability::Full);
        assert!(command.command.contains("/proc/123"));
        assert!(command.command.contains("kill -KILL -- 123"));
    }

    #[test]
    fn macos_and_bsd_are_partial_and_use_ps_checks() {
        let mac = build_process_action_command("macOS", "123", ProcessActionKind::Term).unwrap();
        let bsd =
            build_process_action_command("FreeBSD", "123", ProcessActionKind::Renice { nice: 5 })
                .unwrap();
        assert_eq!(mac.capability, ProcessCommandCapability::Partial);
        assert_eq!(bsd.capability, ProcessCommandCapability::Partial);
        assert!(!mac.command.contains("/proc/123"));
        assert!(bsd.command.contains("renice -n 5 -p 123"));
    }

    #[test]
    fn windows_process_actions_use_powershell_only_for_supported_actions() {
        let term = build_process_action_command("Windows", "123", ProcessActionKind::Term).unwrap();
        assert_eq!(term.capability, ProcessCommandCapability::Partial);
        assert!(term.command.contains("powershell"));
        assert!(term.command.contains("Stop-Process -Id 123"));
        assert!(build_process_action_command("Windows", "123", ProcessActionKind::Stop).is_err());
    }

    #[test]
    fn process_actions_validate_pid_and_renice_range() {
        assert!(build_process_action_command("Linux", "abc", ProcessActionKind::Term).is_err());
        assert!(
            build_process_action_command("Linux", "123", ProcessActionKind::Renice { nice: 20 })
                .is_err()
        );
    }

    fn process(
        pid: &str,
        user: &str,
        state: &str,
        cpu_percent: f64,
        memory_percent: f64,
        command: &str,
    ) -> ResourceTopProcess {
        ResourceTopProcess {
            pid: pid.to_string(),
            ppid: Some("1".to_string()),
            user: Some(user.to_string()),
            state: Some(state.to_string()),
            cpu_percent: Some(cpu_percent),
            memory_percent,
            rss_bytes: Some(1024),
            vsz_bytes: Some(2048),
            elapsed: Some("00:01:00".to_string()),
            command: command.to_string(),
            full_command: Some(format!("/usr/bin/{command} --serve")),
        }
    }

    #[test]
    fn process_filters_use_available_snapshot_fields() {
        let running = process("10", "root", "R", 1.0, 1.0, "sshd");
        let cpu_heavy = process("11", "www-data", "S", 12.0, 1.0, "node");
        let memory_heavy = process("12", "postgres", "S", 1.0, 7.0, "postgres");

        assert!(process_matches_filter(&running, ProcessFilter::Running));
        assert!(process_matches_filter(&cpu_heavy, ProcessFilter::HighCpu));
        assert!(process_matches_filter(
            &memory_heavy,
            ProcessFilter::HighMemory
        ));
        assert!(process_matches_query(&cpu_heavy, "www"));
        assert!(process_matches_query(&memory_heavy, "postgres"));
    }

    #[test]
    fn process_sort_can_order_by_cpu_and_command() {
        let mut rows = vec![
            process("2", "root", "S", 1.0, 2.0, "zsh"),
            process("10", "www-data", "S", 19.0, 1.0, "node"),
            process("1", "root", "R", 3.0, 8.0, "init"),
        ];

        sort_process_rows(&mut rows, ProcessSort::Cpu, true);
        assert_eq!(rows[0].pid, "10");

        sort_process_rows(&mut rows, ProcessSort::Command, false);
        assert_eq!(rows[0].command, "init");
    }

    #[test]
    fn process_display_command_ignores_placeholder_full_command() {
        let mut process = process("2", "root", "S", 1.0, 2.0, "postgres");
        process.full_command = Some("...".to_string());

        assert_eq!(process_display_command(&process), "postgres");
    }

    #[test]
    fn process_display_name_prefers_short_command_for_table_rows() {
        let process = process("2", "root", "S", 1.0, 2.0, "postgres");

        assert_eq!(process_display_name(&process), "postgres");
    }

    #[test]
    fn process_action_messages_prefer_remote_reason() {
        assert_eq!(
            process_action_success_message("Sent TERM to PID 42\n", ""),
            "Sent TERM to PID 42"
        );
        assert_eq!(
            process_action_failure_message("", "Operation not permitted\n", Some(1)),
            "Operation not permitted"
        );
        assert_eq!(
            process_action_failure_message("", "", Some(3)),
            "Process action failed with exit code 3."
        );
    }
}
