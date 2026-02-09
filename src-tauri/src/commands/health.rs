//! Health Check & Resource Profiler Tauri Commands
//!
//! Provides commands for monitoring connection health and remote resource metrics.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;
use tauri::State;

use crate::session::health::ResourceMetrics;
use crate::session::profiler::{ProfilerState, ResourceProfiler};
use crate::session::{HealthMetrics, HealthStatus, HealthTracker, QuickHealthCheck};
use crate::ssh::SshConnectionRegistry;

/// Registry for health trackers
pub struct HealthRegistry {
    trackers: DashMap<String, Arc<HealthTracker>>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self {
            trackers: DashMap::new(),
        }
    }

    /// Register a new health tracker for a session
    pub fn register(&self, session_id: String) -> Arc<HealthTracker> {
        let tracker = Arc::new(HealthTracker::new(session_id.clone()));
        self.trackers.insert(session_id, tracker.clone());
        tracker
    }

    /// Get tracker for a session
    pub fn get(&self, session_id: &str) -> Option<Arc<HealthTracker>> {
        self.trackers.get(session_id).map(|r| r.value().clone())
    }

    /// Remove tracker for a session
    pub fn remove(&self, session_id: &str) {
        if let Some((_, tracker)) = self.trackers.remove(session_id) {
            tracker.deactivate();
        }
    }

    /// Get all session IDs
    pub fn session_ids(&self) -> Vec<String> {
        self.trackers.iter().map(|r| r.key().clone()).collect()
    }
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Get health metrics for a specific session
#[tauri::command]
pub async fn get_connection_health(
    session_id: String,
    health_registry: State<'_, HealthRegistry>,
) -> Result<HealthMetrics, String> {
    let tracker = health_registry
        .get(&session_id)
        .ok_or_else(|| format!("No health tracker for session: {}", session_id))?;

    Ok(tracker.metrics().await)
}

/// Get quick health check for a session (suitable for status indicators)
#[tauri::command]
pub async fn get_quick_health(
    session_id: String,
    health_registry: State<'_, HealthRegistry>,
) -> Result<QuickHealthCheck, String> {
    let tracker = health_registry
        .get(&session_id)
        .ok_or_else(|| format!("No health tracker for session: {}", session_id))?;

    let metrics = tracker.metrics().await;
    Ok(QuickHealthCheck::from_metrics(session_id, &metrics))
}

/// Get health status for all active sessions
#[tauri::command]
pub async fn get_all_health_status(
    health_registry: State<'_, HealthRegistry>,
) -> Result<HashMap<String, QuickHealthCheck>, String> {
    let session_ids = health_registry.session_ids();
    let mut results = HashMap::new();

    for session_id in session_ids {
        if let Some(tracker) = health_registry.get(&session_id) {
            if tracker.is_active() {
                let metrics = tracker.metrics().await;
                results.insert(
                    session_id.clone(),
                    QuickHealthCheck::from_metrics(session_id, &metrics),
                );
            }
        }
    }

    Ok(results)
}

/// Simulate a health response (for testing - in real usage this would be called
/// when SSH keepalive responses are received)
#[tauri::command]
pub async fn simulate_health_response(
    session_id: String,
    latency_ms: u64,
    health_registry: State<'_, HealthRegistry>,
) -> Result<(), String> {
    let tracker = health_registry
        .get(&session_id)
        .ok_or_else(|| format!("No health tracker for session: {}", session_id))?;

    tracker.record_sent();
    tracker.record_response(latency_ms).await;
    Ok(())
}

/// Frontend-friendly health status response
#[derive(serde::Serialize)]
pub struct HealthStatusResponse {
    pub session_id: String,
    pub status: String,
    pub status_color: String,
    pub latency_ms: Option<u64>,
    pub message: String,
    pub uptime_formatted: String,
}

impl HealthStatusResponse {
    pub fn from_check(check: QuickHealthCheck, uptime_secs: u64) -> Self {
        let (status, status_color) = match check.status {
            HealthStatus::Healthy => ("healthy", "#22c55e"), // green-500
            HealthStatus::Degraded => ("degraded", "#f59e0b"), // amber-500
            HealthStatus::Unresponsive => ("unresponsive", "#ef4444"), // red-500
            HealthStatus::Disconnected => ("disconnected", "#6b7280"), // gray-500
            HealthStatus::Unknown => ("unknown", "#9ca3af"), // gray-400
        };

        let uptime_formatted = format_uptime(uptime_secs);

        Self {
            session_id: check.session_id,
            status: status.to_string(),
            status_color: status_color.to_string(),
            latency_ms: check.latency_ms,
            message: check.message,
            uptime_formatted,
        }
    }
}

