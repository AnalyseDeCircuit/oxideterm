//! Resource Profiler Module
//!
//! Samples remote host resources (CPU, memory, load, network) via a persistent SSH shell channel.
//! Uses a single long-lived channel to avoid MaxSessions exhaustion.
//!
//! # Design
//! - One `ResourceProfiler` per connection, bound to SSH lifecycle via `subscribe_disconnect()`
//! - Opens ONE shell channel at startup, reuses it for all sampling cycles
//! - Collects `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/net/dev` via stdin commands
//! - CPU% and network rates require delta between two samples (first sample returns None)
//! - Non-Linux hosts gracefully degrade to `MetricsSource::RttOnly`
//!
//! # Invariants
//! - P1: Profiler does not hold strong references to the connection
//! - P2: SSH disconnect → profiler auto-stops via `disconnect_rx`
//! - P3: Only 1 shell channel held for the entire profiler lifetime
//! - P5: First sample returns None for CPU/network (no delta baseline)

use std::collections::VecDeque;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use russh::client::Msg;
use russh::{Channel, ChannelMsg};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::broadcast;
use tokio::time::{timeout, Duration};
use tracing::{debug, trace, warn};

use crate::session::health::{MetricsSource, ResourceMetrics};
use crate::ssh::HandleController;

/// Maximum number of history points kept (ring buffer)
const HISTORY_CAPACITY: usize = 60;

/// Maximum output size from a single sample (8KB — slimmed command output is ~1KB)
const MAX_OUTPUT_SIZE: usize = 8_192;

/// Timeout for reading a single sample's output from the shell channel
const SAMPLE_TIMEOUT: Duration = Duration::from_secs(5);

/// Default sampling interval (10s to minimise SSH bandwidth contention with PTY)
const DEFAULT_INTERVAL: Duration = Duration::from_secs(10);

/// Number of consecutive failures before degrading to RttOnly
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Timeout for opening the initial shell channel
const CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// Slimmed sampling command — only reads the minimum data needed:
/// - `head -1 /proc/stat` → CPU total line only (~80 bytes, skips per-core lines)
/// - `grep` MemTotal + MemAvailable → 2 lines (~60 bytes, skips full meminfo)
/// - `/proc/loadavg` → 1 line (~30 bytes)
/// - `/proc/net/dev` is small and needed in full for multi-interface aggregation
/// - `nproc` → 1 number
/// Total output: ~500-1500 bytes (was ~10-30KB with full /proc/stat + /proc/meminfo)
const SAMPLE_COMMAND: &str = "echo '===STAT==='; head -1 /proc/stat 2>/dev/null; echo '===MEMINFO==='; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo 2>/dev/null; echo '===LOADAVG==='; cat /proc/loadavg 2>/dev/null; echo '===NETDEV==='; cat /proc/net/dev 2>/dev/null; echo '===NPROC==='; nproc 2>/dev/null; echo '===END==='\n";

/// Raw CPU counters from /proc/stat
#[derive(Debug, Clone, Default)]
struct CpuSnapshot {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
    iowait: u64,
    irq: u64,
    softirq: u64,
    steal: u64,
}

impl CpuSnapshot {
    fn total(&self) -> u64 {
        self.user
            + self.nice
            + self.system
            + self.idle
            + self.iowait
            + self.irq
            + self.softirq
            + self.steal
    }

    fn active(&self) -> u64 {
        self.total() - self.idle - self.iowait
    }
}

/// Raw network counters from /proc/net/dev
#[derive(Debug, Clone, Default)]
struct NetSnapshot {
    rx_bytes: u64,
    tx_bytes: u64,
}

/// Previous sample state for delta calculations
#[derive(Debug, Clone)]
struct PreviousSample {
    cpu: CpuSnapshot,
    net: NetSnapshot,
    timestamp_ms: u64,
}

/// Profiler running state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfilerState {
    Running,
    Stopped,
    Degraded,
}

