use std::collections::HashMap;

use crate::tool_resources::{
    AiStableResourceOperation, ai_live_resource_operation, ai_rejects_legacy_live_target_argument,
    ai_stable_resource_operation,
};
use crate::{
    RuntimeCapability, RuntimeCapabilityRegistry, RuntimeContextError, RuntimeHandleId,
    RuntimeHandleProjection, RuntimeOwnerGeneration, RuntimeOwnerKey, RuntimeOwnerKind,
    RuntimeOwnerRegistration, RuntimeRevocationReason, RuntimeValidationError,
    RuntimeValidationFailure, ToolSessionId,
};
use oxideterm_workspace::{TabId, TerminalSessionId};

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
pub struct AiSftpRuntimeOwner<N> {
    pub node_id: N,
    pub connection_id: String,
    pub session_generation: u64,
}

impl<N> std::fmt::Debug for AiSftpRuntimeOwner<N> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The proof is internal dispatch authority, so diagnostics expose only its class.
        formatter
            .debug_struct("AiSftpRuntimeOwner")
            .field("owner", &"[redacted]")
            .finish()
    }
}

#[derive(Clone)]
struct IdeRuntimeOwner<N> {
    key: RuntimeOwnerKey,
    generation: RuntimeOwnerGeneration,
    node_id: N,
}

#[derive(Clone)]
struct AppSurfaceRuntimeOwner {
    key: RuntimeOwnerKey,
    generation: RuntimeOwnerGeneration,
}

/// Owns AI tool-session authority without taking ownership of terminal or transport runtime.
pub struct AiRuntimeAuthority<N> {
    registry: RuntimeCapabilityRegistry,
    tool_sessions: HashMap<u64, ToolSessionId>,
    terminal_owners: HashMap<TerminalSessionId, TerminalRuntimeOwner>,
    node_owners: HashMap<N, NodeRuntimeOwner>,
    sftp_owners: HashMap<N, SftpRuntimeOwner>,
    ide_owners: HashMap<u64, IdeRuntimeOwner<N>>,
    app_surface_owners: HashMap<u64, AppSurfaceRuntimeOwner>,
    local_shell_owner: LocalShellRuntimeOwner,
    accepting_broker_requests: bool,
}

impl<N> std::fmt::Debug for AiRuntimeAuthority<N> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Runtime owner identities must never leak through diagnostic formatting.
        formatter
            .debug_struct("AiRuntimeAuthority")
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

