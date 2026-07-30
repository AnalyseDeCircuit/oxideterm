use std::collections::HashMap;

use crate::workspace::{TabId, TerminalSessionId};
use gpui::Context;
use oxideterm_ai::{
    RuntimeCapability, RuntimeCapabilityRegistry, RuntimeContextError, RuntimeHandleId,
    RuntimeHandleProjection, RuntimeOwnerGeneration, RuntimeOwnerKey, RuntimeOwnerKind,
    RuntimeOwnerRegistration, RuntimeRevocationReason, RuntimeValidationError,
    RuntimeValidationFailure, ToolSessionId,
};
use oxideterm_ssh::NodeId;

#[derive(Clone, Debug)]
struct TerminalRuntimeOwner {
    key: RuntimeOwnerKey,
    generation: RuntimeOwnerGeneration,
}

#[derive(Clone, Debug)]
struct LocalShellRuntimeOwner {
    key: RuntimeOwnerKey,
    generation: RuntimeOwnerGeneration,
}

#[derive(Clone)]
struct NodeRuntimeOwner {
    key: RuntimeOwnerKey,
    generation: RuntimeOwnerGeneration,
    connection_id: String,
}

#[derive(Clone)]
struct SftpRuntimeOwner {
    key: RuntimeOwnerKey,
    generation: RuntimeOwnerGeneration,
    connection_id: String,
    session_generation: u64,
}

#[derive(Clone)]
pub(in crate::workspace) struct AiSftpRuntimeOwner {
    pub(in crate::workspace) node_id: NodeId,
    pub(in crate::workspace) connection_id: String,
    pub(in crate::workspace) session_generation: u64,
}

impl std::fmt::Debug for AiSftpRuntimeOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The proof is internal dispatch authority, so diagnostics expose only its class.
        formatter
            .debug_struct("AiSftpRuntimeOwner")
            .field("owner", &"[redacted]")
            .finish()
    }
}

#[derive(Clone)]
struct IdeRuntimeOwner {
    key: RuntimeOwnerKey,
    generation: RuntimeOwnerGeneration,
    node_id: NodeId,
}

#[derive(Clone)]
struct AppSurfaceRuntimeOwner {
    key: RuntimeOwnerKey,
    generation: RuntimeOwnerGeneration,
}

/// Owns AI tool-session authority without taking ownership of terminal or transport runtime.
pub(in crate::workspace) struct AiRuntimeContextEntity {
    registry: RuntimeCapabilityRegistry,
    tool_sessions: HashMap<u64, ToolSessionId>,
    terminal_owners: HashMap<TerminalSessionId, TerminalRuntimeOwner>,
    node_owners: HashMap<NodeId, NodeRuntimeOwner>,
    sftp_owners: HashMap<NodeId, SftpRuntimeOwner>,
    ide_owners: HashMap<u64, IdeRuntimeOwner>,
    app_surface_owners: HashMap<u64, AppSurfaceRuntimeOwner>,
    local_shell_owner: LocalShellRuntimeOwner,
    accepting_broker_requests: bool,
}

impl std::fmt::Debug for AiRuntimeContextEntity {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Runtime owner identities must never leak through diagnostic formatting.
        formatter
            .debug_struct("AiRuntimeContextEntity")
            .field("tool_session_count", &self.tool_sessions.len())
            .field("terminal_owner_count", &self.terminal_owners.len())
            .field("node_owner_count", &self.node_owners.len())
            .field("sftp_owner_count", &self.sftp_owners.len())
            .field("ide_owner_count", &self.ide_owners.len())
            .field("app_surface_owner_count", &self.app_surface_owners.len())
            .field("accepting_broker_requests", &self.accepting_broker_requests)
            .finish()
    }
}

impl AiRuntimeContextEntity {
    pub(in crate::workspace) fn new() -> Self {
        let mut registry = RuntimeCapabilityRegistry::new();
        let local_shell_owner = LocalShellRuntimeOwner {
            key: RuntimeOwnerKey::new(),
            generation: RuntimeOwnerGeneration::new(1),
        };
        let registration = RuntimeOwnerRegistration::new(
            local_shell_owner.key.clone(),
            RuntimeOwnerKind::LocalShell,
            local_shell_owner.generation,
            "Local shell service".to_string(),
            [RuntimeCapability::LocalShellRunCommand],
            Some(
                oxideterm_ai::StableResourceRef::new(
                    oxideterm_ai::StableResourceKind::LocalShellProfile,
                    "default".to_string(),
                    Some("Local shell".to_string()),
                )
                .expect("constant local shell resource reference is valid"),
            ),
        )
        .expect("constant local shell runtime owner registration is valid");
        registry
            .register_owner(registration)
            .expect("new local shell runtime owner cannot conflict");
        Self {
            registry,
            tool_sessions: HashMap::new(),
            terminal_owners: HashMap::new(),
            node_owners: HashMap::new(),
            sftp_owners: HashMap::new(),
            ide_owners: HashMap::new(),
            app_surface_owners: HashMap::new(),
            local_shell_owner,
            accepting_broker_requests: true,
        }
    }

