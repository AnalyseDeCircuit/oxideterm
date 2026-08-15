// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::delivery;
#[cfg(test)]
use oxideterm_ssh::ConnectionPoolConfig;
use oxideterm_ssh::{ReconnectForwardRuleSnapshot, SshConnectionRegistry};
use std::{
    collections::HashSet,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const FORWARDING_SESSION_SHUTTING_DOWN: &str = "workspace forwarding session is shutting down";
const WORKSPACE_SESSION_SERVICE_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum ForwardingQuickAction {
    Jupyter,
    Tensorboard,
    Vscode,
}

pub(in crate::workspace) enum ForwardingRuntimeOperation {
    Create {
        rule: ForwardRule,
        check_health: bool,
    },
    Update {
        forward_id: String,
        update: ForwardUpdate,
    },
    Delete {
        forward_id: String,
    },
    Stop {
        forward_id: String,
    },
    Restart {
        forward_id: String,
    },
    Quick {
        action: ForwardingQuickAction,
        port: u16,
    },
}

pub(in crate::workspace) struct ReconnectForwardRestoreRequest {
    pub(in crate::workspace) root_node_id: NodeId,
    pub(in crate::workspace) snapshots: Vec<ReconnectForwardRuleSnapshot>,
    pub(in crate::workspace) old_connection_ids_by_node: HashMap<String, String>,
    pub(in crate::workspace) owner_connection_ids: HashMap<String, Option<String>>,
    pub(in crate::workspace) cancellation: Arc<AtomicBool>,
    pub(in crate::workspace) job_id: String,
}

#[derive(Clone, Default, Eq, PartialEq)]
pub(super) struct ForwardingRuntimeSnapshot {
    pub(super) rules: Vec<ForwardRule>,
    pub(super) stats_by_forward_id: HashMap<String, ForwardStats>,
}

pub(in crate::workspace) struct PublicMcpForwardMutation {
    pub(in crate::workspace) rule: ForwardRule,
    pub(in crate::workspace) persisted: bool,
}

#[derive(Default)]
struct ForwardingBindingState {
    consumers: HashMap<String, (String, ConnectionConsumer)>,
}

impl ForwardingBindingState {
    fn connection_id(&self, session_id: &str) -> Option<String> {
        self.consumers
            .get(session_id)
            .map(|(connection_id, _)| connection_id.clone())
    }

    fn node_for_connection_id(&self, connection_id: &str) -> Option<NodeId> {
        self.consumers
            .iter()
            .find_map(|(session_id, (candidate_connection_id, _))| {
                (candidate_connection_id == connection_id)
                    .then(|| node_id_from_forwarding_session(session_id))
                    .flatten()
            })
    }

    fn replace(
        &mut self,
        session_id: String,
        connection_id: String,
        consumer: ConnectionConsumer,
    ) -> Option<(String, ConnectionConsumer)> {
        self.consumers.insert(session_id, (connection_id, consumer))
    }

    fn remove(&mut self, session_id: &str) -> Option<(String, ConnectionConsumer)> {
        self.consumers.remove(session_id)
    }

    fn remove_exact(
        &mut self,
        session_id: &str,
        connection_id: &str,
        consumer: &ConnectionConsumer,
    ) {
        if self
            .consumers
            .get(session_id)
            .is_some_and(|(stored_connection_id, stored_consumer)| {
                stored_connection_id == connection_id && stored_consumer == consumer
            })
        {
            self.consumers.remove(session_id);
        }
    }

    fn drain(&mut self) -> Vec<(String, ConnectionConsumer)> {
        // Final session shutdown releases every logical PortForward consumer once.
        self.consumers.drain().map(|(_, binding)| binding).collect()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct WorkspaceSessionServiceShutdownReport {
    pub(in crate::workspace) sftp: oxideterm_sftp::SftpTransferShutdownReport,
    pub(in crate::workspace) forwarding: oxideterm_forwarding::ForwardingShutdownReport,
    pub(in crate::workspace) released_forwarding_bindings: usize,
}

/// Owns forwarding managers and SSH consumer bindings independently of UI mounts.
#[derive(Clone)]
pub(in crate::workspace) struct ForwardingRuntimeService {
    registry: ForwardingRegistry,
    ssh_registry: SshConnectionRegistry,
    node_router: NodeRouter,
    task_runtime: Arc<tokio::runtime::Runtime>,
    bindings: Arc<Mutex<ForwardingBindingState>>,
}

impl ForwardingRuntimeService {
    pub(in crate::workspace) fn new(
        registry: ForwardingRegistry,
        ssh_registry: SshConnectionRegistry,
        node_router: NodeRouter,
        task_runtime: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        Self {
            registry,
            ssh_registry,
            node_router,
            task_runtime,
            bindings: Arc::new(Mutex::new(ForwardingBindingState::default())),
        }
    }

    #[cfg(test)]
    pub(super) fn test_fixture() -> Self {
        let ssh_registry = SshConnectionRegistry::new(ConnectionPoolConfig::default());
        let node_router = NodeRouter::new(ssh_registry.clone());
        let task_runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("forwarding test runtime"),
        );
        Self::new(
            ForwardingRegistry::new(),
            ssh_registry,
            node_router,
            task_runtime,
        )
    }

    pub(in crate::workspace) fn registry(&self) -> &ForwardingRegistry {
        &self.registry
    }

    async fn shutdown_session_runtime(
        &self,
        grace_period: Duration,
    ) -> (oxideterm_forwarding::ForwardingShutdownReport, usize) {
        let manager_bindings = self
            .registry
            .session_ids()
            .into_iter()
            .filter_map(|session_id| {
                self.registry.get(&session_id).map(|manager| {
                    (
                        manager.ssh_connection_handle().connection_id().to_string(),
                        ConnectionConsumer::PortForward(session_id),
                    )
                })
            })
            .collect::<Vec<_>>();
        let report = self.registry.shutdown(grace_period).await;
        if !report.started {
            return (report, 0);
        }

        let mut bindings = self
            .binding_state()
            .drain()
            .into_iter()
            .collect::<HashSet<_>>();
        // A manager may exist before its worker result records the same binding.
        bindings.extend(manager_bindings);
        let released_bindings = bindings.len();
        for (connection_id, consumer) in bindings {
            // Forward listeners stop before their SSH ownership is released.
            self.ssh_registry.release(&connection_id, &consumer);
        }
        (report, released_bindings)
    }

    pub(in crate::workspace) fn session_id_for_node(node_id: &NodeId) -> String {
        format!("{FORWARDS_NODE_SESSION_PREFIX}{}", node_id.0)
    }

    pub(in crate::workspace) fn node_id_for_session(session_id: &str) -> Option<NodeId> {
        node_id_from_forwarding_session(session_id)
    }

    pub(in crate::workspace) fn connection_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        self.binding_state()
            .connection_id(&Self::session_id_for_node(node_id))
    }

    pub(in crate::workspace) fn node_for_connection_id(
        &self,
        connection_id: &str,
    ) -> Option<NodeId> {
        self.binding_state().node_for_connection_id(connection_id)
    }

    pub(in crate::workspace) fn manager_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<Arc<ForwardingManager>> {
        self.registry.get(&Self::session_id_for_node(node_id))
    }

    pub(in crate::workspace) fn public_mcp_rules_for_node(
        &self,
        node_id: &NodeId,
    ) -> Vec<ForwardRule> {
        self.manager_for_node(node_id)
            .map_or_else(Vec::new, |manager| manager.list_forwards())
    }

    pub(in crate::workspace) fn public_mcp_forward_is_persisted(&self, forward_id: &str) -> bool {
        self.registry
            .list_all_saved_forwards()
            .iter()
            .any(|forward| forward.id == forward_id)
    }

    pub(in crate::workspace) fn public_mcp_forward_owner_connection_id(
        &self,
        forward_id: &str,
    ) -> Option<String> {
        self.registry
            .list_all_saved_forwards()
            .into_iter()
            .find(|forward| forward.id == forward_id)
            .and_then(|forward| forward.owner_connection_id)
    }

    pub(in crate::workspace) async fn public_mcp_open_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        rule: ForwardRule,
        check_health: bool,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        let manager = self
            .public_mcp_manager_for_node(node_id, owner_connection_id)
            .await?;
        let rule = manager
            .create_forward_with_health_check(rule, check_health)
            .await
            .map_err(|error| error.to_string())?;
        let persisted = persist
            && self
                .registry
                .sync_persisted_forward_rule(
                    &rule.id,
                    &Self::session_id_for_node(node_id),
                    owner_connection_id.map(ToOwned::to_owned),
                    rule.clone(),
                )
                .is_ok_and(|saved| saved.is_some());
        Ok(PublicMcpForwardMutation { rule, persisted })
    }

    pub(in crate::workspace) async fn public_mcp_change_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        expected_rule: &ForwardRule,
        update: ForwardUpdate,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        let manager = self
            .public_mcp_manager_for_node(node_id, owner_connection_id)
            .await?;
        let original = manager
            .list_forwards()
            .into_iter()
            .find(|rule| rule.id == forward_id)
            .ok_or_else(|| "The forward no longer exists".to_owned())?;
        if &original != expected_rule {
            return Err("The forward changed after the expected revision".to_owned());
        }
        let restart = match original.status {
            ForwardStatus::Active => true,
            ForwardStatus::Stopped => false,
            _ => return Err("The forward is not ready to be changed".to_owned()),
        };
        if restart {
            manager
                .stop_forward(forward_id)
                .await
                .map_err(|error| error.to_string())?;
        }
        let updated = match manager.update_forward(forward_id, update) {
            Ok(updated) => updated,
            Err(error) => {
                if restart {
                    let _ = manager.restart_forward(forward_id).await;
                }
                return Err(error.to_string());
            }
        };
        let rule = if restart {
            match manager.restart_forward(forward_id).await {
                Ok(rule) => rule,
                Err(error) => {
                    let _ = manager.update_forward(forward_id, forward_update_from_rule(&original));
                    let _ = manager.restart_forward(forward_id).await;
                    return Err(error.to_string());
                }
            }
        } else {
            updated
        };
        let persisted = persist
            && self
                .registry
                .sync_persisted_forward_rule(
                    &rule.id,
                    &Self::session_id_for_node(node_id),
                    owner_connection_id.map(ToOwned::to_owned),
                    rule.clone(),
                )
                .is_ok_and(|saved| saved.is_some());
        Ok(PublicMcpForwardMutation { rule, persisted })
    }

    pub(in crate::workspace) async fn public_mcp_stop_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
    ) -> Result<ForwardRule, String> {
        self.public_mcp_manager_for_node(node_id, owner_connection_id)
            .await?
            .stop_forward(forward_id)
            .await
            .map_err(|error| error.to_string())
    }

    pub(in crate::workspace) async fn public_mcp_restart_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        let manager = self
            .public_mcp_manager_for_node(node_id, owner_connection_id)
            .await?;
        let rule = manager
            .restart_forward(forward_id)
            .await
            .map_err(|error| error.to_string())?;
        let persisted = persist
            && self
                .registry
                .sync_persisted_forward_rule(
                    &rule.id,
                    &Self::session_id_for_node(node_id),
                    owner_connection_id.map(ToOwned::to_owned),
                    rule.clone(),
                )
                .is_ok_and(|saved| saved.is_some());
        Ok(PublicMcpForwardMutation { rule, persisted })
    }

    pub(in crate::workspace) async fn public_mcp_remove_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        remove_saved: bool,
    ) -> Result<bool, String> {
        self.public_mcp_manager_for_node(node_id, owner_connection_id)
            .await?
            .delete_forward(forward_id)
            .await
            .map_err(|error| error.to_string())?;
        if !remove_saved {
            return Ok(false);
        }
        Ok(self.registry.delete_persisted_forward(forward_id).is_ok())
    }

    pub(in crate::workspace) fn public_mcp_forward_stats(
        &self,
        node_id: &NodeId,
        forward_id: &str,
    ) -> Result<ForwardStats, String> {
        self.manager_for_node(node_id)
            .ok_or_else(|| "The forward manager is unavailable".to_owned())?
            .get_stats(forward_id)
            .map_err(|error| error.to_string())
    }

    pub(in crate::workspace) async fn public_mcp_discover_ports(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
    ) -> Result<PortDetectionSnapshot, String> {
        self.public_mcp_manager_for_node(node_id, owner_connection_id)
            .await?
            .scan_remote_ports()
            .await
            .map_err(|error| error.to_string())
    }

    pub(in crate::workspace) async fn public_mcp_revoke_forward(
        &self,
        node_id: &NodeId,
        forward_id: &str,
        persisted: bool,
    ) {
        if persisted {
            let _ = self.registry.update_auto_start(forward_id, false);
        }
        let Some(manager) = self.manager_for_node(node_id) else {
            return;
        };
        if persisted {
            let _ = manager.stop_forward(forward_id).await;
        } else {
            let _ = manager.delete_forward(forward_id).await;
        }
    }

    async fn public_mcp_manager_for_node(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
    ) -> Result<Arc<ForwardingManager>, String> {
        let (manager, binding) = self
            .manager_for_node_async(node_id, owner_connection_id)
            .await?;
        // ForwardingRuntimeService remains the PortForward consumer owner.
        self.remember_binding(binding, false);
        Ok(manager)
    }

    pub(super) fn snapshot_for_node(&self, node_id: &NodeId) -> ForwardingRuntimeSnapshot {
        let Some(manager) = self.manager_for_node(node_id) else {
            return ForwardingRuntimeSnapshot::default();
        };
        let rules = manager.list_forwards();
        let stats_by_forward_id = rules
            .iter()
            .filter(|rule| matches!(rule.status, ForwardStatus::Active))
            .filter_map(|rule| {
                manager
                    .get_stats(&rule.id)
                    .ok()
                    .map(|stats| (rule.id.clone(), stats))
            })
            .collect();
        ForwardingRuntimeSnapshot {
            rules,
            stats_by_forward_id,
        }
    }

    pub(super) fn ignore_detected_port_for_node(&self, node_id: &NodeId, port: u16) {
        if let Some(connection_id) = self.connection_id_for_node(node_id) {
            self.registry.ignore_detected_port(&connection_id, port);
        }
        if let Some(manager) = self.manager_for_node(node_id) {
            manager.ignore_detected_port(port);
        }
    }

    pub(super) fn submit_operation(
        &self,
        tab_id: TabId,
        node_id: NodeId,
        owner_connection_id: Option<String>,
        message_key: &'static str,
        sync_saved_forwards_on_success: bool,
        operation: ForwardingRuntimeOperation,
        worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
    ) {
        let service = self.clone();
        self.task_runtime.spawn(async move {
            let (binding, result) = service
                .execute_operation(node_id, owner_connection_id, operation)
                .await;
            let _ = worker_tx.send(ForwardingWorkerResult::Operation {
                tab_id,
                message_key,
                sync_saved_forwards_on_success,
                binding,
                result,
            });
        });
    }

    pub(super) fn submit_port_scan(
        &self,
        node_id: NodeId,
        owner_connection_id: Option<String>,
        restart_degraded_profiler: bool,
        worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
    ) {
        let service = self.clone();
        self.task_runtime.spawn(async move {
            let (connection_id, binding, result) = service
                .scan_ports(
                    node_id.clone(),
                    owner_connection_id,
                    restart_degraded_profiler,
                )
                .await;
            let _ = worker_tx.send(ForwardingWorkerResult::PortScan {
                node_id,
                connection_id,
                binding,
                result,
            });
        });
    }

    pub(super) fn submit_session_restore(
        &self,
        node_id: NodeId,
        worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
    ) {
        let service = self.clone();
        self.task_runtime.spawn(async move {
            let session_id = Self::session_id_for_node(&node_id);
            let consumer = ConnectionConsumer::PortForward(session_id.clone());
            let binding = match service
                .node_router
                .acquire_connection_wait(&node_id, consumer.clone(), Duration::from_secs(15))
                .await
            {
                Ok(resolved) => {
                    let connection_id = resolved.connection_id.clone();
                    let _ = service
                        .registry
                        .restore_session(&session_id, resolved.handle)
                        .await;
                    Some((session_id, connection_id, consumer))
                }
                Err(_) => None,
            };
            let _ = worker_tx.send(ForwardingWorkerResult::Binding { binding });
        });
    }

    pub(super) fn submit_reconnect_restore(
        &self,
        request: ReconnectForwardRestoreRequest,
        worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
    ) {
        let service = self.clone();
        self.task_runtime.spawn(async move {
            let ReconnectForwardRestoreRequest {
                root_node_id,
                snapshots,
                old_connection_ids_by_node,
                owner_connection_ids,
                cancellation,
                job_id,
            } = request;
            let mut restored = 0_u32;
            let mut failures = 0_u32;
            let mut failure_details = Vec::<String>::new();
            let mut created_forwards = Vec::<(String, String)>::new();
            let mut bindings = Vec::<(String, String, ConnectionConsumer)>::new();
            for entry in snapshots {
                if !cancellation.load(Ordering::Acquire) {
                    cleanup_reconnect_created_forwards(&service.registry, &created_forwards).await;
                    release_reconnect_forward_bindings(&service.node_router, &bindings);
                    return;
                }
                let entry_node_id = NodeId::new(entry.node_id.clone());
                let session_id = Self::session_id_for_node(&entry_node_id);
                let consumer = ConnectionConsumer::PortForward(session_id.clone());
                let resolved = match service
                    .node_router
                    .acquire_connection_wait(
                        &entry_node_id,
                        consumer.clone(),
                        Duration::from_secs(15),
                    )
                    .await
                {
                    Ok(resolved) => resolved,
                    Err(error) => {
                        failures += entry.rules.len() as u32;
                        for rule in &entry.rules {
                            failure_details.push(format!(
                                "{}: {}",
                                forward_restore_failure_label(rule),
                                error
                            ));
                        }
                        continue;
                    }
                };
                let binding = (
                    session_id.clone(),
                    resolved.connection_id.clone(),
                    consumer.clone(),
                );
                if !cancellation.load(Ordering::Acquire) {
                    service
                        .node_router
                        .release_consumer(&resolved.connection_id, &consumer);
                    cleanup_reconnect_created_forwards(&service.registry, &created_forwards).await;
                    release_reconnect_forward_bindings(&service.node_router, &bindings);
                    return;
                }
                let manager = service
                    .registry
                    .register_for_reconnect_restore(
                        session_id.clone(),
                        resolved.handle,
                        old_connection_ids_by_node
                            .get(&entry.node_id)
                            .map(String::as_str),
                    )
                    .await;
                bindings.push(binding);
                let mut live_keys = manager
                    .list_forwards()
                    .into_iter()
                    .map(|rule| forward_restore_key_for_rule(&rule))
                    .collect::<HashSet<_>>();
                for snapshot_rule in entry.rules {
                    let key = forward_restore_key_for_snapshot_rule(&snapshot_rule);
                    for live_rule in manager.list_forwards() {
                        live_keys.insert(forward_restore_key_for_rule(&live_rule));
                    }
                    if live_keys.contains(&key) {
                        continue;
                    }
                    if !cancellation.load(Ordering::Acquire) {
                        cleanup_reconnect_created_forwards(&service.registry, &created_forwards)
                            .await;
                        release_reconnect_forward_bindings(&service.node_router, &bindings);
                        return;
                    }
                    let failure_label = forward_restore_failure_label(&snapshot_rule);
                    let Some(rule) = forward_rule_from_reconnect_snapshot(&snapshot_rule) else {
                        failures += 1;
                        failure_details.push(format!(
                            "{failure_label}: unsupported forward type '{}'",
                            snapshot_rule.forward_type
                        ));
                        continue;
                    };
                    match manager.create_forward_with_health_check(rule, true).await {
                        Ok(created) => {
                            live_keys.insert(forward_restore_key_for_rule(&created));
                            restored += 1;
                            created_forwards.push((session_id.clone(), created.id.clone()));
                            if let Some(owner_connection_id) =
                                owner_connection_ids.get(&entry.node_id).cloned().flatten()
                            {
                                let created_id = created.id.clone();
                                let _ = service.registry.sync_persisted_forward_rule(
                                    &created_id,
                                    &session_id,
                                    Some(owner_connection_id),
                                    created,
                                );
                            }
                        }
                        Err(error) => {
                            failures += 1;
                            failure_details.push(format!("{failure_label}: {error}"));
                        }
                    }
                }
            }
            let detail = forward_restore_result_detail(restored, failures, &failure_details);
            let _ = worker_tx.send(ForwardingWorkerResult::ReconnectRestore {
                node_id: root_node_id,
                result: forward_restore_phase_result(failures),
                restored,
                detail,
                job_id,
                created_forwards,
                bindings,
            });
        });
    }

    pub(in crate::workspace) fn release_binding_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<String> {
        let session_id = Self::session_id_for_node(node_id);
        self.release_binding_for_session_inner(&session_id, Some(node_id))
    }

    pub(in crate::workspace) fn release_binding_for_session(
        &self,
        session_id: &str,
    ) -> Option<String> {
        self.release_binding_for_session_inner(session_id, None)
    }

    pub(in crate::workspace) fn discard_binding(
        &self,
        session_id: &str,
        connection_id: &str,
        consumer: &ConnectionConsumer,
    ) {
        self.registry.stop_port_profiler(connection_id);
        self.ssh_registry.release(connection_id, consumer);
        self.binding_state()
            .remove_exact(session_id, connection_id, consumer);
    }

    pub(in crate::workspace) fn remember_binding(
        &self,
        binding: Option<(String, String, ConnectionConsumer)>,
        node_is_disconnected: bool,
    ) {
        let Some((session_id, connection_id, consumer)) = binding else {
            return;
        };
        if !self.registry.accepts_new_work()
            || node_is_disconnected
            || !self.binding_is_current(&session_id, &connection_id)
        {
            // A late worker result cannot revive a consumer after explicit
            // session/node teardown or after NodeRouter moved to another connection.
            self.discard_binding(&session_id, &connection_id, &consumer);
            return;
        }

        let previous =
            self.binding_state()
                .replace(session_id, connection_id.clone(), consumer.clone());
        if let Some((previous_connection_id, previous_consumer)) = previous
            && (previous_connection_id != connection_id || previous_consumer != consumer)
        {
            // Reconnect swaps the logical consumer to the fresh node-owned
            // transport and releases the old connection reference.
            self.registry.stop_port_profiler(&previous_connection_id);
            self.ssh_registry
                .release(&previous_connection_id, &previous_consumer);
        }
    }

    fn release_binding_for_session_inner(
        &self,
        session_id: &str,
        node_id: Option<&NodeId>,
    ) -> Option<String> {
        let consumer = ConnectionConsumer::PortForward(session_id.to_string());
        let connection_id = if let Some((connection_id, stored_consumer)) =
            self.binding_state().remove(session_id)
        {
            self.ssh_registry.release(&connection_id, &stored_consumer);
            Some(connection_id)
        } else if let Some(manager) = self.registry.get(session_id) {
            // The manager may be registered before its worker delivery is
            // applied, so explicit disconnect also releases this fallback.
            let connection_id = manager.ssh_connection_handle().connection_id().to_string();
            self.ssh_registry.release(&connection_id, &consumer);
            Some(connection_id)
        } else if let Some(connection_id) =
            node_id.and_then(|node_id| self.node_router.connection_id_for_node(node_id))
        {
            self.ssh_registry.release(&connection_id, &consumer);
            Some(connection_id)
        } else {
            None
        };

        if let Some(connection_id) = connection_id.as_ref() {
            self.registry.stop_port_profiler(connection_id);
        }
        connection_id
    }

    fn binding_is_current(&self, session_id: &str, connection_id: &str) -> bool {
        if !self
            .registry
            .get(session_id)
            .is_some_and(|manager| manager.ssh_connection_handle().connection_id() == connection_id)
        {
            return false;
        }
        let Some(node_id) = node_id_from_forwarding_session(session_id) else {
            return true;
        };
        self.node_router
            .connection_id_for_node(&node_id)
            .is_some_and(|current_connection_id| current_connection_id == connection_id)
    }

    async fn execute_operation(
        &self,
        node_id: NodeId,
        owner_connection_id: Option<String>,
        operation: ForwardingRuntimeOperation,
    ) -> (
        Option<(String, String, ConnectionConsumer)>,
        Result<(), String>,
    ) {
        let session_id = Self::session_id_for_node(&node_id);
        let (manager, binding) = match self
            .manager_for_node_async(&node_id, owner_connection_id.as_deref())
            .await
        {
            Ok(value) => value,
            Err(error) => return (None, Err(error)),
        };
        let result = match operation {
            ForwardingRuntimeOperation::Create { rule, check_health } => manager
                .create_forward_with_health_check(rule, check_health)
                .await
                .map(|created| {
                    self.persist_rule(&session_id, owner_connection_id.clone(), created);
                }),
            ForwardingRuntimeOperation::Update { forward_id, update } => {
                manager.update_forward(&forward_id, update).map(|updated| {
                    self.persist_rule(&session_id, owner_connection_id.clone(), updated);
                })
            }
            ForwardingRuntimeOperation::Delete { forward_id } => {
                manager.delete_forward(&forward_id).await.map(|_| {
                    let _ = self.registry.delete_persisted_forward(&forward_id);
                })
            }
            ForwardingRuntimeOperation::Stop { forward_id } => {
                manager.stop_forward(&forward_id).await.map(|_| ())
            }
            ForwardingRuntimeOperation::Restart { forward_id } => {
                manager.restart_forward(&forward_id).await.map(|restarted| {
                    self.persist_rule(&session_id, owner_connection_id.clone(), restarted);
                })
            }
            ForwardingRuntimeOperation::Quick { action, port } => {
                let created = match action {
                    ForwardingQuickAction::Jupyter => manager.forward_jupyter(port, port).await,
                    ForwardingQuickAction::Tensorboard => {
                        manager.forward_tensorboard(port, port).await
                    }
                    ForwardingQuickAction::Vscode => manager.forward_vscode(port, port).await,
                };
                created.map(|created| {
                    self.persist_rule(&session_id, owner_connection_id.clone(), created);
                })
            }
        }
        .map_err(|error| error.to_string());
        (binding, result)
    }

    async fn scan_ports(
        &self,
        node_id: NodeId,
        owner_connection_id: Option<String>,
        restart_degraded_profiler: bool,
    ) -> (
        Option<String>,
        Option<(String, String, ConnectionConsumer)>,
        Result<PortDetectionSnapshot, String>,
    ) {
        let (manager, binding) = match self
            .manager_for_node_async(&node_id, owner_connection_id.as_deref())
            .await
        {
            Ok(value) => value,
            Err(error) => return (None, None, Err(error)),
        };
        let connection_id = binding
            .as_ref()
            .map(|(_, connection_id, _)| connection_id.clone());
        let result = if let Some(connection_id) = connection_id.as_ref() {
            // Profiler tasks are created inside this service's Tokio runtime.
            if restart_degraded_profiler {
                let _ = self.registry.restart_degraded_port_profiler(
                    connection_id.clone(),
                    manager.ssh_connection_handle(),
                );
            } else {
                let _ = self
                    .registry
                    .start_port_profiler(connection_id.clone(), manager.ssh_connection_handle());
            }
            Ok(self
                .registry
                .detected_ports(connection_id)
                .unwrap_or_default())
        } else {
            Err("node has no forwarding connection binding".to_string())
        };
        (connection_id, binding, result)
    }

    async fn manager_for_node_async(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
    ) -> Result<
        (
            Arc<ForwardingManager>,
            Option<(String, String, ConnectionConsumer)>,
        ),
        String,
    > {
        if !self.registry.accepts_new_work() {
            return Err(FORWARDING_SESSION_SHUTTING_DOWN.to_string());
        }
        let session_id = Self::session_id_for_node(node_id);
        let manager_existed = self.registry.get(&session_id).is_some();
        let consumer = ConnectionConsumer::PortForward(session_id.clone());
        let resolved = self
            .node_router
            .acquire_connection_wait(node_id, consumer.clone(), Duration::from_secs(15))
            .await
            .map_err(|error| error.to_string())?;
        let connection_id = resolved.connection_id.clone();
        if !self.registry.accepts_new_work() {
            self.node_router.release_consumer(&connection_id, &consumer);
            return Err(FORWARDING_SESSION_SHUTTING_DOWN.to_string());
        }
        let (manager, _restored) = self
            .registry
            .register_or_rebind(session_id.clone(), resolved.handle)
            .await;
        if !self.registry.accepts_new_work() {
            let _ = self.registry.remove(&session_id).await;
            self.node_router.release_consumer(&connection_id, &consumer);
            return Err(FORWARDING_SESSION_SHUTTING_DOWN.to_string());
        }

        // Managers are node-owned. Reacquiring through NodeRouter before every
        // action prevents terminal-pane lifetime from selecting the transport.
        if let Some(owner_connection_id) = owner_connection_id {
            let _ = self.registry.saved_store().map(|store| {
                store.bind_owned_forwards_to_session(owner_connection_id, &session_id)
            });
        }
        if manager_existed {
            return Ok((manager, Some((session_id, connection_id, consumer))));
        }

        let saved_forwards = if let Some(owner_connection_id) = owner_connection_id {
            self.registry.load_owned_forwards(owner_connection_id)
        } else {
            self.registry.load_persisted_forwards(&session_id)
        };
        for mut rule in saved_forwards
            .into_iter()
            .filter(|forward| forward.auto_start)
            .map(|forward| forward.rule)
        {
            rule.status = ForwardStatus::Starting;
            let _ = manager.create_forward(rule).await;
        }

        Ok((manager, Some((session_id, connection_id, consumer))))
    }

    fn persist_rule(
        &self,
        session_id: &str,
        owner_connection_id: Option<String>,
        rule: ForwardRule,
    ) {
        let forward_id = rule.id.clone();
        let _ = self.registry.sync_persisted_forward_rule(
            &forward_id,
            session_id,
            owner_connection_id,
            rule,
        );
    }

    fn binding_state(&self) -> MutexGuard<'_, ForwardingBindingState> {
        // Preserve cleanup access after an unrelated panic; consumer release
        // must remain available during explicit node teardown.
        self.bindings
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

