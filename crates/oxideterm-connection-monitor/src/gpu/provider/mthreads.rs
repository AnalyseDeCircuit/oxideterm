// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Moore Threads GMI command protocol and version-tolerant overview parser.

use std::collections::BTreeMap;

use super::{ProviderSnapshot, ProviderStatus, first_section_line, sanitized_error, section};
use crate::gpu::{GpuDevice, GpuProcess, GpuProvider};

pub(super) fn sample_command() -> String {
    concat!(
        "echo '===MTHREADS_STATUS==='; ",
        "if ! command -v mthreads-gmi >/dev/null 2>&1; then ",
        "echo unavailable; ",
        "else ",
        "echo available; ",
        "echo '===MTHREADS_DATA==='; ",
        "mthreads_output=$(LC_ALL=C mthreads-gmi 2>&1); ",
        "mthreads_exit=$?; printf '%s\\n' \"$mthreads_output\"; ",
        "echo '===MTHREADS_QUERY_EXIT==='; echo \"$mthreads_exit\"; ",
        "if [ \"$mthreads_exit\" -ne 0 ]; then ",
        "echo '===MTHREADS_ERROR==='; printf '%s\\n' \"$mthreads_output\"; ",
        "fi; ",
        "fi; "
    )
    .to_string()
}

pub(super) fn parse(output: &str) -> ProviderSnapshot {
    match first_section_line(output, "MTHREADS_STATUS") {
        Some("unavailable") => return empty_snapshot(ProviderStatus::Unavailable),
        Some("available") => {}
        _ => return empty_snapshot(ProviderStatus::Unknown),
    }

    let query_exit = first_section_line(output, "MTHREADS_QUERY_EXIT")
        .and_then(|value| value.parse::<i32>().ok());
    if query_exit.is_some_and(|exit| exit != 0) {
        let message = sanitized_error(output, "MTHREADS_ERROR", "mthreads-gmi query failed");
        if reports_no_devices(&message) {
            return empty_snapshot(ProviderStatus::NoDevices);
        }
        return empty_snapshot(ProviderStatus::Error(format!("Moore Threads: {message}")));
    }

    let Some(payload) = section(output, "MTHREADS_DATA") else {
        return empty_snapshot(ProviderStatus::NoDevices);
    };
    if reports_no_devices(payload) {
        return empty_snapshot(ProviderStatus::NoDevices);
    }

    // The overview layout is verified against official mthreads-gmi 1.6.0 and
    // 2.0.0 samples. Detailed --query JSON is intentionally not consumed until
    // Moore Threads publishes or supplies stable field-level samples.
    let parsed = parse_overview(payload);
    if parsed.devices.is_empty() {
        return empty_snapshot(ProviderStatus::Error(
            "Moore Threads: unrecognized mthreads-gmi output".into(),
        ));
    }

    ProviderSnapshot {
        status: ProviderStatus::Available,
        devices: parsed.devices.into_values().collect(),
        processes: parsed.processes,
    }
}

struct ParsedOverview {
    devices: BTreeMap<u32, GpuDevice>,
    processes: Vec<GpuProcess>,
}