/// Format uptime as human-readable string
fn format_uptime(secs: u64) -> String {
    if secs < 60 {
        format!("{}s", secs)
    } else if secs < 3600 {
        let mins = secs / 60;
        let rem_secs = secs % 60;
        format!("{}m {}s", mins, rem_secs)
    } else if secs < 86400 {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        format!("{}h {}m", hours, mins)
    } else {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        format!("{}d {}h", days, hours)
    }
}

/// Get formatted health status for UI display
#[tauri::command]
pub async fn get_health_for_display(
    session_id: String,
    health_registry: State<'_, HealthRegistry>,
) -> Result<HealthStatusResponse, String> {
    let tracker = health_registry
        .get(&session_id)
        .ok_or_else(|| format!("No health tracker for session: {}", session_id))?;

    let metrics = tracker.metrics().await;
    let check = QuickHealthCheck::from_metrics(session_id, &metrics);

    Ok(HealthStatusResponse::from_check(check, metrics.uptime_secs))
}

// ─── Resource Profiler Registry & Commands ────────────────────────────────────

/// Registry for resource profilers (one per connection)
pub struct ProfilerRegistry {
    profilers: DashMap<String, ResourceProfiler>,
}

impl ProfilerRegistry {
    pub fn new() -> Self {
        Self {
            profilers: DashMap::new(),
        }
    }

    /// Stop and remove all profilers (for exit cleanup)
    pub fn stop_all(&self) {
        let keys: Vec<String> = self.profilers.iter().map(|r| r.key().clone()).collect();
        for key in keys {
            if let Some((_, mut profiler)) = self.profilers.remove(&key) {
                profiler.stop();
            }
        }
    }

    /// Stop and remove a single profiler (for disconnect cleanup)
    pub fn remove(&self, connection_id: &str) {
        if let Some((_, mut profiler)) = self.profilers.remove(connection_id) {
            profiler.stop();
        }
    }
}

impl Default for ProfilerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Start resource profiling for a connection
///
/// Idempotent: if a profiler is already running for this connection, returns Ok.
/// This prevents React StrictMode double-mount from spawning duplicate profilers.
#[tauri::command]
pub async fn start_resource_profiler(
    connection_id: String,
    profiler_registry: State<'_, ProfilerRegistry>,
    connection_registry: State<'_, Arc<SshConnectionRegistry>>,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    // Idempotent: if already running, just return Ok
    if let Some(entry) = profiler_registry.profilers.get(&connection_id) {
        let state = entry.state().await;
        if state == ProfilerState::Running {
            return Ok(());
        }
        // Stopped or Degraded — drop the old entry and respawn below
        drop(entry);
        profiler_registry.profilers.remove(&connection_id);
    }

    // Get HandleController for the connection
    let controller = connection_registry
        .get_handle_controller(&connection_id)
        .ok_or_else(|| format!("Connection not found: {}", connection_id))?;

    let profiler = ResourceProfiler::spawn(connection_id.clone(), controller, app_handle);
    profiler_registry.profilers.insert(connection_id, profiler);

    Ok(())
}

/// Stop resource profiling for a connection
#[tauri::command]
pub async fn stop_resource_profiler(
    connection_id: String,
    profiler_registry: State<'_, ProfilerRegistry>,
) -> Result<(), String> {
    if let Some((_, mut profiler)) = profiler_registry.profilers.remove(&connection_id) {
        profiler.stop();
        Ok(())
    } else {
        // Not an error — idempotent stop
        Ok(())
    }
}

/// Get latest resource metrics for a connection
#[tauri::command]
pub async fn get_resource_metrics(
    connection_id: String,
    profiler_registry: State<'_, ProfilerRegistry>,
) -> Result<Option<ResourceMetrics>, String> {
    if let Some(entry) = profiler_registry.profilers.get(&connection_id) {
        Ok(entry.latest().await)
    } else {
        Ok(None)
    }
}

/// Get resource metrics history for sparkline rendering
#[tauri::command]
pub async fn get_resource_history(
    connection_id: String,
    profiler_registry: State<'_, ProfilerRegistry>,
) -> Result<Vec<ResourceMetrics>, String> {
    if let Some(entry) = profiler_registry.profilers.get(&connection_id) {
        Ok(entry.history().await)
    } else {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(45), "45s");
        assert_eq!(format_uptime(90), "1m 30s");
        assert_eq!(format_uptime(3661), "1h 1m");
        assert_eq!(format_uptime(90061), "1d 1h");
    }

    #[test]
    fn test_profiler_registry_new() {
        let registry = ProfilerRegistry::new();
        assert!(registry.profilers.is_empty());
    }
}
