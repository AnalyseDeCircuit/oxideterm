//! Connection Health Check Module
//!
//! Monitors SSH connection health and provides metrics for UI display.
//! Uses SSH keepalive responses to track connection quality.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::debug;

/// Connection health status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    /// Connection is healthy with good response times
    Healthy,
    /// Connection is degraded (high latency or packet loss)
    Degraded,
    /// Connection appears to be stale or unresponsive
    Unresponsive,
    /// Connection is disconnected
    Disconnected,
    /// Health status unknown (not enough data)
    #[default]
    Unknown,
}

/// Health metrics for a connection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthMetrics {
    /// Current health status
    pub status: HealthStatus,
    /// Last known latency in milliseconds
    pub latency_ms: Option<u64>,
    /// Average latency over recent samples (ms)
    pub avg_latency_ms: Option<u64>,
    /// Packet loss percentage (0-100)
    pub packet_loss_percent: u8,
    /// Time since last successful keepalive response
    pub last_response_ago_ms: Option<u64>,
    /// Total keepalive packets sent
    pub packets_sent: u64,
    /// Total keepalive responses received
    pub packets_received: u64,
    /// Connection uptime in seconds
    pub uptime_secs: u64,
}

impl Default for HealthMetrics {
    fn default() -> Self {
        Self {
            status: HealthStatus::Unknown,
            latency_ms: None,
            avg_latency_ms: None,
            packet_loss_percent: 0,
            last_response_ago_ms: None,
            packets_sent: 0,
            packets_received: 0,
            uptime_secs: 0,
        }
    }
}

/// Remote host resource metrics (single sample)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetrics {
    /// Timestamp of the sample (ms since epoch)
    pub timestamp_ms: u64,
    /// CPU usage percentage (0-100), None if no delta baseline yet
    pub cpu_percent: Option<f64>,
    /// Memory used in bytes
    pub memory_used: Option<u64>,
    /// Total memory in bytes
    pub memory_total: Option<u64>,
    /// Memory usage percentage (0-100)
    pub memory_percent: Option<f64>,
    /// 1-minute load average
    pub load_avg_1: Option<f64>,
    /// 5-minute load average
    pub load_avg_5: Option<f64>,
    /// 15-minute load average
    pub load_avg_15: Option<f64>,
    /// Number of CPU cores
    pub cpu_cores: Option<u32>,
    /// Network RX bytes per second
    pub net_rx_bytes_per_sec: Option<u64>,
    /// Network TX bytes per second
    pub net_tx_bytes_per_sec: Option<u64>,
    /// SSH RTT in milliseconds (from HealthTracker)
    pub ssh_rtt_ms: Option<u64>,
    /// Source quality of the metrics
    pub source: MetricsSource,
}

/// Quality indicator for resource metrics
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricsSource {
    /// All /proc metrics parsed successfully
    Full,
    /// Some /proc metrics parsed, others failed
    Partial,
    /// Only SSH RTT available (non-Linux or /proc unavailable)
    RttOnly,
    /// Sampling failed entirely
    Failed,
}

/// Health check thresholds
#[derive(Debug, Clone)]
pub struct HealthThresholds {
    /// Latency above this is considered degraded (ms)
    pub degraded_latency_ms: u64,
    /// Latency above this is considered unresponsive (ms)
    pub unresponsive_latency_ms: u64,
    /// Packet loss above this percentage is degraded
    pub degraded_loss_percent: u8,
    /// Time without response before considered unresponsive (ms)
    pub unresponsive_timeout_ms: u64,
    /// Number of latency samples to average
    pub latency_sample_count: usize,
}

impl Default for HealthThresholds {
    fn default() -> Self {
        Self {
            degraded_latency_ms: 200,       // 200ms is noticeable
            unresponsive_latency_ms: 2000,  // 2s is very slow
            degraded_loss_percent: 5,       // 5% loss is noticeable
            unresponsive_timeout_ms: 60000, // 1 minute without response
            latency_sample_count: 10,       // Average over 10 samples
        }
    }
}