fn parse_overview(payload: &str) -> ParsedOverview {
    let driver_version = header_value(payload, "Driver Version:");
    let mut devices = BTreeMap::new();
    let mut processes = Vec::new();
    let mut in_process_table = false;
    let mut last_device_index = None;

    for line in payload.lines().map(str::trim) {
        if line.eq_ignore_ascii_case("Processes:") {
            in_process_table = true;
            continue;
        }
        if in_process_table {
            if let Some(process) = parse_process(line, &devices) {
                processes.push(process);
            }
            continue;
        }
        if let Some(device) = parse_device(line, driver_version.as_deref()) {
            last_device_index = Some(device.index);
            devices.insert(device.index, device);
        } else if let Some(index) = last_device_index
            && let Some(temperature) = parse_temperature_line(line)
            && let Some(device) = devices.get_mut(&index)
        {
            device.temperature_celsius = Some(temperature);
        }
    }

    processes.sort_by(|left, right| {
        left.gpu_uuid
            .cmp(&right.gpu_uuid)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    ParsedOverview { devices, processes }
}

fn parse_temperature_line(line: &str) -> Option<f64> {
    let columns = line.split('|').map(str::trim).collect::<Vec<_>>();
    if columns.len() < 3 || !columns[0].eq_ignore_ascii_case("Physical") {
        return None;
    }
    columns[2]
        .split_whitespace()
        .find(|value| value.ends_with('C'))
        .and_then(|value| parse_number(value.trim_end_matches('C')))
}

fn parse_device(line: &str, driver_version: Option<&str>) -> Option<GpuDevice> {
    let columns = line.split('|').map(str::trim).collect::<Vec<_>>();
    if columns.len() < 3 {
        return None;
    }
    let mut identity = columns[0].split_whitespace();
    let index = identity.next()?.parse::<u32>().ok()?;
    let name = identity.collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return None;
    }
    let pci_bus_id = columns[1].split_whitespace().next()?.to_string();
    if !looks_like_pci_address(&pci_bus_id) {
        return None;
    }

    let telemetry = columns[2].split_whitespace().collect::<Vec<_>>();
    let utilization_percent = telemetry
        .iter()
        .find(|value| value.ends_with('%'))
        .and_then(|value| parse_number(value.trim_end_matches('%')));
    let (memory_used, memory_total) = telemetry
        .iter()
        .find_map(|value| parse_memory_pair(value))?;

    Some(GpuDevice {
        provider: GpuProvider::Mthreads,
        index,
        uuid: format!("MTHREADS-{pci_bus_id}"),
        pci_bus_id,
        name,
        driver_version: driver_version.map(str::to_string),
        performance_state: None,
        health_status: None,
        utilization_percent,
        memory_utilization_percent: (memory_total > 0)
            .then_some((memory_used as f64 / memory_total as f64) * 100.0),
        memory_used: Some(memory_used),
        memory_total: Some(memory_total),
        temperature_celsius: None,
        power_draw_watts: None,
        power_limit_watts: None,
        fan_speed_percent: None,
    })
}

fn parse_process(line: &str, devices: &BTreeMap<u32, GpuDevice>) -> Option<GpuProcess> {
    let fields = line.split_whitespace().collect::<Vec<_>>();
    if fields.len() < 4 {
        return None;
    }
    let index = fields[0].parse::<u32>().ok()?;
    let pid = fields[1].parse::<u32>().ok()?;
    let device = devices.get(&index)?;
    let (used_memory, process_name_end) = parse_byte_value(fields.last()?)
        .map(|memory| (memory, fields.len() - 1))
        .or_else(|| {
            // Older table renderers may separate the numeric value and unit.
            let value_with_unit = format!(
                "{}{}",
                fields.get(fields.len().checked_sub(2)?)?,
                fields.last()?
            );
            parse_byte_value(&value_with_unit).map(|memory| (memory, fields.len() - 2))
        })?;
    let process_name = fields[2..process_name_end].join(" ");

    Some(GpuProcess {
        provider: GpuProvider::Mthreads,
        gpu_uuid: device.uuid.clone(),
        pid,
        process_name: if process_name.is_empty() {
            format!("PID {pid}")
        } else {
            process_name
        },
        used_memory: Some(used_memory),
    })
}

fn parse_memory_pair(value: &str) -> Option<(u64, u64)> {
    let open = value.find('(')?;
    let close = value[open + 1..].find(')')? + open + 1;
    let used = parse_byte_value(&value[..open])?;
    let total = parse_byte_value(&value[open + 1..close])?;
    Some((used, total))
}

fn parse_byte_value(value: &str) -> Option<u64> {
    let value = value.trim().trim_end_matches(',');
    let split = value
        .char_indices()
        .find(|(_, character)| character.is_ascii_alphabetic())
        .map(|(index, _)| index)?;
    let number = parse_number(&value[..split])?.max(0.0);
    let unit = value[split..].trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "b" => 1.0,
        "kb" | "kib" => 1024.0,
        "mb" | "mib" => 1024.0 * 1024.0,
        "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * multiplier).round() as u64)
}

fn header_value(payload: &str, label: &str) -> Option<String> {
    let start = payload.find(label)? + label.len();
    payload[start..]
        .split_whitespace()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("N/A"))
        .map(str::to_string)
}

fn parse_number(value: &str) -> Option<f64> {
    value.trim().parse::<f64>().ok()
}

fn looks_like_pci_address(value: &str) -> bool {
    value.contains(':') && value.contains('.')
}

fn reports_no_devices(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("no device")
        || message.contains("no gpu")
        || message.contains("no running processes found") && !message.contains("mtt ")
}