    /// Entity release is the final broker boundary. Queued callbacks must stop
    /// validating before any registered owner projection is discarded.
    pub(in crate::workspace) fn attach_release_shutdown(cx: &mut Context<Self>) {
        cx.on_release(|runtime, _cx| runtime.shutdown()).detach();
    }

    /// A replacement stream must invalidate every previous model capability lease first.
    pub(in crate::workspace) fn begin_tool_session(
        &mut self,
        stream_generation: u64,
    ) -> ToolSessionId {
        self.finish_all_tool_sessions(RuntimeRevocationReason::ToolSessionCancelled);
        let tool_session_id = self.registry.begin_tool_session();
        self.tool_sessions
            .insert(stream_generation, tool_session_id.clone());
        tool_session_id
    }

    pub(in crate::workspace) fn finish_tool_session(
        &mut self,
        stream_generation: u64,
        reason: RuntimeRevocationReason,
    ) {
        if let Some(tool_session_id) = self.tool_sessions.remove(&stream_generation) {
            self.registry.finish_tool_session(&tool_session_id, reason);
        }
    }

    /// Rejects work queued by a stream that has been cancelled or replaced.
    pub(in crate::workspace) fn is_active_tool_session(
        &self,
        stream_generation: u64,
        tool_session_id: &ToolSessionId,
    ) -> bool {
        self.accepting_broker_requests
            && self
                .tool_sessions
                .get(&stream_generation)
                .is_some_and(|current| current == tool_session_id)
            && self.registry.is_tool_session_active(tool_session_id)
    }

    /// The epoch is descriptive context only; dispatch never accepts it from a model.
    pub(in crate::workspace) fn registry_epoch(&self) -> oxideterm_ai::RuntimeRegistryEpoch {
        self.registry.epoch().clone()
    }

    /// Registers the terminal session itself rather than its tab or pane mount.
    /// Detach/return may change presentation ownership, but must not replace the
    /// physical terminal capability.
    pub(in crate::workspace) fn register_terminal_session(
        &mut self,
        session_id: TerminalSessionId,
        label: String,
    ) {
        let owner =
            self.terminal_owners
                .entry(session_id)
                .or_insert_with(|| TerminalRuntimeOwner {
                    key: RuntimeOwnerKey::new(),
                    generation: RuntimeOwnerGeneration::new(1),
                });
        let registration = RuntimeOwnerRegistration::new(
            owner.key.clone(),
            RuntimeOwnerKind::Terminal,
            owner.generation,
            label,
            [
                RuntimeCapability::TerminalObserve,
                RuntimeCapability::TerminalRunCommand,
                RuntimeCapability::TerminalSendInput,
            ],
            None,
        )
        .expect("constant terminal runtime owner registration is valid");
        self.registry
            .register_owner(registration)
            .expect("new terminal runtime owner cannot conflict");
    }

    /// Registers a physical node connection from its lifecycle event. Metadata
    /// refreshes for the same connection never advance the owner generation.
    pub(in crate::workspace) fn register_node_connection(
        &mut self,
        node_id: NodeId,
        connection_id: String,
        label: String,
        resource_ref: Option<oxideterm_ai::StableResourceRef>,
    ) {
        let owner = self
            .node_owners
            .entry(node_id)
            .or_insert_with(|| NodeRuntimeOwner {
                key: RuntimeOwnerKey::new(),
                generation: RuntimeOwnerGeneration::new(1),
                connection_id: connection_id.clone(),
            });
        if owner.connection_id != connection_id {
            owner.connection_id = connection_id;
            owner.generation = next_owner_generation(owner.generation);
        }
        let registration = RuntimeOwnerRegistration::new(
            owner.key.clone(),
            RuntimeOwnerKind::SshNode,
            owner.generation,
            label,
            [RuntimeCapability::NodeInspect],
            resource_ref,
        )
        .expect("node lifecycle data creates a valid runtime owner");
        self.registry
            .register_owner(registration)
            .expect("node lifecycle owner generation is monotonic");
    }