fn forward_update_from_rule(rule: &ForwardRule) -> ForwardUpdate {
    ForwardUpdate {
        forward_type: Some(rule.forward_type),
        bind_address: Some(rule.bind_address.clone()),
        bind_port: Some(rule.bind_port),
        target_host: Some(rule.target_host.clone()),
        target_port: Some(rule.target_port),
        description: Some(rule.description.clone()),
    }
}

/// Shuts down all background services owned by one shared WorkspaceSession.
pub(in crate::workspace) async fn shutdown_workspace_session_services(
    sftp_transfer_manager: &oxideterm_sftp::SftpTransferManager,
    forwarding_runtime_service: &ForwardingRuntimeService,
    grace_period: Duration,
) -> WorkspaceSessionServiceShutdownReport {
    // Both owners share one wall-clock bound instead of extending window teardown serially.
    let (sftp, (forwarding, released_forwarding_bindings)) = tokio::join!(
        sftp_transfer_manager.shutdown_session_transfers(grace_period),
        forwarding_runtime_service.shutdown_session_runtime(grace_period),
    );
    WorkspaceSessionServiceShutdownReport {
        sftp,
        forwarding,
        released_forwarding_bindings,
    }
}

/// Completes final session shutdown before its Tokio runtime field can be dropped.
pub(in crate::workspace) fn shutdown_workspace_session_services_blocking(
    task_runtime: &tokio::runtime::Runtime,
    sftp_transfer_manager: &oxideterm_sftp::SftpTransferManager,
    forwarding_runtime_service: &ForwardingRuntimeService,
    grace_period: Duration,
) -> WorkspaceSessionServiceShutdownReport {
    // WorkspaceSession release runs on GPUI's owner thread, outside this Tokio runtime.
    task_runtime.block_on(shutdown_workspace_session_services(
        sftp_transfer_manager,
        forwarding_runtime_service,
        grace_period,
    ))
}