/// Resource profiler for a single SSH connection
pub struct ResourceProfiler {
    connection_id: String,
    state: Arc<RwLock<ProfilerState>>,
    latest: Arc<RwLock<Option<ResourceMetrics>>>,
    history: Arc<RwLock<VecDeque<ResourceMetrics>>>,
    /// Sender to signal the sampling loop to stop
    stop_tx: Option<tokio::sync::oneshot::Sender<()>>,
}

impl ResourceProfiler {
    /// Spawn a new profiler that samples the remote host via the given controller.
    ///
    /// The profiler automatically stops when:
    /// 1. `stop()` is called
    /// 2. The SSH connection disconnects (via `disconnect_rx`)
    pub fn spawn(
        connection_id: String,
        controller: HandleController,
        app_handle: tauri::AppHandle,
    ) -> Self {
        let state = Arc::new(RwLock::new(ProfilerState::Running));
        let latest = Arc::new(RwLock::new(None));
        let history = Arc::new(RwLock::new(VecDeque::with_capacity(HISTORY_CAPACITY)));
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let profiler = Self {
            connection_id: connection_id.clone(),
            state: state.clone(),
            latest: latest.clone(),
            history: history.clone(),
            stop_tx: Some(stop_tx),
        };

        // Subscribe to SSH disconnect
        let mut disconnect_rx = controller.subscribe_disconnect();

        // Spawn the sampling loop
        let state_clone = state.clone();
        let latest_clone = latest.clone();
        let history_clone = history.clone();
        let conn_id = connection_id.clone();

        tokio::spawn(async move {
            sampling_loop(
                conn_id,
                controller,
                state_clone,
                latest_clone,
                history_clone,
                stop_rx,
                &mut disconnect_rx,
                app_handle,
            )
            .await;
        });

        profiler
    }

    /// Get the latest metrics snapshot
    pub async fn latest(&self) -> Option<ResourceMetrics> {
        self.latest.read().unwrap().clone()
    }

    /// Get metrics history for sparkline rendering
    pub async fn history(&self) -> Vec<ResourceMetrics> {
        self.history.read().unwrap().iter().cloned().collect()
    }

    /// Get current profiler state
    pub async fn state(&self) -> ProfilerState {
        *self.state.read().unwrap()
    }

    /// Stop the profiler
    pub fn stop(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }

    /// Connection ID this profiler is bound to
    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }
}

