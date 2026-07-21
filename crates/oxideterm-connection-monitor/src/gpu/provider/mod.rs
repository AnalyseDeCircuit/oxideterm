// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Vendor-specific GPU sampling commands and parsers.

mod amd;
mod ascend;
mod cambricon;
mod hygon;
mod intel;
mod mthreads;
mod nvidia;

use super::{GpuDevice, GpuProcess, GpuSnapshot, GpuSnapshotStatus};

pub const GPU_END_MARKER: &str = "===GPU_NPU_SAMPLE_END===";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ProviderStatus {
    Available,
    NoDevices,
    Unavailable,
    Error(String),
    Unknown,
}

pub(super) struct ProviderSnapshot {
    pub status: ProviderStatus,
    pub devices: Vec<GpuDevice>,
    pub processes: Vec<GpuProcess>,
}

/// Builds one bounded probe that can collect every supported accelerator vendor.
pub fn build_gpu_sample_command(os_type: &str) -> String {
    if !gpu_os_supported(os_type) {
        return format!("echo '===GPU_NPU_STATUS==='; echo unsupported; echo '{GPU_END_MARKER}'\n");
    }

    let mut command = nvidia::sample_command();
    if matches!(os_type, "Linux" | "linux") {
        command.push_str(&amd::sample_command());
        command.push_str(&hygon::sample_command());
        command.push_str(&ascend::sample_command());
        command.push_str(&cambricon::sample_command());
        command.push_str(&intel::sample_command());
        command.push_str(&mthreads::sample_command());
    }
    command.push_str(&format!("echo '{GPU_END_MARKER}'\n"));
    command
}

/// Parses all vendor sections into one provider-neutral snapshot.
pub fn parse_gpu_snapshot(output: &str, timestamp_ms: u64) -> GpuSnapshot {
    if first_section_line(output, "GPU_NPU_STATUS") == Some("unsupported") {
        return GpuSnapshot {
            timestamp_ms,
            status: GpuSnapshotStatus::Unsupported,
            devices: Vec::new(),
            processes: Vec::new(),
        };
    }

    let provider_snapshots = [
        nvidia::parse(output),
        amd::parse(output),
        hygon::parse(output),
        ascend::parse(output),
        cambricon::parse(output),
        intel::parse(output),
        mthreads::parse(output),
    ];
    let mut devices = provider_snapshots
        .iter()
        .flat_map(|snapshot| snapshot.devices.iter().cloned())
        .collect::<Vec<_>>();
    let mut processes = provider_snapshots
        .iter()
        .flat_map(|snapshot| snapshot.processes.iter().cloned())
        .collect::<Vec<_>>();
    devices.sort_by_key(|device| (device.provider, device.index));
    processes.sort_by(|left, right| {
        left.provider
            .cmp(&right.provider)
            .then_with(|| left.gpu_uuid.cmp(&right.gpu_uuid))
            .then_with(|| left.pid.cmp(&right.pid))
    });

    let status = merged_status(&provider_snapshots, !devices.is_empty());
    GpuSnapshot {
        timestamp_ms,
        status,
        devices,
        processes,
    }
}

fn merged_status(snapshots: &[ProviderSnapshot], has_devices: bool) -> GpuSnapshotStatus {
    if has_devices {
        // A failing secondary provider must not hide usable devices from the
        // other vendor on mixed GPU hosts.
        return GpuSnapshotStatus::Available;
    }

    let errors = snapshots
        .iter()
        .filter_map(|snapshot| match &snapshot.status {
            ProviderStatus::Error(message) => Some(message.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if !errors.is_empty() {
        return GpuSnapshotStatus::Error(errors.join("; "));
    }
    if snapshots.iter().any(|snapshot| {
        matches!(
            snapshot.status,
            ProviderStatus::Available | ProviderStatus::NoDevices
        )
    }) {
        return GpuSnapshotStatus::NoDevices;
    }
    if snapshots
        .iter()
        .any(|snapshot| snapshot.status == ProviderStatus::Unavailable)
    {
        return GpuSnapshotStatus::Unavailable;
    }
    GpuSnapshotStatus::Unknown
}

pub(super) fn section<'a>(output: &'a str, marker: &str) -> Option<&'a str> {
    let marker = format!("==={marker}===");
    let start = output.find(&marker)? + marker.len();
    let rest = output[start..].trim_start_matches(['\r', '\n']);
    // Protocol markers use uppercase identifiers. This distinction prevents
    // vendor table separators and banners made from '=' from ending a section.
    let end = rest
        .match_indices("\n===")
        .find_map(|(offset, _)| {
            let line = rest[offset + 1..].lines().next()?.trim_end_matches('\r');
            let identifier = line.strip_prefix("===")?.strip_suffix("===")?;
            (!identifier.is_empty()
                && identifier.chars().all(|character| {
                    character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
                }))
            .then_some(offset)
        })
        .unwrap_or(rest.len());
    Some(rest[..end].trim())
}

pub(super) fn first_section_line<'a>(output: &'a str, marker: &str) -> Option<&'a str> {
    section(output, marker)?
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
}

