// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Intel XPU SMI command protocol and daemonless JSON parser.

use std::collections::BTreeMap;

use serde_json::Value;

use super::{ProviderSnapshot, ProviderStatus, first_section_line, sanitized_error, section};
use crate::gpu::{GpuDevice, GpuProcess, GpuProvider};

const HEALTH_REFRESH_TICKS: u32 = 15;

pub(super) fn sample_command() -> String {
    format!(
        concat!(
            "echo '===INTEL_STATUS==='; ",
            "if ! command -v xpu-smi >/dev/null 2>&1; then ",
            "echo unavailable; ",
            "else ",
            "echo available; ",
            "if [ -z \"${{oxideterm_intel_discovery_cache+x}}\" ]; then ",
            "oxideterm_intel_discovery_cache=$(LC_ALL=C xpu-smi discovery -j 2>&1); ",
            "oxideterm_intel_exit=$?; ",
            "if [ \"$oxideterm_intel_exit\" -eq 0 ]; then ",
            "oxideterm_intel_ids=$(printf '%s\\n' \"$oxideterm_intel_discovery_cache\" | sed -n 's/.*\"device_id\"[[:space:]]*:[[:space:]]*\\([0-9][0-9]*\\).*/\\1/p'); ",
            "oxideterm_intel_details_cache=$(for intel_id in $oxideterm_intel_ids; do LC_ALL=C xpu-smi discovery -d \"$intel_id\" -j 2>/dev/null || true; done); ",
            "oxideterm_intel_health_cache=$(LC_ALL=C xpu-smi health -l -j 2>/dev/null || true); ",
            "oxideterm_intel_health_tick=0; ",
            "else unset oxideterm_intel_discovery_cache; fi; ",
            "fi; ",
            "echo '===INTEL_DISCOVERY==='; printf '%s\\n' \"$oxideterm_intel_discovery_cache\"; ",
            "echo '===INTEL_QUERY_EXIT==='; echo \"$oxideterm_intel_exit\"; ",
            "if [ \"$oxideterm_intel_exit\" -eq 0 ]; then ",
            "echo '===INTEL_DETAILS==='; printf '%s\\n' \"$oxideterm_intel_details_cache\"; ",
            "echo '===INTEL_STATS==='; ",
            "for intel_id in $oxideterm_intel_ids; do LC_ALL=C xpu-smi stats -d \"$intel_id\" -j 2>/dev/null || true; done; ",
            "if [ \"${{oxideterm_intel_health_tick:-0}}\" -ge {health_refresh_ticks} ]; then ",
            "oxideterm_intel_health_cache=$(LC_ALL=C xpu-smi health -l -j 2>/dev/null || true); ",
            "oxideterm_intel_health_tick=0; fi; ",
            "echo '===INTEL_HEALTH==='; printf '%s\\n' \"$oxideterm_intel_health_cache\"; ",
            "oxideterm_intel_health_tick=$((oxideterm_intel_health_tick + 1)); ",
            "echo '===INTEL_PROCESSES==='; LC_ALL=C xpu-smi ps -j 2>/dev/null || true; ",
            "else ",
            "echo '===INTEL_ERROR==='; printf '%s\\n' \"$oxideterm_intel_discovery_cache\"; ",
            "fi; ",
            "fi; "
        ),
        health_refresh_ticks = HEALTH_REFRESH_TICKS,
    )
}

