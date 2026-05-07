// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

pub const PORT_SCAN_TIMEOUT_SECS: u64 = 5;
pub const PORT_SCAN_MAX_OUTPUT_SIZE: usize = 65_536;

pub const REMOTE_OS_PROBE_TIMEOUT_SECS: u64 = 3;

pub const REMOTE_OS_PROBE_UNIX: &str = "echo '===DETECT==='; uname -s; echo '===END==='";

pub const REMOTE_OS_PROBE_WINDOWS: &str = "powershell -NoProfile -Command \"Write-Output '===DETECT==='; Write-Output ('PLATFORM=' + [System.Environment]::OSVersion.Platform); Write-Output ('OS=' + [System.Environment]::OSVersion.VersionString); Write-Output '===END==='\"";

pub const PORT_SCAN_COMMAND_LINUX: &str = concat!(
    "echo '===PORTS==='; ",
    "((ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null) | grep -i listen || true); ",
    "echo '===PORTS_END==='; ",
    "echo '===DOCKER==='; ",
    "((docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null || sudo -n docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null) || true); ",
    "echo '===DOCKER_END==='; ",
    "echo '===END==='"
);

pub const PORT_SCAN_COMMAND_MACOS: &str = concat!(
    "echo '===PORTS==='; ",
    "((lsof -iTCP -sTCP:LISTEN -nP 2>/dev/null | tail -n +2) || true); ",
    "echo '===PORTS_END==='; ",
    "echo '===DOCKER==='; ",
    "((docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null || sudo -n docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null) || true); ",
    "echo '===DOCKER_END==='; ",
    "echo '===END==='"
);

pub const PORT_SCAN_COMMAND_WINDOWS: &str = "echo '===PORTS==='; powershell -NoProfile -Command \"Get-NetTCPConnection -State Listen 2>$null | Select-Object LocalAddress,LocalPort,OwningProcess | Format-Table -HideTableHeaders\" 2>/dev/null; echo '===PORTS_END==='; echo '===END==='";

pub const PORT_SCAN_COMMAND_FREEBSD: &str = "echo '===PORTS==='; sockstat -4 -6 -l -P tcp 2>/dev/null | tail -n +2; echo '===PORTS_END==='; echo '===END==='";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RemotePortScanPlatform {
    Linux,
    MacOs,
    Windows,
    WindowsUnix,
    FreeBsd,
    Unknown,
}

