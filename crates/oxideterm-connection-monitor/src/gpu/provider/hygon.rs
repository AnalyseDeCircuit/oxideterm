// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Hygon DCU sampling protocol and version-tolerant `hy-smi` parser.

use std::collections::BTreeMap;

use super::{ProviderSnapshot, ProviderStatus, first_section_line, sanitized_error, section};
use crate::gpu::{GpuDevice, GpuProcess, GpuProvider};

pub(super) fn sample_command() -> String {
    concat!(
        "echo '===HYGON_STATUS==='; ",
        "hygon_tool=''; hygon_cached_output=''; hygon_cached_exit=''; ",
        "if command -v hy-smi >/dev/null 2>&1; then ",
        "hygon_tool='hy-smi'; ",
        "elif command -v rocm-smi >/dev/null 2>&1; then ",
        "hygon_cached_output=$(LC_ALL=C rocm-smi 2>&1); hygon_cached_exit=$?; ",
        "if printf '%s\\n' \"$hygon_cached_output\" | grep -Eq '(^|[[:space:]])DCU([[:space:]]|$).*DCU%'; then hygon_tool='rocm-smi'; fi; ",
        "fi; ",
        "if [ -z \"$hygon_tool\" ]; then ",
        "echo unavailable; ",
        "else ",
        "echo available; ",
        "echo '===HYGON_DATA==='; ",
        "if [ -n \"$hygon_cached_exit\" ]; then hygon_output=$hygon_cached_output; hygon_exit=$hygon_cached_exit; ",
        "else hygon_output=$(LC_ALL=C \"$hygon_tool\" 2>&1); hygon_exit=$?; fi; ",
        "printf '%s\\n' \"$hygon_output\"; ",
        "echo '===HYGON_QUERY_EXIT==='; echo \"$hygon_exit\"; ",
        "if [ \"$hygon_exit\" -eq 0 ]; then ",
        "echo '===HYGON_PRODUCTS==='; LC_ALL=C \"$hygon_tool\" --showproductname 2>/dev/null || true; ",
        "echo '===HYGON_DRIVER==='; LC_ALL=C \"$hygon_tool\" --showdriverversion 2>/dev/null || true; ",
        "else ",
        "echo '===HYGON_ERROR==='; printf '%s\\n' \"$hygon_output\"; ",
        "fi; ",
        "fi; "
    )
    .to_string()
}

pub(super) fn parse(output: &str) -> ProviderSnapshot {
    match first_section_line(output, "HYGON_STATUS") {
        Some("unavailable") => return empty_snapshot(ProviderStatus::Unavailable),
        Some("available") => {}
        _ => return empty_snapshot(ProviderStatus::Unknown),
    }

    let query_exit =
        first_section_line(output, "HYGON_QUERY_EXIT").and_then(|value| value.parse::<i32>().ok());
    if query_exit.is_some_and(|exit| exit != 0) {
        let message = sanitized_error(output, "HYGON_ERROR", "hy-smi query failed");
        if reports_no_devices(&message) {
            return empty_snapshot(ProviderStatus::NoDevices);
        }
        return empty_snapshot(ProviderStatus::Error(format!("Hygon: {message}")));
    }

    let Some(payload) = section(output, "HYGON_DATA") else {
        return empty_snapshot(ProviderStatus::NoDevices);
    };
    if reports_no_devices(payload) {
        return empty_snapshot(ProviderStatus::NoDevices);
    }

    let products = section(output, "HYGON_PRODUCTS")
        .map(parse_product_names)
        .unwrap_or_default();
    let driver_version = section(output, "HYGON_DRIVER").and_then(parse_driver_version);
    let devices = parse_devices(payload, &products, driver_version.as_deref());
    if devices.is_empty() {
        let status = if payload.trim().is_empty() || looks_like_empty_table(payload) {
            ProviderStatus::NoDevices
        } else {
            ProviderStatus::Error("Hygon: unrecognized hy-smi output".into())
        };
        return empty_snapshot(status);
    }

    let mut processes = parse_processes(payload, &devices);
    processes.sort_by(|left, right| {
        left.gpu_uuid
            .cmp(&right.gpu_uuid)
            .then_with(|| left.pid.cmp(&right.pid))
    });

    ProviderSnapshot {
        status: ProviderStatus::Available,
        devices: devices.into_values().collect(),
        processes,
    }
}

