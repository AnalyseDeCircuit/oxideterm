use std::collections::{BTreeSet, HashMap, HashSet};

use super::{
    RuntimeCapability, RuntimeContextError, RuntimeHandleId, RuntimeHandleProjection,
    RuntimeOwnerGeneration, RuntimeOwnerKey, RuntimeOwnerKind, RuntimeRegistryEpoch,
    RuntimeRevocationReason, RuntimeValidationError, RuntimeValidationFailure, StableResourceRef,
    ToolSessionId,
};

const MAX_HANDLES_PER_OWNER_PER_TOOL_SESSION: usize = 1;
const MAX_HANDLES_PER_TOOL_SESSION: usize = 128;

/// The safe data needed to register a real application runtime owner.
#[derive(Clone, Debug)]
pub struct RuntimeOwnerRegistration {
    pub key: RuntimeOwnerKey,
    pub kind: RuntimeOwnerKind,
    pub generation: RuntimeOwnerGeneration,
    pub label: String,
    pub capabilities: BTreeSet<RuntimeCapability>,
    pub resource_ref: Option<StableResourceRef>,
}

impl RuntimeOwnerRegistration {
    pub fn new(
        key: RuntimeOwnerKey,
        kind: RuntimeOwnerKind,
        generation: RuntimeOwnerGeneration,
        label: String,
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        resource_ref: Option<StableResourceRef>,
    ) -> Result<Self, RuntimeContextError> {
        if label.trim().is_empty()
            || label.chars().count() > 256
            || label.chars().any(char::is_control)
        {
            return Err(RuntimeContextError::InvalidOwnerRegistration);
        }

        let capabilities = capabilities.into_iter().collect::<BTreeSet<_>>();
        if capabilities.is_empty() || !capabilities_match_owner(kind, &capabilities) {
            return Err(RuntimeContextError::InvalidOwnerRegistration);
        }

        Ok(Self {
            key,
            kind,
            generation,
            label,
            capabilities,
            resource_ref,
        })
    }
}

/// A validated lease that app adapters use immediately before dispatching to the real owner.
#[derive(Clone)]
pub struct ValidatedRuntimeHandle {
    owner_key: RuntimeOwnerKey,
    owner_generation: RuntimeOwnerGeneration,
    owner_kind: RuntimeOwnerKind,
    capability: RuntimeCapability,
    resource_ref: Option<StableResourceRef>,
}

impl ValidatedRuntimeHandle {
    pub fn owner_key(&self) -> &RuntimeOwnerKey {
        &self.owner_key
    }

    pub const fn owner_generation(&self) -> RuntimeOwnerGeneration {
        self.owner_generation
    }

    pub const fn owner_kind(&self) -> RuntimeOwnerKind {
        self.owner_kind
    }

    pub const fn capability(&self) -> RuntimeCapability {
        self.capability
    }

    pub fn resource_ref(&self) -> Option<&StableResourceRef> {
        self.resource_ref.as_ref()
    }
}

impl std::fmt::Debug for ValidatedRuntimeHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ValidatedRuntimeHandle")
            .field("owner_key", &"[redacted]")
            .field("owner_generation", &self.owner_generation)
            .field("owner_kind", &self.owner_kind)
            .field("capability", &self.capability)
            .finish()
    }
}

/// Pure, in-memory authority state for application runtime resources.
#[derive(Debug)]
pub struct RuntimeCapabilityRegistry {
    epoch: RuntimeRegistryEpoch,
    active_sessions: HashSet<ToolSessionId>,
    owners: HashMap<RuntimeOwnerKey, RuntimeOwnerRecord>,
    handles: HashMap<RuntimeHandleId, RuntimeHandleRecord>,
}

#[derive(Clone, Debug)]
struct RuntimeOwnerRecord {
    kind: RuntimeOwnerKind,
    generation: RuntimeOwnerGeneration,
    label: String,
    capabilities: BTreeSet<RuntimeCapability>,
    resource_ref: Option<StableResourceRef>,
}

