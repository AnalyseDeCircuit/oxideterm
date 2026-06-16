use std::collections::{HashMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceService {
    pub id: String,
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub enabled_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_pid: Option<String>,
    pub description: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceCommandCapability {
    #[default]
    Unknown,
    Full,
    Partial,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceServiceStatus {
    #[default]
    Unknown,
    Available {
        capability: ServiceCommandCapability,
        platform: String,
    },
    Unavailable,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceServiceSnapshot {
    pub status: ResourceServiceStatus,
    pub services: Vec<ResourceService>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ServiceActionKind {
    Start,
    Stop,
    Restart,
    Reload,
    Enable,
    Disable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceActionCommand {
    pub command: String,
    pub capability: ServiceCommandCapability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServiceCaptureCommand {
    pub command: String,
    pub capability: ServiceCommandCapability,
}

const SERVICE_UNAVAILABLE_MARKER: &str = "__OXIDE_SERVICE_UNAVAILABLE__";
const SERVICE_ERROR_MARKER: &str = "__OXIDE_SERVICE_ERROR__";
const SERVICE_CAPABILITY_MARKER: &str = "__OXIDE_SERVICE_CAPABILITY__";

// Service sampling emits a small tab-delimited protocol instead of parsing
// localized or width-dependent command tables in the GPUI crate.
const SERVICE_SAMPLE_COMMAND_LINUX_SYSTEMD: &str = concat!(
    "echo '===SERVICES==='; ",
    "if command -v systemctl >/dev/null 2>&1; then ",
    "echo '__OXIDE_SERVICE_CAPABILITY__\tfull\tlinux_systemd'; ",
    "systemctl list-unit-files --type=service --no-legend --no-pager 2>/dev/null | awk 'NF >= 2 { printf \"UNITFILE\\t%s\\t%s\\n\", $1, $2 }'; ",
    "oxide_service_units=$(systemctl list-units --type=service --all --no-legend --no-pager --plain 2>&1); ",
    "oxide_service_status=$?; ",
    "if [ \"$oxide_service_status\" -ne 0 ]; then ",
    "printf '__OXIDE_SERVICE_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_service_units\" | head -n 1 | tr '\\t' ' ')\"; ",
    "else ",
    "printf '%s\\n' \"$oxide_service_units\" | awk 'NF >= 1 { print $1 }' | while IFS= read -r oxide_unit; do ",
    "[ -n \"$oxide_unit\" ] || continue; ",
    "systemctl show \"$oxide_unit\" --no-pager --property=Id,LoadState,ActiveState,SubState,UnitFileState,MainPID,Description 2>/dev/null | awk 'BEGIN { printf \"SHOW\" } { gsub(/\\t/, \" \"); printf \"\\t%s\", $0 } END { printf \"\\n\" }'; ",
    "done; ",
    "fi; ",
    "else echo '__OXIDE_SERVICE_UNAVAILABLE__'; fi; ",
    "echo '===SERVICES_END==='"
);

const SERVICE_SAMPLE_COMMAND_MACOS_LAUNCHCTL: &str = concat!(
    "echo '===SERVICES==='; ",
    "if command -v launchctl >/dev/null 2>&1; then ",
    "echo '__OXIDE_SERVICE_CAPABILITY__\tpartial\tmacos_launchctl'; ",
    "oxide_launchctl_services=$(launchctl list 2>&1); ",
    "oxide_launchctl_status=$?; ",
    "if [ \"$oxide_launchctl_status\" -ne 0 ]; then ",
    "printf '__OXIDE_SERVICE_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_launchctl_services\" | head -n 1 | tr '\\t' ' ')\"; ",
    "else ",
    "printf '%s\\n' \"$oxide_launchctl_services\" | awk 'NR > 1 && NF >= 3 { pid=$1; status=$2; label=$3; active=(pid ~ /^[0-9]+$/ ? \"running\" : \"inactive\"); main=(pid ~ /^[0-9]+$/ ? pid : \"\"); printf \"ROW\\t%s\\tloaded\\t%s\\t%s\\tunknown\\t%s\\t%s\\n\", label, active, status, main, label }'; ",
    "fi; ",
    "else echo '__OXIDE_SERVICE_UNAVAILABLE__'; fi; ",
    "echo '===SERVICES_END==='"
);

const SERVICE_SAMPLE_COMMAND_BSD: &str = concat!(
    "echo '===SERVICES==='; ",
    "if command -v rcctl >/dev/null 2>&1; then ",
    "echo '__OXIDE_SERVICE_CAPABILITY__\tpartial\tbsd_rcctl'; ",
    "rcctl ls on 2>/dev/null | awk 'NF >= 1 { printf \"UNITFILE\\t%s\\tenabled\\n\", $1 }'; ",
    "oxide_rc_services=$(rcctl ls all 2>&1); ",
    "oxide_rc_status=$?; ",
    "if [ \"$oxide_rc_status\" -ne 0 ]; then ",
    "printf '__OXIDE_SERVICE_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_rc_services\" | head -n 1 | tr '\\t' ' ')\"; ",
    "else printf '%s\\n' \"$oxide_rc_services\" | awk 'NF >= 1 { printf \"ROW\\t%s\\tunknown\\tunknown\\tunknown\\tunknown\\t\\t%s\\n\", $1, $1 }'; fi; ",
    "elif command -v service >/dev/null 2>&1; then ",
    "echo '__OXIDE_SERVICE_CAPABILITY__\tpartial\tbsd_service'; ",
    "service -e 2>/dev/null | awk 'NF >= 1 { n=$1; sub(/^.*\\//, \"\", n); printf \"UNITFILE\\t%s\\tenabled\\n\", n }'; ",
    "oxide_service_list=$(service -l 2>&1); ",
    "oxide_service_status=$?; ",
    "if [ \"$oxide_service_status\" -ne 0 ]; then ",
    "printf '__OXIDE_SERVICE_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_service_list\" | head -n 1 | tr '\\t' ' ')\"; ",
    "else printf '%s\\n' \"$oxide_service_list\" | awk 'NF >= 1 { printf \"ROW\\t%s\\tunknown\\tunknown\\tunknown\\tunknown\\t\\t%s\\n\", $1, $1 }'; fi; ",
    "else echo '__OXIDE_SERVICE_UNAVAILABLE__'; fi; ",
    "echo '===SERVICES_END==='"
);

const SERVICE_SAMPLE_COMMAND_WINDOWS: &str = concat!(
    "Write-Output '===SERVICES===';",
    "Write-Output ('__OXIDE_SERVICE_CAPABILITY__'+[char]9+'partial'+[char]9+'windows_powershell');",
    "try{",
    "Get-CimInstance Win32_Service|ForEach-Object{",
    "$pid=if($_.ProcessId -and $_.ProcessId -ne 0){[string]$_.ProcessId}else{''};",
    "$enabled=if($_.StartMode){[string]$_.StartMode}else{'unknown'};",
    "$desc=if($_.DisplayName){[string]$_.DisplayName}else{[string]$_.Name};",
    "Write-Output ('ROW'+[char]9+$_.Name+[char]9+''+[char]9+$_.State+[char]9+$_.Status+[char]9+$enabled+[char]9+$pid+[char]9+$desc)",
    "}",
    "}catch{Write-Output ('__OXIDE_SERVICE_ERROR__'+[char]9+$_.Exception.Message)};",
    "Write-Output '===SERVICES_END===';"
);

pub fn service_sample_command(os_type: &str) -> &'static str {
    match os_type {
        // Windows_* values here come from POSIX-like SSH environments; native
        // Windows OpenSSH stays on the separate PowerShell command path.
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
            SERVICE_SAMPLE_COMMAND_LINUX_SYSTEMD
        }
        "macOS" | "macos" | "Darwin" => SERVICE_SAMPLE_COMMAND_MACOS_LAUNCHCTL,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => SERVICE_SAMPLE_COMMAND_BSD,
        "Windows" | "windows" => SERVICE_SAMPLE_COMMAND_WINDOWS,
        _ => SERVICE_SAMPLE_COMMAND_BSD,
    }
}

pub fn parse_service_snapshot(output: &str) -> ResourceServiceSnapshot {
    let Some(section) = extract_section(output, "SERVICES") else {
        return ResourceServiceSnapshot::default();
    };

    let mut services = Vec::new();
    let mut enabled_by_id = HashMap::new();
    let mut capability = ServiceCommandCapability::Unknown;
    let mut platform = "unknown".to_string();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line == SERVICE_UNAVAILABLE_MARKER {
            return ResourceServiceSnapshot {
                status: ResourceServiceStatus::Unavailable,
                services: Vec::new(),
            };
        }
        if let Some(message) = line.strip_prefix(SERVICE_ERROR_MARKER) {
            return ResourceServiceSnapshot {
                status: ResourceServiceStatus::Error {
                    message: clean_marker_message(message, "Service command failed."),
                },
                services: Vec::new(),
            };
        }
        if let Some((next_capability, next_platform)) = parse_service_capability_line(line) {
            capability = next_capability;
            platform = next_platform;
            continue;
        }
        if let Some((id, enabled)) = parse_service_unit_file_line(line) {
            enabled_by_id.insert(id, enabled);
            continue;
        }
        if let Some(service) =
            parse_service_show_line(line).or_else(|| parse_service_row_line(line))
        {
            services.push(service);
        }
    }

    merge_service_enabled_states(&mut services, &enabled_by_id);
    append_unit_file_only_services(&mut services, &enabled_by_id);
    services.sort_by(|left, right| left.id.to_lowercase().cmp(&right.id.to_lowercase()));
    services.dedup_by(|left, right| left.id == right.id);

    ResourceServiceSnapshot {
        status: ResourceServiceStatus::Available {
            capability,
            platform,
        },
        services,
    }
}

pub fn visible_service_rows(services: &[ResourceService], query: &str) -> Vec<ResourceService> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return services.to_vec();
    }
    services
        .iter()
        .filter(|service| service_matches_query(service, &query))
        .cloned()
        .collect()
}

pub fn service_row_signature(service: &ResourceService) -> u64 {
    let mut hasher = DefaultHasher::new();
    service.id.hash(&mut hasher);
    service.load_state.hash(&mut hasher);
    service.active_state.hash(&mut hasher);
    service.sub_state.hash(&mut hasher);
    service.enabled_state.hash(&mut hasher);
    service.main_pid.hash(&mut hasher);
    service.description.hash(&mut hasher);
    hasher.finish()
}

pub fn service_state_label_key(state: &str) -> &'static str {
    match state.trim().to_lowercase().as_str() {
        "active" | "running" => "sidebar.host_services.states.running",
        "inactive" | "stopped" => "sidebar.host_services.states.stopped",
        "failed" => "sidebar.host_services.states.failed",
        "activating" | "start pending" | "start_pending" => {
            "sidebar.host_services.states.activating"
        }
        "deactivating" | "stop pending" | "stop_pending" => {
            "sidebar.host_services.states.deactivating"
        }
        "reloading" | "continue pending" | "continue_pending" => {
            "sidebar.host_services.states.reloading"
        }
        _ => "sidebar.host_services.states.unknown",
    }
}

pub fn service_enabled_label_key(state: &str) -> &'static str {
    match state.trim().to_lowercase().as_str() {
        "enabled" | "auto" | "automatic" | "boot" | "on" => "sidebar.host_services.enabled.enabled",
        "disabled" | "manual" | "demand" | "off" => "sidebar.host_services.enabled.disabled",
        "masked" => "sidebar.host_services.enabled.masked",
        "static" => "sidebar.host_services.enabled.static",
        "generated" | "transient" | "indirect" | "alias" => "sidebar.host_services.enabled.static",
        _ => "sidebar.host_services.enabled.unknown",
    }
}

