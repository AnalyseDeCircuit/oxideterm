use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceScheduledTask {
    pub id: String,
    pub name: String,
    pub source: String,
    pub schedule: String,
    pub command: String,
    pub user: String,
    pub enabled: String,
    pub active: String,
    pub last_run: String,
    pub next_run: String,
    pub last_result: String,
    pub description: String,
    pub unit: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScheduledTaskCapability {
    #[default]
    Unknown,
    Full,
    Partial,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceScheduledTaskStatus {
    #[default]
    Unknown,
    Available {
        capability: ScheduledTaskCapability,
        platform: String,
    },
    Unavailable,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceScheduledTaskSnapshot {
    pub status: ResourceScheduledTaskStatus,
    pub entries: Vec<ResourceScheduledTask>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ScheduledTaskFilter {
    #[default]
    All,
    Enabled,
    Disabled,
    Systemd,
    Cron,
    Launchd,
    Windows,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScheduledTaskActionKind {
    RunNow { id: String, unit: String },
    Enable { id: String, source: String },
    Disable { id: String, source: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledTaskCaptureCommand {
    pub command: String,
    pub capability: ScheduledTaskCapability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledTaskActionCommand {
    pub command: String,
    pub capability: ScheduledTaskCapability,
}

const SCHEDULED_TASK_UNAVAILABLE_MARKER: &str = "__OXIDE_SCHEDULE_UNAVAILABLE__";
const SCHEDULED_TASK_ERROR_MARKER: &str = "__OXIDE_SCHEDULE_ERROR__";
const SCHEDULED_TASK_CAPABILITY_MARKER: &str = "__OXIDE_SCHEDULE_CAPABILITY__";

pub fn build_scheduled_task_snapshot_command(os_type: &str) -> ScheduledTaskCaptureCommand {
    let (command, capability) = match scheduled_task_os(os_type) {
        ScheduledTaskOs::Linux | ScheduledTaskOs::Unknown => (
            build_linux_scheduled_task_snapshot_command(),
            ScheduledTaskCapability::Full,
        ),
        ScheduledTaskOs::MacOs => (
            build_macos_scheduled_task_snapshot_command(),
            ScheduledTaskCapability::Partial,
        ),
        ScheduledTaskOs::Bsd => (
            build_bsd_scheduled_task_snapshot_command(),
            ScheduledTaskCapability::Partial,
        ),
        ScheduledTaskOs::Windows => (
            build_windows_scheduled_task_snapshot_command(),
            ScheduledTaskCapability::Partial,
        ),
    };
    ScheduledTaskCaptureCommand {
        command,
        capability,
    }
}

pub fn build_scheduled_task_logs_command(
    os_type: &str,
    task: &ResourceScheduledTask,
    follow: bool,
    limit: usize,
) -> Result<ScheduledTaskCaptureCommand, String> {
    let limit = sanitize_log_limit(limit);
    let (command, capability) = match scheduled_task_os(os_type) {
        ScheduledTaskOs::Linux | ScheduledTaskOs::Unknown => {
            if task.source == "systemd" {
                let unit = if !task.unit.trim().is_empty() {
                    task.unit.trim()
                } else if task.id.ends_with(".timer") {
                    task.id.trim_end_matches(".timer").trim()
                } else {
                    task.id.trim()
                };
                if unit.is_empty() {
                    return Err("Scheduled task has no systemd unit.".to_string());
                }
                let command = if follow {
                    format!("journalctl -fu {} --no-pager", shell_quote(unit))
                } else {
                    format!("journalctl -u {} -n {limit} --no-pager", shell_quote(unit))
                };
                (command, ScheduledTaskCapability::Full)
            } else {
                let grep = shell_quote(&task.name);
                (
                    format!(
                        "if [ -r /var/log/syslog ]; then grep -F {grep} /var/log/syslog | tail -n {limit}; elif [ -r /var/log/cron ]; then grep -F {grep} /var/log/cron | tail -n {limit}; else echo 'Cron logs are not available for this host.'; fi"
                    ),
                    ScheduledTaskCapability::Partial,
                )
            }
        }
        ScheduledTaskOs::MacOs => (
            if follow {
                format!(
                    "log stream --style compact --predicate {}",
                    shell_quote(&format!("process CONTAINS[c] \"{}\"", task.name))
                )
            } else {
                format!(
                    "log show --last 1h --style compact --predicate {} | tail -n {limit}",
                    shell_quote(&format!("process CONTAINS[c] \"{}\"", task.name))
                )
            },
            ScheduledTaskCapability::Partial,
        ),
        ScheduledTaskOs::Bsd => (
            if follow {
                "tail -f /var/log/cron /var/log/messages 2>/dev/null".to_string()
            } else {
                format!("tail -n {limit} /var/log/cron /var/log/messages 2>/dev/null")
            },
            ScheduledTaskCapability::Partial,
        ),
        ScheduledTaskOs::Windows => (
            if follow {
                concat!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
                    "while($true){Get-WinEvent -LogName Microsoft-Windows-TaskScheduler/Operational -MaxEvents 50|",
                    "Select-Object TimeCreated,ProviderName,Id,LevelDisplayName,Message|Format-Table -Wrap -AutoSize;Start-Sleep -Seconds 5}\""
                )
                .to_string()
            } else {
                format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"Get-WinEvent -LogName Microsoft-Windows-TaskScheduler/Operational -MaxEvents {limit}|Select-Object TimeCreated,ProviderName,Id,LevelDisplayName,Message|Format-Table -Wrap -AutoSize\""
                )
            },
            ScheduledTaskCapability::Partial,
        ),
    };
    Ok(ScheduledTaskCaptureCommand {
        command,
        capability,
    })
}

pub fn build_scheduled_task_action_command(
    os_type: &str,
    action: ScheduledTaskActionKind,
) -> Result<ScheduledTaskActionCommand, String> {
    match (scheduled_task_os(os_type), action) {
        (
            ScheduledTaskOs::Linux | ScheduledTaskOs::Unknown,
            ScheduledTaskActionKind::RunNow { id, unit },
        ) => {
            let target = if !unit.trim().is_empty() {
                unit
            } else if id.ends_with(".timer") {
                id.trim_end_matches(".timer").to_string()
            } else {
                id
            };
            validate_systemd_unit(&target)?;
            Ok(ScheduledTaskActionCommand {
                command: format!("systemctl start {}", shell_quote(&target)),
                capability: ScheduledTaskCapability::Full,
            })
        }
        (
            ScheduledTaskOs::Linux | ScheduledTaskOs::Unknown,
            ScheduledTaskActionKind::Enable { id, source },
        ) => {
            if source != "systemd" {
                return Err(
                    "Enable is only safely supported for systemd scheduled tasks.".to_string(),
                );
            }
            validate_systemd_unit(&id)?;
            Ok(ScheduledTaskActionCommand {
                command: format!("systemctl enable {}", shell_quote(&id)),
                capability: ScheduledTaskCapability::Full,
            })
        }
        (
            ScheduledTaskOs::Linux | ScheduledTaskOs::Unknown,
            ScheduledTaskActionKind::Disable { id, source },
        ) => {
            if source != "systemd" {
                return Err(
                    "Disable is only safely supported for systemd scheduled tasks.".to_string(),
                );
            }
            validate_systemd_unit(&id)?;
            Ok(ScheduledTaskActionCommand {
                command: format!("systemctl disable {}", shell_quote(&id)),
                capability: ScheduledTaskCapability::Full,
            })
        }
        (ScheduledTaskOs::Windows, ScheduledTaskActionKind::RunNow { id, .. }) => {
            let task_args = windows_scheduled_task_args(&id)?;
            Ok(ScheduledTaskActionCommand {
                command: format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"Start-ScheduledTask {task_args}\""
                ),
                capability: ScheduledTaskCapability::Partial,
            })
        }
        (ScheduledTaskOs::Windows, ScheduledTaskActionKind::Enable { id, .. }) => {
            let task_args = windows_scheduled_task_args(&id)?;
            Ok(ScheduledTaskActionCommand {
                command: format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"Enable-ScheduledTask {task_args}\""
                ),
                capability: ScheduledTaskCapability::Partial,
            })
        }
        (ScheduledTaskOs::Windows, ScheduledTaskActionKind::Disable { id, .. }) => {
            let task_args = windows_scheduled_task_args(&id)?;
            Ok(ScheduledTaskActionCommand {
                command: format!(
                    "powershell -NoProfile -ExecutionPolicy Bypass -Command \"Disable-ScheduledTask {task_args}\""
                ),
                capability: ScheduledTaskCapability::Partial,
            })
        }
        (_, ScheduledTaskActionKind::RunNow { .. }) => {
            Err("Run now is not safely supported for this scheduled task source yet.".to_string())
        }
        (_, ScheduledTaskActionKind::Enable { .. }) => {
            Err("Enable is not safely supported for this scheduled task source yet.".to_string())
        }
        (_, ScheduledTaskActionKind::Disable { .. }) => {
            Err("Disable is not safely supported for this scheduled task source yet.".to_string())
        }
    }
}

pub fn build_scheduled_task_diagnostic_command(os_type: &str) -> String {
    match scheduled_task_os(os_type) {
        ScheduledTaskOs::Windows => {
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"Get-ScheduledTask | Format-Table -AutoSize; Get-ScheduledTaskInfo -TaskName * | Format-Table -AutoSize\"".to_string()
        }
        ScheduledTaskOs::MacOs => "launchctl list; find ~/Library/LaunchAgents /Library/LaunchAgents /Library/LaunchDaemons -maxdepth 1 -name '*.plist' -print 2>/dev/null".to_string(),
        ScheduledTaskOs::Bsd => "crontab -l 2>/dev/null; cat /etc/crontab 2>/dev/null; find /etc/cron.d /etc/periodic -maxdepth 2 -type f -print 2>/dev/null".to_string(),
        ScheduledTaskOs::Linux | ScheduledTaskOs::Unknown => concat!(
            "if command -v systemctl >/dev/null 2>&1; then systemctl list-timers --all --no-pager --plain; fi; ",
            "crontab -l 2>/dev/null; cat /etc/crontab 2>/dev/null; ",
            "find /etc/cron.d /etc/cron.hourly /etc/cron.daily /etc/cron.weekly /etc/cron.monthly -maxdepth 1 -type f -print 2>/dev/null"
        )
        .to_string(),
    }
}

pub fn parse_scheduled_task_snapshot(output: &str) -> ResourceScheduledTaskSnapshot {
    let Some(section) = extract_section(output, "SCHEDULED_TASKS") else {
        return ResourceScheduledTaskSnapshot::default();
    };

    let mut entries = Vec::new();
    let mut capability = ScheduledTaskCapability::Unknown;
    let mut platform = "unknown".to_string();
    let mut pending_timer_details = HashMap::new();

    for line in section
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .filter(|line| !line.trim().is_empty())
    {
        if line == SCHEDULED_TASK_UNAVAILABLE_MARKER {
            return ResourceScheduledTaskSnapshot {
                status: ResourceScheduledTaskStatus::Unavailable,
                entries: Vec::new(),
            };
        }
        if let Some(message) = line.strip_prefix(SCHEDULED_TASK_ERROR_MARKER) {
            return ResourceScheduledTaskSnapshot {
                status: ResourceScheduledTaskStatus::Error {
                    message: clean_marker_message(message, "Scheduled task command failed."),
                },
                entries: Vec::new(),
            };
        }
        if let Some((next_capability, next_platform)) = parse_capability_line(line) {
            capability = next_capability;
            platform = next_platform;
            continue;
        }
        if let Some((timer, details)) = parse_systemd_timer_show_line(line) {
            pending_timer_details.insert(timer, details);
            continue;
        }
        if let Some(entry) = parse_task_row_line(line)
            .or_else(|| parse_systemd_timer_line(line))
            .or_else(|| parse_cron_line(line))
            .or_else(|| parse_launchd_line(line))
            .or_else(|| parse_windows_line(line))
            .or_else(|| parse_at_line(line))
        {
            entries.push(entry);
        }
    }

    for entry in &mut entries {
        if entry.source == "systemd"
            && let Some(details) = pending_timer_details.get(&entry.id)
        {
            if entry.unit.trim().is_empty() && !details.unit.trim().is_empty() {
                entry.unit = details.unit.clone();
            }
            if (entry.enabled.trim().is_empty() || entry.enabled == "unknown")
                && !details.enabled.trim().is_empty()
            {
                entry.enabled = details.enabled.clone();
            }
            if (entry.active.trim().is_empty() || entry.active == "unknown")
                && !details.active.trim().is_empty()
            {
                entry.active = details.active.clone();
            }
            if entry.last_result.trim().is_empty() && !details.result.trim().is_empty() {
                entry.last_result = details.result.clone();
            }
            if (entry.description.trim().is_empty() || entry.description == entry.id)
                && !details.description.trim().is_empty()
            {
                entry.description = details.description.clone();
            }
            if entry.command.trim().is_empty() && !entry.unit.trim().is_empty() {
                entry.command = format!("systemctl start {}", entry.unit);
            }
        }
    }

    dedupe_and_sort_tasks(&mut entries);
    ResourceScheduledTaskSnapshot {
        status: ResourceScheduledTaskStatus::Available {
            capability,
            platform,
        },
        entries,
    }
}

pub fn visible_scheduled_task_rows(
    entries: &[ResourceScheduledTask],
    query: &str,
    filter: ScheduledTaskFilter,
) -> Vec<ResourceScheduledTask> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| scheduled_task_matches_filter(entry, filter))
        .filter(|entry| query.is_empty() || scheduled_task_matches_query(entry, &query))
        .cloned()
        .collect()
}

pub fn scheduled_task_row_signature(entry: &ResourceScheduledTask) -> u64 {
    let mut hasher = DefaultHasher::new();
    entry.id.hash(&mut hasher);
    entry.name.hash(&mut hasher);
    entry.source.hash(&mut hasher);
    entry.schedule.hash(&mut hasher);
    entry.command.hash(&mut hasher);
    entry.user.hash(&mut hasher);
    entry.enabled.hash(&mut hasher);
    entry.active.hash(&mut hasher);
    entry.last_run.hash(&mut hasher);
    entry.next_run.hash(&mut hasher);
    entry.last_result.hash(&mut hasher);
    entry.description.hash(&mut hasher);
    entry.unit.hash(&mut hasher);
    hasher.finish()
}

pub fn scheduled_task_filter_label_key(filter: ScheduledTaskFilter) -> &'static str {
    match filter {
        ScheduledTaskFilter::All => "sidebar.host_schedules.filters.all",
        ScheduledTaskFilter::Enabled => "sidebar.host_schedules.filters.enabled",
        ScheduledTaskFilter::Disabled => "sidebar.host_schedules.filters.disabled",
        ScheduledTaskFilter::Systemd => "sidebar.host_schedules.filters.systemd",
        ScheduledTaskFilter::Cron => "sidebar.host_schedules.filters.cron",
        ScheduledTaskFilter::Launchd => "sidebar.host_schedules.filters.launchd",
        ScheduledTaskFilter::Windows => "sidebar.host_schedules.filters.windows",
        ScheduledTaskFilter::Failed => "sidebar.host_schedules.filters.failed",
    }
}

pub fn scheduled_task_source_label_key(source: &str) -> &'static str {
    match source.trim().to_lowercase().as_str() {
        "systemd" => "sidebar.host_schedules.sources.systemd",
        "cron" => "sidebar.host_schedules.sources.cron",
        "anacron" => "sidebar.host_schedules.sources.anacron",
        "at" => "sidebar.host_schedules.sources.at",
        "launchd" => "sidebar.host_schedules.sources.launchd",
        "windows" | "task_scheduler" => "sidebar.host_schedules.sources.windows",
        _ => "sidebar.host_schedules.sources.unknown",
    }
}

