use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLogEntry {
    pub timestamp: String,
    pub level: String,
    pub source: String,
    pub unit: String,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogCommandCapability {
    #[default]
    Unknown,
    Full,
    Partial,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogPreset {
    #[default]
    All,
    Errors,
    Auth,
    Kernel,
    System,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceLogStatus {
    #[default]
    Unknown,
    Available {
        capability: LogCommandCapability,
        platform: String,
    },
    Unavailable,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceLogSnapshot {
    pub status: ResourceLogStatus,
    pub entries: Vec<ResourceLogEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LogCaptureCommand {
    pub command: String,
    pub capability: LogCommandCapability,
}

const LOG_UNAVAILABLE_MARKER: &str = "__OXIDE_LOG_UNAVAILABLE__";
const LOG_ERROR_MARKER: &str = "__OXIDE_LOG_ERROR__";
const LOG_CAPABILITY_MARKER: &str = "__OXIDE_LOG_CAPABILITY__";

pub fn build_log_snapshot_command(
    os_type: &str,
    preset: LogPreset,
    limit: usize,
) -> Result<LogCaptureCommand, String> {
    let limit = sanitize_log_limit(limit);
    let (command, capability) = match normalized_log_os(os_type) {
        LogOs::Linux => (
            build_linux_log_snapshot_command(preset, limit),
            LogCommandCapability::Full,
        ),
        LogOs::Mac => (
            build_macos_log_snapshot_command(preset, limit),
            LogCommandCapability::Partial,
        ),
        LogOs::Bsd => (
            build_bsd_log_snapshot_command(preset, limit),
            LogCommandCapability::Partial,
        ),
        LogOs::Windows => (
            build_windows_log_snapshot_command(preset, limit),
            LogCommandCapability::Partial,
        ),
        LogOs::Unsupported => {
            return Err(format!(
                "Host logs are not supported for remote OS {os_type}."
            ));
        }
    };
    Ok(LogCaptureCommand {
        command,
        capability,
    })
}

pub fn build_log_follow_command(
    os_type: &str,
    preset: LogPreset,
) -> Result<LogCaptureCommand, String> {
    let (command, capability) = match normalized_log_os(os_type) {
        LogOs::Linux => (
            build_linux_log_follow_command(preset),
            LogCommandCapability::Full,
        ),
        LogOs::Mac => (
            build_macos_log_follow_command(preset),
            LogCommandCapability::Partial,
        ),
        LogOs::Bsd => (
            build_bsd_log_follow_command(preset),
            LogCommandCapability::Partial,
        ),
        LogOs::Windows => (
            build_windows_log_follow_command(preset),
            LogCommandCapability::Partial,
        ),
        LogOs::Unsupported => {
            return Err(format!(
                "Host log following is not supported for remote OS {os_type}."
            ));
        }
    };
    Ok(LogCaptureCommand {
        command,
        capability,
    })
}

pub fn parse_log_snapshot(output: &str) -> ResourceLogSnapshot {
    let Some(section) = extract_section(output, "HOST_LOGS") else {
        return parse_loose_log_snapshot(output);
    };

    let mut entries = Vec::new();
    let mut capability = LogCommandCapability::Unknown;
    let mut platform = "unknown".to_string();
    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line == LOG_UNAVAILABLE_MARKER {
            return ResourceLogSnapshot {
                status: ResourceLogStatus::Unavailable,
                entries: Vec::new(),
            };
        }
        if let Some(message) = line.strip_prefix(LOG_ERROR_MARKER) {
            return ResourceLogSnapshot {
                status: ResourceLogStatus::Error {
                    message: clean_marker_message(message, "Host log command failed."),
                },
                entries: Vec::new(),
            };
        }
        if let Some((next_capability, next_platform)) = parse_log_capability_line(line) {
            capability = next_capability;
            platform = next_platform;
            continue;
        }
        if let Some(entry) = parse_log_json_line(line).or_else(|| parse_log_row_line(line)) {
            entries.push(entry);
        }
    }

    ResourceLogSnapshot {
        status: ResourceLogStatus::Available {
            capability,
            platform,
        },
        entries,
    }
}

pub fn visible_log_rows(
    entries: &[ResourceLogEntry],
    query: &str,
    preset: LogPreset,
) -> Vec<ResourceLogEntry> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| log_entry_matches_preset(entry, preset))
        .filter(|entry| query.is_empty() || log_entry_matches_query(entry, &query))
        .cloned()
        .collect()
}

pub fn log_row_signature(entry: &ResourceLogEntry) -> u64 {
    let mut hasher = DefaultHasher::new();
    entry.timestamp.hash(&mut hasher);
    entry.level.hash(&mut hasher);
    entry.source.hash(&mut hasher);
    entry.unit.hash(&mut hasher);
    entry.message.hash(&mut hasher);
    hasher.finish()
}

pub fn log_level_label_key(level: &str) -> &'static str {
    match normalized_log_level(level).as_str() {
        "error" => "sidebar.host_logs.levels.error",
        "warning" => "sidebar.host_logs.levels.warning",
        "info" => "sidebar.host_logs.levels.info",
        "debug" => "sidebar.host_logs.levels.debug",
        _ => "sidebar.host_logs.levels.unknown",
    }
}

pub fn log_preset_label_key(preset: LogPreset) -> &'static str {
    match preset {
        LogPreset::All => "sidebar.host_logs.presets.all",
        LogPreset::Errors => "sidebar.host_logs.presets.errors",
        LogPreset::Auth => "sidebar.host_logs.presets.auth",
        LogPreset::Kernel => "sidebar.host_logs.presets.kernel",
        LogPreset::System => "sidebar.host_logs.presets.system",
    }
}

