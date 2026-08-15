use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};

use oxideterm_cloud_sync::{
    BackendType, CloudSyncSettings, CloudSyncStatus, ConflictStrategy,
    OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope, StructuredLocalState, StructuredSectionRevisions,
    backend::RemoteMetadata,
    operation::UploadOptions,
    secret_keys,
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::CloudSyncPersistedState,
};
use oxideterm_gpui_cloud_sync::{
    CloudSyncActionResult, CloudSyncApplyOutcome, CloudSyncApplyUiOutcome, CloudSyncDelivery,
    CloudSyncPendingPreview, CloudSyncPreviewSelection, CloudSyncPreviewSummary,
    CloudSyncUploadActionResult, cloud_sync_preview_summary,
    cloud_sync_should_create_rollback_backup, deliver_cloud_sync_apply_preview,
    deliver_cloud_sync_check, deliver_cloud_sync_pull_preview, deliver_cloud_sync_upload,
    finish_cloud_sync_pull_preview_state, has_cloud_sync_structured_conflict,
};
use oxideterm_public_mcp::{
    ClientRef, DomainRequest, PublicSyncConflictStrategy, PublicSyncSection, PublicToolCall,
    SyncPlanRef, SyncSelection, ToolEnvelope, UndoRef,
};
use serde_json::{Value, json};

use super::{PublicMcpWorkspaceBridge, WorkspaceApp, finish_serialized};

const SYNC_PLAN_TTL: Duration = Duration::from_secs(10 * 60);
const SYNC_UNDO_TTL: Duration = Duration::from_secs(15 * 60);
const SYNC_PLAN_CAPACITY: usize = 32;
const SYNC_PLAN_CAPACITY_PER_CLIENT: usize = 8;
const SYNC_UNDO_CAPACITY: usize = 16;
const SYNC_UNDO_CAPACITY_PER_CLIENT: usize = 4;
const SYNC_CANCELLED_ERROR: &str = "cancelled";

pub(super) struct PublicMcpSyncPlan {
    client_ref: ClientRef,
    created_at: Instant,
    local_state: StructuredLocalState,
    raw_scope: RawSyncScope,
    sections: Vec<PublicSyncSection>,
    kind: PublicMcpSyncPlanKind,
}

enum PublicMcpSyncPlanKind {
    Pull {
        preview: Box<CloudSyncPendingPreview>,
        selection: Box<CloudSyncPreviewSelection>,
        remote: PublicMcpRemoteIdentity,
    },
    Publish {
        force: bool,
        remote: PublicMcpRemoteIdentity,
    },
}

pub(super) struct PublicMcpSyncUndo {
    client_ref: ClientRef,
    created_at: Instant,
    post_apply_state: StructuredLocalState,
    checkpoint: PublicMcpLocalSyncCheckpoint,
}

struct PublicMcpLocalSyncCheckpoint {
    connection_store: oxideterm_connections::ConnectionStoreCheckpoint,
    saved_forwards: Option<oxideterm_forwarding::SavedForwardCheckpoint>,
    quick_commands: oxideterm_quick_commands::QuickCommandsCheckpoint,
    plugin_settings: oxideterm_cloud_sync::plugin_settings::PluginSettingsCheckpoint,
    settings_store: oxideterm_settings::SettingsStoreCheckpoint,
    cloud_state: CloudSyncPersistedState,
    settings_path: PathBuf,
}

#[derive(Clone)]
struct PublicMcpRemoteIdentity {
    exists: bool,
    revision: Option<String>,
    etag: Option<String>,
    content_hash: Option<String>,
    section_revisions: Option<StructuredSectionRevisions>,
}

impl PublicMcpRemoteIdentity {
    fn from_metadata(metadata: &RemoteMetadata) -> Self {
        Self {
            exists: metadata.exists,
            revision: metadata.revision.clone(),
            etag: metadata.etag.clone(),
            content_hash: metadata.content_hash.clone(),
            section_revisions: metadata.section_revisions.clone(),
        }
    }

    fn matches(&self, metadata: &RemoteMetadata) -> bool {
        if self.exists != metadata.exists {
            return false;
        }
        if self.revision.is_some() || metadata.revision.is_some() {
            return self.revision == metadata.revision;
        }
        if self.etag.is_some() || metadata.etag.is_some() {
            return self.etag == metadata.etag;
        }
        self.content_hash == metadata.content_hash
    }
}

struct PublicMcpPullWorkerResult {
    preview: CloudSyncPendingPreview,
    secret_hints: BTreeMap<String, bool>,
}

struct PublicMcpCheckWorkerResult {
    metadata: RemoteMetadata,
    secret_hints: BTreeMap<String, bool>,
}

struct PublicMcpApplyWorkerResult {
    outcome: CloudSyncApplyUiOutcome,
    rollback_backup: Option<oxideterm_cloud_sync::state::CloudSyncRollbackBackup>,
    secret_hints: BTreeMap<String, bool>,
}

struct PublicMcpUploadWorkerResult {
    result: Result<oxideterm_cloud_sync::operation::UploadOutcome, String>,
    remote_metadata: Option<RemoteMetadata>,
    revision_sequence_consumed: Option<u64>,
    secret_hints: BTreeMap<String, bool>,
}

