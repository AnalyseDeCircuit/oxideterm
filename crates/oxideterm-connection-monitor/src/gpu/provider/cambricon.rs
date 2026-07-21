// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Cambricon MLU sampling protocol and `cnmon info` key-value parser.

use super::{ProviderSnapshot, ProviderStatus, first_section_line, sanitized_error, section};
use crate::gpu::{GpuDevice, GpuProcess, GpuProvider};

pub(super) fn sample_command() -> String {
    concat!(
        "echo '===CAMBRICON_STATUS==='; ",
        "if ! command -v cnmon >/dev/null 2>&1; then ",
        "echo unavailable; ",
        "else ",
        "echo available; ",
        "echo '===CAMBRICON_DATA==='; ",
        "cambricon_output=$(LC_ALL=C cnmon info 2>&1); ",
        "cambricon_exit=$?; printf '%s\\n' \"$cambricon_output\"; ",
        "echo '===CAMBRICON_QUERY_EXIT==='; echo \"$cambricon_exit\"; ",
        "if [ \"$cambricon_exit\" -ne 0 ]; then ",
        "echo '===CAMBRICON_ERROR==='; printf '%s\\n' \"$cambricon_output\"; ",
        "fi; ",
        "fi; "
    )
    .to_string()
}

pub(super) fn parse(output: &str) -> ProviderSnapshot {
    match first_section_line(output, "CAMBRICON_STATUS") {
        Some("unavailable") => return empty_snapshot(ProviderStatus::Unavailable),
        Some("available") => {}
        _ => return empty_snapshot(ProviderStatus::Unknown),
    }

    let query_exit = first_section_line(output, "CAMBRICON_QUERY_EXIT")
        .and_then(|value| value.parse::<i32>().ok());
    if query_exit.is_some_and(|exit| exit != 0) {
        let message = sanitized_error(output, "CAMBRICON_ERROR", "cnmon info query failed");
        if reports_no_devices(&message) {
            return empty_snapshot(ProviderStatus::NoDevices);
        }
        return empty_snapshot(ProviderStatus::Error(format!("Cambricon: {message}")));
    }

    let Some(payload) = section(output, "CAMBRICON_DATA") else {
        return empty_snapshot(ProviderStatus::NoDevices);
    };
    if reports_no_devices(payload) {
        return empty_snapshot(ProviderStatus::NoDevices);
    }

    let parsed = parse_cards(payload);
    if parsed.devices.is_empty() {
        let status = if payload.trim().is_empty() {
            ProviderStatus::NoDevices
        } else {
            ProviderStatus::Error("Cambricon: unrecognized cnmon info output".into())
        };
        return empty_snapshot(status);
    }

    ProviderSnapshot {
        status: ProviderStatus::Available,
        devices: parsed.devices,
        processes: parsed.processes,
    }
}

