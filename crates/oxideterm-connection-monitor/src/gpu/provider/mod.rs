// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Vendor-specific GPU sampling commands and parsers.

mod amd;
mod ascend;
mod cambricon;
mod hygon;
mod intel;
mod mthreads;
mod native;
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

    if matches!(os_type, "Windows" | "windows") {
        return nvidia::windows_sample_command();
    }
    if matches!(os_type, "macOS" | "macos" | "Darwin") {
        return format!(
            "/usr/sbin/system_profiler SPDisplaysDataType -json && echo && echo '{GPU_END_MARKER}'\n"
        );
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
    if output.trim_start().starts_with('{') || section(output, "VIDEO_CONTROLLERS").is_some() {
        return native::inventory(output, timestamp_ms);
    }
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
    let rest = &output[start..];
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

pub(super) fn gpu_os_supported(os_type: &str) -> bool {
    matches!(
        os_type,
        "Linux"
            | "linux"
            | "Windows"
            | "windows"
            | "macOS"
            | "macos"
            | "Darwin"
            | "Windows_MinGW"
            | "Windows_MSYS"
            | "Windows_Cygwin"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn run_provider_script(tool: &str, fixture: &str, script: &str) -> (String, String) {
        use std::os::unix::fs::PermissionsExt;
        let directory = std::env::temp_dir().join(format!(
            "oxideterm-{tool}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let executable = directory.join(tool);
        std::fs::write(
            &executable,
            format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >&3\n{fixture}"),
        )
        .unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o755)).unwrap();
        let output = std::process::Command::new("sh")
            .args(["-c", &format!("exec 3>&2; {script}")])
            .env("PATH", format!("{}:/usr/bin:/bin", directory.display()))
            .output()
            .unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(output.status.success(), "{stderr}");
        (String::from_utf8(output.stdout).unwrap(), stderr)
    }

    #[cfg(unix)]
    #[test]
    fn amd_selected_sampling_reuses_inventory_and_recovers_from_failures() {
        let fixture = r#"
case "$*" in
  'metric --help') echo '--usage --mem-usage --power --temperature' ;;
  'static --help') echo '--limit' ;;
  'static --asic --bus --driver --vram --limit --json')
    if [ "$FAIL" = static ]; then echo 'No GPUs detected'; exit 1; fi
    echo '{"gpu_data":[{"gpu":0,"asic":{"market_name":"AMD Test GPU"},"bus":{"bdf":"0000:01:00.0"},"vram":{"size":1024}}]}' ;;
  'metric --usage --mem-usage --power --temperature --json')
    if [ "$FAIL" = metrics ]; then echo 'permission denied'; exit 1; fi
    echo '{"gpu_data":[{"gpu":0,"usage":{"gfx_activity":45},"mem_usage":{"used_vram":512}}]}' ;;
  'process --general --json') echo '{"gpu_data":[]}' ;;
  *) echo "unexpected arguments: $*"; exit 99 ;;
