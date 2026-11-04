use super::{GPU_END_MARKER, GpuDevice, GpuSnapshot, GpuSnapshotStatus, section};
use crate::gpu::GpuProvider;

pub(super) fn inventory(output: &str, timestamp_ms: u64) -> GpuSnapshot {
    let windows = section(output, "VIDEO_CONTROLLERS");
    let json = windows.unwrap_or_else(|| {
        output
            .trim_end()
            .trim_end_matches(GPU_END_MARKER)
            .trim_end()
    });
    let parsed = if windows.is_some() && json.is_empty() {
        Some(serde_json::Value::Array(Vec::new()))
    } else {
        serde_json::from_str::<serde_json::Value>(json).ok()
    };
    let rows = parsed.as_ref().and_then(|value| {
        if windows.is_some() {
            Some(
                value
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_else(|| std::slice::from_ref(value)),
            )
        } else {
            value
                .get("SPDisplaysDataType")?
                .as_array()
                .map(Vec::as_slice)
        }
    });
    let devices: Vec<_> = rows
        .into_iter()
        .flatten()
        .enumerate()
        .filter_map(|(index, row)| {
            let name = row
                .get("Name")
                .or_else(|| row.get("sppci_model"))
                .or_else(|| row.get("_name"))?
                .as_str()?;
            Some(GpuDevice {
                provider: GpuProvider::System,
                index: index as u32,
                uuid: row
                    .get("PNPDeviceID")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{index}:{name}")),
                pci_bus_id: String::new(),
                name: name.to_owned(),
                driver_version: row
                    .get("DriverVersion")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
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
            })
        })
        .collect();
    let status = if rows.is_none() {
        GpuSnapshotStatus::Error("Invalid GPU inventory".into())
    } else if devices.is_empty() {
        GpuSnapshotStatus::NoDevices
    } else {
        GpuSnapshotStatus::Available
    };
    GpuSnapshot {
        timestamp_ms,
        status,
        devices,
        processes: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_inventory_identifies_each_adapter_without_inventing_metrics() {
        for (data, name, id) in [
            (
                r#"{"SPDisplaysDataType":[{"sppci_model":"Apple M4"}]}"#,
                "Apple M4",
                "0:Apple M4",
            ),
            (
                "===VIDEO_CONTROLLERS===\n{\"Name\":\"Intel Arc\",\"PNPDeviceID\":\"PCI-1\"}",
                "Intel Arc",
                "PCI-1",
            ),
        ] {
            let snapshot =
                super::super::parse_gpu_snapshot(&format!("{data}\n{GPU_END_MARKER}"), 0);
            assert_eq!(snapshot.status, GpuSnapshotStatus::Available);
            let device = &snapshot.devices[0];
            assert_eq!(
                (&*device.name, &*device.uuid, device.utilization_percent),
                (name, id, None)
            );
        }
    }
}
