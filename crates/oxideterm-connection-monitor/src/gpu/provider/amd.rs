// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! AMD SMI command protocol and provider-neutral JSON parser.

use serde_json::Value;

use super::{ProviderSnapshot, ProviderStatus, first_section_line, sanitized_error, section};
use crate::gpu::{GpuDevice, GpuProcess, GpuProvider};

pub(super) fn sample_command() -> String {
    concat!(
        "echo '===AMD_STATUS==='; ",
        "if ! command -v amd-smi >/dev/null 2>&1; then ",
        "echo unavailable; ",
        "else ",
        "echo available; ",
        // Capability and inventory caches belong to this sampling shell, not the host filesystem.
        "if [ -z \"${oxideterm_amd_metric_flags+x}\" ]; then ",
        "amd_help=$(LC_ALL=C amd-smi metric --help 2>/dev/null); oxideterm_amd_metric_flags=''; ",
        "for flag in --usage --mem-usage --power --temperature --fan; do ",
        "if printf '%s' \"$amd_help\" | grep -q -- \"$flag\"; then oxideterm_amd_metric_flags=\"$oxideterm_amd_metric_flags $flag\"; fi; done; ",
        "oxideterm_amd_limit_flag=''; if LC_ALL=C amd-smi static --help 2>/dev/null | grep -q -- '--limit'; then oxideterm_amd_limit_flag='--limit'; fi; fi; ",
        "if [ -z \"${oxideterm_amd_static+x}\" ] || [ \"${oxideterm_amd_static_tick:-0}\" -ge 30 ]; then ",
        "oxideterm_amd_static=$(LC_ALL=C amd-smi static --asic --bus --driver --vram $oxideterm_amd_limit_flag --json 2>&1); ",
        "oxideterm_amd_static_exit=$?; oxideterm_amd_static_tick=0; fi; ",
        "echo '===AMD_STATIC==='; printf '%s\\n' \"$oxideterm_amd_static\"; ",
        "amd_exit=$oxideterm_amd_static_exit; ",
        "if [ \"$amd_exit\" -eq 0 ] && [ -n \"$oxideterm_amd_metric_flags\" ]; then ",
        "amd_output=$(LC_ALL=C amd-smi metric $oxideterm_amd_metric_flags --json 2>&1); amd_exit=$?; ",
        "echo '===AMD_METRICS==='; printf '%s\\n' \"$amd_output\"; ",
        "if [ \"$amd_exit\" -ne 0 ]; then echo '===AMD_ERROR==='; printf '%s\\n' \"$amd_output\"; unset oxideterm_amd_static; ",
        "else echo '===AMD_PROCESSES==='; LC_ALL=C amd-smi process --general --json 2>/dev/null; fi; ",
        "else echo '===AMD_ERROR==='; ",
        "if [ \"$amd_exit\" -ne 0 ]; then printf '%s\\n' \"$oxideterm_amd_static\"; ",
        "else printf '%s\\n' 'AMD metric selection unavailable'; amd_exit=1; unset oxideterm_amd_metric_flags; fi; ",
        "unset oxideterm_amd_static; fi; ",
        "oxideterm_amd_static_tick=$((${oxideterm_amd_static_tick:-0}+1)); ",
        "echo '===AMD_QUERY_EXIT==='; echo \"$amd_exit\"; ",
        "fi; ",
    )
    .to_string()
}

pub(super) fn parse(output: &str) -> ProviderSnapshot {
    let query_exit =
        first_section_line(output, "AMD_QUERY_EXIT").and_then(|value| value.parse::<i32>().ok());
    let status_value = first_section_line(output, "AMD_STATUS");
    if status_value == Some("unavailable") {
        return empty_snapshot(ProviderStatus::Unavailable);
    }
    if status_value != Some("available") {
        return empty_snapshot(ProviderStatus::Unknown);
    }
    if query_exit.is_some_and(|exit| exit != 0) {
        let message = sanitized_error(output, "AMD_ERROR", "AMD amd-smi query failed");
        if message.to_ascii_lowercase().contains("no gpu")
            || message.to_ascii_lowercase().contains("no devices")
        {
            return empty_snapshot(ProviderStatus::NoDevices);
        }
        return empty_snapshot(ProviderStatus::Error(format!("AMD: {message}")));
    }

    parse_selected_snapshot(output)
}

fn gpu_records(value: &Value) -> Option<&Vec<Value>> {
    value.get("gpu_data").unwrap_or(value).as_array()
}