#[derive(Clone, Debug)]
struct RuntimeHandleRecord {
    tool_session_id: ToolSessionId,
    owner_key: RuntimeOwnerKey,
    owner_generation: RuntimeOwnerGeneration,
    kind: RuntimeOwnerKind,
    label: String,
    capabilities: BTreeSet<RuntimeCapability>,
    resource_ref: Option<StableResourceRef>,
    state: RuntimeHandleState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeHandleState {
    Active,
    Revoked(RuntimeRevocationReason),
}

impl RuntimeCapabilityRegistry {
    pub fn new() -> Self {
        Self {
            epoch: RuntimeRegistryEpoch::new(),
            active_sessions: HashSet::new(),
            owners: HashMap::new(),
            handles: HashMap::new(),
        }
    }

    pub fn epoch(&self) -> &RuntimeRegistryEpoch {
        &self.epoch
    }

    pub fn begin_tool_session(&mut self) -> ToolSessionId {
        let session_id = ToolSessionId::new();
        self.active_sessions.insert(session_id.clone());
        session_id
    }

    /// The UI bridge uses this before dispatching work that crossed from an async model task.
    pub fn is_tool_session_active(&self, tool_session_id: &ToolSessionId) -> bool {
        self.active_sessions.contains(tool_session_id)
    }

    /// Revokes and discards every handle for a completed or cancelled model turn.
    pub fn finish_tool_session(
        &mut self,
        tool_session_id: &ToolSessionId,
        _reason: RuntimeRevocationReason,
    ) {
        self.active_sessions.remove(tool_session_id);
        self.handles
            .retain(|_, handle| handle.tool_session_id != *tool_session_id);
    }

    pub fn register_owner(
        &mut self,
        registration: RuntimeOwnerRegistration,
    ) -> Result<(), RuntimeContextError> {
        if let Some(existing) = self.owners.get(&registration.key) {
            if registration.generation < existing.generation {
                return Err(RuntimeContextError::OwnerGenerationRegression);
            }
            if registration.generation == existing.generation
                && (registration.kind != existing.kind
                    || registration.resource_ref != existing.resource_ref
                    || registration.capabilities != existing.capabilities)
            {
                return Err(RuntimeContextError::OwnerIdentityChangedWithoutGeneration);
            }
        }

        let replaced = self
            .owners
            .get(&registration.key)
            .is_some_and(|existing| registration.generation > existing.generation);
        if replaced {
            self.revoke_owner_handles(&registration.key, RuntimeRevocationReason::OwnerReplaced);
        }

        let label_changed = self
            .owners
            .get(&registration.key)
            .is_some_and(|existing| existing.label != registration.label);
        let owner_key = registration.key.clone();
        let owner_generation = registration.generation;
        let owner_label = registration.label.clone();
        self.owners.insert(
            owner_key.clone(),
            RuntimeOwnerRecord {
                kind: registration.kind,
                generation: registration.generation,
                label: registration.label,
                capabilities: registration.capabilities,
                resource_ref: registration.resource_ref,
            },
        );
        if label_changed {
            // A label is presentation metadata, so it may refresh without extending a lease.
            for handle in self.handles.values_mut() {
                if handle.owner_key == owner_key
                    && handle.owner_generation == owner_generation
                    && handle.state == RuntimeHandleState::Active
                {
                    handle.label = owner_label.clone();
                }
            }
        }
        Ok(())
    }

    /// Revocation is independent from pane ownership and must be driven by the real owner event.
    pub fn revoke_owner(&mut self, owner_key: &RuntimeOwnerKey, reason: RuntimeRevocationReason) {
        self.owners.remove(owner_key);
        self.revoke_owner_handles(owner_key, reason);
    }