pub fn build_service_action_command(
    os_type: &str,
    service_id: &str,
    action: ServiceActionKind,
) -> Result<ServiceActionCommand, String> {
    // The service id crosses into a remote shell command, so validate before
    // selecting the platform-specific action template.
    let service_id = validated_service_id(service_id)?;
    match normalized_service_os(os_type) {
        ServiceOs::LinuxSystemd => Ok(ServiceActionCommand {
            command: build_systemd_service_action_command(service_id, &action),
            capability: ServiceCommandCapability::Full,
        }),
        ServiceOs::MacLaunchctl => Ok(ServiceActionCommand {
            command: build_launchctl_service_action_command(service_id, &action),
            capability: ServiceCommandCapability::Partial,
        }),
        ServiceOs::Bsd => Ok(ServiceActionCommand {
            command: build_bsd_service_action_command(service_id, &action),
            capability: ServiceCommandCapability::Partial,
        }),
        ServiceOs::Windows => Ok(ServiceActionCommand {
            command: build_windows_service_action_command(service_id, &action),
            capability: ServiceCommandCapability::Partial,
        }),
        ServiceOs::Unsupported => Err(format!(
            "Service management is not supported for remote OS {os_type}."
        )),
    }
}

pub fn build_service_logs_command(
    os_type: &str,
    service_id: &str,
) -> Result<ServiceCaptureCommand, String> {
    let service_id = validated_service_id(service_id)?;
    let (command, capability) = match normalized_service_os(os_type) {
        ServiceOs::LinuxSystemd => (
            format!(
                "journalctl -u {} -n 200 --no-pager 2>&1 || sudo -n journalctl -u {} -n 200 --no-pager",
                shell_quote(service_id),
                shell_quote(service_id)
            ),
            ServiceCommandCapability::Full,
        ),
        ServiceOs::MacLaunchctl => (
            format!(
                "log show --last 1h --style compact --predicate {} 2>&1",
                shell_quote(&format!(
                    "process == \"{service_id}\" OR sender == \"{service_id}\" OR eventMessage CONTAINS \"{service_id}\""
                ))
            ),
            ServiceCommandCapability::Partial,
        ),
        ServiceOs::Bsd => (
            format!(
                "tail -n 200 /var/log/messages 2>/dev/null | grep -F {} || true",
                shell_quote(service_id)
            ),
            ServiceCommandCapability::Partial,
        ),
        ServiceOs::Windows => (
            build_windows_service_logs_command(service_id, false),
            ServiceCommandCapability::Partial,
        ),
        ServiceOs::Unsupported => {
            return Err(format!(
                "Service logs are not supported for remote OS {os_type}."
            ));
        }
    };
    Ok(ServiceCaptureCommand {
        command,
        capability,
    })
}