impl PublicMcpWorkspaceBridge {
    pub(super) fn revoke_client_sync_handles(&mut self, client_ref: &ClientRef) {
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

    fn expire_sync_handles(&mut self) {
        let now = Instant::now();
        self.sync_plans
            .retain(|_, plan| now.saturating_duration_since(plan.created_at) <= SYNC_PLAN_TTL);
        self.sync_undos
            .retain(|_, undo| now.saturating_duration_since(undo.created_at) <= SYNC_UNDO_TTL);
    }
}

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_sync_status(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let state = self.cloud_sync.read(cx).controller.store.state().clone();
        let local_snapshot = match build_local_snapshot(
            &self.connection_store,
            self.forwarding_service.registry(),
            &self.settings_store,
            state.last_synced_structured_state.as_ref(),
            Some(&state.sync_scope),
        ) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                request.finish(ToolEnvelope::failed(
                    "The local Cloud Sync snapshot could not be prepared",
                ));
                return;
            }
        };
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.store.state_mut().local_dirty = local_snapshot.dirty.has_dirty;
            cloud_sync.controller.store.state_mut().local_dirty_sections =
                Some(local_snapshot.dirty.dirty_sections.clone());
        });
        self.save_cloud_sync_state(cx);
        let state = self.cloud_sync.read(cx).controller.store.state();
        let secret_hints = &state.secret_hints;
        finish_serialized(
            request,
            json!({
                "backend_type": state.settings.backend_type,
                "configured": cloud_sync_is_configured(&state.settings, secret_hints),
                "status": state.status,
                "operation_in_flight": self.cloud_sync.read(cx).operation_in_flight(),
                "remote_exists": state.remote_exists,
                "remote_revision": state.last_known_remote_revision,
                "local_dirty": local_snapshot.dirty.has_dirty,
                "dirty_sections": dirty_sections_projection(&local_snapshot),
                "last_sync_at": state.last_sync_at,
                "last_upload_at": state.last_upload_at,
                "last_check_at": state.last_check_at,
                "blocked_by_conflict": state.auto_upload_blocked_by_conflict,
                "has_sync_password": secret_hints
                    .get(secret_keys::SYNC_PASSWORD)
                    .copied()
                    .unwrap_or(false),
                "has_backend_credentials": secret_hints.iter().any(|(key, present)| {
                    key != secret_keys::SYNC_PASSWORD && *present
                }),
            }),
        );
    }

    pub(super) fn handle_public_mcp_sync_pull_preview(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::SyncPullPreview(args) = &request.call else {
            return;
        };
        if !self.begin_public_mcp_sync_action("mcp_pull_preview", CloudSyncStatus::Checking, cx) {
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        let state = self.cloud_sync.read(cx).controller.store.state().clone();
        let (raw_scope, sections) = match public_sync_scope(&state.sync_scope, &args.selection) {
            Ok(scope) => scope,
            Err(error) => {
                self.clear_public_mcp_sync_action(cx);
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        let local_state = match self.public_mcp_full_local_state() {
            Ok(state) => state,
            Err(error) => {
                self.clear_public_mcp_sync_action(cx);
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        let conflict_strategy = public_conflict_strategy(args.conflict_strategy);
        let service = self.cloud_sync.read(cx).controller.service.clone();
        let connection_store = self.connection_store.clone();
        let settings = state.settings.clone();
        let hints = state.secret_hints.clone();
        let previous_remote_sections = state.last_synced_remote_sections.clone();
        let cancellation = request.cancellation_token();
        let worker = self.forwarding_runtime.spawn(async move {
            tokio::select! {
                _ = cancellation.cancelled() => Err(SYNC_CANCELLED_ERROR.to_owned()),
                result = run_pull_preview_worker(
                    service,
                    connection_store,
                    settings,
                    hints,
                    previous_remote_sections,
                ) => result,
            }
        });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.finish_public_mcp_sync_pull_preview(
                    request,
                    raw_scope,
                    sections,
                    local_state,
                    conflict_strategy,
                    result,
                    cx,
                );
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_public_mcp_sync_pull_preview(
        &mut self,
        request: DomainRequest,
        raw_scope: RawSyncScope,
        sections: Vec<PublicSyncSection>,
        local_state: StructuredLocalState,
        conflict_strategy: ConflictStrategy,
        result: Result<Result<PublicMcpPullWorkerResult, String>, tokio::task::JoinError>,
        cx: &mut gpui::Context<Self>,
    ) {
        if request.is_cancelled() {
            self.clear_public_mcp_sync_action(cx);
            return;
        }
        let worker = match result {
            Ok(Ok(worker)) => worker,
            Ok(Err(error)) => {
                self.fail_public_mcp_sync_action("pull", &error, request, cx);
                return;
            }
            Err(_) => {
                self.fail_public_mcp_sync_action("pull", "worker_stopped", request, cx);
                return;
            }
        };
        let preview = worker.preview;
        let mut selection = CloudSyncPreviewSelection::from_preview(&preview, conflict_strategy);
        restrict_pull_selection(&mut selection, &sections);
        let summary = cloud_sync_preview_summary(&preview);
        let remote = PublicMcpRemoteIdentity::from_metadata(preview_remote_metadata(&preview));
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.store.state_mut().secret_hints = worker.secret_hints;
            finish_cloud_sync_pull_preview_state(cloud_sync.controller.store.state_mut(), &preview);
            cloud_sync.controller.active_action = None;
            cloud_sync.controller.progress = None;
        });
        self.save_cloud_sync_state(cx);
        let plan_ref = self.public_mcp.insert_sync_plan(PublicMcpSyncPlan {
            client_ref: request.client_ref.clone(),
            created_at: Instant::now(),
            local_state,
            raw_scope,
            sections: sections.clone(),
            kind: PublicMcpSyncPlanKind::Pull {
                preview: Box::new(preview),
                selection: Box::new(selection),
                remote: remote.clone(),
            },
        });
        finish_serialized(
            request,
            json!({
                "sync_plan_ref": plan_ref,
                "kind": "pull",
                "sections": sections,
                "remote_revision": remote.revision,
                "summary": preview_summary_projection(&summary),
            }),
        );
    }

    pub(super) fn handle_public_mcp_sync_publish_preview(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::SyncPublishPreview(args) = &request.call else {
            return;
        };
        if !self.begin_public_mcp_sync_action("mcp_publish_preview", CloudSyncStatus::Checking, cx)
        {
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        let state = self.cloud_sync.read(cx).controller.store.state().clone();
        let (raw_scope, sections) = match public_sync_scope(&state.sync_scope, &args.selection) {
            Ok(scope) => scope,
            Err(error) => {
                self.clear_public_mcp_sync_action(cx);
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        let local_snapshot = match build_local_snapshot(
            &self.connection_store,
            self.forwarding_service.registry(),
            &self.settings_store,
            state.last_synced_structured_state.as_ref(),
            Some(&raw_scope),
        ) {
            Ok(snapshot) => snapshot,
            Err(_) => {
                self.clear_public_mcp_sync_action(cx);
                request.finish(ToolEnvelope::failed(
                    "The local Cloud Sync snapshot could not be prepared",
                ));
                return;
            }
        };
        let local_state = match self.public_mcp_full_local_state() {
            Ok(state) => state,
            Err(error) => {
                self.clear_public_mcp_sync_action(cx);
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        let force = args.force;
        let service = self.cloud_sync.read(cx).controller.service.clone();
        let settings = state.settings.clone();
        let hints = state.secret_hints.clone();
        let cancellation = request.cancellation_token();
        let skip_remote_check = matches!(settings.backend_type, BackendType::GithubGist)
            && settings.git_repository.trim().is_empty();
        let worker = self.forwarding_runtime.spawn(async move {
            if skip_remote_check {
                return Ok(PublicMcpCheckWorkerResult {
                    metadata: RemoteMetadata::missing(),
                    secret_hints: hints,
                });
            }
            tokio::select! {
                _ = cancellation.cancelled() => Err(SYNC_CANCELLED_ERROR.to_owned()),
                result = run_check_worker(service, settings, hints) => result,
            }
        });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.finish_public_mcp_sync_publish_preview(
                    request,
                    raw_scope,
                    sections,
                    local_state,
                    local_snapshot,
                    force,
                    result,
                    cx,
                );
            });
        })
        .detach();
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_public_mcp_sync_publish_preview(
        &mut self,
        request: DomainRequest,
        raw_scope: RawSyncScope,
        sections: Vec<PublicSyncSection>,
        local_state: StructuredLocalState,
        local_snapshot: CloudSyncLocalSnapshot,
        force: bool,
        result: Result<Result<PublicMcpCheckWorkerResult, String>, tokio::task::JoinError>,
        cx: &mut gpui::Context<Self>,
    ) {
        if request.is_cancelled() {
            self.clear_public_mcp_sync_action(cx);
            return;
        }
        let worker = match result {
            Ok(Ok(worker)) => worker,
            Ok(Err(error)) => {
                self.fail_public_mcp_sync_action("publish_preview", &error, request, cx);
                return;
            }
            Err(_) => {
                self.fail_public_mcp_sync_action("publish_preview", "worker_stopped", request, cx);
                return;
            }
        };
        let remote = PublicMcpRemoteIdentity::from_metadata(&worker.metadata);
        let previous_remote_sections = self
            .cloud_sync
            .read(cx)
            .controller
            .store
            .state()
            .last_synced_remote_sections
            .as_ref();
        let has_conflict = worker.metadata.exists
            && has_cloud_sync_structured_conflict(
                &local_snapshot.dirty.dirty_sections,
                worker.metadata.section_revisions.as_ref(),
                previous_remote_sections,
            );
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.store.state_mut().secret_hints = worker.secret_hints;
            cloud_sync.controller.active_action = None;
            cloud_sync.controller.progress = None;
        });
        self.finish_cloud_sync_check(Some(worker.metadata), cx);
        let plan_ref = self.public_mcp.insert_sync_plan(PublicMcpSyncPlan {
            client_ref: request.client_ref.clone(),
            created_at: Instant::now(),
            local_state,
            raw_scope,
            sections: sections.clone(),
            kind: PublicMcpSyncPlanKind::Publish {
                force,
                remote: remote.clone(),
            },
        });
        finish_serialized(
            request,
            json!({
                "sync_plan_ref": plan_ref,
                "kind": "publish",
                "sections": sections,
                "force": force,
                "remote_revision": remote.revision,
                "remote_exists": remote.exists,
                "has_conflict": has_conflict,
                "local_dirty": local_snapshot.dirty.has_dirty,
                "summary": local_snapshot_summary(&local_snapshot),
            }),
        );
    }

    pub(super) fn handle_public_mcp_sync_apply_plan(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::SyncApplyPlan(args) = &request.call else {
            return;
        };
        let Some(plan) = self
            .public_mcp
            .take_sync_plan(&request.client_ref, &args.sync_plan_ref)
        else {
            request.finish(ToolEnvelope::failed(
                "The Cloud Sync plan is unavailable, expired, or already used",
            ));
            return;
        };
        if self.cloud_sync.read(cx).operation_in_flight() {
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        let current_state = match self.public_mcp_full_local_state() {
            Ok(state) => state,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if current_state != plan.local_state {
            request.finish(ToolEnvelope::failed(
                "Local synchronized data changed after the plan was created",
            ));
            return;
        }
        match plan.kind {
            PublicMcpSyncPlanKind::Pull {
                preview,
                selection,
                remote,
            } => self.start_public_mcp_sync_pull_apply(
                request,
                plan.raw_scope,
                plan.sections,
                *preview,
                *selection,
                remote,
                cx,
            ),
            PublicMcpSyncPlanKind::Publish { force, remote } => self
                .start_public_mcp_sync_publish_apply(
                    request,
                    plan.raw_scope,
                    plan.sections,
                    force,
                    remote,
                    cx,
                ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn start_public_mcp_sync_pull_apply(
        &mut self,
        request: DomainRequest,
        raw_scope: RawSyncScope,
        sections: Vec<PublicSyncSection>,
        preview: CloudSyncPendingPreview,
        selection: CloudSyncPreviewSelection,
        remote: PublicMcpRemoteIdentity,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.begin_public_mcp_sync_action("mcp_apply_pull", CloudSyncStatus::Checking, cx) {
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        let strict_undo = pull_selection_has_strict_undo(&selection);
        let checkpoint = if strict_undo {
            match self.capture_public_mcp_sync_checkpoint(cx) {
                Ok(checkpoint) => Some(checkpoint),
                Err(error) => {
                    self.clear_public_mcp_sync_action(cx);
                    request.finish(ToolEnvelope::failed(error));
                    return;
                }
            }
        } else {
            None
        };
        let state = self.cloud_sync.read(cx).controller.store.state().clone();
        let create_rollback_backup =
            cloud_sync_should_create_rollback_backup(&preview, state.local_dirty);
        let service = self.cloud_sync.read(cx).controller.service.clone();
        let connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_service.registry().clone();
        let settings_store = self.settings_store.clone();
        let settings = state.settings.clone();
        let hints = state.secret_hints.clone();
        let source_revision = state.last_known_remote_revision.clone();
        let cancellation = request.cancellation_token();
        let worker = self.forwarding_runtime.spawn(async move {
            run_pull_apply_worker(
                service,
                connection_store,
                forwarding_registry,
                settings_store,
                settings,
                hints,
                source_revision,
                preview,
                selection,
                create_rollback_backup,
                remote,
                cancellation,
            )
            .await
        });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.finish_public_mcp_sync_pull_apply(
                    request, raw_scope, sections, checkpoint, result, cx,
                );
            });
        })
        .detach();
    }

    fn finish_public_mcp_sync_pull_apply(
        &mut self,
        request: DomainRequest,
        _raw_scope: RawSyncScope,
        sections: Vec<PublicSyncSection>,
        checkpoint: Option<PublicMcpLocalSyncCheckpoint>,
        result: Result<Result<PublicMcpApplyWorkerResult, String>, tokio::task::JoinError>,
        cx: &mut gpui::Context<Self>,
    ) {
        let request_cancelled = request.is_cancelled();
        let worker = match result {
            Ok(Ok(worker)) => worker,
            Ok(Err(error)) if request_cancelled && error == SYNC_CANCELLED_ERROR => {
                self.clear_public_mcp_sync_action(cx);
                return;
            }
            Ok(Err(error)) => {
                self.fail_public_mcp_sync_action("apply", &error, request, cx);
                return;
            }
            Err(_) => {
                self.fail_public_mcp_sync_action("apply", "worker_stopped", request, cx);
                return;
            }
        };
        let outcome_projection = apply_outcome_projection(&worker.outcome.outcome);
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.store.state_mut().secret_hints = worker.secret_hints;
            cloud_sync.controller.active_action = None;
            if let Some(backup) = worker.rollback_backup.clone() {
                cloud_sync
                    .controller
                    .store
                    .state_mut()
                    .append_rollback_backup(backup);
            }
        });
        self.finish_cloud_sync_apply_preview(worker.outcome, cx);
        let requires_publish = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.upload_after_current.take().is_some()
        });
        let undo_ref = (!request_cancelled)
            .then(|| {
                checkpoint.and_then(|checkpoint| {
                    self.public_mcp_full_local_state()
                        .ok()
                        .map(|post_apply_state| {
                            self.public_mcp.insert_sync_undo(PublicMcpSyncUndo {
                                client_ref: request.client_ref.clone(),
                                created_at: Instant::now(),
                                post_apply_state,
                                checkpoint,
                            })
                        })
                })
            })
            .flatten();
        finish_serialized(
            request,
            json!({
                "applied": true,
                "sections": sections,
                "outcome": outcome_projection,
                "requires_publish": requires_publish,
                "undo_ref": undo_ref,
                "local_recovery_backup_retained": worker.rollback_backup.is_some(),
            }),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn start_public_mcp_sync_publish_apply(
        &mut self,
        request: DomainRequest,
        raw_scope: RawSyncScope,
        sections: Vec<PublicSyncSection>,
        force: bool,
        remote: PublicMcpRemoteIdentity,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.begin_public_mcp_sync_action("mcp_apply_publish", CloudSyncStatus::Uploading, cx) {
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        let state = self.cloud_sync.read(cx).controller.store.state().clone();
        let portable_secrets =
            match self.collect_cloud_sync_sensitive_portable_secrets(&raw_scope, cx) {
                Ok(secrets) => secrets,
                Err(_) => {
                    self.clear_public_mcp_sync_action(cx);
                    request.finish(ToolEnvelope::failed(
                        "The selected protected Cloud Sync content is unavailable",
                    ));
                    return;
                }
            };
        let device_id = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync
                .controller
                .store
                .state_mut()
                .ensure_device_id(oxideterm_gpui_cloud_sync::cloud_sync_platform_label())
        });
        self.save_cloud_sync_state(cx);
        let revision_sequence = state.revision_seq.saturating_add(1);
        let service = self.cloud_sync.read(cx).controller.service.clone();
        let connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_service.registry().clone();
        let settings_store = self.settings_store.clone();
        let settings = state.settings.clone();
        let hints = state.secret_hints.clone();
        let cancellation = request.cancellation_token();
        let options = UploadOptions {
            force,
            device_id,
            revision_sequence,
            previous_remote_revision: remote.revision.clone(),
            previous_remote_sections: remote.section_revisions.clone(),
            last_synced_structured_state: state.last_synced_structured_state.clone(),
            raw_sync_scope: Some(raw_scope),
            portable_secrets,
            automatic: false,
            skip_if_busy: false,
            ..UploadOptions::default()
        };
        let worker = self.forwarding_runtime.spawn(async move {
            run_upload_worker(
                service,
                connection_store,
                forwarding_registry,
                settings_store,
                settings,
                hints,
                options,
                cancellation,
            )
            .await
        });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.finish_public_mcp_sync_publish_apply(request, sections, result, cx);
            });
        })
        .detach();
    }

    fn finish_public_mcp_sync_publish_apply(
        &mut self,
        request: DomainRequest,
        sections: Vec<PublicSyncSection>,
        result: Result<Result<PublicMcpUploadWorkerResult, String>, tokio::task::JoinError>,
        cx: &mut gpui::Context<Self>,
    ) {
        let request_cancelled = request.is_cancelled();
        let worker = match result {
            Ok(Ok(worker)) => worker,
            Ok(Err(error)) if request_cancelled && error == SYNC_CANCELLED_ERROR => {
                self.clear_public_mcp_sync_action(cx);
                return;
            }
            Ok(Err(error)) => {
                self.fail_public_mcp_sync_action("upload", &error, request, cx);
                return;
            }
            Err(_) => {
                self.fail_public_mcp_sync_action("upload", "worker_stopped", request, cx);
                return;
            }
        };
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.store.state_mut().secret_hints = worker.secret_hints;
            if let Some(metadata) = worker.remote_metadata.as_ref() {
                oxideterm_gpui_cloud_sync::persist_remote_metadata(
                    cloud_sync.controller.store.state_mut(),
                    metadata,
                );
            }
            if let Some(sequence) = worker.revision_sequence_consumed {
                cloud_sync.controller.store.state_mut().revision_seq = cloud_sync
                    .controller
                    .store
                    .state()
                    .revision_seq
                    .max(sequence);
            }
            cloud_sync.controller.active_action = None;
        });
        let outcome = match worker.result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.fail_public_mcp_sync_action("upload", &error, request, cx);
                return;
            }
        };
        let revision = outcome.revision.clone();
        self.finish_cloud_sync_upload(outcome, false, cx);
        finish_serialized(
            request,
            json!({
                "published": true,
                "sections": sections,
                "remote_revision": revision,
                "undo_ref": Value::Null,
            }),
        );
    }

    pub(super) fn handle_public_mcp_sync_restore(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::SyncRestore(args) = &request.call else {
            return;
        };
        let Some(undo) = self
            .public_mcp
            .take_sync_undo(&request.client_ref, &args.undo_ref)
        else {
            request.finish(ToolEnvelope::failed(
                "The Cloud Sync undo handle is unavailable, expired, or already used",
            ));
            return;
        };
        if self.cloud_sync.read(cx).operation_in_flight() {
            self.public_mcp
                .sync_undos
                .insert(args.undo_ref.clone(), undo);
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        let current_state = match self.public_mcp_full_local_state() {
            Ok(state) => state,
            Err(error) => {
                self.public_mcp
                    .sync_undos
                    .insert(args.undo_ref.clone(), undo);
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if current_state != undo.post_apply_state {
            self.public_mcp
                .sync_undos
                .insert(args.undo_ref.clone(), undo);
            request.finish(ToolEnvelope::failed(
                "Local synchronized data changed after the undo handle was created",
            ));
            return;
        }
        let compensation = match self.capture_public_mcp_sync_checkpoint(cx) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                self.public_mcp
                    .sync_undos
                    .insert(args.undo_ref.clone(), undo);
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        if let Err(error) = self.restore_public_mcp_sync_checkpoint(&undo.checkpoint, cx) {
            let _ = self.restore_public_mcp_sync_checkpoint(&compensation, cx);
            self.public_mcp
                .sync_undos
                .insert(args.undo_ref.clone(), undo);
            request.finish(ToolEnvelope::failed(error));
            return;
        }
        self.terminal.update(cx, |terminal, _cx| {
            terminal.quick_commands.store.reload_from_store()
        });
        self.bootstrap_native_plugin_runtime(cx);
        self.invalidate_cloud_sync_snapshot_caches(cx);
        self.refresh_cloud_sync_local_dirty_state(cx);
        self.save_cloud_sync_state(cx);
        cx.notify();
        finish_serialized(request, json!({ "restored": true }));
    }

    fn begin_public_mcp_sync_action(
        &mut self,
        action: &'static str,
        status: CloudSyncStatus,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.cloud_sync.read(cx).operation_in_flight() {
            return false;
        }
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.active_action = Some(action);
            cloud_sync.controller.progress = None;
            cloud_sync.controller.store.state_mut().status = status;
            cloud_sync.controller.store.state_mut().last_error = None;
        });
        self.save_cloud_sync_state(cx);
        true
    }

    fn clear_public_mcp_sync_action(&mut self, cx: &mut gpui::Context<Self>) {
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.active_action = None;
            cloud_sync.controller.progress = None;
            if matches!(
                cloud_sync.controller.store.state().status,
                CloudSyncStatus::Checking | CloudSyncStatus::Uploading
            ) {
                cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Idle;
            }
        });
        self.save_cloud_sync_state(cx);
    }

    fn fail_public_mcp_sync_action(
        &mut self,
        action: &str,
        raw_error: &str,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.active_action = None;
        });
        self.finish_cloud_sync_error(action, raw_error.to_owned(), cx);
        request.finish(ToolEnvelope::failed(public_cloud_sync_error(raw_error)));
    }

    fn public_mcp_full_local_state(&self) -> Result<StructuredLocalState, String> {
        build_local_snapshot(
            &self.connection_store,
            self.forwarding_service.registry(),
            &self.settings_store,
            None,
            Some(&full_sync_scope()),
        )
        .map(|snapshot| snapshot.dirty.current_state)
        .map_err(|_| "The local Cloud Sync revision could not be calculated".to_owned())
    }

    fn capture_public_mcp_sync_checkpoint(
        &self,
        cx: &gpui::App,
    ) -> Result<PublicMcpLocalSyncCheckpoint, String> {
        let settings_path = self.settings_store.path().to_path_buf();
        Ok(PublicMcpLocalSyncCheckpoint {
            connection_store: self.connection_store.create_checkpoint().map_err(|_| {
                "The connection store could not be checkpointed for Cloud Sync".to_owned()
            })?,
            saved_forwards: self
                .forwarding_service
                .registry()
                .checkpoint_saved_forwards()
                .map_err(|_| {
                    "Saved forwards could not be checkpointed for Cloud Sync".to_owned()
                })?,
            quick_commands: oxideterm_quick_commands::capture_checkpoint(&settings_path).map_err(
                |_| "Quick Commands could not be checkpointed for Cloud Sync".to_owned(),
            )?,
            plugin_settings: oxideterm_cloud_sync::plugin_settings::checkpoint_plugin_settings(
                &settings_path,
            )
            .map_err(|_| "Plugin settings could not be checkpointed for Cloud Sync".to_owned())?,
            settings_store: self
                .settings_store
                .create_checkpoint()
                .map_err(|_| "App settings could not be checkpointed for Cloud Sync".to_owned())?,
            cloud_state: self.cloud_sync.read(cx).controller.store.state().clone(),
            settings_path,
        })
    }

    fn restore_public_mcp_sync_checkpoint(
        &mut self,
        checkpoint: &PublicMcpLocalSyncCheckpoint,
        cx: &mut gpui::Context<Self>,
    ) -> Result<(), String> {
        self.connection_store
            .restore_checkpoint(&checkpoint.connection_store)
            .map_err(|_| "The connection store could not be restored".to_owned())?;
        if let Some(saved_forwards) = checkpoint.saved_forwards.as_ref() {
            self.forwarding_service
                .registry()
                .restore_saved_forwards(saved_forwards)
                .map_err(|_| "Saved forwards could not be restored".to_owned())?;
        }
        oxideterm_quick_commands::restore_checkpoint(
            &checkpoint.settings_path,
            &checkpoint.quick_commands,
        )
        .map_err(|_| "Quick Commands could not be restored".to_owned())?;
        oxideterm_cloud_sync::plugin_settings::restore_plugin_settings(
            &checkpoint.settings_path,
            &checkpoint.plugin_settings,
        )
        .map_err(|_| "Plugin settings could not be restored".to_owned())?;
        self.settings_store
            .restore_checkpoint(&checkpoint.settings_store)
            .map_err(|_| "App settings could not be restored".to_owned())?;
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync
                .controller
                .store
                .replace_state(checkpoint.cloud_state.clone());
        });
        Ok(())
    }
}