pub fn scheduled_task_enabled_label_key(enabled: &str) -> &'static str {
    match enabled.trim().to_lowercase().as_str() {
        "enabled" | "true" | "yes" => "sidebar.host_schedules.enabled.enabled",
        "disabled" | "false" | "no" => "sidebar.host_schedules.enabled.disabled",
        "static" => "sidebar.host_schedules.enabled.static",
        "masked" => "sidebar.host_schedules.enabled.masked",
        _ => "sidebar.host_schedules.enabled.unknown",
    }
}

pub fn scheduled_task_active_label_key(active: &str) -> &'static str {
    match active.trim().to_lowercase().as_str() {
        "active" | "running" | "ready" => "sidebar.host_schedules.active.active",
        "inactive" | "stopped" => "sidebar.host_schedules.active.inactive",
        "failed" | "error" => "sidebar.host_schedules.active.failed",
        _ => "sidebar.host_schedules.active.unknown",
    }
}

fn build_linux_scheduled_task_snapshot_command() -> String {
    concat!(
        "echo '===SCHEDULED_TASKS==='; ",
        "if command -v systemctl >/dev/null 2>&1; then ",
        "echo '__OXIDE_SCHEDULE_CAPABILITY__\tfull\tlinux_systemd'; ",
        "oxide_timers=$(systemctl list-timers --all --no-legend --no-pager --plain 2>&1); oxide_timer_status=$?; ",
        "if [ \"$oxide_timer_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_timers\" | awk 'NF >= 5 { unit=$(NF-1); activates=$NF; last=$3\" \"$4; next=$1\" \"$2; left=\"\"; passed=\"\"; if (NF >= 7) { left=$3; passed=$(NF-3) }; printf \"SYSTEMD\\t%s\\t%s\\t%s\\t%s\\t%s\\t%s\\n\", unit, activates, next, left, last, passed }'; ",
        "printf '%s\\n' \"$oxide_timers\" | awk 'NF >= 5 { printf \"%s\\n\", $(NF-1) }' | while IFS= read -r oxide_timer; do ",
        "[ -n \"$oxide_timer\" ] || continue; ",
        "systemctl show \"$oxide_timer\" --no-pager --property=Id,Description,Unit,ActiveState,SubState,UnitFileState,NextElapseUSecRealtime,LastTriggerUSec,Result 2>/dev/null | awk 'BEGIN { printf \"SHOW\" } { gsub(/\\t/, \" \"); printf \"\\t%s\", $0 } END { printf \"\\n\" }'; ",
        "done; ",
        "else printf '__OXIDE_SCHEDULE_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_timers\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
        "fi; ",
        "if command -v crontab >/dev/null 2>&1; then crontab -l 2>/dev/null | awk 'NF && $1 !~ /^#/ { printf \"CRON\\tuser\\t%s\\n\", $0 }'; fi; ",
        "if [ -r /etc/crontab ]; then awk 'NF && $1 !~ /^#/ { printf \"CRON\\t/etc/crontab\\t%s\\n\", $0 }' /etc/crontab; fi; ",
        "if [ -d /etc/cron.d ]; then for f in /etc/cron.d/*; do [ -f \"$f\" ] || continue; awk -v src=\"$f\" 'NF && $1 !~ /^#/ { printf \"CRON\\t%s\\t%s\\n\", src, $0 }' \"$f\"; done; fi; ",
        "if [ -d /etc/cron.hourly ]; then find /etc/cron.hourly -maxdepth 1 -type f -printf 'DIRCRON\\thourly\\t%p\\n' 2>/dev/null; fi; ",
        "if [ -d /etc/cron.daily ]; then find /etc/cron.daily -maxdepth 1 -type f -printf 'DIRCRON\\tdaily\\t%p\\n' 2>/dev/null; fi; ",
        "if [ -d /etc/cron.weekly ]; then find /etc/cron.weekly -maxdepth 1 -type f -printf 'DIRCRON\\tweekly\\t%p\\n' 2>/dev/null; fi; ",
        "if [ -d /etc/cron.monthly ]; then find /etc/cron.monthly -maxdepth 1 -type f -printf 'DIRCRON\\tmonthly\\t%p\\n' 2>/dev/null; fi; ",
        "if command -v atq >/dev/null 2>&1; then atq 2>/dev/null | awk 'NF >= 5 { printf \"AT\\t%s\\t%s %s %s %s\\t%s\\n\", $1, $2, $3, $4, $5, $NF }'; fi; ",
        "echo '===SCHEDULED_TASKS_END==='"
    )
    .to_string()
}

