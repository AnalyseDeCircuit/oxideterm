use std::time::Instant;

use oxideterm_cloud_sync::{CloudSyncStatus, StructuredLocalState};
use oxideterm_public_mcp::{
    DomainRequest, PublicSyncConflictStrategy, PublicToolCall, SyncSelection, ToolEnvelope,
};
use oxideterm_public_mcp_host::sync_protocol::{
    PublicMcpSyncFailure, capture_undo_checkpoint, finish_sync_publication, full_local_state,
    local_snapshot, public_sync_scope, status_projection,
};
use oxideterm_public_mcp_host::{PublicMcpSyncPlanKind, PublicMcpSyncUndo};
use serde_json::json;

use super::{WorkspaceApp, finish_serialized};

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_sync_status(
        &mut self,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let state = self.cloud_sync.read(cx).controller.store.state().clone();
        let local_snapshot = match local_snapshot(
            &self.connection_store,
            self.forwarding_service.registry(),
            &self.settings_store,
            &state,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                request.finish(ToolEnvelope::failed(error));
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
        finish_serialized(
            request,
            status_projection(
                state,
                &local_snapshot,
                self.cloud_sync.read(cx).operation_in_flight(),
            ),
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
        let worker = self.public_mcp.background().spawn_sync_preview(
            self.cloud_sync.read(cx).controller.service.clone(),
            self.connection_store.clone(),
            self.forwarding_service.registry().clone(),
            self.settings_store.clone(),
            state,
            raw_scope,
            force,
            request.cancellation_token(),
        );
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.clear_public_mcp_sync_action(cx);
                if request.is_cancelled() {
                    return;
                }
                let mut preview = match result {
                    Ok(Ok(preview)) => preview,
                    Ok(Err(error)) => {
                        workspace.fail_public_mcp_sync_action(error, request, cx);
                        return;
                    }
                    Err(_) => {
                        request.finish(ToolEnvelope::failed("Cloud Sync worker stopped"));
                        return;
                    }
                };
                workspace.cloud_sync.update(cx, |cloud_sync, _| {
                    cloud_sync.controller.store.state_mut().secret_hints =
                        std::mem::take(&mut preview.secret_hints);
                });
                workspace
                    .public_mcp
                    .finish_sync_preview(request, preview, local_state, sections);
            });
        })
        .detach();
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
        let checkpoint = match capture_undo_checkpoint(
            &plan.sections,
            &self.connection_store,
            self.forwarding_service.registry(),
            &self.settings_store,
            self.cloud_sync.read(cx).controller.store.state(),
        ) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                self.clear_public_mcp_sync_action(cx);
                request.finish(ToolEnvelope::failed(error));
                return;
            }
        };
        let previous = self.settings_store.settings().clone();
        let applied = match prepared.apply(
            &mut self.connection_store,
            self.forwarding_service.registry(),
            &mut self.settings_store,
        ) {
            Ok(applied) => applied,
            Err(error) => {
                self.fail_public_mcp_sync_action(
                    PublicMcpSyncFailure::from_error(error.to_string()),
                    request,
                    cx,
                );
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
        let worker = self.public_mcp.background().spawn_sync_publication(applied);
        cx.spawn(async move |workspace, cx| {
            let result = worker.await;
            let _ = workspace.update(cx, |workspace, cx| {
                workspace.clear_public_mcp_sync_action(cx);
                match result {
                    Ok(Ok(outcome)) => {
                        workspace.finish_causal_sync(&outcome, cx);
                        finish_sync_publication(request, &outcome, undo_ref);
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
                .return_sync_undo(args.undo_ref.clone(), undo);
            request.finish(ToolEnvelope::failed(
                "Another Cloud Sync operation is already running",
            ));
            return;
        }
        if let Err(error) = undo.restore(
            &mut self.connection_store,
            self.forwarding_service.registry(),
            &mut self.settings_store,
            self.cloud_sync.read(cx).controller.store.state(),
        ) {
            self.public_mcp
                .return_sync_undo(args.undo_ref.clone(), undo);
            request.finish(ToolEnvelope::failed(error));
            return;
        }
        self.cloud_sync.update(cx, |cloud_sync, _| {
            cloud_sync
                .controller
                .store
                .replace_state(undo.checkpoint.cloud_state);
        });
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
        error: PublicMcpSyncFailure,
        request: DomainRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.active_action = None;
        });
        self.finish_cloud_sync_error("sync", error.display_error.to_owned(), cx);
        request.finish(ToolEnvelope::failed(error.message));
    }

    fn public_mcp_full_local_state(&self) -> Result<StructuredLocalState, String> {
        full_local_state(
            &self.connection_store,
            self.forwarding_service.registry(),
            &self.settings_store,
        )
    }
}
