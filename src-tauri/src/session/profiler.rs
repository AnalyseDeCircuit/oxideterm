// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Resource Profiler Module
//!
//! Samples remote host resources (CPU, memory, load, network) via a persistent SSH shell channel.
//! Uses a single long-lived channel to avoid MaxSessions exhaustion.
//!
//! Also performs **smart port detection**: scans remote listening ports each cycle, detects
//! changes (new ports / closed ports), and emits `port-detected:{connectionId}` events so
//! the frontend can offer one-click forwarding (similar to VS Code SSH Remote).
//!
//! # Design
//! - One `ResourceProfiler` per connection, bound to SSH lifecycle via `subscribe_disconnect()`
//! - Opens ONE shell channel at startup, reuses it for all sampling cycles
//! - Collects `/proc/stat`, `/proc/meminfo`, `/proc/loadavg`, `/proc/net/dev` via stdin commands
//! - CPU% and network rates require delta between two samples (first sample returns None)
//! - Non-Linux hosts gracefully degrade to `MetricsSource::RttOnly`
//! - Port detection commands are platform-dispatched based on `os_type`
//!
//! # Invariants
//! - P1: Profiler does not hold strong references to the connection
//! - P2: SSH disconnect → profiler auto-stops via `disconnect_rx`
//! - P3: Only 1 shell channel held for the entire profiler lifetime
//! - P5: First sample returns None for CPU/network (no delta baseline)
//! - P6: First port scan is silent (establishes baseline, no event emitted)

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::RwLock;

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

/// Maximum output size from a single sample (64KB — includes ss/netstat + docker ps)
const MAX_OUTPUT_SIZE: usize = 65_536;

/// Timeout for reading a single sample's output from the shell channel
const SAMPLE_TIMEOUT: Duration = Duration::from_secs(5);

/// Default sampling interval (10s to minimise SSH bandwidth contention with PTY)
const DEFAULT_INTERVAL: Duration = Duration::from_secs(10);

/// Number of consecutive failures before degrading to RttOnly
const MAX_CONSECUTIVE_FAILURES: u32 = 3;

/// Timeout for opening the initial shell channel
const CHANNEL_OPEN_TIMEOUT: Duration = Duration::from_secs(10);

/// Slimmed sampling command (Linux only) — reads /proc pseudo-files for metrics.
/// The full command is now built dynamically by `build_sample_command()` based on `os_type`,
/// appending a platform-specific port scan after the metrics section.
const METRICS_COMMAND_LINUX: &str = "echo '===STAT==='; head -1 /proc/stat 2>/dev/null; echo '===MEMINFO==='; grep -E '^(MemTotal|MemAvailable):' /proc/meminfo 2>/dev/null; echo '===LOADAVG==='; cat /proc/loadavg 2>/dev/null; echo '===NETDEV==='; cat /proc/net/dev 2>/dev/null; echo '===NPROC==='; nproc 2>/dev/null";

// ─── Port Detection: Platform-Dispatched Commands ─────────────────────────

/// Linux: Use `ss` (modern) with `netstat` fallback.
/// Output: one line per listening socket with addr:port and optional process info.
const PORT_CMD_LINUX: &str = "echo '===PORTS==='; ((ss -tlnp 2>/dev/null || netstat -tlnp 2>/dev/null) | grep -i listen || true); echo '===PORTS_END==='; echo '===DOCKER==='; ((docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null || sudo -n docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null) || true); echo '===DOCKER_END==='";

/// macOS: Use `lsof` to list listening TCP sockets.
const PORT_CMD_MACOS: &str = "echo '===PORTS==='; ((lsof -iTCP -sTCP:LISTEN -nP 2>/dev/null | tail -n +2) || true); echo '===PORTS_END==='; echo '===DOCKER==='; ((docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null || sudo -n docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}' 2>/dev/null) || true); echo '===DOCKER_END==='";

/// Windows (PowerShell): `Get-NetTCPConnection` → CSV-like output.
const PORT_CMD_WINDOWS: &str = "echo '===PORTS==='; powershell -NoProfile -Command \"Get-NetTCPConnection -State Listen 2>$null | Select-Object LocalAddress,LocalPort,OwningProcess | Format-Table -HideTableHeaders\" 2>/dev/null; echo '===PORTS_END==='";

/// FreeBSD: Use `sockstat` to list listening TCP sockets.
const PORT_CMD_FREEBSD: &str =
    "echo '===PORTS==='; sockstat -4 -6 -l -P tcp 2>/dev/null | tail -n +2; echo '===PORTS_END==='";

