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
    Gpu,
    GpuMemory,
    LoadAverage,
    Rtt,
    Source,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MonitorSectionKind {
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CompactMonitorRow {
    Metric {
        kind: MonitorMetricKind,
        value: String,
        level: MonitorValueLevel,
    },
    Network {
        rx: String,
        tx: String,
    },
    Section {
        kind: MonitorSectionKind,
    },
    Detail {
        name: String,
        value: String,
        level: MonitorValueLevel,
    },
    /// Keeps network rates structured so compact UIs can choose their layout.
    Interface {
        name: String,
        rx: String,
        tx: String,
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

/// Builds the compact monitor rows from resource metrics without depending on GPUI types.
pub fn compact_monitor_rows(
    metrics: &ResourceMetrics,
    retry_connection_id: Option<String>,
) -> Vec<CompactMonitorRow> {
    let mut rows = Vec::new();

    push_compact_system_rows(metrics, &mut rows);
    if !resource_metrics_is_rtt_only(metrics) {
        push_compact_metric_rows(metrics, &mut rows);
        push_compact_detail_rows(metrics, &mut rows);
    }

    rows.push(CompactMonitorRow::Metric {
        kind: MonitorMetricKind::Rtt,
        value: metrics
            .ssh_rtt_ms
            .map(|rtt| format!("{rtt} ms"))
            .unwrap_or_else(dash),
        level: rtt_level(metrics.ssh_rtt_ms),
    });
    rows.push(CompactMonitorRow::Metric {
        kind: MonitorMetricKind::Source,
        value: metrics_source_label_key(metrics.source).to_string(),
        level: MonitorValueLevel::Muted,
    });
    if let Some(connection_id) = retry_connection_id {
        rows.push(CompactMonitorRow::Retry { connection_id });
    }
    rows
}

/// Produces a stable row identity without treating live metric values as row geometry.
pub fn compact_monitor_row_signature(row: &CompactMonitorRow) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        CompactMonitorRow::Metric { kind, .. } => {
            0_u8.hash(&mut hasher);
            kind.hash(&mut hasher);
        }
        CompactMonitorRow::Network { .. } => {
            1_u8.hash(&mut hasher);
        }
        CompactMonitorRow::Section { kind } => {
            2_u8.hash(&mut hasher);
            kind.hash(&mut hasher);
        }
        CompactMonitorRow::Detail { name, .. } => {
            3_u8.hash(&mut hasher);
            name.hash(&mut hasher);
        }
        CompactMonitorRow::Interface { name, .. } => {
            4_u8.hash(&mut hasher);
            name.hash(&mut hasher);
        }
        CompactMonitorRow::Retry { connection_id } => {
            5_u8.hash(&mut hasher);
            connection_id.hash(&mut hasher);
        }
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

pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
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

fn push_compact_metric_rows(metrics: &ResourceMetrics, rows: &mut Vec<CompactMonitorRow>) {
    if let Some(cpu) = metrics.cpu_percent {
        rows.push(CompactMonitorRow::Metric {
            kind: MonitorMetricKind::Cpu,
            value: format!("{cpu:.1}%"),
            level: percent_level(Some(cpu)),
        });
    }
    if let (Some(used), Some(total)) = (metrics.memory_used, metrics.memory_total) {
        rows.push(CompactMonitorRow::Metric {
            kind: MonitorMetricKind::Memory,
            value: format!("{} / {}", format_bytes(used), format_bytes(total)),
            level: percent_level(metrics.memory_percent),
        });
    }
    if let (Some(used), Some(total)) = (metrics.swap_used, metrics.swap_total) {
        rows.push(CompactMonitorRow::Metric {
            kind: MonitorMetricKind::Swap,
            value: format!("{} / {}", format_bytes(used), format_bytes(total)),
            level: percent_level(metrics.swap_percent),
        });
    }
    if let (Some(used), Some(total)) = (metrics.disk_used, metrics.disk_total) {
        rows.push(CompactMonitorRow::Metric {
            kind: MonitorMetricKind::Disk,
            value: format!("{} / {}", format_bytes(used), format_bytes(total)),
            level: percent_level(metrics.disk_percent),
        });
    }
    if let Some(gpu_utilization) = gpu_utilization_percent(metrics) {
        rows.push(CompactMonitorRow::Metric {
            kind: MonitorMetricKind::Gpu,
            value: format!("{gpu_utilization:.1}%"),
            level: percent_level(Some(gpu_utilization)),
        });
    }
    if let Some(summary) = gpu_memory_summary(metrics) {
        rows.push(CompactMonitorRow::Metric {
            kind: MonitorMetricKind::GpuMemory,
            value: format!(
                "{} / {}",
                format_bytes(summary.used),
                format_bytes(summary.total)
            ),
            level: percent_level(summary.percent),
        });
    }
    if let Some(load) = metrics.load_avg_1 {
        rows.push(CompactMonitorRow::Metric {
            kind: MonitorMetricKind::LoadAverage,
            value: format!(
                "{load:.2} / {:.2} / {:.2}",
                metrics.load_avg_5.unwrap_or_default(),
                metrics.load_avg_15.unwrap_or_default()
            ),
            level: MonitorValueLevel::Muted,
        });
    }
    if metrics.net_rx_bytes_per_sec.is_some() || metrics.net_tx_bytes_per_sec.is_some() {
        rows.push(CompactMonitorRow::Network {
            rx: format_rate(metrics.net_rx_bytes_per_sec.unwrap_or_default()),
            tx: format_rate(metrics.net_tx_bytes_per_sec.unwrap_or_default()),
        });
    }
}

fn push_compact_system_rows(metrics: &ResourceMetrics, rows: &mut Vec<CompactMonitorRow>) {
    let Some(system_info) = metrics.system_info.as_ref() else {
        return;
    };
    let mut push_value = |kind, value: Option<String>| {
        if let Some(value) = value {
            rows.push(CompactMonitorRow::Metric {
                kind,
                value,
                level: MonitorValueLevel::Normal,
            });
        }
    };

    push_value(MonitorMetricKind::System, system_info.system_name.clone());
    push_value(
        MonitorMetricKind::SystemVersion,
        system_info.system_version.clone(),
    );
    push_value(
        MonitorMetricKind::Architecture,
        system_info.architecture.clone(),
    );
    push_value(
        MonitorMetricKind::BootTime,
        system_info.boot_time_ms.and_then(format_boot_time),
    );
    push_value(
        MonitorMetricKind::Uptime,
        system_info.uptime_seconds.map(format_uptime),
    );
}

fn push_compact_detail_rows(metrics: &ResourceMetrics, rows: &mut Vec<CompactMonitorRow>) {
    push_section_rows(rows, MonitorSectionKind::Mounts, disk_list_rows(metrics, 8));
    push_section_rows(rows, MonitorSectionKind::Gpus, gpu_list_rows(metrics, 4));
    push_compact_interface_rows(metrics, rows);
    push_section_rows(
        rows,
        MonitorSectionKind::TopProcesses,
        top_process_list_rows(metrics, 8),
    );
}

fn push_section_rows(
    rows: &mut Vec<CompactMonitorRow>,
    kind: MonitorSectionKind,
    details: Vec<MonitorListRow>,
) {
    if details.is_empty() {
        return;
    }
    rows.push(CompactMonitorRow::Section { kind });
    for detail in details {
        rows.push(CompactMonitorRow::Detail {
            name: detail.name,
            value: detail.value,
            level: detail.level,
        });
    }
}

fn push_compact_interface_rows(metrics: &ResourceMetrics, rows: &mut Vec<CompactMonitorRow>) {
    if metrics.net_interfaces.is_empty() {
        return;
    }
    rows.push(CompactMonitorRow::Section {
        kind: MonitorSectionKind::Interfaces,
    });
    for interface in metrics.net_interfaces.iter().take(8) {
        // Keep rates as separate fields so narrow UIs can stack them without
        // parsing a presentation string back into structured data.
        rows.push(CompactMonitorRow::Interface {
            name: interface.name.clone(),
            rx: interface
                .rx_bytes_per_sec
                .map(format_rate)
                .unwrap_or_else(dash),
            tx: interface
                .tx_bytes_per_sec
                .map(format_rate)
                .unwrap_or_else(dash),
        });
    }
}

fn dash() -> String {
    "—".to_string()
}
