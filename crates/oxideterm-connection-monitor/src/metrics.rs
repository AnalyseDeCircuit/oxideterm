// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

pub const RESOURCE_HISTORY_CAPACITY: usize = 60;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricsSource {
    Full,
    Partial,
    RttOnly,
    Failed,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceCpuCore {
    pub index: u32,
    pub percent: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDisk {
    pub mount_point: String,
    pub used: u64,
    pub total: u64,
    pub percent: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceNetInterface {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_bytes_per_sec: Option<u64>,
    pub tx_bytes_per_sec: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceTopProcess {
    pub pid: String,
    pub memory_percent: f64,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetrics {
    pub timestamp_ms: u64,
    pub cpu_percent: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub memory_percent: Option<f64>,
    pub memory_buffers: Option<u64>,
    pub memory_cached: Option<u64>,
    pub swap_used: Option<u64>,
    pub swap_total: Option<u64>,
    pub swap_percent: Option<f64>,
    pub disk_used: Option<u64>,
    pub disk_total: Option<u64>,
    pub disk_percent: Option<f64>,
    pub load_avg_1: Option<f64>,
    pub load_avg_5: Option<f64>,
    pub load_avg_15: Option<f64>,
    pub cpu_cores: Option<u32>,
    pub cpu_per_core: Vec<ResourceCpuCore>,
    pub disks: Vec<ResourceDisk>,
    pub net_rx_bytes_per_sec: Option<u64>,
    pub net_tx_bytes_per_sec: Option<u64>,
    pub net_interfaces: Vec<ResourceNetInterface>,
    pub top_processes: Vec<ResourceTopProcess>,
    pub ssh_rtt_ms: Option<u64>,
    pub source: MetricsSource,
}

impl ResourceMetrics {
    pub fn empty(timestamp_ms: u64, source: MetricsSource) -> Self {
        Self {
            timestamp_ms,
            cpu_percent: None,
            memory_used: None,
            memory_total: None,
            memory_percent: None,
            memory_buffers: None,
            memory_cached: None,
            swap_used: None,
            swap_total: None,
            swap_percent: None,
            disk_used: None,
            disk_total: None,
            disk_percent: None,
            load_avg_1: None,
            load_avg_5: None,
            load_avg_15: None,
            cpu_cores: None,
            cpu_per_core: Vec::new(),
            disks: Vec::new(),
            net_rx_bytes_per_sec: None,
            net_tx_bytes_per_sec: None,
            net_interfaces: Vec::new(),
            top_processes: Vec::new(),
            ssh_rtt_ms: None,
            source,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CpuSnapshot {
    pub user: u64,
    pub nice: u64,
    pub system: u64,
    pub idle: u64,
    pub iowait: u64,
    pub irq: u64,
    pub softirq: u64,
    pub steal: u64,
}

impl CpuSnapshot {
    pub fn total(&self) -> u64 {
        self.user
            .saturating_add(self.nice)
            .saturating_add(self.system)
            .saturating_add(self.idle)
            .saturating_add(self.iowait)
            .saturating_add(self.irq)
            .saturating_add(self.softirq)
            .saturating_add(self.steal)
    }

    pub fn active(&self) -> u64 {
        self.total()
            .saturating_sub(self.idle)
            .saturating_sub(self.iowait)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NetSnapshot {
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NetInterfaceSnapshot {
    pub name: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviousResourceSample {
    pub cpu: CpuSnapshot,
    pub cpu_per_core: Vec<CpuSnapshot>,
    pub net: NetSnapshot,
    pub net_interfaces: Vec<NetInterfaceSnapshot>,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemorySnapshot {
    pub used: u64,
    pub total: u64,
    pub buffers: Option<u64>,
    pub cached: Option<u64>,
    pub swap_used: Option<u64>,
    pub swap_total: Option<u64>,
}

pub fn parse_resource_metrics(
    output: &str,
    previous: Option<&PreviousResourceSample>,
    timestamp_ms: u64,
) -> ResourceMetrics {
    if extract_section(output, "UNSUPPORTED").is_some() {
        return ResourceMetrics::empty(timestamp_ms, MetricsSource::Unsupported);
    }

    let cpu_snap = parse_cpu_snapshot(output);
    let cpu_direct = parse_cpu_direct(output);
    let cpu_core_snaps = parse_cpu_core_snapshots(output);
    let net_snap = parse_net_snapshot(output);
    let net_interface_snaps = parse_net_interface_snapshots(output);
    let mem = parse_memory_snapshot(output);
    let disks = parse_disks(output);
    let disk = parse_root_disk_usage(&disks).or_else(|| parse_disk_usage_legacy(output));
    let load = parse_loadavg(output);
    let nproc = parse_nproc(output);
    let top_processes = parse_top_processes(output);
    let has_memory = mem.is_some();

    let cpu_percent = match (&cpu_snap, previous) {
        (Some(current), Some(previous)) => cpu_usage_percent(current, &previous.cpu),
        _ => cpu_direct,
    };

    let cpu_per_core = cpu_core_snaps
        .iter()
        .enumerate()
        .map(|(index, current)| {
            let percent = previous
                .and_then(|previous| previous.cpu_per_core.get(index))
                .and_then(|previous| cpu_usage_percent(current, previous));
            ResourceCpuCore {
                index: index as u32,
                percent,
            }
        })
        .collect::<Vec<_>>();

    let (net_rx_rate, net_tx_rate) = match (&net_snap, previous) {
        (Some(current), Some(previous)) => {
            let elapsed_ms = timestamp_ms.saturating_sub(previous.timestamp_ms);
            if elapsed_ms > 0 {
                let elapsed_secs = elapsed_ms as f64 / 1000.0;
                (
                    Some(
                        (current.rx_bytes.saturating_sub(previous.net.rx_bytes) as f64
                            / elapsed_secs) as u64,
                    ),
                    Some(
                        (current.tx_bytes.saturating_sub(previous.net.tx_bytes) as f64
                            / elapsed_secs) as u64,
                    ),
                )
            } else {
                (None, None)
            }
        }
        _ => (None, None),
    };

    let net_interfaces = net_interface_snaps
        .iter()
        .map(|current| {
            let (rx_bytes_per_sec, tx_bytes_per_sec) = previous
                .and_then(|previous| {
                    let previous_iface = previous
                        .net_interfaces
                        .iter()
                        .find(|iface| iface.name == current.name)?;
                    let elapsed_ms = timestamp_ms.saturating_sub(previous.timestamp_ms);
                    if elapsed_ms == 0 {
                        return None;
                    }
                    let elapsed_secs = elapsed_ms as f64 / 1000.0;
                    Some((
                        (current.rx_bytes.saturating_sub(previous_iface.rx_bytes) as f64
                            / elapsed_secs) as u64,
                        (current.tx_bytes.saturating_sub(previous_iface.tx_bytes) as f64
                            / elapsed_secs) as u64,
                    ))
                })
                .unwrap_or((0, 0));
            ResourceNetInterface {
                name: current.name.clone(),
                rx_bytes: current.rx_bytes,
                tx_bytes: current.tx_bytes,
                rx_bytes_per_sec: Some(rx_bytes_per_sec),
                tx_bytes_per_sec: Some(tx_bytes_per_sec),
            }
        })
        .collect::<Vec<_>>();

    let (
        memory_used,
        memory_total,
        memory_percent,
        memory_buffers,
        memory_cached,
        swap_used,
        swap_total,
        swap_percent,
    ) = match mem {
        Some(mem) => (
            Some(mem.used),
            Some(mem.total),
            percent(mem.used, mem.total),
            mem.buffers,
            mem.cached,
            mem.swap_used,
            mem.swap_total,
            match (mem.swap_used, mem.swap_total) {
                (Some(used), Some(total)) => percent(used, total),
                _ => None,
            },
        ),
        None => (None, None, None, None, None, None, None, None),
    };

    let (disk_used, disk_total, disk_percent) = match disk {
        Some((used, total)) => (Some(used), Some(total), percent(used, total)),
        None => (None, None, None),
    };

    let source = match (
        cpu_snap.is_some() || cpu_direct.is_some(),
        has_memory,
        load.is_some(),
        disk.is_some() || !disks.is_empty(),
    ) {
        (true, true, true, _) => MetricsSource::Full,
        (true, _, _, _) | (_, true, _, _) | (_, _, true, _) | (_, _, _, true) => {
            MetricsSource::Partial
        }
        _ => MetricsSource::RttOnly,
    };

    ResourceMetrics {
        timestamp_ms,
        cpu_percent,
        memory_used,
        memory_total,
        memory_percent,
        memory_buffers,
        memory_cached,
        swap_used,
        swap_total,
        swap_percent,
        disk_used,
        disk_total,
        disk_percent,
        load_avg_1: load.map(|(one, _, _)| one),
        load_avg_5: load.map(|(_, five, _)| five),
        load_avg_15: load.map(|(_, _, fifteen)| fifteen),
        cpu_cores: nproc,
        cpu_per_core,
        disks,
        net_rx_bytes_per_sec: net_rx_rate,
        net_tx_bytes_per_sec: net_tx_rate,
        net_interfaces,
        top_processes,
        ssh_rtt_ms: None,
        source,
    }
}

pub fn previous_sample_from_metrics(
    metrics: &ResourceMetrics,
    output: &str,
) -> Option<PreviousResourceSample> {
    Some(PreviousResourceSample {
        cpu: parse_cpu_snapshot(output)?,
        cpu_per_core: parse_cpu_core_snapshots(output),
        net: parse_net_snapshot(output).unwrap_or_default(),
        net_interfaces: parse_net_interface_snapshots(output),
        timestamp_ms: metrics.timestamp_ms,
    })
}

pub fn push_history(history: &mut Vec<ResourceMetrics>, metrics: ResourceMetrics) {
    history.push(metrics);
    if history.len() > RESOURCE_HISTORY_CAPACITY {
        history.drain(0..history.len() - RESOURCE_HISTORY_CAPACITY);
    }
}

fn extract_section<'a>(output: &'a str, marker: &str) -> Option<&'a str> {
    let start_marker = format!("==={marker}===");
    let start = output.find(&start_marker)?;
    let after_marker = start + start_marker.len();
    let rest = &output[after_marker..];
    let end = rest.find("===").unwrap_or(rest.len());
    Some(rest[..end].trim())
}

pub fn parse_cpu_snapshot(output: &str) -> Option<CpuSnapshot> {
    let section = extract_section(output, "STAT")?;
    let line = section.lines().find(|line| line.starts_with("cpu "))?;
    parse_cpu_line(line)
}

fn parse_cpu_line(line: &str) -> Option<CpuSnapshot> {
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 9 || !parts[0].starts_with("cpu") {
        return None;
    }
    Some(CpuSnapshot {
        user: parts[1].parse().ok()?,
        nice: parts[2].parse().ok()?,
        system: parts[3].parse().ok()?,
        idle: parts[4].parse().ok()?,
        iowait: parts[5].parse().ok()?,
        irq: parts[6].parse().ok()?,
        softirq: parts[7].parse().ok()?,
        steal: parts[8].parse().ok()?,
    })
}

fn parse_cpu_core_snapshots(output: &str) -> Vec<CpuSnapshot> {
    let Some(section) = extract_section(output, "STAT") else {
        return Vec::new();
    };
    section
        .lines()
        .filter_map(|line| {
            let label = line.split_whitespace().next()?;
            if label.len() > 3
                && label.starts_with("cpu")
                && label[3..].chars().all(|ch| ch.is_ascii_digit())
            {
                parse_cpu_line(line)
            } else {
                None
            }
        })
        .collect()
}

fn parse_cpu_direct(output: &str) -> Option<f64> {
    extract_section(output, "CPU_DIRECT")?
        .lines()
        .find_map(|line| line.trim().parse::<f64>().ok())
        .map(|value| value.clamp(0.0, 100.0))
}

fn cpu_usage_percent(current: &CpuSnapshot, previous: &CpuSnapshot) -> Option<f64> {
    let total_delta = current.total().saturating_sub(previous.total());
    let active_delta = current.active().saturating_sub(previous.active());
    if total_delta > 0 {
        Some((active_delta as f64 / total_delta as f64) * 100.0)
    } else {
        None
    }
}

pub fn parse_meminfo(output: &str) -> Option<(u64, u64)> {
    let mem = parse_memory_snapshot(output)?;
    Some((mem.used, mem.total))
}

pub fn parse_memory_snapshot(output: &str) -> Option<MemorySnapshot> {
    let section = extract_section(output, "MEMINFO")?;
    let mut total_kb = None;
    let mut available_kb = None;
    let mut free_kb = None;
    let mut buffers_kb = None;
    let mut cached_kb = None;
    let mut reclaimable_kb = None;
    let mut swap_total_kb = None;
    let mut swap_free_kb = None;

    for line in section.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = extract_kb_value(line);
        } else if line.starts_with("MemAvailable:") {
            available_kb = extract_kb_value(line);
        } else if line.starts_with("MemFree:") {
            free_kb = extract_kb_value(line);
        } else if line.starts_with("Buffers:") {
            buffers_kb = extract_kb_value(line);
        } else if line.starts_with("Cached:") {
            cached_kb = extract_kb_value(line);
        } else if line.starts_with("SReclaimable:") {
            reclaimable_kb = extract_kb_value(line);
        } else if line.starts_with("SwapTotal:") {
            swap_total_kb = extract_kb_value(line);
        } else if line.starts_with("SwapFree:") {
            swap_free_kb = extract_kb_value(line);
        }
    }

    let total = total_kb? * 1024;
    let available = available_kb.or_else(|| {
        Some(
            free_kb?
                .saturating_add(buffers_kb.unwrap_or_default())
                .saturating_add(cached_kb.unwrap_or_default())
                .saturating_add(reclaimable_kb.unwrap_or_default()),
        )
    })? * 1024;
    let cached = cached_kb
        .unwrap_or_default()
        .saturating_add(reclaimable_kb.unwrap_or_default());
    let swap_total = swap_total_kb.map(|value| value * 1024);
    let swap_free = swap_free_kb.map(|value| value * 1024);
    Some(MemorySnapshot {
        used: total.saturating_sub(available),
        total,
        buffers: buffers_kb.map(|value| value * 1024),
        cached: Some(cached * 1024).filter(|value| *value > 0),
        swap_used: match (swap_total, swap_free) {
            (Some(total), Some(free)) => Some(total.saturating_sub(free)),
            _ => None,
        },
        swap_total,
    })
}

fn extract_kb_value(line: &str) -> Option<u64> {
    line.split_whitespace().nth(1)?.parse().ok()
}

pub fn parse_loadavg(output: &str) -> Option<(f64, f64, f64)> {
    let section = extract_section(output, "LOADAVG")?;
    let line = section.lines().next()?;
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

pub fn parse_net_snapshot(output: &str) -> Option<NetSnapshot> {
    let interfaces = parse_net_interface_snapshots(output);
    if interfaces.is_empty() {
        return None;
    }
    Some(NetSnapshot {
        rx_bytes: interfaces.iter().map(|iface| iface.rx_bytes).sum(),
        tx_bytes: interfaces.iter().map(|iface| iface.tx_bytes).sum(),
    })
}

fn parse_net_interface_snapshots(output: &str) -> Vec<NetInterfaceSnapshot> {
    let Some(section) = extract_section(output, "NETDEV") else {
        return Vec::new();
    };
    section
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.contains('|') || line.is_empty() {
                return None;
            }
            let (iface, rest) = line.split_once(':')?;
            let name = iface.trim();
            if should_skip_net_interface(name) {
                return None;
            }
            let parts = rest.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 9 {
                return None;
            }
            Some(NetInterfaceSnapshot {
                name: name.to_string(),
                rx_bytes: parts[0].parse().ok()?,
                tx_bytes: parts[8].parse().ok()?,
            })
        })
        .collect()
}

fn should_skip_net_interface(name: &str) -> bool {
    name == "lo"
        || name.starts_with("veth")
        || name.starts_with("docker")
        || name.starts_with("br-")
        || name.starts_with("virbr")
        || name.starts_with("cni")
        || name.starts_with("flannel")
}

pub fn parse_nproc(output: &str) -> Option<u32> {
    let section = extract_section(output, "NPROC")?;
    section.lines().next()?.trim().parse().ok()
}

pub fn parse_disk_usage(output: &str) -> Option<(u64, u64)> {
    parse_root_disk_usage(&parse_disks(output)).or_else(|| parse_disk_usage_legacy(output))
}

fn parse_disk_usage_legacy(output: &str) -> Option<(u64, u64)> {
    let section = extract_section(output, "DISK")?;
    let line = section.lines().find(|line| !line.trim().is_empty())?;
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let total_kib = parts[1].parse::<u64>().ok()?;
    let used_kib = parts[2].parse::<u64>().ok()?;
    Some((
        used_kib.saturating_mul(1024),
        total_kib.saturating_mul(1024),
    ))
}

pub fn parse_disks(output: &str) -> Vec<ResourceDisk> {
    let Some(section) = extract_section(output, "DISKS") else {
        return Vec::new();
    };
    section
        .lines()
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            if parts.len() >= 4 {
                let mount_point = parts[0].trim();
                let used = parts[1].trim().parse::<u64>().ok()?;
                let total = parts[2].trim().parse::<u64>().ok()?;
                let percent_value = parts[3].trim().parse::<f64>().ok();
                if mount_point.is_empty() || total == 0 {
                    return None;
                }
                return Some(ResourceDisk {
                    mount_point: mount_point.to_string(),
                    used,
                    total,
                    percent: percent_value.or_else(|| percent(used, total)),
                });
            }

            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 6 || !parts[0].starts_with("/dev") {
                return None;
            }
            let total = parts[1].parse::<u64>().ok()?.saturating_mul(1024);
            let used = parts[2].parse::<u64>().ok()?.saturating_mul(1024);
            let percent_value = parts[4].trim_end_matches('%').parse::<f64>().ok();
            Some(ResourceDisk {
                mount_point: parts[5].to_string(),
                used,
                total,
                percent: percent_value.or_else(|| percent(used, total)),
            })
        })
        .collect()
}

fn parse_root_disk_usage(disks: &[ResourceDisk]) -> Option<(u64, u64)> {
    let root = disks
        .iter()
        .find(|disk| disk.mount_point == "/")
        .or_else(|| disks.first())?;
    Some((root.used, root.total))
}

pub fn parse_top_processes(output: &str) -> Vec<ResourceTopProcess> {
    let Some(section) = extract_section(output, "TOPPROCS") else {
        return Vec::new();
    };
    section
        .lines()
        .filter_map(|line| {
            let parts = line.splitn(3, '\t').collect::<Vec<_>>();
            if parts.len() < 3 {
                return None;
            }
            let memory_percent = parts[1].trim().parse::<f64>().ok()?;
            Some(ResourceTopProcess {
                pid: parts[0].trim().to_string(),
                memory_percent,
                command: parts[2].trim().to_string(),
            })
        })
        .collect()
}

fn percent(used: u64, total: u64) -> Option<f64> {
    if total > 0 {
        Some((used as f64 / total as f64) * 100.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = r#"===STAT===
cpu  10000100 290000 3000050 46000200 16000 0 25000 0 0 0
cpu0 5000100 145000 1500050 23000200 8000 0 12000 0 0 0
cpu1 5000000 145000 1500000 23000000 8000 0 13000 0 0 0
===MEMINFO===
MemTotal:       16384000 kB
MemAvailable:   8192000 kB
Buffers:          64000 kB
Cached:         1024000 kB
SReclaimable:    128000 kB
SwapTotal:      2097152 kB
SwapFree:       1048576 kB
===LOADAVG===
0.52 0.61 0.74 1/123 4567
===NETDEV===
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0
  eth0: 987654321 0 0 0 0 0 0 0 123456789 0 0 0 0 0 0 0
  docker0: 777 0 0 0 0 0 0 0 888 0 0 0 0 0 0 0
===DISKS===
/	53687091200	107374182400	50
/data	10737418240	53687091200	20
===TOPPROCS===
123	12.5	postgres
456	8.0	rust-analyzer
===NPROC===
4
===END==="#;

    #[test]
    fn parses_full_metrics_without_first_sample_delta() {
        let metrics = parse_resource_metrics(SAMPLE_OUTPUT, None, 10_000);

        assert_eq!(metrics.source, MetricsSource::Full);
        assert_eq!(metrics.cpu_percent, None);
        assert_eq!(metrics.memory_used, Some(8_192_000 * 1024));
        assert_eq!(metrics.memory_total, Some(16_384_000 * 1024));
        assert_eq!(metrics.memory_buffers, Some(64_000 * 1024));
        assert_eq!(metrics.memory_cached, Some(1_152_000 * 1024));
        assert_eq!(metrics.swap_used, Some(1_048_576 * 1024));
        assert_eq!(metrics.disk_used, Some(53_687_091_200));
        assert_eq!(metrics.disk_total, Some(107_374_182_400));
        assert_eq!(metrics.disk_percent, Some(50.0));
        assert_eq!(metrics.disks.len(), 2);
        assert_eq!(metrics.top_processes.len(), 2);
        assert_eq!(metrics.net_interfaces.len(), 1);
        assert_eq!(metrics.load_avg_1, Some(0.52));
        assert_eq!(metrics.cpu_cores, Some(4));
        assert_eq!(metrics.cpu_per_core.len(), 2);
        assert_eq!(metrics.net_rx_bytes_per_sec, None);
        assert_eq!(metrics.net_tx_bytes_per_sec, None);
    }

    #[test]
    fn parses_cpu_and_network_delta_like_tauri() {
        let previous = PreviousResourceSample {
            cpu: CpuSnapshot {
                user: 10_000_000,
                nice: 290_000,
                system: 3_000_000,
                idle: 46_000_000,
                iowait: 16_000,
                irq: 0,
                softirq: 25_000,
                steal: 0,
            },
            cpu_per_core: vec![CpuSnapshot {
                user: 5_000_000,
                nice: 145_000,
                system: 1_500_000,
                idle: 23_000_000,
                iowait: 8_000,
                irq: 0,
                softirq: 12_000,
                steal: 0,
            }],
            net: NetSnapshot {
                rx_bytes: 900_000_000,
                tx_bytes: 100_000_000,
            },
            net_interfaces: vec![NetInterfaceSnapshot {
                name: "eth0".to_string(),
                rx_bytes: 900_000_000,
                tx_bytes: 100_000_000,
            }],
            timestamp_ms: 5_000,
        };

        let metrics = parse_resource_metrics(SAMPLE_OUTPUT, Some(&previous), 10_000);

        assert!(metrics.cpu_percent.is_some());
        assert!(metrics.cpu_per_core[0].percent.is_some());
        assert_eq!(metrics.net_rx_bytes_per_sec, Some(17_530_864));
        assert_eq!(metrics.net_tx_bytes_per_sec, Some(4_691_357));
        assert_eq!(metrics.net_interfaces[0].rx_bytes_per_sec, Some(17_530_864));
    }

    #[test]
    fn partial_metrics_keep_tauri_source_semantics() {
        let output = "===MEMINFO===\nMemTotal: 1024 kB\nMemAvailable: 512 kB\n===DISK===\n/dev/root 2048 1024 1024 50% /\n===END===";
        let metrics = parse_resource_metrics(output, None, 1);

        assert_eq!(metrics.source, MetricsSource::Partial);
        assert_eq!(metrics.memory_used, Some(512 * 1024));
        assert_eq!(metrics.disk_used, Some(1024 * 1024));
        assert_eq!(metrics.cpu_percent, None);
    }

    #[test]
    fn parses_root_disk_usage_from_df_posix_output() {
        let output = "===DISK===\n/dev/disk1s1 411528304 178655880 232872424 44% /\n===END===";

        assert_eq!(
            parse_disk_usage(output),
            Some((178_655_880 * 1024, 411_528_304 * 1024))
        );
    }

    #[test]
    fn unsupported_marker_is_explicit() {
        let metrics = parse_resource_metrics("===UNSUPPORTED===\nFreeBSD\n===END===", None, 1);

        assert_eq!(metrics.source, MetricsSource::Unsupported);
    }

    #[test]
    fn cpu_direct_supports_macos_and_windows_samples() {
        let output = "===CPU_DIRECT===\n37.5\n===MEMINFO===\nMemTotal: 1024 kB\nMemAvailable: 512 kB\n===END===";
        let metrics = parse_resource_metrics(output, None, 1);

        assert_eq!(metrics.cpu_percent, Some(37.5));
        assert_eq!(metrics.source, MetricsSource::Partial);
    }

    #[test]
    fn empty_metrics_are_rtt_only() {
        let metrics = parse_resource_metrics("", None, 1);

        assert_eq!(metrics.source, MetricsSource::RttOnly);
    }

    #[test]
    fn trims_history_to_tauri_capacity() {
        let mut history = Vec::new();
        for timestamp_ms in 0..65 {
            push_history(
                &mut history,
                ResourceMetrics::empty(timestamp_ms, MetricsSource::Failed),
            );
        }

        assert_eq!(history.len(), RESOURCE_HISTORY_CAPACITY);
        assert_eq!(history.first().map(|metrics| metrics.timestamp_ms), Some(5));
        assert_eq!(history.last().map(|metrics| metrics.timestamp_ms), Some(64));
    }
}