struct ParsedCards {
    devices: Vec<GpuDevice>,
    processes: Vec<GpuProcess>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DetailSection {
    General,
    Health,
    Utilization,
    Temperature,
    PhysicalMemory,
    Power,
    Pci,
    Processes,
    Other,
}

#[derive(Default)]
struct PendingProcess {
    pid: Option<u32>,
    name: Option<String>,
    used_memory: Option<u64>,
}

fn parse_cards(payload: &str) -> ParsedCards {
    let mut blocks = Vec::<(u32, Vec<&str>)>::new();
    for line in payload.lines() {
        if let Some(index) = card_index(line) {
            blocks.push((index, Vec::new()));
        } else if let Some((_, lines)) = blocks.last_mut() {
            lines.push(line);
        }
    }

    let mut devices = Vec::new();
    let mut processes = Vec::new();
    for (index, lines) in blocks {
        if let Some((device, mut card_processes)) = parse_card(index, &lines) {
            devices.push(device);
            processes.append(&mut card_processes);
        }
    }
    devices.sort_by_key(|device| device.index);
    processes.sort_by(|left, right| {
        left.gpu_uuid
            .cmp(&right.gpu_uuid)
            .then_with(|| left.pid.cmp(&right.pid))
    });
    ParsedCards { devices, processes }
}

fn parse_card(index: u32, lines: &[&str]) -> Option<(GpuDevice, Vec<GpuProcess>)> {
    let mut name = None;
    let mut reported_uuid = None;
    let mut driver_version = None;
    let mut health_status = None;
    let mut utilization_percent = None;
    let mut memory_used = None;
    let mut memory_total = None;
    let mut maximum_temperature = None::<f64>;
    let mut power_draw_watts = None;
    let mut power_limit_watts = None;
    let mut pci_domain = None;
    let mut pci_bus = None;
    let mut pci_device = None;
    let mut pci_function = None;
    let mut section = DetailSection::General;
    let mut pending_process = None::<PendingProcess>;
    let mut raw_processes = Vec::new();

    for line in lines {
        let Some((raw_key, raw_value)) = line.split_once(':') else {
            continue;
        };
        let key = raw_key.trim();
        let value = raw_value.trim();

        section = match key {
            "Health State" => DetailSection::Health,
            "Utilization" => DetailSection::Utilization,
            "Temperature" => DetailSection::Temperature,
            "Physical Memory Usage" => DetailSection::PhysicalMemory,
            "Power" => DetailSection::Power,
            "PCI" => DetailSection::Pci,
            "Processes" => DetailSection::Processes,
            "Virtual Memory Usage"
            | "Device System Memory Usage"
            | "Fast Alloc Memory"
            | "Frequency"
            | "Channel Memory Usage"
            | "CRC Err Count"
            | "Retired Pages"
            | "Row-Remapping"
            | "Chassis" => DetailSection::Other,
            _ => section,
        };

        match (section, key) {
            (_, "Product Name") => name = optional_text(value),
            (_, "UUID") => reported_uuid = optional_text(value),
            (_, "Driver") if section != DetailSection::Health => {
                driver_version = optional_text(value)
            }
            (DetailSection::Health, "Device") => health_status = optional_text(value),
            (DetailSection::Utilization, "MLU Average") => utilization_percent = percentage(value),
            (DetailSection::Temperature, "Board" | "Chip" | "Memory") => {
                if let Some(temperature) = number_with_unit(value, 'c') {
                    maximum_temperature = Some(
                        maximum_temperature
                            .map(|current| current.max(temperature))
                            .unwrap_or(temperature),
                    );
                }
            }
            (DetailSection::PhysicalMemory, "Used") => memory_used = byte_value(value),
            (DetailSection::PhysicalMemory, "Total") => memory_total = byte_value(value),
            (DetailSection::Power, "Usage") => power_draw_watts = number_with_unit(value, 'w'),
            (DetailSection::Power, "Cap") => power_limit_watts = number_with_unit(value, 'w'),
            (DetailSection::Pci, "Domain ID") => pci_domain = hexadecimal_component(value, 4),
            (DetailSection::Pci, "Bus num") => pci_bus = hexadecimal_component(value, 2),
            (DetailSection::Pci, "Device") => pci_device = hexadecimal_component(value, 2),
            (DetailSection::Pci, "Function") => pci_function = hexadecimal_component(value, 1),
            (DetailSection::Processes, "Process") => {
                if let Some(process) = pending_process.take() {
                    raw_processes.push(process);
                }
                pending_process = Some(PendingProcess::default());
            }
            (DetailSection::Processes, "PID") => {
                pending_process
                    .get_or_insert_with(PendingProcess::default)
                    .pid = value.parse::<u32>().ok()
            }
            (DetailSection::Processes, "cmdline") => {
                pending_process
                    .get_or_insert_with(PendingProcess::default)
                    .name = optional_text(value)
            }
            (DetailSection::Processes, "MLU Memory Usage") => {
                pending_process
                    .get_or_insert_with(PendingProcess::default)
                    .used_memory = byte_value(value)
            }
            _ => {}
        }
    }
    if let Some(process) = pending_process {
        raw_processes.push(process);
    }

    // A Card block without a product or any telemetry is not a device record.
    if name.is_none()
        && utilization_percent.is_none()
        && memory_total.is_none()
        && maximum_temperature.is_none()
    {
        return None;
    }
    let pci_bus_id = match (pci_domain, pci_bus, pci_device, pci_function) {
        (Some(domain), Some(bus), Some(device), Some(function)) => {
            format!("{domain}:{bus}:{device}.{function}")
        }
        _ => format!("CAMBRICON:{index}"),
    };
    let uuid = reported_uuid.unwrap_or_else(|| {
        if pci_bus_id.starts_with("CAMBRICON:") {
            format!("CAMBRICON-MLU-{index}")
        } else {
            format!("CAMBRICON-{pci_bus_id}")
        }
    });
    let processes = raw_processes
        .into_iter()
        .filter_map(|process| {
            let pid = process.pid?;
            Some(GpuProcess {
                provider: GpuProvider::Cambricon,
                gpu_uuid: uuid.clone(),
                pid,
                process_name: process.name.unwrap_or_else(|| format!("PID {pid}")),
                used_memory: process.used_memory,
            })
        })
        .collect::<Vec<_>>();
    let memory_utilization_percent = match (memory_used, memory_total) {
        (Some(used), Some(total)) if total > 0 => Some((used as f64 / total as f64) * 100.0),
        _ => None,
    };

    Some((
        GpuDevice {
            provider: GpuProvider::Cambricon,
            index,
            uuid,
            pci_bus_id,
            name: name.unwrap_or_else(|| format!("Cambricon MLU {index}")),
            driver_version,
            performance_state: None,
            health_status,
            utilization_percent,
            memory_utilization_percent,
            memory_used,
            memory_total,
            temperature_celsius: maximum_temperature,
            power_draw_watts,
            power_limit_watts,
            fan_speed_percent: None,
        },
        processes,
    ))
}

fn card_index(line: &str) -> Option<u32> {
    let mut fields = line.split_whitespace();
    fields
        .next()?
        .eq_ignore_ascii_case("card")
        .then(|| fields.next()?.trim_end_matches(':').parse::<u32>().ok())
        .flatten()
}

fn percentage(value: &str) -> Option<f64> {
    value
        .split_whitespace()
        .next()?
        .trim_end_matches('%')
        .parse::<f64>()
        .ok()
}

fn number_with_unit(value: &str, unit: char) -> Option<f64> {
    let mut fields = value.split_whitespace();
    let number = fields.next()?.parse::<f64>().ok()?;
    let reported_unit = fields.next()?.chars().next()?;
    reported_unit.eq_ignore_ascii_case(&unit).then_some(number)
}

fn byte_value(value: &str) -> Option<u64> {
    let mut fields = value.split_whitespace();
    let number = fields.next()?.parse::<f64>().ok()?.max(0.0);
    let multiplier = match fields.next()?.to_ascii_lowercase().as_str() {
        "b" => 1.0,
        "kb" | "kib" => 1024.0,
        "mb" | "mib" => 1024.0 * 1024.0,
        "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * multiplier).round().min(u64::MAX as f64) as u64)
}

fn hexadecimal_component(value: &str, width: usize) -> Option<String> {
    let value = value.trim().trim_start_matches("0x");
    u32::from_str_radix(value, 16)
        .ok()
        .map(|component| format!("{component:0width$X}"))
}

fn optional_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "n/a" | "na" | "unknown" | "not available"
        ))
    .then(|| value.to_string())
}