    pub fn issue_handle(
        &mut self,
        tool_session_id: &ToolSessionId,
        owner_key: &RuntimeOwnerKey,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        if !self.active_sessions.contains(tool_session_id) {
            return Err(RuntimeContextError::ToolSessionInactive);
        }
        let owner = self
            .owners
            .get(owner_key)
            .cloned()
            .ok_or(RuntimeContextError::OwnerNotFound)?;

        if let Some(existing) = self.handles.iter().find_map(|(handle_id, handle)| {
            (handle.tool_session_id == *tool_session_id
                && handle.owner_key == *owner_key
                && handle.owner_generation == owner.generation
                && handle.state == RuntimeHandleState::Active)
                .then(|| handle_id.clone())
        }) {
            return self.projection_for_handle(&existing);
        }

        let current_count = self
            .handles
            .values()
            .filter(|handle| {
                handle.tool_session_id == *tool_session_id
                    && handle.owner_key == *owner_key
                    && handle.state == RuntimeHandleState::Active
            })
            .count();
        if current_count >= MAX_HANDLES_PER_OWNER_PER_TOOL_SESSION {
            return Err(RuntimeContextError::HandleAllocationLimitReached);
        }
        let session_handle_count = self
            .handles
            .values()
            .filter(|handle| {
                handle.tool_session_id == *tool_session_id
                    && handle.state == RuntimeHandleState::Active
            })
            .count();
        if session_handle_count >= MAX_HANDLES_PER_TOOL_SESSION {
            return Err(RuntimeContextError::HandleAllocationLimitReached);
        }

        let handle_id = RuntimeHandleId::new();
        self.handles.insert(
            handle_id.clone(),
            RuntimeHandleRecord {
                tool_session_id: tool_session_id.clone(),
                owner_key: owner_key.clone(),
                owner_generation: owner.generation,
                kind: owner.kind,
                label: owner.label,
                capabilities: owner.capabilities,
                resource_ref: owner.resource_ref,
                state: RuntimeHandleState::Active,
            },
        );
        self.projection_for_handle(&handle_id)
    }

    pub fn validate_handle(
        &self,
        tool_session_id: &ToolSessionId,
        handle_id: Option<&RuntimeHandleId>,
        required_capability: RuntimeCapability,
    ) -> Result<ValidatedRuntimeHandle, RuntimeValidationError> {
        let Some(handle_id) = handle_id else {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::MissingHandle,
            ));
        };
        if !self.active_sessions.contains(tool_session_id) {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::ToolSessionInactive,
            ));
        }
        let Some(handle) = self.handles.get(handle_id) else {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::UnknownHandle,
            ));
        };
        if handle.tool_session_id != *tool_session_id {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::WrongToolSession,
            ));
        }
        match handle.state {
            RuntimeHandleState::Revoked(RuntimeRevocationReason::OwnerClosed)
            | RuntimeHandleState::Revoked(RuntimeRevocationReason::ApplicationShutdown) => {
                return Err(RuntimeValidationError::new(
                    RuntimeValidationFailure::OwnerClosed,
                ));
            }
            RuntimeHandleState::Revoked(RuntimeRevocationReason::OwnerReplaced) => {
                return Err(RuntimeValidationError::new(
                    RuntimeValidationFailure::OwnerReplaced,
                ));
            }
            RuntimeHandleState::Revoked(_) => {
                return Err(RuntimeValidationError::new(
                    RuntimeValidationFailure::ToolSessionInactive,
                ));
            }
            RuntimeHandleState::Active => {}
        }
        let Some(owner) = self.owners.get(&handle.owner_key) else {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::OwnerClosed,
            ));
        };
        if owner.generation != handle.owner_generation {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::OwnerReplaced,
            ));
        }
        if !handle.capabilities.contains(&required_capability)
            || !owner.capabilities.contains(&required_capability)
        {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::CapabilityUnavailable,
            ));
        }

        Ok(ValidatedRuntimeHandle {
            owner_key: handle.owner_key.clone(),
            owner_generation: handle.owner_generation,
            owner_kind: handle.kind,
            capability: required_capability,
            resource_ref: handle.resource_ref.clone(),
        })
    }

    /// Validates a handle for a safe state projection without widening the
    /// capabilities stored in its backend record.
    pub fn validate_handle_projection(
        &self,
        tool_session_id: &ToolSessionId,
        handle_id: Option<&RuntimeHandleId>,
    ) -> Result<RuntimeHandleProjection, RuntimeValidationError> {
        const STATE_READ_CAPABILITIES: &[RuntimeCapability] = &[
            RuntimeCapability::TerminalObserve,
            RuntimeCapability::LocalShellRunCommand,
            RuntimeCapability::NodeInspect,
            RuntimeCapability::SftpRead,
            RuntimeCapability::IdeRead,
            RuntimeCapability::SurfaceFocus,
        ];
        let Some(handle_id) = handle_id else {
            return Err(RuntimeValidationError::new(
                RuntimeValidationFailure::MissingHandle,
            ));
        };
        for capability in STATE_READ_CAPABILITIES {
            match self.validate_handle(tool_session_id, Some(handle_id), *capability) {
                Ok(_) => {
                    return self.projection_for_handle(handle_id).map_err(|_| {
                        RuntimeValidationError::new(RuntimeValidationFailure::OwnerClosed)
                    });
                }
                Err(error)
                    if error.failure() == RuntimeValidationFailure::CapabilityUnavailable => {}
                Err(error) => return Err(error),
            }
        }
        Err(RuntimeValidationError::new(
            RuntimeValidationFailure::CapabilityUnavailable,
        ))
    }

    pub fn handles_for_session(
        &mut self,
        tool_session_id: &ToolSessionId,
    ) -> Result<Vec<RuntimeHandleProjection>, RuntimeContextError> {
        let mut owner_keys = self
            .owners
            .iter()
            .map(|(key, owner)| {
                (
                    owner.label.clone(),
                    format!("{:?}", owner.kind),
                    key.clone(),
                )
            })
            .collect::<Vec<_>>();
        owner_keys.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        if owner_keys.len() > MAX_HANDLES_PER_TOOL_SESSION {
            return Err(RuntimeContextError::HandleAllocationLimitReached);
        }
        let mut handles = owner_keys
            .iter()
            .map(|(_, _, owner_key)| self.issue_handle(tool_session_id, owner_key))
            .collect::<Result<Vec<_>, _>>()?;
        handles.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
        });
        Ok(handles)
    }

    /// Returns only leases already issued by authoritative discovery. This
    /// cannot allocate extra handles or turn an unprojected owner into authority.
    pub fn issued_handles_for_session(
        &self,
        tool_session_id: &ToolSessionId,
    ) -> Vec<RuntimeHandleProjection> {
        let mut handles = self
            .handles
            .iter()
            .filter(|(_, handle)| {
                handle.tool_session_id == *tool_session_id
                    && handle.state == RuntimeHandleState::Active
            })
            .filter_map(|(handle_id, _)| self.projection_for_handle(handle_id).ok())
            .collect::<Vec<_>>();
        handles.sort_by(|left, right| {
            left.label
                .cmp(&right.label)
                .then_with(|| format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind)))
        });
        handles
    }

    fn projection_for_handle(
        &self,
        handle_id: &RuntimeHandleId,
    ) -> Result<RuntimeHandleProjection, RuntimeContextError> {
        let handle = self
            .handles
            .get(handle_id)
            .ok_or(RuntimeContextError::OwnerNotFound)?;
        Ok(RuntimeHandleProjection {
            handle_id: handle_id.clone(),
            kind: handle.kind,
            label: handle.label.clone(),
            capabilities: handle.capabilities.iter().copied().collect(),
            resource_ref: handle.resource_ref.clone(),
        })
    }

    fn revoke_owner_handles(
        &mut self,
        owner_key: &RuntimeOwnerKey,
        reason: RuntimeRevocationReason,
    ) {
        for handle in self.handles.values_mut() {
            if handle.owner_key == *owner_key {
                handle.state = RuntimeHandleState::Revoked(reason);
            }
        }
    }
}

