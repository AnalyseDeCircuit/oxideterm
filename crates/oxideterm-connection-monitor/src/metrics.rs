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
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetrics {
    pub timestamp_ms: u64,
    pub cpu_percent: Option<f64>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub memory_percent: Option<f64>,
    pub load_avg_1: Option<f64>,
    pub load_avg_5: Option<f64>,
    pub load_avg_15: Option<f64>,
    pub cpu_cores: Option<u32>,
    pub net_rx_bytes_per_sec: Option<u64>,
    pub net_tx_bytes_per_sec: Option<u64>,
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
            load_avg_1: None,
            load_avg_5: None,
            load_avg_15: None,
            cpu_cores: None,
            net_rx_bytes_per_sec: None,
            net_tx_bytes_per_sec: None,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviousResourceSample {
    pub cpu: CpuSnapshot,
    pub net: NetSnapshot,
    pub timestamp_ms: u64,
}

pub fn parse_resource_metrics(
    output: &str,
    previous: Option<&PreviousResourceSample>,
    timestamp_ms: u64,
) -> ResourceMetrics {
    let cpu_snap = parse_cpu_snapshot(output);
    let net_snap = parse_net_snapshot(output);
    let mem = parse_meminfo(output);
    let load = parse_loadavg(output);
    let nproc = parse_nproc(output);

    let cpu_percent = match (&cpu_snap, previous) {
        (Some(current), Some(previous)) => {
            let total_delta = current.total().saturating_sub(previous.cpu.total());
            let active_delta = current.active().saturating_sub(previous.cpu.active());
            if total_delta > 0 {
                Some((active_delta as f64 / total_delta as f64) * 100.0)
            } else {
                None
            }
        }
        _ => None,
    };

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

    let (memory_used, memory_total, memory_percent) = match mem {
        Some((used, total)) => {
            let percent = if total > 0 {
                Some((used as f64 / total as f64) * 100.0)
            } else {
                None
            };
            (Some(used), Some(total), percent)
        }
        None => (None, None, None),
    };

    let source = match (cpu_snap.is_some(), mem.is_some(), load.is_some()) {
        (true, true, true) => MetricsSource::Full,
        (true, _, _) | (_, true, _) | (_, _, true) => MetricsSource::Partial,
        _ => MetricsSource::RttOnly,
    };

    ResourceMetrics {
        timestamp_ms,
        cpu_percent,
        memory_used,
        memory_total,
        memory_percent,
        load_avg_1: load.map(|(one, _, _)| one),
        load_avg_5: load.map(|(_, five, _)| five),
        load_avg_15: load.map(|(_, _, fifteen)| fifteen),
        cpu_cores: nproc,
        net_rx_bytes_per_sec: net_rx_rate,
        net_tx_bytes_per_sec: net_tx_rate,
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
        net: parse_net_snapshot(output).unwrap_or_default(),
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
    let line = section.lines().next()?;
    if !line.starts_with("cpu ") {
        return None;
    }
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 9 {
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

pub fn parse_meminfo(output: &str) -> Option<(u64, u64)> {
    let section = extract_section(output, "MEMINFO")?;
    let mut total_kb = None;
    let mut available_kb = None;

    for line in section.lines() {
        if line.starts_with("MemTotal:") {
            total_kb = extract_kb_value(line);
        } else if line.starts_with("MemAvailable:") {
            available_kb = extract_kb_value(line);
        }
        if total_kb.is_some() && available_kb.is_some() {
            break;
        }
    }

    let total = total_kb? * 1024;
    let available = available_kb? * 1024;
    Some((total.saturating_sub(available), total))
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
    let section = extract_section(output, "NETDEV")?;
    let mut total_rx = 0_u64;
    let mut total_tx = 0_u64;
    let mut found = false;

    for line in section.lines() {
        let line = line.trim();
        if line.contains('|') || line.is_empty() {
            continue;
        }
        if let Some((iface, rest)) = line.split_once(':') {
            if iface.trim() == "lo" {
                continue;
            }
            let parts = rest.split_whitespace().collect::<Vec<_>>();
            if parts.len() >= 9 {
                if let (Ok(rx), Ok(tx)) = (parts[0].parse::<u64>(), parts[8].parse::<u64>()) {
                    total_rx += rx;
                    total_tx += tx;
                    found = true;
                }
            }
        }
    }

    found.then_some(NetSnapshot {
        rx_bytes: total_rx,
        tx_bytes: total_tx,
    })
}

pub fn parse_nproc(output: &str) -> Option<u32> {
    let section = extract_section(output, "NPROC")?;
    section.lines().next()?.trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = r#"===STAT===
cpu  10000100 290000 3000050 46000200 16000 0 25000 0 0 0
===MEMINFO===
MemTotal:       16384000 kB
MemAvailable:   8192000 kB
===LOADAVG===
0.52 0.61 0.74 1/123 4567
===NETDEV===
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1000 0 0 0 0 0 0 0 2000 0 0 0 0 0 0 0
  eth0: 987654321 0 0 0 0 0 0 0 123456789 0 0 0 0 0 0 0
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
        assert_eq!(metrics.load_avg_1, Some(0.52));
        assert_eq!(metrics.cpu_cores, Some(4));
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
            net: NetSnapshot {
                rx_bytes: 900_000_000,
                tx_bytes: 100_000_000,
            },
            timestamp_ms: 5_000,
        };

        let metrics = parse_resource_metrics(SAMPLE_OUTPUT, Some(&previous), 10_000);

        assert!(metrics.cpu_percent.is_some());
        assert_eq!(metrics.net_rx_bytes_per_sec, Some(17_530_864));
        assert_eq!(metrics.net_tx_bytes_per_sec, Some(4_691_357));
    }

    #[test]
    fn partial_metrics_keep_tauri_source_semantics() {
        let output = "===MEMINFO===\nMemTotal: 1024 kB\nMemAvailable: 512 kB\n===END===";
        let metrics = parse_resource_metrics(output, None, 1);

        assert_eq!(metrics.source, MetricsSource::Partial);
        assert_eq!(metrics.memory_used, Some(512 * 1024));
        assert_eq!(metrics.cpu_percent, None);
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