impl WorkspaceApp {
    pub(in crate::workspace) fn shutdown_final_session_services(&self) {
        // This runs only from WorkspaceApp Entity release, after all window leases are gone.
        let _ = shutdown_workspace_session_services_blocking(
            self.forwarding_runtime.as_ref(),
            self.sftp_transfer_manager.as_ref(),
            &self.forwarding_service,
            WORKSPACE_SESSION_SERVICE_SHUTDOWN_GRACE_PERIOD,
        );
    }
}

fn node_id_from_forwarding_session(session_id: &str) -> Option<NodeId> {
    session_id
        .strip_prefix(FORWARDS_NODE_SESSION_PREFIX)
        .map(|raw_node_id| NodeId(raw_node_id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binding_state_replaces_and_removes_one_logical_consumer() {
        let mut state = ForwardingBindingState::default();
        let session_id = ForwardingRuntimeService::session_id_for_node(&NodeId::new("node-a"));
        let first_consumer = ConnectionConsumer::PortForward(session_id.clone());
        let second_consumer = first_consumer.clone();

        assert!(
            state
                .replace(
                    session_id.clone(),
                    "connection-a".to_string(),
                    first_consumer,
                )
                .is_none()
        );
        assert_eq!(
            state.replace(
                session_id.clone(),
                "connection-b".to_string(),
                second_consumer.clone(),
            ),
            Some((
                "connection-a".to_string(),
                ConnectionConsumer::PortForward(session_id.clone()),
            ))
        );
        assert_eq!(
            state.node_for_connection_id("connection-b"),
            Some(NodeId::new("node-a"))
        );
        assert_eq!(
            state.remove(&session_id),
            Some(("connection-b".to_string(), second_consumer))
        );
    }

    #[test]
    fn stale_exact_removal_does_not_delete_reconnected_binding() {
        let mut state = ForwardingBindingState::default();
        let session_id = ForwardingRuntimeService::session_id_for_node(&NodeId::new("node-b"));
        let consumer = ConnectionConsumer::PortForward(session_id.clone());
        state.replace(
            session_id.clone(),
            "connection-new".to_string(),
            consumer.clone(),
        );

        state.remove_exact(&session_id, "connection-old", &consumer);

        assert_eq!(
            state.connection_id(&session_id).as_deref(),
            Some("connection-new")
        );
    }

    #[test]
    fn workspace_session_service_shutdown_is_exactly_once() {
        let service = ForwardingRuntimeService::test_fixture();
        let task_runtime = service.task_runtime.clone();
        let transfers = oxideterm_sftp::SftpTransferManager::new();

        let first = shutdown_workspace_session_services_blocking(
            task_runtime.as_ref(),
            &transfers,
            &service,
            Duration::from_millis(300),
        );
        let second = shutdown_workspace_session_services_blocking(
            task_runtime.as_ref(),
            &transfers,
            &service,
            Duration::from_millis(300),
        );

        assert!(first.sftp.started);
        assert!(first.forwarding.started);
        assert!(!second.sftp.started);
        assert!(!second.forwarding.started);
        assert_eq!(second.released_forwarding_bindings, 0);
    }
}