pub(super) fn sanitized_error(output: &str, marker: &str, fallback: &str) -> String {
    let message = first_section_line(output, marker).unwrap_or(fallback);
    message
        .chars()
        .filter(|character| !character.is_control())
        .take(240)
        .collect()
}

fn gpu_os_supported(os_type: &str) -> bool {
    matches!(
        os_type,
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_command_queries_all_supported_providers() {
        let command = build_gpu_sample_command("Linux");

        assert!(command.contains("nvidia-smi --query-gpu="));
        assert!(command.contains("amd-smi --json"));
        assert!(command.contains("hy-smi"));
        assert!(command.contains("rocm-smi"));
        assert!(command.contains("npu-smi info"));
        assert!(command.contains("cnmon info"));
        assert!(command.contains("xpu-smi discovery -j"));
        assert!(command.contains("mthreads-gmi"));
        assert!(command.contains(GPU_END_MARKER));
    }

    #[cfg(unix)]
    #[test]
    fn linux_command_is_valid_posix_shell_syntax() {
        let command = build_gpu_sample_command("Linux");
        // Validate the composed provider protocol without executing vendor tools.
        let status = std::process::Command::new("sh")
            .args(["-n", "-c", &command])
            .status()
            .expect("POSIX shell should be available on Unix test hosts");

        assert!(status.success());
    }

    #[test]
    fn windows_command_does_not_invoke_linux_only_amd_smi() {
        let command = build_gpu_sample_command("Windows_MSYS");

        assert!(command.contains("nvidia-smi --query-gpu="));
        assert!(!command.contains("amd-smi"));
        assert!(!command.contains("hy-smi"));
        assert!(!command.contains("rocm-smi"));
        assert!(!command.contains("npu-smi"));
        assert!(!command.contains("cnmon"));
        assert!(!command.contains("xpu-smi"));
        assert!(!command.contains("mthreads-gmi"));
    }

    #[test]
    fn unsupported_system_does_not_invoke_provider_tools() {
        let command = build_gpu_sample_command("macOS");

        assert!(command.contains("echo unsupported"));
        assert!(!command.contains("nvidia-smi"));
        assert!(!command.contains("amd-smi"));
        assert!(!command.contains("hy-smi"));
        assert!(!command.contains("rocm-smi"));
        assert!(!command.contains("npu-smi"));
        assert!(!command.contains("cnmon"));
        assert!(!command.contains("xpu-smi"));
        assert!(!command.contains("mthreads-gmi"));
    }

    #[test]
    fn mixed_provider_failure_keeps_available_devices_visible() {
        let output = concat!(
            "===NVIDIA_STATUS===\navailable\n",
            "===NVIDIA_GPUS===\n",
            "0, GPU-a, 00000000:01:00.0, NVIDIA L40S, 555.42, P0, 10, 2, 512, 46068, 41, 50, 350, N/A\n",
            "===NVIDIA_GPU_QUERY_EXIT===\n0\n",
            "===NVIDIA_PROCESSES===\n",
            "===AMD_STATUS===\navailable\n",
            "===AMD_DATA===\npermission denied\n",
            "===AMD_QUERY_EXIT===\n1\n",
            "===AMD_ERROR===\npermission denied\n",
            "===GPU_NPU_SAMPLE_END==="
        );

        let snapshot = parse_gpu_snapshot(output, 1);

        assert_eq!(snapshot.status, GpuSnapshotStatus::Available);
        assert_eq!(snapshot.devices.len(), 1);
    }
}