pub(super) fn parse(output: &str) -> ProviderSnapshot {
    match first_section_line(output, "INTEL_STATUS") {
        Some("unavailable") => return empty_snapshot(ProviderStatus::Unavailable),
        Some("available") => {}
        _ => return empty_snapshot(ProviderStatus::Unknown),
    }

    let query_exit =
        first_section_line(output, "INTEL_QUERY_EXIT").and_then(|value| value.parse::<i32>().ok());
    if query_exit.is_some_and(|exit| exit != 0) {
        let message = sanitized_error(output, "INTEL_ERROR", "xpu-smi discovery failed");
        if reports_no_devices(&message) {
            return empty_snapshot(ProviderStatus::NoDevices);
        }
        return empty_snapshot(ProviderStatus::Error(format!("Intel: {message}")));
    }

    let discovery_values = section(output, "INTEL_DISCOVERY")
        .map(parse_json_stream)
        .unwrap_or_default();
    if discovery_values.is_empty() {
        return empty_snapshot(ProviderStatus::Error(
            "Intel: invalid xpu-smi discovery JSON".into(),
        ));
    }
    if let Some(error) = discovery_values.iter().find_map(json_error) {
        if reports_no_devices(&error) {
            return empty_snapshot(ProviderStatus::NoDevices);
        }
        return empty_snapshot(ProviderStatus::Error(format!("Intel: {error}")));
    }

    let mut devices = BTreeMap::new();
    for value in &discovery_values {
        collect_inventory(value, &mut devices);
    }
    if let Some(payload) = section(output, "INTEL_DETAILS") {
        for value in parse_json_stream(payload) {
            collect_inventory(&value, &mut devices);
        }
    }
    if devices.is_empty() {
        return empty_snapshot(ProviderStatus::NoDevices);
    }

    if let Some(payload) = section(output, "INTEL_STATS") {
        for value in parse_json_stream(payload) {
            apply_statistics(&value, &mut devices);
        }
    }
    if let Some(payload) = section(output, "INTEL_HEALTH") {
        for value in parse_json_stream(payload) {
            apply_health(&value, &mut devices);
        }
    }
    let processes = section(output, "INTEL_PROCESSES")
        .map(parse_json_stream)
        .unwrap_or_default()
        .iter()
        .flat_map(|value| parse_process_records(value, &devices))
        .collect::<Vec<_>>();

    ProviderSnapshot {
        status: ProviderStatus::Available,
        devices: devices.into_values().collect(),
        processes,
    }
}

fn collect_inventory(value: &Value, devices: &mut BTreeMap<u32, GpuDevice>) {
    if let Some(list) = value.get("device_list").and_then(Value::as_array) {
        for device in list {
            collect_inventory(device, devices);
        }
        return;
    }

    let Some(index) = value
        .get("device_id")
        .and_then(integer_value)
        .map(|id| id as u32)
    else {
        return;
    };
    let Some(name) = value.get("device_name").and_then(text_value) else {
        return;
    };
    let existing = devices.get(&index);
    let pci_bus_id = value
        .get("pci_bdf_address")
        .and_then(text_value)
        .or_else(|| existing.map(|device| device.pci_bus_id.clone()))
        .unwrap_or_else(|| format!("INTEL:{index}"));
    let uuid = value
        .get("uuid")
        .and_then(text_value)
        .or_else(|| existing.map(|device| device.uuid.clone()))
        .unwrap_or_else(|| intel_device_id(index, &pci_bus_id));

    devices.insert(
        index,
        GpuDevice {
            provider: GpuProvider::Intel,
            index,
            uuid,
            pci_bus_id,
            name,
            driver_version: value
                .get("driver_version")
                .and_then(text_value)
                .or_else(|| existing.and_then(|device| device.driver_version.clone())),
            performance_state: None,
            health_status: existing.and_then(|device| device.health_status.clone()),
            utilization_percent: existing.and_then(|device| device.utilization_percent),
            memory_utilization_percent: existing
                .and_then(|device| device.memory_utilization_percent),
            memory_used: existing.and_then(|device| device.memory_used),
            memory_total: value
                .get("memory_physical_size_byte")
                .and_then(byte_count)
                .or_else(|| existing.and_then(|device| device.memory_total)),
            temperature_celsius: existing.and_then(|device| device.temperature_celsius),
            power_draw_watts: existing.and_then(|device| device.power_draw_watts),
            power_limit_watts: existing.and_then(|device| device.power_limit_watts),
            fan_speed_percent: existing.and_then(|device| device.fan_speed_percent),
        },
    );
}