fn reports_no_devices(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("no mlu")
        || value.contains("no device")
        || value.contains("device not found")
        || value.contains("there is no device")
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
    fn parses_cnmon_5_multi_card_metrics_health_and_processes() {
        let output = r#"===CAMBRICON_STATUS===
available
===CAMBRICON_DATA===
================CNMON v5.10.22================
Card 0
    Product Name                   : MLU370-X4
    UUID                           : 77013006-2457-0000-0000-000000000000
    Driver                         : v5.10.22
    Health State                   :
        Device                     : Good
        Driver                     : Running
    Utilization                    :
        MLU Average                : 66 %
    Temperature                    :
        Board                      : 64 C
        Chip                       : 78 C
        Memory                     : 63 C
    Physical Memory Usage          :
        Total                      : 23308 MiB
        Used                       : 17320 MiB
        Free                       : 5988 MiB
    Power                          :
        Usage                      : 44 W
        Cap                        : 150 W
    PCI                            :
        Domain ID                  : 0000
        Bus num                    : 01
        Device                     : 00
        Function                   : 0
    Processes                      :
        Process                    : 0
            PID                    : 101934
            cmdline                : /torch/venv3/pytorch_infer/bin/python
            MLU Memory Usage       : 15829 MiB
Card 1
    Product Name                   : MLU370-X4
    Driver                         : v5.10.22
    Health State                   :
        Device                     : Good
    Utilization                    :
        MLU Average                : 5 %
    Temperature                    :
        Chip                       : 51 C
    Physical Memory Usage          :
        Total                      : 23308 MiB
        Used                       : 1024 MiB
    Power                          :
        Usage                      : 30 W
        Cap                        : 150 W
    Processes                      :
===CAMBRICON_QUERY_EXIT===
0
===GPU_NPU_SAMPLE_END==="#;

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices.len(), 2);
        assert_eq!(snapshot.devices[0].provider, GpuProvider::Cambricon);
        assert_eq!(snapshot.devices[0].name, "MLU370-X4");
        assert_eq!(snapshot.devices[0].health_status.as_deref(), Some("Good"));
        assert_eq!(snapshot.devices[0].utilization_percent, Some(66.0));
        assert_eq!(snapshot.devices[0].memory_used, Some(17_320 * 1024 * 1024));
        assert_eq!(snapshot.devices[0].temperature_celsius, Some(78.0));
        assert_eq!(snapshot.devices[0].power_draw_watts, Some(44.0));
        assert_eq!(snapshot.devices[0].pci_bus_id, "0000:01:00.0");
        assert_eq!(snapshot.processes.len(), 1);
        assert_eq!(snapshot.processes[0].pid, 101934);
        assert_eq!(
            snapshot.processes[0].used_memory,
            Some(15_829 * 1024 * 1024)
        );
        assert_eq!(snapshot.devices[1].uuid, "CAMBRICON-MLU-1");
    }

    #[test]
    fn parses_cnmon_4_detail_shape_without_optional_health_or_pci() {
        let output = r#"===CAMBRICON_STATUS===
available
===CAMBRICON_DATA===
================CNMON v4.20.18================
Card 0
    Product Name                   : MLU370-S4
    Driver                         : v4.20.18
    Utilization                    :
        MLU Average                : 66 %
    Temperature                    :
        Board                      : 49 C
    Physical Memory Usage          :
        Total                      : 23308 MiB
        Used                       : 4096 MiB
    Power                          :
        Usage                      : 38 W
===CAMBRICON_QUERY_EXIT===
0
===GPU_NPU_SAMPLE_END==="#;

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices.len(), 1);
        assert_eq!(snapshot.devices[0].name, "MLU370-S4");
        assert_eq!(snapshot.devices[0].health_status, None);
        assert_eq!(snapshot.devices[0].pci_bus_id, "CAMBRICON:0");
        assert_eq!(
            snapshot.devices[0].driver_version.as_deref(),
            Some("v4.20.18")
        );
    }

    #[test]
    fn distinguishes_unavailable_no_devices_malformed_and_failed_states() {
        let unavailable = parse("===CAMBRICON_STATUS===\nunavailable\n===GPU_NPU_SAMPLE_END===");
        let no_devices = parse(
            "===CAMBRICON_STATUS===\navailable\n===CAMBRICON_DATA===\nNo MLU devices found\n===CAMBRICON_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let malformed = parse(
            "===CAMBRICON_STATUS===\navailable\n===CAMBRICON_DATA===\nunexpected output\n===CAMBRICON_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let failed = parse(
            "===CAMBRICON_STATUS===\navailable\n===CAMBRICON_DATA===\npermission denied\n===CAMBRICON_QUERY_EXIT===\n1\n===CAMBRICON_ERROR===\npermission denied\n===GPU_NPU_SAMPLE_END===",
        );

        assert_eq!(unavailable.status, ProviderStatus::Unavailable);
        assert_eq!(no_devices.status, ProviderStatus::NoDevices);
        assert!(matches!(malformed.status, ProviderStatus::Error(_)));
        assert_eq!(
            failed.status,
            ProviderStatus::Error("Cambricon: permission denied".into())
        );
    }
}