/// Build the complete sampling command including port scan for the given OS.
/// Returns a String with a trailing newline, ready to send to the shell channel.
fn build_sample_command(os_type: &str) -> String {
    let metrics = match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
            METRICS_COMMAND_LINUX
        }
        // Non-Linux: metrics will degrade to RttOnly, but port scan still runs
        _ => METRICS_COMMAND_LINUX,
    };

    let port_cmd = match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => PORT_CMD_LINUX,
        "macOS" | "macos" | "Darwin" => PORT_CMD_MACOS,
        "Windows" | "windows" => PORT_CMD_WINDOWS,
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => PORT_CMD_FREEBSD,
        _ => PORT_CMD_LINUX, // Fallback to Linux commands
    };

    format!("{}; {}; echo '===END==='\n", metrics, port_cmd)
}

// ─── Port Detection Data Structures ───────────────────────────────────────

/// A detected listening port on the remote host.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DetectedPort {
    /// The port number
    pub port: u16,
    /// Bind address (e.g. "0.0.0.0", "127.0.0.1", "::")
    pub bind_addr: String,
    /// Process name if available (e.g. "node", "python3")
    pub process_name: Option<String>,
    /// Process ID if available
    pub pid: Option<u32>,
}

/// Event emitted when new listening ports are detected on the remote host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortDetectionEvent {
    /// Connection this event belongs to
    pub connection_id: String,
    /// Newly detected ports (not in previous scan)
    pub new_ports: Vec<DetectedPort>,
    /// Ports that were closed since last scan
    pub closed_ports: Vec<DetectedPort>,
    /// Full list of currently listening ports
    pub all_ports: Vec<DetectedPort>,
}

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
    /// Ports the user has dismissed / ignored (not shown again until restart)
    ignored_ports: Arc<RwLock<HashSet<u16>>>,
    /// Latest detected listening ports
    detected_ports: Arc<RwLock<Vec<DetectedPort>>>,
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
        os_type: String,
    ) -> Self {
        let state = Arc::new(RwLock::new(ProfilerState::Running));
        let latest = Arc::new(RwLock::new(None));
        let history = Arc::new(RwLock::new(VecDeque::with_capacity(HISTORY_CAPACITY)));
        let ignored_ports = Arc::new(RwLock::new(HashSet::new()));
        let detected_ports = Arc::new(RwLock::new(Vec::new()));
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();

        let profiler = Self {
            connection_id: connection_id.clone(),
            state: state.clone(),
            latest: latest.clone(),
            history: history.clone(),
            stop_tx: Some(stop_tx),
            ignored_ports: ignored_ports.clone(),
            detected_ports: detected_ports.clone(),
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
                os_type,
                ignored_ports,
                detected_ports,
            )
            .await;
        });

        profiler
    }

    /// Get the latest metrics snapshot
    pub async fn latest(&self) -> Option<ResourceMetrics> {
        self.latest.read().clone()
    }

    /// Get metrics history for sparkline rendering
    pub async fn history(&self) -> Vec<ResourceMetrics> {
        self.history.read().iter().cloned().collect()
    }

    /// Get current profiler state
    pub async fn state(&self) -> ProfilerState {
        *self.state.read()
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

    /// Get the latest detected listening ports
    pub fn detected_ports(&self) -> Vec<DetectedPort> {
        self.detected_ports.read().clone()
    }

    /// Add a port to the ignore list (user dismissed the notification)
    pub fn ignore_port(&self, port: u16) {
        self.ignored_ports.write().insert(port);
    }
}