fn build_macos_scheduled_task_snapshot_command() -> String {
    concat!(
        "echo '===SCHEDULED_TASKS==='; ",
        "if command -v launchctl >/dev/null 2>&1; then ",
        "echo '__OXIDE_SCHEDULE_CAPABILITY__\tpartial\tmacos_launchd'; ",
        "launchctl list 2>/dev/null | awk 'NR > 1 && NF >= 3 { pid=$1; status=$2; label=$3; active=(pid ~ /^[0-9]+$/ ? \"active\" : \"inactive\"); printf \"LAUNCHD\\t%s\\t%s\\t%s\\t\\t\\t\\t%s\\n\", label, active, status, label }'; ",
        "find ~/Library/LaunchAgents /Library/LaunchAgents /Library/LaunchDaemons -maxdepth 1 -name '*.plist' -print 2>/dev/null | while IFS= read -r plist; do label=$(basename \"$plist\" .plist); printf 'LAUNCHD\\t%s\\tinactive\\tunknown\\t\\t\\t%s\\t%s\\n' \"$label\" \"$plist\" \"$label\"; done; ",
        "else echo '__OXIDE_SCHEDULE_UNAVAILABLE__'; fi; ",
        "echo '===SCHEDULED_TASKS_END==='"
    )
    .to_string()
}

