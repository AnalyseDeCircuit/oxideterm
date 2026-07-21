// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuProvider {
    Nvidia,
    Amd,
    Hygon,
    Ascend,
    Cambricon,
    Intel,
    Mthreads,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "message")]
pub enum GpuSnapshotStatus {
    Available,
    NoDevices,
    Unavailable,
    Unsupported,
    Error(String),
    #[default]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuDevice {
    pub provider: GpuProvider,
    pub index: u32,
    pub uuid: String,
    pub pci_bus_id: String,
    pub name: String,
    pub driver_version: Option<String>,
    pub performance_state: Option<String>,
    #[serde(default)]
    pub health_status: Option<String>,
    pub utilization_percent: Option<f64>,
    pub memory_utilization_percent: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub temperature_celsius: Option<f64>,
    pub power_draw_watts: Option<f64>,
    pub power_limit_watts: Option<f64>,
    pub fan_speed_percent: Option<f64>,
}

impl GpuDevice {
    pub fn memory_percent(&self) -> Option<f64> {
        let used = self.memory_used?;
        let total = self.memory_total?;
        (total > 0).then_some((used as f64 / total as f64) * 100.0)
    }
}

pub fn gpu_device_row_signature(device: &GpuDevice, process_count: usize, expanded: bool) -> u64 {
    let mut hasher = DefaultHasher::new();
    device.provider.hash(&mut hasher);
    device.uuid.hash(&mut hasher);
    device.index.hash(&mut hasher);
    device.name.hash(&mut hasher);
    process_count.hash(&mut hasher);
    expanded.hash(&mut hasher);
    hasher.finish()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuProcess {
    pub provider: GpuProvider,
    pub gpu_uuid: String,
    pub pid: u32,
    pub process_name: String,
    pub used_memory: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuSnapshot {
    pub timestamp_ms: u64,
    pub status: GpuSnapshotStatus,
    pub devices: Vec<GpuDevice>,
    pub processes: Vec<GpuProcess>,
}

impl GpuSnapshot {
    pub fn summary(&self) -> GpuSummary {
        let memory_used = self.devices.iter().filter_map(|gpu| gpu.memory_used).sum();
        let memory_total = self.devices.iter().filter_map(|gpu| gpu.memory_total).sum();
        let utilization_values = self
            .devices
            .iter()
            .filter_map(|gpu| gpu.utilization_percent)
            .collect::<Vec<_>>();
        let average_utilization_percent = (!utilization_values.is_empty())
            .then(|| utilization_values.iter().sum::<f64>() / utilization_values.len() as f64);
        let maximum_utilization_percent = utilization_values.iter().copied().reduce(f64::max);
        let maximum_temperature_celsius = self
            .devices
            .iter()
            .filter_map(|gpu| gpu.temperature_celsius)
            .reduce(f64::max);
        let power_draw_watts = self
            .devices
            .iter()
            .filter_map(|gpu| gpu.power_draw_watts)
            .reduce(|left, right| left + right);

        GpuSummary {
            device_count: self.devices.len(),
            memory_used,
            memory_total,
            average_utilization_percent,
            maximum_utilization_percent,
            maximum_temperature_celsius,
            power_draw_watts,
        }
    }

    pub fn processes_for(&self, device: &GpuDevice) -> impl Iterator<Item = &GpuProcess> {
        self.processes
            .iter()
            // NVIDIA MIG process UUIDs include the parent physical GPU UUID.
            // AMD identifiers use a vendor prefix and only match exactly.
            .filter(move |process| {
                process.provider == device.provider
                    && (process.gpu_uuid == device.uuid
                        || (device.provider == GpuProvider::Nvidia
                            && process.gpu_uuid.contains(&device.uuid)))
            })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuSummary {
    pub device_count: usize,
    pub memory_used: u64,
    pub memory_total: u64,
    pub average_utilization_percent: Option<f64>,
    pub maximum_utilization_percent: Option<f64>,
    pub maximum_temperature_celsius: Option<f64>,
    pub power_draw_watts: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GpuUpdate {
    pub connection_id: String,
    pub snapshot: GpuSnapshot,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(index: u32, utilization: f64, used: u64, total: u64) -> GpuDevice {
        GpuDevice {
            provider: GpuProvider::Nvidia,
            index,
            uuid: format!("GPU-{index}"),
            pci_bus_id: format!("00000000:{index:02x}:00.0"),
            name: "NVIDIA Test GPU".into(),
            driver_version: Some("555.1".into()),
            performance_state: Some("P0".into()),
            health_status: None,
            utilization_percent: Some(utilization),
            memory_utilization_percent: None,
            memory_used: Some(used),
            memory_total: Some(total),
            temperature_celsius: Some(60.0 + index as f64),
            power_draw_watts: Some(100.0 + index as f64),
            power_limit_watts: Some(300.0),
            fan_speed_percent: Some(40.0),
        }
    }

    #[test]
    fn summarizes_multiple_devices() {
        let snapshot = GpuSnapshot {
            timestamp_ms: 1,
            status: GpuSnapshotStatus::Available,
            devices: vec![device(0, 20.0, 100, 1_000), device(1, 80.0, 300, 1_000)],
            processes: Vec::new(),
        };

        let summary = snapshot.summary();

        assert_eq!(summary.device_count, 2);
        assert_eq!(summary.memory_used, 400);
        assert_eq!(summary.memory_total, 2_000);
        assert_eq!(summary.average_utilization_percent, Some(50.0));
        assert_eq!(summary.maximum_utilization_percent, Some(80.0));
        assert_eq!(summary.maximum_temperature_celsius, Some(61.0));
        assert_eq!(summary.power_draw_watts, Some(201.0));
    }

    #[test]
    fn maps_mig_processes_to_their_physical_gpu() {
        let snapshot = GpuSnapshot {
            timestamp_ms: 1,
            status: GpuSnapshotStatus::Available,
            devices: vec![device(0, 20.0, 100, 1_000)],
            processes: vec![GpuProcess {
                provider: GpuProvider::Nvidia,
                gpu_uuid: "MIG-GPU-0/1/0".into(),
                pid: 42,
                process_name: "python".into(),
                used_memory: Some(100),
            }],
        };

        assert_eq!(snapshot.processes_for(&snapshot.devices[0]).count(), 1);
    }

    #[test]
    fn row_signature_ignores_live_metrics_but_tracks_layout_changes() {
        let original = device(0, 20.0, 100, 1_000);
        let mut updated = original.clone();
        updated.utilization_percent = Some(95.0);
        updated.memory_used = Some(900);
        updated.temperature_celsius = Some(88.0);
        updated.health_status = Some("Warning".into());

        assert_eq!(
            gpu_device_row_signature(&original, 1, false),
            gpu_device_row_signature(&updated, 1, false)
        );
        assert_ne!(
            gpu_device_row_signature(&original, 1, false),
            gpu_device_row_signature(&original, 1, true)
        );
        assert_ne!(
            gpu_device_row_signature(&original, 1, true),
            gpu_device_row_signature(&original, 2, true)
        );
    }
}