/// Connection health tracker
pub struct HealthTracker {
    /// Session ID being tracked
    session_id: String,
    /// Connection start time
    connected_at: Instant,
    /// Last response time
    last_response: RwLock<Option<Instant>>,
    /// Recent latency samples (circular buffer)
    latency_samples: RwLock<Vec<u64>>,
    /// Packets sent counter
    packets_sent: AtomicU64,
    /// Packets received counter  
    packets_received: AtomicU64,
    /// Thresholds for health evaluation
    thresholds: HealthThresholds,
    /// Whether tracking is active
    active: AtomicBool,
}

impl HealthTracker {
    /// Create a new health tracker
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            connected_at: Instant::now(),
            last_response: RwLock::new(Some(Instant::now())),
            latency_samples: RwLock::new(Vec::new()),
            packets_sent: AtomicU64::new(0),
            packets_received: AtomicU64::new(0),
            thresholds: HealthThresholds::default(),
            active: AtomicBool::new(true),
        }
    }

    /// Create with custom thresholds
    pub fn with_thresholds(session_id: String, thresholds: HealthThresholds) -> Self {
        Self {
            session_id,
            connected_at: Instant::now(),
            last_response: RwLock::new(Some(Instant::now())),
            latency_samples: RwLock::new(Vec::new()),
            packets_sent: AtomicU64::new(0),
            packets_received: AtomicU64::new(0),
            thresholds,
            active: AtomicBool::new(true),
        }
    }

    /// Record a keepalive packet sent
    pub fn record_sent(&self) {
        self.packets_sent.fetch_add(1, Ordering::SeqCst);
        debug!(
            "Health[{}]: keepalive sent (total: {})",
            self.session_id,
            self.packets_sent.load(Ordering::SeqCst)
        );
    }

    /// Record a keepalive response received with latency
    pub async fn record_response(&self, latency_ms: u64) {
        self.packets_received.fetch_add(1, Ordering::SeqCst);
        *self.last_response.write().await = Some(Instant::now());

        // Add to latency samples (circular buffer)
        let mut samples = self.latency_samples.write().await;
        if samples.len() >= self.thresholds.latency_sample_count {
            samples.remove(0);
        }
        samples.push(latency_ms);

        debug!(
            "Health[{}]: response received, latency={}ms",
            self.session_id, latency_ms
        );
    }

    /// Mark tracker as inactive (connection closed)
    pub fn deactivate(&self) {
        self.active.store(false, Ordering::SeqCst);
    }

    /// Check if tracker is active
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::SeqCst)
    }

    /// Get current health metrics
    pub async fn metrics(&self) -> HealthMetrics {
        let last_response = self.last_response.read().await;
        let samples = self.latency_samples.read().await;

        let packets_sent = self.packets_sent.load(Ordering::SeqCst);
        let packets_received = self.packets_received.load(Ordering::SeqCst);

        // Calculate packet loss
        let packet_loss_percent = if packets_sent > 0 {
            ((packets_sent - packets_received) * 100 / packets_sent) as u8
        } else {
            0
        };

        // Calculate average latency
        let avg_latency_ms = if !samples.is_empty() {
            Some(samples.iter().sum::<u64>() / samples.len() as u64)
        } else {
            None
        };

        // Latest latency
        let latency_ms = samples.last().copied();

        // Time since last response
        let last_response_ago_ms = last_response.map(|t| t.elapsed().as_millis() as u64);

        // Uptime
        let uptime_secs = self.connected_at.elapsed().as_secs();

        // Determine status
        let status = self.evaluate_status(latency_ms, packet_loss_percent, last_response_ago_ms);

        HealthMetrics {
            status,
            latency_ms,
            avg_latency_ms,
            packet_loss_percent,
            last_response_ago_ms,
            packets_sent,
            packets_received,
            uptime_secs,
        }
    }

    /// Evaluate health status based on current metrics
    fn evaluate_status(
        &self,
        latency_ms: Option<u64>,
        packet_loss_percent: u8,
        last_response_ago_ms: Option<u64>,
    ) -> HealthStatus {
        if !self.is_active() {
            return HealthStatus::Disconnected;
        }

        // Check for unresponsive (no recent response)
        if let Some(ago) = last_response_ago_ms {
            if ago > self.thresholds.unresponsive_timeout_ms {
                return HealthStatus::Unresponsive;
            }
        }

        // Check latency
        if let Some(latency) = latency_ms {
            if latency > self.thresholds.unresponsive_latency_ms {
                return HealthStatus::Unresponsive;
            }
            if latency > self.thresholds.degraded_latency_ms {
                return HealthStatus::Degraded;
            }
        }

        // Check packet loss
        if packet_loss_percent > self.thresholds.degraded_loss_percent {
            return HealthStatus::Degraded;
        }

        // If we have recent data and it looks good, we're healthy
        if latency_ms.is_some() || last_response_ago_ms.map(|a| a < 35000).unwrap_or(false) {
            return HealthStatus::Healthy;
        }

        // Not enough data
        HealthStatus::Unknown
    }
}