fn apply_statistics(value: &Value, devices: &mut BTreeMap<u32, GpuDevice>) {
    if let Some(records) = value.get("datas").and_then(Value::as_array) {
        for record in records {
            apply_statistics(record, devices);
        }
        return;
    }
    let Some(index) = value
        .get("device_id")
        .and_then(integer_value)
        .map(|id| id as u32)
    else {
        return;
    };
    let Some(device) = devices.get_mut(&index) else {
        return;
    };

    device.utilization_percent =
        metric_value(value, "XPUM_STATS_GPU_UTILIZATION", Aggregate::Average);
    device.memory_utilization_percent =
        metric_value(value, "XPUM_STATS_MEMORY_UTILIZATION", Aggregate::Average);
    device.memory_used =
        metric_value(value, "XPUM_STATS_MEMORY_USED", Aggregate::Sum).map(mibibytes);
    device.temperature_celsius =
        metric_value(value, "XPUM_STATS_GPU_CORE_TEMPERATURE", Aggregate::Maximum);
    device.power_draw_watts = metric_value(value, "XPUM_STATS_POWER", Aggregate::Sum);
}

#[derive(Clone, Copy)]
enum Aggregate {
    Average,
    Maximum,
    Sum,
}

fn metric_value(value: &Value, metric_type: &str, aggregate: Aggregate) -> Option<f64> {
    let device_values = metric_values(value.get("device_level"), metric_type);
    if !device_values.is_empty() {
        return aggregate_values(&device_values, aggregate);
    }

    let tile_values = value
        .get("tile_level")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .flat_map(|tile| metric_values(tile.get("data_list"), metric_type))
        .collect::<Vec<_>>();
    aggregate_values(&tile_values, aggregate)
}

fn metric_values(value: Option<&Value>, metric_type: &str) -> Vec<f64> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|metric| metric.get("metrics_type").and_then(Value::as_str) == Some(metric_type))
        .filter_map(|metric| {
            metric
                .get("value")
                .or_else(|| metric.get("avg"))
                .and_then(number_value)
        })
        .collect()
}

fn aggregate_values(values: &[f64], aggregate: Aggregate) -> Option<f64> {
    match aggregate {
        Aggregate::Average => {
            (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
        }
        Aggregate::Maximum => values.iter().copied().reduce(f64::max),
        Aggregate::Sum => (!values.is_empty()).then(|| values.iter().sum()),
    }
}

fn apply_health(value: &Value, devices: &mut BTreeMap<u32, GpuDevice>) {
    if let Some(records) = value.get("device_list").and_then(Value::as_array) {
        for record in records {
            apply_health(record, devices);
        }
        return;
    }
    let Some(index) = value
        .get("device_id")
        .and_then(integer_value)
        .map(|id| id as u32)
    else {
        return;
    };
    let Some(device) = devices.get_mut(&index) else {
        return;
    };

    let statuses = [
        "core_temperature",
        "memory_temperature",
        "power",
        "memory",
        "xe_link_port",
        "frequency",
    ]
    .iter()
    .filter_map(|component| value.get(component))
    .filter_map(|component| component.get("status"))
    .filter_map(text_value)
    .collect::<Vec<_>>();
    device.health_status = aggregate_health(&statuses);
}

fn aggregate_health(statuses: &[String]) -> Option<String> {
    statuses
        .iter()
        .find(|status| !status.eq_ignore_ascii_case("ok"))
        .cloned()
        .or_else(|| (!statuses.is_empty()).then(|| "Ok".to_string()))
}

fn parse_process_records(value: &Value, devices: &BTreeMap<u32, GpuDevice>) -> Vec<GpuProcess> {
    value
        .get("device_util_by_proc_list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|record| {
            let index = record.get("device_id").and_then(integer_value)? as u32;
            let device = devices.get(&index)?;
            let pid = record.get("process_id").and_then(integer_value)? as u32;
            Some(GpuProcess {
                provider: GpuProvider::Intel,
                gpu_uuid: device.uuid.clone(),
                pid,
                process_name: record
                    .get("process_name")
                    .and_then(text_value)
                    .unwrap_or_else(|| format!("PID {pid}")),
                // Intel documents SHR and MEM process values in KiB.
                used_memory: record.get("mem_size").and_then(number_value).map(kibibytes),
            })
        })
        .collect()
}

