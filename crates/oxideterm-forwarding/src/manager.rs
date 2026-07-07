// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    net::TcpListener as StdTcpListener,
    sync::mpsc::Sender,
    sync::{Arc, Mutex},
    time::Duration,
};

use dashmap::DashMap;
use oxideterm_ssh::SshConnectionHandle;

use crate::{
    ForwardEvent, ForwardRule, ForwardStats, ForwardStatus, ForwardType, ForwardUpdate,
    ForwardingError, PortDetectionSnapshot, PortDetectionTracker,
    detection::{
        PORT_SCAN_MAX_OUTPUT_SIZE, PORT_SCAN_TIMEOUT_SECS, REMOTE_OS_PROBE_TIMEOUT_SECS,
        REMOTE_OS_PROBE_UNIX, REMOTE_OS_PROBE_WINDOWS, RemotePortScanPlatform,
    },
    dynamic::DynamicForward,
    local::LocalForward,
    remote::{RemoteForward, RemoteForwardRouter},
};

pub struct ForwardingManager {
    session_id: String,
    ssh_connection: Mutex<SshConnectionHandle>,
    event_tx: Option<Sender<ForwardEvent>>,
    remote_router: Arc<RemoteForwardRouter>,
    local_forwards: DashMap<String, LocalForward>,
    remote_forwards: DashMap<String, RemoteForward>,
    dynamic_forwards: DashMap<String, DynamicForward>,
    stopped_forwards: DashMap<String, ForwardRule>,
    port_detection: Mutex<PortDetectionTracker>,
    port_scan_platform: Mutex<Option<RemotePortScanPlatform>>,
}

impl ForwardingManager {
    pub fn new(session_id: impl Into<String>, ssh_connection: SshConnectionHandle) -> Self {
        Self::new_with_event_sender(session_id, ssh_connection, None)
    }

    pub fn new_with_event_sender(
        session_id: impl Into<String>,
        ssh_connection: SshConnectionHandle,
        event_tx: Option<Sender<ForwardEvent>>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            ssh_connection: Mutex::new(ssh_connection),
            event_tx,
            remote_router: Arc::new(RemoteForwardRouter::default()),
            local_forwards: DashMap::new(),
            remote_forwards: DashMap::new(),
            dynamic_forwards: DashMap::new(),
            stopped_forwards: DashMap::new(),
            port_detection: Mutex::new(PortDetectionTracker::default()),
            port_scan_platform: Mutex::new(None),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub async fn create_forward(
        &self,
        mut rule: ForwardRule,
    ) -> Result<ForwardRule, ForwardingError> {
        rule.normalize_hosts_for_runtime();
        if self.has_rule(&rule.id) {
            return Err(ForwardingError::AlreadyExists(rule.id));
        }

        let result = match rule.forward_type {
            ForwardType::Local => LocalForward::start(rule, self.current_ssh_connection())
                .await
                .map(|forward| {
                    let active_rule = forward.rule();
                    self.local_forwards.insert(active_rule.id.clone(), forward);
                    active_rule
                }),
            ForwardType::Dynamic => DynamicForward::start(rule, self.current_ssh_connection())
                .await
                .map(|forward| {
                    let active_rule = forward.rule();
                    self.dynamic_forwards
                        .insert(active_rule.id.clone(), forward);
                    active_rule
                }),
            ForwardType::Remote => RemoteForward::start(
                rule,
                self.current_ssh_connection(),
                self.remote_router.clone(),
            )
            .await
            .map(|forward| {
                let active_rule = forward.rule();
                self.remote_forwards.insert(active_rule.id.clone(), forward);
                active_rule
            }),
        };

        if let Ok(active_rule) = &result {
            self.emit_status_changed(&active_rule.id, active_rule.status.clone(), None);
        }
        result
    }

    pub async fn create_forward_with_health_check(
        &self,
        mut rule: ForwardRule,
        check_health: bool,
    ) -> Result<ForwardRule, ForwardingError> {
        rule.normalize_hosts_for_runtime();
        if check_health && rule.forward_type != ForwardType::Dynamic {
            match self
                .check_port_available(&rule.target_host, rule.target_port, 3000)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    return Err(ForwardingError::InvalidRule(build_unreachable_port_error(
                        &rule.target_host,
                        rule.target_port,
                    )));
                }
                Err(error) => {
                    return Err(ForwardingError::Ssh(build_health_check_error_message(
                        &error.to_string(),
                    )));
                }
            }
        }