/// The main sampling loop. Runs until stopped or disconnected.
///
/// Opens ONE persistent shell channel at startup and reuses it for all samples.
/// This avoids MaxSessions exhaustion on servers with low limits.
async fn sampling_loop(
    connection_id: String,
    controller: HandleController,
    state: Arc<RwLock<ProfilerState>>,
    latest: Arc<RwLock<Option<ResourceMetrics>>>,
    history: Arc<RwLock<VecDeque<ResourceMetrics>>>,
    mut stop_rx: tokio::sync::oneshot::Receiver<()>,
    disconnect_rx: &mut broadcast::Receiver<()>,
    app_handle: tauri::AppHandle,
) {
    let mut prev_sample: Option<PreviousSample> = None;
    let mut consecutive_failures: u32 = 0;
    let mut interval = tokio::time::interval(DEFAULT_INTERVAL);
    // Skip the immediate first tick
    interval.tick().await;

    debug!("Resource profiler started for connection {}", connection_id);

    // Open persistent shell channel
    let mut shell_channel = match open_shell_channel(&controller).await {
        Ok(ch) => ch,
        Err(e) => {
            warn!(
                "Profiler failed to open shell channel for {}: {}",
                connection_id, e
            );
            *state.write().unwrap() = ProfilerState::Degraded;
            // Emit degraded metrics so frontend knows
            let metrics = make_empty_metrics(MetricsSource::RttOnly);
            store_metrics(&latest, &history, &metrics);
            emit_metrics(&app_handle, &connection_id, &metrics);
            return;
        }
    };

    loop {
        tokio::select! {
            _ = interval.tick() => {
                // Degraded mode: only emit RTT-only metrics
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    let current_state = *state.read().unwrap();
                    if current_state != ProfilerState::Degraded {
                        *state.write().unwrap() = ProfilerState::Degraded;
                        warn!(
                            "Resource profiler degraded for {} after {} consecutive failures",
                            connection_id, consecutive_failures
                        );
                    }
                    let metrics = make_empty_metrics(MetricsSource::RttOnly);
                    store_metrics(&latest, &history, &metrics);
                    emit_metrics(&app_handle, &connection_id, &metrics);
                    continue;
                }

                // Execute sampling command on persistent shell
                match shell_sample(&mut shell_channel).await {
                    Ok(output) => {
                        consecutive_failures = 0;
                        let metrics = parse_metrics(&output, &prev_sample);

                        let cpu = parse_cpu_snapshot(&output);
                        let net = parse_net_snapshot(&output);
                        prev_sample = Some(PreviousSample {
                            cpu: cpu.unwrap_or_default(),
                            net: net.unwrap_or_default(),
                            timestamp_ms: metrics.timestamp_ms,
                        });

                        store_metrics(&latest, &history, &metrics);
                        emit_metrics(&app_handle, &connection_id, &metrics);
                        trace!("Profiler sample for {}: source={:?}", connection_id, metrics.source);
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        warn!(
                            "Profiler sample failed for {} ({}/{}): {}",
                            connection_id, consecutive_failures, MAX_CONSECUTIVE_FAILURES, e
                        );

                        // Try to reopen the shell channel once
                        if let Ok(new_ch) = open_shell_channel(&controller).await {
                            shell_channel = new_ch;
                            debug!("Profiler reopened shell channel for {}", connection_id);
                        }

                        let failed_metrics = make_empty_metrics(MetricsSource::Failed);
                        store_metrics(&latest, &history, &failed_metrics);
                        emit_metrics(&app_handle, &connection_id, &failed_metrics);
                    }
                }
            }
            _ = disconnect_rx.recv() => {
                debug!("SSH disconnected, stopping profiler for {}", connection_id);
                break;
            }
            _ = &mut stop_rx => {
                debug!("Profiler stop requested for {}", connection_id);
                break;
            }
        }
    }

    // Close the persistent channel
    let _ = shell_channel.close().await;
    *state.write().unwrap() = ProfilerState::Stopped;
    debug!("Resource profiler stopped for {}", connection_id);
}

/// Open a persistent shell channel for sampling
async fn open_shell_channel(controller: &HandleController) -> Result<Channel<Msg>, String> {
    let channel = timeout(CHANNEL_OPEN_TIMEOUT, controller.open_session_channel())
        .await
        .map_err(|_| "Timeout opening shell channel".to_string())?
        .map_err(|e| format!("Failed to open shell channel: {}", e))?;

    // Request a shell (not exec) so we can send multiple commands
    channel
        .request_shell(false)
        .await
        .map_err(|e| format!("Failed to request shell: {}", e))?;

    // Disable echo and prompt to get clean output
    let init_cmd = "export PS1=''; export PS2=''; stty -echo 2>/dev/null; export LANG=C\n";
    channel
        .data(init_cmd.as_bytes())
        .await
        .map_err(|e| format!("Failed to init shell: {}", e))?;

    // Wait briefly for init to settle, drain any initial output
    tokio::time::sleep(Duration::from_millis(200)).await;

    Ok(channel)
}