pub fn build_service_follow_logs_command(
    os_type: &str,
    service_id: &str,
) -> Result<ServiceCaptureCommand, String> {
    let service_id = validated_service_id(service_id)?;
    let (command, capability) = match normalized_service_os(os_type) {
        ServiceOs::LinuxSystemd => (
            format!("journalctl -fu {} --no-pager", shell_quote(service_id)),
            ServiceCommandCapability::Full,
        ),
        ServiceOs::MacLaunchctl => (
            format!(
                "log stream --style compact --predicate {}",
                shell_quote(&format!(
                    "process == \"{service_id}\" OR sender == \"{service_id}\" OR eventMessage CONTAINS \"{service_id}\""
                ))
            ),
            ServiceCommandCapability::Partial,
        ),
        ServiceOs::Bsd => (
            format!(
                "tail -f /var/log/messages 2>/dev/null | grep --line-buffered -F {}",
                shell_quote(service_id)
            ),
            ServiceCommandCapability::Partial,
        ),
        ServiceOs::Windows => (
            build_windows_service_logs_command(service_id, true),
            ServiceCommandCapability::Partial,
        ),
        ServiceOs::Unsupported => {
            return Err(format!(
                "Service log following is not supported for remote OS {os_type}."
            ));
        }
    };
    Ok(ServiceCaptureCommand {
        command,
        capability,
    })
}

