//! Unprivileged Apple GPU counters exposed by the IOAccelerator registry.

use super::{ProviderSnapshot, ProviderStatus, first_section_line, section};
use crate::gpu::{GpuDevice, GpuProvider};

pub(super) fn sample_command() -> &'static str {
    concat!(
        "echo '===APPLE_STATUS==='; ",
        "if [ ! -x /usr/sbin/ioreg ]; then echo unavailable; else ",
        "oxide_gpu=$(/usr/sbin/ioreg -r -c IOAccelerator -d 1 -l -w 0 2>/dev/null); ",
        "if [ $? -ne 0 ]; then echo error; else echo available; ",
        "echo '===APPLE_GPUS==='; ",
        // Only transmit device identities and counters, not arbitrary registry properties.
        "printf '%s\\n' \"$oxide_gpu\" | /usr/bin/awk '/^\\+-o / || /\"model\" = / || /\"PerformanceStatistics\" = /'; ",
        "fi; fi; "
    )
}

pub(super) fn parse(output: &str) -> ProviderSnapshot {
    let mut devices = Vec::new();
    if let Some(body) = section(output, "APPLE_GPUS") {
        for entry in body.split("+-o ").skip(1) {
            let Some(name) = entry
                .lines()
                .find_map(|line| {
                    line.trim()
                        .strip_prefix("\"model\" = ")
                        .and_then(|value| serde_json::from_str::<String>(value).ok())
                })
                .filter(|name| name.starts_with("Apple "))
            else {
                continue;
            };
            let Some(id) = entry
                .lines()
                .next()
                .and_then(|line| line.split("id ").nth(1))
                .and_then(|tail| tail.split(',').next())
            else {
                continue;
            };
            let counters = entry
                .lines()
                .find_map(|line| line.trim().strip_prefix("\"PerformanceStatistics\" = "))
                .unwrap_or_default();
            devices.push(GpuDevice {
                provider: GpuProvider::Apple,
                index: devices.len() as u32,
                uuid: format!("ioreg:{id}"),
                pci_bus_id: String::new(),
                name,
                driver_version: None,
                performance_state: None,
                health_status: None,
                utilization_percent: counter(counters, "Device Utilization %")
                    .filter(|value| (0.0..=100.0).contains(value)),
                memory_utilization_percent: None,
                // Unified memory has no dedicated VRAM capacity. Allocated memory is
                // not the same counter as memory currently in use by the GPU.
                memory_used: integer_counter(counters, "In use system memory"),
                memory_total: None,
                temperature_celsius: None,
                power_draw_watts: None,
                power_limit_watts: None,
                fan_speed_percent: None,
            });
        }
    }
    let status = match first_section_line(output, "APPLE_STATUS") {
        Some("unavailable") => ProviderStatus::Unavailable,
        Some("error") => ProviderStatus::Error("Apple GPU registry query failed".into()),
        Some("available") if devices.is_empty() => ProviderStatus::NoDevices,
        Some("available") => ProviderStatus::Available,
        _ => ProviderStatus::Unknown,
    };
    ProviderSnapshot {
        status,
        devices,
        processes: Vec::new(),
    }
}

fn counter_text<'a>(counters: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("\"{key}\"=");
    counters
        .trim_matches(['{', '}'])
        .split(',')
        .find_map(|field| field.trim().strip_prefix(&prefix))
}

fn counter(counters: &str, key: &str) -> Option<f64> {
    counter_text(counters, key)?
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

fn integer_counter(counters: &str, key: &str) -> Option<u64> {
    counter_text(counters, key)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_registry_devices_without_inventing_missing_metrics() {
        let output = concat!(
            "===APPLE_STATUS===\navailable\n===APPLE_GPUS===\n",
            "+-o AGXAccelerator <class AGXAccelerator, id 0x100, registered>\n",
            "  \"PerformanceStatistics\" = {\"Alloc system memory\"=9999,\"Device Utilization %\"=29,\"In use system memory\"=1148141568}\n",
            "  \"model\" = \"Apple M5\"\n",
            "+-o AGXAccelerator <class AGXAccelerator, id 0x200, registered>\n",
            "  \"model\" = \"Apple Test\"\n",
            "  \"PerformanceStatistics\" = {\"Device Utilization %\"=NaN}\n",
            "===GPU_NPU_SAMPLE_END===\n"
        );
        let snapshot = parse(output);
        assert_eq!(snapshot.status, ProviderStatus::Available);
        assert_eq!(
            snapshot
                .devices
                .iter()
                .map(|device| (
                    device.name.as_str(),
                    device.uuid.as_str(),
                    device.utilization_percent,
                    device.memory_used,
                    device.memory_total,
                    device.power_draw_watts,
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    "Apple M5",
                    "ioreg:0x100",
                    Some(29.0),
                    Some(1148141568),
                    None,
                    None
                ),
                ("Apple Test", "ioreg:0x200", None, None, None, None),
            ]
        );
        assert_eq!(
            parse("===APPLE_STATUS===\nerror\n").status,
            ProviderStatus::Error("Apple GPU registry query failed".into())
        );
        assert_eq!(
            parse("===APPLE_STATUS===\navailable\n===APPLE_GPUS===\n").status,
            ProviderStatus::NoDevices
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn native_registry_probe_returns_a_complete_snapshot() {
        let command = crate::gpu::build_gpu_sample_command("Darwin");
        let output = std::process::Command::new("/bin/sh")
            .args(["-c", &command])
            .output()
            .unwrap();
        assert!(output.status.success());
        let output = String::from_utf8(output.stdout).unwrap();
        assert!(
            output
                .lines()
                .any(|line| line == crate::gpu::GPU_END_MARKER)
        );
        let snapshot = crate::gpu::parse_gpu_snapshot(&output, 1);
        assert!(matches!(
            snapshot.status,
            crate::gpu::GpuSnapshotStatus::Available | crate::gpu::GpuSnapshotStatus::NoDevices
        ));
        #[cfg(target_arch = "aarch64")]
        {
            let model = std::process::Command::new("/usr/sbin/sysctl")
                .args(["-n", "machdep.cpu.brand_string"])
                .output()
                .unwrap();
            let model = String::from_utf8(model.stdout).unwrap();
            assert_eq!(
                snapshot
                    .devices
                    .iter()
                    .map(|device| device.name.as_str())
                    .collect::<Vec<_>>(),
                vec![model.trim()]
            );
        }
        for device in snapshot.devices {
            assert_eq!(device.provider, GpuProvider::Apple);
            assert!(device.name.starts_with("Apple "));
            assert_eq!(device.memory_total, None);
        }
    }
}
