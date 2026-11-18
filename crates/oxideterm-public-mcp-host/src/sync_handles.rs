use super::PublicMcpHost;
use oxideterm_cloud_sync::{StructuredLocalState, state::CloudSyncPersistedState};
use oxideterm_public_mcp::{ClientRef, PublicSyncSection, SyncPlanRef, UndoRef};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

const SYNC_PLAN_TTL: Duration = Duration::from_secs(10 * 60);
const SYNC_UNDO_TTL: Duration = Duration::from_secs(15 * 60);
const SYNC_PLAN_CAPACITY: usize = 32;
const SYNC_PLAN_CAPACITY_PER_CLIENT: usize = 8;
const SYNC_UNDO_CAPACITY: usize = 16;
const SYNC_UNDO_CAPACITY_PER_CLIENT: usize = 4;

pub struct PublicMcpSyncPlan {
    pub client_ref: ClientRef,
    pub created_at: Instant,
    pub local_state: StructuredLocalState,
    pub sections: Vec<PublicSyncSection>,
    pub kind: PublicMcpSyncPlanKind,
}

pub enum PublicMcpSyncPlanKind {
    Causal(oxideterm_cloud_sync::operation::PreparedSync),
}

pub struct PublicMcpSyncUndo {
    pub client_ref: ClientRef,
    pub created_at: Instant,
    pub post_apply_state: StructuredLocalState,
    pub checkpoint: PublicMcpLocalSyncCheckpoint,
}

pub struct PublicMcpLocalSyncCheckpoint {
    pub connection_store: oxideterm_connections::ConnectionStoreCheckpoint,
    pub saved_forwards: Option<oxideterm_forwarding::SavedForwardCheckpoint>,
    pub quick_commands: oxideterm_quick_commands::QuickCommandsCheckpoint,
    pub plugin_settings: oxideterm_cloud_sync::plugin_settings::PluginSettingsCheckpoint,
    pub settings_store: oxideterm_settings::SettingsStoreCheckpoint,
    pub cloud_state: CloudSyncPersistedState,
    pub settings_path: PathBuf,
}

#[derive(Default)]
pub(crate) struct SyncHandles {
    sync_plans: std::collections::HashMap<SyncPlanRef, PublicMcpSyncPlan>,
    sync_undos: std::collections::HashMap<UndoRef, PublicMcpSyncUndo>,
}

impl PublicMcpHost {
    pub fn revoke_client_sync_handles(&self, client_ref: &ClientRef) {
        self.sync_handles
            .lock()
            .revoke_client_sync_handles(client_ref);
    }

    pub fn insert_sync_plan(&self, plan: PublicMcpSyncPlan) -> SyncPlanRef {
        self.sync_handles.lock().insert_sync_plan(plan)
    }

    pub fn take_sync_plan(
        &self,
        client_ref: &ClientRef,
        plan_ref: &SyncPlanRef,
    ) -> Option<PublicMcpSyncPlan> {
        self.sync_handles
            .lock()
            .take_sync_plan(client_ref, plan_ref)
    }

    pub fn insert_sync_undo(&self, undo: PublicMcpSyncUndo) -> UndoRef {
        self.sync_handles.lock().insert_sync_undo(undo)
    }

    pub fn take_sync_undo(
        &self,
        client_ref: &ClientRef,
        undo_ref: &UndoRef,
    ) -> Option<PublicMcpSyncUndo> {
        self.sync_handles
            .lock()
            .take_sync_undo(client_ref, undo_ref)
    }

    // Failed restores keep the existing handle and its original expiry.
    pub fn return_sync_undo(&self, undo_ref: UndoRef, undo: PublicMcpSyncUndo) {
        self.sync_handles.lock().sync_undos.insert(undo_ref, undo);
    }
}

impl SyncHandles {
    fn revoke_client_sync_handles(&mut self, client_ref: &ClientRef) {
        self.sync_plans
            .retain(|_, plan| &plan.client_ref != client_ref);
        self.sync_undos
            .retain(|_, undo| &undo.client_ref != client_ref);
    }

    fn insert_sync_plan(&mut self, plan: PublicMcpSyncPlan) -> SyncPlanRef {
        self.expire_sync_handles();
        while self.sync_plans.len() >= SYNC_PLAN_CAPACITY
            || self
                .sync_plans
                .values()
                .filter(|candidate| candidate.client_ref == plan.client_ref)
                .count()
                >= SYNC_PLAN_CAPACITY_PER_CLIENT
        {
            let Some(oldest) = self
                .sync_plans
                .iter()
                .filter(|(_, candidate)| candidate.client_ref == plan.client_ref)
                .min_by_key(|(_, candidate)| candidate.created_at)
                .or_else(|| {
                    self.sync_plans
                        .iter()
                        .min_by_key(|(_, plan)| plan.created_at)
                })
                .map(|(plan_ref, _)| plan_ref.clone())
            else {
                break;
            };
            self.sync_plans.remove(&oldest);
        }
        let plan_ref = SyncPlanRef::new();
        self.sync_plans.insert(plan_ref.clone(), plan);
        plan_ref
    }

    fn take_sync_plan(
        &mut self,
        client_ref: &ClientRef,
        plan_ref: &SyncPlanRef,
    ) -> Option<PublicMcpSyncPlan> {
        self.expire_sync_handles();
        self.sync_plans
            .get(plan_ref)
            .is_some_and(|plan| &plan.client_ref == client_ref)
            .then(|| self.sync_plans.remove(plan_ref))
            .flatten()
    }

    fn insert_sync_undo(&mut self, undo: PublicMcpSyncUndo) -> UndoRef {
        self.expire_sync_handles();
        while self.sync_undos.len() >= SYNC_UNDO_CAPACITY
            || self
                .sync_undos
                .values()
                .filter(|candidate| candidate.client_ref == undo.client_ref)
                .count()
                >= SYNC_UNDO_CAPACITY_PER_CLIENT
        {
            let Some(oldest) = self
                .sync_undos
                .iter()
                .filter(|(_, candidate)| candidate.client_ref == undo.client_ref)
                .min_by_key(|(_, candidate)| candidate.created_at)
                .or_else(|| {
                    self.sync_undos
                        .iter()
                        .min_by_key(|(_, undo)| undo.created_at)
                })
                .map(|(undo_ref, _)| undo_ref.clone())
            else {
                break;
            };
            self.sync_undos.remove(&oldest);
        }
        let undo_ref = UndoRef::new();
        self.sync_undos.insert(undo_ref.clone(), undo);
        undo_ref
    }

    fn take_sync_undo(
        &mut self,
        client_ref: &ClientRef,
        undo_ref: &UndoRef,
    ) -> Option<PublicMcpSyncUndo> {
        self.expire_sync_handles();
        self.sync_undos
            .get(undo_ref)
            .is_some_and(|undo| &undo.client_ref == client_ref)
            .then(|| self.sync_undos.remove(undo_ref))
            .flatten()
    }

    pub(crate) fn expire_sync_handles(&mut self) {
        let now = Instant::now();
        self.sync_plans
            .retain(|_, plan| now.saturating_duration_since(plan.created_at) <= SYNC_PLAN_TTL);
        self.sync_undos
            .retain(|_, undo| now.saturating_duration_since(undo.created_at) <= SYNC_UNDO_TTL);
    }
}