pub fn service_action_succeeded(exit_code: Option<i32>) -> bool {
    exit_code.unwrap_or(0) == 0
}

pub fn service_action_success_message(stdout: &str, stderr: &str) -> String {
    compact_service_command_message(stdout)
        .or_else(|| compact_service_command_message(stderr))
        .unwrap_or_else(|| "Service action completed.".to_string())
}

pub fn service_action_failure_message(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
) -> String {
    compact_service_command_message(stderr)
        .or_else(|| compact_service_command_message(stdout))
        .unwrap_or_else(|| {
            exit_code
                .map(|code| format!("Service action failed with exit code {code}."))
                .unwrap_or_else(|| "Service action failed.".to_string())
        })
}

fn parse_service_capability_line(line: &str) -> Option<(ServiceCommandCapability, String)> {
    let payload = line.strip_prefix(SERVICE_CAPABILITY_MARKER)?;
    let parts = payload
        .trim_start_matches('\t')
        .split('\t')
        .collect::<Vec<_>>();
    let capability = match parts.first().copied().unwrap_or("unknown") {
        "full" => ServiceCommandCapability::Full,
        "partial" => ServiceCommandCapability::Partial,
        _ => ServiceCommandCapability::Unknown,
    };
    let platform = clean_service_field(parts.get(1).copied().unwrap_or("unknown"))
        .unwrap_or_else(|| "unknown".to_string());
    Some((capability, platform))
}