fn build_linux_log_snapshot_command(preset: LogPreset, limit: usize) -> String {
    let journal_args = linux_journalctl_preset_args(preset);
    let file_candidates = linux_log_file_candidates(preset).join(" ");
    let grep_filter = linux_log_grep_filter(preset);
    let file_filter = if grep_filter.is_empty() {
        String::new()
    } else {
        format!(" | grep -E -i {}", shell_quote(grep_filter))
    };
    format!(
        concat!(
            "echo '===HOST_LOGS==='; ",
            "if command -v journalctl >/dev/null 2>&1; then ",
            "echo '__OXIDE_LOG_CAPABILITY__\tfull\tlinux_systemd'; ",
            "oxide_logs=$(journalctl {journal_args} -n {limit} --no-pager -o json 2>&1); ",
            "oxide_status=$?; ",
            "if [ \"$oxide_status\" -eq 0 ]; then printf '%s\n' \"$oxide_logs\" | sed 's/^/JSON\t/'; ",
            "else printf '__OXIDE_LOG_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_logs\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
            "else ",
            "echo '__OXIDE_LOG_CAPABILITY__\tpartial\tlinux_files'; ",
            "oxide_found=0; ",
            "for oxide_log_file in {file_candidates}; do ",
            "[ -r \"$oxide_log_file\" ] && oxide_found=1; ",
            "done; ",
            "if [ \"$oxide_found\" -eq 1 ]; then ",
            "for oxide_log_file in {file_candidates}; do ",
            "[ -r \"$oxide_log_file\" ] || continue; ",
            "tail -n {limit} \"$oxide_log_file\" 2>/dev/null{file_filter} | awk -v src=\"$oxide_log_file\" '{{ gsub(/\\t/, \" \"); printf \"ROW\\t\\tinfo\\t%s\\t\\t%s\\n\", src, $0 }}'; ",
            "done | tail -n {limit}; ",
            "else echo '__OXIDE_LOG_UNAVAILABLE__'; ",
            "fi; ",
            "echo '===HOST_LOGS_END==='"
        ),
        journal_args = journal_args,
        limit = limit,
        file_candidates = file_candidates,
        file_filter = file_filter,
    )
}

fn build_linux_log_follow_command(preset: LogPreset) -> String {
    let journal_args = linux_journalctl_preset_args(preset);
    let file_candidates = linux_log_file_candidates(preset).join(" ");
    format!(
        "if command -v journalctl >/dev/null 2>&1; then journalctl {journal_args} -f --no-pager; else tail -f {file_candidates}; fi"
    )
}

fn build_macos_log_snapshot_command(preset: LogPreset, limit: usize) -> String {
    let predicate = macos_log_predicate(preset);
    format!(
        concat!(
            "echo '===HOST_LOGS==='; ",
            "if command -v log >/dev/null 2>&1; then ",
            "echo '__OXIDE_LOG_CAPABILITY__\tpartial\tmacos_log'; ",
            "oxide_logs=$(log show --last 1h --style compact {predicate} 2>&1 | tail -n {limit}); ",
            "oxide_status=$?; ",
            "if [ \"$oxide_status\" -eq 0 ]; then printf '%s\n' \"$oxide_logs\" | awk '{{ gsub(/\\t/, \" \"); printf \"ROW\\t\\tinfo\\tmacos_log\\t\\t%s\\n\", $0 }}'; ",
            "else printf '__OXIDE_LOG_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_logs\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
            "else echo '__OXIDE_LOG_UNAVAILABLE__'; fi; ",
            "echo '===HOST_LOGS_END==='"
        ),
        predicate = predicate,
        limit = limit,
    )
}