fn parse_json_stream(payload: &str) -> Vec<Value> {
    let mut values = Vec::new();
    let mut remaining = payload;
    while let Some(start) = remaining.find(['{', '[']) {
        let candidate = &remaining[start..];
        let mut stream = serde_json::Deserializer::from_str(candidate).into_iter::<Value>();
        match stream.next() {
            Some(Ok(value)) => {
                let consumed = stream.byte_offset();
                values.push(value);
                remaining = &candidate[consumed..];
            }
            Some(Err(_)) | None => {
                remaining = &candidate[1..];
            }
        }
    }
    values
}

fn json_error(value: &Value) -> Option<String> {
    value.get("error").and_then(text_value)
}

fn integer_value(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
}

fn number_value(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse::<f64>().ok())
}

fn byte_count(value: &Value) -> Option<u64> {
    if let Some(bytes) = value.as_u64() {
        return Some(bytes);
    }
    let text = value.as_str()?.trim();
    if let Ok(bytes) = text.parse::<u64>() {
        return Some(bytes);
    }
    let number = text.split_whitespace().next()?.parse::<f64>().ok()?;
    let unit = text.split_whitespace().nth(1).unwrap_or("bytes");
    let multiplier = match unit.to_ascii_lowercase().as_str() {
        "kib" | "kb" => 1024.0,
        "mib" | "mb" => 1024.0 * 1024.0,
        "gib" | "gb" => 1024.0 * 1024.0 * 1024.0,
        _ => 1.0,
    };
    Some((number * multiplier).round() as u64)
}

fn text_value(value: &Value) -> Option<String> {
    let text = value.as_str()?.trim();
    (!matches!(
        text.to_ascii_lowercase().as_str(),
        "" | "n/a" | "unknown" | "not supported" | "not available"
    ))
    .then(|| text.to_string())
}

fn intel_device_id(index: u32, pci_bus_id: &str) -> String {
    if pci_bus_id.starts_with("INTEL:") {
        format!("INTEL-GPU-{index}")
    } else {
        format!("INTEL-{pci_bus_id}")
    }
}

fn reports_no_devices(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("no device") || message.contains("device not found")
}

fn mibibytes(value: f64) -> u64 {
    (value.max(0.0) * 1024.0 * 1024.0).round() as u64
}