    /// A ready SFTP session is a separate capability owner from the transport
    /// node because the router may recreate it without replacing the SSH link.
    pub(in crate::workspace) fn register_sftp_session(
        &mut self,
        node_id: NodeId,
        connection_id: String,
        session_generation: u64,
        label: String,
        resource_ref: Option<oxideterm_ai::StableResourceRef>,
    ) {
        let owner = self
            .sftp_owners
            .entry(node_id)
            .or_insert_with(|| SftpRuntimeOwner {
                key: RuntimeOwnerKey::new(),
                generation: RuntimeOwnerGeneration::new(1),
                connection_id: connection_id.clone(),
                session_generation,
            });
        if owner.connection_id != connection_id || owner.session_generation != session_generation {
            owner.connection_id = connection_id;
            owner.session_generation = session_generation;
            owner.generation = next_owner_generation(owner.generation);
        }
        let registration = RuntimeOwnerRegistration::new(
            owner.key.clone(),
            RuntimeOwnerKind::SftpSession,
            owner.generation,
            label,
            [
                RuntimeCapability::SftpRead,
                RuntimeCapability::SftpWrite,
                RuntimeCapability::SftpStartTransfer,
            ],
            resource_ref,
        )
        .expect("SFTP lifecycle data creates a valid runtime owner");
        self.registry
            .register_owner(registration)
            .expect("SFTP lifecycle owner generation is monotonic");
    }

    /// IDE tabs own their editor runtime independently from the shared node.
    pub(in crate::workspace) fn register_ide_surface(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        label: String,
        resource_ref: Option<oxideterm_ai::StableResourceRef>,
    ) {
        let owner = self
            .ide_owners
            .entry(tab_id.0)
            .or_insert_with(|| IdeRuntimeOwner {
                key: RuntimeOwnerKey::new(),
                generation: RuntimeOwnerGeneration::new(1),
                node_id: node_id.clone(),
            });
        if owner.node_id != node_id {
            owner.node_id = node_id;
            owner.generation = next_owner_generation(owner.generation);
        }
        let registration = RuntimeOwnerRegistration::new(
            owner.key.clone(),
            RuntimeOwnerKind::IdeSurface,
            owner.generation,
            label,
            [RuntimeCapability::IdeRead, RuntimeCapability::IdeWrite],
            resource_ref,
        )
        .expect("IDE lifecycle data creates a valid runtime owner");
        self.registry
            .register_owner(registration)
            .expect("IDE lifecycle owner generation is monotonic");
    }

    /// Closing an IDE tab revokes only that surface capability; it must never
    /// release the shared NodeRouter connection.
    pub(in crate::workspace) fn revoke_ide_surface(&mut self, tab_id: TabId) {
        let Some(owner) = self.ide_owners.remove(&tab_id.0) else {
            return;
        };
        self.registry
            .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
    }

    /// A mounted tab is the exact owner of focus authority. Reopening the same
    /// surface kind creates another owner instead of rebinding a stale handle.
    pub(in crate::workspace) fn register_app_surface(
        &mut self,
        tab_id: TabId,
        label: String,
        resource_ref: Option<oxideterm_ai::StableResourceRef>,
    ) {
        let owner =
            self.app_surface_owners
                .entry(tab_id.0)
                .or_insert_with(|| AppSurfaceRuntimeOwner {
                    key: RuntimeOwnerKey::new(),
                    generation: RuntimeOwnerGeneration::new(1),
                });
        let registration = RuntimeOwnerRegistration::new(
            owner.key.clone(),
            RuntimeOwnerKind::AppSurface,
            owner.generation,
            label,
            [RuntimeCapability::SurfaceFocus],
            resource_ref,
        )
        .expect("tab lifecycle data creates a valid application surface owner");
        self.registry
            .register_owner(registration)
            .expect("application surface owner identity remains stable while mounted");
    }

    pub(in crate::workspace) fn revoke_app_surface(&mut self, tab_id: TabId) {
        let Some(owner) = self.app_surface_owners.remove(&tab_id.0) else {
            return;
        };
        self.registry
            .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
    }

