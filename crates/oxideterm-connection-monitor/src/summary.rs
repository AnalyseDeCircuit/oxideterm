use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use chrono::{DateTime, Local, Utc};

use crate::metrics::{MetricsSource, ResourceGpu, ResourceMetrics};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MonitorMetricKind {
    System,
    SystemVersion,
    Architecture,
    BootTime,
    Uptime,
    Cpu,
    Memory,
    Swap,
    Disk,
    DiskRead,
    DiskWrite,
    Gpu,
    GpuMemory,
    LoadAverage,
    Rtt,
    Source,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MonitorSectionKind {
    CpuCores,
    Mounts,
    Gpus,
    Interfaces,
    TopProcesses,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MonitorValueLevel {
    Muted,
    Normal,
    Warning,
    Critical,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CompactMonitorOptions {
    pub cpu_cores_expanded: bool,
    pub show_cpu_sparkline: bool,
    pub show_network_sparkline: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MonitorUsageKind {
    Cpu {
        core_count: Option<u32>,
        expandable: bool,
        expanded: bool,
    },
    Memory,
    Swap,
    Disk,
    Mount {
        mount_point: String,
    },
    Gpu {
        index: u32,
        label: String,
    },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MonitorSparklineKind {
    Cpu,
    Network,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MonitorCpuCoreCell {
    pub index: u32,
    pub percent: Option<f64>,
    pub level: MonitorValueLevel,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MonitorDiskIoMeta {
    pub read: String,
    pub write: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompactMonitorRow {
    Usage {
        kind: MonitorUsageKind,
        detail: Option<String>,
        percent: Option<f64>,
        level: MonitorValueLevel,
    },
    CpuCorePair {
        left: MonitorCpuCoreCell,
        right: Option<MonitorCpuCoreCell>,
    },
    LoadAverage {
        value: String,
    },
    Sparkline {
        kind: MonitorSparklineKind,
    },
    SectionHeader {
        kind: MonitorSectionKind,
        disk_io: Option<MonitorDiskIoMeta>,
    },
    NetworkSummary {
        rx: Option<String>,
        tx: Option<String>,
        rx_active: bool,
        tx_active: bool,
    },
    Interface {
        name: String,
        rx: String,
        tx: String,
        rx_active: bool,
        tx_active: bool,
    },
    Process {
        pid: String,
        command: String,
        memory_percent: String,
        level: MonitorValueLevel,
    },
    Retry {
        connection_id: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorListRow {
    pub name: String,
    pub value: String,
    pub level: MonitorValueLevel,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GpuMemorySummary {
    pub used: u64,
    pub total: u64,
    pub percent: Option<f64>,
}

pub fn compact_monitor_rows(
    metrics: &ResourceMetrics,
    retry_connection_id: Option<String>,
) -> Vec<CompactMonitorRow> {
    compact_monitor_rows_with_options(
        metrics,
        CompactMonitorOptions::default(),
        retry_connection_id,
    )
}

pub fn compact_monitor_rows_with_options(
    metrics: &ResourceMetrics,
    options: CompactMonitorOptions,
    retry_connection_id: Option<String>,
) -> Vec<CompactMonitorRow> {
    let mut rows = Vec::new();
    if !resource_metrics_is_rtt_only(metrics) {
        let core_count = metrics.cpu_cores.filter(|&count| count > 0).or_else(|| {
            (!metrics.cpu_per_core.is_empty()).then_some(metrics.cpu_per_core.len() as u32)
        });
        let expandable = has_sampled_cpu_cores(metrics);
        let expanded = expandable && options.cpu_cores_expanded;
        rows.push(usage(
            MonitorUsageKind::Cpu {
                core_count,
                expandable,
                expanded,
            },
            None,
            normalized_percent(metrics.cpu_percent),
        ));
        if expanded {
            let core_cell = |core: &crate::metrics::ResourceCpuCore| {
                let percent = normalized_percent(core.percent);
                MonitorCpuCoreCell {
                    index: core.index,
                    percent,
                    level: percent_level(percent),
                }
            };
            rows.extend(metrics.cpu_per_core.chunks(2).map(|pair| {
                CompactMonitorRow::CpuCorePair {
                    left: core_cell(&pair[0]),
                    right: pair.get(1).map(core_cell),
                }
            }));
        }
        if let Some(load) = metrics.load_avg_1 {
            rows.push(CompactMonitorRow::LoadAverage {
                value: format!(
                    "{load:.2} · {:.2} · {:.2}",
                    metrics.load_avg_5.unwrap_or_default(),
                    metrics.load_avg_15.unwrap_or_default(),
                ),
            });
        }
        if options.show_cpu_sparkline {
            rows.push(CompactMonitorRow::Sparkline {
                kind: MonitorSparklineKind::Cpu,
            });
        }
        for (kind, used, total, percent) in [
            (
                MonitorUsageKind::Memory,
                metrics.memory_used,
                metrics.memory_total,
                metrics.memory_percent,
            ),
            (
                MonitorUsageKind::Swap,
                metrics.swap_used,
                metrics.swap_total,
                metrics.swap_percent,
            ),
        ] {
            if let (Some(used), Some(total)) = (used, total)
                && total > 0
            {
                rows.push(capacity_usage(kind, used, total, percent));
            }
        }
        if !metrics.gpus.is_empty() {
            rows.push(CompactMonitorRow::SectionHeader {
                kind: MonitorSectionKind::Gpus,
                disk_io: None,
            });
            rows.extend(metrics.gpus.iter().take(4).map(|gpu| {
                let capacity = gpu.memory_used.zip(gpu.memory_total);
                let detail = capacity
                    .filter(|&(_, total)| total > 0)
                    .map(|(used, total)| format_compact_capacity(used, total));
                let percent = normalized_percent(gpu.utilization_percent)
                    .or_else(|| normalized_percent(gpu.memory_percent))
                    .or_else(|| capacity.and_then(|(used, total)| capacity_percent(used, total)));
                let label = if gpu.name.trim().is_empty() {
                    gpu_label(gpu)
                } else {
                    format!("GPU {} · {}", gpu.index, gpu.name.trim())
                };
                usage(
                    MonitorUsageKind::Gpu {
                        index: gpu.index,
                        label,
                    },
                    detail,
                    percent,
                )
            }));
        }
        let disk_io = (metrics.disk_read_bytes_per_sec.is_some()
            || metrics.disk_write_bytes_per_sec.is_some())
        .then(|| {
            let read = metrics.disk_read_bytes_per_sec.unwrap_or_default();
            let write = metrics.disk_write_bytes_per_sec.unwrap_or_default();
            MonitorDiskIoMeta {
                read: format_rate(read),
                write: format_rate(write),
                active: read > 0 || write > 0,
            }
        });
        let disk_capacity = metrics
            .disk_used
            .zip(metrics.disk_total)
            .filter(|&(_, total)| total > 0);
        if !metrics.disks.is_empty() || disk_capacity.is_some() || disk_io.is_some() {
            rows.push(CompactMonitorRow::SectionHeader {
                kind: MonitorSectionKind::Mounts,
                disk_io,
            });
            if metrics.disks.is_empty() {
                if let Some((used, total)) = disk_capacity {
                    rows.push(capacity_usage(
                        MonitorUsageKind::Disk,
                        used,
                        total,
                        metrics.disk_percent,
                    ));
                }
            } else {
                rows.extend(metrics.disks.iter().take(8).map(|disk| {
                    capacity_usage(
                        MonitorUsageKind::Mount {
                            mount_point: disk.mount_point.clone(),
                        },
                        disk.used,
                        disk.total,
                        disk.percent,
                    )
                }));
            }
        }
        let has_rates =
            metrics.net_rx_bytes_per_sec.is_some() || metrics.net_tx_bytes_per_sec.is_some();
        if has_rates || !metrics.net_interfaces.is_empty() {
            let rx = metrics.net_rx_bytes_per_sec.unwrap_or_default();
            let tx = metrics.net_tx_bytes_per_sec.unwrap_or_default();
            rows.push(CompactMonitorRow::NetworkSummary {
                rx: has_rates.then(|| format_rate(rx)),
                tx: has_rates.then(|| format_rate(tx)),
                rx_active: rx > 0,
                tx_active: tx > 0,
            });
            if options.show_network_sparkline {
                rows.push(CompactMonitorRow::Sparkline {
                    kind: MonitorSparklineKind::Network,
                });
            }
            rows.extend(metrics.net_interfaces.iter().take(8).map(|iface| {
                CompactMonitorRow::Interface {
                    name: iface.name.clone(),
                    rx: iface.rx_bytes_per_sec.map(format_rate).unwrap_or_else(dash),
                    tx: iface.tx_bytes_per_sec.map(format_rate).unwrap_or_else(dash),
                    rx_active: iface.rx_bytes_per_sec.unwrap_or_default() > 0,
                    tx_active: iface.tx_bytes_per_sec.unwrap_or_default() > 0,
                }
            }));
        }
        if !metrics.top_processes.is_empty() {
            rows.push(CompactMonitorRow::SectionHeader {
                kind: MonitorSectionKind::TopProcesses,
                disk_io: None,
            });
            rows.extend(metrics.top_processes.iter().take(5).map(|process| {
                CompactMonitorRow::Process {
                    pid: process.pid.clone(),
                    command: process.command.clone(),
                    memory_percent: format!("{:.1}%", process.memory_percent),
                    level: percent_level(Some(process.memory_percent)),
                }
            }));
        }
    }
    if let Some(connection_id) = retry_connection_id {
        rows.push(CompactMonitorRow::Retry { connection_id });
    }
    rows
}

fn normalized_percent(percent: Option<f64>) -> Option<f64> {
    percent
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 100.0))
}

fn usage(
    kind: MonitorUsageKind,
    detail: Option<String>,
    percent: Option<f64>,
) -> CompactMonitorRow {
    CompactMonitorRow::Usage {
        kind,
        detail,
        percent,
        level: percent_level(percent),
    }
}

fn capacity_usage(
    kind: MonitorUsageKind,
    used: u64,
    total: u64,
    percent: Option<f64>,
) -> CompactMonitorRow {
    usage(
        kind,
        Some(format_compact_capacity(used, total)),
        normalized_percent(percent).or_else(|| capacity_percent(used, total)),
    )
}

/// Produces a stable row identity without treating live metric values as row geometry.
pub fn compact_monitor_row_signature(row: &CompactMonitorRow) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        CompactMonitorRow::Usage { kind, .. } => (0_u8, kind).hash(&mut hasher),
        CompactMonitorRow::CpuCorePair { left, right } => {
            (1_u8, left.index, right.as_ref().map(|core| core.index)).hash(&mut hasher)
        }
        CompactMonitorRow::LoadAverage { .. } => 2_u8.hash(&mut hasher),
        CompactMonitorRow::Sparkline { kind } => (3_u8, kind).hash(&mut hasher),
        CompactMonitorRow::SectionHeader { kind, disk_io } => {
            (4_u8, kind, disk_io.is_some()).hash(&mut hasher)
        }
        CompactMonitorRow::NetworkSummary { rx, .. } => (5_u8, rx.is_some()).hash(&mut hasher),
        CompactMonitorRow::Interface { name, .. } => (6_u8, name).hash(&mut hasher),
        CompactMonitorRow::Process { pid, command, .. } => (7_u8, pid, command).hash(&mut hasher),
        CompactMonitorRow::Retry { connection_id } => (8_u8, connection_id).hash(&mut hasher),
    }
    hasher.finish()
}

pub fn resource_metrics_is_rtt_only(metrics: &ResourceMetrics) -> bool {
    matches!(
        metrics.source,
        MetricsSource::RttOnly | MetricsSource::Failed | MetricsSource::Unsupported
    )
}

pub fn percent_level(value: Option<f64>) -> MonitorValueLevel {
    match value {
        None => MonitorValueLevel::Muted,
        Some(value) if value < 70.0 => MonitorValueLevel::Normal,
        Some(value) if value < 90.0 => MonitorValueLevel::Warning,
        Some(_) => MonitorValueLevel::Critical,
    }
}

pub fn rtt_level(value: Option<u64>) -> MonitorValueLevel {
    match value {
        None => MonitorValueLevel::Muted,
        Some(value) if value < 100 => MonitorValueLevel::Normal,
        Some(value) if value < 300 => MonitorValueLevel::Warning,
        Some(_) => MonitorValueLevel::Critical,
    }
}

pub fn metrics_source_label_key(source: MetricsSource) -> &'static str {
    match source {
        MetricsSource::Full => "profiler.panel.source_full",
        MetricsSource::Partial => "profiler.panel.source_partial",
        MetricsSource::RttOnly => "profiler.panel.source_rtt_only",
        MetricsSource::Failed => "profiler.panel.source_failed",
        MetricsSource::Unsupported => "profiler.panel.source_unsupported",
    }
}

const BYTE_UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];

fn scaled_bytes(bytes: u64) -> (f64, usize) {
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < BYTE_UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    (value, unit)
}

pub fn format_bytes(bytes: u64) -> String {
    let (value, unit) = scaled_bytes(bytes);
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", BYTE_UNITS[unit])
    }
}

pub fn capacity_percent(used: u64, total: u64) -> Option<f64> {
    (total > 0).then(|| (used as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

pub fn format_compact_capacity(used: u64, total: u64) -> String {
    let (used_value, used_unit) = scaled_bytes(used);
    let (total_value, total_unit) = scaled_bytes(total);
    if used_unit != total_unit {
        format!("{} / {}", format_bytes(used), format_bytes(total))
    } else if total_unit == 0 {
        format!("{used} / {total} B")
    } else {
        format!(
            "{used_value:.1} / {total_value:.1} {}",
            BYTE_UNITS[total_unit]
        )
    }
}

pub fn format_compact_uptime(uptime_seconds: u64) -> String {
    let days = uptime_seconds / 86_400;
    let hours = uptime_seconds / 3_600 % 24;
    let minutes = uptime_seconds / 60 % 60;
    let seconds = uptime_seconds % 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn strip_trailing_codename(os: &str) -> String {
    let trimmed = os.trim();
    if let Some(stripped) = trimmed.strip_suffix(')')
        && let Some((prefix, _codename)) = stripped.rsplit_once(" (")
    {
        let prefix = prefix.trim_end();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }
    trimmed.to_string()
}

pub fn format_compact_system_parts(
    info: &crate::metrics::ResourceSystemInfo,
) -> (Option<String>, Option<String>) {
    fn nonempty(value: &Option<String>) -> Option<&str> {
        value.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }
    let os = match (nonempty(&info.system_name), nonempty(&info.system_version)) {
        (Some(name), Some(version))
            if version
                .get(..name.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(name)) =>
        {
            Some(strip_trailing_codename(version))
        }
        (Some(name), Some(version)) => Some(strip_trailing_codename(&format!("{name} {version}"))),
        (Some(value), None) | (None, Some(value)) => Some(strip_trailing_codename(value)),
        (None, None) => None,
    };
    (os, nonempty(&info.architecture).map(str::to_owned))
}

pub fn format_compact_system_identity(info: &crate::metrics::ResourceSystemInfo) -> Option<String> {
    match format_compact_system_parts(info) {
        (Some(os), Some(arch)) => Some(format!("{os} · {arch}")),
        (Some(os), None) => Some(os),
        (None, Some(arch)) => Some(arch),
        (None, None) => None,
    }
}

pub fn has_sampled_cpu_cores(metrics: &ResourceMetrics) -> bool {
    metrics.cpu_per_core.len() > 1
        && metrics
            .cpu_per_core
            .iter()
            .any(|core| core.percent.is_some_and(f64::is_finite))
}

pub fn has_cpu_trend_series(history: &[crate::metrics::ResourceTrendSample]) -> bool {
    history
        .iter()
        .filter(|sample| {
            sample
                .cpu_percent
                .is_some_and(|value| value.is_finite() && value >= 0.0)
        })
        .nth(1)
        .is_some()
}

pub fn has_network_trend_series(history: &[crate::metrics::ResourceTrendSample]) -> bool {
    history
        .iter()
        .filter(|sample| sample.rx_bytes_per_sec.is_some() || sample.tx_bytes_per_sec.is_some())
        .nth(1)
        .is_some()
}

pub fn format_boot_time(timestamp_ms: u64) -> Option<String> {
    let timestamp_ms = i64::try_from(timestamp_ms).ok()?;
    DateTime::<Utc>::from_timestamp_millis(timestamp_ms).map(|value| {
        value
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    })
}

pub fn format_uptime(uptime_seconds: u64) -> String {
    const SECONDS_PER_MINUTE: u64 = 60;
    const SECONDS_PER_HOUR: u64 = 60 * SECONDS_PER_MINUTE;
    const SECONDS_PER_DAY: u64 = 24 * SECONDS_PER_HOUR;

    let days = uptime_seconds / SECONDS_PER_DAY;
    let hours = (uptime_seconds % SECONDS_PER_DAY) / SECONDS_PER_HOUR;
    let minutes = (uptime_seconds % SECONDS_PER_HOUR) / SECONDS_PER_MINUTE;
    let seconds = uptime_seconds % SECONDS_PER_MINUTE;
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    }
}

pub fn format_rate(bytes: u64) -> String {
    format!("{}/s", format_bytes(bytes))
}

pub fn gpu_utilization_percent(metrics: &ResourceMetrics) -> Option<f64> {
    metrics
        .gpus
        .iter()
        .filter_map(|gpu| gpu.utilization_percent)
        .max_by(|left, right| left.total_cmp(right))
}

pub fn gpu_memory_percent(metrics: &ResourceMetrics) -> Option<f64> {
    gpu_memory_summary(metrics).and_then(|summary| summary.percent)
}

pub fn gpu_memory_summary(metrics: &ResourceMetrics) -> Option<GpuMemorySummary> {
    let mut used = 0_u64;
    let mut total = 0_u64;
    for gpu in &metrics.gpus {
        let (Some(gpu_used), Some(gpu_total)) = (gpu.memory_used, gpu.memory_total) else {
            continue;
        };
        used = used.saturating_add(gpu_used);
        total = total.saturating_add(gpu_total);
    }
    if total == 0 {
        return None;
    }
    Some(GpuMemorySummary {
        used,
        total,
        percent: Some((used as f64 / total as f64) * 100.0),
    })
}

pub fn gpu_label(gpu: &ResourceGpu) -> String {
    if gpu.name.trim().is_empty() {
        format!("GPU {}", gpu.index)
    } else {
        format!("GPU {} {}", gpu.index, gpu.name)
    }
}

pub fn gpu_detail_value(gpu: &ResourceGpu) -> String {
    let utilization = gpu
        .utilization_percent
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(dash);
    let memory = match (gpu.memory_used, gpu.memory_total, gpu.memory_percent) {
        (Some(used), Some(total), Some(percent)) => {
            format!(
                "{} / {} ({percent:.0}%)",
                format_bytes(used),
                format_bytes(total)
            )
        }
        (Some(used), Some(total), None) => {
            format!("{} / {}", format_bytes(used), format_bytes(total))
        }
        _ => dash(),
    };
    format!("GPU {utilization} · VRAM {memory}")
}

pub fn disk_list_rows(metrics: &ResourceMetrics, limit: usize) -> Vec<MonitorListRow> {
    metrics
        .disks
        .iter()
        .take(limit)
        .map(|disk| MonitorListRow {
            name: disk.mount_point.clone(),
            value: disk
                .percent
                .map(|percent| format!("{percent:.0}%"))
                .unwrap_or_else(dash),
            level: percent_level(disk.percent),
        })
        .collect()
}

pub fn gpu_list_rows(metrics: &ResourceMetrics, limit: usize) -> Vec<MonitorListRow> {
    metrics
        .gpus
        .iter()
        .take(limit)
        .map(|gpu| MonitorListRow {
            name: gpu_label(gpu),
            value: gpu_detail_value(gpu),
            level: percent_level(gpu.memory_percent.or(gpu.utilization_percent)),
        })
        .collect()
}

pub fn interface_list_rows(metrics: &ResourceMetrics, limit: usize) -> Vec<MonitorListRow> {
    metrics
        .net_interfaces
        .iter()
        .take(limit)
        .map(|iface| {
            let rx = iface.rx_bytes_per_sec.map(format_rate).unwrap_or_else(dash);
            let tx = iface.tx_bytes_per_sec.map(format_rate).unwrap_or_else(dash);
            MonitorListRow {
                name: iface.name.clone(),
                value: format!("rx {rx} / tx {tx}"),
                level: MonitorValueLevel::Muted,
            }
        })
        .collect()
}

pub fn top_process_list_rows(metrics: &ResourceMetrics, limit: usize) -> Vec<MonitorListRow> {
    metrics
        .top_processes
        .iter()
        .take(limit)
        .map(|process| MonitorListRow {
            name: format!("{} {}", process.pid, process.command),
            value: format!("{:.1}%", process.memory_percent),
            level: percent_level(Some(process.memory_percent)),
        })
        .collect()
}

fn dash() -> String {
    "—".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::{ResourceCpuCore, ResourceDisk, ResourceSystemInfo, ResourceTrendSample};

    const GIB: u64 = 1 << 30;

    #[test]
    fn first_sample_keeps_cpu_slot_stable_and_suppresses_unsampled_cores_and_swap_zero() {
        let mut metrics = ResourceMetrics::empty(0, MetricsSource::Full);
        metrics.cpu_cores = Some(4);
        metrics.cpu_per_core = (0..4)
            .map(|index| ResourceCpuCore {
                index,
                percent: None,
            })
            .collect();
        metrics.swap_used = Some(0);
        metrics.swap_total = Some(0);
        metrics.memory_used = Some(4 * GIB);
        metrics.memory_total = Some(16 * GIB);

        assert_eq!(
            compact_monitor_rows_with_options(
                &metrics,
                CompactMonitorOptions {
                    cpu_cores_expanded: true,
                    ..Default::default()
                },
                None
            ),
            [
                CompactMonitorRow::Usage {
                    kind: MonitorUsageKind::Cpu {
                        core_count: Some(4),
                        expandable: false,
                        expanded: false
                    },
                    detail: None,
                    percent: None,
                    level: MonitorValueLevel::Muted,
                },
                CompactMonitorRow::Usage {
                    kind: MonitorUsageKind::Memory,
                    detail: Some("4.0 / 16.0 GB".into()),
                    percent: Some(25.0),
                    level: MonitorValueLevel::Normal,
                },
            ],
        );
    }

    #[test]
    fn expanded_sampled_cpu_cores_chunk_into_two_column_pairs() {
        let mut metrics = ResourceMetrics::empty(0, MetricsSource::Full);
        metrics.cpu_per_core = [15.0, 75.0, 92.0]
            .into_iter()
            .enumerate()
            .map(|(index, percent)| ResourceCpuCore {
                index: index as u32,
                percent: Some(percent),
            })
            .collect();
        metrics.cpu_percent = Some(60.0);
        metrics.load_avg_1 = Some(0.77);

        let collapsed = compact_monitor_rows(&metrics, None);
        let cpu = |expanded| CompactMonitorRow::Usage {
            kind: MonitorUsageKind::Cpu {
                core_count: Some(3),
                expandable: true,
                expanded,
            },
            detail: None,
            percent: Some(60.0),
            level: MonitorValueLevel::Normal,
        };
        let load = CompactMonitorRow::LoadAverage {
            value: "0.77 · 0.00 · 0.00".into(),
        };
        assert_eq!(collapsed, [cpu(false), load.clone()]);

        let options = CompactMonitorOptions {
            cpu_cores_expanded: true,
            ..Default::default()
        };
        let expanded = compact_monitor_rows_with_options(&metrics, options, None);
        let core = |index, percent, level| MonitorCpuCoreCell {
            index,
            percent: Some(percent),
            level,
        };
        assert_eq!(
            expanded,
            [
                cpu(true),
                CompactMonitorRow::CpuCorePair {
                    left: core(0, 15.0, MonitorValueLevel::Normal),
                    right: Some(core(1, 75.0, MonitorValueLevel::Warning)),
                },
                CompactMonitorRow::CpuCorePair {
                    left: core(2, 92.0, MonitorValueLevel::Critical),
                    right: None,
                },
                load,
            ]
        );
        assert_ne!(
            compact_monitor_row_signature(&collapsed[0]),
            compact_monitor_row_signature(&expanded[0])
        );
        assert_eq!(
            compact_monitor_row_signature(&collapsed[1]),
            compact_monitor_row_signature(&expanded[3])
        );

        let signatures = |rows: &[CompactMonitorRow]| {
            rows.iter()
                .map(compact_monitor_row_signature)
                .collect::<Vec<_>>()
        };
        metrics.cpu_percent = Some(91.0);
        metrics.cpu_per_core[0].percent = Some(85.0);
        metrics.load_avg_1 = Some(1.25);
        let updated = compact_monitor_rows_with_options(&metrics, options, None);
        assert_ne!(updated, expanded);
        assert_eq!(signatures(&updated), signatures(&expanded));
    }

    #[test]
    fn mounts_deduplicate_top_level_disk_card_and_preserve_fallback() {
        let mut metrics = ResourceMetrics::empty(0, MetricsSource::Full);
        metrics.disk_used = Some(20 * GIB);
        metrics.disk_total = Some(100 * GIB);
        metrics.disk_percent = Some(25.0);
        metrics.disks = vec![
            ResourceDisk {
                mount_point: "/".into(),
                used: 20 * GIB,
                total: 100 * GIB,
                percent: Some(25.0),
            },
            ResourceDisk {
                mount_point: "/boot/efi".into(),
                used: 128,
                total: 512,
                percent: None,
            },
        ];
        let header = CompactMonitorRow::SectionHeader {
            kind: MonitorSectionKind::Mounts,
            disk_io: None,
        };
        assert_eq!(
            &compact_monitor_rows(&metrics, None)[1..],
            [
                header.clone(),
                CompactMonitorRow::Usage {
                    kind: MonitorUsageKind::Mount {
                        mount_point: "/".into()
                    },
                    detail: Some("20.0 / 100.0 GB".into()),
                    percent: Some(25.0),
                    level: MonitorValueLevel::Normal,
                },
                CompactMonitorRow::Usage {
                    kind: MonitorUsageKind::Mount {
                        mount_point: "/boot/efi".into()
                    },
                    detail: Some("128 / 512 B".into()),
                    percent: Some(25.0),
                    level: MonitorValueLevel::Normal,
                },
            ]
        );
        metrics.disks.clear();
        assert_eq!(
            &compact_monitor_rows(&metrics, None)[1..],
            [
                header,
                CompactMonitorRow::Usage {
                    kind: MonitorUsageKind::Disk,
                    detail: Some("20.0 / 100.0 GB".into()),
                    percent: Some(25.0),
                    level: MonitorValueLevel::Normal,
                },
            ]
        );
    }

    #[test]
    fn compact_formatters_and_trend_thresholds_handle_boundaries() {
        assert_eq!(
            format_compact_capacity(6_335_076_762, 16_536_624_333),
            "5.9 / 15.4 GB"
        );
        assert_eq!(format_compact_capacity(6_396_314, GIB), "6.1 MB / 1.0 GB");
        assert_eq!(format_compact_capacity(0, 512), "0 / 512 B");
        assert_eq!(capacity_percent(0, 0), None);
        assert_eq!(capacity_percent(101, 100), Some(100.0));
        assert_eq!(format_compact_uptime(61 * 86_400 + 20 * 3_600), "61d 20h");
        assert_eq!(format_compact_uptime(5 * 3_600 + 42 * 60), "5h 42m");
        assert_eq!(format_compact_uptime(12 * 60 + 8), "12m 8s");
        assert_eq!(format_compact_uptime(8), "8s");
        assert_eq!(
            format_compact_system_identity(&ResourceSystemInfo {
                system_name: Some(" Ubuntu ".into()),
                system_version: Some("Ubuntu 24.04 LTS".into()),
                architecture: Some("x86_64".into()),
                ..Default::default()
            })
            .as_deref(),
            Some("Ubuntu 24.04 LTS · x86_64")
        );
        assert_eq!(
            format_compact_system_parts(&ResourceSystemInfo {
                system_name: Some("Ubuntu".into()),
                system_version: Some("24.04.3 LTS (Noble Numbat)".into()),
                architecture: Some("x86_64".into()),
                ..Default::default()
            }),
            (Some("Ubuntu 24.04.3 LTS".into()), Some("x86_64".into()))
        );
        assert_eq!(
            format_compact_system_identity(&ResourceSystemInfo::default()),
            None
        );

        let samples = [
            ResourceTrendSample {
                timestamp_ms: 1,
                cpu_percent: Some(0.0),
                rx_bytes_per_sec: Some(0),
                tx_bytes_per_sec: None,
            },
            ResourceTrendSample {
                timestamp_ms: 2,
                cpu_percent: None,
                rx_bytes_per_sec: None,
                tx_bytes_per_sec: None,
            },
            ResourceTrendSample {
                timestamp_ms: 3,
                cpu_percent: Some(f64::NAN),
                rx_bytes_per_sec: None,
                tx_bytes_per_sec: None,
            },
            ResourceTrendSample {
                timestamp_ms: 4,
                cpu_percent: Some(-1.0),
                rx_bytes_per_sec: None,
                tx_bytes_per_sec: None,
            },
            ResourceTrendSample {
                timestamp_ms: 5,
                cpu_percent: Some(20.0),
                rx_bytes_per_sec: None,
                tx_bytes_per_sec: Some(128),
            },
        ];
        for (count, expected) in [(0, false), (1, false), (4, false), (5, true)] {
            assert_eq!(
                (
                    has_cpu_trend_series(&samples[..count]),
                    has_network_trend_series(&samples[..count])
                ),
                (expected, expected),
                "history length {count}",
            );
        }
    }
}