fn kibibytes(value: f64) -> u64 {
    (value.max(0.0) * 1024.0).round() as u64
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
    fn parses_multiple_intel_devices_metrics_health_and_processes() {
        let output = r#"===INTEL_STATUS===
available
===INTEL_DISCOVERY===
{"device_list":[{"device_id":0,"device_name":"Intel Data Center GPU Max 1550","pci_bdf_address":"0000:4d:00.0","uuid":"intel-0"},{"device_id":1,"device_name":"Intel Data Center GPU Flex 170","pci_bdf_address":"0000:5e:00.0","uuid":"intel-1"}]}
===INTEL_QUERY_EXIT===
0
===INTEL_DETAILS===
{"device_id":0,"device_name":"Intel Data Center GPU Max 1550","pci_bdf_address":"0000:4d:00.0","uuid":"intel-0","driver_version":"1.3.30872","memory_physical_size_byte":68719476736}
{"device_id":1,"device_name":"Intel Data Center GPU Flex 170","pci_bdf_address":"0000:5e:00.0","uuid":"intel-1","driver_version":"1.3.30872","memory_physical_size_byte":"16384 MiB"}
===INTEL_STATS===
{"device_id":0,"device_level":[{"metrics_type":"XPUM_STATS_GPU_UTILIZATION","value":75},{"metrics_type":"XPUM_STATS_MEMORY_USED","value":8192},{"metrics_type":"XPUM_STATS_MEMORY_UTILIZATION","value":12},{"metrics_type":"XPUM_STATS_GPU_CORE_TEMPERATURE","value":68},{"metrics_type":"XPUM_STATS_POWER","value":310}]}
{"device_id":1,"device_level":[],"tile_level":[{"tile_id":0,"data_list":[{"metrics_type":"XPUM_STATS_GPU_UTILIZATION","value":20},{"metrics_type":"XPUM_STATS_MEMORY_USED","value":1024},{"metrics_type":"XPUM_STATS_GPU_CORE_TEMPERATURE","value":51},{"metrics_type":"XPUM_STATS_POWER","value":40}]},{"tile_id":1,"data_list":[{"metrics_type":"XPUM_STATS_GPU_UTILIZATION","value":40},{"metrics_type":"XPUM_STATS_MEMORY_USED","value":2048},{"metrics_type":"XPUM_STATS_GPU_CORE_TEMPERATURE","value":55},{"metrics_type":"XPUM_STATS_POWER","value":45}]}]}
===INTEL_HEALTH===
{"device_list":[{"device_id":0,"core_temperature":{"status":"Ok"},"memory":{"status":"Ok"}},{"device_id":1,"core_temperature":{"status":"Warning"},"memory":{"status":"Ok"}}]}
===INTEL_PROCESSES===
{"device_util_by_proc_list":[{"device_id":0,"process_id":12961,"process_name":"python","shared_mem_size":0,"mem_size":1966}]}
===GPU_NPU_SAMPLE_END==="#;

        let snapshot = parse(output);

        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(snapshot.devices.len(), 2);
        assert_eq!(snapshot.devices[0].provider, GpuProvider::Intel);
        assert_eq!(snapshot.devices[0].memory_total, Some(68_719_476_736));
        assert_eq!(snapshot.devices[0].memory_used, Some(8192 * 1024 * 1024));
        assert_eq!(snapshot.devices[1].utilization_percent, Some(30.0));
        assert_eq!(snapshot.devices[1].temperature_celsius, Some(55.0));
        assert_eq!(snapshot.devices[1].power_draw_watts, Some(85.0));
        assert_eq!(
            snapshot.devices[1].health_status.as_deref(),
            Some("Warning")
        );
        assert_eq!(snapshot.processes.len(), 1);
        assert_eq!(snapshot.processes[0].used_memory, Some(1966 * 1024));
    }

    #[test]
    fn distinguishes_unavailable_empty_invalid_and_failed_states() {
        let unavailable = parse("===INTEL_STATUS===\nunavailable\n===GPU_NPU_SAMPLE_END===");
        let empty = parse(
            "===INTEL_STATUS===\navailable\n===INTEL_DISCOVERY===\n{\"device_list\":[]}\n===INTEL_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let invalid = parse(
            "===INTEL_STATUS===\navailable\n===INTEL_DISCOVERY===\nnot-json\n===INTEL_QUERY_EXIT===\n0\n===GPU_NPU_SAMPLE_END===",
        );
        let failed = parse(
            "===INTEL_STATUS===\navailable\n===INTEL_DISCOVERY===\npermission denied\n===INTEL_QUERY_EXIT===\n1\n===INTEL_ERROR===\npermission denied\n===GPU_NPU_SAMPLE_END===",
        );

        assert_eq!(unavailable.status, ProviderStatus::Unavailable);
        assert_eq!(empty.status, ProviderStatus::NoDevices);
        assert!(matches!(invalid.status, ProviderStatus::Error(_)));
        assert_eq!(
            failed.status,
            ProviderStatus::Error("Intel: permission denied".into())
        );
    }

    #[test]
    fn extracts_json_after_tool_warnings() {
        let values =
            parse_json_stream("warning: partial telemetry\n{\"device_id\":0}\n{\"device_id\":1}\n");

        assert_eq!(values.len(), 2);
    }
}
