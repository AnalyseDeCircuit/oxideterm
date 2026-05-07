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

    pub async fn create_forward(&self, rule: ForwardRule) -> Result<ForwardRule, ForwardingError> {
        if self.has_rule(&rule.id) {
            return Err(ForwardingError::AlreadyExists(rule.id));
        }

        self.emit_status_changed(&rule.id, ForwardStatus::Starting, None);
        let rule_id = rule.id.clone();
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

        match &result {
            Ok(active_rule) => {
                self.emit_status_changed(&active_rule.id, active_rule.status.clone(), None)
            }
            Err(error) => self.emit_status_changed(
                &rule_id,
                ForwardStatus::Error(error.to_string()),
                Some(error.to_string()),
            ),
        }
        result
    }

    pub async fn create_forward_with_health_check(
        &self,
        rule: ForwardRule,
        check_health: bool,
    ) -> Result<ForwardRule, ForwardingError> {
        if check_health && rule.forward_type != ForwardType::Dynamic {
            match self
                .check_port_available(&rule.target_host, rule.target_port, 3000)
                .await
            {
                Ok(true) => {}
                Ok(false) => {
                    return Err(ForwardingError::InvalidRule(format!(
                        "Target port {}:{} is not reachable. Please ensure the service is running on the remote server.\n\nTroubleshooting:\n- Check if service is running: ss -tlnp | grep {}\n- Verify the port number is correct\n- Try connecting manually: nc -zv {} {}",
                        rule.target_host,
                        rule.target_port,
                        rule.target_port,
                        rule.target_host,
                        rule.target_port
                    )));
                }
                Err(error) => {
                    return Err(ForwardingError::Ssh(format!(
                        "Failed to check port availability: {error}\n\nYou can skip this check with the 'Skip port availability check' option."
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
        self.stopped_forwards
            .get(rule_id)
            .map(|rule| rule.clone())
            .ok_or_else(|| ForwardingError::NotFound(rule_id.to_string()))
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
        let local_ids: Vec<String> = self
            .local_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        let dynamic_ids: Vec<String> = self
            .dynamic_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        let remote_ids: Vec<String> = self
            .remote_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        for rule_id in local_ids.into_iter().chain(dynamic_ids).chain(remote_ids) {
            let _ = self.stop_forward(&rule_id).await;
        }
    }

    pub async fn suspend_all_and_save_rules(&self) -> Vec<ForwardRule> {
        let mut suspended = Vec::new();
        let local_ids: Vec<String> = self
            .local_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        let dynamic_ids: Vec<String> = self
            .dynamic_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        let remote_ids: Vec<String> = self
            .remote_forwards
            .iter()
            .map(|entry| entry.key().clone())
            .collect();

        for rule_id in local_ids.into_iter().chain(dynamic_ids).chain(remote_ids) {
            if let Ok(mut rule) = self.stop_forward(&rule_id).await {
                rule.status = ForwardStatus::Suspended;
                self.stopped_forwards.insert(rule.id.clone(), rule.clone());
                self.emit_status_changed(&rule.id, ForwardStatus::Suspended, None);
                suspended.push(rule);
            }
        }

        if !suspended.is_empty() {
            self.emit_session_suspended(suspended.iter().map(|rule| rule.id.clone()).collect());
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

    fn emit_status_changed(&self, forward_id: &str, status: ForwardStatus, error: Option<String>) {
        self.emit(ForwardEvent::StatusChanged {
            forward_id: forward_id.to_string(),
            session_id: self.session_id.clone(),
            status,
            error,
        });
    }

    fn emit_session_suspended(&self, forward_ids: Vec<String>) {
        self.emit(ForwardEvent::SessionSuspended {
            session_id: self.session_id.clone(),
            forward_ids,
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