impl RemotePortScanPlatform {
    pub fn scan_command(self) -> &'static str {
        match self {
            Self::Linux | Self::WindowsUnix | Self::Unknown => PORT_SCAN_COMMAND_LINUX,
            Self::MacOs => PORT_SCAN_COMMAND_MACOS,
            Self::Windows => PORT_SCAN_COMMAND_WINDOWS,
            Self::FreeBsd => PORT_SCAN_COMMAND_FREEBSD,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DetectedPort {
    pub port: u16,
    pub bind_addr: String,
    pub process_name: Option<String>,
    pub pid: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PortDetectionSnapshot {
    pub new_ports: Vec<DetectedPort>,
    pub closed_ports: Vec<DetectedPort>,
    pub all_ports: Vec<DetectedPort>,
    pub has_scanned: bool,
}

#[derive(Debug, Default)]
pub struct PortDetectionTracker {
    ignored_ports: HashSet<u16>,
    previous_ports: HashSet<u16>,
    detected_ports: Vec<DetectedPort>,
    has_scanned: bool,
}

impl PortDetectionTracker {
    pub fn snapshot(&self) -> PortDetectionSnapshot {
        PortDetectionSnapshot {
            new_ports: Vec::new(),
            closed_ports: Vec::new(),
            all_ports: self.detected_ports.clone(),
            has_scanned: self.has_scanned,
        }
    }

    pub fn ignore_port(&mut self, port: u16) {
        self.ignored_ports.insert(port);
    }

    pub fn apply_scan(&mut self, ports: Vec<DetectedPort>) -> PortDetectionSnapshot {
        let current_ports: HashSet<u16> = ports.iter().map(|port| port.port).collect();

        if !self.has_scanned {
            self.previous_ports = current_ports;
            self.detected_ports = ports.clone();
            self.has_scanned = true;
            return PortDetectionSnapshot {
                new_ports: Vec::new(),
                closed_ports: Vec::new(),
                all_ports: ports,
                has_scanned: true,
            };
        }

        let new_port_numbers: HashSet<u16> = current_ports
            .difference(&self.previous_ports)
            .copied()
            .collect();
        let closed_port_numbers: HashSet<u16> = self
            .previous_ports
            .difference(&current_ports)
            .copied()
            .collect();

        let new_ports = ports
            .iter()
            .filter(|port| {
                new_port_numbers.contains(&port.port)
                    && port.port != 22
                    && !self.ignored_ports.contains(&port.port)
            })
            .cloned()
            .collect();
        let closed_ports = self
            .previous_ports
            .iter()
            .filter(|port| closed_port_numbers.contains(port))
            .map(|port| DetectedPort {
                port: *port,
                bind_addr: String::new(),
                process_name: None,
                pid: None,
            })
            .collect();

        self.previous_ports = current_ports;
        self.detected_ports = ports.clone();

        PortDetectionSnapshot {
            new_ports,
            closed_ports,
            all_ports: ports,
            has_scanned: true,
        }
    }
}

pub fn classify_remote_platform(output: &str) -> RemotePortScanPlatform {
    let upper = output.to_uppercase();
    if upper.contains("PLATFORM=WIN32NT") || upper.contains("WINDOWS") {
        return RemotePortScanPlatform::Windows;
    }

    let platform = extract_section(output, "DETECT")
        .and_then(|section| {
            section
                .lines()
                .map(str::trim)
                .find(|line| {
                    !line.is_empty()
                        && !line.starts_with("PLATFORM=")
                        && !line.eq_ignore_ascii_case("===DETECT===")
                })
                .map(ToOwned::to_owned)
        })
        .unwrap_or_default();
    classify_unix_platform(&platform)
}

pub fn classify_unix_platform(uname_s: &str) -> RemotePortScanPlatform {
    let upper = uname_s.trim().to_uppercase();
    if upper.starts_with("MINGW") || upper.starts_with("MSYS") || upper.starts_with("CYGWIN") {
        return RemotePortScanPlatform::WindowsUnix;
    }

    match uname_s.trim() {
        "Linux" | "linux" => RemotePortScanPlatform::Linux,
        "Darwin" | "macOS" | "macos" => RemotePortScanPlatform::MacOs,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => RemotePortScanPlatform::FreeBsd,
        _ => RemotePortScanPlatform::Unknown,
    }
}

pub fn parse_listening_ports(output: &str, platform: RemotePortScanPlatform) -> Vec<DetectedPort> {
    let ports_section = match extract_section(output, "PORTS") {
        Some(section) => section,
        None => return Vec::new(),
    };

    let mut ports = match platform {
        RemotePortScanPlatform::Linux
        | RemotePortScanPlatform::WindowsUnix
        | RemotePortScanPlatform::Unknown => parse_ports_ss(ports_section),
        RemotePortScanPlatform::MacOs => parse_ports_lsof(ports_section),
        RemotePortScanPlatform::Windows => parse_ports_powershell(ports_section),
        RemotePortScanPlatform::FreeBsd => parse_ports_sockstat(ports_section),
    };
    let mut seen: HashSet<u16> = ports.iter().map(|port| port.port).collect();
    push_unique_ports(&mut ports, &mut seen, parse_ports_docker(output));
    ports
}

fn push_unique_ports(
    ports: &mut Vec<DetectedPort>,
    seen: &mut HashSet<u16>,
    candidates: Vec<DetectedPort>,
) {
    for port in candidates {
        if seen.insert(port.port) {
            ports.push(port);
        }
    }
}

fn extract_section<'a>(output: &'a str, marker: &str) -> Option<&'a str> {
    let start_marker = format!("==={marker}===");
    let start = output.find(&start_marker)?;
    let rest = &output[start + start_marker.len()..];
    let end = rest.find("===").unwrap_or(rest.len());
    Some(rest[..end].trim())
}

fn parse_ports_ss(section: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() >= 4 && parts[0].eq_ignore_ascii_case("listen") {
            if let Some(mut detected) = parse_addr_port(parts[3]) {
                if let Some(users_part) = parts.iter().find(|part| part.starts_with("users:")) {
                    detected = extract_process_from_ss_users(users_part, detected);
                }
                if seen.insert(detected.port) {
                    ports.push(detected);
                }
            }
            continue;
        }

        if parts.len() >= 6 && parts.iter().any(|part| part.eq_ignore_ascii_case("listen")) {
            if let Some(mut detected) = parse_addr_port(parts[3]) {
                if let Some(last) = parts.last()
                    && let Some((pid, name)) = last.split_once('/')
                {
                    detected.pid = pid.parse().ok();
                    detected.process_name = Some(name.to_string());
                }
                if seen.insert(detected.port) {
                    ports.push(detected);
                }
            }
        }
    }

