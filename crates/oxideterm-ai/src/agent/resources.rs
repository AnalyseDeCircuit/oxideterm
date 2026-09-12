use super::{AgentError, AgentRunRef};
use crate::RuntimeOwnerKey;
use parking_lot::Mutex;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::watch;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AgentResourceLease {
    pub resource: RuntimeOwnerKey,
    pub owner: AgentRunRef,
    token: uuid::Uuid,
}

#[derive(Clone)]
pub struct AgentResourceCoordinator {
    state: Arc<Mutex<ResourceState>>,
    changed: watch::Sender<u64>,
    workspace: RuntimeOwnerKey,
}

#[derive(Default)]
struct ResourceState {
    leases: HashMap<RuntimeOwnerKey, AgentResourceLease>,
    epochs: HashMap<RuntimeOwnerKey, u64>,
    blocked: std::collections::HashSet<RuntimeOwnerKey>,
    blocked_owners: HashMap<RuntimeOwnerKey, AgentRunRef>,
}
impl Default for AgentResourceCoordinator {
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(ResourceState::default())),
            changed: watch::channel(0).0,
            workspace: RuntimeOwnerKey::new(),
        }
    }
}
impl AgentResourceCoordinator {
    pub fn has_unresolved(&self) -> bool {
        let state = self.state.lock();
        !state.leases.is_empty() || !state.blocked.is_empty()
    }

    pub fn unresolved_owned_by(
        &self,
        run: &AgentRunRef,
        include_running_command: bool,
    ) -> Vec<RuntimeOwnerKey> {
        let state = self.state.lock();
        state
            .blocked_owners
            .iter()
            .filter(|(_, owner)| *owner == run)
            .map(|(key, _)| key.clone())
            .chain(
                state
                    .leases
                    .values()
                    .filter(|lease| include_running_command && &lease.owner == run)
                    .map(|lease| lease.resource.clone()),
            )
            .collect()
    }
    pub fn workspace_resource(&self) -> RuntimeOwnerKey {
        self.workspace.clone()
    }
    pub fn is_blocked(&self, resource: &RuntimeOwnerKey) -> bool {
        self.state.lock().blocked.contains(resource)
    }
    pub fn has_owner(&self, resource: &RuntimeOwnerKey) -> bool {
        self.state.lock().leases.contains_key(resource)
    }
    pub fn owned_by(
        &self,
        resource: &RuntimeOwnerKey,
        owner: &AgentRunRef,
    ) -> Option<AgentResourceLease> {
        self.state
            .lock()
            .leases
            .get(resource)
            .filter(|lease| &lease.owner == owner)
            .cloned()
    }
    pub async fn acquire(
        &self,
        resource: RuntimeOwnerKey,
        owner: AgentRunRef,
        mut cancellation: watch::Receiver<bool>,
    ) -> Result<AgentResourceLease, AgentError> {
        let mut changes = self.changed.subscribe();
        let epoch = self
            .state
            .lock()
            .epochs
            .get(&resource)
            .copied()
            .unwrap_or_default();
        loop {
            if *cancellation.borrow() {
                return Err(AgentError::Cancelled);
            }
            {
                let mut state = self.state.lock();
                if state.blocked.contains(&resource)
                    || state.epochs.get(&resource).copied().unwrap_or_default() != epoch
                {
                    return Err(AgentError::ResourceUnresolved);
                }
                if !state.leases.contains_key(&resource) {
                    let lease = AgentResourceLease {
                        resource: resource.clone(),
                        owner: owner.clone(),
                        token: uuid::Uuid::new_v4(),
                    };
                    state.leases.insert(resource.clone(), lease.clone());
                    return Ok(lease);
                }
            }
            tokio::select! {
                _ = cancellation.changed() => return Err(AgentError::Cancelled),
                result = changes.changed() => { result.map_err(|_| AgentError::Cancelled)?; }
            }
        }
    }

    /// Only a confirmed command boundary releases ownership. A timed-out waiter must not call this.
    pub fn complete(&self, lease: &AgentResourceLease) -> bool {
        let mut state = self.state.lock();
        if state.leases.get(&lease.resource) != Some(lease) {
            return false;
        }
        state.leases.remove(&lease.resource);
        drop(state);
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        true
    }