fn build_bsd_scheduled_task_snapshot_command() -> String {
    concat!(
        "echo '===SCHEDULED_TASKS==='; ",
        "echo '__OXIDE_SCHEDULE_CAPABILITY__\tpartial\tbsd_cron'; ",
        "if command -v crontab >/dev/null 2>&1; then crontab -l 2>/dev/null | awk 'NF && $1 !~ /^#/ { printf \"CRON\\tuser\\t%s\\n\", $0 }'; fi; ",
        "if [ -r /etc/crontab ]; then awk 'NF && $1 !~ /^#/ { printf \"CRON\\t/etc/crontab\\t%s\\n\", $0 }' /etc/crontab; fi; ",
        "if [ -d /etc/cron.d ]; then for f in /etc/cron.d/*; do [ -f \"$f\" ] || continue; awk -v src=\"$f\" 'NF && $1 !~ /^#/ { printf \"CRON\\t%s\\t%s\\n\", src, $0 }' \"$f\"; done; fi; ",
        "if [ -d /etc/periodic ]; then find /etc/periodic -type f -maxdepth 2 -print 2>/dev/null | awk '{ printf \"DIRCRON\\tperiodic\\t%s\\n\", $0 }'; fi; ",
        "echo '===SCHEDULED_TASKS_END==='"
    )
    .to_string()
}