    ports
}

fn parse_ports_lsof(section: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9
            || !parts
                .iter()
                .any(|part| part.eq_ignore_ascii_case("(LISTEN)"))
        {
            continue;
        }

        let process_name = parts[0].to_string();
        let pid = parts[1].parse().ok();
        if let Some(mut detected) = parse_addr_port(parts[8]) {
            detected.process_name = Some(process_name);
            detected.pid = pid;
            if seen.insert(detected.port) {
                ports.push(detected);
            }
        }
    }

    ports
}

fn parse_ports_sockstat(section: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 || parts[0].eq_ignore_ascii_case("user") {
            continue;
        }

        if let Some(mut detected) = parse_addr_port(parts[5]) {
            detected.process_name = Some(parts[1].to_string());
            detected.pid = parts[2].parse().ok();
            if seen.insert(detected.port) {
                ports.push(detected);
            }
        }
    }

    ports
}

fn parse_ports_powershell(section: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }
        let Ok(port) = parts[1].parse::<u16>() else {
            continue;
        };
        if seen.insert(port) {
            ports.push(DetectedPort {
                port,
                bind_addr: parts[0].to_string(),
                process_name: None,
                pid: parts.get(2).and_then(|pid| pid.parse().ok()),
            });
        }
    }

    ports
}

fn parse_ports_docker(output: &str) -> Vec<DetectedPort> {
    let section = match extract_section(output, "DOCKER") {
        Some(section) => section,
        None => return Vec::new(),
    };

    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        let tab_parts: Vec<&str> = line.splitn(3, '\t').collect();
        if tab_parts.len() < 3 {
            continue;
        }
        let container_name = tab_parts[1].trim();
        for segment in tab_parts[2].split(", ") {
            let Some((host_part, _)) = segment.trim().split_once("->") else {
                continue;
            };
            let Some(last_colon) = host_part.rfind(':') else {
                continue;
            };
            let Ok(port) = host_part[last_colon + 1..].parse::<u16>() else {
                continue;
            };
            if seen.insert(port) {
                let bind_addr = match &host_part[..last_colon] {
                    "" | "*" => "0.0.0.0".to_string(),
                    addr => addr.to_string(),
                };
                ports.push(DetectedPort {
                    port,
                    bind_addr,
                    process_name: Some(format!("docker:{container_name}")),
                    pid: None,
                });
            }
        }
    }

    ports
}

fn parse_addr_port(value: &str) -> Option<DetectedPort> {
    if let Some(bracket_end) = value.rfind("]:") {
        let port = value[bracket_end + 2..].parse().ok()?;
        return Some(DetectedPort {
            port,
            bind_addr: value[..bracket_end + 1].to_string(),
            process_name: None,
            pid: None,
        });
    }

    let last_colon = value.rfind(':')?;
    let port = value[last_colon + 1..].parse().ok()?;
    let addr = &value[..last_colon];
    let bind_addr = match addr {
        "" | "*" => "0.0.0.0".to_string(),
        addr => addr.to_string(),
    };
    Some(DetectedPort {
        port,
        bind_addr,
        process_name: None,
        pid: None,
    })
}