    /// User takeover/disconnect blocks queued work as well as invalidating the current owner.
    pub fn invalidate(&self, resource: &RuntimeOwnerKey) -> Option<AgentRunRef> {
        let mut state = self.state.lock();
        state.blocked.insert(resource.clone());
        *state.epochs.entry(resource.clone()).or_default() += 1;
        let removed = state.leases.remove(resource);
        if let Some(lease) = &removed {
            state
                .blocked_owners
                .insert(resource.clone(), lease.owner.clone());
        }
        drop(state);
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
        removed.map(|lease| lease.owner)
    }

    /// The host calls this only after an explicit hand-back or a confirmed new connection.
    /// Requests queued before takeover still fail their epoch check.
    pub fn allow_new_requests(&self, resource: &RuntimeOwnerKey) {
        let mut state = self.state.lock();
        state.blocked.remove(resource);
        state.blocked_owners.remove(resource);
        drop(state);
        self.changed
            .send_modify(|revision| *revision = revision.wrapping_add(1));
    }

    pub fn owns(&self, lease: &AgentResourceLease) -> bool {
        self.state.lock().leases.get(&lease.resource) == Some(lease)
    }
}

struct ToolLeaseState {
    borrowed: bool,
    coordinator: AgentResourceCoordinator,
    lease: AgentResourceLease,
    dispatched: std::sync::atomic::AtomicBool,
    command_monitor: std::sync::atomic::AtomicBool,
    response_finished: std::sync::atomic::AtomicBool,
}

impl Drop for ToolLeaseState {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        if self.borrowed {
            if !self.response_finished.load(Ordering::Acquire)
                && self.dispatched.load(Ordering::Acquire)
                && self.coordinator.owns(&self.lease)
            {
                self.coordinator.invalidate(&self.lease.resource);
            }
            return;
        }
        if self.command_monitor.load(Ordering::Acquire) {
            return;
        }
        if self.dispatched.load(Ordering::Acquire) {
            // A dropped response is not proof that a remote mutation was undone.
            if self.coordinator.owns(&self.lease) {
                self.coordinator.invalidate(&self.lease.resource);
            }
        } else {
            self.coordinator.complete(&self.lease);
        }
    }
}

#[derive(Clone)]
pub struct AgentToolLease(Arc<ToolLeaseState>);

impl AgentToolLease {
    pub fn new(coordinator: AgentResourceCoordinator, lease: AgentResourceLease) -> Self {
        Self(Arc::new(ToolLeaseState {
            borrowed: false,
            coordinator,
            lease,
            dispatched: false.into(),
            command_monitor: false.into(),
            response_finished: false.into(),
        }))
    }
    /// Interactive input can continue this run's command without releasing its execution lease.
    pub fn borrow_command(
        coordinator: AgentResourceCoordinator,
        lease: AgentResourceLease,
    ) -> Self {
        Self(Arc::new(ToolLeaseState {
            borrowed: true,
            coordinator,
            lease,
            dispatched: false.into(),
            command_monitor: false.into(),
            response_finished: false.into(),
        }))
    }
    pub fn lease(&self) -> &AgentResourceLease {
        &self.0.lease
    }
    pub fn is_current(&self) -> bool {
        self.0.coordinator.owns(&self.0.lease)
    }
    pub fn dispatched(&self) {
        self.0
            .dispatched
            .store(true, std::sync::atomic::Ordering::Release);
    }
    pub fn monitor_command(&self) {
        self.0
            .command_monitor
            .store(true, std::sync::atomic::Ordering::Release);
    }
    pub fn finish_response(&self, success: bool) {
        use std::sync::atomic::Ordering;
        if self.0.response_finished.swap(true, Ordering::AcqRel) {
            return;
        }
        if self.0.borrowed {
            if !success && self.0.dispatched.load(Ordering::Acquire) && self.is_current() {
                self.0.coordinator.invalidate(&self.0.lease.resource);
            }
            return;
        }
        if !self.0.command_monitor.load(Ordering::Acquire) {
            if success || !self.0.dispatched.load(Ordering::Acquire) {
                self.0.coordinator.complete(&self.0.lease);
            } else if self.is_current() {
                self.0.coordinator.invalidate(&self.0.lease.resource);
            }
        }
    }
    pub fn response_finished(&self) -> bool {
        self.0
            .response_finished
            .load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn command_unresolved(&self) {
        if self.is_current() {
            self.0.coordinator.invalidate(&self.0.lease.resource);
        }
    }
    pub fn command_finished(&self) {
        self.0.coordinator.complete(&self.0.lease);
    }
}

pub struct AgentToolResponse(pub Vec<AgentToolLease>);
impl Drop for AgentToolResponse {
    fn drop(&mut self) {
        for lease in &self.0 {
            lease.finish_response(false);
        }
    }
}