fn build_macos_log_follow_command(preset: LogPreset) -> String {
    let predicate = macos_log_predicate(preset);
    format!("log stream --style compact {predicate}")
}

fn build_bsd_log_snapshot_command(preset: LogPreset, limit: usize) -> String {
    let grep_filter = bsd_log_grep_filter(preset);
    let file_filter = if grep_filter.is_empty() {
        String::new()
    } else {
        format!(" | grep -E -i {}", shell_quote(grep_filter))
    };
    format!(
        concat!(
            "echo '===HOST_LOGS==='; ",
            "if [ -r /var/log/messages ]; then ",
            "echo '__OXIDE_LOG_CAPABILITY__\tpartial\tbsd_messages'; ",
            "tail -n {limit} /var/log/messages 2>/dev/null{file_filter} | awk '{{ gsub(/\\t/, \" \"); printf \"ROW\\t\\tinfo\\t/var/log/messages\\t\\t%s\\n\", $0 }}'; ",
            "else echo '__OXIDE_LOG_UNAVAILABLE__'; fi; ",
            "echo '===HOST_LOGS_END==='"
        ),
        limit = limit,
        file_filter = file_filter,
    )
}

fn build_bsd_log_follow_command(preset: LogPreset) -> String {
    let grep_filter = bsd_log_grep_filter(preset);
    if grep_filter.is_empty() {
        "tail -f /var/log/messages".to_string()
    } else {
        format!(
            "tail -f /var/log/messages | grep --line-buffered -E -i {}",
            shell_quote(grep_filter)
        )
    }
}

fn build_windows_log_snapshot_command(preset: LogPreset, limit: usize) -> String {
    let script = windows_log_script(preset, limit, false);
    format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{script}\"")
}

fn build_windows_log_follow_command(preset: LogPreset) -> String {
    let script = windows_log_script(preset, 100, true);
    format!("powershell -NoProfile -ExecutionPolicy Bypass -Command \"{script}\"")
}

fn windows_log_script(preset: LogPreset, limit: usize, follow: bool) -> String {
    let log_name = match preset {
        LogPreset::Auth => "Security",
        _ => "System",
    };
    let level_filter = if matches!(preset, LogPreset::Errors) {
        "|Where-Object{$_.LevelDisplayName -match 'Error|Critical'}"
    } else if matches!(preset, LogPreset::Kernel) {
        "|Where-Object{$_.ProviderName -match 'Kernel'}"
    } else {
        ""
    };
    let emit = format!(
        concat!(
            "$events=Get-WinEvent -LogName '{log_name}' -MaxEvents {limit} -ErrorAction Stop{level_filter};",
            "$events|Sort-Object TimeCreated|ForEach-Object{{",
            "$msg=([string]$_.Message -replace '[\\r\\n\\t]+',' ');",
            "$level=if($_.LevelDisplayName){{[string]$_.LevelDisplayName}}else{{'Info'}};",
            "$provider=if($_.ProviderName){{[string]$_.ProviderName}}else{{'{log_name}'}};",
            "Write-Output ('ROW'+[char]9+$_.TimeCreated.ToString('s')+[char]9+$level+[char]9+$provider+[char]9+'{log_name}'+[char]9+$msg)",
            "}};"
        ),
        log_name = log_name,
        limit = limit,
        level_filter = level_filter,
    );
    let body = if follow {
        format!(
            "while($true){{try{{{emit}}}catch{{Write-Output ('__OXIDE_LOG_ERROR__'+[char]9+$_.Exception.Message)}};Start-Sleep -Seconds 2}}"
        )
    } else {
        format!(
            "try{{{emit}}}catch{{Write-Output ('__OXIDE_LOG_ERROR__'+[char]9+$_.Exception.Message)}};"
        )
    };
    format!(
        "Write-Output '===HOST_LOGS===';Write-Output ('__OXIDE_LOG_CAPABILITY__'+[char]9+'partial'+[char]9+'windows_powershell');{body}Write-Output '===HOST_LOGS_END===';"
    )
}