fn parse_devices(
    payload: &str,
    products: &BTreeMap<u32, String>,
    driver_version: Option<&str>,
) -> BTreeMap<u32, GpuDevice> {
    let mut devices = parse_concise_table(payload, products, driver_version);
    parse_hsmi_table(payload, products, driver_version, &mut devices);
    devices
}

fn parse_concise_table(
    payload: &str,
    products: &BTreeMap<u32, String>,
    driver_version: Option<&str>,
) -> BTreeMap<u32, GpuDevice> {
    let lines = payload.lines().collect::<Vec<_>>();
    let Some((header_index, headers)) = lines.iter().enumerate().find_map(|(index, line)| {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        let lower = fields
            .iter()
            .map(|field| field.to_ascii_lowercase())
            .collect::<Vec<_>>();
        (lower.iter().any(|field| field == "dcu")
            && lower.iter().any(|field| field.starts_with("temp"))
            && lower.iter().any(|field| field == "dcu%"))
        .then_some((index, lower))
    }) else {
        return BTreeMap::new();
    };

    let mut devices = BTreeMap::new();
    for line in lines.into_iter().skip(header_index + 1) {
        let values = line.split_whitespace().collect::<Vec<_>>();
        let Some(index) = values.first().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        if values.len() < headers.len() {
            continue;
        }

        let value = |names: &[&str]| -> Option<&str> {
            let position = headers
                .iter()
                .position(|header| names.iter().any(|name| header == name))?;
            values.get(position).copied()
        };
        let name = products
            .get(&index)
            .cloned()
            .unwrap_or_else(|| format!("Hygon DCU {index}"));
        devices.insert(
            index,
            GpuDevice {
                provider: GpuProvider::Hygon,
                index,
                uuid: format!("HYGON-DCU-{index}"),
                pci_bus_id: format!("HYGON:{index}"),
                name,
                driver_version: driver_version.map(str::to_string),
                performance_state: value(&["perf"]).and_then(optional_text),
                health_status: None,
                utilization_percent: value(&["dcu%"])
                    .and_then(|value| number_with_suffix(value, '%')),
                memory_utilization_percent: value(&["vram%"])
                    .and_then(|value| number_with_suffix(value, '%')),
                memory_used: None,
                memory_total: None,
                temperature_celsius: value(&["temp"])
                    .and_then(|value| number_with_suffix_case_insensitive(value, 'c')),
                power_draw_watts: value(&["avgpwr"])
                    .and_then(|value| number_with_suffix_case_insensitive(value, 'w')),
                power_limit_watts: value(&["pwrcap"])
                    .and_then(|value| number_with_suffix_case_insensitive(value, 'w')),
                fan_speed_percent: value(&["fan"]).and_then(|value| number_with_suffix(value, '%')),
            },
        );
    }
    devices
}