fn extract_process_from_ss_users(users_field: &str, mut detected: DetectedPort) -> DetectedPort {
    if let Some(start) = users_field.find("((\"") {
        let rest = &users_field[start + 3..];
        if let Some(end) = rest.find('"') {
            detected.process_name = Some(rest[..end].to_string());
        }
    }
    if let Some(start) = users_field.find("pid=") {
        let rest = &users_field[start + 4..];
        let pid: String = rest
            .chars()
            .take_while(|char| char.is_ascii_digit())
            .collect();
        detected.pid = pid.parse().ok();
    }
    detected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ss_netstat_and_docker_ports() {
        let output = "\
===PORTS===
LISTEN 0 128 0.0.0.0:8080 0.0.0.0:* users:((\"node\",pid=1234,fd=3))
tcp 0 0 127.0.0.1:5432 0.0.0.0:* LISTEN 55/postgres
===PORTS_END===
===DOCKER===
abc\tweb\t0.0.0.0:3000->3000/tcp, :::3000->3000/tcp
===DOCKER_END===
===END===";

        let ports = parse_listening_ports(output, RemotePortScanPlatform::Linux);

        assert_eq!(ports.len(), 3);
        assert!(ports.iter().any(|port| port.port == 8080
            && port.process_name.as_deref() == Some("node")
            && port.pid == Some(1234)));
        assert!(ports.iter().any(|port| port.port == 5432
            && port.process_name.as_deref() == Some("postgres")
            && port.pid == Some(55)));
        assert!(ports.iter().any(|port| port.port == 3000
            && port.process_name.as_deref() == Some("docker:web")));
    }

    #[test]
    fn classifies_tauri_remote_os_outputs() {
        assert_eq!(
            classify_remote_platform("===DETECT===\nLinux\n===END==="),
            RemotePortScanPlatform::Linux
        );
        assert_eq!(
            classify_remote_platform("===DETECT===\nDarwin\n===END==="),
            RemotePortScanPlatform::MacOs
        );
        assert_eq!(
            classify_remote_platform("===DETECT===\nMINGW64_NT-10.0-19045\n===END==="),
            RemotePortScanPlatform::WindowsUnix
        );
        assert_eq!(
            classify_remote_platform(
                "===DETECT===\nPLATFORM=Win32NT\nOS=Microsoft Windows NT 10.0.22631.0\n===END==="
            ),
            RemotePortScanPlatform::Windows
        );
    }

    #[test]
    fn parses_windows_powershell_listening_ports() {
        let output = "\
===PORTS===
0.0.0.0  8080  1234
::       3000  5678
===PORTS_END===
===END===";

        let ports = parse_listening_ports(output, RemotePortScanPlatform::Windows);

        assert_eq!(ports.len(), 2);
        assert!(ports.iter().any(|port| port.port == 8080
            && port.bind_addr == "0.0.0.0"
            && port.pid == Some(1234)));
        assert!(
            ports
                .iter()
                .any(|port| port.port == 3000 && port.bind_addr == "::" && port.pid == Some(5678))
        );
    }

    #[test]
    fn first_scan_is_silent_and_later_scans_report_visible_changes() {
        let mut tracker = PortDetectionTracker::default();
        let baseline = tracker.apply_scan(vec![DetectedPort {
            port: 22,
            bind_addr: "0.0.0.0".to_string(),
            process_name: Some("sshd".to_string()),
            pid: Some(1),
        }]);
        assert!(baseline.has_scanned);
        assert!(baseline.new_ports.is_empty());

        let next = tracker.apply_scan(vec![
            DetectedPort {
                port: 22,
                bind_addr: "0.0.0.0".to_string(),
                process_name: Some("sshd".to_string()),
                pid: Some(1),
            },
            DetectedPort {
                port: 8888,
                bind_addr: "127.0.0.1".to_string(),
                process_name: Some("python".to_string()),
                pid: Some(42),
            },
        ]);
        assert_eq!(next.new_ports.len(), 1);
        assert_eq!(next.new_ports[0].port, 8888);

        tracker.ignore_port(6006);
        let ignored = tracker.apply_scan(vec![
            DetectedPort {
                port: 8888,
                bind_addr: "127.0.0.1".to_string(),
                process_name: Some("python".to_string()),
                pid: Some(42),
            },
            DetectedPort {
                port: 6006,
                bind_addr: "127.0.0.1".to_string(),
                process_name: Some("tensorboard".to_string()),
                pid: Some(43),
            },
        ]);
        assert!(ignored.new_ports.is_empty());
        assert_eq!(ignored.closed_ports.len(), 1);
        assert_eq!(ignored.closed_ports[0].port, 22);
    }
}
