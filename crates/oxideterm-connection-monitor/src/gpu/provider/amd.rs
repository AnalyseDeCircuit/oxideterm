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
        "echo '===AMD_DATA==='; ",
        "amd_output=$(LC_ALL=C amd-smi --json 2>&1); ",
        "amd_exit=$?; printf '%s\\n' \"$amd_output\"; ",
        "echo '===AMD_QUERY_EXIT==='; echo \"$amd_exit\"; ",
        "if [ \"$amd_exit\" -ne 0 ]; then ",
        "echo '===AMD_ERROR==='; printf '%s\\n' \"$amd_output\"; ",
        "fi; ",
        "fi; "
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
        if message.to_ascii_lowercase().contains("no gpu") {
            return empty_snapshot(ProviderStatus::NoDevices);
        }
        return empty_snapshot(ProviderStatus::Error(format!("AMD: {message}")));
    }

    let Some(payload) = section(output, "AMD_DATA") else {
        return empty_snapshot(ProviderStatus::NoDevices);
    };
    let normalized_payload = payload.to_ascii_lowercase();
    if normalized_payload.contains("no gpu") || normalized_payload.contains("no devices") {
        return empty_snapshot(ProviderStatus::NoDevices);
    }
    let json = match parse_json_payload(payload) {
        Ok(json) => json,
        Err(error) => {
            return empty_snapshot(ProviderStatus::Error(format!(
                "AMD: invalid amd-smi JSON: {error}"
            )));
        }
    };
    let root = default_payload(&json);
    let driver_version = root
        .get("version_info")
        .and_then(|value| value.get("amdgpu version"))
        .and_then(text_value);
    let mut devices = root
        .get("gpu_info_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| parse_device(value, driver_version.as_deref()))
        .collect::<Vec<_>>();
    devices.sort_by_key(|device| device.index);
    let mut processes = root
        .get("processes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| parse_process(value, &devices))
        .collect::<Vec<_>>();
    processes.sort_by(|left, right| {
        left.gpu_uuid
            .cmp(&right.gpu_uuid)
            .then_with(|| left.pid.cmp(&right.pid))
    });

    ProviderSnapshot {
        status: if devices.is_empty() {
            ProviderStatus::NoDevices
        } else {
            ProviderStatus::Available
        },
        devices,
        processes,
    }
}

fn parse_device(value: &Value, driver_version: Option<&str>) -> Option<GpuDevice> {
    let index = value.get("gpu_id").and_then(integer_value)? as u32;
    let pci_bus_id = value
        .get("bdf")
        .and_then(text_value)
        .unwrap_or_else(|| format!("AMD:{index}"));
    let name = value
        .get("market_name")
        .and_then(text_value)
        .unwrap_or_else(|| format!("AMD GPU {index}"));
    let uuid = amd_device_id(index, &pci_bus_id);

    Some(GpuDevice {
        provider: GpuProvider::Amd,
        index,
        uuid,
        pci_bus_id,
        name,
        driver_version: driver_version.map(str::to_string),
        performance_state: None,
        health_status: None,
        utilization_percent: value.get("gfx_util").and_then(number_value),
        memory_utilization_percent: value.get("mem_util").and_then(number_value),
        memory_used: value
            .get("mem_usage")
            .and_then(|memory| memory.get("used_vram"))
            .and_then(|memory| bytes_value(memory, ByteUnit::Mibibytes)),
        memory_total: value
            .get("mem_usage")
            .and_then(|memory| memory.get("total_vram"))
            .and_then(|memory| bytes_value(memory, ByteUnit::Mibibytes)),
        temperature_celsius: value.get("temp").and_then(number_value),
        power_draw_watts: value
            .get("power_usage")
            .and_then(|power| power.get("current_power"))
            .and_then(watts_value),
        power_limit_watts: value
            .get("power_usage")
            .and_then(|power| power.get("power_limit"))
            .and_then(watts_value),
        fan_speed_percent: value.get("fan").and_then(number_value),
    })
}

fn parse_process(value: &Value, devices: &[GpuDevice]) -> Option<GpuProcess> {
    let gpu_index = value.get("gpu").and_then(integer_value)? as u32;
    let pid = value.get("pid").and_then(integer_value)? as u32;
    let gpu_uuid = devices
        .iter()
        .find(|device| device.index == gpu_index)
        .map(|device| device.uuid.clone())?;
    let process_name = value
        .get("name")
        .and_then(text_value)
        .unwrap_or_else(|| format!("PID {pid}"));

    Some(GpuProcess {
        provider: GpuProvider::Amd,
        gpu_uuid,
        pid,
        process_name,
        used_memory: value
            .get("mem_usage")
            .and_then(|memory| bytes_value(memory, ByteUnit::Bytes)),
    })
}

fn default_payload(value: &Value) -> &Value {
    value
        .as_array()
        .and_then(|values| values.first())
        .unwrap_or(value)
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
    fn parses_multiple_amd_gpus_metrics_and_processes() {
        let output = r#"===AMD_STATUS===
available
===AMD_DATA===
[{"version_info":{"amdgpu version":"6.14.4"},"gpu_info_list":[{"gpu_id":0,"market_name":"AMD Instinct MI300A","bdf":"0000:01:00.0","mem_util":12,"gfx_util":45,"temp":67,"power_usage":{"current_power":110,"power_limit":550},"mem_usage":{"used_vram":1024,"total_vram":96432},"fan":20},{"gpu_id":1,"market_name":"AMD Radeon PRO W7900","bdf":"0000:02:00.0","mem_util":{"value":3,"unit":"%"},"gfx_util":{"value":9,"unit":"%"},"temp":{"value":51,"unit":"C"},"power_usage":{"current_power":{"value":87500,"unit":"mW"},"power_limit":{"value":295,"unit":"W"}},"mem_usage":{"used_vram":{"value":2,"unit":"GiB"},"total_vram":{"value":48,"unit":"GiB"}},"fan":"N/A"}],"processes":[{"gpu":0,"pid":12345,"name":"python3","mem_usage":"4.13 GB"},{"gpu":1,"pid":99,"name":"N/A","mem_usage":{"value":512,"unit":"MiB"}}]}]
===AMD_QUERY_EXIT===
0
===GPU_NPU_SAMPLE_END==="#;

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices.len(), 2);
        assert_eq!(snapshot.devices[0].provider, GpuProvider::Amd);
        assert_eq!(snapshot.devices[0].memory_used, Some(1024 * 1024 * 1024));
        assert_eq!(snapshot.devices[1].power_draw_watts, Some(87.5));
        assert_eq!(
            snapshot.devices[1].memory_total,
            Some(48 * 1024 * 1024 * 1024)
        );
        assert_eq!(snapshot.processes.len(), 2);
        assert_eq!(snapshot.processes[0].process_name, "python3");
        assert_eq!(snapshot.processes[1].process_name, "PID 99");
    }

    #[test]
    fn distinguishes_unavailable_empty_invalid_and_failed_states() {
        let unavailable = parse("===AMD_STATUS===\nunavailable\n===GPU_NPU_SAMPLE_END===");
        let empty = parse(
            "===AMD_STATUS===\navailable\n===AMD_DATA===\n[{\"gpu_info_list\":[],\"processes\":[]}]\n===AMD_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let invalid = parse(
            "===AMD_STATUS===\navailable\n===AMD_DATA===\nnot-json\n===AMD_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let failed = parse(
            "===AMD_STATUS===\navailable\n===AMD_DATA===\npermission denied\n===AMD_QUERY_EXIT===\n1\n===AMD_ERROR===\npermission denied\n===GPU_NPU_SAMPLE_END===",
        );

        assert_eq!(unavailable.status, ProviderStatus::Unavailable);
        assert_eq!(empty.status, ProviderStatus::NoDevices);
        assert!(matches!(invalid.status, ProviderStatus::Error(_)));
        assert_eq!(
            failed.status,
            ProviderStatus::Error("AMD: permission denied".into())
        );
    }

    #[test]
    fn accepts_valid_json_surrounded_by_tool_warnings() {
        let output = concat!(
            "===AMD_STATUS===\navailable\n",
            "===AMD_DATA===\nwarning: render group is recommended\n",
            "[{\"gpu_info_list\":[{\"gpu_id\":0,\"market_name\":\"AMD Test GPU\",\"bdf\":\"0000:01:00.0\"}],\"processes\":[]}]\n",
            "warning: partial process names\n",
            "===AMD_QUERY_EXIT===\n0\n",
            "===GPU_NPU_SAMPLE_END==="
        );

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices.len(), 1);
    }
}