/// The main sampling loop. Runs until stopped or disconnected.
///
/// Opens ONE persistent shell channel at startup and reuses it for all samples.
/// This avoids MaxSessions exhaustion on servers with low limits.
///
/// Also performs port detection: parses `===PORTS===...===PORTS_END===` from each sample,
/// diffs against previous scan, and emits `port-detected:{connectionId}` on changes.
async fn sampling_loop(
    connection_id: String,
    controller: HandleController,
    state: Arc<RwLock<ProfilerState>>,
    latest: Arc<RwLock<Option<ResourceMetrics>>>,
    history: Arc<RwLock<VecDeque<ResourceMetrics>>>,
    mut stop_rx: tokio::sync::oneshot::Receiver<()>,
    disconnect_rx: &mut broadcast::Receiver<()>,
    app_handle: tauri::AppHandle,
    os_type: String,
    ignored_ports: Arc<RwLock<HashSet<u16>>>,
    detected_ports: Arc<RwLock<Vec<DetectedPort>>>,
) {
    let mut prev_sample: Option<PreviousSample> = None;
    let mut consecutive_failures: u32 = 0;
    let mut interval = tokio::time::interval(DEFAULT_INTERVAL);
    // Skip the immediate first tick
    interval.tick().await;

    // Port detection state
    let mut prev_ports: HashSet<u16> = HashSet::new();
    let mut is_initial_scan = true;

    // Build the sample command once (includes port scan for this OS)
    let sample_command = build_sample_command(&os_type);

    debug!(
        "Resource profiler started for connection {} (os_type={})",
        connection_id, os_type
    );

    // Open persistent shell channel
    let mut shell_channel = match open_shell_channel(&controller, &os_type).await {
        Ok(ch) => ch,
        Err(e) => {
            warn!(
                "Profiler failed to open shell channel for {}: {}",
                connection_id, e
            );
            *state.write() = ProfilerState::Degraded;
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
                    let mut s = state.write();
                    if *s != ProfilerState::Degraded {
                        *s = ProfilerState::Degraded;
                        warn!(
                            "Resource profiler degraded for {} after {} consecutive failures",
                            connection_id, consecutive_failures
                        );
                    }
                    drop(s);
                    let metrics = make_empty_metrics(MetricsSource::RttOnly);
                    store_metrics(&latest, &history, &metrics);
                    emit_metrics(&app_handle, &connection_id, &metrics);
                    continue;
                }

                // Execute sampling command on persistent shell
                match shell_sample(&mut shell_channel, &sample_command).await {
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

                        // ── Port Detection ──
                        // Skip port diff if sample was truncated (no ===END=== marker),
                        // as partial output would produce false closed/new diffs.
                        let sample_complete = output.contains("===END===");
                        if !sample_complete {
                            warn!("Profiler sample for {} was truncated, skipping port diff", connection_id);
                        }

                        if sample_complete {
                        let current_ports = parse_listening_ports(&output, &os_type);
                        let current_port_numbers: HashSet<u16> =
                            current_ports.iter().map(|p| p.port).collect();

                        if is_initial_scan {
                            // P6: first scan is silent — establish baseline
                            prev_ports = current_port_numbers;
                            *detected_ports.write() = current_ports;
                            is_initial_scan = false;
                            trace!("Port detection baseline for {}: {} ports", connection_id, prev_ports.len());
                        } else {
                            // Diff: find new and closed ports
                            let new_port_numbers: Vec<u16> = current_port_numbers
                                .difference(&prev_ports)
                                .copied()
                                .collect();
                            let closed_port_numbers: Vec<u16> = prev_ports
                                .difference(&current_port_numbers)
                                .copied()
                                .collect();

                            if !new_port_numbers.is_empty() || !closed_port_numbers.is_empty() {
                                // Filter out ignored ports and port 22 (SSH)
                                let ignored = ignored_ports.read();
                                let new_ports: Vec<DetectedPort> = current_ports
                                    .iter()
                                    .filter(|p| {
                                        new_port_numbers.contains(&p.port)
                                            && p.port != 22
                                            && !ignored.contains(&p.port)
                                    })
                                    .cloned()
                                    .collect();
                                let closed_ports: Vec<DetectedPort> = prev_ports
                                    .iter()
                                    .filter(|port| closed_port_numbers.contains(port))
                                    .map(|&port| DetectedPort {
                                        port,
                                        bind_addr: String::new(),
                                        process_name: None,
                                        pid: None,
                                    })
                                    .collect();
                                drop(ignored);

                                // Only emit if there are actually visible new ports
                                if !new_ports.is_empty() || !closed_ports.is_empty() {
                                    let event = PortDetectionEvent {
                                        connection_id: connection_id.clone(),
                                        new_ports,
                                        closed_ports,
                                        all_ports: current_ports.clone(),
                                    };
                                    let event_name =
                                        format!("port-detected:{}", connection_id);
                                    if let Err(e) = app_handle.emit(&event_name, &event) {
                                        warn!("Failed to emit port detection event: {}", e);
                                    }
                                    debug!(
                                        "Port detection for {}: {} new, {} closed",
                                        connection_id,
                                        event.new_ports.len(),
                                        event.closed_ports.len()
                                    );
                                }
                            }

                            // Always update snapshot: port numbers for diff baseline,
                            // and full DetectedPort data for metadata freshness
                            // (bind_addr / process_name / pid may change even if port set is stable)
                            prev_ports = current_port_numbers;
                            *detected_ports.write() = current_ports;
                        }
                        } // end if sample_complete
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        warn!(
                            "Profiler sample failed for {} ({}/{}): {}",
                            connection_id, consecutive_failures, MAX_CONSECUTIVE_FAILURES, e
                        );

                        // Try to reopen the shell channel once
                        if let Ok(new_ch) = open_shell_channel(&controller, &os_type).await {
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
    *state.write() = ProfilerState::Stopped;
    debug!("Resource profiler stopped for {}", connection_id);
}

/// Open a persistent shell channel for sampling
async fn open_shell_channel(
    controller: &HandleController,
    os_type: &str,
) -> Result<Channel<Msg>, String> {
    let channel = timeout(CHANNEL_OPEN_TIMEOUT, controller.open_session_channel())
        .await
        .map_err(|_| "Timeout opening shell channel".to_string())?
        .map_err(|e| format!("Failed to open shell channel: {}", e))?;

    // Request a shell (not exec) so we can send multiple commands
    channel
        .request_shell(false)
        .await
        .map_err(|e| format!("Failed to request shell: {}", e))?;

    // Platform-specific init command:
    // - Unix: disable echo/prompt via stty, set C locale
    // - Windows: stty not available, but prompt should still be suppressed
    let init_cmd = match os_type {
        "Windows" | "windows" => {
            // PowerShell / cmd.exe: set prompt to empty
            "set PROMPT=\r\n"
        }
        _ => "export PS1=''; export PS2=''; stty -echo 2>/dev/null; export LANG=C\n",
    };
    channel
        .data(init_cmd.as_bytes())
        .await
        .map_err(|e| format!("Failed to init shell: {}", e))?;

    // Wait briefly for init to settle, drain any initial output
    tokio::time::sleep(Duration::from_millis(200)).await;

    Ok(channel)
}

/// Send the sampling command to the persistent shell and read output until ===END===
async fn shell_sample(channel: &mut Channel<Msg>, command: &str) -> Result<String, String> {
    // Write command to stdin
    channel
        .data(command.as_bytes())
        .await
        .map_err(|e| format!("Failed to write to shell: {}", e))?;

    let mut stdout = Vec::new();

    let result = timeout(SAMPLE_TIMEOUT, async {
        loop {
            match channel.wait().await {
                Some(ChannelMsg::Data { data }) => {
                    stdout.extend_from_slice(&data);
                    // Check if we've received the end marker (always preferred over truncation)
                    if let Ok(s) = std::str::from_utf8(&stdout) {
                        if s.contains("===END===") {
                            break;
                        }
                    }
                    // Safety cap: prevent unbounded memory growth
                    if stdout.len() > MAX_OUTPUT_SIZE {
                        warn!(
                            "Profiler output exceeded {}KB, truncating",
                            MAX_OUTPUT_SIZE / 1024
                        );
                        stdout.truncate(MAX_OUTPUT_SIZE);
                        break;
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

// ─── Port Detection Parsers ──────────────────────────────────────────────

/// Parse listening ports from the ===PORTS=== section, dispatching by OS type.
fn parse_listening_ports(output: &str, os_type: &str) -> Vec<DetectedPort> {
    let section = match extract_section(output, "PORTS") {
        Some(s) => s,
        None => return Vec::new(),
    };

    // Strip the ===PORTS_END=== residual if present
    let section = section
        .strip_suffix("===PORTS_END===")
        .unwrap_or(section)
        .trim();

    if section.is_empty() {
        return Vec::new();
    }

    let mut ports = match os_type {
        "Linux" | "linux" | "Windows_MinGW" | "Windows_MSYS" | "Windows_Cygwin" => {
            parse_ports_ss(section)
        }
        "macOS" | "macos" | "Darwin" => parse_ports_lsof(section),
        "Windows" | "windows" => parse_ports_powershell(section),
        "FreeBSD" | "freebsd" | "OpenBSD" | "NetBSD" => parse_ports_sockstat(section),
        _ => parse_ports_ss(section), // fallback
    };

    // Merge Docker-mapped ports (handles iptables DNAT where ss can't see them)
    let docker_ports = parse_ports_docker(output);
    if !docker_ports.is_empty() {
        let mut seen: HashSet<u16> = ports.iter().map(|p| p.port).collect();
        for dp in docker_ports {
            if seen.insert(dp.port) {
                ports.push(dp);
            }
        }
    }

    ports
}

/// Parse `ss -tlnp` or `netstat -tlnp` output.
///
/// `ss` output example:
/// ```text
/// LISTEN  0  128  0.0.0.0:8080  0.0.0.0:*  users:(("node",pid=1234,fd=3))
/// LISTEN  0  128  [::]:3000     [::]:*     users:(("python3",pid=5678,fd=4))
/// ```
///
/// `netstat -tlnp` output example:
/// ```text
/// tcp  0  0  0.0.0.0:22  0.0.0.0:*  LISTEN  1234/sshd
/// ```
fn parse_ports_ss(section: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();

        // Try ss format first: LISTEN 0 128 addr:port ...
        if parts.len() >= 4 && parts[0].eq_ignore_ascii_case("listen") {
            // addr:port is at index 3
            if let Some(dp) = parse_addr_port(parts[3]) {
                // Extract process info from users:(...) if present
                let mut dp = dp;
                if let Some(users_part) = parts.iter().find(|p| p.starts_with("users:")) {
                    dp = extract_process_from_ss_users(users_part, dp);
                }
                if seen.insert(dp.port) {
                    ports.push(dp);
                }
            }
            continue;
        }

        // Try netstat format: tcp 0 0 addr:port addr:port LISTEN pid/name
        if parts.len() >= 6 && parts.iter().any(|p| p.eq_ignore_ascii_case("listen")) {
            // addr:port is at index 3
            if let Some(mut dp) = parse_addr_port(parts[3]) {
                // pid/name is the last column
                if let Some(last) = parts.last() {
                    if let Some((pid_str, name)) = last.split_once('/') {
                        dp.pid = pid_str.parse().ok();
                        dp.process_name = Some(name.to_string());
                    }
                }
                if seen.insert(dp.port) {
                    ports.push(dp);
                }
            }
        }
    }

    ports
}

/// Parse `docker ps --format '{{.ID}}\t{{.Names}}\t{{.Ports}}'` output.
///
/// Docker Ports column examples:
/// ```text
/// 0.0.0.0:8080->80/tcp, :::8080->80/tcp
/// 0.0.0.0:3306->3306/tcp
/// 0.0.0.0:5432->5432/tcp, 0.0.0.0:5433->5433/tcp, :::5432->5432/tcp, :::5433->5433/tcp
/// 80/tcp                      (exposed but not mapped - skip)
/// ```
fn parse_ports_docker(output: &str) -> Vec<DetectedPort> {
    let section = match extract_section(output, "DOCKER") {
        Some(s) => s,
        None => return Vec::new(),
    };

    let section = section
        .strip_suffix("===DOCKER_END===")
        .unwrap_or(section)
        .trim();

    if section.is_empty() {
        return Vec::new();
    }

    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Format: ID\tNAME\tPORTS
        let tab_parts: Vec<&str> = line.splitn(3, '\t').collect();
        if tab_parts.len() < 3 {
            continue;
        }

        let container_name = tab_parts[1].trim();
        let ports_field = tab_parts[2].trim();

        if ports_field.is_empty() {
            continue;
        }

        // Split by ", " — each segment is like "0.0.0.0:8080->80/tcp"
        for segment in ports_field.split(", ") {
            let segment = segment.trim();
            // Must contain "->" to be a host-mapped port
            if !segment.contains("->") {
                continue;
            }

            // Split on "->" → left is host side ("0.0.0.0:8080"), right is container ("80/tcp")
            if let Some((host_part, _container_part)) = segment.split_once("->") {
                // Extract host port from host_part — last colon-separated value
                if let Some(last_colon) = host_part.rfind(':') {
                    let port_str = &host_part[last_colon + 1..];
                    if let Ok(port) = port_str.parse::<u16>() {
                        if seen.insert(port) {
                            let bind_addr = &host_part[..last_colon];
                            let bind_addr = if bind_addr.is_empty() || bind_addr == "*" {
                                "0.0.0.0".to_string()
                            } else {
                                bind_addr.to_string()
                            };
                            ports.push(DetectedPort {
                                port,
                                bind_addr,
                                process_name: Some(format!("docker:{}", container_name)),
                                pid: None,
                            });
                        }
                    }
                }
            }
        }
    }

    ports
}

/// Parse `lsof -iTCP -sTCP:LISTEN -nP` output (macOS).
///
/// Example:
/// ```text
/// node    1234  user   23u  IPv4  0x1234  0t0  TCP *:3000 (LISTEN)
/// python3 5678  user   4u   IPv6  0x5678  0t0  TCP [::1]:8080 (LISTEN)
/// ```
fn parse_ports_lsof(section: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 9 {
            continue;
        }

        let process_name = parts[0].to_string();
        let pid: Option<u32> = parts[1].parse().ok();

        // TCP field is typically at index 8: "*:3000" or "[::1]:8080"
        let tcp_field = parts[8];
        if let Some(dp) = parse_lsof_addr(tcp_field, Some(process_name), pid) {
            if seen.insert(dp.port) {
                ports.push(dp);
            }
        }
    }

    ports
}

/// Parse PowerShell `Get-NetTCPConnection` output (Windows).
///
/// Expected format (Format-Table -HideTableHeaders):
/// ```text
/// 0.0.0.0   8080  1234
/// ::        3000  5678
/// ```
fn parse_ports_powershell(section: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let bind_addr = parts[0].to_string();
        if let Ok(port) = parts[1].parse::<u16>() {
            let pid = parts.get(2).and_then(|p| p.parse().ok());
            if seen.insert(port) {
                ports.push(DetectedPort {
                    port,
                    bind_addr,
                    process_name: None,
                    pid,
                });
            }
        }
    }

    ports
}

/// Parse `sockstat` output (FreeBSD).
///
/// Example:
/// ```text
/// USER  COMMAND  PID  FD  PROTO  LOCAL ADDRESS  FOREIGN ADDRESS
/// root  sshd     1234  3  tcp4   *:22           *:*
/// ```
fn parse_ports_sockstat(section: &str) -> Vec<DetectedPort> {
    let mut ports = Vec::new();
    let mut seen = HashSet::new();

    for line in section.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }

        let process_name = parts[1].to_string();
        let pid: Option<u32> = parts[2].parse().ok();
        // Local address at index 5: "*:22" or "127.0.0.1:8080"
        let local_addr = parts[5];
        if let Some(dp) = parse_addr_port(local_addr) {
            let dp = DetectedPort {
                process_name: Some(process_name),
                pid,
                ..dp
            };
            if seen.insert(dp.port) {
                ports.push(dp);
            }
        }
    }

    ports
}

/// Parse "addr:port" format (handles IPv6 bracket notation).
/// Examples: "0.0.0.0:8080", "[::]:3000", "*:22", ":::80"
fn parse_addr_port(s: &str) -> Option<DetectedPort> {
    // IPv6 with brackets: [::1]:8080
    if let Some(bracket_end) = s.rfind("]:") {
        let addr = &s[..bracket_end + 1];
        let port_str = &s[bracket_end + 2..];
        let port: u16 = port_str.parse().ok()?;
        return Some(DetectedPort {
            port,
            bind_addr: addr.to_string(),
            process_name: None,
            pid: None,
        });
    }

    // IPv6 without brackets (ss format): :::80 → addr="::", port=80
    // Also handles *:port
    if let Some(last_colon) = s.rfind(':') {
        let port_str = &s[last_colon + 1..];
        let addr = &s[..last_colon];
        if let Ok(port) = port_str.parse::<u16>() {
            let bind_addr = if addr.is_empty() || addr == "*" {
                "0.0.0.0".to_string()
            } else {
                addr.to_string()
            };
            return Some(DetectedPort {
                port,
                bind_addr,
                process_name: None,
                pid: None,
            });
        }
    }

    None
}

/// Parse lsof TCP field: "*:3000", "[::1]:8080", "127.0.0.1:4000"
fn parse_lsof_addr(
    s: &str,
    process_name: Option<String>,
    pid: Option<u32>,
) -> Option<DetectedPort> {
    let mut dp = parse_addr_port(s)?;
    dp.process_name = process_name;
    dp.pid = pid;
    Some(dp)
}

/// Extract process name and PID from ss `users:((...))` field.
/// Format: `users:(("node",pid=1234,fd=3))`
fn extract_process_from_ss_users(users_field: &str, mut dp: DetectedPort) -> DetectedPort {
    // Find process name between quotes
    if let Some(start) = users_field.find("((\"") {
        let rest = &users_field[start + 3..];
        if let Some(end) = rest.find('"') {
            dp.process_name = Some(rest[..end].to_string());
        }
    }
    // Find pid=NNNN
    if let Some(start) = users_field.find("pid=") {
        let rest = &users_field[start + 4..];
        let pid_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        dp.pid = pid_str.parse().ok();
    }
    dp
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
    *latest.write() = Some(metrics.clone());
    let mut hist = history.write();
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

    // ─── Port Detection Tests ──────────────────────────────────────────────

    #[test]
    fn test_parse_ports_ss() {
        let section = r#"LISTEN  0  128  0.0.0.0:8080  0.0.0.0:*  users:(("node",pid=1234,fd=3))
LISTEN  0  128  [::]:3000  [::]:*  users:(("python3",pid=5678,fd=4))
LISTEN  0  128  127.0.0.1:5432  0.0.0.0:*  users:(("postgres",pid=999,fd=5))"#;
        let ports = parse_ports_ss(section);
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].port, 8080);
        assert_eq!(ports[0].bind_addr, "0.0.0.0");
        assert_eq!(ports[0].process_name.as_deref(), Some("node"));
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[1].port, 3000);
        assert_eq!(ports[1].process_name.as_deref(), Some("python3"));
        assert_eq!(ports[2].port, 5432);
        assert_eq!(ports[2].bind_addr, "127.0.0.1");
    }

    #[test]
    fn test_parse_ports_netstat() {
        let section = r#"tcp  0  0  0.0.0.0:22  0.0.0.0:*  LISTEN  1234/sshd
tcp  0  0  0.0.0.0:80  0.0.0.0:*  LISTEN  5678/nginx"#;
        let ports = parse_ports_ss(section); // ss parser also handles netstat
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 22);
        assert_eq!(ports[0].process_name.as_deref(), Some("sshd"));
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[1].port, 80);
        assert_eq!(ports[1].process_name.as_deref(), Some("nginx"));
    }

    #[test]
    fn test_parse_ports_lsof() {
        let section = r#"node    1234  user   23u  IPv4  0x1234  0t0  TCP *:3000 (LISTEN)
python3 5678  user   4u   IPv6  0x5678  0t0  TCP [::1]:8080 (LISTEN)"#;
        let ports = parse_ports_lsof(section);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 3000);
        assert_eq!(ports[0].process_name.as_deref(), Some("node"));
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[1].port, 8080);
        assert_eq!(ports[1].process_name.as_deref(), Some("python3"));
    }

    #[test]
    fn test_parse_ports_powershell() {
        let section = r#"0.0.0.0   8080  1234
::        3000  5678
127.0.0.1 5432  999"#;
        let ports = parse_ports_powershell(section);
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].port, 8080);
        assert_eq!(ports[0].bind_addr, "0.0.0.0");
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[1].port, 3000);
        assert_eq!(ports[1].bind_addr, "::");
        assert_eq!(ports[2].port, 5432);
    }

    #[test]
    fn test_parse_ports_sockstat() {
        let section = r#"root  sshd   1234  3  tcp4  *:22            *:*
www   nginx  5678  4  tcp4  *:80            *:*"#;
        let ports = parse_ports_sockstat(section);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 22);
        assert_eq!(ports[0].process_name.as_deref(), Some("sshd"));
        assert_eq!(ports[0].pid, Some(1234));
        assert_eq!(ports[1].port, 80);
        assert_eq!(ports[1].process_name.as_deref(), Some("nginx"));
    }

    #[test]
    fn test_parse_addr_port_ipv4() {
        let dp = parse_addr_port("0.0.0.0:8080").unwrap();
        assert_eq!(dp.port, 8080);
        assert_eq!(dp.bind_addr, "0.0.0.0");
    }

    #[test]
    fn test_parse_addr_port_ipv6_brackets() {
        let dp = parse_addr_port("[::]:3000").unwrap();
        assert_eq!(dp.port, 3000);
        assert_eq!(dp.bind_addr, "[::]");
    }

    #[test]
    fn test_parse_addr_port_wildcard() {
        let dp = parse_addr_port("*:22").unwrap();
        assert_eq!(dp.port, 22);
        assert_eq!(dp.bind_addr, "0.0.0.0");
    }

    #[test]
    fn test_parse_addr_port_ipv6_no_brackets() {
        let dp = parse_addr_port(":::80").unwrap();
        assert_eq!(dp.port, 80);
        assert_eq!(dp.bind_addr, "::");
    }

    #[test]
    fn test_parse_listening_ports_full_output() {
        // Simulate full output with metrics + ports section
        let output = r#"===STAT===
cpu  10132153 290696 3084719 46828483 16683 0 25195 0 0 0
===MEMINFO===
MemTotal:       16384000 kB
MemAvailable:    8192000 kB
===LOADAVG===
0.52 0.58 0.59 2/345 12345
===NETDEV===
Inter-|   Receive                                                |  Transmit
    lo: 1234567     890    0    0    0     0          0         0  1234567     890    0    0    0     0       0          0
  eth0: 987654321  12345    0    0    0     0          0         0 123456789   6789    0    0    0     0       0          0
===NPROC===
4
===PORTS===
LISTEN  0  128  0.0.0.0:8080  0.0.0.0:*  users:(("node",pid=1234,fd=3))
LISTEN  0  128  0.0.0.0:22  0.0.0.0:*  users:(("sshd",pid=1,fd=5))
===PORTS_END===
===END==="#;

        let ports = parse_listening_ports(output, "Linux");
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 8080);
        assert_eq!(ports[1].port, 22);

        // Metrics parsing should still work
        let metrics = parse_metrics(output, &None);
        assert!(metrics.memory_used.is_some());
        assert!(metrics.load_avg_1.is_some());
    }

    #[test]
    fn test_dedup_ports() {
        let section = r#"LISTEN  0  128  0.0.0.0:8080  0.0.0.0:*
LISTEN  0  128  [::]:8080  [::]:*"#;
        let ports = parse_ports_ss(section);
        // Same port 8080 on IPv4 and IPv6 — should deduplicate
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 8080);
    }

    #[test]
    fn test_parse_ports_docker() {
        let output = r#"===DOCKER===
abc123	my-nginx	0.0.0.0:8080->80/tcp, :::8080->80/tcp
def456	my-postgres	0.0.0.0:5432->5432/tcp
===DOCKER_END==="#;
        let ports = parse_ports_docker(output);
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 8080);
        assert_eq!(ports[0].bind_addr, "0.0.0.0");
        assert_eq!(ports[0].process_name.as_deref(), Some("docker:my-nginx"));
        assert_eq!(ports[1].port, 5432);
        assert_eq!(ports[1].process_name.as_deref(), Some("docker:my-postgres"));
    }

    #[test]
    fn test_parse_ports_docker_multi_mapping() {
        let output = r#"===DOCKER===
aaa111	redis	0.0.0.0:6379->6379/tcp, :::6379->6379/tcp
bbb222	web-app	0.0.0.0:3000->3000/tcp, 0.0.0.0:3001->3001/tcp
ccc333	exposed-only	80/tcp
===DOCKER_END==="#;
        let ports = parse_ports_docker(output);
        assert_eq!(ports.len(), 3);
        assert_eq!(ports[0].port, 6379);
        assert_eq!(ports[1].port, 3000);
        assert_eq!(ports[2].port, 3001);
    }

    #[test]
    fn test_parse_ports_docker_empty() {
        let output = "===DOCKER===\n===DOCKER_END===";
        let ports = parse_ports_docker(output);
        assert_eq!(ports.len(), 0);
    }

    #[test]
    fn test_parse_ports_docker_no_section() {
        let output = "===PORTS===\nLISTEN 0 128 0.0.0.0:22 0.0.0.0:*\n===PORTS_END===";
        let ports = parse_ports_docker(output);
        assert_eq!(ports.len(), 0);
    }

    #[test]
    fn test_parse_listening_ports_with_docker_merge() {
        // ss shows port 22, Docker adds port 8080 (not visible via ss/iptables DNAT)
        let output = r#"===PORTS===
LISTEN  0  128  0.0.0.0:22  0.0.0.0:*  users:(("sshd",pid=1,fd=5))
===PORTS_END===
===DOCKER===
abc123	my-app	0.0.0.0:8080->80/tcp
===DOCKER_END===
===END==="#;
        let ports = parse_listening_ports(output, "Linux");
        assert_eq!(ports.len(), 2);
        assert_eq!(ports[0].port, 22);
        assert_eq!(ports[0].process_name.as_deref(), Some("sshd"));
        assert_eq!(ports[1].port, 8080);
        assert_eq!(ports[1].process_name.as_deref(), Some("docker:my-app"));
    }

    #[test]
    fn test_parse_listening_ports_docker_dedup_with_ss() {
        // Both ss and docker report port 8080 — should not duplicate
        let output = r#"===PORTS===
LISTEN  0  128  0.0.0.0:8080  0.0.0.0:*  users:(("docker-proxy",pid=999,fd=3))
===PORTS_END===
===DOCKER===
abc123	my-app	0.0.0.0:8080->80/tcp
===DOCKER_END===
===END==="#;
        let ports = parse_listening_ports(output, "Linux");
        assert_eq!(ports.len(), 1);
        assert_eq!(ports[0].port, 8080);
        // ss result takes precedence (first seen)
        assert_eq!(ports[0].process_name.as_deref(), Some("docker-proxy"));
    }

    // ─── build_sample_command Tests ────────────────────────────────────

    #[test]
    fn test_build_sample_command_linux() {
        let cmd = build_sample_command("Linux");
        assert!(cmd.contains("===STAT==="));
        assert!(cmd.contains("ss -tlnp"));
        assert!(cmd.contains("===END==="));
        assert!(cmd.ends_with('\n'));
    }

    #[test]
    fn test_build_sample_command_linux_lowercase() {
        let cmd = build_sample_command("linux");
        assert!(cmd.contains("ss -tlnp"));
    }

    #[test]
    fn test_build_sample_command_macos() {
        let cmd = build_sample_command("macOS");
        assert!(cmd.contains("lsof -iTCP"));
        assert!(!cmd.contains("ss -tlnp"));
    }

    #[test]
    fn test_build_sample_command_darwin() {
        let cmd = build_sample_command("Darwin");
        assert!(cmd.contains("lsof -iTCP"));
    }

    #[test]
    fn test_build_sample_command_windows() {
        let cmd = build_sample_command("Windows");
        assert!(cmd.contains("Get-NetTCPConnection"));
        assert!(!cmd.contains("ss -tlnp"));
    }

    #[test]
    fn test_build_sample_command_freebsd() {
        let cmd = build_sample_command("FreeBSD");
        assert!(cmd.contains("sockstat"));
    }

    #[test]
    fn test_build_sample_command_openbsd() {
        let cmd = build_sample_command("OpenBSD");
        assert!(cmd.contains("sockstat"));
    }

    #[test]
    fn test_build_sample_command_mingw() {
        let cmd = build_sample_command("Windows_MinGW");
        // MinGW uses Linux-style commands
        assert!(cmd.contains("ss -tlnp"));
    }

    #[test]
    fn test_build_sample_command_unknown_fallback() {
        let cmd = build_sample_command("SomeUnknownOS");
        // Falls back to Linux commands
        assert!(cmd.contains("ss -tlnp"));
        assert!(cmd.contains("===END==="));
    }

    // ─── CpuSnapshot arithmetic Tests ──────────────────────────────────

    #[test]
    fn test_cpu_snapshot_total() {
        let snap = CpuSnapshot {
            user: 100,
            nice: 10,
            system: 50,
            idle: 200,
            iowait: 5,
            irq: 2,
            softirq: 3,
            steal: 1,
        };
        assert_eq!(snap.total(), 371);
    }

    #[test]
    fn test_cpu_snapshot_active() {
        let snap = CpuSnapshot {
            user: 100,
            nice: 10,
            system: 50,
            idle: 200,
            iowait: 5,
            irq: 2,
            softirq: 3,
            steal: 1,
        };
        // active = total - idle - iowait = 371 - 200 - 5 = 166
        assert_eq!(snap.active(), 166);
    }
}