impl<N: Clone + Eq + std::hash::Hash> AiRuntimeAuthority<N> {
    pub fn new() -> Self {
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
                crate::StableResourceRef::new(
                    crate::StableResourceKind::LocalShellProfile,
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

    /// Authority belongs to one run. Replacing that run cannot revoke another agent's lease.
    pub fn begin_tool_session(&mut self, stream_generation: u64) -> ToolSessionId {
        self.finish_tool_session(
            stream_generation,
            RuntimeRevocationReason::ToolSessionCancelled,
        );
        let tool_session_id = self.registry.begin_tool_session();
        self.tool_sessions
            .insert(stream_generation, tool_session_id.clone());
        tool_session_id
    }

    pub fn finish_tool_session(&mut self, stream_generation: u64, reason: RuntimeRevocationReason) {
        if let Some(tool_session_id) = self.tool_sessions.remove(&stream_generation) {
            self.registry.finish_tool_session(&tool_session_id, reason);
        }
    }

    pub fn is_agent_scoped_session(&self, session: &ToolSessionId) -> bool {
        self.registry.is_restricted_session(session)
    }

    pub fn agent_scope(
        &self,
        session: &ToolSessionId,
        tools: std::collections::BTreeSet<String>,
    ) -> crate::agent::AgentScope {
        crate::agent::AgentScope {
            targets: self.registry.session_owners(session),
            tools,
        }
    }

    pub fn restrict_agent_session(
        &mut self,
        session: &ToolSessionId,
        scope: &crate::agent::AgentScope,
    ) -> Result<(), crate::RuntimeContextError> {
        self.registry
            .restrict_tool_session(session, scope.targets.clone())
    }

    pub fn delegated_agent_target(
        &self,
        session: &ToolSessionId,
        handle: &str,
    ) -> Result<(crate::RuntimeOwnerKey, String), crate::RuntimeValidationError> {
        let handle = crate::RuntimeHandleId::parse(handle.to_owned()).map_err(|_| {
            crate::RuntimeValidationError::new(crate::RuntimeValidationFailure::UnknownHandle)
        })?;
        let label = self
            .registry
            .validate_handle_projection(session, Some(&handle))?
            .label;
        Ok((self.registry.delegated_owner(session, &handle)?, label))
    }

    pub fn terminal_resource_key(
        &self,
        session: TerminalSessionId,
    ) -> Option<crate::RuntimeOwnerKey> {
        self.terminal_owners
            .get(&session)
            .map(|owner| owner.key.clone())
    }

    pub fn agent_can_observe_terminal(
        &self,
        session: &ToolSessionId,
        terminal: TerminalSessionId,
    ) -> bool {
        self.terminal_owners
            .get(&terminal)
            .is_some_and(|owner| self.registry.permits_owner(session, &owner.key))
    }

    /// Rejects work queued by a stream that has been cancelled or replaced.
    pub fn is_active_tool_session(
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
    pub fn registry_epoch(&self) -> crate::RuntimeRegistryEpoch {
        self.registry.epoch().clone()
    }

    /// Registers the terminal session itself rather than its tab or pane mount.
    /// Detach/return may change presentation ownership, but must not replace the
    /// physical terminal capability.
    pub fn register_terminal_session(&mut self, session_id: TerminalSessionId, label: String) {
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
    pub fn register_node_connection(
        &mut self,
        node_id: N,
        connection_id: String,
        label: String,
        resource_ref: Option<crate::StableResourceRef>,
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
    pub fn register_sftp_session(
        &mut self,
        node_id: N,
        connection_id: String,
        session_generation: u64,
        label: String,
        resource_ref: Option<crate::StableResourceRef>,
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
    pub fn register_ide_surface(
        &mut self,
        tab_id: TabId,
        node_id: N,
        label: String,
        resource_ref: Option<crate::StableResourceRef>,
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
    pub fn revoke_ide_surface(&mut self, tab_id: TabId) {
        let Some(owner) = self.ide_owners.remove(&tab_id.0) else {
            return;
        };
        self.registry
            .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
    }

    /// A mounted tab is the exact owner of focus authority. Reopening the same
    /// surface kind creates another owner instead of rebinding a stale handle.
    pub fn register_app_surface(
        &mut self,
        tab_id: TabId,
        label: String,
        resource_ref: Option<crate::StableResourceRef>,
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

    pub fn revoke_app_surface(&mut self, tab_id: TabId) {
        let Some(owner) = self.app_surface_owners.remove(&tab_id.0) else {
            return;
        };
        self.registry
            .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
    }

    /// Physical-node teardown invalidates the node and every AI owner whose
    /// backend depends on that connection. Terminal owners remain independent.
    pub fn revoke_node_connection(&mut self, node_id: &N) {
        if let Some(owner) = self.node_owners.remove(node_id) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
        }
        self.revoke_sftp_session(node_id);
        self.revoke_ide_surfaces_for_node(node_id);
    }

    /// An SFTP subsystem reset cannot revoke the node or its terminal owners.
    pub fn revoke_sftp_session(&mut self, node_id: &N) {
        if let Some(owner) = self.sftp_owners.remove(node_id) {
            self.registry
                .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
        }
    }

    fn revoke_ide_surfaces_for_node(&mut self, node_id: &N) {
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
    pub fn revoke_terminal_session(&mut self, session_id: TerminalSessionId) {
        let Some(owner) = self.terminal_owners.remove(&session_id) else {
            return;
        };
        self.registry
            .revoke_owner(&owner.key, RuntimeRevocationReason::OwnerClosed);
    }

    pub fn issue_terminal_handle(
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

    pub fn issue_local_shell_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        self.registry
            .issue_handle(tool_session_id, &self.local_shell_owner.key)
    }

    pub fn issue_node_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
        node_id: &N,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        // Node inspection is intentionally distinct from terminal command
        // authority even when both owners share one physical connection.
        let owner = self
            .node_owners
            .get(node_id)
            .ok_or(RuntimeContextError::OwnerNotFound)?;
        self.registry.issue_handle(tool_session_id, &owner.key)
    }

    pub fn issue_sftp_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
        node_id: &N,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        let owner = self
            .sftp_owners
            .get(node_id)
            .ok_or(RuntimeContextError::OwnerNotFound)?;
        self.registry.issue_handle(tool_session_id, &owner.key)
    }

    pub fn issue_ide_handle(
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

    pub fn issue_app_surface_handle(
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
    pub fn current_handle_projections(
        &self,
        tool_session_id: &ToolSessionId,
    ) -> Vec<RuntimeHandleProjection> {
        self.registry.issued_handles_for_session(tool_session_id)
    }

    /// Parses and validates a model-submitted handle immediately before routing
    /// to a terminal session. The resulting session id never enters model output.
    pub fn validate_terminal_handle(
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

    /// Resolves node inspection authority without granting terminal or SFTP capabilities.
    pub fn validate_node_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
    ) -> Result<N, RuntimeValidationError> {
        let validated = self.validate_handle_for_kind(
            tool_session_id,
            raw_handle_id,
            RuntimeCapability::NodeInspect,
            RuntimeOwnerKind::SshNode,
        )?;
        self.node_owners
            .iter()
            .find_map(|(node_id, owner)| {
                (owner.key == *validated.owner_key()
                    && owner.generation == validated.owner_generation())
                .then(|| node_id.clone())
            })
            .ok_or_else(|| RuntimeValidationError::new(RuntimeValidationFailure::OwnerClosed))
    }

    /// Validates run-command authority without permitting a terminal handle to
    /// be reinterpreted as the app-wide local shell service, or vice versa.
    pub fn validate_run_command_handle(
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

    pub fn validate_sftp_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
        capability: RuntimeCapability,
    ) -> Result<AiSftpRuntimeOwner<N>, RuntimeValidationError> {
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

    pub fn validate_ide_handle(
        &self,
        tool_session_id: &ToolSessionId,
        raw_handle_id: Option<&str>,
        capability: RuntimeCapability,
    ) -> Result<(TabId, N), RuntimeValidationError> {
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

    pub fn validate_app_surface_handle(
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

    pub fn validate_state_handle(
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
    ) -> Result<crate::ValidatedRuntimeHandle, RuntimeValidationError> {
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

    /// Validates typed stable references or opaque live handles without
    /// translating either authority form back into a legacy target identifier.
    pub fn validate_tool_authority(
        &self,
        tool_session_id: &ToolSessionId,
        tool_name: &str,
        args: &serde_json::Value,
        saved_connection_exists: impl Fn(&str) -> bool,
        connected_saved_node: impl Fn(&str) -> bool,
    ) -> Result<(), RuntimeValidationError> {
        if self.is_agent_scoped_session(tool_session_id)
            && (args.get("resource_ref").is_some()
                || (tool_name == "get_state"
                    && args.get("scope").and_then(serde_json::Value::as_str) != Some("target")))
        {
            return Err(crate::RuntimeValidationError::new(
                crate::RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
        if ai_rejects_legacy_live_target_argument(tool_name, args) {
            return Err(crate::RuntimeValidationError::new(
                crate::RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }
        if matches!(tool_name, "read_resource" | "write_resource") {
            if args.get("resource_ref").is_some() {
                ai_stable_resource_operation(tool_name, args)?;
            } else {
                let operation = ai_live_resource_operation(tool_name, args)?;
                let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
                if operation.requires_ide_owner() {
                    self.validate_ide_handle(
                        tool_session_id,
                        raw_handle_id,
                        operation.capability(),
                    )?;
                } else {
                    self.validate_sftp_handle(
                        tool_session_id,
                        raw_handle_id,
                        operation.capability(),
                    )?;
                }
            }
            return Ok(());
        }
        if tool_name == "transfer_resource" {
            let operation = ai_live_resource_operation(tool_name, args)?;
            let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
            self.validate_sftp_handle(tool_session_id, raw_handle_id, operation.capability())?;
            return Ok(());
        }
        if tool_name == "open_app_surface" {
            if args.get("handle_id").is_some() {
                self.validate_app_surface_handle(
                    tool_session_id,
                    args.get("handle_id").and_then(serde_json::Value::as_str),
                )?;
            } else {
                ai_stable_resource_operation(tool_name, args)?;
            }
            return Ok(());
        }
        if tool_name == "connect_target" {
            let operation = ai_stable_resource_operation(tool_name, args)?;
            let AiStableResourceOperation::SavedConnection(resource_ref) = operation else {
                return Err(crate::RuntimeValidationError::new(
                    crate::RuntimeValidationFailure::CapabilityUnavailable,
                ));
            };
            if !saved_connection_exists(resource_ref.id()) {
                return Err(crate::RuntimeValidationError::new(
                    crate::RuntimeValidationFailure::CapabilityUnavailable,
                ));
            }
            return Ok(());
        }
        if tool_name == "get_state"
            && args.get("scope").and_then(serde_json::Value::as_str) == Some("target")
        {
            if args.get("handle_id").is_some() {
                self.validate_state_handle(
                    tool_session_id,
                    args.get("handle_id").and_then(serde_json::Value::as_str),
                )?;
            } else {
                let resource_ref = args
                    .get("resource_ref")
                    .cloned()
                    .and_then(|value| {
                        serde_json::from_value::<crate::StableResourceRef>(value).ok()
                    })
                    .ok_or_else(|| {
                        crate::RuntimeValidationError::new(
                            crate::RuntimeValidationFailure::CapabilityUnavailable,
                        )
                    })?;
                if resource_ref.kind() == crate::StableResourceKind::SavedConnection
                    && !saved_connection_exists(resource_ref.id())
                {
                    return Err(crate::RuntimeValidationError::new(
                        crate::RuntimeValidationFailure::CapabilityUnavailable,
                    ));
                }
            }
            return Ok(());
        }
        if matches!(
            tool_name,
            "inspect_host_tools" | "control_host_tool" | "list_forwards" | "manage_forward"
        ) {
            if let Some(handle) = args.get("handle_id").and_then(serde_json::Value::as_str) {
                self.validate_node_handle(tool_session_id, Some(handle))?;
            } else {
                let id = args
                    .pointer("/resource_ref/id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| {
                        RuntimeValidationError::new(RuntimeValidationFailure::CapabilityUnavailable)
                    })?;
                if !connected_saved_node(id) {
                    return Err(RuntimeValidationError::new(
                        RuntimeValidationFailure::OwnerClosed,
                    ));
                }
            }
            return Ok(());
        }
        let raw_handle_id = args.get("handle_id").and_then(serde_json::Value::as_str);
        match tool_name {
            "run_command" => {
                self.validate_run_command_handle(tool_session_id, raw_handle_id)?;
            }
            "observe_terminal" | "get_transport_session_state" => {
                self.validate_terminal_handle(
                    tool_session_id,
                    raw_handle_id,
                    crate::RuntimeCapability::TerminalObserve,
                )?;
            }
            "send_terminal_input" | "manage_serial_session" | "manage_telnet_session" => {
                self.validate_terminal_handle(
                    tool_session_id,
                    raw_handle_id,
                    crate::RuntimeCapability::TerminalSendInput,
                )?;
            }
            "wait_terminal_output" | "get_terminal_command_status" => {
                self.validate_terminal_handle(
                    tool_session_id,
                    raw_handle_id,
                    crate::RuntimeCapability::TerminalObserve,
                )?;
            }
            _ => {}
        }
        Ok(())
    }

    fn finish_all_tool_sessions(&mut self, reason: RuntimeRevocationReason) {
        for tool_session_id in self.tool_sessions.drain().map(|(_, session)| session) {
            self.registry.finish_tool_session(&tool_session_id, reason);
        }
    }

    /// Begins application shutdown before UI interaction owners are released.
    pub fn stop_accepting_and_finish_tool_sessions(&mut self) {
        if !self.accepting_broker_requests {
            return;
        }
        self.accepting_broker_requests = false;
        self.finish_all_tool_sessions(RuntimeRevocationReason::ApplicationShutdown);
    }

    /// Revokes owner projections after pending approval and selection waiters
    /// have been cancelled by the AI workspace entity.
    pub fn revoke_registered_owner_projections(&mut self) {
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

    pub fn shutdown(&mut self) {
        self.stop_accepting_and_finish_tool_sessions();
        self.revoke_registered_owner_projections();
    }
}

fn next_owner_generation(current: RuntimeOwnerGeneration) -> RuntimeOwnerGeneration {
    RuntimeOwnerGeneration::new(current.value().saturating_add(1))
}

impl<N: Clone + Eq + std::hash::Hash> Default for AiRuntimeAuthority<N> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiRunCommandOwner {
    Terminal(TerminalSessionId),
    LocalShell,
}

#[cfg(test)]
mod tests;