    /// Physical-node teardown invalidates the node and every AI owner whose
    /// backend depends on that connection. Terminal owners remain independent.
    pub(in crate::workspace) fn revoke_node_connection(&mut self, node_id: &NodeId) {
        if let Some(owner) = self.node_owners.remove(node_id) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
        }
        self.revoke_sftp_session(node_id);
        self.revoke_ide_surfaces_for_node(node_id);
    }

    /// An SFTP subsystem reset cannot revoke the node or its terminal owners.
    pub(in crate::workspace) fn revoke_sftp_session(&mut self, node_id: &NodeId) {
        if let Some(owner) = self.sftp_owners.remove(node_id) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
        }
    }

    fn revoke_ide_surfaces_for_node(&mut self, node_id: &NodeId) {
        let tab_ids = self
            .ide_owners
            .iter()
            .filter_map(|(tab_id, owner)| (&owner.node_id == node_id).then_some(*tab_id))
            .collect::<Vec<_>>();
        for tab_id in tab_ids {
            if let Some(owner) = self.ide_owners.remove(&tab_id) {
                self.registry
                    .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
            }
        }
    }

    /// Revocation follows terminal-session termination, not a pane unmount.
    pub(in crate::workspace) fn revoke_terminal_session(&mut self, session_id: TerminalSessionId) {
        let Some(owner) = self.terminal_owners.remove(&session_id) else {
            return;
        };
        self.registry
            .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
    }

    pub(in crate::workspace) fn issue_terminal_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
        session_id: TerminalSessionId,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        let owner = self
            .terminal_owners
            .get(&session_id)
            .ok_or(RuntimeContextError::OwnerNotFound)?;
        self.registry.issue_handle(tool_session_id, &owner.key)
    }

    pub(in crate::workspace) fn issue_local_shell_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        self.registry
            .issue_handle(tool_session_id, &self.local_shell_owner.key)
    }

    pub(in crate::workspace) fn issue_node_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
        node_id: &NodeId,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        // Node inspection is intentionally distinct from terminal command
        // authority even when both owners share one physical connection.
        let owner = self
            .node_owners
            .get(node_id)
            .ok_or(RuntimeContextError::OwnerNotFound)?;
        self.registry.issue_handle(tool_session_id, &owner.key)
    }

    pub(in crate::workspace) fn issue_sftp_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
        node_id: &NodeId,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        let owner = self
            .sftp_owners
            .get(node_id)
            .ok_or(RuntimeContextError::OwnerNotFound)?;
        self.registry.issue_handle(tool_session_id, &owner.key)
    }

    pub(in crate::workspace) fn issue_ide_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
        tab_id: TabId,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        let owner = self
            .ide_owners
            .get(&tab_id.0)
            .ok_or(RuntimeContextError::OwnerNotFound)?;
        self.registry.issue_handle(tool_session_id, &owner.key)
    }

    pub(in crate::workspace) fn issue_app_surface_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
        tab_id: TabId,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        let owner = self
            .app_surface_owners
            .get(&tab_id.0)
            .ok_or(RuntimeContextError::OwnerNotFound)?;
        self.registry.issue_handle(tool_session_id, &owner.key)
    }

    /// Projects leases already issued by authoritative discovery. It never
    /// scans tabs, nodes, or labels to synthesize missing runtime registrations.
    pub(in crate::workspace) fn current_handle_projections(
        &self,
        tool_session_id: &ToolSessionId,
    ) -> Vec<RuntimeHandleProjection> {
        self.registry.issued_handles_for_session(tool_session_id)
    }

    /// Parses and validates a model-submitted handle immediately before routing
    /// to a terminal session. The resulting session id never enters model output.
    pub(in crate::workspace) fn validate_terminal_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
        capability: RuntimeCapability,
    ) -> Result<TerminalSessionId, RuntimeValidationError> {
        let handle_id = raw_handle_id
            .map(|value| RuntimeHandleId::parse(value.to_string()))
            .transpose()
            .map_err(|_| RuntimeValidationError::new(RuntimeValidationFailure::UnknownHandle))?;
        let validated =
            self.registry
                .validate_handle(tool_session_id, handle_id.as_ref(), capability)?;
        if validated.owner_kind() != RuntimeOwnerKind::Terminal {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
        self.terminal_owners
            .iter()
            .find_map(|(session_id, owner)| {
                (owner.key == *validated.owner_key()
                    && owner.generation == validated.owner_generation())
                .then_some(*session_id)
            })
            .ok_or_else(|| RuntimeValidationError::new(RuntimeValidationFailure::OwnerClosed))
    }

    /// Validates run-command authority without permitting a terminal handle to
    /// be reinterpreted as the app-wide local shell service, or vice versa.
    pub(in crate::workspace) fn validate_run_command_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
    ) -> Result<AiRunCommandOwner, RuntimeValidationError> {
        match self.validate_terminal_handle(
            tool_session_id,
            raw_handle_id,
            RuntimeCapability::TerminalRunCommand,
        ) {
            Ok(session_id) => Ok(AiRunCommandOwner::Terminal(session_id)),
            Err(error) if error.failure() == RuntimeValidationFailure::CapabilityUnavailable => {
                let handle_id = raw_handle_id
                    .map(|value| RuntimeHandleId::parse(value.to_string()))
                    .transpose()
                    .map_err(|_| {
                        RuntimeValidationError::new(RuntimeValidationFailure::UnknownHandle)
                    })?;
                let validated = self.registry.validate_handle(
                    tool_session_id,
                    handle_id.as_ref(),
                    RuntimeCapability::LocalShellRunCommand,
                )?;
                if validated.owner_kind() != RuntimeOwnerKind::LocalShell
                    || validated.owner_key() != &self.local_shell_owner.key
                    || validated.owner_generation() != self.local_shell_owner.generation
                {
                    return Err(RuntimeValidationError::new(
                        RuntimeValidationFailure::CapabilityUnavailable,
                    ));
                }
                Ok(AiRunCommandOwner::LocalShell)
            }
            Err(error) => Err(error),
        }
    }

    pub(in crate::workspace) fn validate_sftp_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
        capability: RuntimeCapability,
    ) -> Result<AiSftpRuntimeOwner, RuntimeValidationError> {
        let validated = self.validate_handle_for_kind(
            tool_session_id,
            raw_handle_id,
            capability,
            RuntimeOwnerKind::SftpSession,
        )?;
        self.sftp_owners
            .iter()
            .find_map(|(node_id, owner)| {
                (owner.key == *validated.owner_key()
                    && owner.generation == validated.owner_generation())
                .then(|| AiSftpRuntimeOwner {
                    node_id: node_id.clone(),
                    connection_id: owner.connection_id.clone(),
                    session_generation: owner.session_generation,
                })
            })
            .ok_or_else(|| RuntimeValidationError::new(RuntimeValidationFailure::OwnerClosed))
    }

    pub(in crate::workspace) fn validate_ide_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
        capability: RuntimeCapability,
    ) -> Result<(TabId, NodeId), RuntimeValidationError> {
        let validated = self.validate_handle_for_kind(
            tool_session_id,
            raw_handle_id,
            capability,
            RuntimeOwnerKind::IdeSurface,
        )?;
        self.ide_owners
            .iter()
            .find_map(|(tab_id, owner)| {
                (owner.key == *validated.owner_key()
                    && owner.generation == validated.owner_generation())
                .then(|| (TabId(*tab_id), owner.node_id.clone()))
            })
            .ok_or_else(|| RuntimeValidationError::new(RuntimeValidationFailure::OwnerClosed))
    }

    pub(in crate::workspace) fn validate_app_surface_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
    ) -> Result<TabId, RuntimeValidationError> {
        let validated = self.validate_handle_for_kind(
            tool_session_id,
            raw_handle_id,
            RuntimeCapability::SurfaceFocus,
            RuntimeOwnerKind::AppSurface,
        )?;
        self.app_surface_owners
            .iter()
            .find_map(|(tab_id, owner)| {
                (owner.key == *validated.owner_key()
                    && owner.generation == validated.owner_generation())
                .then_some(TabId(*tab_id))
            })
            .ok_or_else(|| RuntimeValidationError::new(RuntimeValidationFailure::OwnerClosed))
    }

    pub(in crate::workspace) fn validate_state_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
    ) -> Result<RuntimeHandleProjection, RuntimeValidationError> {
        let handle_id = raw_handle_id
            .map(|value| RuntimeHandleId::parse(value.to_string()))
            .transpose()
            .map_err(|_| RuntimeValidationError::new(RuntimeValidationFailure::UnknownHandle))?;
        self.registry
            .validate_handle_projection(tool_session_id, handle_id.as_ref())
    }

    fn validate_handle_for_kind(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
        capability: RuntimeCapability,
        owner_kind: RuntimeOwnerKind,
    ) -> Result<oxideterm_ai::ValidatedRuntimeHandle, RuntimeValidationError> {
        let handle_id = raw_handle_id
            .map(|value| RuntimeHandleId::parse(value.to_string()))
            .transpose()
            .map_err(|_| RuntimeValidationError::new(RuntimeValidationFailure::UnknownHandle))?;
        let validated =
            self.registry
                .validate_handle(tool_session_id, handle_id.as_ref(), capability)?;
        if validated.owner_kind() != owner_kind {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
        Ok(validated)
    }

    fn finish_all_tool_sessions(&mut self, reason: RuntimeRevocationReason) {
        for tool_session_id in self.tool_sessions.drain().map(|(_, session)| session) {
            self.registry.finish_tool_session(&tool_session_id, reason);
        }
    }

    /// Begins application shutdown before UI interaction owners are released.
    pub(in crate::workspace) fn stop_accepting_and_finish_tool_sessions(&mut self) {
        if !self.accepting_broker_requests {
            return;
        }
        self.accepting_broker_requests = false;
        self.finish_all_tool_sessions(RuntimeRevocationReason::ApplicationShutdown);
    }

    /// Revokes owner projections after pending approval and selection waiters
    /// have been cancelled by the AI workspace entity.
    pub(in crate::workspace) fn revoke_registered_owner_projections(&mut self) {
        for owner in self.terminal_owners.drain().map(|(_, owner)| owner) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::ApplicationShutdown);
        }
        for owner in self.node_owners.drain().map(|(_, owner)| owner) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::ApplicationShutdown);
        }
        for owner in self.sftp_owners.drain().map(|(_, owner)| owner) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::ApplicationShutdown);
        }
        for owner in self.ide_owners.drain().map(|(_, owner)| owner) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::ApplicationShutdown);
        }
        for owner in self.app_surface_owners.drain().map(|(_, owner)| owner) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::ApplicationShutdown);
        }
        self.registry.revoke_owner(
            &self.local_shell_owner.key,
            RuntimeRevocationReason::ApplicationShutdown,
        );
    }

    fn shutdown(&mut self) {
        self.stop_accepting_and_finish_tool_sessions();
        self.revoke_registered_owner_projections();
    }
}