impl Default for RuntimeCapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

fn capabilities_match_owner(
    owner_kind: RuntimeOwnerKind,
    capabilities: &BTreeSet<RuntimeCapability>,
) -> bool {
    capabilities.iter().all(|capability| match owner_kind {
        RuntimeOwnerKind::LocalShell => *capability == RuntimeCapability::LocalShellRunCommand,
        RuntimeOwnerKind::Terminal => matches!(
            capability,
            RuntimeCapability::TerminalObserve
                | RuntimeCapability::TerminalRunCommand
                | RuntimeCapability::TerminalSendInput
        ),
        RuntimeOwnerKind::SshNode => *capability == RuntimeCapability::NodeInspect,
        RuntimeOwnerKind::SftpSession => matches!(
            capability,
            RuntimeCapability::SftpRead
                | RuntimeCapability::SftpWrite
                | RuntimeCapability::SftpStartTransfer
        ),
        RuntimeOwnerKind::IdeSurface => matches!(
            capability,
            RuntimeCapability::IdeRead | RuntimeCapability::IdeWrite
        ),
        RuntimeOwnerKind::AppSurface => *capability == RuntimeCapability::SurfaceFocus,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{StableResourceKind, StableResourceRef};

    fn terminal_registration(key: RuntimeOwnerKey, generation: u64) -> RuntimeOwnerRegistration {
        RuntimeOwnerRegistration::new(
            key,
            RuntimeOwnerKind::Terminal,
            RuntimeOwnerGeneration::new(generation),
            "SSH terminal".to_string(),
            [
                RuntimeCapability::TerminalObserve,
                RuntimeCapability::TerminalRunCommand,
                RuntimeCapability::TerminalSendInput,
            ],
            Some(
                StableResourceRef::new(
                    StableResourceKind::SavedConnection,
                    "4e22e673-067e-46e2-8b9f-902d7b21af4c".to_string(),
                    Some("Production".to_string()),
                )
                .expect("valid saved connection"),
            ),
        )
        .expect("valid terminal registration")
    }

    #[test]
    fn fabricated_handle_is_rejected() {
        let mut registry = RuntimeCapabilityRegistry::new();
        let session = registry.begin_tool_session();
        let fabricated = RuntimeHandleId::new();

        let error = registry
            .validate_handle(
                &session,
                Some(&fabricated),
                RuntimeCapability::TerminalObserve,
            )
            .expect_err("fabricated handle is rejected");

        assert_eq!(error.failure(), RuntimeValidationFailure::UnknownHandle);
        assert_eq!(error.public_code(), "runtime_handle_expired");
    }

    #[test]
    fn handle_from_another_tool_session_is_rejected_without_oracle() {
        let mut registry = RuntimeCapabilityRegistry::new();
        let owner_key = RuntimeOwnerKey::new();
        registry
            .register_owner(terminal_registration(owner_key.clone(), 1))
            .expect("owner registers");
        let first_session = registry.begin_tool_session();
        let second_session = registry.begin_tool_session();
        let handle = registry
            .issue_handle(&first_session, &owner_key)
            .expect("handle issues");

        let error = registry
            .validate_handle(
                &second_session,
                Some(&handle.handle_id),
                RuntimeCapability::TerminalObserve,
            )
            .expect_err("cross-session handle is rejected");

        assert_eq!(error.failure(), RuntimeValidationFailure::WrongToolSession);
        assert_eq!(error.public_code(), "runtime_handle_expired");
    }

    #[test]
    fn finishing_tool_session_discards_its_handles() {
        let mut registry = RuntimeCapabilityRegistry::new();
        let owner_key = RuntimeOwnerKey::new();
        registry
            .register_owner(terminal_registration(owner_key.clone(), 1))
            .expect("owner registers");
        let session = registry.begin_tool_session();
        let handle = registry
            .issue_handle(&session, &owner_key)
            .expect("handle issues");

        registry.finish_tool_session(&session, RuntimeRevocationReason::ToolSessionFinished);
        let error = registry
            .validate_handle(
                &session,
                Some(&handle.handle_id),
                RuntimeCapability::TerminalObserve,
            )
            .expect_err("finished session cannot execute");

        assert_eq!(error.public_code(), "runtime_handle_expired");
    }

    #[test]
    fn replaced_owner_generation_revokes_old_handle() {
        let mut registry = RuntimeCapabilityRegistry::new();
        let owner_key = RuntimeOwnerKey::new();
        registry
            .register_owner(terminal_registration(owner_key.clone(), 1))
            .expect("owner registers");
        let session = registry.begin_tool_session();
        let handle = registry
            .issue_handle(&session, &owner_key)
            .expect("handle issues");

        registry
            .register_owner(terminal_registration(owner_key, 2))
            .expect("replacement owner registers");
        let error = registry
            .validate_handle(
                &session,
                Some(&handle.handle_id),
                RuntimeCapability::TerminalObserve,
            )
            .expect_err("old generation is revoked");

        assert_eq!(error.failure(), RuntimeValidationFailure::OwnerReplaced);
    }

    #[test]
    fn capability_change_requires_a_new_owner_generation() {
        let mut registry = RuntimeCapabilityRegistry::new();
        let owner_key = RuntimeOwnerKey::new();
        registry
            .register_owner(terminal_registration(owner_key.clone(), 1))
            .expect("owner registers");
        let update = registry.register_owner(
            RuntimeOwnerRegistration::new(
                owner_key,
                RuntimeOwnerKind::Terminal,
                RuntimeOwnerGeneration::new(1),
                "SSH terminal renamed".to_string(),
                [RuntimeCapability::TerminalObserve],
                Some(
                    StableResourceRef::new(
                        StableResourceKind::SavedConnection,
                        "4e22e673-067e-46e2-8b9f-902d7b21af4c".to_string(),
                        Some("Production renamed".to_string()),
                    )
                    .expect("valid stable reference"),
                ),
            )
            .expect("valid metadata update"),
        );

        assert_eq!(
            update,
            Err(RuntimeContextError::OwnerIdentityChangedWithoutGeneration)
        );
    }

    #[test]
    fn metadata_update_does_not_replace_owner_generation() {
        let mut registry = RuntimeCapabilityRegistry::new();
        let owner_key = RuntimeOwnerKey::new();
        registry
            .register_owner(terminal_registration(owner_key.clone(), 1))
            .expect("owner registers");
        let session = registry.begin_tool_session();
        let handle = registry
            .issue_handle(&session, &owner_key)
            .expect("handle issues");
        let mut renamed = terminal_registration(owner_key, 1);
        renamed.label = "Renamed terminal".to_string();

        registry
            .register_owner(renamed)
            .expect("presentation metadata may update in place");

        assert!(
            registry
                .validate_handle(
                    &session,
                    Some(&handle.handle_id),
                    RuntimeCapability::TerminalObserve,
                )
                .is_ok()
        );
        assert_eq!(
            registry
                .handles_for_session(&session)
                .expect("session handles")[0]
                .label,
            "Renamed terminal"
        );
    }

    #[test]
    fn submitted_capability_cannot_expand_backend_capabilities() {
        let mut registry = RuntimeCapabilityRegistry::new();
        let owner_key = RuntimeOwnerKey::new();
        let registration = RuntimeOwnerRegistration::new(
            owner_key.clone(),
            RuntimeOwnerKind::Terminal,
            RuntimeOwnerGeneration::new(1),
            "Read-only terminal".to_string(),
            [RuntimeCapability::TerminalObserve],
            None,
        )
        .expect("read-only terminal registration");
        registry
            .register_owner(registration)
            .expect("owner registers");
        let session = registry.begin_tool_session();
        let handle = registry
            .issue_handle(&session, &owner_key)
            .expect("handle issues");

        let error = registry
            .validate_handle(
                &session,
                Some(&handle.handle_id),
                RuntimeCapability::TerminalRunCommand,
            )
            .expect_err("a caller cannot add an ungranted capability");

        assert_eq!(
            error.failure(),
            RuntimeValidationFailure::CapabilityUnavailable
        );
    }

    #[test]
    fn ssh_node_cannot_claim_terminal_command_capability() {
        let registration = RuntimeOwnerRegistration::new(
            RuntimeOwnerKey::new(),
            RuntimeOwnerKind::SshNode,
            RuntimeOwnerGeneration::new(1),
            "Connection".to_string(),
            [RuntimeCapability::TerminalRunCommand],
            None,
        );

        assert!(matches!(
            registration,
            Err(RuntimeContextError::InvalidOwnerRegistration)
        ));
    }

    #[test]
    fn same_owner_reuses_one_handle_per_tool_session() {
        let mut registry = RuntimeCapabilityRegistry::new();
        let owner_key = RuntimeOwnerKey::new();
        registry
            .register_owner(terminal_registration(owner_key.clone(), 1))
            .expect("owner registers");
        let session = registry.begin_tool_session();

        let first = registry
            .issue_handle(&session, &owner_key)
            .expect("first handle issues");
        let second = registry
            .issue_handle(&session, &owner_key)
            .expect("second handle reuses first");

        assert_eq!(first.handle_id.as_str(), second.handle_id.as_str());
    }

    #[test]
    fn discovery_is_bounded_per_tool_session() {
        let mut registry = RuntimeCapabilityRegistry::new();
        for index in 0..=MAX_HANDLES_PER_TOOL_SESSION {
            registry
                .register_owner(
                    RuntimeOwnerRegistration::new(
                        RuntimeOwnerKey::new(),
                        RuntimeOwnerKind::Terminal,
                        RuntimeOwnerGeneration::new(1),
                        format!("Terminal {index:03}"),
                        [RuntimeCapability::TerminalObserve],
                        None,
                    )
                    .expect("valid terminal owner"),
                )
                .expect("owner registers");
        }
        let session = registry.begin_tool_session();

        assert!(matches!(
            registry.handles_for_session(&session),
            Err(RuntimeContextError::HandleAllocationLimitReached)
        ));
        assert!(registry.issued_handles_for_session(&session).is_empty());
    }
}