fn empty_snapshot(status: ProviderStatus) -> ProviderSnapshot {
    ProviderSnapshot {
        status,
        devices: Vec::new(),
        processes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_version_2_multi_gpu_overview_and_processes() {
        let output = r#"===MTHREADS_STATUS===
available
===MTHREADS_DATA===
Mon Jul 17 07:49:07 2023
---------------------------------------------------------------
    mthreads-gmi:2.0.0           Driver Version:3.0.0
---------------------------------------------------------------
ID   Name           |PCIe                |%GPU  Mem
     Device Type    |Pcie Lane Width     |Temp  MPC Capable
+-------------------------------------------------------------+
0    MTT S2000      |00000000:03:00.0    |0%    4MiB(16384MiB)
     Physical       |8x(8x)              |51C   NO
ID   Name           |PCIe                |%GPU  Mem
     Device Type    |Pcie Lane Width     |Temp  MPC Capable
+-------------------------------------------------------------+
1    MTT S2000      |00000000:04:00.0    |75%   8192MiB(16384MiB)
     Physical       |8x(8x)              |50C   NO
---------------------------------------------------------------
Processes:
ID   PID       Process name                         GPU Memory Usage
+-------------------------------------------------------------+
1    4242      python worker.py                     2048MiB
===MTHREADS_QUERY_EXIT===
0
===GPU_NPU_SAMPLE_END==="#;

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices.len(), 2);
        assert_eq!(snapshot.devices[0].provider, GpuProvider::Mthreads);
        assert_eq!(snapshot.devices[0].driver_version.as_deref(), Some("3.0.0"));
        assert_eq!(snapshot.devices[1].utilization_percent, Some(75.0));
        assert_eq!(snapshot.devices[1].memory_used, Some(8192 * 1024 * 1024));
        assert_eq!(snapshot.devices[1].temperature_celsius, Some(50.0));
        assert_eq!(snapshot.processes.len(), 1);
        assert_eq!(snapshot.processes[0].process_name, "python worker.py");
        assert_eq!(snapshot.processes[0].used_memory, Some(2048 * 1024 * 1024));
    }

    #[test]
    fn parses_official_version_1_6_overview_shape() {
        let output = r#"===MTHREADS_STATUS===
available
===MTHREADS_DATA===
mthreads-gmi:1.6.0           Driver Version:N/A
ID   Name           |PCIe                |%GPU  Mem
     Device Type    |Pcie Lane Width     |Temp  MPC Capable
+-------------------------------------------------------------+
0    MTT S2000      |00000000:04:00.0    |0%    4MiB(16384MiB)
     Physical       |8x(8x)              |50C   NO
Processes:
   No running processes found
===MTHREADS_QUERY_EXIT===
0
===GPU_NPU_SAMPLE_END==="#;

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].driver_version, None);
        assert!(snapshot.processes.is_empty());
    }

    #[test]
    fn accepts_process_memory_with_a_separate_unit_column() {
        let device = GpuDevice {
            provider: GpuProvider::Mthreads,
            index: 0,
            uuid: "MTHREADS-00000000:04:00.0".into(),
            pci_bus_id: "00000000:04:00.0".into(),
            name: "MTT S2000".into(),
            driver_version: None,
            performance_state: None,
            health_status: None,
            utilization_percent: None,
            memory_utilization_percent: None,
            memory_used: None,
            memory_total: None,
            temperature_celsius: None,
            power_draw_watts: None,
            power_limit_watts: None,
            fan_speed_percent: None,
        };
        let devices = BTreeMap::from([(0, device)]);

        let process = parse_process("0 525 render app 512 MiB", &devices)
            .expect("split memory unit should be parsed");

        assert_eq!(process.process_name, "render app");
        assert_eq!(process.used_memory, Some(512 * 1024 * 1024));
    }

    #[test]
    fn distinguishes_unavailable_no_devices_malformed_and_failed_states() {
        let unavailable = parse("===MTHREADS_STATUS===\nunavailable\n===GPU_NPU_SAMPLE_END===");
        let no_devices = parse(
            "===MTHREADS_STATUS===\navailable\n===MTHREADS_DATA===\nNo GPU devices found\n===MTHREADS_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let malformed = parse(
            "===MTHREADS_STATUS===\navailable\n===MTHREADS_DATA===\nunexpected output\n===MTHREADS_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let failed = parse(
            "===MTHREADS_STATUS===\navailable\n===MTHREADS_DATA===\npermission denied\n===MTHREADS_QUERY_EXIT===\n1\n===MTHREADS_ERROR===\npermission denied\n===GPU_NPU_SAMPLE_END===",
        );

        assert_eq!(unavailable.status, ProviderStatus::Unavailable);
        assert_eq!(no_devices.status, ProviderStatus::NoDevices);
        assert!(matches!(malformed.status, ProviderStatus::Error(_)));
        assert_eq!(
            failed.status,
            ProviderStatus::Error("Moore Threads: permission denied".into())
        );
    }
}