fn parse_hsmi_table(
    payload: &str,
    products: &BTreeMap<u32, String>,
    driver_version: Option<&str>,
    devices: &mut BTreeMap<u32, GpuDevice>,
) {
    let mut pending_index = None;
    for line in payload.lines().map(str::trim) {
        if !line.starts_with('|') {
            continue;
        }
        let columns = line
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect::<Vec<_>>();
        if columns.len() < 3 {
            continue;
        }

        let mut identity = columns[0].split_whitespace();
        if let Some(index) = identity.next().and_then(|value| value.parse::<u32>().ok()) {
            let inline_name = identity.collect::<Vec<_>>().join(" ");
            let name = products
                .get(&index)
                .cloned()
                .or_else(|| optional_text(&inline_name))
                .unwrap_or_else(|| format!("Hygon DCU {index}"));
            let memory = parse_memory_pair(columns[2]);
            let telemetry = columns[1].split_whitespace().collect::<Vec<_>>();
            devices.insert(
                index,
                GpuDevice {
                    provider: GpuProvider::Hygon,
                    index,
                    uuid: format!("HYGON-DCU-{index}"),
                    pci_bus_id: format!("HYGON:{index}"),
                    name,
                    driver_version: driver_version.map(str::to_string),
                    performance_state: None,
                    health_status: None,
                    utilization_percent: None,
                    memory_utilization_percent: memory.and_then(|(used, total)| {
                        (total > 0).then_some((used as f64 / total as f64) * 100.0)
                    }),
                    memory_used: memory.map(|(used, _)| used),
                    memory_total: memory.map(|(_, total)| total),
                    temperature_celsius: telemetry
                        .iter()
                        .find_map(|value| number_with_suffix_case_insensitive(value, 'c')),
                    power_draw_watts: telemetry
                        .iter()
                        .find_map(|value| number_with_suffix_case_insensitive(value, 'w')),
                    power_limit_watts: None,
                    fan_speed_percent: None,
                },
            );
            pending_index = Some(index);
            continue;
        }

        let Some(index) = pending_index else {
            continue;
        };
        let Some(device) = devices.get_mut(&index) else {
            continue;
        };
        device.fan_speed_percent = columns[1]
            .split_whitespace()
            .find_map(|value| number_with_suffix(value, '%'));
        device.utilization_percent = columns[2]
            .split_whitespace()
            .find_map(|value| number_with_suffix(value, '%'));
    }
}

fn parse_processes(payload: &str, devices: &BTreeMap<u32, GpuDevice>) -> Vec<GpuProcess> {
    let mut in_processes = false;
    let mut processes = Vec::new();
    for line in payload.lines().map(str::trim) {
        if line.to_ascii_lowercase().contains("processes:") {
            in_processes = true;
            continue;
        }
        if !in_processes || !line.starts_with('|') {
            continue;
        }
        let fields = line
            .trim_matches('|')
            .split_whitespace()
            .collect::<Vec<_>>();
        let Some(index) = fields.first().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let Some((pid_position, pid)) = fields
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(position, value)| value.parse::<u32>().ok().map(|pid| (position, pid)))
        else {
            continue;
        };
        let Some(device) = devices.get(&index) else {
            continue;
        };
        let Some((memory_position, used_memory)) = fields
            .iter()
            .enumerate()
            .rev()
            .find_map(|(position, value)| parse_byte_value(value).map(|bytes| (position, bytes)))
        else {
            continue;
        };
        let mut name_start = pid_position + 1;
        if fields.get(name_start).is_some_and(|value| value.len() == 1) {
            name_start += 1;
        }
        let process_name = fields[name_start..memory_position].join(" ");
        processes.push(GpuProcess {
            provider: GpuProvider::Hygon,
            gpu_uuid: device.uuid.clone(),
            pid,
            process_name: optional_text(&process_name).unwrap_or_else(|| format!("PID {pid}")),
            used_memory: Some(used_memory),
        });
    }
    processes
}

fn parse_product_names(payload: &str) -> BTreeMap<u32, String> {
    let mut products = BTreeMap::new();
    for line in payload.lines() {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("card series") && !lower.contains("product name") {
            continue;
        }
        let Some((_, value)) = line.rsplit_once(':') else {
            continue;
        };
        let Some(index) = accelerator_index(line) else {
            continue;
        };
        if let Some(name) = optional_text(value) {
            products.insert(index, name);
        }
    }
    products
}

fn accelerator_index(value: &str) -> Option<u32> {
    let normalized = value.replace(['[', ']', ':', '='], " ");
    let fields = normalized.split_whitespace().collect::<Vec<_>>();
    fields.windows(2).find_map(|pair| {
        matches!(
            pair[0].to_ascii_lowercase().as_str(),
            "hcu" | "dcu" | "gpu" | "card"
        )
        .then(|| pair[1].parse::<u32>().ok())
        .flatten()
    })
}

fn parse_driver_version(payload: &str) -> Option<String> {
    payload.lines().find_map(|line| {
        let value = line
            .split_once(':')
            .map(|(_, value)| value)
            .unwrap_or(line)
            .trim();
        optional_text(value)
            .filter(|value| value.chars().any(|character| character.is_ascii_digit()))
    })
}