esac
"#;
        let script = format!(
            "sample() {{ {} echo '===SAMPLE_END==='; }}; \
             i=0; while [ \"$i\" -lt 31 ]; do sample; i=$((i+1)); done; \
             export FAIL=metrics; sample; unset FAIL; sample; \
             unset oxideterm_amd_static; export FAIL=static; sample; unset FAIL; sample;",
            amd::sample_command()
        );
        let (output, calls) = run_provider_script("amd-smi", fixture, &script);
        let snapshots: Vec<_> = output
            .split("===SAMPLE_END===")
            .filter(|sample| !sample.trim().is_empty())
            .map(amd::parse)
            .collect();
        assert_eq!(snapshots.len(), 35);
        for (index, snapshot) in snapshots.iter().enumerate() {
            match index {
                31 => assert_eq!(
                    snapshot.status,
                    ProviderStatus::Error("AMD: permission denied".into())
                ),
                33 => assert_eq!(snapshot.status, ProviderStatus::NoDevices),
                _ => {
                    assert_eq!(snapshot.status, ProviderStatus::Available, "sample {index}");
                    assert_eq!(snapshot.devices[0].name, "AMD Test GPU");
                    assert_eq!(snapshot.devices[0].memory_used, Some(536_870_912));
                    assert_eq!(snapshot.devices[0].utilization_percent, Some(45.0));
                }
            }
        }
        assert_eq!(
            calls
                .lines()
                .filter(|line| *line == "metric --help")
                .count(),
            1
        );
        assert_eq!(
            calls
                .lines()
                .filter(|line| *line == "static --help")
                .count(),
            1
        );
        // Two periodic reads, then one after each failed query and one forced failure.
        assert_eq!(
            calls
                .lines()
                .filter(|line| line.starts_with("static --asic"))
                .count(),
            5
        );
        assert_eq!(
            calls
                .lines()
                .filter(|line| line.starts_with("metric --usage"))
                .count(),
            34
        );
        assert!(!calls.lines().any(|line| line.contains("--fan")));
    }

    #[cfg(unix)]
    #[test]
    fn intel_negotiates_query_support_caches_inventory_and_recovers() {
        let fixture = r#"
case "$*" in
  --help) if [ "$MODE" = fields ]; then echo '--query-gpu'; else echo discovery; fi ;;
  '--query-gpu=index,uuid,pci.bus_id,driver_version,memory.total,name --format=csv,noheader,nounits')
    echo '0, intel-0, 0000:4d:00.0, 2.3, 16384, Intel Test GPU'
    echo '1, intel-1, 0000:5e:00.0, 2.3, 8192, Intel Second GPU' ;;
  '--query-gpu=index,utilization.gpu,utilization.memory,memory.used,temperature.gpu,power.draw --format=csv,noheader,nounits')
    if [ "$FAIL" = metrics ]; then echo 'permission denied'; exit 1; fi
    echo '1, 70, 3, 512, 51, 85'; echo '0, 20, 2, 256, 40, 50' ;;
  'discovery -j')
    if [ "$FAIL" = inventory ]; then echo 'permission denied'; exit 1; fi
    printf '%s\n' '{"device_list":[' '{"device_id":0,"device_name":"Intel Test GPU"},' '{"device_id":1,"device_name":"Intel Second GPU"}]}' ;;
  'discovery -d 0 -j'|'discovery -d 1 -j')
    printf '{"device_id":%s,"device_name":"Intel Detail GPU","uuid":"intel-%s"}\n' "$3" "$3" ;;
  'stats -d 0 -j') echo '{"device_id":0,"device_level":[{"metrics_type":"XPUM_STATS_GPU_UTILIZATION","value":20}]}' ;;
  'stats -d 1 -j') echo '{"device_id":1,"device_level":[{"metrics_type":"XPUM_STATS_GPU_UTILIZATION","value":70}]}' ;;
  'health -l -j') echo '{"device_list":[{"device_id":0,"core_temperature":{"status":"Ok"}}]}' ;;
  'ps -j') echo '{"device_util_by_proc_list":[]}' ;;
  *) echo "unexpected arguments: $*"; exit 99 ;;