fn parse_service_unit_file_line(line: &str) -> Option<(String, String)> {
    let payload = line.strip_prefix("UNITFILE\t")?;
    let parts = payload.splitn(2, '\t').collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    let id = clean_service_id(parts[0])?;
    let enabled = clean_service_field(parts[1]).unwrap_or_else(|| "unknown".to_string());
    Some((id, enabled))
}

fn parse_service_show_line(line: &str) -> Option<ResourceService> {
    let payload = line.strip_prefix("SHOW\t")?;
    let mut values = HashMap::new();
    for part in payload.split('\t') {
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        values.insert(key.trim(), value.trim());
    }
    let id = values.get("Id").and_then(|value| clean_service_id(value))?;
    let main_pid = values
        .get("MainPID")
        .and_then(|value| clean_service_pid(value));
    Some(ResourceService {
        id: id.clone(),
        load_state: value_or_unknown(values.get("LoadState").copied()),
        active_state: value_or_unknown(values.get("ActiveState").copied()),
        sub_state: value_or_unknown(values.get("SubState").copied()),
        enabled_state: value_or_unknown(values.get("UnitFileState").copied()),
        main_pid,
        description: clean_service_field(values.get("Description").copied().unwrap_or(""))
            .unwrap_or(id),
    })
}

fn parse_service_row_line(line: &str) -> Option<ResourceService> {
    let payload = line.strip_prefix("ROW\t")?;
    let parts = payload.splitn(7, '\t').collect::<Vec<_>>();
    if parts.len() < 7 {
        return None;
    }
    let id = clean_service_id(parts[0])?;
    Some(ResourceService {
        id: id.clone(),
        load_state: value_or_unknown(Some(parts[1])),
        active_state: value_or_unknown(Some(parts[2])),
        sub_state: value_or_unknown(Some(parts[3])),
        enabled_state: value_or_unknown(Some(parts[4])),
        main_pid: clean_service_pid(parts[5]),
        description: clean_service_field(parts[6]).unwrap_or(id),
    })
}

fn merge_service_enabled_states(
    services: &mut [ResourceService],
    enabled_by_id: &HashMap<String, String>,
) {
    for service in services {
        if service.enabled_state == "unknown"
            && let Some(enabled) = enabled_by_id.get(&service.id)
        {
            service.enabled_state = enabled.clone();
        }
    }
}

fn append_unit_file_only_services(
    services: &mut Vec<ResourceService>,
    enabled_by_id: &HashMap<String, String>,
) {
    for (id, enabled) in enabled_by_id {
        if services.iter().any(|service| service.id == *id) {
            continue;
        }
        services.push(ResourceService {
            id: id.clone(),
            load_state: "unknown".to_string(),
            active_state: "inactive".to_string(),
            sub_state: "unknown".to_string(),
            enabled_state: enabled.clone(),
            main_pid: None,
            description: id.clone(),
        });
    }
}