fn parse_memory_pair(value: &str) -> Option<(u64, u64)> {
    let (used, total) = value.split_once('/')?;
    Some((parse_byte_value(used)?, parse_byte_value(total)?))
}

fn parse_byte_value(value: &str) -> Option<u64> {
    let value = value.trim().trim_end_matches('|');
    let unit_start = value
        .char_indices()
        .find(|(_, character)| character.is_ascii_alphabetic())?
        .0;
    let number = value[..unit_start].trim().parse::<f64>().ok()?.max(0.0);
    let multiplier = match value[unit_start..].trim().to_ascii_lowercase().as_str() {
        "b" => 1.0,
        "kb" | "kib" => 1024.0,
        "mb" | "mib" => 1024.0 * 1024.0,
        "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    Some((number * multiplier).round().min(u64::MAX as f64) as u64)
}

fn number_with_suffix(value: &str, suffix: char) -> Option<f64> {
    value
        .trim()
        .strip_suffix(suffix)?
        .trim()
        .parse::<f64>()
        .ok()
}

fn number_with_suffix_case_insensitive(value: &str, suffix: char) -> Option<f64> {
    let value = value.trim();
    let last = value.chars().last()?;
    last.eq_ignore_ascii_case(&suffix)
        .then(|| {
            value[..value.len() - last.len_utf8()]
                .trim()
                .parse::<f64>()
                .ok()
        })
        .flatten()
}

fn optional_text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()
        && !matches!(
            value.to_ascii_lowercase().as_str(),
            "n/a" | "na" | "unknown"
        ))
    .then(|| value.to_string())
}

fn looks_like_empty_table(payload: &str) -> bool {
    payload
        .to_ascii_lowercase()
        .contains("system management interface")
        && !payload.lines().any(|line| {
            line.split_whitespace()
                .next()
                .is_some_and(|value| value.parse::<u32>().is_ok())
        })
}