async fn run_pull_preview_worker(
    service: oxideterm_cloud_sync::operation::CloudSyncOperationService,
    connection_store: oxideterm_connections::ConnectionStore,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    previous_remote_sections: Option<StructuredSectionRevisions>,
) -> Result<PublicMcpPullWorkerResult, String> {
    let (sender, receiver) = mpsc::channel();
    deliver_cloud_sync_pull_preview(
        sender,
        service,
        connection_store,
        settings,
        hints,
        previous_remote_sections,
    )
    .await;
    receiver
        .into_iter()
        .find_map(|delivery| match delivery {
            CloudSyncDelivery::PullPreviewFinished(action) => Some(action),
            _ => None,
        })
        .ok_or_else(|| "worker_stopped".to_owned())
        .and_then(|action| {
            action.result.map(|preview| PublicMcpPullWorkerResult {
                preview,
                secret_hints: action.secret_hints,
            })
        })
}

async fn run_check_worker(
    service: oxideterm_cloud_sync::operation::CloudSyncOperationService,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
) -> Result<PublicMcpCheckWorkerResult, String> {
    let (sender, receiver) = mpsc::channel();
    deliver_cloud_sync_check(sender, service, settings, hints, false).await;
    receiver
        .into_iter()
        .find_map(|delivery| match delivery {
            CloudSyncDelivery::CheckFinished(action) => Some(action),
            _ => None,
        })
        .ok_or_else(|| "worker_stopped".to_owned())
        .and_then(|action| {
            action
                .result
                .and_then(|metadata| metadata.ok_or_else(|| "worker_busy".to_owned()))
                .map(|metadata| PublicMcpCheckWorkerResult {
                    metadata,
                    secret_hints: action.secret_hints,
                })
        })
}