fn service_matches_query(service: &ResourceService, query: &str) -> bool {
    service.id.to_lowercase().contains(query)
        || service.description.to_lowercase().contains(query)
        || service.active_state.to_lowercase().contains(query)
        || service.sub_state.to_lowercase().contains(query)
        || service.enabled_state.to_lowercase().contains(query)
        || service
            .main_pid
            .as_deref()
            .is_some_and(|pid| pid.contains(query))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServiceOs {
    LinuxSystemd,
    MacLaunchctl,
    Bsd,
    Windows,
    Unsupported,
}

fn normalized_service_os(os_type: &str) -> ServiceOs {
    match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
            ServiceOs::LinuxSystemd
        }
        "macOS" | "macos" | "Darwin" => ServiceOs::MacLaunchctl,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => ServiceOs::Bsd,
        "Windows" | "windows" => ServiceOs::Windows,
        _ => ServiceOs::Unsupported,
    }
}

fn build_systemd_service_action_command(service_id: &str, action: &ServiceActionKind) -> String {
    let verb = match action {
        ServiceActionKind::Start => "start",
        ServiceActionKind::Stop => "stop",
        ServiceActionKind::Restart => "restart",
        ServiceActionKind::Reload => "reload",
        ServiceActionKind::Enable => "enable",
        ServiceActionKind::Disable => "disable",
    };
    let service = shell_quote(service_id);
    let success = service_action_success_text(service_id, action);
    format!(
        "if systemctl {verb} {service} 2>&1 || sudo -n systemctl {verb} {service} 2>&1; then echo {}; else status=$?; echo 'Service action failed' >&2; exit $status; fi",
        shell_quote(&success)
    )
}

fn build_launchctl_service_action_command(service_id: &str, action: &ServiceActionKind) -> String {
    let label = shell_quote(service_id);
    let target_system = shell_quote(&format!("system/{service_id}"));
    let target_user = format!("\"gui/$(id -u)/{}\"", shell_escape_double(service_id));
    let operation = match action {
        ServiceActionKind::Start | ServiceActionKind::Restart | ServiceActionKind::Reload => {
            format!(
                "launchctl kickstart -k {target_system} 2>&1 || launchctl kickstart -k {target_user} 2>&1"
            )
        }
        ServiceActionKind::Stop => {
            format!(
                "launchctl kill TERM {target_system} 2>&1 || launchctl kill TERM {target_user} 2>&1 || launchctl stop {label} 2>&1"
            )
        }
        ServiceActionKind::Enable => {
            format!("launchctl enable {target_system} 2>&1 || launchctl enable {target_user} 2>&1")
        }
        ServiceActionKind::Disable => {
            format!(
                "launchctl disable {target_system} 2>&1 || launchctl disable {target_user} 2>&1"
            )
        }
    };
    let success = service_action_success_text(service_id, action);
    format!(
        "if {operation}; then echo {}; else status=$?; echo 'Service action failed' >&2; exit $status; fi",
        shell_quote(&success)
    )
}

fn build_bsd_service_action_command(service_id: &str, action: &ServiceActionKind) -> String {
    let service = shell_quote(service_id);
    let operation = match action {
        ServiceActionKind::Start => format!(
            "if command -v rcctl >/dev/null 2>&1; then rcctl start {service}; else service {service} start; fi"
        ),
        ServiceActionKind::Stop => format!(
            "if command -v rcctl >/dev/null 2>&1; then rcctl stop {service}; else service {service} stop; fi"
        ),
        ServiceActionKind::Restart => format!(
            "if command -v rcctl >/dev/null 2>&1; then rcctl restart {service}; else service {service} restart; fi"
        ),
        ServiceActionKind::Reload => format!(
            "if command -v rcctl >/dev/null 2>&1; then rcctl reload {service}; else service {service} reload; fi"
        ),
        ServiceActionKind::Enable => format!(
            "if command -v rcctl >/dev/null 2>&1; then rcctl enable {service}; elif command -v sysrc >/dev/null 2>&1; then sysrc {}_enable=YES; else echo 'No BSD service enable command found' >&2; exit 2; fi",
            shell_escape_sysrc_name(service_id)
        ),
        ServiceActionKind::Disable => format!(
            "if command -v rcctl >/dev/null 2>&1; then rcctl disable {service}; elif command -v sysrc >/dev/null 2>&1; then sysrc {}_enable=NO; else echo 'No BSD service disable command found' >&2; exit 2; fi",
            shell_escape_sysrc_name(service_id)
        ),
    };
    let success = service_action_success_text(service_id, action);
    format!(
        "if {operation} 2>&1; then echo {}; else status=$?; echo 'Service action failed' >&2; exit $status; fi",
        shell_quote(&success)
    )
}