/// Send the sampling command to the persistent shell and read output until ===END===
async fn shell_sample(channel: &mut Channel<Msg>) -> Result<String, String> {
    // Write command to stdin
    channel
        .data(SAMPLE_COMMAND.as_bytes())
        .await
        .map_err(|e| format!("Failed to write to shell: {}", e))?;

    let mut stdout = Vec::new();

    let result = timeout(SAMPLE_TIMEOUT, async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                    if stdout.len() > MAX_OUTPUT_SIZE {
                        stdout.truncate(MAX_OUTPUT_SIZE);
                        break;
                    }
                    // Check if we've received the end marker
                    if let Ok(s) = std::str::from_utf8(&stdout) {
                        if s.contains("===END===") {
                            break;
                        }
                    }
                }
                Some(ChannelMsg::ExtendedData { .. }) => {}
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                    return Err("Shell channel closed".to_string());
                }
                Some(_) => {}
                None => {
                    return Err("Shell channel returned None".to_string());
                }
            }
        }
        Ok(())
    })
    .await;

    match result {
        Err(_) => Err("Sample command timed out".into()),
        Ok(Err(e)) => Err(e),
        Ok(Ok(())) => {
            let full = String::from_utf8(stdout).map_err(|e| format!("Invalid UTF-8: {}", e))?;
            // Extract only the portion from ===STAT=== to ===END===
            if let Some(start) = full.find("===STAT===") {
                if let Some(end) = full.find("===END===") {
                    return Ok(full[start..end + "===END===".len()].to_string());
                }
            }
            Ok(full)
        }
    }
}

/// Create empty metrics with a given source
fn make_empty_metrics(source: MetricsSource) -> ResourceMetrics {
    ResourceMetrics {
        timestamp_ms: now_ms(),
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

/// Parse all metrics from the composite command output
fn parse_metrics(output: &str, prev: &Option<PreviousSample>) -> ResourceMetrics {
    let ts = now_ms();
    let cpu_snap = parse_cpu_snapshot(output);
    let net_snap = parse_net_snapshot(output);
    let mem = parse_meminfo(output);
    let load = parse_loadavg(output);
    let nproc = parse_nproc(output);

    // CPU% via delta
    let cpu_percent = match (&cpu_snap, prev) {
        (Some(curr), Some(prev_s)) => {
            let total_delta = curr.total().saturating_sub(prev_s.cpu.total());
            let active_delta = curr.active().saturating_sub(prev_s.cpu.active());
            if total_delta > 0 {
                Some((active_delta as f64 / total_delta as f64) * 100.0)
            } else {
                None
            }
        }
        _ => None, // P5: first sample has no baseline
    };

    // Network rate via delta
    let (net_rx_rate, net_tx_rate) = match (&net_snap, prev) {
        (Some(curr), Some(prev_s)) => {
            let elapsed_ms = ts.saturating_sub(prev_s.timestamp_ms);
            if elapsed_ms > 0 {
                let elapsed_secs = elapsed_ms as f64 / 1000.0;
                let rx = ((curr.rx_bytes.saturating_sub(prev_s.net.rx_bytes)) as f64 / elapsed_secs)
                    as u64;
                let tx = ((curr.tx_bytes.saturating_sub(prev_s.net.tx_bytes)) as f64 / elapsed_secs)
                    as u64;
                (Some(rx), Some(tx))
            } else {
                (None, None)
            }
        }
        _ => (None, None),
    };

    // Memory
    let (mem_used, mem_total, mem_percent) = match mem {
        Some((used, total)) => {
            let pct = if total > 0 {
                Some((used as f64 / total as f64) * 100.0)
            } else {
                None
            };
            (Some(used), Some(total), pct)
        }
        None => (None, None, None),
    };

    // Determine source quality
    let has_cpu = cpu_snap.is_some();
    let has_mem = mem.is_some();
    let has_load = load.is_some();
    let source = if has_cpu && has_mem && has_load {
        MetricsSource::Full
    } else if has_cpu || has_mem || has_load {
        MetricsSource::Partial
    } else {
        MetricsSource::RttOnly
    };

    ResourceMetrics {
        timestamp_ms: ts,
        cpu_percent,
        memory_used: mem_used,
        memory_total: mem_total,
        memory_percent: mem_percent,
        load_avg_1: load.map(|(a, _, _)| a),
        load_avg_5: load.map(|(_, b, _)| b),
        load_avg_15: load.map(|(_, _, c)| c),
        cpu_cores: nproc,
        net_rx_bytes_per_sec: net_rx_rate,
        net_tx_bytes_per_sec: net_tx_rate,
        ssh_rtt_ms: None, // Filled by frontend from HealthTracker
        source,
    }
}

// ─── Parsers ──────────────────────────────────────────────────────────────

/// Extract section between markers
fn extract_section<'a>(output: &'a str, marker: &str) -> Option<&'a str> {
    let start_marker = format!("==={}===", marker);
    let start = output.find(&start_marker)?;
    let after_marker = start + start_marker.len();
    // Find the next === marker or end
    let rest = &output[after_marker..];
    let end = rest.find("===").unwrap_or(rest.len());
    Some(rest[..end].trim())
}