fn parse_loose_log_snapshot(output: &str) -> ResourceLogSnapshot {
    let entries = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| ResourceLogEntry {
            timestamp: String::new(),
            level: infer_log_level(line),
            source: String::new(),
            unit: String::new(),
            message: line.trim().to_string(),
        })
        .collect();
    ResourceLogSnapshot {
        status: ResourceLogStatus::Available {
            capability: LogCommandCapability::Unknown,
            platform: "plain_text".to_string(),
        },
        entries,
    }
}

fn parse_log_capability_line(line: &str) -> Option<(LogCommandCapability, String)> {
    let payload = line.strip_prefix(LOG_CAPABILITY_MARKER)?;
    let parts = payload
        .trim_start_matches('\t')
        .split('\t')
        .collect::<Vec<_>>();
    let capability = match parts.first().copied().unwrap_or("unknown") {
        "full" => LogCommandCapability::Full,
        "partial" => LogCommandCapability::Partial,
        _ => LogCommandCapability::Unknown,
    };
    Some((
        capability,
        parts
            .get(1)
            .copied()
            .unwrap_or("unknown")
            .trim()
            .to_string(),
    ))
}

fn parse_log_json_line(line: &str) -> Option<ResourceLogEntry> {
    let json = line.strip_prefix("JSON\t").unwrap_or(line);
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let message = json_string(&value, "MESSAGE")?;
    let level = value
        .get("PRIORITY")
        .and_then(journal_priority_value)
        .as_deref()
        .map(journal_priority_label)
        .unwrap_or_else(|| infer_log_level(&message));
    let source = json_string(&value, "SYSLOG_IDENTIFIER")
        .or_else(|| json_string(&value, "_COMM"))
        .or_else(|| json_string(&value, "_TRANSPORT"))
        .unwrap_or_default();
    let unit = json_string(&value, "_SYSTEMD_UNIT")
        .or_else(|| json_string(&value, "UNIT"))
        .unwrap_or_default();
    let timestamp = json_string(&value, "__REALTIME_TIMESTAMP")
        .or_else(|| json_string(&value, "_SOURCE_REALTIME_TIMESTAMP"))
        .or_else(|| json_string(&value, "__MONOTONIC_TIMESTAMP"))
        .unwrap_or_default();
    Some(ResourceLogEntry {
        timestamp,
        level,
        source,
        unit,
        message,
    })
}

fn journal_priority_value(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(ToString::to_string)
        .or_else(|| value.as_u64().map(|priority| priority.to_string()))
}

fn parse_log_row_line(line: &str) -> Option<ResourceLogEntry> {
    let payload = line.strip_prefix("ROW\t")?;
    let parts = payload.splitn(5, '\t').collect::<Vec<_>>();
    if parts.len() < 5 {
        return None;
    }
    Some(ResourceLogEntry {
        timestamp: clean_log_field(parts[0]),
        level: normalize_log_level_or_infer(parts[1], parts[4]),
        source: clean_log_field(parts[2]),
        unit: clean_log_field(parts[3]),
        message: clean_log_field(parts[4]),
    })
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|field| field.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn journal_priority_label(priority: &str) -> String {
    match priority.trim() {
        "0" | "1" | "2" | "3" => "error".to_string(),
        "4" => "warning".to_string(),
        "7" => "debug".to_string(),
        _ => "info".to_string(),
    }
}

fn infer_log_level(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("panic")
        || lower.contains("critical")
        || lower.contains("error")
        || lower.contains("failed")
        || lower.contains("failure")
    {
        "error".to_string()
    } else if lower.contains("warn") {
        "warning".to_string()
    } else if lower.contains("debug") {
        "debug".to_string()
    } else {
        "info".to_string()
    }
}

fn normalize_log_level_or_infer(level: &str, message: &str) -> String {
    let normalized = normalized_log_level(level);
    if normalized == "unknown" {
        infer_log_level(message)
    } else if normalized == "info" {
        let inferred = infer_log_level(message);
        if inferred == "info" {
            normalized
        } else {
            inferred
        }
    } else {
        normalized
    }
}

fn normalized_log_level(level: &str) -> String {
    match level.trim().to_lowercase().as_str() {
        "0" | "1" | "2" | "3" | "err" | "error" | "critical" | "crit" => "error".to_string(),
        "4" | "warn" | "warning" => "warning".to_string(),
        "7" | "debug" => "debug".to_string(),
        "5" | "6" | "info" | "notice" | "information" | "informational" => "info".to_string(),
        _ => "unknown".to_string(),
    }
}

fn log_entry_matches_query(entry: &ResourceLogEntry, query: &str) -> bool {
    entry.timestamp.to_lowercase().contains(query)
        || entry.level.to_lowercase().contains(query)
        || entry.source.to_lowercase().contains(query)
        || entry.unit.to_lowercase().contains(query)
        || entry.message.to_lowercase().contains(query)
}

fn log_entry_matches_preset(entry: &ResourceLogEntry, preset: LogPreset) -> bool {
    match preset {
        LogPreset::All => true,
        LogPreset::Errors => {
            normalized_log_level(&entry.level) == "error"
                || infer_log_level(&entry.message) == "error"
        }
        LogPreset::Auth => {
            let haystack = format!(
                "{} {} {}",
                entry.source.to_lowercase(),
                entry.unit.to_lowercase(),
                entry.message.to_lowercase()
            );
            haystack.contains("ssh")
                || haystack.contains("auth")
                || haystack.contains("sudo")
                || haystack.contains("login")
        }
        LogPreset::Kernel => {
            let haystack = format!(
                "{} {} {}",
                entry.source.to_lowercase(),
                entry.unit.to_lowercase(),
                entry.message.to_lowercase()
            );
            haystack.contains("kernel") || haystack.contains("kern")
        }
        LogPreset::System => true,
    }
}

fn linux_journalctl_preset_args(preset: LogPreset) -> &'static str {
    match preset {
        LogPreset::All | LogPreset::System => "",
        LogPreset::Errors => "-p err..alert",
        LogPreset::Auth => "-u ssh -u sshd -u systemd-logind",
        LogPreset::Kernel => "-k",
    }
}