fn next_owner_generation(current: RuntimeOwnerGeneration) -> RuntimeOwnerGeneration {
    RuntimeOwnerGeneration::new(current.value().saturating_add(1))
}

impl Default for AiRuntimeContextEntity {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::workspace) enum AiRunCommandOwner {
    Terminal(TerminalSessionId),
    LocalShell,
}

#[cfg(test)]
mod tests {
    use crate::workspace::{TabId, TerminalSessionId};
    use oxideterm_ssh::NodeId;

    use super::AiRuntimeContextEntity;

    #[test]
    fn replacement_stream_invalidates_the_previous_tool_session() {
        let mut entity = AiRuntimeContextEntity::new();
        let first = entity.begin_tool_session(7);
        let second = entity.begin_tool_session(8);

        assert!(!entity.is_active_tool_session(7, &first));
        assert!(entity.is_active_tool_session(8, &second));
        assert_ne!(first, second);
    }

    #[test]
    fn cancelled_stream_cannot_dispatch_queued_work() {
        let mut entity = AiRuntimeContextEntity::new();
        let session = entity.begin_tool_session(7);

        entity.finish_tool_session(
            7,
            oxideterm_ai::RuntimeRevocationReason::ToolSessionCancelled,
        );

        assert!(!entity.is_active_tool_session(7, &session));
    }