fn parse_selected_snapshot(output: &str) -> ProviderSnapshot {
    let inventory = section(output, "AMD_STATIC").and_then(|text| parse_json_payload(text).ok());
    let Some(records) = inventory.as_ref().and_then(gpu_records) else {
        return empty_snapshot(ProviderStatus::Error("AMD: invalid static GPU data".into()));
    };
    let mut devices: Vec<_> = records
        .iter()
        .filter_map(|value| {
            let index = u32::try_from(value.get("gpu").and_then(integer_value)?).ok()?;
            let pci_bus_id = value
                .pointer("/bus/bdf")
                .and_then(text_value)
                .unwrap_or_else(|| format!("AMD:{index}"));
            Some(GpuDevice {
                provider: GpuProvider::Amd,
                index,
                uuid: amd_device_id(index, &pci_bus_id),
                pci_bus_id,
                name: value
                    .pointer("/asic/market_name")
                    .and_then(text_value)
                    .unwrap_or_else(|| format!("AMD GPU {index}")),
                driver_version: value.pointer("/driver/version").and_then(text_value),
                memory_total: value
                    .pointer("/vram/size")
                    .and_then(|v| bytes_value(v, ByteUnit::Mibibytes)),
                // ROCm 7 split power caps by PPT sensor; ROCm 6 exposes the socket cap directly.
                power_limit_watts: value
                    .pointer("/limit/ppt0/socket_power_limit")
                    .or_else(|| value.pointer("/limit/socket_power"))
                    .and_then(watts_value),
                performance_state: None,
                health_status: None,
                utilization_percent: None,
                memory_utilization_percent: None,
                memory_used: None,
                temperature_celsius: None,
                power_draw_watts: None,
                fan_speed_percent: None,
            })
        })
        .collect();
    if devices.is_empty() {
        return empty_snapshot(if records.is_empty() {
            ProviderStatus::NoDevices
        } else {
            ProviderStatus::Error("AMD: missing GPU identities".into())
        });
    }
    let metrics = section(output, "AMD_METRICS").and_then(|text| parse_json_payload(text).ok());
    let Some(metrics) = metrics.as_ref().and_then(gpu_records) else {
        return empty_snapshot(ProviderStatus::Error("AMD: invalid GPU metrics".into()));
    };
    for metric in metrics {
        let Some(device) = devices
            .iter_mut()
            .find(|d| Some(u64::from(d.index)) == metric.get("gpu").and_then(integer_value))
        else {
            continue;
        };
        device.utilization_percent = metric.pointer("/usage/gfx_activity").and_then(number_value);
        device.memory_utilization_percent =
            metric.pointer("/usage/umc_activity").and_then(number_value);
        device.memory_used = metric
            .pointer("/mem_usage/used_vram")
            .and_then(|v| bytes_value(v, ByteUnit::Mibibytes));
        device.memory_total = metric
            .pointer("/mem_usage/total_vram")
            .and_then(|v| bytes_value(v, ByteUnit::Mibibytes))
            .or(device.memory_total);
        device.temperature_celsius = metric
            .pointer("/temperature/hotspot")
            .and_then(number_value);
        device.power_draw_watts = metric.pointer("/power/socket_power").and_then(watts_value);
        device.fan_speed_percent = metric.pointer("/fan/usage").and_then(number_value);
    }
    let process_data =
        section(output, "AMD_PROCESSES").and_then(|text| parse_json_payload(text).ok());
    let mut processes = Vec::new();
    for gpu in process_data
        .as_ref()
        .and_then(gpu_records)
        .into_iter()
        .flatten()
    {
        let Some(device) = devices
            .iter()
            .find(|d| Some(u64::from(d.index)) == gpu.get("gpu").and_then(integer_value))
        else {
            continue;
        };
        for process in gpu
            .get("process_list")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(info) = process.get("process_info") else {
                continue;
            };
            let Some(pid) = info
                .get("pid")
                .and_then(integer_value)
                .and_then(|id| u32::try_from(id).ok())
            else {
                continue;
            };
            processes.push(GpuProcess {
                provider: GpuProvider::Amd,
                gpu_uuid: device.uuid.clone(),
                pid,
                process_name: info
                    .get("name")
                    .and_then(text_value)
                    .unwrap_or_else(|| format!("PID {pid}")),
                used_memory: info
                    .get("mem_usage")
                    .and_then(|v| bytes_value(v, ByteUnit::Bytes)),
            });
        }
    }
    devices.sort_by_key(|device| device.index);
    processes.sort_by(|a, b| a.gpu_uuid.cmp(&b.gpu_uuid).then(a.pid.cmp(&b.pid)));
    ProviderSnapshot {
        status: ProviderStatus::Available,
        devices,
        processes,
    }
}