fn linux_log_file_candidates(preset: LogPreset) -> Vec<&'static str> {
    match preset {
        LogPreset::Auth => vec!["/var/log/auth.log", "/var/log/secure", "/var/log/messages"],
        LogPreset::Kernel => vec!["/var/log/kern.log", "/var/log/messages", "/var/log/syslog"],
        _ => vec![
            "/var/log/syslog",
            "/var/log/messages",
            "/var/log/auth.log",
            "/var/log/kern.log",
        ],
    }
}

fn linux_log_grep_filter(preset: LogPreset) -> &'static str {
    match preset {
        LogPreset::Errors => "error|failed|failure|critical|panic",
        LogPreset::Auth => "ssh|sshd|auth|sudo|login",
        LogPreset::Kernel => "kernel|kern",
        LogPreset::All | LogPreset::System => "",
    }
}

fn macos_log_predicate(preset: LogPreset) -> String {
    match preset {
        LogPreset::All | LogPreset::System => String::new(),
        LogPreset::Errors => "--predicate 'eventType == logEvent AND messageType == error'".to_string(),
        LogPreset::Auth => "--predicate 'process == \"sshd\" OR eventMessage CONTAINS \"auth\" OR eventMessage CONTAINS \"login\"'".to_string(),
        LogPreset::Kernel => "--predicate 'sender CONTAINS \"kernel\" OR process == \"kernel\"'".to_string(),
    }
}

fn bsd_log_grep_filter(preset: LogPreset) -> &'static str {
    match preset {
        LogPreset::Errors => "error|failed|failure|critical|panic",
        LogPreset::Auth => "ssh|sshd|auth|sudo|login",
        LogPreset::Kernel => "kernel|kern",
        LogPreset::All | LogPreset::System => "",
    }
}

fn clean_marker_message(message: &str, fallback: &str) -> String {
    let trimmed = message.trim_start_matches('\t').trim();
    if trimmed.is_empty() {
        fallback.to_string()
    } else {
        trimmed.to_string()
    }
}