fn build_windows_service_action_command(service_id: &str, action: &ServiceActionKind) -> String {
    let name = powershell_quote(service_id);
    let operation = match action {
        ServiceActionKind::Start => format!("Start-Service -Name {name} -ErrorAction Stop"),
        ServiceActionKind::Stop => format!("Stop-Service -Name {name} -ErrorAction Stop"),
        ServiceActionKind::Restart | ServiceActionKind::Reload => {
            format!("Restart-Service -Name {name} -ErrorAction Stop")
        }
        ServiceActionKind::Enable => {
            format!("Set-Service -Name {name} -StartupType Automatic -ErrorAction Stop")
        }
        ServiceActionKind::Disable => {
            format!("Set-Service -Name {name} -StartupType Disabled -ErrorAction Stop")
        }
    };
    let success = powershell_quote(&service_action_success_text(service_id, action));
    let script = format!(
        "$ErrorActionPreference='Stop'; try {{ Get-Service -Name {name} -ErrorAction Stop | Out-Null; {operation}; Write-Output {success}; exit 0 }} catch {{ Write-Error $_.Exception.Message; exit 1 }}"
    );
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"{}\"",
        script.replace('"', "`\"")
    )
}

fn build_windows_service_logs_command(service_id: &str, follow: bool) -> String {
    let needle = powershell_quote(service_id);
    let body = "Get-WinEvent -LogName System -MaxEvents 200 -ErrorAction SilentlyContinue | Where-Object { $_.ProviderName -like ('*' + $name + '*') -or $_.Message -like ('*' + $name + '*') } | Select-Object -First 200 | ForEach-Object { $_.TimeCreated.ToString('s') + ' ' + $_.ProviderName + ' ' + $_.Message }";
    let script = if follow {
        format!("$name={needle}; while($true){{ {body}; Start-Sleep -Seconds 2 }}")
    } else {
        format!("$name={needle}; {body}")
    };
    format!(
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"{}\"",
        script.replace('"', "`\"")
    )
}

fn service_action_success_text(service_id: &str, action: &ServiceActionKind) -> String {
    let verb = match action {
        ServiceActionKind::Start => "Started",
        ServiceActionKind::Stop => "Stopped",
        ServiceActionKind::Restart => "Restarted",
        ServiceActionKind::Reload => "Reloaded",
        ServiceActionKind::Enable => "Enabled",
        ServiceActionKind::Disable => "Disabled",
    };
    format!("{verb} service {service_id}")
}

fn validated_service_id(service_id: &str) -> Result<&str, String> {
    let service_id = service_id.trim();
    if service_id.is_empty() {
        return Err("Service name cannot be empty.".to_string());
    }
    if service_id.starts_with('-')
        || service_id
            .chars()
            .any(|character| character.is_control() || matches!(character, '\n' | '\r' | '\0'))
    {
        return Err("Invalid service name.".to_string());
    }
    Ok(service_id)
}

fn clean_service_id(value: &str) -> Option<String> {
    let value = clean_service_field(value)?;
    validated_service_id(&value).ok()?;
    Some(value)
}

fn clean_service_field(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value.chars().all(|character| character == '.') {
        return None;
    }
    Some(value.to_string())
}

fn clean_service_pid(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() || value == "0" || value == "-" {
        return None;
    }
    value
        .chars()
        .all(|character| character.is_ascii_digit())
        .then(|| value.to_string())
}

fn value_or_unknown(value: Option<&str>) -> String {
    clean_service_field(value.unwrap_or("")).unwrap_or_else(|| "unknown".to_string())
}

fn clean_marker_message(value: &str, fallback: &str) -> String {
    let value = value.trim_start_matches('\t').trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.chars().take(180).collect()
    }
}

fn compact_service_command_message(value: &str) -> Option<String> {
    let summary = value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())?
        .chars()
        .take(180)
        .collect::<String>();
    Some(summary)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

fn shell_escape_double(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('$', "\\$")
        .replace('`', "\\`")
}