    #[test]
    fn shutdown_rejects_late_broker_callbacks() {
        let mut entity = AiRuntimeContextEntity::new();
        let session = entity.begin_tool_session(7);

        entity.shutdown();
        entity.shutdown();

        assert!(!entity.is_active_tool_session(7, &session));
        assert!(entity.terminal_owners.is_empty());
        assert!(entity.node_owners.is_empty());
        assert!(entity.sftp_owners.is_empty());
        assert!(entity.ide_owners.is_empty());
        assert!(entity.app_surface_owners.is_empty());
    }

    #[test]
    fn closed_terminal_handle_cannot_be_validated() {
        let mut entity = AiRuntimeContextEntity::new();
        let session_id = TerminalSessionId(42);
        entity.register_terminal_session(session_id, "Terminal".to_string());
        let tool_session_id = entity.begin_tool_session(7);
        let handle = entity
            .issue_terminal_handle(&tool_session_id, session_id)
            .expect("terminal handle issues");

        entity.revoke_terminal_session(session_id);

        let error = entity
            .validate_terminal_handle(
                &tool_session_id,
                Some(handle.handle_id.as_str()),
                oxideterm_ai::RuntimeCapability::TerminalObserve,
            )
            .expect_err("closed terminal does not retain authority");
        assert_eq!(error.public_code(), "runtime_owner_closed");
    }