        self.create_forward(rule).await
    }

    pub async fn stop_forward(&self, rule_id: &str) -> Result<ForwardRule, ForwardingError> {
        if let Some((_, forward)) = self.local_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.stopped_forwards
                .insert(stopped.id.clone(), stopped.clone());
            self.emit_status_changed(&stopped.id, stopped.status.clone(), None);
            return Ok(stopped);
        }
        if let Some((_, forward)) = self.dynamic_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.stopped_forwards
                .insert(stopped.id.clone(), stopped.clone());
            self.emit_status_changed(&stopped.id, stopped.status.clone(), None);
            return Ok(stopped);
        }
        if let Some((_, forward)) = self.remote_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.stopped_forwards
                .insert(stopped.id.clone(), stopped.clone());
            self.emit_status_changed(&stopped.id, stopped.status.clone(), None);
            return Ok(stopped);
        }
        Err(ForwardingError::NotFound(rule_id.to_string()))
    }

    pub async fn restart_forward(&self, rule_id: &str) -> Result<ForwardRule, ForwardingError> {
        let Some((_, mut rule)) = self.stopped_forwards.remove(rule_id) else {
            return Err(ForwardingError::NotFound(rule_id.to_string()));
        };
        rule.status = ForwardStatus::Starting;

        match self.create_forward(rule.clone()).await {
            Ok(active) => Ok(active),
            Err(error) => {
                let mut restored = rule;
                restored.status = ForwardStatus::Stopped;
                self.stopped_forwards.insert(restored.id.clone(), restored);
                Err(error)
            }
        }
    }

    pub async fn delete_forward(&self, rule_id: &str) -> Result<(), ForwardingError> {
        if let Some((_, forward)) = self.local_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.emit_status_changed(&stopped.id, stopped.status, None);
            return Ok(());
        }
        if let Some((_, forward)) = self.dynamic_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.emit_status_changed(&stopped.id, stopped.status, None);
            return Ok(());
        }
        if let Some((_, forward)) = self.remote_forwards.remove(rule_id) {
            let stopped = forward.stop().await;
            self.emit_status_changed(&stopped.id, stopped.status, None);
            return Ok(());
        }
        self.stopped_forwards
            .remove(rule_id)
            .map(|_| ())
            .ok_or_else(|| ForwardingError::NotFound(rule_id.to_string()))
    }

    pub fn update_stopped_forward(
        &self,
        rule_id: &str,
        update: ForwardUpdate,
    ) -> Result<ForwardRule, ForwardingError> {
        if self.local_forwards.contains_key(rule_id)
            || self.dynamic_forwards.contains_key(rule_id)
            || self.remote_forwards.contains_key(rule_id)
        {
            return Err(ForwardingError::ActiveRuleCannotBeEdited(
                rule_id.to_string(),
            ));
        }

        let Some(mut rule) = self.stopped_forwards.get_mut(rule_id) else {
            return Err(ForwardingError::NotFound(rule_id.to_string()));
        };
        rule.apply_update(update);
        rule.status = ForwardStatus::Stopped;
        let updated = rule.clone();
        drop(rule);
        self.emit_status_changed(&updated.id, updated.status.clone(), None);
        Ok(updated)
    }

    pub fn update_forward(
        &self,
        rule_id: &str,
        update: ForwardUpdate,
    ) -> Result<ForwardRule, ForwardingError> {
        // Tauri exposes this as update_forward/node_update_forward, but the
        // backend contract is still stopped-only. Keep the public name aligned
        // with Tauri while retaining the explicit native helper for call sites
        // that need to document the stopped-rule constraint.
        self.update_stopped_forward(rule_id, update)
    }

    pub fn list_forwards(&self) -> Vec<ForwardRule> {
        let mut rules = Vec::new();
        rules.extend(self.local_forwards.iter().map(|entry| entry.rule()));
        rules.extend(self.remote_forwards.iter().map(|entry| entry.rule()));
        rules.extend(self.dynamic_forwards.iter().map(|entry| entry.rule()));
        rules.extend(self.stopped_forwards.iter().map(|entry| entry.clone()));
        rules.sort_by(|left, right| left.id.cmp(&right.id));
        rules
    }

    pub fn get_stats(&self, rule_id: &str) -> Result<ForwardStats, ForwardingError> {
        if let Some(forward) = self.local_forwards.get(rule_id) {
            let stats = forward.stats();
            self.emit_stats_updated(rule_id, stats.clone());
            return Ok(stats);
        }
        if let Some(forward) = self.dynamic_forwards.get(rule_id) {
            let stats = forward.stats();
            self.emit_stats_updated(rule_id, stats.clone());
            return Ok(stats);
        }
        if let Some(forward) = self.remote_forwards.get(rule_id) {
            let stats = forward.stats();
            self.emit_stats_updated(rule_id, stats.clone());
            return Ok(stats);
        }
        if self.stopped_forwards.contains_key(rule_id) {
            let stats = ForwardStats::default();
            self.emit_stats_updated(rule_id, stats.clone());
            return Ok(stats);
        }
        Err(ForwardingError::NotFound(rule_id.to_string()))
    }

    pub async fn check_port_available(
        &self,
        host: &str,
        port: u16,
        timeout_ms: u64,
    ) -> Result<bool, ForwardingError> {
        if host.trim().is_empty() || port == 0 {
            return Err(ForwardingError::InvalidRule(
                "target host and port are required".to_string(),
            ));
        }

        let connection = self.current_ssh_connection();
        let check = connection.open_direct_tcpip(host, port, "127.0.0.1", 0);
        match tokio::time::timeout(Duration::from_millis(timeout_ms), check).await {
            Ok(Ok(_stream)) => Ok(true),
            Ok(Err(error)) => {
                let message = error.to_string().to_lowercase();
                if message.contains("connection refused")
                    || message.contains("connect failed")
                    || message.contains("connectfailed")
                    || message.contains("refused")
                    || message.contains("failed to open channel")
                {
                    Ok(false)
                } else {
                    Err(ForwardingError::Ssh(error.to_string()))
                }
            }
            Err(_) => Err(ForwardingError::Ssh(format!(
                "Timeout checking port {host}:{port} ({timeout_ms}ms)"
            ))),
        }
    }

    pub async fn forward_jupyter(
        &self,
        local_port: u16,
        remote_port: u16,
    ) -> Result<ForwardRule, ForwardingError> {
        let mut rule = ForwardRule::local("127.0.0.1", local_port, "localhost", remote_port);
        rule.description = format!("Jupyter Notebook ({remote_port})");
        self.create_forward(rule).await
    }

    pub async fn forward_tensorboard(
        &self,
        local_port: u16,
        remote_port: u16,
    ) -> Result<ForwardRule, ForwardingError> {
        let mut rule = ForwardRule::local("127.0.0.1", local_port, "localhost", remote_port);
        rule.description = format!("TensorBoard ({remote_port})");
        self.create_forward(rule).await
    }

    pub async fn forward_vscode(
        &self,
        local_port: u16,
        remote_port: u16,
    ) -> Result<ForwardRule, ForwardingError> {
        let mut rule = ForwardRule::local("127.0.0.1", local_port, "localhost", remote_port);
        rule.description = format!("VS Code Server ({remote_port})");
        self.create_forward(rule).await
    }

    pub async fn scan_remote_ports(&self) -> Result<PortDetectionSnapshot, ForwardingError> {
        let platform = self.detect_remote_port_scan_platform().await;
        let output = self
            .ssh_connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .run_command(
                platform.scan_command(),
                Duration::from_secs(PORT_SCAN_TIMEOUT_SECS),
                PORT_SCAN_MAX_OUTPUT_SIZE,
            )
            .await?;
        if !output.contains("===END===") {
            return Err(ForwardingError::Ssh(
                "remote port scan output was truncated".to_string(),
            ));
        }
        let ports = crate::detection::parse_listening_ports(&output, platform);
        Ok(self
            .port_detection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .apply_scan(ports))
    }

    pub fn detected_ports(&self) -> PortDetectionSnapshot {
        self.port_detection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .snapshot()
    }

    pub fn ignore_detected_port(&self, port: u16) {
        self.port_detection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .ignore_port(port);
    }

    async fn detect_remote_port_scan_platform(&self) -> RemotePortScanPlatform {
        if let Some(platform) = *self
            .port_scan_platform
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
        {
            return platform;
        }

        let platform = match self
            .ssh_connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .run_command(
                REMOTE_OS_PROBE_UNIX,
                Duration::from_secs(REMOTE_OS_PROBE_TIMEOUT_SECS),
                PORT_SCAN_MAX_OUTPUT_SIZE,
            )
            .await
        {
            Ok(output) => {
                let platform = crate::detection::classify_remote_platform(&output);
                if platform == RemotePortScanPlatform::Unknown {
                    self.detect_windows_port_scan_platform().await
                } else {
                    platform
                }
            }
            Err(_) => self.detect_windows_port_scan_platform().await,
        };

        *self
            .port_scan_platform
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(platform);
        platform
    }

    async fn detect_windows_port_scan_platform(&self) -> RemotePortScanPlatform {
        match self
            .ssh_connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .run_command(
                REMOTE_OS_PROBE_WINDOWS,
                Duration::from_secs(REMOTE_OS_PROBE_TIMEOUT_SECS),
                PORT_SCAN_MAX_OUTPUT_SIZE,
            )
            .await
        {
            Ok(output) => {
                let platform = crate::detection::classify_remote_platform(&output);
                if platform == RemotePortScanPlatform::Windows {
                    RemotePortScanPlatform::Windows
                } else {
                    RemotePortScanPlatform::Unknown
                }
            }
            Err(_) => RemotePortScanPlatform::Unknown,
        }
    }

    pub async fn stop_all(&self) {
        // Tauri `stop_all` drains active handles without preserving them in
        // `stopped_forwards`; only explicit per-rule stop keeps a restartable
        // stopped row. Keep native's bulk stop destructive in the same way.
        let local_forwards: Vec<LocalForward> = self
            .local_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .filter_map(|rule_id| {
                self.local_forwards
                    .remove(&rule_id)
                    .map(|(_, forward)| forward)
            })
            .collect();
        let dynamic_forwards: Vec<DynamicForward> = self
            .dynamic_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .filter_map(|rule_id| {
                self.dynamic_forwards
                    .remove(&rule_id)
                    .map(|(_, forward)| forward)
            })
            .collect();
        let remote_forwards: Vec<RemoteForward> = self
            .remote_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .filter_map(|rule_id| {
                self.remote_forwards
                    .remove(&rule_id)
                    .map(|(_, forward)| forward)
            })
            .collect();

        for forward in local_forwards {
            let _ = forward.stop().await;
        }
        for forward in dynamic_forwards {
            let _ = forward.stop().await;
        }
        for forward in remote_forwards {
            let _ = forward.stop().await;
        }
    }

    pub async fn suspend_all_and_save_rules(&self) -> Vec<ForwardRule> {
        let mut suspended = Vec::new();
        let local_forwards: Vec<LocalForward> = self
            .local_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .filter_map(|rule_id| {
                self.local_forwards
                    .remove(&rule_id)
                    .map(|(_, forward)| forward)
            })
            .collect();
        let dynamic_forwards: Vec<DynamicForward> = self
            .dynamic_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .filter_map(|rule_id| {
                self.dynamic_forwards
                    .remove(&rule_id)
                    .map(|(_, forward)| forward)
            })
            .collect();
        let remote_forwards: Vec<RemoteForward> = self
            .remote_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .filter_map(|rule_id| {
                self.remote_forwards
                    .remove(&rule_id)
                    .map(|(_, forward)| forward)
            })
            .collect();

        for forward in local_forwards {
            let mut rule = forward.stop().await;
            rule.status = ForwardStatus::Suspended;
            self.stopped_forwards.insert(rule.id.clone(), rule.clone());
            self.emit_status_changed(
                &rule.id,
                ForwardStatus::Suspended,
                Some("SSH connection lost".to_string()),
            );
            suspended.push(rule);
        }
        for forward in dynamic_forwards {
            let mut rule = forward.stop().await;
            rule.status = ForwardStatus::Suspended;
            self.stopped_forwards.insert(rule.id.clone(), rule.clone());
            self.emit_status_changed(
                &rule.id,
                ForwardStatus::Suspended,
                Some("SSH connection lost".to_string()),
            );
            suspended.push(rule);
        }
        for forward in remote_forwards {
            let mut rule = forward.stop().await;
            rule.status = ForwardStatus::Suspended;
            self.stopped_forwards.insert(rule.id.clone(), rule.clone());
            self.emit_status_changed(
                &rule.id,
                ForwardStatus::Suspended,
                Some("SSH connection lost".to_string()),
            );
            suspended.push(rule);
        }
        suspended
    }

    pub fn list_stopped_forwards(&self) -> Vec<ForwardRule> {
        let mut rules: Vec<ForwardRule> = self
            .stopped_forwards
            .iter()
            .map(|entry| entry.clone())
            .collect();
        rules.sort_by(|left, right| left.id.cmp(&right.id));
        rules
    }

    pub async fn restore_saved_forwards(
        &self,
        ssh_connection: SshConnectionHandle,
    ) -> Vec<Result<ForwardRule, ForwardingError>> {
        self.replace_ssh_connection(ssh_connection);
        let rules = self.list_stopped_forwards();
        let mut restored = Vec::with_capacity(rules.len());

        for rule in rules {
            self.stopped_forwards.remove(&rule.id);
            let mut restarting = rule;
            restarting.status = ForwardStatus::Starting;
            let result = self.create_forward(restarting.clone()).await;
            if result.is_err() {
                let mut suspended = restarting;
                suspended.status = ForwardStatus::Suspended;
                self.stopped_forwards
                    .insert(suspended.id.clone(), suspended.clone());
            }
            restored.push(result);
        }
        restored
    }

    pub fn replace_ssh_connection(&self, ssh_connection: SshConnectionHandle) {
        *self
            .ssh_connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = ssh_connection;
        *self
            .port_scan_platform
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    pub fn local_bind_port_available(bind_address: &str, bind_port: u16) -> bool {
        StdTcpListener::bind((bind_address, bind_port)).is_ok()
    }

    fn has_rule(&self, rule_id: &str) -> bool {
        self.local_forwards.contains_key(rule_id)
            || self.dynamic_forwards.contains_key(rule_id)
            || self.remote_forwards.contains_key(rule_id)
            || self.stopped_forwards.contains_key(rule_id)
    }

    fn current_ssh_connection(&self) -> SshConnectionHandle {
        self.ssh_connection
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn ssh_connection_handle(&self) -> SshConnectionHandle {
        self.current_ssh_connection()
    }

    fn emit_status_changed(&self, forward_id: &str, status: ForwardStatus, error: Option<String>) {
        self.emit(ForwardEvent::StatusChanged {
            forward_id: forward_id.to_string(),
            session_id: self.session_id.clone(),
            status,
            error,
        });
    }

    fn emit_stats_updated(&self, forward_id: &str, stats: ForwardStats) {
        self.emit(ForwardEvent::StatsUpdated {
            forward_id: forward_id.to_string(),
            session_id: self.session_id.clone(),
            stats,
        });
    }

    fn emit(&self, event: ForwardEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event);
        }
    }
}

