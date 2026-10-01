use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    time::{Duration, Instant},
};

use oxideterm_cloud_sync::{
    BackendType, CloudSyncSettings, CloudSyncStatus, OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope,
    StructuredLocalState, secret_keys,
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::CloudSyncPersistedState,
};
use oxideterm_public_mcp::{
    ClientRef, DomainRequest, PublicSyncConflictStrategy, PublicSyncSection, PublicToolCall,
    SyncPlanRef, SyncSelection, ToolEnvelope, UndoRef,
};
use serde_json::json;

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
    sections: Vec<PublicSyncSection>,
    kind: PublicMcpSyncPlanKind,
}

enum PublicMcpSyncPlanKind {
    Causal(oxideterm_cloud_sync::operation::PreparedSync),
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
                    .get(state.settings.password_secret_key())
                    .copied()
                    .unwrap_or(false),
                "has_backend_credentials": secret_hints.iter().any(|(key, present)| {
                    key != secret_keys::SYNC_PASSWORD && !key.starts_with("sync-v3-") && *present
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
        if args.conflict_strategy != PublicSyncConflictStrategy::Merge {
            request.finish(ToolEnvelope::failed("Causal synchronization preserves concurrent values. Resolve individual conflicts in the desktop preview."));
            return;
        }
        let selection = args.selection.clone();
        self.start_public_mcp_causal_preview(request, selection, false, cx);
    }

    pub(super) fn handle_public_mcp_sync_publish_preview(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let PublicToolCall::SyncPublishPreview(args) = &request.call else {
            return;
        };
        let selection = args.selection.clone();
        let force = args.force;
        self.start_public_mcp_causal_preview(request, selection, force, cx);
    }

    fn start_public_mcp_causal_preview(
        &mut self,
        request: DomainRequest,
        selection: SyncSelection,
        force: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.public_mcp.expire_sync_handles();
        if !self.begin_public_mcp_sync_action("mcp_sync_preview", CloudSyncStatus::Checking, cx) {
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        let state = self.cloud_sync.read(cx).controller.store.state().clone();
        let (raw_scope, sections) = match public_sync_scope(&state.sync_scope, &selection) {
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
        let mut connections = self.connection_store.clone();
        let forwards = self.forwarding_service.registry().clone();
        let mut settings_store = self.settings_store.clone();
        let service = self.cloud_sync.read(cx).controller.service.clone();
        let scope = oxideterm_cloud_sync::normalize_sync_scope(Some(&raw_scope), &[]);
        let cancellation = request.cancellation_token();
        let worker=self.forwarding_runtime.spawn(async move {
            let mut provider=oxideterm_cloud_sync::secrets::CloudSyncKeychainSecretProvider::new(state.secret_hints);
            let result=tokio::select! {
                _=cancellation.cancelled()=>Err(anyhow::anyhow!("cancelled")),
                result=service.prepare_sync(&mut connections,&forwards,&mut settings_store,&state.settings,&mut provider,scope,Default::default())=>result,
            };
            (result.map_err(|error|error.to_string()),provider.hints().clone())
        });
        cx.spawn(async move |workspace,cx| {
            let worker=worker.await;
            let _=workspace.update(cx,|workspace,cx| {
                workspace.clear_public_mcp_sync_action(cx);
                if request.is_cancelled(){return;}
                let (mut prepared,hints)=match worker {Ok((Ok(prepared),hints))=>(prepared,hints),Ok((Err(error),_))=>{workspace.fail_public_mcp_sync_action("sync",&error,request,cx);return;},Err(_)=>{request.finish(ToolEnvelope::failed("Cloud Sync worker stopped"));return;}};
                if force && let Err(error)=prepared.choose_local_conflicts(){workspace.fail_public_mcp_sync_action("sync",&error.to_string(),request,cx);return;}
                let summary=match prepared.summary(){Ok(summary)=>summary,Err(error)=>{workspace.fail_public_mcp_sync_action("sync",&error.to_string(),request,cx);return;}};
                workspace.cloud_sync.update(cx,|cloud_sync,_|cloud_sync.controller.store.state_mut().secret_hints=hints);
                let plan_ref=workspace.public_mcp.insert_sync_plan(PublicMcpSyncPlan {client_ref:request.client_ref.clone(),created_at:Instant::now(),local_state,sections:sections.clone(),kind:PublicMcpSyncPlanKind::Causal(prepared)});
                cx.spawn(async move |workspace,cx| {gpui::Timer::after(SYNC_PLAN_TTL).await;let _=workspace.update(cx,|workspace,_|workspace.public_mcp.expire_sync_handles());}).detach();
                finish_serialized(request,json!({"sync_plan_ref":plan_ref,"sections":sections,"mode":"causal_merge","changed_fields":summary.changed_fields,"conflicts":summary.conflicts.len(),"upgrading":summary.upgrading,"requires_confirmation":true}));
            });
        }).detach();
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
        if self.public_mcp_full_local_state().ok().as_ref() != Some(&plan.local_state) {
            request.finish(ToolEnvelope::failed(
                "Local synchronized data changed after the plan was created",
            ));
            return;
        }
        if !self.begin_public_mcp_sync_action("mcp_sync_apply", CloudSyncStatus::Uploading, cx) {
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        let PublicMcpSyncPlanKind::Causal(prepared) = plan.kind;
        if !prepared.matches_settings(&self.cloud_sync.read(cx).controller.store.state().settings) {
            self.clear_public_mcp_sync_action(cx);
            request.finish(ToolEnvelope::failed(
                "Cloud Sync configuration changed after the preview",
            ));
            return;
        }
        if request.is_cancelled() {
            self.clear_public_mcp_sync_action(cx);
            return;
        }
        let safe_undo = plan.sections.iter().all(|section| {
            matches!(
                section,
                PublicSyncSection::QuickCommands | PublicSyncSection::PluginSettings
            )
        });
        let checkpoint = if safe_undo {
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
        let previous = self.settings_store.settings().clone();
        let applied = match prepared.apply(
            &mut self.connection_store,
            self.forwarding_service.registry(),
            &mut self.settings_store,
        ) {
            Ok(applied) => applied,
            Err(error) => {
                self.fail_public_mcp_sync_action("sync", &error.to_string(), request, cx);
                return;
            }
        };
        self.refresh_causal_sync_owners(&previous, cx);
        let undo_ref = checkpoint.and_then(|checkpoint| {
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
        });
        // Once local application commits, finish publication even if the caller
        // disconnects. Dropping a request must not undo an acknowledged mutation.
        let worker = self
            .forwarding_runtime
            .spawn(async move { applied.publish().await.map_err(|error| error.to_string()) });
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.clear_public_mcp_sync_action(cx);
                match result {
                    Ok(Ok(outcome)) => {
                        workspace.finish_causal_sync(&outcome, cx);
                        finish_serialized(
                            request,
                            json!({
                                "applied": outcome.applied,
                                "published": outcome.published,
                                "conflicts": outcome.conflicts.len(),
                                "cleanup_pending": outcome.cleanup_pending,
                                "undo_ref": undo_ref,
                                "requires_publish": outcome.publication_pending,
                            }),
                        );
                    }
                    Ok(Err(error)) => {
                        workspace.fail_public_mcp_sync_action("sync", &error, request, cx)
                    }
                    Err(_) => {
                        request.finish(ToolEnvelope::failed(
                            "Cloud Sync publication remains pending",
                        ));
                    }
                }
            });
        })
        .detach();
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