    #[test]
    fn detaching_terminal_preserves_session_owner() {
        let mut entity = AiRuntimeContextEntity::new();
        let session_id = TerminalSessionId(42);
        entity.register_terminal_session(session_id, "Terminal".to_string());
        let tool_session_id = entity.begin_tool_session(7);
        let handle = entity
            .issue_terminal_handle(&tool_session_id, session_id)
            .expect("terminal handle issues");

        // Detach and return re-announce the same session owner instead of
        // creating a new terminal generation.
        entity.register_terminal_session(session_id, "Detached terminal".to_string());

        assert!(
            entity
                .validate_terminal_handle(
                    &tool_session_id,
                    Some(handle.handle_id.as_str()),
                    oxideterm_ai::RuntimeCapability::TerminalObserve,
                )
                .is_ok()
        );
    }

    #[test]
    fn closing_terminal_does_not_revoke_shared_node() {
        let mut entity = AiRuntimeContextEntity::new();
        let node_id = NodeId::new("node-a");
        let session_id = TerminalSessionId(42);
        entity.register_node_connection(
            node_id.clone(),
            "connection-a".to_string(),
            "Node".to_string(),
            None,
        );
        entity.register_terminal_session(session_id, "Terminal".to_string());
        let tool_session_id = entity.begin_tool_session(7);
        let node_handle = entity
            .issue_node_handle(&tool_session_id, &node_id)
            .expect("node handle issues");

        entity.revoke_terminal_session(session_id);

        assert!(
            entity
                .registry
                .validate_handle(
                    &tool_session_id,
                    Some(&node_handle.handle_id),
                    oxideterm_ai::RuntimeCapability::NodeInspect,
                )
                .is_ok(),
            "terminal teardown must not revoke the shared SSH node"
        );
    }

    #[test]
    fn sftp_replacement_revokes_only_the_previous_sftp_generation() {
        let mut entity = AiRuntimeContextEntity::new();
        let node_id = NodeId::new("node-a");
        let terminal_session_id = TerminalSessionId(42);
        entity.register_terminal_session(terminal_session_id, "Terminal".to_string());
        entity.register_sftp_session(
            node_id.clone(),
            "connection-a".to_string(),
            7,
            "SFTP owner".to_string(),
            None,
        );
        let tool_session_id = entity.begin_tool_session(7);
        let terminal_handle = entity
            .issue_terminal_handle(&tool_session_id, terminal_session_id)
            .expect("terminal handle issues");
        let old_sftp_handle = entity
            .issue_sftp_handle(&tool_session_id, &node_id)
            .expect("SFTP handle issues");

        entity.register_sftp_session(
            node_id.clone(),
            "connection-a".to_string(),
            8,
            "SFTP owner".to_string(),
            None,
        );

        let error = entity
            .validate_sftp_handle(
                &tool_session_id,
                Some(old_sftp_handle.handle_id.as_str()),
                oxideterm_ai::RuntimeCapability::SftpRead,
            )
            .expect_err("the prior concrete channel generation must be stale");
        assert_eq!(error.public_code(), "runtime_owner_replaced");
        assert!(
            entity
                .validate_terminal_handle(
                    &tool_session_id,
                    Some(terminal_handle.handle_id.as_str()),
                    oxideterm_ai::RuntimeCapability::TerminalObserve,
                )
                .is_ok(),
            "replacing SFTP must not revoke an unrelated terminal owner"
        );
    }

    #[test]
    fn node_disconnect_revokes_sftp_and_ide_but_not_terminal_handles() {
        let mut entity = AiRuntimeContextEntity::new();
        let node_id = NodeId::new("node-a");
        let terminal_session_id = TerminalSessionId(42);
        let ide_tab_id = TabId(13);
        entity.register_node_connection(
            node_id.clone(),
            "connection-a".to_string(),
            "Node".to_string(),
            None,
        );
        entity.register_terminal_session(terminal_session_id, "Terminal".to_string());
        entity.register_sftp_session(
            node_id.clone(),
            "connection-a".to_string(),
            7,
            "SFTP owner".to_string(),
            None,
        );
        entity.register_ide_surface(ide_tab_id, node_id.clone(), "IDE owner".to_string(), None);
        let tool_session_id = entity.begin_tool_session(7);
        let terminal_handle = entity
            .issue_terminal_handle(&tool_session_id, terminal_session_id)
            .expect("terminal handle issues");
        let node_handle = entity
            .issue_node_handle(&tool_session_id, &node_id)
            .expect("node handle issues");
        let sftp_handle = entity
            .issue_sftp_handle(&tool_session_id, &node_id)
            .expect("SFTP handle issues");
        let ide_handle = entity
            .issue_ide_handle(&tool_session_id, ide_tab_id)
            .expect("IDE handle issues");

        entity.revoke_node_connection(&node_id);

        assert!(
            entity
                .registry
                .validate_handle(
                    &tool_session_id,
                    Some(&node_handle.handle_id),
                    oxideterm_ai::RuntimeCapability::NodeInspect,
                )
                .is_err()
        );
        assert!(
            entity
                .validate_sftp_handle(
                    &tool_session_id,
                    Some(sftp_handle.handle_id.as_str()),
                    oxideterm_ai::RuntimeCapability::SftpRead,
                )
                .is_err()
        );
        assert!(
            entity
                .validate_ide_handle(
                    &tool_session_id,
                    Some(ide_handle.handle_id.as_str()),
                    oxideterm_ai::RuntimeCapability::IdeRead,
                )
                .is_err()
        );
        assert!(
            entity
                .validate_terminal_handle(
                    &tool_session_id,
                    Some(terminal_handle.handle_id.as_str()),
                    oxideterm_ai::RuntimeCapability::TerminalObserve,
                )
                .is_ok(),
            "node teardown must not revoke the independently owned terminal session"
        );
    }

