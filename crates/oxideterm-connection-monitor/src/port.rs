use std::collections::{HashSet, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePortEntry {
    pub protocol: String,
    pub local_address: String,
    pub local_port: String,
    pub remote_address: String,
    pub remote_port: String,
    pub state: String,
    pub pid: String,
    pub process_name: String,
    pub user: String,
    pub command: String,
    pub inode: String,
    pub source: String,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PortCommandCapability {
    #[default]
    Unknown,
    Full,
    Partial,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourcePortStatus {
    #[default]
    Unknown,
    Available {
        capability: PortCommandCapability,
        platform: String,
    },
    Unavailable,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePortSnapshot {
    pub status: ResourcePortStatus,
    pub entries: Vec<ResourcePortEntry>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PortFilter {
    #[default]
    All,
    Listening,
    Connected,
    Tcp,
    Udp,
    Risky,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortCaptureCommand {
    pub command: String,
    pub capability: PortCommandCapability,
}

const PORT_UNAVAILABLE_MARKER: &str = "__OXIDE_PORT_UNAVAILABLE__";
const PORT_ERROR_MARKER: &str = "__OXIDE_PORT_ERROR__";
const PORT_CAPABILITY_MARKER: &str = "__OXIDE_PORT_CAPABILITY__";

pub fn build_port_snapshot_command(os_type: &str) -> PortCaptureCommand {
    let (command, capability) = match port_os(os_type) {
        PortOs::Windows => (
            build_windows_port_snapshot_command(),
            PortCommandCapability::Partial,
        ),
        PortOs::MacOs => (
            build_macos_port_snapshot_command(),
            PortCommandCapability::Partial,
        ),
        PortOs::Bsd => (
            build_bsd_port_snapshot_command(),
            PortCommandCapability::Partial,
        ),
        PortOs::Linux | PortOs::Unknown => (
            build_linux_port_snapshot_command(),
            PortCommandCapability::Full,
        ),
    };
    PortCaptureCommand {
        command,
        capability,
    }
}

pub fn build_port_diagnostic_command(os_type: &str) -> String {
    match port_os(os_type) {
        PortOs::Windows => concat!(
            "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
            "Get-NetTCPConnection | Sort-Object LocalPort | Format-Table -AutoSize; ",
            "Get-NetUDPEndpoint | Sort-Object LocalPort | Format-Table -AutoSize",
            "\""
        )
        .to_string(),
        PortOs::MacOs => "lsof -nP -iTCP -iUDP || netstat -anv".to_string(),
        PortOs::Bsd => "sockstat -4 -6 -l || netstat -an".to_string(),
        PortOs::Linux | PortOs::Unknown => concat!(
            "if command -v ss >/dev/null 2>&1; then ss -tulpen; ss -tanp; ",
            "elif command -v lsof >/dev/null 2>&1; then lsof -nP -iTCP -iUDP; ",
            "else netstat -tunlp; fi"
        )
        .to_string(),
    }
}

pub fn parse_port_snapshot(output: &str) -> ResourcePortSnapshot {
    let Some(section) = extract_section(output, "PORTS") else {
        return ResourcePortSnapshot::default();
    };

    let mut entries = Vec::new();
    let mut capability = PortCommandCapability::Unknown;
    let mut platform = "unknown".to_string();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line == PORT_UNAVAILABLE_MARKER {
            return ResourcePortSnapshot {
                status: ResourcePortStatus::Unavailable,
                entries: Vec::new(),
            };
        }
        if let Some(message) = line.strip_prefix(PORT_ERROR_MARKER) {
            return ResourcePortSnapshot {
                status: ResourcePortStatus::Error {
                    message: clean_marker_message(message, "Port command failed."),
                },
                entries: Vec::new(),
            };
        }
        if let Some((next_capability, next_platform)) = parse_port_capability_line(line) {
            capability = next_capability;
            platform = next_platform;
            continue;
        }
        if let Some(entry) = parse_port_row_line(line)
            .or_else(|| parse_ss_line(line))
            .or_else(|| parse_netstat_line(line))
            .or_else(|| parse_lsof_line(line))
            .or_else(|| parse_sockstat_line(line))
            .or_else(|| parse_windows_port_line(line))
        {
            entries.push(entry);
        }
    }

    dedupe_and_sort_port_entries(&mut entries);
    ResourcePortSnapshot {
        status: ResourcePortStatus::Available {
            capability,
            platform,
        },
        entries,
    }
}

pub fn visible_port_rows(
    entries: &[ResourcePortEntry],
    query: &str,
    filter: PortFilter,
) -> Vec<ResourcePortEntry> {
    let query = query.trim().to_lowercase();
    entries
        .iter()
        .filter(|entry| port_matches_filter(entry, filter))
        .filter(|entry| query.is_empty() || port_matches_query(entry, &query))
        .cloned()
        .collect()
}

pub fn port_row_signature(entry: &ResourcePortEntry) -> u64 {
    let mut hasher = DefaultHasher::new();
    entry.protocol.hash(&mut hasher);
    entry.local_address.hash(&mut hasher);
    entry.local_port.hash(&mut hasher);
    entry.remote_address.hash(&mut hasher);
    entry.remote_port.hash(&mut hasher);
    entry.state.hash(&mut hasher);
    entry.pid.hash(&mut hasher);
    entry.process_name.hash(&mut hasher);
    entry.user.hash(&mut hasher);
    entry.command.hash(&mut hasher);
    entry.inode.hash(&mut hasher);
    entry.source.hash(&mut hasher);
    hasher.finish()
}

pub fn port_state_label_key(state: &str) -> &'static str {
    match state.trim().to_lowercase().as_str() {
        "listen" | "listening" => "sidebar.host_ports.states.listening",
        "estab" | "established" => "sidebar.host_ports.states.established",
        "udp" | "unconn" | "open" => "sidebar.host_ports.states.open",
        "time-wait" | "time_wait" => "sidebar.host_ports.states.time_wait",
        "close-wait" | "close_wait" => "sidebar.host_ports.states.close_wait",
        _ => "sidebar.host_ports.states.unknown",
    }
}

pub fn port_filter_label_key(filter: PortFilter) -> &'static str {
    match filter {
        PortFilter::All => "sidebar.host_ports.filters.all",
        PortFilter::Listening => "sidebar.host_ports.filters.listening",
        PortFilter::Connected => "sidebar.host_ports.filters.connected",
        PortFilter::Tcp => "sidebar.host_ports.filters.tcp",
        PortFilter::Udp => "sidebar.host_ports.filters.udp",
        PortFilter::Risky => "sidebar.host_ports.filters.risky",
    }
}

pub fn port_endpoint(address: &str, port: &str) -> String {
    if address.trim().is_empty() && port.trim().is_empty() {
        return "-".to_string();
    }
    if port.trim().is_empty() {
        return address.trim().to_string();
    }
    if address.trim().is_empty() {
        return port.trim().to_string();
    }
    format!("{}:{}", address.trim(), port.trim())
}

pub fn port_is_risky_exposure(entry: &ResourcePortEntry) -> bool {
    if !is_listening_state(&entry.state) || !is_wildcard_address(&entry.local_address) {
        return false;
    }
    matches!(
        entry.local_port.parse::<u16>().ok(),
        Some(
            21 | 22
                | 23
                | 25
                | 80
                | 443
                | 3306
                | 3389
                | 5432
                | 5900
                | 6379
                | 8080
                | 9200
                | 9300
                | 11211
                | 15672
                | 2375
                | 27017
        )
    )
}

fn build_linux_port_snapshot_command() -> String {
    concat!(
        "echo '===PORTS==='; ",
        "if command -v ss >/dev/null 2>&1; then ",
        "echo '__OXIDE_PORT_CAPABILITY__\tfull\tlinux_ss'; ",
        "oxide_ports_ss=$(ss -H -tunlp 2>&1); oxide_ports_status=$?; ",
        "if [ \"$oxide_ports_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_ports_ss\" | sed 's/^/SS\\t/'; ",
        "else printf '__OXIDE_PORT_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_ports_ss\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
        "oxide_ports_estab=$(ss -H -tanp 2>/dev/null); ",
        "if [ -n \"$oxide_ports_estab\" ]; then printf '%s\\n' \"$oxide_ports_estab\" | sed 's/^/SS\\t/'; fi; ",
        "elif command -v netstat >/dev/null 2>&1; then ",
        "echo '__OXIDE_PORT_CAPABILITY__\tpartial\tlinux_netstat'; ",
        "oxide_ports_netstat=$(netstat -tunlp 2>&1); oxide_ports_status=$?; ",
        "if [ \"$oxide_ports_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_ports_netstat\" | sed 's/^/NETSTAT\\t/'; ",
        "else printf '__OXIDE_PORT_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_ports_netstat\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
        "elif command -v lsof >/dev/null 2>&1; then ",
        "echo '__OXIDE_PORT_CAPABILITY__\tpartial\tlinux_lsof'; ",
        "oxide_ports_lsof=$(lsof -nP -iTCP -iUDP 2>&1); oxide_ports_status=$?; ",
        "if [ \"$oxide_ports_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_ports_lsof\" | sed 's/^/LSOF\\t/'; ",
        "else printf '__OXIDE_PORT_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_ports_lsof\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
        "else echo '__OXIDE_PORT_UNAVAILABLE__'; fi; ",
        "echo '===PORTS_END==='"
    )
    .to_string()
}

fn build_macos_port_snapshot_command() -> String {
    concat!(
        "echo '===PORTS==='; ",
        "if command -v lsof >/dev/null 2>&1; then ",
        "echo '__OXIDE_PORT_CAPABILITY__\tpartial\tmacos_lsof'; ",
        "oxide_ports_lsof=$(lsof -nP -iTCP -iUDP 2>&1); oxide_ports_status=$?; ",
        "if [ \"$oxide_ports_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_ports_lsof\" | sed 's/^/LSOF\\t/'; ",
        "else printf '__OXIDE_PORT_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_ports_lsof\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
        "else echo '__OXIDE_PORT_UNAVAILABLE__'; fi; ",
        "echo '===PORTS_END==='"
    )
    .to_string()
}

fn build_bsd_port_snapshot_command() -> String {
    concat!(
        "echo '===PORTS==='; ",
        "if command -v sockstat >/dev/null 2>&1; then ",
        "echo '__OXIDE_PORT_CAPABILITY__\tpartial\tbsd_sockstat'; ",
        "oxide_ports_sockstat=$(sockstat -4 -6 -l 2>&1); oxide_ports_status=$?; ",
        "if [ \"$oxide_ports_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_ports_sockstat\" | sed 's/^/SOCKSTAT\\t/'; ",
        "else printf '__OXIDE_PORT_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_ports_sockstat\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
        "elif command -v netstat >/dev/null 2>&1; then ",
        "echo '__OXIDE_PORT_CAPABILITY__\tpartial\tbsd_netstat'; ",
        "oxide_ports_netstat=$(netstat -an 2>&1); oxide_ports_status=$?; ",
        "if [ \"$oxide_ports_status\" -eq 0 ]; then printf '%s\\n' \"$oxide_ports_netstat\" | sed 's/^/NETSTAT\\t/'; ",
        "else printf '__OXIDE_PORT_ERROR__\\t%s\\n' \"$(printf '%s' \"$oxide_ports_netstat\" | head -n 1 | tr '\\t' ' ')\"; fi; ",
        "else echo '__OXIDE_PORT_UNAVAILABLE__'; fi; ",
        "echo '===PORTS_END==='"
    )
    .to_string()
}

fn build_windows_port_snapshot_command() -> String {
    concat!(
        "powershell -NoProfile -ExecutionPolicy Bypass -Command \"",
        "Write-Output '===PORTS===';",
        "Write-Output ('__OXIDE_PORT_CAPABILITY__'+[char]9+'partial'+[char]9+'windows_powershell');",
        "try{",
        "Get-NetTCPConnection|ForEach-Object{",
        "$p='';$n='';try{if($_.OwningProcess){$pr=Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue;$p=[string]$_.OwningProcess;$n=[string]$pr.ProcessName}}catch{};",
        "Write-Output ('WIN'+[char]9+'tcp'+[char]9+$_.LocalAddress+[char]9+$_.LocalPort+[char]9+$_.RemoteAddress+[char]9+$_.RemotePort+[char]9+$_.State+[char]9+$p+[char]9+$n)",
        "};",
        "Get-NetUDPEndpoint|ForEach-Object{",
        "$p='';$n='';try{if($_.OwningProcess){$pr=Get-Process -Id $_.OwningProcess -ErrorAction SilentlyContinue;$p=[string]$_.OwningProcess;$n=[string]$pr.ProcessName}}catch{};",
        "Write-Output ('WIN'+[char]9+'udp'+[char]9+$_.LocalAddress+[char]9+$_.LocalPort+[char]9+''+[char]9+''+[char]9+'Open'+[char]9+$p+[char]9+$n)",
        "}",
        "}catch{Write-Output ('__OXIDE_PORT_ERROR__'+[char]9+$_.Exception.Message)};",
        "Write-Output '===PORTS_END==='",
        "\""
    )
    .to_string()
}

fn parse_port_capability_line(line: &str) -> Option<(PortCommandCapability, String)> {
    let payload = line.strip_prefix(PORT_CAPABILITY_MARKER)?;
    let parts = payload
        .trim_start_matches('\t')
        .split('\t')
        .collect::<Vec<_>>();
    let capability = match parts.first().copied().unwrap_or("unknown") {
        "full" => PortCommandCapability::Full,
        "partial" => PortCommandCapability::Partial,
        _ => PortCommandCapability::Unknown,
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

fn parse_port_row_line(line: &str) -> Option<ResourcePortEntry> {
    let payload = line.strip_prefix("ROW\t")?;
    let parts = payload.splitn(12, '\t').collect::<Vec<_>>();
    if parts.len() != 12 {
        return None;
    }
    Some(ResourcePortEntry {
        protocol: clean(parts[0]),
        local_address: clean(parts[1]),
        local_port: clean(parts[2]),
        remote_address: clean(parts[3]),
        remote_port: clean(parts[4]),
        state: clean(parts[5]),
        pid: clean(parts[6]),
        process_name: clean(parts[7]),
        user: clean(parts[8]),
        command: clean(parts[9]),
        inode: clean(parts[10]),
        source: clean(parts[11]),
    })
}

fn parse_ss_line(line: &str) -> Option<ResourcePortEntry> {
    let payload = line.strip_prefix("SS\t")?;
    let fields = payload.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 6 {
        return None;
    }
    let protocol = fields[0].trim_end_matches(|c: char| c.is_ascii_digit());
    let state = fields[1];
    let (local_address, local_port) = split_endpoint(fields[4]);
    let (remote_address, remote_port) = split_endpoint(fields[5]);
    let process_field = fields
        .get(6..)
        .map(|parts| parts.join(" "))
        .unwrap_or_default();
    let (pid, process_name) = parse_ss_process_field(&process_field);
    Some(ResourcePortEntry {
        protocol: protocol.to_lowercase(),
        local_address,
        local_port,
        remote_address,
        remote_port,
        state: clean(state),
        pid,
        process_name,
        user: String::new(),
        command: process_field,
        inode: String::new(),
        source: "ss".to_string(),
    })
}

fn parse_netstat_line(line: &str) -> Option<ResourcePortEntry> {
    let payload = line.strip_prefix("NETSTAT\t")?;
    let fields = payload.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 4 {
        return None;
    }
    let protocol = fields[0].trim_end_matches(|c: char| c.is_ascii_digit());
    if protocol != "tcp" && protocol != "udp" {
        return None;
    }
    let local_index = 3;
    let remote_index = 4;
    let state_index = 5;
    let process_index = if protocol == "tcp" { 6 } else { 5 };
    let (local_address, local_port) = split_endpoint(fields.get(local_index).copied()?);
    let (remote_address, remote_port) = fields
        .get(remote_index)
        .map(|endpoint| split_endpoint(endpoint))
        .unwrap_or_default();
    let state = if protocol == "udp" {
        "Open".to_string()
    } else {
        clean(fields.get(state_index).copied().unwrap_or_default())
    };
    let process_field = fields.get(process_index).copied().unwrap_or_default();
    let (pid, process_name) = split_pid_process(process_field);
    Some(ResourcePortEntry {
        protocol: protocol.to_string(),
        local_address,
        local_port,
        remote_address,
        remote_port,
        state,
        pid,
        process_name: process_name.clone(),
        user: String::new(),
        command: process_name,
        inode: String::new(),
        source: "netstat".to_string(),
    })
}

fn parse_lsof_line(line: &str) -> Option<ResourcePortEntry> {
    let payload = line.strip_prefix("LSOF\t")?;
    if payload.starts_with("COMMAND ") {
        return None;
    }
    let fields = payload.split_whitespace().collect::<Vec<_>>();
    let protocol_index = fields.iter().position(|field| {
        matches!(
            field.to_ascii_uppercase().as_str(),
            "TCP" | "UDP" | "TCP4" | "TCP6" | "UDP4" | "UDP6"
        )
    })?;
    if protocol_index < 3 || fields.len() <= protocol_index + 1 {
        return None;
    }
    // lsof's COMMAND column is intentionally truncated by lsof itself. Keep it
    // as a parser-owned process hint instead of trying to reconstruct a shell
    // command from variable-width columns.
    let command = clean(fields[0]);
    let pid = clean(fields.get(1).copied().unwrap_or_default());
    let user = clean(fields.get(2).copied().unwrap_or_default());
    let protocol = fields[protocol_index]
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .to_lowercase();
    let name = fields[protocol_index + 1..].join(" ");
    let state = name
        .rsplit_once('(')
        .and_then(|(_, suffix)| suffix.strip_suffix(')'))
        .unwrap_or(if protocol == "udp" { "Open" } else { "" });
    let endpoint = name.split_whitespace().next().unwrap_or_default();
    let (local, remote) = endpoint
        .split_once("->")
        .map(|(local, remote)| (local, remote))
        .unwrap_or((endpoint, ""));
    let (local_address, local_port) = split_endpoint(local);
    let (remote_address, remote_port) = split_endpoint(remote);
    Some(ResourcePortEntry {
        protocol,
        local_address,
        local_port,
        remote_address,
        remote_port,
        state: clean(state),
        pid,
        process_name: command.clone(),
        user,
        command,
        inode: String::new(),
        source: "lsof".to_string(),
    })
}

fn parse_sockstat_line(line: &str) -> Option<ResourcePortEntry> {
    let payload = line.strip_prefix("SOCKSTAT\t")?;
    if payload.starts_with("USER ") {
        return None;
    }
    let fields = payload.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 7 {
        return None;
    }
    let protocol = fields[4].trim_end_matches(|c: char| c.is_ascii_digit());
    if protocol != "tcp" && protocol != "udp" {
        return None;
    }
    let (local_address, local_port) = split_endpoint(fields[5]);
    let (remote_address, remote_port) = split_endpoint(fields[6]);
    Some(ResourcePortEntry {
        protocol: protocol.to_string(),
        local_address,
        local_port,
        remote_address,
        remote_port,
        state: if protocol == "udp" {
            "Open".to_string()
        } else {
            "Listen".to_string()
        },
        pid: clean(fields[2]),
        process_name: clean(fields[1]),
        user: clean(fields[0]),
        command: clean(fields[1]),
        inode: String::new(),
        source: "sockstat".to_string(),
    })
}

fn parse_windows_port_line(line: &str) -> Option<ResourcePortEntry> {
    let payload = line.strip_prefix("WIN\t")?;
    let parts = payload.splitn(9, '\t').collect::<Vec<_>>();
    if parts.len() < 8 {
        return None;
    }
    let process_name = clean(parts[7]);
    let command = parts
        .get(8)
        .map(|value| clean(value))
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| process_name.clone());
    Some(ResourcePortEntry {
        protocol: clean(parts[0]).to_lowercase(),
        local_address: clean(parts[1]),
        local_port: clean(parts[2]),
        remote_address: clean(parts[3]),
        remote_port: clean(parts[4]),
        state: clean(parts[5]),
        pid: clean(parts[6]),
        process_name,
        user: String::new(),
        command,
        inode: String::new(),
        source: "windows_powershell".to_string(),
    })
}

fn parse_ss_process_field(value: &str) -> (String, String) {
    let pid = value
        .split("pid=")
        .nth(1)
        .map(|tail| {
            tail.chars()
                .take_while(|ch| ch.is_ascii_digit())
                .collect::<String>()
        })
        .unwrap_or_default();
    let process = value
        .split("(\"")
        .nth(1)
        .and_then(|tail| tail.split('"').next())
        .unwrap_or_default()
        .to_string();
    (pid, process)
}

fn split_pid_process(value: &str) -> (String, String) {
    let Some((pid, process)) = value.split_once('/') else {
        return (String::new(), clean(value));
    };
    (clean(pid), clean(process))
}

fn split_endpoint(value: &str) -> (String, String) {
    let value = value.trim();
    if value.is_empty() || value == "*" {
        return (clean(value), String::new());
    }
    if let Some(rest) = value.strip_prefix('[')
        && let Some((address, tail)) = rest.split_once(']')
    {
        let port = tail.strip_prefix(':').unwrap_or_default();
        return (clean(address), clean(port));
    }
    if let Some((address, port)) = value.rsplit_once(':') {
        return (clean(address), clean(port));
    }
    if let Some((address, port)) = value.rsplit_once('.') {
        return (clean(address), clean(port));
    }
    (clean(value), String::new())
}

fn dedupe_and_sort_port_entries(entries: &mut Vec<ResourcePortEntry>) {
    let mut seen = HashSet::new();
    entries.retain(|entry| {
        seen.insert((
            entry.protocol.clone(),
            entry.local_address.clone(),
            entry.local_port.clone(),
            entry.remote_address.clone(),
            entry.remote_port.clone(),
            entry.state.clone(),
            entry.pid.clone(),
            entry.process_name.clone(),
        ))
    });
    entries.sort_by(|left, right| {
        port_sort_key(&left.local_port)
            .cmp(&port_sort_key(&right.local_port))
            .then(left.protocol.cmp(&right.protocol))
            .then(left.local_address.cmp(&right.local_address))
            .then(left.remote_address.cmp(&right.remote_address))
            .then(left.pid.cmp(&right.pid))
    });
}

fn port_sort_key(port: &str) -> u32 {
    port.parse::<u32>().unwrap_or(u32::MAX)
}

fn port_matches_filter(entry: &ResourcePortEntry, filter: PortFilter) -> bool {
    match filter {
        PortFilter::All => true,
        PortFilter::Listening => is_listening_state(&entry.state),
        PortFilter::Connected => is_connected_state(&entry.state),
        PortFilter::Tcp => entry.protocol.eq_ignore_ascii_case("tcp"),
        PortFilter::Udp => entry.protocol.eq_ignore_ascii_case("udp"),
        PortFilter::Risky => port_is_risky_exposure(entry),
    }
}

fn port_matches_query(entry: &ResourcePortEntry, query: &str) -> bool {
    [
        entry.protocol.as_str(),
        entry.local_address.as_str(),
        entry.local_port.as_str(),
        entry.remote_address.as_str(),
        entry.remote_port.as_str(),
        entry.state.as_str(),
        entry.pid.as_str(),
        entry.process_name.as_str(),
        entry.user.as_str(),
        entry.command.as_str(),
        entry.inode.as_str(),
        entry.source.as_str(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(query))
}

fn is_listening_state(state: &str) -> bool {
    matches!(
        state.trim().to_lowercase().as_str(),
        "listen" | "listening" | "unconn" | "udp" | "open"
    )
}

fn is_connected_state(state: &str) -> bool {
    matches!(
        state.trim().to_lowercase().as_str(),
        "estab" | "established" | "syn-sent" | "syn-recv" | "close-wait" | "time-wait"
    )
}

fn is_wildcard_address(address: &str) -> bool {
    matches!(
        address.trim().to_lowercase().as_str(),
        "*" | "0.0.0.0" | "::" | "[::]" | ":::" | ""
    )
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
enum PortOs {
    Linux,
    MacOs,
    Bsd,
    Windows,
    Unknown,
}

fn port_os(os_type: &str) -> PortOs {
    match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => PortOs::Linux,
        "macOS" | "macos" | "Darwin" => PortOs::MacOs,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => PortOs::Bsd,
        "Windows" | "windows" => PortOs::Windows,
        _ => PortOs::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_linux_ss_tcp_listen_and_udp_without_pid() {
        let output = concat!(
            "===PORTS===\n",
            "__OXIDE_PORT_CAPABILITY__\tfull\tlinux_ss\n",
            "SS\ttcp LISTEN 0 4096 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=123,fd=3))\n",
            "SS\tudp UNCONN 0 0 127.0.0.53%lo:53 0.0.0.0:*\n",
            "===PORTS_END===\n"
        );

        let snapshot = parse_port_snapshot(output);
        let rows = visible_port_rows(&snapshot.entries, "sshd", PortFilter::All);

        assert_eq!(
            snapshot.status,
            ResourcePortStatus::Available {
                capability: PortCommandCapability::Full,
                platform: "linux_ss".to_string(),
            }
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].local_port, "22");
        assert_eq!(rows[0].pid, "123");
        assert_eq!(snapshot.entries[1].protocol, "udp");
        assert_eq!(snapshot.entries[1].pid, "");
    }

    #[test]
    fn parses_linux_ss_established_ipv6_and_risky_filter() {
        let output = concat!(
            "===PORTS===\n",
            "__OXIDE_PORT_CAPABILITY__\tfull\tlinux_ss\n",
            "SS\ttcp LISTEN 0 128 [::]:6379 [::]:* users:((\"redis-server\",pid=6379,fd=7))\n",
            "SS\ttcp ESTAB 0 0 [2001:db8::1]:22 [2001:db8::2]:51444 users:((\"sshd\",pid=777,fd=4))\n",
            "===PORTS_END===\n"
        );

        let snapshot = parse_port_snapshot(output);
        let risky = visible_port_rows(&snapshot.entries, "", PortFilter::Risky);
        let connected = visible_port_rows(&snapshot.entries, "51444", PortFilter::Connected);

        assert_eq!(risky.len(), 1);
        assert_eq!(risky[0].process_name, "redis-server");
        assert_eq!(connected.len(), 1);
        assert_eq!(connected[0].remote_address, "2001:db8::2");
    }

    #[test]
    fn parses_macos_lsof_rows() {
        let output = concat!(
            "===PORTS===\n",
            "__OXIDE_PORT_CAPABILITY__\tpartial\tmacos_lsof\n",
            "LSOF\tCOMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME\n",
            "LSOF\tPython 3456 dominical 12u IPv4 0x123 0t0 TCP 127.0.0.1:8000 (LISTEN)\n",
            "LSOF\tssh 3457 dominical 13u IPv4 0x124 0t0 TCP 127.0.0.1:55555->10.0.0.2:22 (ESTABLISHED)\n",
            "===PORTS_END===\n"
        );

        let snapshot = parse_port_snapshot(output);
        let rows = visible_port_rows(&snapshot.entries, "dominical", PortFilter::Tcp);

        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].local_port, "8000");
        assert_eq!(rows[0].state, "LISTEN");
        assert_eq!(rows[1].remote_port, "22");
        assert_eq!(rows[1].state, "ESTABLISHED");
    }

    #[test]
    fn parses_bsd_sockstat_rows() {
        let output = concat!(
            "===PORTS===\n",
            "__OXIDE_PORT_CAPABILITY__\tpartial\tbsd_sockstat\n",
            "SOCKSTAT\tUSER COMMAND PID FD PROTO LOCAL ADDRESS FOREIGN ADDRESS\n",
            "SOCKSTAT\troot sshd 159 4 tcp4 *:22 *:*\n",
            "SOCKSTAT\twww nginx 456 7 tcp6 [::]:443 *:*\n",
            "===PORTS_END===\n"
        );

        let snapshot = parse_port_snapshot(output);

        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(snapshot.entries[0].user, "root");
        assert_eq!(snapshot.entries[1].local_address, "::");
    }

    #[test]
    fn parses_windows_powershell_rows() {
        let output = concat!(
            "===PORTS===\n",
            "__OXIDE_PORT_CAPABILITY__\tpartial\twindows_powershell\n",
            "WIN\ttcp\t0.0.0.0\t3389\t0.0.0.0\t0\tListen\t888\tTermService\n",
            "WIN\tudp\t127.0.0.1\t5353\t\t\tOpen\t999\tmDNSResponder\n",
            "===PORTS_END===\n"
        );

        let snapshot = parse_port_snapshot(output);
        let udp = visible_port_rows(&snapshot.entries, "mdns", PortFilter::Udp);

        assert_eq!(snapshot.entries.len(), 2);
        assert_eq!(udp.len(), 1);
        assert_eq!(snapshot.entries[0].local_port, "3389");
    }

    #[test]
    fn normalized_rows_preserve_command_and_search_all_fields() {
        let output = concat!(
            "===PORTS===\n",
            "__OXIDE_PORT_CAPABILITY__\tpartial\tfixture\n",
            "ROW\ttcp\t127.0.0.1\t3000\t127.0.0.1\t51234\tESTABLISHED\t4242\tnode server\tdominical\tnode server.js --inspect\tinode-1\tfixture\n",
            "===PORTS_END===\n"
        );

        let snapshot = parse_port_snapshot(output);
        let rows = visible_port_rows(&snapshot.entries, "--inspect", PortFilter::Connected);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].process_name, "node server");
        assert_eq!(rows[0].command, "node server.js --inspect");
    }

    #[test]
    fn port_commands_keep_platforms_separate() {
        let linux = build_port_snapshot_command("Linux");
        let mac = build_port_snapshot_command("macOS");
        let bsd = build_port_snapshot_command("FreeBSD");
        let windows = build_port_snapshot_command("Windows");

        assert!(linux.command.contains("ss -H -tunlp"));
        assert_eq!(linux.capability, PortCommandCapability::Full);
        assert!(mac.command.contains("lsof -nP"));
        assert_eq!(mac.capability, PortCommandCapability::Partial);
        assert!(bsd.command.contains("sockstat"));
        assert_eq!(bsd.capability, PortCommandCapability::Partial);
        assert!(windows.command.contains("Get-NetTCPConnection"));
        assert_eq!(windows.capability, PortCommandCapability::Partial);
    }
}