/// Parse /proc/stat first line → CpuSnapshot
fn parse_cpu_snapshot(output: &str) -> Option<CpuSnapshot> {
    let section = extract_section(output, "STAT")?;
    // First line: "cpu  user nice system idle iowait irq softirq steal ..."
    let line = section.lines().next()?;
    if !line.starts_with("cpu ") {
        return None;
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
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

/// Parse /proc/meminfo → (used_bytes, total_bytes)
fn parse_meminfo(output: &str) -> Option<(u64, u64)> {
    let section = extract_section(output, "MEMINFO")?;
    let mut total_kb: Option<u64> = None;
    let mut available_kb: Option<u64> = None;

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

    let total = total_kb? * 1024; // KB → bytes
    let available = available_kb? * 1024;
    let used = total.saturating_sub(available);
    Some((used, total))
}

/// Extract "MemTotal:    1234 kB" → 1234
fn extract_kb_value(line: &str) -> Option<u64> {
    line.split_whitespace().nth(1)?.parse().ok()
}

/// Parse /proc/loadavg → (1min, 5min, 15min)
fn parse_loadavg(output: &str) -> Option<(f64, f64, f64)> {
    let section = extract_section(output, "LOADAVG")?;
    let line = section.lines().next()?;
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }
    Some((
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    ))
}

/// Parse /proc/net/dev → aggregate NetSnapshot (excluding lo)
fn parse_net_snapshot(output: &str) -> Option<NetSnapshot> {
    let section = extract_section(output, "NETDEV")?;
    let mut total_rx: u64 = 0;
    let mut total_tx: u64 = 0;
    let mut found = false;

    for line in section.lines() {
        let line = line.trim();
        // Skip header lines (contain |)
        if line.contains('|') || line.is_empty() {
            continue;
        }
        // Format: "iface: rx_bytes rx_packets ... tx_bytes tx_packets ..."
        if let Some((iface, rest)) = line.split_once(':') {
            let iface = iface.trim();
            if iface == "lo" {
                continue; // Skip loopback
            }
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 9 {
                if let (Ok(rx), Ok(tx)) = (parts[0].parse::<u64>(), parts[8].parse::<u64>()) {
                    total_rx += rx;
                    total_tx += tx;
                    found = true;
                }
            }
        }
    }

    if found {
        Some(NetSnapshot {
            rx_bytes: total_rx,
            tx_bytes: total_tx,
        })
    } else {
        None
    }
}

/// Parse nproc output → core count
fn parse_nproc(output: &str) -> Option<u32> {
    let section = extract_section(output, "NPROC")?;
    section.lines().next()?.trim().parse().ok()
}

// ─── Helpers ──────────────────────────────────────────────────────────────

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn store_metrics(
    latest: &Arc<RwLock<Option<ResourceMetrics>>>,
    history: &Arc<RwLock<VecDeque<ResourceMetrics>>>,
    metrics: &ResourceMetrics,
) {
    *latest.write().unwrap() = Some(metrics.clone());
    let mut hist = history.write().unwrap();
    if hist.len() >= HISTORY_CAPACITY {
        hist.pop_front();
    }
    hist.push_back(metrics.clone());
}

fn emit_metrics(app_handle: &tauri::AppHandle, connection_id: &str, metrics: &ResourceMetrics) {
    let event_name = format!("profiler:update:{}", connection_id);
    if let Err(e) = app_handle.emit(&event_name, metrics) {
        warn!("Failed to emit profiler event: {}", e);
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_OUTPUT: &str = r#"===STAT===
cpu  10132153 290696 3084719 46828483 16683 0 25195 0 0 0
cpu0 1393280 32966 572056 13343292 6130 0 17875 0 0 0
===MEMINFO===
MemTotal:       16384000 kB
MemFree:         2048000 kB
MemAvailable:    8192000 kB
Buffers:          512000 kB
Cached:          4096000 kB
===LOADAVG===
0.52 0.58 0.59 2/345 12345
===NETDEV===
Inter-|   Receive                                                |  Transmit
 face |bytes    packets errs drop fifo frame compressed multicast|bytes    packets errs drop fifo colls carrier compressed
    lo: 1234567     890    0    0    0     0          0         0  1234567     890    0    0    0     0       0          0
  eth0: 987654321  12345    0    0    0     0          0         0 123456789   6789    0    0    0     0       0          0
===NPROC===
4
===END==="#;

    #[test]
    fn test_parse_cpu_snapshot() {
        let snap = parse_cpu_snapshot(SAMPLE_OUTPUT).unwrap();
        assert_eq!(snap.user, 10132153);
        assert_eq!(snap.nice, 290696);
        assert_eq!(snap.system, 3084719);
        assert_eq!(snap.idle, 46828483);
    }

    #[test]
    fn test_parse_meminfo() {
        let (used, total) = parse_meminfo(SAMPLE_OUTPUT).unwrap();
        assert_eq!(total, 16384000 * 1024);
        assert_eq!(used, (16384000 - 8192000) * 1024);
    }

    #[test]
    fn test_parse_loadavg() {
        let (l1, l5, l15) = parse_loadavg(SAMPLE_OUTPUT).unwrap();
        assert!((l1 - 0.52).abs() < 0.001);
        assert!((l5 - 0.58).abs() < 0.001);
        assert!((l15 - 0.59).abs() < 0.001);
    }

    #[test]
    fn test_parse_net_snapshot() {
        let snap = parse_net_snapshot(SAMPLE_OUTPUT).unwrap();
        // Should exclude lo, only eth0
        assert_eq!(snap.rx_bytes, 987654321);
        assert_eq!(snap.tx_bytes, 123456789);
    }

    #[test]
    fn test_parse_nproc() {
        let cores = parse_nproc(SAMPLE_OUTPUT).unwrap();
        assert_eq!(cores, 4);
    }

    #[test]
    fn test_parse_metrics_first_sample_no_delta() {
        let metrics = parse_metrics(SAMPLE_OUTPUT, &None);
        // P5: first sample has no CPU% or net rate
        assert!(metrics.cpu_percent.is_none());
        assert!(metrics.net_rx_bytes_per_sec.is_none());
        assert!(metrics.net_tx_bytes_per_sec.is_none());
        // But memory and load should be present
        assert!(metrics.memory_used.is_some());
        assert!(metrics.load_avg_1.is_some());
        assert_eq!(metrics.cpu_cores, Some(4));
        assert_eq!(metrics.source, MetricsSource::Full);
    }

    #[test]
    fn test_parse_metrics_with_delta() {
        let prev = PreviousSample {
            cpu: CpuSnapshot {
                user: 10000000,
                nice: 290000,
                system: 3000000,
                idle: 46000000,
                iowait: 16000,
                irq: 0,
                softirq: 25000,
                steal: 0,
            },
            net: NetSnapshot {
                rx_bytes: 900000000,
                tx_bytes: 100000000,
            },
            timestamp_ms: now_ms() - 5000,
        };

        let metrics = parse_metrics(SAMPLE_OUTPUT, &Some(prev));
        assert!(metrics.cpu_percent.is_some());
        assert!(metrics.net_rx_bytes_per_sec.is_some());
        assert!(metrics.net_tx_bytes_per_sec.is_some());
    }

    #[test]
    fn test_extract_section() {
        let section = extract_section(SAMPLE_OUTPUT, "LOADAVG").unwrap();
        assert!(section.starts_with("0.52"));
    }

    #[test]
    fn test_empty_output() {
        let metrics = parse_metrics("", &None);
        assert_eq!(metrics.source, MetricsSource::RttOnly);
    }
}