esac
"#;
        for mode in ["fields", "legacy"] {
            let fail = if mode == "fields" {
                "metrics"
            } else {
                "inventory"
            };
            let script = format!(
                "export MODE={mode}; sample() {{ {} echo '===SAMPLE_END==='; }}; \
                 i=0; while [ \"$i\" -lt 31 ]; do sample; i=$((i+1)); done; \
                 unset oxideterm_intel_discovery_cache; export FAIL={fail}; sample; unset FAIL; sample;",
                intel::sample_command()
            );
            let (output, calls) = run_provider_script("xpu-smi", fixture, &script);
            let snapshots: Vec<_> = output
                .split("===SAMPLE_END===")
                .filter(|sample| !sample.trim().is_empty())
                .map(intel::parse)
                .collect();
            assert_eq!(snapshots.len(), 33);
            for (index, snapshot) in snapshots.iter().enumerate() {
                if index == 31 {
                    assert_eq!(
                        snapshot.status,
                        ProviderStatus::Error("Intel: permission denied".into())
                    );
                } else {
                    assert_eq!(
                        snapshot.status,
                        ProviderStatus::Available,
                        "{mode} sample {index}"
                    );
                    assert_eq!(
                        snapshot
                            .devices
                            .iter()
                            .map(|d| (d.uuid.as_str(), d.utilization_percent))
                            .collect::<Vec<_>>(),
                        vec![("intel-0", Some(20.0)), ("intel-1", Some(70.0))]
                    );
                    assert_eq!(snapshot.devices[0].health_status.as_deref(), Some("Ok"));
                }
            }
            assert_eq!(calls.lines().filter(|line| *line == "--help").count(), 1);
            assert_eq!(
                calls.lines().filter(|line| *line == "health -l -j").count(),
                4
            );
            if mode == "fields" {
                assert_eq!(
                    calls
                        .lines()
                        .filter(|line| line.starts_with("--query-gpu=index,uuid"))
                        .count(),
                    3
                );
                assert_eq!(
                    calls
                        .lines()
                        .filter(|line| line.starts_with("--query-gpu=index,utilization"))
                        .count(),
                    33
                );
                assert!(
                    !calls
                        .lines()
                        .any(|line| line.starts_with("discovery") || line.starts_with("stats"))
                );
            } else {
                assert_eq!(
                    calls.lines().filter(|line| *line == "discovery -j").count(),
                    4
                );
                assert_eq!(
                    calls
                        .lines()
                        .filter(|line| line.starts_with("discovery -d"))
                        .count(),
                    6
                );
                assert_eq!(
                    calls
                        .lines()
                        .filter(|line| line.starts_with("stats -d"))
                        .count(),
                    64
                );
                assert!(!calls.lines().any(|line| line.starts_with("--query-gpu")));
            }
        }
    }

    #[test]
    fn gpu_command_queries_only_platform_supported_providers() {
        let providers = [
            "nvidia-smi",
            "amd-smi",
            "hy-smi",
            "rocm-smi",
            "npu-smi",
            "cnmon",
            "xpu-smi",
            "mthreads-gmi",
        ];
        for (os, expected) in [
            ("Linux", [true; 8]),
            (
                "Windows_MSYS",
                [true, false, false, false, false, false, false, false],
            ),
            ("macOS", [false; 8]),
        ] {
            let command = build_gpu_sample_command(os);
            for (provider, enabled) in providers.into_iter().zip(expected) {
                assert_eq!(command.contains(provider), enabled, "{os}: {provider}");
            }
            assert!(command.contains(GPU_END_MARKER));
            if os == "Linux" {
                for invocation in [
                    "nvidia-smi --query-gpu=",
                    "amd-smi static --asic",
                    "npu-smi info",
                    "cnmon info",
                    "xpu-smi discovery -j",
                ] {
                    assert!(command.contains(invocation), "{invocation}");
                }
            } else if os == "Windows_MSYS" {
                assert!(command.contains("nvidia-smi --query-gpu="));
            } else {
                assert!(command.contains("system_profiler SPDisplaysDataType -json"));
            }
        }
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
    fn mixed_provider_failure_keeps_available_devices_visible() {
        let output = concat!(
            "===NVIDIA_STATUS===\navailable\n",
            "===NVIDIA_GPUS===\n",
            "0, GPU-a, 00000000:01:00.0, NVIDIA L40S, 555.42, P0, 10, 2, 512, 46068, 41, 50, 350, N/A\n",
            "===NVIDIA_GPU_QUERY_EXIT===\n0\n",
            "===NVIDIA_PROCESSES===\n",
            "===AMD_STATUS===\navailable\n",
            "===AMD_STATIC===\npermission denied\n",
            "===AMD_QUERY_EXIT===\n1\n",
            "===AMD_ERROR===\npermission denied\n",
            "===GPU_NPU_SAMPLE_END==="
        );

        let snapshot = parse_gpu_snapshot(output, 1);

        assert_eq!(snapshot.status, GpuSnapshotStatus::Available);
        assert_eq!(snapshot.devices.len(), 1);
    }
}