    #[test]
    fn node_metadata_refresh_preserves_the_connection_generation() {
        let mut entity = AiRuntimeContextEntity::new();
        let node_id = NodeId::new("node-a");
        entity.register_node_connection(
            node_id.clone(),
            "connection-a".to_string(),
            "Initial label".to_string(),
            None,
        );
        let tool_session_id = entity.begin_tool_session(7);
        let handle = entity
            .issue_node_handle(&tool_session_id, &node_id)
            .expect("node handle issues");

        // A label update is metadata, not a replacement of the physical connection.
        entity.register_node_connection(
            node_id,
            "connection-a".to_string(),
            "Updated label".to_string(),
            None,
        );

        assert!(
            entity
                .registry
                .validate_handle(
                    &tool_session_id,
                    Some(&handle.handle_id),
                    oxideterm_ai::RuntimeCapability::NodeInspect,
                )
                .is_ok()
        );
    }

    #[test]
    fn closing_one_ide_surface_does_not_select_another_surface() {
        let mut entity = AiRuntimeContextEntity::new();
        let first_tab = TabId(11);
        let second_tab = TabId(12);
        entity.register_ide_surface(
            first_tab,
            NodeId::new("node-a"),
            "First IDE".to_string(),
            None,
        );
        entity.register_ide_surface(
            second_tab,
            NodeId::new("node-a"),
            "Second IDE".to_string(),
            None,
        );
        let tool_session_id = entity.begin_tool_session(7);
        let first_handle = entity
            .issue_ide_handle(&tool_session_id, first_tab)
            .expect("first IDE handle issues");
        let second_handle = entity
            .issue_ide_handle(&tool_session_id, second_tab)
            .expect("second IDE handle issues");

        entity.revoke_ide_surface(first_tab);

        assert!(
            entity
                .validate_ide_handle(
                    &tool_session_id,
                    Some(first_handle.handle_id.as_str()),
                    oxideterm_ai::RuntimeCapability::IdeRead,
                )
                .is_err(),
            "a closed surface must not rebind to another surface on the same node"
        );
        assert_eq!(
            entity
                .validate_ide_handle(
                    &tool_session_id,
                    Some(second_handle.handle_id.as_str()),
                    oxideterm_ai::RuntimeCapability::IdeRead,
                )
                .expect("the second surface remains valid")
                .0,
            second_tab
        );
    }

    #[test]
    fn closed_app_surface_handle_cannot_focus_another_tab() {
        let mut entity = AiRuntimeContextEntity::new();
        let first_tab = TabId(21);
        let second_tab = TabId(22);
        entity.register_app_surface(first_tab, "First settings".to_string(), None);
        entity.register_app_surface(second_tab, "Second settings".to_string(), None);
        let tool_session_id = entity.begin_tool_session(9);
        let first_handle = entity
            .issue_app_surface_handle(&tool_session_id, first_tab)
            .expect("surface handle issues");

        entity.revoke_app_surface(first_tab);

        let error = entity
            .validate_app_surface_handle(&tool_session_id, Some(first_handle.handle_id.as_str()))
            .expect_err("closed surface cannot rebind to another tab");
        assert_eq!(error.public_code(), "runtime_owner_closed");
        let second_handle = entity
            .issue_app_surface_handle(&tool_session_id, second_tab)
            .expect("remaining surface handle issues");
        assert_eq!(
            entity
                .validate_app_surface_handle(
                    &tool_session_id,
                    Some(second_handle.handle_id.as_str()),
                )
                .expect("remaining surface stays valid"),
            second_tab
        );
    }
}