#[allow(clippy::too_many_arguments)]
async fn run_pull_apply_worker(
    service: oxideterm_cloud_sync::operation::CloudSyncOperationService,
    connection_store: oxideterm_connections::ConnectionStore,
    forwarding_registry: oxideterm_forwarding::ForwardingRegistry,
    settings_store: oxideterm_settings::SettingsStore,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    source_revision: Option<String>,
    preview: CloudSyncPendingPreview,
    selection: CloudSyncPreviewSelection,
    create_rollback_backup: bool,
    remote: PublicMcpRemoteIdentity,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<PublicMcpApplyWorkerResult, String> {
    let checked = tokio::select! {
        _ = cancellation.cancelled() => return Err(SYNC_CANCELLED_ERROR.to_owned()),
        checked = run_check_worker(service.clone(), settings.clone(), hints) => checked?,
    };
    if !remote.matches(&checked.metadata) {
        return Err("remote_changed_after_preview".to_owned());
    }
    if cancellation.is_cancelled() {
        return Err(SYNC_CANCELLED_ERROR.to_owned());
    }
    let (sender, receiver) = mpsc::channel();
    // Once apply starts it owns real local writes and must deliver its completion state.
    deliver_cloud_sync_apply_preview(
        sender,
        service,
        connection_store,
        forwarding_registry,
        settings_store,
        settings,
        checked.secret_hints,
        source_revision,
        preview,
        selection,
        create_rollback_backup,
    )
    .await;
    let mut backup = None;
    let mut action: Option<CloudSyncActionResult<CloudSyncApplyUiOutcome>> = None;
    for delivery in receiver {
        match delivery {
            CloudSyncDelivery::RollbackBackupCreated(created) => backup = Some(created),
            CloudSyncDelivery::ApplyPreviewFinished(finished) => action = Some(finished),
            _ => {}
        }
    }
    let action = action.ok_or_else(|| "worker_stopped".to_owned())?;
    action.result.map(|outcome| PublicMcpApplyWorkerResult {
        outcome,
        rollback_backup: backup,
        secret_hints: action.secret_hints,
    })
}

#[allow(clippy::too_many_arguments)]
async fn run_upload_worker(
    service: oxideterm_cloud_sync::operation::CloudSyncOperationService,
    connection_store: oxideterm_connections::ConnectionStore,
    forwarding_registry: oxideterm_forwarding::ForwardingRegistry,
    settings_store: oxideterm_settings::SettingsStore,
    settings: CloudSyncSettings,
    hints: BTreeMap<String, bool>,
    options: UploadOptions,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<PublicMcpUploadWorkerResult, String> {
    if cancellation.is_cancelled() {
        return Err(SYNC_CANCELLED_ERROR.to_owned());
    }
    let (sender, receiver) = mpsc::channel();
    // Remote upload has no reliable rollback after dispatch, so completion is always observed.
    deliver_cloud_sync_upload(
        sender,
        service,
        connection_store,
        forwarding_registry,
        settings_store,
        settings,
        hints,
        options,
        false,
    )
    .await;
    receiver
        .into_iter()
        .find_map(|delivery| match delivery {
            CloudSyncDelivery::UploadFinished { action, .. } => Some(action),
            _ => None,
        })
        .map(upload_worker_result)
        .ok_or_else(|| "worker_stopped".to_owned())
}

fn upload_worker_result(action: CloudSyncUploadActionResult) -> PublicMcpUploadWorkerResult {
    PublicMcpUploadWorkerResult {
        result: action.result,
        remote_metadata: action.remote_metadata,
        revision_sequence_consumed: action.revision_sequence_consumed,
        secret_hints: action.secret_hints,
    }
}

fn public_sync_scope(
    base: &RawSyncScope,
    selection: &SyncSelection,
) -> Result<(RawSyncScope, Vec<PublicSyncSection>), String> {
    let Some(selected) = selection.sections.as_ref() else {
        let scope = base.clone();
        let sections = raw_scope_sections(&scope);
        return Ok((scope, sections));
    };
    if selected.is_empty() {
        return Err("At least one Cloud Sync section must be selected".to_owned());
    }
    let selected = selected.iter().copied().collect::<BTreeSet<_>>();
    if selected.contains(&PublicSyncSection::SensitiveCredentials)
        && !selected.contains(&PublicSyncSection::Connections)
    {
        return Err("Sensitive credentials require the connections section".to_owned());
    }
    let contains = |section| selected.contains(&section);
    let scope = RawSyncScope {
        sync_connections: Some(contains(PublicSyncSection::Connections)),
        sync_forwards: Some(contains(PublicSyncSection::Forwards)),
        sync_quick_commands: Some(contains(PublicSyncSection::QuickCommands)),
        sync_serial_profiles: Some(contains(PublicSyncSection::SerialProfiles)),
        sync_telnet_profiles: Some(contains(PublicSyncSection::TelnetProfiles)),
        sync_mosh_profiles: Some(contains(PublicSyncSection::MoshProfiles)),
        sync_remote_desktop_profiles: Some(contains(PublicSyncSection::RemoteDesktopProfiles)),
        sync_sensitive_credentials: Some(contains(PublicSyncSection::SensitiveCredentials)),
        sync_app_settings: Some(contains(PublicSyncSection::AppSettings)),
        app_settings_sections: base.app_settings_sections.clone(),
        include_local_terminal_env_vars: Some(
            contains(PublicSyncSection::AppSettings)
                && base.include_local_terminal_env_vars.unwrap_or(false),
        ),
        sync_plugin_settings: Some(contains(PublicSyncSection::PluginSettings)),
        plugin_ids: base.plugin_ids.clone(),
    };
    Ok((scope, selected.into_iter().collect()))
}

fn raw_scope_sections(scope: &RawSyncScope) -> Vec<PublicSyncSection> {
    let mut sections = Vec::new();
    push_enabled_sync_section(
        &mut sections,
        scope.sync_connections,
        PublicSyncSection::Connections,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_forwards,
        PublicSyncSection::Forwards,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_quick_commands,
        PublicSyncSection::QuickCommands,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_serial_profiles,
        PublicSyncSection::SerialProfiles,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_telnet_profiles,
        PublicSyncSection::TelnetProfiles,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_mosh_profiles,
        PublicSyncSection::MoshProfiles,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_remote_desktop_profiles,
        PublicSyncSection::RemoteDesktopProfiles,
    );
    if scope.sync_sensitive_credentials.unwrap_or(false) {
        sections.push(PublicSyncSection::SensitiveCredentials);
    }
    push_enabled_sync_section(
        &mut sections,
        scope.sync_app_settings,
        PublicSyncSection::AppSettings,
    );
    push_enabled_sync_section(
        &mut sections,
        scope.sync_plugin_settings,
        PublicSyncSection::PluginSettings,
    );
    sections
}

fn push_enabled_sync_section(
    sections: &mut Vec<PublicSyncSection>,
    enabled: Option<bool>,
    section: PublicSyncSection,
) {
    if enabled.unwrap_or(true) {
        sections.push(section);
    }
}

fn full_sync_scope() -> RawSyncScope {
    RawSyncScope {
        sync_connections: Some(true),
        sync_forwards: Some(true),
        sync_quick_commands: Some(true),
        sync_serial_profiles: Some(true),
        sync_telnet_profiles: Some(true),
        sync_mosh_profiles: Some(true),
        sync_remote_desktop_profiles: Some(true),
        sync_sensitive_credentials: Some(true),
        sync_app_settings: Some(true),
        app_settings_sections: Some(
            OXIDE_APP_SETTINGS_SECTION_IDS
                .iter()
                .map(|section| (*section).to_owned())
                .collect(),
        ),
        include_local_terminal_env_vars: Some(true),
        sync_plugin_settings: Some(true),
        plugin_ids: None,
    }
}

fn public_conflict_strategy(strategy: PublicSyncConflictStrategy) -> ConflictStrategy {
    match strategy {
        PublicSyncConflictStrategy::Merge => ConflictStrategy::Merge,
        PublicSyncConflictStrategy::Replace => ConflictStrategy::Replace,
        PublicSyncConflictStrategy::Skip => ConflictStrategy::Skip,
        PublicSyncConflictStrategy::Rename => ConflictStrategy::Rename,
    }
}

fn restrict_pull_selection(
    selection: &mut CloudSyncPreviewSelection,
    sections: &[PublicSyncSection],
) {
    let selected = sections.iter().copied().collect::<BTreeSet<_>>();
    if !selected.contains(&PublicSyncSection::Connections) {
        selection.import_connections = false;
        selection.selected_connection_ids.clear();
        selection.selected_connection_names.clear();
    }
    if !selected.contains(&PublicSyncSection::Forwards) {
        selection.import_forwards = false;
        selection.selected_forward_ids.clear();
    }
    if !selected.contains(&PublicSyncSection::QuickCommands) {
        selection.import_quick_commands = false;
        selection.selected_quick_command_ids.clear();
    }
    if !selected.contains(&PublicSyncSection::SerialProfiles) {
        selection.import_serial_profiles = false;
        selection.selected_serial_profile_ids.clear();
    }
    if !selected.contains(&PublicSyncSection::TelnetProfiles) {
        selection.import_telnet_profiles = false;
        selection.selected_telnet_profile_ids.clear();
    }
    if !selected.contains(&PublicSyncSection::MoshProfiles) {
        selection.import_mosh_profiles = false;
        selection.selected_mosh_profile_ids.clear();
    }
    if !selected.contains(&PublicSyncSection::RemoteDesktopProfiles) {
        selection.import_remote_desktop_profiles = false;
        selection.selected_remote_desktop_profile_ids.clear();
    }
    if !selected.contains(&PublicSyncSection::SensitiveCredentials) {
        selection.import_sensitive_credentials = false;
    }
    if !selected.contains(&PublicSyncSection::AppSettings) {
        selection.import_app_settings = false;
        selection.selected_app_settings_sections.clear();
    }
    if !selected.contains(&PublicSyncSection::PluginSettings) {
        selection.import_plugin_settings = false;
        selection.selected_plugin_ids.clear();
    }
}

fn pull_selection_has_strict_undo(selection: &CloudSyncPreviewSelection) -> bool {
    !selection.import_connections
        && !selection.import_mosh_profiles
        && !selection.import_sensitive_credentials
}

fn preview_remote_metadata(preview: &CloudSyncPendingPreview) -> &RemoteMetadata {
    match preview {
        CloudSyncPendingPreview::Structured(preview) => &preview.remote_metadata,
        CloudSyncPendingPreview::Legacy { preview, .. } => &preview.remote_metadata,
    }
}

fn preview_summary_projection(summary: &CloudSyncPreviewSummary) -> Value {
    json!({
        "connections": summary.connections,
        "forwards": summary.forwards,
        "quick_commands": summary.quick_commands,
        "serial_profiles": summary.serial_profiles,
        "telnet_profiles": summary.telnet_profiles,
        "mosh_profiles": summary.mosh_profiles,
        "remote_desktop_profiles": summary.remote_desktop_profiles,
        "sensitive_credentials": summary.sensitive_credentials,
        "app_settings_sections": summary.app_settings_sections.iter().map(|section| {
            json!({ "id": section.id, "field_count": section.field_count })
        }).collect::<Vec<_>>(),
        "plugin_settings_count": summary.plugin_settings_count,
        "records": summary.records.iter().map(|record| {
            json!({
                "resource": record.resource,
                "name": record.name,
                "action": record.action,
                "reason_code": record.reason_code,
                "target_name": record.target_name,
            })
        }).collect::<Vec<_>>(),
    })
}

fn local_snapshot_summary(snapshot: &CloudSyncLocalSnapshot) -> Value {
    json!({
        "connections": snapshot.connections_record_count,
        "forwards": snapshot.forwards_record_count,
        "quick_commands": snapshot.quick_commands_record_count,
        "serial_profiles": snapshot.serial_profiles_record_count,
        "telnet_profiles": snapshot.telnet_profiles_record_count,
        "mosh_profiles": snapshot.mosh_profiles_record_count,
        "remote_desktop_profiles": snapshot.remote_desktop_profiles_record_count,
        "sensitive_credentials": snapshot.sensitive_credentials_record_count,
        "upload_units": snapshot.upload_units,
    })
}

fn dirty_sections_projection(snapshot: &CloudSyncLocalSnapshot) -> Vec<&'static str> {
    let dirty = &snapshot.dirty.dirty_sections;
    let mut sections = Vec::new();
    if dirty.connections {
        sections.push("connections");
    }
    if dirty.forwards {
        sections.push("forwards");
    }
    if dirty.quick_commands {
        sections.push("quick_commands");
    }
    if dirty.serial_profiles {
        sections.push("serial_profiles");
    }
    if dirty.telnet_profiles {
        sections.push("telnet_profiles");
    }
    if dirty.mosh_profiles {
        sections.push("mosh_profiles");
    }
    if dirty.remote_desktop_profiles {
        sections.push("remote_desktop_profiles");
    }
    if dirty.sensitive_credentials {
        sections.push("sensitive_credentials");
    }
    if dirty.app_settings.values().any(|value| *value) {
        sections.push("app_settings");
    }
    if dirty.plugin_settings.values().any(|value| *value) {
        sections.push("plugin_settings");
    }
    sections
}

fn apply_outcome_projection(outcome: &CloudSyncApplyOutcome) -> Value {
    match outcome {
        CloudSyncApplyOutcome::Structured(outcome) => json!({
            "format": "structured",
            "connections": outcome.content_summary.connections,
            "forwards": outcome.content_summary.forwards,
            "quick_commands": outcome.content_summary.quick_commands,
            "serial_profiles": outcome.content_summary.serial_profiles,
            "telnet_profiles": outcome.content_summary.telnet_profiles,
            "mosh_profiles": outcome.content_summary.mosh_profiles,
            "remote_desktop_profiles": outcome.content_summary.remote_desktop_profiles,
            "sensitive_credentials": outcome.content_summary.sensitive_credentials,
            "app_settings": outcome.content_summary.has_app_settings,
            "plugin_settings": outcome.content_summary.plugin_settings_count,
        }),
        CloudSyncApplyOutcome::Legacy { outcome, .. } => json!({
            "format": "legacy",
            "connections_imported": outcome.envelope.imported,
            "connections_merged": outcome.envelope.merged,
            "connections_skipped": outcome.envelope.skipped,
            "forwards_imported": outcome.envelope.imported_forwards,
        }),
    }
}

fn cloud_sync_is_configured(
    settings: &CloudSyncSettings,
    secret_hints: &BTreeMap<String, bool>,
) -> bool {
    match settings.backend_type {
        BackendType::S3 => !settings.s3_bucket.trim().is_empty(),
        BackendType::Git => !settings.git_repository.trim().is_empty(),
        BackendType::GithubGist => {
            !settings.git_repository.trim().is_empty()
                || secret_hints
                    .get(secret_keys::GIT_TOKEN)
                    .copied()
                    .unwrap_or(false)
        }
        BackendType::OneDrive => secret_hints
            .get(secret_keys::MICROSOFT_REFRESH_TOKEN)
            .copied()
            .unwrap_or(false),
        BackendType::GoogleDrive => secret_hints
            .get(secret_keys::GOOGLE_REFRESH_TOKEN)
            .copied()
            .unwrap_or(false),
        BackendType::Webdav | BackendType::HttpJson | BackendType::Dropbox => {
            !settings.endpoint.trim().is_empty()
        }
    }
}

fn public_cloud_sync_error(error: &str) -> &'static str {
    let code = error.split_once(':').map_or(error, |(code, _)| code);
    match code.trim() {
        SYNC_CANCELLED_ERROR => "The Cloud Sync operation was cancelled",
        "remote_changed_after_preview" | "remote_changed_before_upload" => {
            "The remote Cloud Sync revision changed after the plan was created"
        }
        "remote_not_found" => "The remote Cloud Sync snapshot does not exist",
        "missing_sync_password" => "The Cloud Sync password is not configured",
        "worker_busy" | "operation_in_progress" => {
            "Another Cloud Sync operation is already running"
        }
        "worker_stopped" => "The Cloud Sync worker stopped before completion",
        _ => "The Cloud Sync operation failed",
    }
}