/// Quick health check result for UI display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickHealthCheck {
    pub session_id: String,
    pub status: HealthStatus,
    pub latency_ms: Option<u64>,
    pub message: String,
}

impl QuickHealthCheck {
    pub fn from_metrics(session_id: String, metrics: &HealthMetrics) -> Self {
        let message = match metrics.status {
            HealthStatus::Healthy => {
                if let Some(lat) = metrics.latency_ms {
                    format!("Connected • {}ms", lat)
                } else {
                    "Connected".to_string()
                }
            }
            HealthStatus::Degraded => {
                let mut parts = Vec::new();
                if let Some(lat) = metrics.latency_ms {
                    parts.push(format!("High latency: {}ms", lat));
                }
                if metrics.packet_loss_percent > 0 {
                    parts.push(format!("{}% loss", metrics.packet_loss_percent));
                }
                if parts.is_empty() {
                    "Connection degraded".to_string()
                } else {
                    parts.join(" • ")
                }
            }
            HealthStatus::Unresponsive => "Connection unresponsive".to_string(),
            HealthStatus::Disconnected => "Disconnected".to_string(),
            HealthStatus::Unknown => "Checking connection...".to_string(),
        };

        Self {
            session_id,
            status: metrics.status,
            latency_ms: metrics.latency_ms,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_tracker_basic() {
        let tracker = HealthTracker::new("test-session".to_string());

        // Initial state
        let metrics = tracker.metrics().await;
        assert!(matches!(
            metrics.status,
            HealthStatus::Unknown | HealthStatus::Healthy
        ));

        // Record some data
        tracker.record_sent();
        tracker.record_response(50).await;

        let metrics = tracker.metrics().await;
        assert_eq!(metrics.packets_sent, 1);
        assert_eq!(metrics.packets_received, 1);
        assert_eq!(metrics.latency_ms, Some(50));
        assert_eq!(metrics.status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_health_degraded() {
        let tracker = HealthTracker::new("test-session".to_string());

        // High latency
        tracker.record_sent();
        tracker.record_response(500).await; // Above degraded threshold

        let metrics = tracker.metrics().await;
        assert_eq!(metrics.status, HealthStatus::Degraded);
    }

    #[tokio::test]
    async fn test_packet_loss() {
        let tracker = HealthTracker::new("test-session".to_string());

        // Send 10, receive 8 = 20% loss
        for _ in 0..10 {
            tracker.record_sent();
        }
        for _ in 0..8 {
            tracker.record_response(50).await;
        }

        let metrics = tracker.metrics().await;
        assert_eq!(metrics.packet_loss_percent, 20);
        assert_eq!(metrics.status, HealthStatus::Degraded);
    }
}