fn shell_escape_sysrc_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>()
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn extract_section<'a>(output: &'a str, name: &str) -> Option<&'a str> {
    let start = format!("==={name}===");
    let end = format!("==={name}_END===");
    let after_start = output.split_once(&start)?.1;
    Some(after_start.split_once(&end)?.0.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_systemd_services_with_enabled_state_and_main_pid() {
        let output = concat!(
            "===SERVICES===\n",
            "__OXIDE_SERVICE_CAPABILITY__\tfull\tlinux_systemd\n",
            "UNITFILE\tsshd.service\tenabled\n",
            "UNITFILE\torphan.service\tdisabled\n",
            "SHOW\tId=sshd.service\tLoadState=loaded\tActiveState=active\tSubState=running\tUnitFileState=enabled\tMainPID=42\tDescription=OpenSSH server daemon\n",
            "===SERVICES_END===\n",
        );

        let snapshot = parse_service_snapshot(output);

        assert_eq!(
            snapshot.status,
            ResourceServiceStatus::Available {
                capability: ServiceCommandCapability::Full,
                platform: "linux_systemd".to_string()
            }
        );
        assert_eq!(snapshot.services.len(), 2);
        let sshd = snapshot
            .services
            .iter()
            .find(|service| service.id == "sshd.service")
            .unwrap();
        assert_eq!(sshd.active_state, "active");
        assert_eq!(sshd.enabled_state, "enabled");
        assert_eq!(sshd.main_pid.as_deref(), Some("42"));
    }

    #[test]
    fn parses_partial_service_rows_from_non_systemd_platforms() {
        let output = concat!(
            "===SERVICES===\n",
            "__OXIDE_SERVICE_CAPABILITY__\tpartial\twindows_powershell\n",
            "ROW\tSpooler\t\trunning\tOK\tAutomatic\t123\tPrint Spooler\n",
            "ROW\tManualSvc\t\tstopped\tStopped\tManual\t\tManual Service\n",
            "===SERVICES_END===\n",
        );

        let snapshot = parse_service_snapshot(output);

        assert_eq!(snapshot.services.len(), 2);
        assert_eq!(snapshot.services[0].id, "ManualSvc");
        assert_eq!(snapshot.services[1].description, "Print Spooler");
        assert_eq!(snapshot.services[1].main_pid.as_deref(), Some("123"));
    }

    #[test]
    fn service_actions_use_platform_specific_commands() {
        let linux =
            build_service_action_command("Linux", "sshd.service", ServiceActionKind::Restart)
                .unwrap();
        assert_eq!(linux.capability, ServiceCommandCapability::Full);
        assert!(linux.command.contains("systemctl restart 'sshd.service'"));

        let mac =
            build_service_action_command("macOS", "com.example.agent", ServiceActionKind::Disable)
                .unwrap();
        assert_eq!(mac.capability, ServiceCommandCapability::Partial);
        assert!(mac.command.contains("launchctl disable"));

        let windows =
            build_service_action_command("Windows", "Spooler", ServiceActionKind::Enable).unwrap();
        assert_eq!(windows.capability, ServiceCommandCapability::Partial);
        assert!(windows.command.contains("Set-Service"));
        assert!(windows.command.contains("StartupType Automatic"));
    }

    #[test]
    fn service_logs_use_journalctl_for_linux_follow() {
        let command = build_service_follow_logs_command("Linux", "sshd.service").unwrap();

        assert_eq!(command.capability, ServiceCommandCapability::Full);
        assert!(command.command.contains("journalctl -fu 'sshd.service'"));
    }

    #[test]
    fn visible_service_rows_match_identity_state_and_pid() {
        let rows = vec![ResourceService {
            id: "sshd.service".to_string(),
            load_state: "loaded".to_string(),
            active_state: "active".to_string(),
            sub_state: "running".to_string(),
            enabled_state: "enabled".to_string(),
            main_pid: Some("42".to_string()),
            description: "OpenSSH server daemon".to_string(),
        }];

        assert_eq!(visible_service_rows(&rows, "openssh").len(), 1);
        assert_eq!(visible_service_rows(&rows, "42").len(), 1);
        assert_eq!(visible_service_rows(&rows, "postgres").len(), 0);
    }
}
