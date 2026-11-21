// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::workspace::delivery;
use oxideterm_public_mcp_host::PublicMcpForwardStopHandoff;
use oxideterm_remote_desktop::RemoteDesktopEndpoint;
#[cfg(test)]
use oxideterm_ssh::ConnectionPoolConfig;
use oxideterm_ssh::{
    ConnectionConsumer, ConnectionConsumerLease, ReconnectForwardRuleSnapshot,
    SshConnectionRegistry,
};
use std::{
    collections::{BTreeSet, HashSet},
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

const FORWARDING_SESSION_SHUTTING_DOWN: &str = "workspace forwarding session is shutting down";
// The reserved marker keeps session-owned desktop tunnels out of every user
// forward surface while still allowing reconnect restoration to identify them.
const REMOTE_DESKTOP_TUNNEL_DESCRIPTION_PREFIX: &str =
    "__oxideterm_internal_remote_desktop_tunnel_v1__:";
const WORKSPACE_SESSION_SERVICE_SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(5);

pub(in crate::workspace) struct ReconnectForwardRestoreRequest {
    pub(in crate::workspace) root_node_id: NodeId,
    pub(in crate::workspace) snapshots: Vec<ReconnectForwardRuleSnapshot>,
    pub(in crate::workspace) snapshot_reader: Arc<ForwardRestoreReader>,
    pub(in crate::workspace) old_connection_ids_by_node: HashMap<String, String>,
    pub(in crate::workspace) owner_connection_ids: HashMap<String, Option<String>>,
    pub(in crate::workspace) cancellation: Arc<AtomicBool>,
    pub(in crate::workspace) job_id: String,
}

pub(in crate::workspace) struct PublicMcpForwardMutation {
    pub(in crate::workspace) rule: ForwardRule,
    pub(in crate::workspace) persisted: bool,
}

#[derive(Default)]
struct RemoteDesktopTunnelState {
    leases: HashMap<String, RemoteDesktopTunnelBinding>,
}

struct RemoteDesktopTunnelBinding {
    node_id: NodeId,
    forward_id: String,
    audit: Option<oxideterm_audit::AuditContext>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum ForwardRestoreBlock {
    Stopped,
    Deleted,
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct ForwardIntent {
    block: ForwardRestoreBlock,
    revision: u64,
    deleted_at: Option<u64>,
}

#[derive(Clone, Copy)]
pub(in crate::workspace) struct ForwardStopRequest {
    intent: ForwardIntent,
    first_revision: u64,
}

impl ForwardStopRequest {
    async fn apply(
        &self,
        service: &ForwardingRuntimeService,
        manager: &ForwardingManager,
        id: &str,
    ) -> Result<bool, String> {
        if service.forward_intent(id) != Some(self.intent) {
            return Ok(false);
        }
        let result = self.intent.block.apply(manager, id).await;
        if result.is_err() {
            service.finish_desktop_stop(id, *self, false);
        }
        result.map(|_| true)
    }
}

#[derive(Default)]
struct ForwardRestoreState {
    revision: u64,
    readers: BTreeSet<u64>,
    blocked: HashMap<String, ForwardIntent>,
    // Queued desktop requests suppress restoration immediately, but only an applied stop is durable intent.
    requested: HashMap<String, ForwardStopRequest>,
}

impl ForwardRestoreState {
    fn current(&self, id: &str) -> Option<ForwardIntent> {
        self.requested
            .get(id)
            .map(|stop| stop.intent)
            .or_else(|| self.blocked.get(id).copied())
    }

    fn next_revision(&mut self) -> u64 {
        self.revision += 1;
        self.revision
    }
}

/// A snapshot can outlive the disconnect that captured it, including retry and cancellation.
pub(in crate::workspace) struct ForwardRestoreReader {
    state: Arc<Mutex<ForwardRestoreState>>,
    revision: u64,
}

impl Drop for ForwardRestoreReader {
    fn drop(&mut self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        let oldest = state.readers.first().copied();
        state.readers.remove(&self.revision);
        if oldest == Some(self.revision) {
            let next = state.readers.first().copied();
            state.blocked.retain(|_, intent| {
                intent
                    .deleted_at
                    .is_none_or(|deleted| next.is_some_and(|reader| reader <= deleted))
            });
        }
    }
}

impl ForwardRestoreBlock {
    async fn apply(self, manager: &ForwardingManager, id: &str) -> Result<(), String> {
        manager
            .close_forward_if_present(id, self == Self::Deleted)
            .await
            .map_err(|e| e.to_string())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) struct WorkspaceSessionServiceShutdownReport {
    pub(in crate::workspace) sftp: oxideterm_sftp::SftpTransferShutdownReport,
    pub(in crate::workspace) forwarding: oxideterm_forwarding::ForwardingShutdownReport,
}

/// Owns node forwarding managers independently of UI mounts.
#[derive(Clone)]
pub(in crate::workspace) struct ForwardingRuntimeService {
    registry: ForwardingRegistry,
    node_router: NodeRouter,
    task_runtime: Arc<tokio::runtime::Runtime>,
    single_channel_forwarding_error: Arc<String>,
    remote_desktop_tunnels: Arc<Mutex<RemoteDesktopTunnelState>>,
    suppressed_restores: Arc<Mutex<ForwardRestoreState>>,
}

impl ForwardingRuntimeService {
    pub(in crate::workspace) fn new(
        registry: ForwardingRegistry,
        ssh_registry: SshConnectionRegistry,
        node_router: NodeRouter,
        task_runtime: Arc<tokio::runtime::Runtime>,
        single_channel_forwarding_error: String,
    ) -> Self {
        Self {
            registry: registry.with_ssh_registry(ssh_registry),
            node_router,
            task_runtime,
            single_channel_forwarding_error: Arc::new(single_channel_forwarding_error),
            remote_desktop_tunnels: Arc::new(Mutex::new(RemoteDesktopTunnelState::default())),
            suppressed_restores: Arc::default(),
        }
    }

    #[cfg(test)]
    pub(in crate::workspace) fn test_fixture() -> Self {
        Self::test_fixture_with_registries(
            ForwardingRegistry::new(),
            SshConnectionRegistry::new(ConnectionPoolConfig::default()),
        )
    }

    #[cfg(test)]
    fn test_fixture_with_registries(
        registry: ForwardingRegistry,
        ssh_registry: SshConnectionRegistry,
    ) -> Self {
        let node_router = NodeRouter::new(ssh_registry.clone());
        let task_runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("forwarding test runtime"),
        );
        Self::new(
            registry,
            ssh_registry,
            node_router,
            task_runtime,
            "single-channel forwarding unavailable".to_string(),
        )
    }

    pub(in crate::workspace) fn registry(&self) -> &ForwardingRegistry {
        &self.registry
    }

    pub(in crate::workspace) fn track_forward_restore(&self) -> Arc<ForwardRestoreReader> {
        let mut state = self
            .suppressed_restores
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let revision = state.next_revision();
        state.readers.insert(revision);
        Arc::new(ForwardRestoreReader {
            state: self.suppressed_restores.clone(),
            revision,
        })
    }

    pub(in crate::workspace) fn request_forward_start(&self, id: &str) {
        let mut state = self
            .suppressed_restores
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        state.requested.remove(id);
        state.blocked.remove(id);
    }

    pub(in crate::workspace) fn request_forward_stop(
        &self,
        id: &str,
        delete: bool,
    ) -> ForwardStopRequest {
        let mut state = self
            .suppressed_restores
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !delete
            && let Some(intent) = state
                .current(id)
                .filter(|intent| intent.block == ForwardRestoreBlock::Deleted)
        {
            return state
                .requested
                .get(id)
                .copied()
                .unwrap_or(ForwardStopRequest {
                    intent,
                    first_revision: intent.revision,
                });
        }
        let revision = state.next_revision();
        let request = ForwardStopRequest {
            intent: ForwardIntent {
                block: if delete {
                    ForwardRestoreBlock::Deleted
                } else {
                    ForwardRestoreBlock::Stopped
                },
                revision,
                deleted_at: None,
            },
            first_revision: state
                .requested
                .get(id)
                .map_or(revision, |pending| pending.first_revision),
        };
        state.requested.insert(id.to_owned(), request);
        request
    }

    fn finish_desktop_stop(&self, id: &str, request: ForwardStopRequest, applied: bool) -> bool {
        let mut state = self
            .suppressed_restores
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let Some(pending) = state.requested.get(id).copied() else {
            return false;
        };
        if pending.first_revision != request.first_revision {
            return false;
        }
        if applied
            && state
                .blocked
                .get(id)
                .is_none_or(|old| old.revision <= request.intent.revision)
        {
            state.blocked.insert(id.to_owned(), request.intent);
        }
        let current = pending.intent == request.intent;
        if current {
            state.requested.remove(id);
        }
        current
    }

    fn block_forward(&self, id: &str, delete: bool) -> (Option<ForwardIntent>, ForwardIntent) {
        let mut state = self
            .suppressed_restores
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let previous = state.current(id);
        if !delete
            && let Some(deleted) =
                previous.filter(|intent| intent.block == ForwardRestoreBlock::Deleted)
        {
            return (previous, deleted);
        }
        let block = if delete {
            ForwardRestoreBlock::Deleted
        } else {
            ForwardRestoreBlock::Stopped
        };
        let intent = ForwardIntent {
            block,
            revision: state.next_revision(),
            deleted_at: None,
        };
        state.requested.remove(id);
        let confirmed = state.blocked.insert(id.to_owned(), intent);
        (confirmed, intent)
    }

    fn forward_intent(&self, id: &str) -> Option<ForwardIntent> {
        self.suppressed_restores
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .current(id)
    }

    fn restore_block(&self, id: &str) -> Option<ForwardRestoreBlock> {
        self.forward_intent(id).map(|intent| intent.block)
    }

    fn replace_intent_if_current(
        &self,
        id: &str,
        expected: Option<ForwardIntent>,
        next: Option<ForwardIntent>,
    ) -> bool {
        let mut state = self
            .suppressed_restores
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if state.current(id) != expected {
            return false;
        }
        if expected == next {
            return true;
        }
        state.requested.remove(id);
        match next {
            Some(intent) => {
                state.blocked.insert(id.to_owned(), intent);
            }
            None => {
                state.blocked.remove(id);
            }
        }
        true
    }

    fn delete_saved_rule_for_intent(&self, id: &str, intent: ForwardIntent) -> Result<(), String> {
        self.registry
            .delete_persisted_forward(id)
            .map_err(|e| e.to_string())?;
        self.finish_forward_delete(id, Some(intent));
        Ok(())
    }

    fn finish_forward_delete(&self, id: &str, expected: Option<ForwardIntent>) {
        let mut state = self
            .suppressed_restores
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if expected.is_none_or(|intent| intent.block != ForwardRestoreBlock::Deleted)
            || state.blocked.get(id).copied() != expected
        {
            return;
        }
        if state.readers.is_empty() {
            state.blocked.remove(id);
        } else {
            // Readers that captured the rule before the durable removal can still recreate it.
            let deleted_at = state.next_revision();
            if let Some(intent) = state.blocked.get_mut(id) {
                intent.deleted_at = Some(deleted_at);
            }
        }
    }

    async fn discard_blocked_restore(
        &self,
        manager: &ForwardingManager,
        id: &str,
    ) -> Result<bool, String> {
        let Some(block) = self.restore_block(id) else {
            return Ok(false);
        };
        // A user can delete a rule while its listener is still starting or stopping.
        block.apply(manager, id).await?;
        if block == ForwardRestoreBlock::Stopped
            && self.restore_block(id) == Some(ForwardRestoreBlock::Deleted)
        {
            ForwardRestoreBlock::Deleted.apply(manager, id).await?;
        }
        Ok(true)
    }

    fn persist_started_rule(
        &self,
        session: &str,
        owner: Option<String>,
        rule: ForwardRule,
    ) -> Result<bool, String> {
        // Serialize the final durable write with a user's stop/delete intent.
        // A reconnect snapshot captured earlier cannot recreate the saved entry.
        let stopped = self
            .suppressed_restores
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if stopped.current(&rule.id).is_some() {
            return Ok(false);
        }
        self.persist_rule(session, owner, rule)?;
        Ok(true)
    }

    pub(in crate::workspace) async fn stop_saved_tunnel(
        &self,
        id: &str,
        stop: ForwardStopRequest,
    ) -> Result<(), String> {
        let _reader = self.track_forward_restore();
        let intent = stop.intent;
        // Stopping a saved entry must not connect its host or disturb sibling consumers.
        for session in self.registry.session_ids() {
            if let Some(manager) = self.registry.get(&session)
                && !stop.apply(self, &manager, id).await?
            {
                return Ok(());
            }
        }
        if !self.finish_desktop_stop(id, stop, true) {
            return Ok(());
        }
        if intent.block == ForwardRestoreBlock::Deleted {
            self.delete_saved_rule_for_intent(id, intent)
        } else if self
            .registry
            .list_all_saved_forwards()
            .iter()
            .any(|rule| rule.id == id)
        {
            self.registry
                .update_auto_start(id, false)
                .map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }

    pub(in crate::workspace) async fn start_saved_tunnel(
        &self,
        node: NodeId,
        owner: String,
        id: String,
    ) -> Result<(), String> {
        let _reader = self.track_forward_restore();
        if self.restore_block(&id).is_some() {
            return Ok(());
        }
        self.execute_operation(
            node,
            Some(owner),
            ForwardingRuntimeOperation::Restart { forward_id: id },
            None,
        )
        .await
    }

    pub(in crate::workspace) fn session_id_for_node(node_id: &NodeId) -> String {
        format!("{FORWARDS_NODE_SESSION_PREFIX}{}", node_id.0)
    }

    pub(in crate::workspace) fn node_id_for_session(session_id: &str) -> Option<NodeId> {
        node_id_from_forwarding_session(session_id)
    }

    pub(in crate::workspace) fn connection_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        self.node_router.connection_id_for_node(node_id)
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
        let hidden_forward_ids = self.remote_desktop_tunnel_forward_ids();
        self.manager_for_node(node_id)
            .map_or_else(Vec::new, |manager| {
                manager
                    .list_forwards()
                    .into_iter()
                    .filter(|rule| {
                        !hidden_forward_ids.contains(&rule.id)
                            && Self::remote_desktop_tunnel_lease_id(rule).is_none()
                    })
                    .collect()
            })
    }

    pub(in crate::workspace) async fn open_remote_desktop_tunnel(
        &self,
        lease_id: String,
        node_id: &NodeId,
        owner_connection_id: &str,
        target_host: String,
        target_port: u16,
    ) -> Result<RemoteDesktopEndpoint, String> {
        let (manager, _operation) = self
            .manager_for_node_async(node_id, Some(owner_connection_id))
            .await?;
        let mut rule = ForwardRule::local("127.0.0.1", 0, target_host, target_port);
        rule.description = format!("{REMOTE_DESKTOP_TUNNEL_DESCRIPTION_PREFIX}{lease_id}");
        let rule = manager
            .create_forward_with_health_check(rule, true)
            .await
            .map_err(|error| error.to_string())?;
        self.remote_desktop_tunnel_state().leases.insert(
            lease_id,
            RemoteDesktopTunnelBinding {
                node_id: node_id.clone(),
                forward_id: rule.id.clone(),
                audit: oxideterm_audit::AuditContext::current_request(),
            },
        );
        Ok(RemoteDesktopEndpoint::new(
            rule.bind_address,
            rule.bind_port,
        ))
    }

    pub(in crate::workspace) fn close_remote_desktop_tunnel(&self, lease_id: String) {
        let Some(binding) = self.remote_desktop_tunnel_state().leases.remove(&lease_id) else {
            return;
        };
        let service = self.clone();
        self.task_runtime
            .spawn(oxideterm_audit::AuditContext::scope_optional(
                binding.audit,
                async move {
                    if let Some(manager) = service.manager_for_node(&binding.node_id) {
                        // The desktop lease owns this listener, not the shared SSH node.
                        let _ = manager.delete_forward(&binding.forward_id).await;
                    }
                },
            ));
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

    async fn finish_mcp_forward_mutation(
        &self,
        node_id: &NodeId,
        owner: Option<&str>,
        manager: &ForwardingManager,
        rule: ForwardRule,
        persist: bool,
        original: Option<&ForwardRule>,
    ) -> Result<PublicMcpForwardMutation, String> {
        let result = {
            let state = self
                .suppressed_restores
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let block = state.current(&rule.id).map(|intent| intent.block);
            if block == Some(ForwardRestoreBlock::Deleted)
                || (block.is_some() && rule.status == ForwardStatus::Active)
            {
                None
            } else {
                Some(
                    persist
                        && self
                            .registry
                            .sync_persisted_forward_rule(
                                &rule.id,
                                &Self::session_id_for_node(node_id),
                                owner.map(ToOwned::to_owned),
                                rule.clone(),
                            )
                            .is_ok_and(|saved| saved.is_some()),
                )
            }
        };
        if let Some(persisted) = result {
            return Ok(PublicMcpForwardMutation { rule, persisted });
        }
        self.discard_blocked_restore(manager, &rule.id).await?;
        if let Some(original) = original {
            let state = self
                .suppressed_restores
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            if state
                .current(&rule.id)
                .is_some_and(|intent| intent.block == ForwardRestoreBlock::Stopped)
            {
                let mut stopped = rule;
                stopped.status = ForwardStatus::Stopped;
                manager.revert_stopped_update(&stopped, original);
            }
        }
        Err("The forward changed during the operation".into())
    }

    pub(in crate::workspace) async fn public_mcp_open_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        rule: ForwardRule,
        check_health: bool,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        let _reader = self.track_forward_restore();
        let (manager, _operation) = self
            .manager_for_node_async(node_id, owner_connection_id)
            .await?;
        let rule = manager
            .create_forward_with_health_check(rule, check_health)
            .await
            .map_err(|error| error.to_string())?;
        self.finish_mcp_forward_mutation(
            node_id,
            owner_connection_id,
            &manager,
            rule,
            persist,
            None,
        )
        .await
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
        let _reader = self.track_forward_restore();
        let (manager, _operation) = self
            .manager_for_node_async(node_id, owner_connection_id)
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
            if self.discard_blocked_restore(&manager, forward_id).await? {
                return Err("The forward changed during the operation".into());
            }
        }
        let updated = manager
            .update_forward(forward_id, update)
            .map_err(|error| error.to_string())?;
        let rule = if restart {
            match manager.restart_forward(forward_id).await {
                Ok(rule) => rule,
                Err(error) => {
                    // Only the failed edit's stopped definition can be rolled back.
                    if manager.revert_stopped_update(&updated, &original)
                        && self.restore_block(forward_id).is_none()
                    {
                        let rollback = manager.restart_forward(forward_id).await;
                        if let Err(rollback) = rollback {
                            self.discard_blocked_restore(&manager, forward_id).await?;
                            return Err(format!("{error}; {rollback}"));
                        }
                    }
                    self.discard_blocked_restore(&manager, forward_id).await?;
                    return Err(error.to_string());
                }
            }
        } else {
            updated
        };
        self.finish_mcp_forward_mutation(
            node_id,
            owner_connection_id,
            &manager,
            rule,
            persist,
            Some(&original),
        )
        .await
    }

    pub(in crate::workspace) async fn public_mcp_stop_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
    ) -> Result<(ForwardRule, PublicMcpForwardStopHandoff), String> {
        let _reader = self.track_forward_restore();
        let (manager, connection) = self
            .manager_for_node_async(node_id, owner_connection_id)
            .await?;
        let (previous, intent) = self.block_forward(forward_id, false);
        match manager.stop_forward(forward_id).await {
            Ok(stopped) => {
                if self.restore_block(forward_id) == Some(ForwardRestoreBlock::Deleted) {
                    ForwardRestoreBlock::Deleted
                        .apply(&manager, forward_id)
                        .await?;
                }
                Ok((
                    stopped,
                    PublicMcpForwardStopHandoff {
                        revision: intent.revision,
                        connection,
                    },
                ))
            }
            Err(error) => {
                self.replace_intent_if_current(forward_id, Some(intent), previous);
                Err(error.to_string())
            }
        }
    }

    pub(in crate::workspace) async fn public_mcp_restart_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        persist: bool,
    ) -> Result<PublicMcpForwardMutation, String> {
        let _reader = self.track_forward_restore();
        let (manager, _operation) = self
            .manager_for_node_async(node_id, owner_connection_id)
            .await?;
        // A grant to restart a stopped rule does not override a concurrent deletion.
        let prior = self.forward_intent(forward_id);
        if prior.is_some_and(|intent| intent.block == ForwardRestoreBlock::Deleted) {
            return Err("The forward is being removed".into());
        }
        let rule = manager
            .restart_forward(forward_id)
            .await
            .map_err(|error| error.to_string())?;
        if !self.replace_intent_if_current(forward_id, prior, None)
            && self.discard_blocked_restore(&manager, forward_id).await?
        {
            return Err("The forward was stopped during restart".into());
        }
        self.finish_mcp_forward_mutation(
            node_id,
            owner_connection_id,
            &manager,
            rule,
            persist,
            None,
        )
        .await
    }

    pub(in crate::workspace) async fn public_mcp_remove_forward(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        forward_id: &str,
        remove_saved: bool,
    ) -> Result<bool, String> {
        let _reader = self.track_forward_restore();
        let (manager, _operation) = self
            .manager_for_node_async(node_id, owner_connection_id)
            .await?;
        let (previous, intent) = self.block_forward(forward_id, true);
        if let Err(error) = manager.delete_forward(forward_id).await {
            self.replace_intent_if_current(forward_id, Some(intent), previous);
            return Err(error.to_string());
        }
        let saved_removed =
            remove_saved && self.registry.delete_persisted_forward(forward_id).is_ok();
        self.finish_forward_delete(forward_id, Some(intent));
        Ok(saved_removed)
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
        let (manager, _operation) = self
            .manager_for_node_async(node_id, owner_connection_id)
            .await?;
        manager
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
        let _reader = self.track_forward_restore();
        let (_, intent) = self.block_forward(forward_id, !persisted);
        if persisted {
            let _ = self.registry.update_auto_start(forward_id, false);
        }
        if let Some(manager) = self.manager_for_node(node_id) {
            let _ = intent.block.apply(&manager, forward_id).await;
        }
        if !persisted {
            self.finish_forward_delete(forward_id, Some(intent));
        }
    }

    pub(in crate::workspace) async fn public_mcp_restore_forward_after_revocation(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
        original_rule: &ForwardRule,
        revoked_rule: &ForwardRule,
        stop_handoff: Option<&PublicMcpForwardStopHandoff>,
        created_by_client: bool,
        persisted: bool,
    ) -> bool {
        let _reader = self.track_forward_restore();
        let restore_active = !created_by_client && original_rule.status == ForwardStatus::Active;
        // Rollback can stop the last listener before restarting it with its original target.
        let (manager, _operation) = if restore_active {
            let Ok((manager, lease)) = self
                .manager_for_node_async(node_id, owner_connection_id)
                .await
            else {
                return false;
            };
            (manager, Some(lease))
        } else {
            let Some(manager) = self.manager_for_node(node_id) else {
                return false;
            };
            (manager, None)
        };
        let Some(current_rule) = manager
            .list_forwards()
            .into_iter()
            .find(|rule| rule.id == original_rule.id)
        else {
            return false;
        };
        if created_by_client {
            // Client-owned revocation may already have stopped the late rule.
            let mut current_definition = current_rule.clone();
            current_definition.status = revoked_rule.status.clone();
            if current_definition != *revoked_rule {
                return false;
            }
        } else if current_rule != *revoked_rule {
            // Do not overwrite a newer UI or client edit that followed the revoked operation.
            return false;
        }
        if created_by_client && !persisted {
            let (previous, intent) = self.block_forward(&current_rule.id, true);
            if manager.delete_forward(&current_rule.id).await.is_err() {
                self.replace_intent_if_current(&current_rule.id, Some(intent), previous);
                return false;
            }
            self.finish_forward_delete(&current_rule.id, Some(intent));
            return true;
        }

        // Reconcile the original owner only after matching the revoked mutation.
        let prior = self.forward_intent(&current_rule.id);
        let stop_revision = stop_handoff.map(|stop| stop.revision);
        if prior.is_some_and(|intent| intent.block == ForwardRestoreBlock::Deleted)
            || (restore_active && prior.map(|intent| intent.revision) != stop_revision)
        {
            return false;
        }
        if !restore_active {
            self.block_forward(&current_rule.id, false);
        }
        if current_rule.status == ForwardStatus::Active
            && manager.stop_forward(&current_rule.id).await.is_err()
        {
            return false;
        }
        let Ok(restored_stopped) =
            manager.update_forward(&original_rule.id, forward_update_from_rule(original_rule))
        else {
            return false;
        };
        let restored_rule = if restore_active {
            let Ok(restored_active) = manager.restart_forward(&original_rule.id).await else {
                return false;
            };
            if !self.replace_intent_if_current(&original_rule.id, prior, None)
                && self
                    .discard_blocked_restore(&manager, &original_rule.id)
                    .await
                    .unwrap_or(true)
            {
                return false;
            }
            restored_active
        } else {
            restored_stopped
        };

        self.finish_mcp_forward_mutation(
            node_id,
            owner_connection_id,
            &manager,
            restored_rule,
            persisted,
            None,
        )
        .await
        .is_ok_and(|result| !persisted || result.persisted)
    }

    pub(super) fn snapshot_for_node(&self, node_id: &NodeId) -> ForwardingRuntimeSnapshot {
        let manager = self.manager_for_node(node_id);
        let hidden_forward_ids = self.remote_desktop_tunnel_forward_ids();
        let mut names = HashMap::new();
        let mut rules: Vec<_> = self
            .saved_records_for_node(node_id)
            .into_iter()
            .map(|saved| {
                if let Some(profile) = saved.tunnel {
                    names.insert(saved.rule.id.clone(), profile.name);
                }
                saved.rule
            })
            .collect();
        // Runtime state wins, but a stopped saved rule does not need a live SSH manager.
        for rule in manager.iter().flat_map(|manager| manager.list_forwards()) {
            rules.retain(|saved| saved.id != rule.id);
            rules.push(rule);
        }
        let mut rules = rules
            .into_iter()
            .filter(|rule| {
                !hidden_forward_ids.contains(&rule.id)
                    && Self::remote_desktop_tunnel_lease_id(rule).is_none()
            })
            .collect::<Vec<_>>();
        rules.sort_by(|left, right| left.id.cmp(&right.id));
        let stats_by_forward_id = rules
            .iter()
            .filter(|rule| matches!(rule.status, ForwardStatus::Active))
            .filter_map(|rule| {
                manager
                    .as_ref()?
                    .get_stats(&rule.id)
                    .ok()
                    .map(|stats| (rule.id.clone(), stats))
            })
            .collect();
        ForwardingRuntimeSnapshot {
            names,
            rules,
            stats_by_forward_id,
        }
    }

    fn saved_rules_for_node(&self, node_id: &NodeId) -> Vec<ForwardRule> {
        self.saved_records_for_node(node_id)
            .into_iter()
            .map(|saved| saved.rule)
            .collect()
    }

    fn saved_owner_for_node(&self, node_id: &NodeId) -> Option<String> {
        self.node_router
            .node_metadata(node_id)?
            .origin
            .saved_target_id()
            .map(str::to_owned)
    }

    fn saved_records_for_node(
        &self,
        node_id: &NodeId,
    ) -> Vec<oxideterm_forwarding::PersistedForward> {
        // Runtime node IDs can be reused after restart. Only a saved connection owns disk rules.
        self.saved_owner_for_node(node_id)
            .map(|owner| self.registry.load_owned_forwards(&owner))
            .unwrap_or_default()
    }

    fn remote_desktop_tunnel_state(&self) -> MutexGuard<'_, RemoteDesktopTunnelState> {
        self.remote_desktop_tunnels
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn remote_desktop_tunnel_forward_ids(&self) -> HashSet<String> {
        self.remote_desktop_tunnel_state()
            .leases
            .values()
            .map(|binding| binding.forward_id.clone())
            .collect()
    }

    fn remote_desktop_tunnel_lease_id(rule: &ForwardRule) -> Option<&str> {
        rule.description
            .strip_prefix(REMOTE_DESKTOP_TUNNEL_DESCRIPTION_PREFIX)
    }

    fn remote_desktop_tunnel_lease_is_active(&self, lease_id: &str) -> bool {
        self.remote_desktop_tunnel_state()
            .leases
            .contains_key(lease_id)
    }

    fn restore_remote_desktop_tunnel_binding(
        &self,
        lease_id: &str,
        node_id: NodeId,
        forward_id: String,
    ) -> bool {
        let mut state = self.remote_desktop_tunnel_state();
        let Some(binding) = state.leases.get_mut(lease_id) else {
            return false;
        };
        binding.node_id = node_id;
        binding.forward_id = forward_id;
        true
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
        let stop = match &operation {
            ForwardingRuntimeOperation::Create { rule, .. } => {
                self.request_forward_start(&rule.id);
                None
            }
            ForwardingRuntimeOperation::Restart { forward_id } => {
                self.request_forward_start(forward_id);
                None
            }
            ForwardingRuntimeOperation::Stop { forward_id } => {
                Some(self.request_forward_stop(forward_id, false))
            }
            ForwardingRuntimeOperation::Delete { forward_id } => {
                Some(self.request_forward_stop(forward_id, true))
            }
            _ => None,
        };
        let starting = matches!(
            &operation,
            ForwardingRuntimeOperation::Create { .. }
                | ForwardingRuntimeOperation::Restart { .. }
                | ForwardingRuntimeOperation::Quick { .. }
        );
        let handoff = starting
            .then(|| self.connection_handoff(&node_id))
            .flatten();
        let reader = self.track_forward_restore();
        let service = self.clone();
        self.task_runtime.spawn(async move {
            let (_handoff, _reader) = (handoff, reader);
            let result = service
                .execute_operation(node_id, owner_connection_id, operation, stop)
                .await;
            let _ = worker_tx.send(ForwardingWorkerResult::Operation {
                tab_id,
                message_key,
                sync_saved_forwards_on_success,
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
            let (connection_id, result) = service
                .scan_ports(
                    node_id.clone(),
                    owner_connection_id,
                    restart_degraded_profiler,
                )
                .await;
            let _ = worker_tx.send(ForwardingWorkerResult::PortScan {
                node_id,
                connection_id,
                result,
            });
        });
    }

    pub(super) fn submit_session_restore(&self, node_id: NodeId) {
        let reader = self.track_forward_restore();
        if !self.node_has_restorable_rules(&node_id) {
            return;
        }
        let handoff = self.connection_handoff(&node_id);
        let service = self.clone();
        self.task_runtime.spawn(async move {
            let (_handoff, _reader) = (handoff, reader);
            service.restore_session_for_node(&node_id).await;
        });
    }

    pub(in crate::workspace) async fn suspend_session(
        &self,
        session: &str,
        connection: Option<&str>,
    ) {
        let Some(connection) = connection else {
            return;
        };
        self.registry
            .stop_session_port_profiler(session, connection);
        let Some(manager) = self.registry.get(session) else {
            return;
        };
        let _reader = self.track_forward_restore();
        for rule in manager.suspend_connection_forwards(Some(connection)).await {
            // Suspension removes a listener while it awaits shutdown, then publishes the row again.
            let _ = self.discard_blocked_restore(&manager, &rule.id).await;
        }
    }

    fn node_has_restorable_rules(&self, node_id: &NodeId) -> bool {
        if let Some(manager) = self.manager_for_node(node_id) {
            let needs_rebind = (manager.active_forward_count() > 0
                || manager.suspension_in_progress())
                && self.node_router.connection_id_for_node(node_id).as_deref()
                    != Some(manager.ssh_connection_handle().connection_id());
            needs_rebind
                || manager
                    .list_stopped_forwards()
                    .iter()
                    .any(|rule| rule.status == ForwardStatus::Suspended)
        } else {
            self.saved_records_for_node(node_id)
                .iter()
                .any(|saved| saved.auto_start && saved.tunnel.is_none())
        }
    }

    async fn restore_session_for_node(&self, node_id: &NodeId) {
        let owner = self.saved_owner_for_node(node_id);
        if let Ok((manager, _operation)) =
            self.manager_for_node_async(node_id, owner.as_deref()).await
        {
            let _ = manager
                .restore_saved_forwards(manager.ssh_connection_handle())
                .await;
            for rule in manager.list_forwards() {
                let _ = self.discard_blocked_restore(&manager, &rule.id).await;
            }
        }
    }

    pub(super) fn submit_reconnect_restore(
        &self,
        request: ReconnectForwardRestoreRequest,
        worker_tx: delivery::ActiveDeliverySender<ForwardingWorkerResult>,
    ) {
        let handoffs: Vec<_> = request
            .snapshots
            .iter()
            .filter(|entry| !entry.rules.is_empty())
            .filter_map(|entry| self.connection_handoff(&NodeId::new(entry.node_id.clone())))
            .collect();
        let service = self.clone();
        self.task_runtime.spawn(async move {
            let _handoffs = handoffs;
            let ReconnectForwardRestoreRequest {
                root_node_id,
                snapshots,
                snapshot_reader: _snapshot_reader,
                old_connection_ids_by_node,
                owner_connection_ids,
                cancellation,
                job_id,
            } = request;
            let mut restored = 0_u32;
            let mut failures = 0_u32;
            let mut failure_details = Vec::<String>::new();
            let mut created_forwards = Vec::<(Arc<ForwardingManager>, String)>::new();
            for entry in snapshots {
                if !cancellation.load(Ordering::Acquire) {
                    cleanup_reconnect_created_forwards(&created_forwards).await;
                    return;
                }
                let entry_node_id = NodeId::new(entry.node_id.clone());
                let session_id = Self::session_id_for_node(&entry_node_id);
                if entry.rules.is_empty() {
                    continue;
                }
                let lease = match service.lease_operation(&entry_node_id).await {
                    Ok(lease) => lease,
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
                if !cancellation.load(Ordering::Acquire) {
                    cleanup_reconnect_created_forwards(&created_forwards).await;
                    return;
                }
                let manager = service
                    .registry
                    .register_for_reconnect_restore(
                        session_id.clone(),
                        lease.handle().clone(),
                        old_connection_ids_by_node
                            .get(&entry.node_id)
                            .map(String::as_str),
                    )
                    .await;
                let mut live_keys = manager
                    .list_forwards()
                    .into_iter()
                    .map(|rule| forward_restore_key_for_rule(&rule))
                    .collect::<HashSet<_>>();
                for snapshot_rule in entry.rules {
                    if service.restore_block(&snapshot_rule.id).is_some() {
                        continue;
                    }
                    let remote_desktop_lease_id = snapshot_rule
                        .description
                        .strip_prefix(REMOTE_DESKTOP_TUNNEL_DESCRIPTION_PREFIX)
                        .map(ToOwned::to_owned);
                    if remote_desktop_lease_id.as_deref().is_some_and(|lease_id| {
                        !service.remote_desktop_tunnel_lease_is_active(lease_id)
                    }) {
                        // A closed desktop must not be resurrected by a node
                        // reconnect that captured its listener moments earlier.
                        continue;
                    }
                    let key = forward_restore_key_for_snapshot_rule(&snapshot_rule);
                    for live_rule in manager.list_forwards() {
                        live_keys.insert(forward_restore_key_for_rule(&live_rule));
                    }
                    if live_keys.contains(&key) {
                        continue;
                    }
                    if !cancellation.load(Ordering::Acquire) {
                        cleanup_reconnect_created_forwards(&created_forwards).await;
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
                            match service.discard_blocked_restore(&manager, &created.id).await {
                                Ok(true) => continue,
                                Ok(false) => {}
                                Err(error) => {
                                    failures += 1;
                                    failure_details.push(format!("{failure_label}: {error}"));
                                    continue;
                                }
                            }
                            live_keys.insert(forward_restore_key_for_rule(&created));
                            if let Some(lease_id) = remote_desktop_lease_id {
                                if !service.restore_remote_desktop_tunnel_binding(
                                    &lease_id,
                                    entry_node_id.clone(),
                                    created.id.clone(),
                                ) {
                                    let _ = manager.delete_forward(&created.id).await;
                                    continue;
                                }
                            } else if let Some(owner_connection_id) =
                                owner_connection_ids.get(&entry.node_id).cloned().flatten()
                            {
                                let created_id = created.id.clone();
                                match service.persist_started_rule(
                                    &session_id,
                                    Some(owner_connection_id),
                                    created.clone(),
                                ) {
                                    Ok(true) => {}
                                    Ok(false) => {
                                        if let Err(error) = service
                                            .discard_blocked_restore(&manager, &created_id)
                                            .await
                                        {
                                            failures += 1;
                                            failure_details
                                                .push(format!("{failure_label}: {error}"));
                                        }
                                        continue;
                                    }
                                    Err(error) => {
                                        failures += 1;
                                        failure_details.push(format!("{failure_label}: {error}"));
                                    }
                                }
                            }
                            restored += 1;
                            created_forwards.push((manager.clone(), created.id.clone()));
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
            });
        });
    }

    async fn execute_operation(
        &self,
        node_id: NodeId,
        owner_connection_id: Option<String>,
        operation: ForwardingRuntimeOperation,
        stop: Option<ForwardStopRequest>,
    ) -> Result<(), String> {
        let session_id = Self::session_id_for_node(&node_id);
        // Configuration changes and stopping a listener must never acquire SSH.
        let manager = self.manager_for_node(&node_id);
        let result = match &operation {
            ForwardingRuntimeOperation::Save { rule } => {
                Some(self.persist_rule(&session_id, owner_connection_id.clone(), rule.clone()))
            }
            ForwardingRuntimeOperation::Update { forward_id, update } => {
                let updated =
                    if let Some(manager) = manager.as_ref().filter(|m| m.has_rule(forward_id)) {
                        manager
                            .update_forward(forward_id, update.clone())
                            .map_err(|e| e.to_string())
                    } else {
                        self.saved_rules_for_node(&node_id)
                            .into_iter()
                            .find(|rule| &rule.id == forward_id)
                            .ok_or_else(|| {
                                oxideterm_forwarding::ForwardingError::NotFound(forward_id.clone())
                                    .to_string()
                            })
                            .map(|mut rule| {
                                rule.apply_update(update.clone());
                                rule
                            })
                    };
                Some(updated.and_then(|rule| {
                    self.persist_rule(&session_id, owner_connection_id.clone(), rule)
                }))
            }
            ForwardingRuntimeOperation::Delete { forward_id } => {
                let stop = stop.unwrap_or_else(|| self.request_forward_stop(forward_id, true));
                if let Some(manager) = &manager
                    && !stop.apply(self, manager, forward_id).await?
                {
                    return Ok(());
                }
                if !self.finish_desktop_stop(forward_id, stop, true) {
                    return Ok(());
                }
                Some(self.delete_saved_rule_for_intent(forward_id, stop.intent))
            }
            ForwardingRuntimeOperation::Stop { forward_id } => {
                let stop = stop.unwrap_or_else(|| self.request_forward_stop(forward_id, false));
                if stop.intent.block == ForwardRestoreBlock::Deleted {
                    return Ok(());
                }
                if let Some(manager) = &manager
                    && !stop.apply(self, manager, forward_id).await?
                {
                    return Ok(());
                }
                if !self.finish_desktop_stop(forward_id, stop, true) {
                    return Ok(());
                }
                Some(
                    if self
                        .saved_rules_for_node(&node_id)
                        .iter()
                        .any(|rule| &rule.id == forward_id)
                    {
                        self.registry
                            .update_auto_start(forward_id, false)
                            .map_err(|e| e.to_string())
                    } else {
                        Ok(())
                    },
                )
            }
            _ => None,
        };
        if let Some(result) = result {
            return result;
        }
        let (manager, _operation) = self
            .manager_for_node_async(&node_id, owner_connection_id.as_deref())
            .await?;
        let result = match operation {
            ForwardingRuntimeOperation::Create { rule, check_health } => manager
                .create_forward_with_health_check(rule, check_health)
                .await
                .map_err(|e| e.to_string()),
            ForwardingRuntimeOperation::Restart { forward_id } => {
                if let Some(rule) = manager
                    .list_forwards()
                    .into_iter()
                    .find(|rule| rule.id == forward_id && rule.status == ForwardStatus::Active)
                {
                    Ok(rule)
                } else if manager.has_rule(&forward_id) {
                    manager
                        .restart_forward(&forward_id)
                        .await
                        .map_err(|e| e.to_string())
                } else if let Some(rule) = self
                    .saved_rules_for_node(&node_id)
                    .into_iter()
                    .find(|rule| rule.id == forward_id)
                {
                    manager
                        .create_forward(rule)
                        .await
                        .map_err(|e| e.to_string())
                } else {
                    Err(oxideterm_forwarding::ForwardingError::NotFound(forward_id).to_string())
                }
            }
            ForwardingRuntimeOperation::Quick { action, port } => {
                let created = match action {
                    ForwardingQuickAction::Jupyter => manager.forward_jupyter(port, port).await,
                    ForwardingQuickAction::Tensorboard => {
                        manager.forward_tensorboard(port, port).await
                    }
                    ForwardingQuickAction::Vscode => manager.forward_vscode(port, port).await,
                };
                created.map_err(|e| e.to_string())
            }
            _ => unreachable!("configuration operations were handled without SSH"),
        };
        match result {
            Ok(rule) => {
                let result =
                    self.persist_started_rule(&session_id, owner_connection_id, rule.clone());
                if matches!(result, Ok(false)) {
                    return self
                        .discard_blocked_restore(&manager, &rule.id)
                        .await
                        .map(|_| ());
                }
                if let Err(save_error) = &result {
                    // Do not leave an unrecorded listener running after a failed durable save.
                    if let Err(stop_error) = manager.stop_forward(&rule.id).await {
                        return Err(format!("{save_error}; {stop_error}"));
                    }
                }
                result.map(|_| ())
            }
            Err(error) => Err(error),
        }
    }

    async fn scan_ports(
        &self,
        node_id: NodeId,
        owner_connection_id: Option<String>,
        restart_degraded_profiler: bool,
    ) -> (Option<String>, Result<PortDetectionSnapshot, String>) {
        let (_manager, lease) = match self
            .manager_for_node_async(&node_id, owner_connection_id.as_deref())
            .await
        {
            Ok(value) => value,
            Err(error) => return (None, Err(error)),
        };
        let connection_id = lease.connection_id().to_owned();
        let handle = lease.handle().clone();
        if restart_degraded_profiler {
            self.registry
                .restart_degraded_port_profiler(connection_id.clone(), handle);
        } else {
            self.registry
                .start_port_profiler(connection_id.clone(), handle);
        }
        let result = self
            .registry
            .detected_ports(&connection_id)
            .unwrap_or_default();
        (Some(connection_id), Ok(result))
    }

    fn connection_handoff(&self, node: &NodeId) -> Option<ConnectionConsumerLease> {
        self.connection_id_for_node(node)
            .and_then(|id| self.node_router.lease_connection_handoff(&id))
    }

    async fn lease_operation(&self, node_id: &NodeId) -> Result<ConnectionConsumerLease, String> {
        self.node_router
            .lease_connection_wait(
                node_id,
                ConnectionConsumer::PortForward(format!("operation:{}", uuid::Uuid::new_v4())),
                Duration::from_secs(15),
            )
            .await
            .map_err(|error| error.to_string())
    }

    async fn manager_for_node_async(
        &self,
        node_id: &NodeId,
        owner_connection_id: Option<&str>,
    ) -> Result<(Arc<ForwardingManager>, ConnectionConsumerLease), String> {
        if !self.registry.accepts_new_work() {
            return Err(FORWARDING_SESSION_SHUTTING_DOWN.to_string());
        }
        let session_id = Self::session_id_for_node(node_id);
        if self
            .node_router
            .node_runtime_snapshot(node_id)
            .is_some_and(|snapshot| {
                snapshot
                    .config
                    .ssh_channel_strategy
                    .requires_dedicated_consumers()
            })
        {
            // Forward listeners can create multiple concurrent SSH channels;
            // a per-consumer transport cannot make that safe on one-channel appliances.
            return Err(self.single_channel_forwarding_error.as_ref().clone());
        }
        let _reader = self.track_forward_restore();
        let manager_existed = self.registry.get(&session_id).is_some();
        let lease = self.lease_operation(node_id).await?;
        if !self.registry.accepts_new_work() {
            return Err(FORWARDING_SESSION_SHUTTING_DOWN.to_string());
        }
        let (manager, restored) = self
            .registry
            .register_or_rebind(session_id.clone(), lease.handle().clone())
            .await;
        if !self.registry.accepts_new_work() {
            let _ = self.registry.remove(&session_id).await;
            return Err(FORWARDING_SESSION_SHUTTING_DOWN.to_string());
        }
        if !restored.is_empty() {
            // Failed restores can leave suspended rows, including ones deleted during startup.
            for rule in manager.list_forwards() {
                self.discard_blocked_restore(&manager, &rule.id).await?;
            }
        }

        // Managers are node-owned. Reacquiring through NodeRouter before every
        // action prevents terminal-pane lifetime from selecting the transport.
        if let Some(owner_connection_id) = owner_connection_id {
            let _ = self.registry.saved_store().map(|store| {
                store.bind_owned_forwards_to_session(owner_connection_id, &session_id)
            });
        }
        if manager_existed {
            return Ok((manager, lease));
        }

        for mut rule in self
            .saved_records_for_node(node_id)
            .into_iter()
            .filter(|forward| forward.auto_start && forward.tunnel.is_none())
            .map(|forward| forward.rule)
        {
            if self.restore_block(&rule.id).is_some() {
                continue;
            }
            rule.status = ForwardStatus::Starting;
            if let Ok(started) = manager.create_forward(rule).await {
                self.discard_blocked_restore(&manager, &started.id).await?;
            }
        }

        Ok((manager, lease))
    }

    fn persist_rule(
        &self,
        session_id: &str,
        owner_connection_id: Option<String>,
        rule: ForwardRule,
    ) -> Result<(), String> {
        let forward_id = rule.id.clone();
        let durable = owner_connection_id.is_some();
        let saved = self
            .registry
            .sync_persisted_forward_rule(&forward_id, session_id, owner_connection_id, rule)
            .map_err(|error| error.to_string())?;
        if durable && saved.is_none() {
            return Err("Saved forward storage is unavailable".to_string());
        }
        Ok(())
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
    let (sftp, forwarding) = tokio::join!(
        sftp_transfer_manager.shutdown_session_transfers(grace_period),
        forwarding_runtime_service.registry.shutdown(grace_period),
    );
    WorkspaceSessionServiceShutdownReport { sftp, forwarding }
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
    fn saved_rules_remain_visible_without_a_runtime_manager() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let store = oxideterm_forwarding::SavedForwardStore::load(&path).unwrap();
        let mut rule = ForwardRule::local("127.0.0.1", 8080, "localhost", 80);
        rule.id = "saved-rule".into();
        store
            .sync_persisted_forward_rule(
                &rule.id,
                "old-session",
                Some("saved-host".into()),
                rule.clone(),
            )
            .unwrap();
        let mut service = ForwardingRuntimeService::test_fixture();
        service.registry = ForwardingRegistry::new_with_store(
            oxideterm_forwarding::SavedForwardStore::load(&path).unwrap(),
        );
        let node = NodeId::new("new-node");
        service.node_router.upsert_node_with_origin(
            node.clone(),
            oxideterm_ssh::SshConfig::default(),
            oxideterm_ssh::NodeOrigin::Restored {
                saved_connection_id: "saved-host".into(),
            },
        );
        rule.status = ForwardStatus::Stopped;
        assert!(service.snapshot_for_node(&node).names.is_empty());
        service
            .registry
            .saved_store()
            .unwrap()
            .update_tunnel_profile(
                &rule.id,
                oxideterm_forwarding::SshTunnelProfile {
                    name: "PROD MySQL".into(),
                    group: None,
                    connect_on_startup: false,
                },
            )
            .unwrap();
        let snapshot = service.snapshot_for_node(&node);
        assert_eq!(
            snapshot.names,
            HashMap::from([(rule.id.clone(), "PROD MySQL".into())])
        );
        assert_eq!(snapshot.rules, vec![rule]);
        assert!(
            service
                .snapshot_for_node(&NodeId::new("other-node"))
                .rules
                .is_empty()
        );
    }

    #[test]
    fn reused_session_id_cannot_restore_another_hosts_listener() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let reused = NodeId::new("ssh-1");
        let rule = ForwardRule::dynamic("127.0.0.1", 0);
        {
            let store = oxideterm_forwarding::SavedForwardStore::load(&path).unwrap();
            store
                .sync_persisted_forward_rule(
                    &rule.id,
                    &ForwardingRuntimeService::session_id_for_node(&reused),
                    Some("saved-a".into()),
                    rule.clone(),
                )
                .unwrap();
            store.update_auto_start(&rule.id, true).unwrap();
        }
        let ssh = SshConnectionRegistry::default();
        let service = ForwardingRuntimeService::test_fixture_with_registries(
            ForwardingRegistry::new_with_store(
                oxideterm_forwarding::SavedForwardStore::load(&path).unwrap(),
            ),
            ssh.clone(),
        );
        let config = |host: &str| oxideterm_ssh::SshConfig {
            host: host.into(),
            ..Default::default()
        };
        service
            .node_router
            .upsert_node(reused.clone(), config("b.invalid"));
        let saved = service
            .node_router
            .expand_manual_preset("saved-a", vec![config("jump.invalid")], config("a.invalid"))
            .unwrap();
        let temporary = || {
            service
                .node_router
                .expand_temporary_chain(vec![config("jump.invalid")], config("a.invalid"))
                .unwrap()
        };
        let first = temporary();
        let second = temporary();
        assert_ne!(first.target_node_id, second.target_node_id);
        for (node, expected) in [
            (reused, vec![]),
            (saved.path_node_ids[0].clone(), vec![]),
            (first.target_node_id, vec![]),
            (second.target_node_id, vec![]),
            (saved.target_node_id, vec![rule.id.clone()]),
        ] {
            let config = service
                .node_router
                .node_runtime_snapshot(&node)
                .unwrap()
                .config;
            let owner = service.saved_owner_for_node(&node);
            let host = config.host.clone();
            let handle = ssh.acquire(
                config.clone(),
                ConnectionConsumer::NodeRouter(node.0.clone()),
            );
            handle.set_physical(Arc::new(()));
            ssh.mark_state(handle.connection_id(), ConnectionState::Active)
                .unwrap();
            assert_eq!(
                service
                    .snapshot_for_node(&node)
                    .rules
                    .into_iter()
                    .map(|r| r.id)
                    .collect::<Vec<_>>(),
                expected,
                "{host} offline rules"
            );
            service
                .node_router
                .bind_connection(&node, handle.connection_id())
                .unwrap();
            assert_eq!(
                service.node_has_restorable_rules(&node),
                !expected.is_empty()
            );
            service.task_runtime.block_on(async {
                let (manager, _lease) = service
                    .manager_for_node_async(&node, owner.as_deref())
                    .await
                    .unwrap();
                let running = manager
                    .list_forwards()
                    .into_iter()
                    .filter(|r| r.status == ForwardStatus::Active)
                    .map(|r| r.id)
                    .collect::<Vec<_>>();
                assert_eq!(running, expected, "{host} active listeners");
                manager.stop_all().await;
            });
        }
    }

    #[test]
    fn single_channel_nodes_reject_forwarding_before_connection_acquisition() {
        let service = ForwardingRuntimeService::test_fixture();
        let node_id = NodeId::new("single-channel-node");
        service.node_router.upsert_node(
            node_id.clone(),
            oxideterm_ssh::SshConfig {
                ssh_channel_strategy:
                    oxideterm_connections::SshChannelStrategy::DedicatedPerConsumer,
                ..oxideterm_ssh::SshConfig::default()
            },
        );

        let result = service
            .task_runtime
            .block_on(service.manager_for_node_async(&node_id, None));

        assert_eq!(
            result.err().as_deref(),
            Some("single-channel forwarding unavailable")
        );
    }

    #[test]
    fn offline_rule_edits_are_durable_and_report_save_failures() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let mut service = ForwardingRuntimeService::test_fixture();
        service.registry = ForwardingRegistry::new_with_store(
            oxideterm_forwarding::SavedForwardStore::load(&path).unwrap(),
        );
        let node = NodeId::new("offline-node");
        service.node_router.upsert_node_with_origin(
            node.clone(),
            oxideterm_ssh::SshConfig::default(),
            oxideterm_ssh::NodeOrigin::Restored {
                saved_connection_id: "saved-host".into(),
            },
        );
        let mut rule = ForwardRule::local("127.0.0.1", 8080, "localhost", 80);
        rule.id = "offline-rule".into();
        let run = |operation| {
            let result = service.task_runtime.block_on(service.execute_operation(
                node.clone(),
                Some("saved-host".into()),
                operation,
                None,
            ));
            result.unwrap();
        };
        run(ForwardingRuntimeOperation::Save { rule: rule.clone() });
        run(ForwardingRuntimeOperation::Update {
            forward_id: rule.id.clone(),
            update: ForwardUpdate {
                target_port: Some(443),
                ..Default::default()
            },
        });
        rule.target_port = 443;
        rule.status = ForwardStatus::Stopped;
        let reloaded = oxideterm_forwarding::SavedForwardStore::load(&path).unwrap();
        assert_eq!(reloaded.load_owned_forwards("saved-host")[0].rule, rule);
        service.registry.update_auto_start(&rule.id, true).unwrap();
        run(ForwardingRuntimeOperation::Stop {
            forward_id: rule.id.clone(),
        });
        assert!(
            !oxideterm_forwarding::SavedForwardStore::load(&path)
                .unwrap()
                .load_owned_forwards("saved-host")[0]
                .auto_start
        );
        run(ForwardingRuntimeOperation::Delete {
            forward_id: rule.id.clone(),
        });
        service.request_forward_stop(&rule.id, true);
        assert!(
            !service
                .persist_started_rule(
                    &ForwardingRuntimeService::session_id_for_node(&node),
                    Some("saved-host".into()),
                    rule.clone(),
                )
                .unwrap()
        );
        assert!(
            oxideterm_forwarding::SavedForwardStore::load(&path)
                .unwrap()
                .load_owned_forwards("saved-host")
                .is_empty()
        );
        assert!(service.node_router.connection_id_for_node(&node).is_none());

        let blocked_parent = dir.path().join("blocked");
        service.registry = ForwardingRegistry::new_with_store(
            oxideterm_forwarding::SavedForwardStore::load(blocked_parent.join("forwards.json"))
                .unwrap(),
        );
        std::fs::write(&blocked_parent, "not a directory").unwrap();
        let result = service.task_runtime.block_on(service.execute_operation(
            node.clone(),
            Some("saved-host".into()),
            ForwardingRuntimeOperation::Save { rule },
            None,
        ));
        assert!(
            result.is_err(),
            "failed durable save must reach the UI error path"
        );
        assert!(service.snapshot_for_node(&node).rules.is_empty());
    }

    #[test]
    fn stop_and_delete_during_initial_restore_do_not_reopen_saved_listeners() {
        let dir = tempfile::tempdir().unwrap();
        let store = oxideterm_forwarding::SavedForwardStore::load(dir.path().join("forwards.json"))
            .unwrap();
        let node = NodeId::new("restoring-node");
        let session = ForwardingRuntimeService::session_id_for_node(&node);
        for id in ["keep", "stop", "delete"] {
            let mut rule = ForwardRule::dynamic("127.0.0.1", 0);
            rule.id = id.into();
            store
                .sync_persisted_forward_rule(id, &session, Some("host".into()), rule)
                .unwrap();
            store.update_auto_start(id, true).unwrap();
        }
        let (events, first_status) = std::sync::mpsc::channel();
        let (resume, resumed) = std::sync::mpsc::channel();
        let resumed = Mutex::new(Some(resumed));
        let delivery = oxideterm_forwarding::ForwardEventDeliverySender::with_wake(
            events,
            Arc::new(move || {
                // Pause after reading the snapshot and starting the first real listener.
                if let Some(resumed) = resumed.lock().unwrap().take() {
                    resumed.recv_timeout(Duration::from_secs(5)).unwrap();
                }
            }),
        );
        let ssh = SshConnectionRegistry::default();
        let service = ForwardingRuntimeService::test_fixture_with_registries(
            ForwardingRegistry::new_with_event_delivery_and_store(delivery, store),
            ssh.clone(),
        );
        let config = oxideterm_ssh::SshConfig::default();
        let handle = ssh.acquire(
            config.clone(),
            ConnectionConsumer::NodeRouter(node.0.clone()),
        );
        handle.set_physical(Arc::new(()));
        ssh.mark_state(handle.connection_id(), ConnectionState::Active)
            .unwrap();
        service.node_router.upsert_node_with_origin(
            node.clone(),
            config,
            oxideterm_ssh::NodeOrigin::Restored {
                saved_connection_id: "host".into(),
            },
        );
        service
            .node_router
            .bind_connection(&node, handle.connection_id())
            .unwrap();
        let restore_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        std::thread::scope(|scope| {
            let restore = scope.spawn(|| {
                restore_runtime
                    .block_on(service.manager_for_node_async(&node, Some("host")))
                    .unwrap()
            });
            let first = first_status.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(
                matches!(first, ForwardEvent::StatusChanged { forward_id, status: ForwardStatus::Active, .. } if forward_id == "keep")
            );
            for operation in [
                ForwardingRuntimeOperation::Stop {
                    forward_id: "stop".into(),
                },
                ForwardingRuntimeOperation::Delete {
                    forward_id: "delete".into(),
                },
            ] {
                let (tx, rx) = delivery::ActiveDeliverySender::channel();
                let wake = tx.wake();
                service.submit_operation(
                    TabId(1),
                    node.clone(),
                    Some("host".into()),
                    "",
                    false,
                    operation,
                    tx,
                );
                service.task_runtime.block_on(wake.wait());
                assert!(matches!(
                    rx.try_recv().unwrap(),
                    ForwardingWorkerResult::Operation { result: Ok(()), .. }
                ));
            }
            resume.send(()).unwrap();
            let (manager, _operation) = restore.join().unwrap();
            assert_eq!(manager.active_forward_count(), 1);
            let rows: Vec<_> = service
                .snapshot_for_node(&node)
                .rules
                .into_iter()
                .map(|rule| (rule.id, rule.status))
                .collect();
            assert_eq!(
                rows,
                [
                    ("keep".into(), ForwardStatus::Active),
                    ("stop".into(), ForwardStatus::Stopped)
                ]
            );
        });
    }

    #[test]
    fn mcp_port_change_preserves_a_later_stop_or_delete() {
        use std::net::TcpListener;

        let ssh = SshConnectionRegistry::default();
        let router = NodeRouter::new(ssh.clone());
        let node = NodeId::new("edit-forward");
        let config = oxideterm_ssh::SshConfig::default();
        let handle = ssh.acquire(
            config.clone(),
            ConnectionConsumer::NodeRouter(node.0.clone()),
        );
        handle.set_physical(Arc::new(()));
        ssh.mark_state(handle.connection_id(), ConnectionState::Active)
            .unwrap();
        router.upsert_node(node.clone(), config);
        router
            .bind_connection(&node, handle.connection_id())
            .unwrap();
        let runtime = Arc::new(
            tokio::runtime::Builder::new_current_thread()
                .max_blocking_threads(1)
                .enable_all()
                .build()
                .unwrap(),
        );
        let (events, changes) = std::sync::mpsc::channel();
        let dir = tempfile::tempdir().unwrap();
        let store = oxideterm_forwarding::SavedForwardStore::load(dir.path().join("forwards.json"))
            .unwrap();
        let service = ForwardingRuntimeService::new(
            ForwardingRegistry::new_with_event_sender_and_store(events, store),
            ssh,
            router,
            runtime.clone(),
            String::new(),
        );
        let occupied = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = occupied.local_addr().unwrap().port();
        // Cover both loopback families when localhost resolves to IPv6 as well.
        let _ipv6 = TcpListener::bind(("::1", port)).ok();
        runtime.block_on(async {
            for (intent, available) in [(None, false), (Some(false), false), (Some(true), false), (Some(false), true)] {
                let port = if available { TcpListener::bind(("127.0.0.1",0)).unwrap().local_addr().unwrap().port() } else { port };
                let original = service
                    .public_mcp_open_forward(
                        &node, Some("host"), ForwardRule::dynamic("127.0.0.1", 0), false, true,
                    )
                    .await
                    .unwrap()
                    .rule;
                // Hold Tokio's DNS worker so the new bind cannot finish before the user's command.
                let (resume, wait) = std::sync::mpsc::channel();
                let blocker = tokio::task::spawn_blocking(move || {
                    wait.recv_timeout(Duration::from_secs(10))
                });
                let edit = tokio::spawn({
                    let service = service.clone();
                    let node = node.clone();
                    let original = original.clone();
                    async move {
                        let update = ForwardUpdate {
                            bind_address: Some("localhost".into()),
                            bind_port: Some(port),
                            ..Default::default()
                        };
                        service
                            .public_mcp_change_forward(&node, Some("host"), &original.id, &original, update, true)
                            .await
                    }
                });
                tokio::time::timeout(Duration::from_secs(3), async {
                    loop {
                        let stopped = changes.try_iter().any(|event| matches!(
                            event,
                            ForwardEvent::StatusChanged { forward_id, status: ForwardStatus::Stopped, .. }
                                if forward_id == original.id
                        ));
                        if stopped {
                            break;
                        }
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                let manager = service.manager_for_node(&node).unwrap();
                assert!(!manager.has_rule(&original.id), "the changed listener is still binding");
                if let Some(delete) = intent {
                    service.stop_saved_tunnel(&original.id, service.request_forward_stop(&original.id, delete)).await.unwrap();
                }
                resume.send(()).unwrap();
                blocker.await.unwrap().unwrap();
                assert!(edit.await.unwrap().is_err());
                let row = manager.list_forwards().into_iter().find(|rule| rule.id == original.id);
                match intent {
                    None => {
                        assert_eq!(row.as_ref(), Some(&original));
                        assert!(std::net::TcpStream::connect(("127.0.0.1", original.bind_port)).is_ok());
                        service.stop_saved_tunnel(&original.id, service.request_forward_stop(&original.id, true)).await.unwrap();
                    }
                    Some(false) => {
                        let mut expected = original.clone();
                        expected.status = ForwardStatus::Stopped;
                        assert_eq!(row, Some(expected));
                        let persisted = service.registry.list_all_saved_forwards().into_iter().find(|saved| saved.id == original.id).unwrap();
                        assert_eq!(persisted.rule.bind_port, original.bind_port);
                        if available { TcpListener::bind(("127.0.0.1",port)).expect("the replacement port was released"); }
                    }
                    Some(true) => assert_eq!(row, None),
                }
                TcpListener::bind(("127.0.0.1", original.bind_port))
                    .expect("a failed edit cannot reopen the original listener");
            }
        });
    }

    #[allow(
        dead_code,
        reason = "The protocol tests also inspect direct TCP channel requests."
    )]
    mod ssh_peer {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../oxideterm-forwarding/tests/support/peer.rs"
        ));
    }

    #[test]
    fn refused_desktop_stop_and_delete_allow_remote_listener_to_reconnect() {
        use oxideterm_ssh::{SshConfig, SshTransportClient};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let ssh = SshConnectionRegistry::default();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("forwards.json");
        let service = ForwardingRuntimeService::test_fixture_with_registries(
            ForwardingRegistry::new_with_store(
                oxideterm_forwarding::SavedForwardStore::load(&path).unwrap(),
            ),
            ssh.clone(),
        );
        service.task_runtime.block_on(async {
            let server = ssh_peer::start_forwarding_ssh_server().await;
            let mut config = SshConfig::password("127.0.0.1", server.port, "tester", "password");
            config.expected_host_key_fingerprint = Some(server.host_key_fingerprint.clone());
            let node = NodeId::new("remote-cancel");
            service
                .node_router
                .upsert_node(node.clone(), config.clone());
            let connect = || {
                SshTransportClient::new(config.clone()).connect_node_with_registry(
                    ssh.clone(),
                    ConnectionConsumer::NodeRouter(node.0.clone()),
                )
            };
            let first = connect().await.unwrap();
            service
                .node_router
                .bind_connection(&node, first.connection_id())
                .unwrap();
            let target = tokio::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let rule = service
                .public_mcp_open_forward(
                    &node,
                    Some("saved-host"),
                    ForwardRule::remote(
                        "127.0.0.1",
                        0,
                        "127.0.0.1",
                        target.local_addr().unwrap().port(),
                    ),
                    false,
                    true,
                )
                .await
                .unwrap()
                .rule;
            let manager = service.manager_for_node(&node).unwrap();
            // Pause the edit after its new listener was bound, before the service commits it.
            manager.stop_forward(&rule.id).await.unwrap();
            let port = tokio::net::TcpListener::bind(("127.0.0.1", 0)).await.unwrap().local_addr().unwrap().port();
            manager.update_forward(&rule.id, ForwardUpdate { bind_port: Some(port), ..Default::default() }).unwrap();
            let replacement = manager.restart_forward(&rule.id).await.unwrap();
            let stop = service.request_forward_stop(&rule.id, false);
            let (stopped, edited) = tokio::join!(biased;
                service.stop_saved_tunnel(&rule.id, stop),
                async {
                    assert!(!manager.has_rule(&rule.id), "the remote cancellation is awaiting its reply");
                    service.finish_mcp_forward_mutation(&node, Some("saved-host"), &manager, replacement, true, Some(&rule)).await
                }
            );
            stopped.unwrap();
            assert!(edited.is_err());
            let saved = oxideterm_forwarding::SavedForwardStore::load(&path).unwrap().load_owned_forwards("saved-host");
            assert_eq!(saved[0].rule.bind_port, rule.bind_port);
            let mut expected = rule.clone();
            expected.status = ForwardStatus::Stopped;
            assert_eq!(manager.list_forwards(), [expected]);
            service.public_mcp_restart_forward(&node, Some("saved-host"), &rule.id, true).await.unwrap();

            server.reject_remote_cancel.store(true, Ordering::SeqCst);
            let stop = service.request_forward_stop(&rule.id, false);
            let (stopped, deleted) = tokio::join!(biased;
                service.stop_saved_tunnel(&rule.id, stop),
                async {
                    assert!(!manager.has_rule(&rule.id), "delete is submitted before the stop reply");
                    let delete = service.request_forward_stop(&rule.id, true);
                    let operation = ForwardingRuntimeOperation::Delete { forward_id: rule.id.clone() };
                    service.execute_operation(node.clone(), None, operation, Some(delete)).await
                }
            );
            assert!(stopped.is_err() && deleted.is_err());
            server.reject_remote_cancel.store(false, Ordering::SeqCst);
            ssh.mark_state_without_event(first.connection_id(), ConnectionState::LinkDown);
            let second = connect().await.unwrap();
            service
                .node_router
                .bind_connection(&node, second.connection_id())
                .unwrap();
            service.restore_session_for_node(&node).await;
            let mut inbound = tokio::net::TcpStream::connect(("127.0.0.1", rule.bind_port))
                .await
                .expect("both failed commands must leave the rule eligible for restore");
            inbound.write_all(b"restored").await.unwrap();
            let received = tokio::time::timeout(Duration::from_secs(3), async {
                let (mut forwarded, _) = target.accept().await.unwrap();
                let mut bytes = [0; 8];
                forwarded.read_exact(&mut bytes).await.unwrap();
                bytes
            })
            .await
            .unwrap();
            assert_eq!(&received, b"restored");
            service
                .manager_for_node(&node)
                .unwrap()
                .delete_forward(&rule.id)
                .await
                .unwrap();
        });
    }

    #[test]
    fn reconnect_restores_mcp_restart_but_not_a_rule_deleted_after_capture() {
        let dir = tempfile::tempdir().unwrap();
        let store = oxideterm_forwarding::SavedForwardStore::load(dir.path().join("forwards.json"))
            .unwrap();
        let ssh = SshConnectionRegistry::default();
        let service = ForwardingRuntimeService::test_fixture_with_registries(
            ForwardingRegistry::new_with_store(store),
            ssh.clone(),
        );
        let node = NodeId::new("forward-reconnect");
        let config = oxideterm_ssh::SshConfig::default();
        let connection = ssh.acquire(
            config.clone(),
            ConnectionConsumer::NodeRouter(node.0.clone()),
        );
        connection.set_physical(Arc::new(()));
        ssh.mark_state(connection.connection_id(), ConnectionState::Active)
            .unwrap();
        service.node_router.upsert_node(node.clone(), config);
        service
            .node_router
            .bind_connection(&node, connection.connection_id())
            .unwrap();

        service.task_runtime.block_on(async {
            let create = |id: &str| {
                let mut rule = ForwardRule::dynamic("127.0.0.1", 0);
                rule.id = id.to_string();
                service.public_mcp_open_forward(&node, Some("host"), rule, false, true)
            };
            let keep = create("restart").await.unwrap().rule;
            ssh.set_idle_timeout(Some(Duration::ZERO));
            let (stopped, handoff) = service
                .public_mcp_stop_forward(&node, Some("host"), &keep.id)
                .await
                .unwrap();
            // The listener has released its lease; revocation may be delivered on a later turn.
            tokio::task::yield_now().await;
            assert_eq!(connection.state(), ConnectionState::Active);
            assert!(
                service
                    .public_mcp_restore_forward_after_revocation(
                        &node,
                        Some("host"),
                        &keep,
                        &stopped,
                        Some(&handoff),
                        false,
                        true
                    )
                    .await
            );
            let remove = create("remove").await.unwrap().rule;
            let (tx, rx) = delivery::ActiveDeliverySender::channel();
            let wake = tx.wake();
            service.submit_operation(
                TabId(1),
                node.clone(),
                Some("host".into()),
                "",
                false,
                ForwardingRuntimeOperation::Stop {
                    forward_id: keep.id.clone(),
                },
                tx,
            );
            wake.wait().await;
            assert!(matches!(
                rx.try_recv().unwrap(),
                ForwardingWorkerResult::Operation { result: Ok(()), .. }
            ));
            // A revoked stop cannot undo a later independent stop, even when the rules look identical.
            assert!(
                !service
                    .public_mcp_restore_forward_after_revocation(
                        &node,
                        Some("host"),
                        &keep,
                        &stopped,
                        Some(&handoff),
                        false,
                        true
                    )
                    .await
            );
            drop(handoff);
            let occupied = std::net::TcpListener::bind(("127.0.0.1", keep.bind_port)).unwrap();
            assert!(
                service
                    .public_mcp_restart_forward(&node, Some("host"), &keep.id, true)
                    .await
                    .is_err()
            );
            assert!(matches!(
                service.restore_block(&keep.id),
                Some(ForwardRestoreBlock::Stopped)
            ));
            drop(occupied);
            let restarted = service
                .public_mcp_restart_forward(&node, Some("host"), &keep.id, true)
                .await
                .unwrap();
            assert_eq!(restarted.rule.status, ForwardStatus::Active);

            let snapshot_reader = service.track_forward_restore();
            let rules = vec![
                reconnect_forward_rule_from_rule(restarted.rule),
                reconnect_forward_rule_from_rule(remove.clone()),
            ];
            assert!(
                service
                    .public_mcp_remove_forward(&node, Some("host"), &remove.id, true)
                    .await
                    .unwrap()
            );
            let replacement = ssh.acquire(
                oxideterm_ssh::SshConfig {
                    port: 2222,
                    ..Default::default()
                },
                ConnectionConsumer::NodeRouter(node.0.clone()),
            );
            replacement.set_physical(Arc::new(()));
            ssh.mark_state(replacement.connection_id(), ConnectionState::Active)
                .unwrap();
            let reconnected = service
                .node_router
                .bind_established_connection(&node, replacement.connection_id())
                .unwrap();
            let (tx, rx) = delivery::ActiveDeliverySender::channel();
            let wake = tx.wake();
            service.submit_reconnect_restore(
                ReconnectForwardRestoreRequest {
                    root_node_id: node.clone(),
                    snapshots: vec![ReconnectForwardRuleSnapshot {
                        node_id: node.0.clone(),
                        rules,
                    }],
                    snapshot_reader,
                    old_connection_ids_by_node: HashMap::new(),
                    owner_connection_ids: HashMap::from([(node.0.clone(), Some("host".into()))]),
                    cancellation: Arc::new(AtomicBool::new(true)),
                    job_id: "reconnect".into(),
                },
                tx,
            );
            drop(reconnected);
            assert_eq!(replacement.state(), ConnectionState::Active);
            wake.wait().await;
            assert!(matches!(
                rx.try_recv().unwrap(),
                ForwardingWorkerResult::ReconnectRestore {
                    restored: 1,
                    result: PhaseResult::Ok,
                    ..
                }
            ));
            let manager = service.manager_for_node(&node).unwrap();
            assert_eq!(
                manager.ssh_connection_handle().connection_id(),
                replacement.connection_id()
            );
            let active: Vec<_> = manager
                .list_forwards()
                .into_iter()
                .map(|rule| (rule.id, rule.status))
                .collect();
            assert_eq!(active, [(keep.id.clone(), ForwardStatus::Active)]);
            let saved: Vec<_> =
                oxideterm_forwarding::SavedForwardStore::load(dir.path().join("forwards.json"))
                    .unwrap()
                    .load_owned_forwards("host")
                    .into_iter()
                    .map(|item| item.id)
                    .collect();
            assert_eq!(saved, [keep.id]);
            assert!(std::net::TcpListener::bind(("127.0.0.1", remove.bind_port)).is_ok());
            assert!(
                service.forward_intent(&remove.id).is_none(),
                "completed snapshots must release their deletion markers"
            );
        });
    }

    #[test]
    fn forwarding_usage_tracks_listeners_across_shared_nodes_and_final_delete() {
        let ssh = SshConnectionRegistry::default();
        let service = ForwardingRuntimeService::test_fixture_with_registries(
            ForwardingRegistry::new(),
            ssh.clone(),
        );
        let node = NodeId::new("a");
        let sibling = NodeId::new("b");
        let owner = ConnectionConsumer::NodeRouter(node.0.clone());
        let config = oxideterm_ssh::SshConfig::default();
        let handle = ssh.acquire(config.clone(), owner.clone());
        let connection = handle.connection_id();
        ssh.mark_state(connection, ConnectionState::Active).unwrap();
        service.node_router.upsert_node(node.clone(), config);
        service
            .node_router
            .bind_connection(&node, connection)
            .unwrap();
        service.task_runtime.block_on(async {
            // An empty restore must not even probe a transport or create a manager.
            // The pool entry has no transport, so a needless probe would mark it link-down.
            service.submit_session_restore(node.clone());
            tokio::task::yield_now().await;
            assert_eq!(handle.state(), ConnectionState::Active);
            assert!(service.manager_for_node(&node).is_none());
            let first = service.registry.register(
                ForwardingRuntimeService::session_id_for_node(&node),
                handle.clone(),
            );
            let second = service.registry.register(
                ForwardingRuntimeService::session_id_for_node(&sibling),
                handle.clone(),
            );
            let running = first
                .create_forward(ForwardRule::local("127.0.0.1", 0, "localhost", 80))
                .await
                .unwrap();
            let other = second
                .create_forward(ForwardRule::dynamic("127.0.0.1", 0))
                .await
                .unwrap();
            let collision = ForwardRule::dynamic("127.0.0.1", running.bind_port);
            assert!(second.create_forward(collision).await.is_err());
            assert_eq!(
                service.registry.active_forward_counts_by_connection(),
                HashMap::from([(connection.to_owned(), 2)])
            );
            assert_eq!(
                handle
                    .info()
                    .consumers
                    .iter()
                    .filter(|consumer| matches!(consumer, ConnectionConsumer::PortForward(_)))
                    .count(),
                2
            );

            service
                .execute_operation(
                    node,
                    None,
                    ForwardingRuntimeOperation::Stop {
                        forward_id: running.id,
                    },
                    None,
                )
                .await
                .unwrap();
            assert_eq!(
                service.registry.active_forward_counts_by_connection(),
                HashMap::from([(connection.to_owned(), 1)])
            );
            assert!(second.get_stats(&other.id).is_ok());
            assert_eq!(
                handle
                    .info()
                    .consumers
                    .iter()
                    .filter(|consumer| matches!(consumer, ConnectionConsumer::PortForward(_)))
                    .count(),
                1
            );

            service
                .execute_operation(
                    sibling,
                    None,
                    ForwardingRuntimeOperation::Delete {
                        forward_id: other.id,
                    },
                    None,
                )
                .await
                .unwrap();
            assert!(
                service
                    .registry
                    .active_forward_counts_by_connection()
                    .is_empty()
            );
            assert_eq!(handle.info().consumers, vec![owner]);
        });
    }

    #[test]
    fn workspace_session_service_shutdown_is_exactly_once() {
        let ssh = SshConnectionRegistry::default();
        let (events, _rx) = std::sync::mpsc::channel();
        let service = ForwardingRuntimeService::test_fixture_with_registries(
            ForwardingRegistry::new_with_event_sender(events),
            ssh.clone(),
        );
        let task_runtime = service.task_runtime.clone();
        let owner = ConnectionConsumer::Terminal("unrelated".into());
        let handle = ssh.acquire(oxideterm_ssh::SshConfig::default(), owner.clone());
        let profiler = {
            let _entered = task_runtime.enter();
            service
                .registry
                .start_port_profiler(handle.connection_id(), handle.clone())
                .unwrap()
        };
        assert!(
            handle
                .info()
                .consumers
                .iter()
                .any(|consumer| matches!(consumer, ConnectionConsumer::Monitor(_)))
        );
        assert!(
            service
                .registry
                .active_forward_counts_by_connection()
                .is_empty()
        );
        service.registry.register("node:first", handle.clone());
        service.registry.register("node:sibling", handle.clone());
        task_runtime.block_on(service.registry.remove("node:first"));
        assert!(
            service
                .registry
                .detected_ports(handle.connection_id())
                .is_some()
        );
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
        assert!(profiler.is_stopped());
        // The current-thread runtime runs the aborted sampler's destructor on its next turn.
        task_runtime.block_on(tokio::task::yield_now());
        assert_eq!(handle.info().consumers, vec![owner]);
        assert!(!second.sftp.started);
        assert!(!second.forwarding.started);
    }
}