fn clean_log_field(value: &str) -> String {
    value
        .replace(['\r', '\n', '\t'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn sanitize_log_limit(limit: usize) -> usize {
    limit.clamp(50, 500)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn normalized_log_os(os_type: &str) -> LogOs {
    match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => LogOs::Linux,
        "macOS" | "macos" | "Darwin" => LogOs::Mac,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => LogOs::Bsd,
        "Windows" | "windows" => LogOs::Windows,
        _ => LogOs::Unsupported,
    }
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    Some(
        after_start
            .split_once(&end)
            .map_or(after_start, |(section, _)| section),
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LogOs {
    Linux,
    Mac,
    Bsd,
    Windows,
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_journalctl_json_rows() {
        let output = concat!(
            "===HOST_LOGS===\n",
            "__OXIDE_LOG_CAPABILITY__\tfull\tlinux_systemd\n",
            "JSON\t{\"__REALTIME_TIMESTAMP\":\"1713940000000000\",\"PRIORITY\":\"3\",\"SYSLOG_IDENTIFIER\":\"sshd\",\"_SYSTEMD_UNIT\":\"ssh.service\",\"MESSAGE\":\"Failed password for root\"}\n",
            "JSON\t{\"PRIORITY\":6,\"_COMM\":\"kernel\",\"MESSAGE\":\"boot complete\"}\n",
            "===HOST_LOGS_END===\n"
        );

        let snapshot = parse_log_snapshot(output);

        assert_eq!(
            snapshot.status,
            ResourceLogStatus::Available {
                capability: LogCommandCapability::Full,
                platform: "linux_systemd".to_string(),
            }
        );
        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.entries[0].level, "error");
        assert_eq!(snapshot.entries[0].source, "sshd");
        assert_eq!(snapshot.entries[0].unit, "ssh.service");
        assert_eq!(snapshot.entries[1].level, "info");
    }

    #[test]
    fn parses_syslog_fallback_rows_and_filters() {
        let output = concat!(
            "===HOST_LOGS===\n",
            "__OXIDE_LOG_CAPABILITY__\tpartial\tlinux_files\n",
            "ROW\t\tinfo\t/var/log/syslog\t\tApr 24 host sshd[1]: Accepted publickey\n",
            "ROW\t\tinfo\t/var/log/syslog\t\tApr 24 host kernel: disk error\n",
            "===HOST_LOGS_END===\n"
        );

        let snapshot = parse_log_snapshot(output);
        let auth_rows = visible_log_rows(&snapshot.entries, "", LogPreset::Auth);
        let error_rows = visible_log_rows(&snapshot.entries, "disk", LogPreset::Errors);

        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(auth_rows.len(), 1);
        assert_eq!(error_rows.len(), 1);
        assert_eq!(error_rows[0].level, "error");
    }

    #[test]
    fn parses_macos_compact_rows() {
        let output = concat!(
            "===HOST_LOGS===\n",
            "__OXIDE_LOG_CAPABILITY__\tpartial\tmacos_log\n",
            "ROW\t\tinfo\tmacos_log\t\t2026-04-24 18:55:39.194 INFO WindowServer ready\n",
            "===HOST_LOGS_END===\n"
        );

        let snapshot = parse_log_snapshot(output);

        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].source, "macos_log");
        assert!(snapshot.entries[0].message.contains("WindowServer"));
    }

    #[test]
    fn parses_bsd_tail_rows() {
        let output = concat!(
            "===HOST_LOGS===\n",
            "__OXIDE_LOG_CAPABILITY__\tpartial\tbsd_messages\n",
            "ROW\t\tinfo\t/var/log/messages\t\tApr 24 host cron[10]: job started\n",
            "===HOST_LOGS_END===\n"
        );

        let snapshot = parse_log_snapshot(output);

        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].source, "/var/log/messages");
    }

    #[test]
    fn parses_windows_get_winevent_rows() {
        let output = concat!(
            "===HOST_LOGS===\n",
            "__OXIDE_LOG_CAPABILITY__\tpartial\twindows_powershell\n",
            "ROW\t2026-04-24T18:55:39\tError\tService Control Manager\tSystem\tService failed to start\n",
            "===HOST_LOGS_END===\n"
        );

        let snapshot = parse_log_snapshot(output);

        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.entries[0].level, "error");
        assert_eq!(snapshot.entries[0].unit, "System");
    }

    #[test]
    fn log_commands_keep_windows_powershell_separate() {
        let linux = build_log_snapshot_command("Linux", LogPreset::All, 300).unwrap();
        let windows = build_log_snapshot_command("Windows", LogPreset::All, 300).unwrap();
        let follow = build_log_follow_command("Linux", LogPreset::Kernel).unwrap();

        assert!(linux.command.contains("journalctl"));
        assert!(linux.command.contains("done | tail -n 300"));
        assert!(
            !linux
                .command
                .contains("else echo '__OXIDE_LOG_UNAVAILABLE__'; done;")
        );
        assert_eq!(linux.capability, LogCommandCapability::Full);
        assert!(windows.command.contains("Get-WinEvent"));
        assert!(windows.command.starts_with("powershell "));
        assert!(follow.command.contains("journalctl -k -f --no-pager"));
    }
}