fn parse_json_payload(payload: &str) -> Result<Value, serde_json::Error> {
    match serde_json::from_str(payload) {
        Ok(value) => Ok(value),
        Err(original_error) => {
            // Some AMD SMI installations emit a group or permission warning
            // next to valid JSON. Try every plausible JSON envelope so a
            // leading "[WARNING]" token cannot hide the actual payload.
            for (start, opening) in payload.char_indices() {
                let closing = match opening {
                    '{' => '}',
                    '[' => ']',
                    _ => continue,
                };
                let Some(relative_end) = payload[start..].rfind(closing) else {
                    continue;
                };
                let end = start + relative_end;
                if let Ok(value) = serde_json::from_str(&payload[start..=end]) {
                    return Ok(value);
                }
            }
            Err(original_error)
        }
    }
}

fn amd_device_id(index: u32, pci_bus_id: &str) -> String {
    if pci_bus_id.starts_with("AMD:") {
        format!("AMD-GPU-{index}")
    } else {
        format!("AMD-{pci_bus_id}")
    }
}

fn integer_value(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
}

fn text_value(value: &Value) -> Option<String> {
    let value = value
        .as_str()
        .map(str::trim)
        .filter(|value| !is_unavailable(value))?;
    Some(value.to_string())
}

fn number_value(value: &Value) -> Option<f64> {
    if let Some(number) = value.as_f64() {
        return Some(number);
    }
    if let Some(inner) = value.get("value") {
        return number_value(inner);
    }
    let text = value.as_str()?.trim();
    if is_unavailable(text) {
        return None;
    }
    text.split_whitespace().next()?.parse::<f64>().ok()
}

fn watts_value(value: &Value) -> Option<f64> {
    let number = number_value(value)?;
    let unit = value
        .get("unit")
        .and_then(Value::as_str)
        .or_else(|| value.as_str()?.split_whitespace().nth(1));
    match unit.map(|unit| unit.to_ascii_lowercase()) {
        Some(unit) if unit == "mw" => Some(number / 1_000.0),
        Some(unit) if unit == "uw" || unit == "µw" => Some(number / 1_000_000.0),
        _ => Some(number),
    }
}

#[derive(Clone, Copy)]
enum ByteUnit {
    Bytes,
    Mibibytes,
}

fn bytes_value(value: &Value, default_unit: ByteUnit) -> Option<u64> {
    let number = number_value(value)?.max(0.0);
    let unit = value
        .get("unit")
        .and_then(Value::as_str)
        .or_else(|| value.as_str()?.split_whitespace().nth(1));
    let multiplier = match unit.map(|unit| unit.to_ascii_lowercase()) {
        Some(unit) if matches!(unit.as_str(), "b" | "bytes") => 1.0,
        Some(unit) if matches!(unit.as_str(), "kb" | "kib") => 1024.0,
        Some(unit) if matches!(unit.as_str(), "mb" | "mib") => 1024.0 * 1024.0,
        Some(unit) if matches!(unit.as_str(), "gb" | "gib") => 1024.0 * 1024.0 * 1024.0,
        Some(unit) if matches!(unit.as_str(), "tb" | "tib") => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        Some(_) | None if matches!(default_unit, ByteUnit::Bytes) => 1.0,
        Some(_) | None => 1024.0 * 1024.0,
    };
    Some((number * multiplier).round().min(u64::MAX as f64) as u64)
}