fn build_windows_scheduled_task_snapshot_command() -> String {
    concat!(
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
        "Write-Output '===SCHEDULED_TASKS===';",
        "Write-Output ('__OXIDE_SCHEDULE_CAPABILITY__'+[char]9+'partial'+[char]9+'windows_task_scheduler');",
        "try{",
        "Get-ScheduledTask|ForEach-Object{",
        "$info=$null;try{$info=Get-ScheduledTaskInfo -TaskPath $_.TaskPath -TaskName $_.TaskName -ErrorAction SilentlyContinue}catch{};",
        "$next=if($info -and $info.NextRunTime){[string]$info.NextRunTime}else{''};",
        "$last=if($info -and $info.LastRunTime){[string]$info.LastRunTime}else{''};",
        "$res=if($info){[string]$info.LastTaskResult}else{''};",
        "$cmd='';try{$cmd=($_.Actions|ForEach-Object{($_.Execute+' '+$_.Arguments).Trim()}) -join ' ; '}catch{};",
        "$trig='';try{$trig=($_.Triggers|ForEach-Object{[string]$_}) -join ' ; '}catch{};",
        "$enabled=if($_.State -eq 'Disabled'){'disabled'}elseif($_.Settings -and ($_.Settings.Enabled -eq $false)){'disabled'}else{'enabled'};",
        "$id=($_.TaskPath+$_.TaskName);",
        "Write-Output ('WIN'+[char]9+$id+[char]9+$_.TaskName+[char]9+$_.State+[char]9+$enabled+[char]9+$trig+[char]9+$cmd+[char]9+''+[char]9+$last+[char]9+$next+[char]9+$res+[char]9+$_.Description)",
        "}",
        "}catch{Write-Output ('__OXIDE_SCHEDULE_ERROR__'+[char]9+$_.Exception.Message)};",
        "Write-Output '===SCHEDULED_TASKS_END==='",
        "\""
    )
    .to_string()
}