impl std::fmt::Debug for ForwardingManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ForwardingManager")
            .field("session_id", &self.session_id)
            .field("local_forwards", &self.local_forwards.len())
            .field("remote_forwards", &self.remote_forwards.len())
            .field("dynamic_forwards", &self.dynamic_forwards.len())
            .field("stopped_forwards", &self.stopped_forwards.len())
            .finish()
    }
}

fn build_unreachable_port_error(target_host: &str, target_port: u16) -> String {
    format!(
        "Target port {}:{} is not reachable. Please ensure the service is running on the remote server.\n\nTroubleshooting:\n• Check if service is running: ss -tlnp | grep {}\n• Verify the port number is correct\n• Try connecting manually: nc -zv {} {}",
        target_host, target_port, target_port, target_host, target_port
    )
}

fn build_health_check_error_message(error: &str) -> String {
    format!(
        "Failed to check port availability: {}\n\nYou can skip this check with the 'Skip port availability check' option.",
        error
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_check_messages_match_tauri_node_forwarding() {
        let unreachable = build_unreachable_port_error("service.internal", 3000);
        assert!(unreachable.contains("Target port service.internal:3000 is not reachable"));
        assert!(unreachable.contains("• Check if service is running: ss -tlnp | grep 3000"));
        assert!(unreachable.contains("• Verify the port number is correct"));
        assert!(unreachable.contains("• Try connecting manually: nc -zv service.internal 3000"));

        let failed = build_health_check_error_message("timeout");
        assert_eq!(
            failed,
            "Failed to check port availability: timeout\n\nYou can skip this check with the 'Skip port availability check' option."
        );
    }
}