fn is_unavailable(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "" | "n/a" | "[n/a]" | "not supported" | "not available"
    )
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
    fn parses_selected_rocm_6_and_7_metrics_by_gpu_id() {
        let inventory = r#"[{"gpu":1,"asic":{"market_name":"AMD Radeon PRO W7900"},"bus":{"bdf":"0000:02:00.0"},"driver":{"version":"6.14.4"},"vram":{"size":{"value":48,"unit":"GiB"}},"limit":{"ppt0":{"socket_power_limit":{"value":295,"unit":"W"}}}},
            {"gpu":0,"asic":{"market_name":"AMD Instinct MI300A"},"bus":{"bdf":"0000:01:00.0"},"driver":{"version":"6.14.4"},"vram":{"size":96432},"limit":{"socket_power":550}}]"#;
        let metrics = r#"[{"gpu":0,"usage":{"gfx_activity":45,"umc_activity":12},"mem_usage":{"used_vram":1024,"total_vram":96432},"temperature":{"hotspot":67},"power":{"socket_power":110},"fan":{"usage":20}},
            {"gpu":1,"usage":{"gfx_activity":{"value":9,"unit":"%"},"umc_activity":3},"mem_usage":{"used_vram":{"value":2147483648,"unit":"B"}},"temperature":{"hotspot":{"value":51,"unit":"C"}},"power":{"socket_power":{"value":87500,"unit":"mW"}},"fan":{"usage":"N/A"}}]"#;
        let processes = r#"[{"gpu":1,"process_list":[{"process_info":{"pid":99,"name":"N/A","mem_usage":{"value":512,"unit":"MiB"}}}]},
            {"gpu":0,"process_list":[{"process_info":{"pid":12345,"name":"python3","mem_usage":{"value":4096,"unit":"B"}}},{"process_info":"No running processes detected"}]}]"#;
        for wrapped in [false, true] {
            // ROCm 6 emits an array; ROCm 7 nests the same records under gpu_data.
            let payload = |data: &str| {
                if wrapped {
                    format!(r#"{{"gpu_data":{data}}}"#)
                } else {
                    data.into()
                }
            };
            let output = format!(
                "===AMD_STATUS===\navailable\n===AMD_STATIC===\n[WARNING] render group\n{}\n===AMD_METRICS===\n{}\n===AMD_PROCESSES===\n{}\n===AMD_QUERY_EXIT===\n0\n",
                payload(inventory),
                payload(metrics),
                payload(processes)
            );
            let snapshot = parse(&output);
            assert_eq!(snapshot.status, ProviderStatus::Available);
            assert_eq!(snapshot.devices.len(), 2);
            let first = &snapshot.devices[0];
            assert_eq!(first.index, 0);
            assert_eq!(first.name, "AMD Instinct MI300A");
            assert_eq!(first.driver_version.as_deref(), Some("6.14.4"));
            assert_eq!(first.memory_used, Some(1_073_741_824));
            assert_eq!(first.memory_total, Some(101_116_280_832));
            assert_eq!(first.utilization_percent, Some(45.0));
            assert_eq!(first.memory_utilization_percent, Some(12.0));
            assert_eq!(first.temperature_celsius, Some(67.0));
            assert_eq!(first.power_draw_watts, Some(110.0));
            assert_eq!(first.power_limit_watts, Some(550.0));
            assert_eq!(first.fan_speed_percent, Some(20.0));
            let second = &snapshot.devices[1];
            assert_eq!(second.uuid, "AMD-0000:02:00.0");
            assert_eq!(second.memory_used, Some(2_147_483_648));
            assert_eq!(second.memory_total, Some(51_539_607_552));
            assert_eq!(second.power_draw_watts, Some(87.5));
            assert_eq!(second.power_limit_watts, Some(295.0));
            assert_eq!(second.fan_speed_percent, None);
            assert_eq!(
                snapshot
                    .processes
                    .iter()
                    .map(|p| (
                        p.gpu_uuid.as_str(),
                        p.pid,
                        p.process_name.as_str(),
                        p.used_memory
                    ))
                    .collect::<Vec<_>>(),
                vec![
                    ("AMD-0000:01:00.0", 12345, "python3", Some(4096)),
                    ("AMD-0000:02:00.0", 99, "PID 99", Some(536_870_912))
                ]
            );
        }
    }

    #[test]
    fn distinguishes_unavailable_empty_invalid_and_failed_states() {
        for (body, expected) in [
            ("===AMD_STATUS===\nunavailable", ProviderStatus::Unavailable),
            (
                "===AMD_STATUS===\navailable\n===AMD_STATIC===\n[]\n===AMD_QUERY_EXIT===\n0",
                ProviderStatus::NoDevices,
            ),
            (
                "===AMD_STATUS===\navailable\n===AMD_STATIC===\nnot-json\n===AMD_QUERY_EXIT===\n0",
                ProviderStatus::Error("AMD: invalid static GPU data".into()),
            ),
            (
                "===AMD_STATUS===\navailable\n===AMD_STATIC===\n[{\"gpu\":0}]\n===AMD_METRICS===\nnot-json\n===AMD_QUERY_EXIT===\n0",
                ProviderStatus::Error("AMD: invalid GPU metrics".into()),
            ),
            (
                "===AMD_STATUS===\navailable\n===AMD_QUERY_EXIT===\n1\n===AMD_ERROR===\npermission denied",
                ProviderStatus::Error("AMD: permission denied".into()),
            ),
            (
                "===AMD_STATUS===\navailable\n===AMD_QUERY_EXIT===\n1\n===AMD_ERROR===\nNo GPUs detected",
                ProviderStatus::NoDevices,
            ),
        ] {
            assert_eq!(parse(body).status, expected, "{body}");
        }
    }
}