fn parse_capability_line(line: &str) -> Option<(ScheduledTaskCapability, String)> {
    let payload = line.strip_prefix(SCHEDULED_TASK_CAPABILITY_MARKER)?;
    let parts = payload
        .trim_start_matches('\t')
        .split('\t')
        .collect::<Vec<_>>();
    let capability = match parts.first().copied().unwrap_or("unknown") {
        "full" => ScheduledTaskCapability::Full,
        "partial" => ScheduledTaskCapability::Partial,
        _ => ScheduledTaskCapability::Unknown,
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

fn parse_task_row_line(line: &str) -> Option<ResourceScheduledTask> {
    let payload = line.strip_prefix("ROW\t")?;
    let parts = payload.splitn(13, '\t').collect::<Vec<_>>();
    if parts.len() != 13 {
        return None;
    }
    Some(ResourceScheduledTask {
        id: clean(parts[0]),
        name: clean(parts[1]),
        source: clean(parts[2]),
        schedule: clean(parts[3]),
        command: clean(parts[4]),
        user: clean(parts[5]),
        enabled: clean(parts[6]),
        active: clean(parts[7]),
        last_run: clean(parts[8]),
        next_run: clean(parts[9]),
        last_result: clean(parts[10]),
        description: clean(parts[11]),
        unit: clean(parts[12]),
    })
}

#[derive(Clone, Debug, Default)]
struct SystemdTimerShow {
    unit: String,
    description: String,
    enabled: String,
    active: String,
    result: String,
}

fn parse_systemd_timer_show_line(line: &str) -> Option<(String, SystemdTimerShow)> {
    let payload = line.strip_prefix("SHOW\t")?;
    let properties = parse_key_value_properties(payload);
    let id = properties.get("Id")?.to_string();
    let details = SystemdTimerShow {
        unit: properties.get("Unit").cloned().unwrap_or_default(),
        description: properties.get("Description").cloned().unwrap_or_default(),
        enabled: properties.get("UnitFileState").cloned().unwrap_or_default(),
        active: properties.get("ActiveState").cloned().unwrap_or_default(),
        result: properties.get("Result").cloned().unwrap_or_default(),
    };
    Some((id, details))
}

fn parse_systemd_timer_line(line: &str) -> Option<ResourceScheduledTask> {
    let payload = line.strip_prefix("SYSTEMD\t")?;
    let parts = payload.splitn(6, '\t').collect::<Vec<_>>();
    if parts.len() != 6 {
        return None;
    }
    let id = clean(parts[0]);
    let unit = clean(parts[1]);
    Some(ResourceScheduledTask {
        name: id.trim_end_matches(".timer").to_string(),
        id: id.clone(),
        source: "systemd".to_string(),
        schedule: clean(parts[3]),
        command: if unit.is_empty() {
            String::new()
        } else {
            format!("systemctl start {unit}")
        },
        user: "root".to_string(),
        enabled: "unknown".to_string(),
        active: "active".to_string(),
        last_run: clean(parts[4]),
        next_run: clean(parts[2]),
        last_result: clean(parts[5]),
        description: id,
        unit,
    })
}

fn parse_cron_line(line: &str) -> Option<ResourceScheduledTask> {
    let payload = line.strip_prefix("CRON\t")?;
    let mut parts = payload.splitn(2, '\t');
    let source_path = clean(parts.next()?);
    let raw = parts.next()?.trim();
    let fields = raw.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 6 {
        return None;
    }
    let system_cron = source_path != "user" && source_path.contains("crontab");
    let schedule_end = 5;
    let user_index = system_cron.then_some(5);
    let command_start = if system_cron { 6 } else { 5 };
    if fields.len() <= command_start {
        return None;
    }
    let command = fields[command_start..].join(" ");
    let name = cron_command_name(&command);
    Some(ResourceScheduledTask {
        id: format!("cron:{source_path}:{}", raw),
        name,
        source: "cron".to_string(),
        schedule: fields[..schedule_end].join(" "),
        command,
        user: user_index
            .and_then(|index| fields.get(index).copied())
            .unwrap_or("")
            .to_string(),
        enabled: "enabled".to_string(),
        active: "unknown".to_string(),
        last_run: String::new(),
        next_run: String::new(),
        last_result: String::new(),
        description: source_path,
        unit: String::new(),
    })
}

fn parse_launchd_line(line: &str) -> Option<ResourceScheduledTask> {
    let payload = line.strip_prefix("LAUNCHD\t")?;
    let parts = payload.splitn(7, '\t').collect::<Vec<_>>();
    if parts.len() != 7 {
        return None;
    }
    let id = clean(parts[0]);
    Some(ResourceScheduledTask {
        name: id.clone(),
        id,
        source: "launchd".to_string(),
        schedule: clean(parts[3]),
        command: clean(parts[5]),
        user: String::new(),
        enabled: "unknown".to_string(),
        active: clean(parts[1]),
        last_run: String::new(),
        next_run: String::new(),
        last_result: clean(parts[2]),
        description: clean(parts[6]),
        unit: clean(parts[4]),
    })
}

fn parse_windows_line(line: &str) -> Option<ResourceScheduledTask> {
    let payload = line.strip_prefix("WIN\t")?;
    let parts = payload.splitn(11, '\t').collect::<Vec<_>>();
    if parts.len() != 10 && parts.len() != 11 {
        return None;
    }
    let has_enabled_column = parts.len() == 11;
    let active = clean(parts[2]);
    let enabled = if has_enabled_column {
        clean(parts[3])
    } else if active.eq_ignore_ascii_case("disabled") {
        "disabled".to_string()
    } else {
        "enabled".to_string()
    };
    let offset = if has_enabled_column { 1 } else { 0 };
    Some(ResourceScheduledTask {
        id: clean(parts[0]),
        name: clean(parts[1]),
        source: "windows".to_string(),
        active,
        enabled,
        schedule: clean(parts[3 + offset]),
        command: clean(parts[4 + offset]),
        user: clean(parts[5 + offset]),
        last_run: clean(parts[6 + offset]),
        next_run: clean(parts[7 + offset]),
        last_result: clean(parts[8 + offset]),
        description: clean(parts[9 + offset]),
        unit: String::new(),
    })
}

fn parse_at_line(line: &str) -> Option<ResourceScheduledTask> {
    let payload = line.strip_prefix("AT\t")?;
    let parts = payload.splitn(3, '\t').collect::<Vec<_>>();
    if parts.len() != 3 {
        return None;
    }
    Some(ResourceScheduledTask {
        id: format!("at:{}", clean(parts[0])),
        name: format!("at {}", clean(parts[0])),
        source: "at".to_string(),
        schedule: clean(parts[1]),
        command: String::new(),
        user: clean(parts[2]),
        enabled: "enabled".to_string(),
        active: "unknown".to_string(),
        last_run: String::new(),
        next_run: clean(parts[1]),
        last_result: String::new(),
        description: "at job".to_string(),
        unit: String::new(),
    })
}

fn parse_key_value_properties(payload: &str) -> HashMap<String, String> {
    payload
        .split('\t')
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((key.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn dedupe_and_sort_tasks(entries: &mut Vec<ResourceScheduledTask>) {
    entries.sort_by(|left, right| {
        left.source
            .cmp(&right.source)
            .then(left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then(left.id.cmp(&right.id))
    });
    entries.dedup_by(|left, right| left.id == right.id && left.source == right.source);
}

fn scheduled_task_matches_filter(
    entry: &ResourceScheduledTask,
    filter: ScheduledTaskFilter,
) -> bool {
    match filter {
        ScheduledTaskFilter::All => true,
        ScheduledTaskFilter::Enabled => is_enabled_state(&entry.enabled),
        ScheduledTaskFilter::Disabled => is_disabled_state(&entry.enabled),
        ScheduledTaskFilter::Systemd => entry.source == "systemd",
        ScheduledTaskFilter::Cron => entry.source == "cron" || entry.source == "anacron",
        ScheduledTaskFilter::Launchd => entry.source == "launchd",
        ScheduledTaskFilter::Windows => entry.source == "windows",
        ScheduledTaskFilter::Failed => {
            entry.active.eq_ignore_ascii_case("failed")
                || entry.last_result.to_lowercase().contains("fail")
                || entry
                    .last_result
                    .trim()
                    .parse::<i32>()
                    .is_ok_and(|code| code != 0)
        }
    }
}

fn scheduled_task_matches_query(entry: &ResourceScheduledTask, query: &str) -> bool {
    [
        entry.id.as_str(),
        entry.name.as_str(),
        entry.source.as_str(),
        entry.schedule.as_str(),
        entry.command.as_str(),
        entry.user.as_str(),
        entry.enabled.as_str(),
        entry.active.as_str(),
        entry.last_run.as_str(),
        entry.next_run.as_str(),
        entry.last_result.as_str(),
        entry.description.as_str(),
        entry.unit.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(query))
}

fn is_enabled_state(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "enabled" | "true" | "yes" | "static"
    )
}

fn is_disabled_state(value: &str) -> bool {
    matches!(
        value.trim().to_lowercase().as_str(),
        "disabled" | "false" | "no" | "masked"
    )
}

fn cron_command_name(command: &str) -> String {
    command
        .split_whitespace()
        .next()
        .map(|value| {
            value
                .rsplit('/')
                .next()
                .unwrap_or(value)
                .trim_matches('"')
                .to_string()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "cron".to_string())
}

fn validate_systemd_unit(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("Scheduled task unit is empty.".to_string());
    }
    if value
        .chars()
        .any(|ch| !(ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '@' | ':')))
    {
        return Err("Scheduled task unit contains unsupported characters.".to_string());
    }
    Ok(())
}

fn sanitize_log_limit(limit: usize) -> usize {
    limit.clamp(50, 1000)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn windows_scheduled_task_args(id: &str) -> Result<String, String> {
    let normalized = id.trim().replace('/', "\\");
    if normalized.is_empty() {
        return Err("Scheduled task id is empty.".to_string());
    }
    if let Some(index) = normalized.rfind('\\') {
        let name = normalized[index + 1..].trim();
        if name.is_empty() {
            return Err("Scheduled task name is empty.".to_string());
        }
        let path = &normalized[..=index];
        if path.is_empty() || path == "\\" {
            Ok(format!("-TaskName {}", powershell_quote(name)))
        } else {
            Ok(format!(
                "-TaskPath {} -TaskName {}",
                powershell_quote(path),
                powershell_quote(name)
            ))
        }
    } else {
        Ok(format!("-TaskName {}", powershell_quote(&normalized)))
    }
}

fn clean(value: &str) -> String {
    value.trim().trim_matches('"').to_string()
}

fn clean_marker_message(message: &str, fallback: &str) -> String {
    let cleaned = message.trim_start_matches('\t').trim();
    if cleaned.is_empty() {
        fallback.to_string()
    } else {
        cleaned.to_string()
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
enum ScheduledTaskOs {
    Linux,
    MacOs,
    Bsd,
    Windows,
    Unknown,
}

fn scheduled_task_os(os_type: &str) -> ScheduledTaskOs {
    match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
            ScheduledTaskOs::Linux
        }
        "macOS" | "macos" | "Darwin" => ScheduledTaskOs::MacOs,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => ScheduledTaskOs::Bsd,
        "Windows" | "windows" => ScheduledTaskOs::Windows,
        _ => ScheduledTaskOs::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_systemd_timers_and_links_service_unit() {
        let output = concat!(
            "===SCHEDULED_TASKS===\n",
            "__OXIDE_SCHEDULE_CAPABILITY__\tfull\tlinux_systemd\n",
            "SYSTEMD\tapt-daily.timer\tapt-daily.service\tTue 2026-06-16 12:00:00 UTC\t1h left\tMon 2026-06-15 12:00:00 UTC\t1 day ago\n",
            "SHOW\tId=apt-daily.timer\tDescription=Daily apt download activities\tUnit=apt-daily.service\tActiveState=active\tUnitFileState=enabled\tResult=success\n",
            "===SCHEDULED_TASKS_END===\n",
        );

        let snapshot = parse_scheduled_task_snapshot(output);
        let rows =
            visible_scheduled_task_rows(&snapshot.entries, "apt", ScheduledTaskFilter::Systemd);

        assert_eq!(
            snapshot.status,
            ResourceScheduledTaskStatus::Available {
                capability: ScheduledTaskCapability::Full,
                platform: "linux_systemd".to_string(),
            }
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].unit, "apt-daily.service");
        assert_eq!(rows[0].enabled, "enabled");
        assert_eq!(rows[0].description, "Daily apt download activities");
        assert_eq!(rows[0].command, "systemctl start apt-daily.service");
    }

    #[test]
    fn parses_user_and_system_cron_without_losing_command() {
        let output = concat!(
            "===SCHEDULED_TASKS===\n",
            "__OXIDE_SCHEDULE_CAPABILITY__\tpartial\tlinux_cron\n",
            "CRON\tuser\t*/5 * * * * /usr/local/bin/backup job --flag value\n",
            "CRON\t/etc/crontab\t0 2 * * * root /usr/sbin/logrotate /etc/logrotate.conf\n",
            "===SCHEDULED_TASKS_END===\n",
        );

        let snapshot = parse_scheduled_task_snapshot(output);
        let rows =
            visible_scheduled_task_rows(&snapshot.entries, "logrotate", ScheduledTaskFilter::Cron);

        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(
            snapshot.entries[0].command,
            "/usr/local/bin/backup job --flag value"
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].user, "root");
        assert_eq!(rows[0].schedule, "0 2 * * *");
    }

    #[test]
    fn parses_launchd_and_windows_rows() {
        let output = concat!(
            "===SCHEDULED_TASKS===\n",
            "__OXIDE_SCHEDULE_CAPABILITY__\tpartial\tmixed\n",
            "LAUNCHD\tcom.example.sync\tactive\t0\tStartInterval=300\t/Library/LaunchDaemons/com.example.sync.plist\t/usr/local/bin/sync\tExample sync\n",
            "WIN\t\\Microsoft\\Windows\\Defrag\\ScheduledDefrag\tScheduledDefrag\tReady\tWeekly\tdefrag.exe C:\tSYSTEM\t2026-06-15 10:00:00\t2026-06-22 10:00:00\t0\tDisk defrag\n",
            "===SCHEDULED_TASKS_END===\n",
        );

        let snapshot = parse_scheduled_task_snapshot(output);
        let launchd =
            visible_scheduled_task_rows(&snapshot.entries, "sync", ScheduledTaskFilter::Launchd);
        let windows =
            visible_scheduled_task_rows(&snapshot.entries, "defrag", ScheduledTaskFilter::Windows);

        assert_eq!(launchd.len(), 1);
        assert_eq!(launchd[0].command, "/usr/local/bin/sync");
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].enabled, "enabled");
        assert_eq!(windows[0].last_result, "0");
    }

    #[test]
    fn filters_failed_tasks_and_preserves_unicode_names() {
        let output = concat!(
            "===SCHEDULED_TASKS===\n",
            "__OXIDE_SCHEDULE_CAPABILITY__\tpartial\tfixture\n",
            "ROW\tid-1\t备份 任务\tcron\t0 1 * * *\t/opt/bin/backup --中文\talice\tenabled\tfailed\tyesterday\ttomorrow\t1\tNightly backup\t\n",
            "===SCHEDULED_TASKS_END===\n",
        );

        let snapshot = parse_scheduled_task_snapshot(output);
        let failed =
            visible_scheduled_task_rows(&snapshot.entries, "中文", ScheduledTaskFilter::Failed);

        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].name, "备份 任务");
        assert_eq!(failed[0].command, "/opt/bin/backup --中文");
    }

    #[test]
    fn commands_keep_platforms_and_validation_separate() {
        let linux = build_scheduled_task_snapshot_command("Linux");
        let mac = build_scheduled_task_snapshot_command("macOS");
        let bsd = build_scheduled_task_snapshot_command("FreeBSD");
        let windows = build_scheduled_task_snapshot_command("Windows");

        assert!(linux.command.contains("systemctl list-timers"));
        assert_eq!(linux.capability, ScheduledTaskCapability::Full);
        assert!(mac.command.contains("launchctl list"));
        assert_eq!(mac.capability, ScheduledTaskCapability::Partial);
        assert!(bsd.command.contains("crontab -l"));
        assert!(windows.command.contains("Get-ScheduledTask"));

        let action = build_scheduled_task_action_command(
            "Linux",
            ScheduledTaskActionKind::RunNow {
                id: "safe.timer".to_string(),
                unit: "safe.service".to_string(),
            },
        )
        .expect("safe systemd unit");
        assert_eq!(action.command, "systemctl start 'safe.service'");
        let enable = build_scheduled_task_action_command(
            "Linux",
            ScheduledTaskActionKind::Enable {
                id: "safe.timer".to_string(),
                source: "systemd".to_string(),
            },
        )
        .expect("safe systemd enable");
        assert_eq!(enable.command, "systemctl enable 'safe.timer'");
        let disable = build_scheduled_task_action_command(
            "Linux",
            ScheduledTaskActionKind::Disable {
                id: "safe.timer".to_string(),
                source: "systemd".to_string(),
            },
        )
        .expect("safe systemd disable");
        assert_eq!(disable.command, "systemctl disable 'safe.timer'");
        let windows_enable = build_scheduled_task_action_command(
            "Windows",
            ScheduledTaskActionKind::Enable {
                id: "\\Microsoft\\Windows\\Defrag\\ScheduledDefrag".to_string(),
                source: "windows".to_string(),
            },
        )
        .expect("windows scheduled task enable");
        assert!(windows_enable.command.contains("Enable-ScheduledTask"));
        assert!(
            windows_enable
                .command
                .contains("-TaskPath '\\Microsoft\\Windows\\Defrag\\'")
        );
        assert!(
            windows_enable
                .command
                .contains("-TaskName 'ScheduledDefrag'")
        );
        assert!(
            build_scheduled_task_action_command(
                "Linux",
                ScheduledTaskActionKind::RunNow {
                    id: "bad;rm.timer".to_string(),
                    unit: "bad;rm.service".to_string(),
                },
            )
            .is_err()
        );
        assert!(
            build_scheduled_task_action_command(
                "Linux",
                ScheduledTaskActionKind::Enable {
                    id: "cron-job".to_string(),
                    source: "cron".to_string(),
                },
            )
            .is_err()
        );
    }
}