fn reports_no_devices(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("no dcu")
        || value.contains("no gpu")
        || value.contains("no device")
        || value.contains("device not found")
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

    #[cfg(unix)]
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[cfg(unix)]
    #[test]
    fn rocm_fallback_accepts_dcu_headers_but_ignores_amd_gpu_headers() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after the Unix epoch")
            .as_nanos();
        let tool_dir = std::env::temp_dir().join(format!(
            "oxideterm-hygon-provider-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&tool_dir).expect("temporary tool directory should be created");
        let tool_path = tool_dir.join("rocm-smi");

        write_fake_rocm_smi(
            &tool_path,
            "GPU Temp AvgPwr Perf PwrCap VRAM% GPU% Mode\n0 40.0C 30.0W auto 300.0W 1% 2% Normal",
        );
        let amd_output = run_hygon_command(&tool_dir);
        assert_eq!(
            first_section_line(&amd_output, "HYGON_STATUS"),
            Some("unavailable")
        );

        write_fake_rocm_smi(
            &tool_path,
            "DCU Temp AvgPwr Perf PwrCap VRAM% DCU% Mode\n0 40.0C 30.0W auto 300.0W 1% 2% Normal",
        );
        let hygon_output = run_hygon_command(&tool_dir);
        assert_eq!(parse(&hygon_output).status, ProviderStatus::Available);

        fs::remove_dir_all(&tool_dir).expect("temporary tool directory should be removed");
    }

    #[cfg(unix)]
    fn write_fake_rocm_smi(path: &std::path::Path, overview: &str) {
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  --showproductname) echo 'DCU[0] : Card Series: Test DCU' ;;\n  --showdriverversion) echo 'Driver version: 1.0' ;;\n  *) printf '%s\\n' '{}' ;;\nesac\n",
            overview.replace('\\', "\\\\").replace('\'', "'\\''")
        );
        fs::write(path, script).expect("fake rocm-smi should be written");
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))
            .expect("fake rocm-smi should be executable");
    }

    #[cfg(unix)]
    fn run_hygon_command(tool_dir: &std::path::Path) -> String {
        let output = Command::new("sh")
            .args(["-c", &sample_command()])
            .env("PATH", format!("{}:/usr/bin:/bin", tool_dir.display()))
            .output()
            .expect("provider command should execute");
        assert!(output.status.success());
        String::from_utf8(output.stdout).expect("provider output should be UTF-8")
    }

    #[test]
    fn parses_multi_dcu_concise_layout_and_product_names() {
        let output = r#"===HYGON_STATUS===
available
===HYGON_DATA===
============================ System Management Interface =============================
======================================================================================
DCU     Temp     AvgPwr     Fan     Perf     PwrCap     VRAM%      DCU%      Mode
0       51.0C    75.0W      12%     auto     300.0W     18%        96%       Normal
1       48.0C    42.0W      0%      auto     300.0W     2%         5.5%      Normal
======================================================================================
=================================== End of SMI Log ===================================
===HYGON_QUERY_EXIT===
0
===HYGON_PRODUCTS===
DCU[0] : Card Series: K100_AI
DCU[1] : Card Series: BW1000
===HYGON_DRIVER===
Driver version: 6.3.16
===GPU_NPU_SAMPLE_END==="#;

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices.len(), 2);
        assert_eq!(snapshot.devices[0].provider, GpuProvider::Hygon);
        assert_eq!(snapshot.devices[0].name, "K100_AI");
        assert_eq!(snapshot.devices[0].utilization_percent, Some(96.0));
        assert_eq!(snapshot.devices[0].memory_utilization_percent, Some(18.0));
        assert_eq!(snapshot.devices[0].power_limit_watts, Some(300.0));
        assert_eq!(
            snapshot.devices[0].driver_version.as_deref(),
            Some("6.3.16")
        );
        assert_eq!(snapshot.devices[1].name, "BW1000");
    }

    #[test]
    fn parses_hsmi_memory_layout_and_processes() {
        let output = r#"===HYGON_STATUS===
available
===HYGON_DATA===
=============================== HYGON HSMI ===============================
| GPU                  | Temp      Pwr        | Memory-Usage         |
| Fan                  | Perf     Pwr:GPU/CP  | GPU-Util  Compute M. |
|======================+======================+======================|
| 0  K100-AI           | 45C       120W       | 32MiB / 65536MiB     |
|                      | 0%       300W/0W     | 60%       Default    |
Processes:
| GPU   GI   CI        PID   Type   Process name              GPU Memory |
| 0     N/A  N/A       7894  C      python worker.py          264MiB     |
===HYGON_QUERY_EXIT===
0
===HYGON_PRODUCTS===
===HYGON_DRIVER===
===GPU_NPU_SAMPLE_END==="#;

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices[0].memory_used, Some(32 * 1024 * 1024));
        assert_eq!(snapshot.devices[0].memory_total, Some(65_536 * 1024 * 1024));
        assert_eq!(snapshot.devices[0].utilization_percent, Some(60.0));
        assert_eq!(snapshot.processes.len(), 1);
        assert_eq!(snapshot.processes[0].pid, 7894);
        assert_eq!(snapshot.processes[0].process_name, "python worker.py");
    }

    #[test]
    fn distinguishes_unavailable_no_devices_malformed_and_failed_states() {
        let unavailable = parse("===HYGON_STATUS===\nunavailable\n===GPU_NPU_SAMPLE_END===");
        let no_devices = parse(
            "===HYGON_STATUS===\navailable\n===HYGON_DATA===\nNo DCU devices found\n===HYGON_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let malformed = parse(
            "===HYGON_STATUS===\navailable\n===HYGON_DATA===\nunexpected output\n===HYGON_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let failed = parse(
            "===HYGON_STATUS===\navailable\n===HYGON_DATA===\npermission denied\n===HYGON_QUERY_EXIT===\n1\n===HYGON_ERROR===\npermission denied\n===GPU_NPU_SAMPLE_END===",
        );

        assert_eq!(unavailable.status, ProviderStatus::Unavailable);
        assert_eq!(no_devices.status, ProviderStatus::NoDevices);
        assert!(matches!(malformed.status, ProviderStatus::Error(_)));
        assert_eq!(
            failed.status,
            ProviderStatus::Error("Hygon: permission denied".into())
        );
    }
}
