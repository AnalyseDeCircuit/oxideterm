use super::WorkspaceApp;
use gpui::Context;
use oxideterm_cloud_sync::operation::CloudSyncOperationService;
use oxideterm_public_mcp::DomainRequest;
use oxideterm_public_mcp_host::sync_protocol::{
    PublicMcpSyncEffect, PublicMcpSyncOwner, PublicMcpSyncStores,
};

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_sync(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let backend = self.public_mcp.background().clone();
        let Some(pending) = backend.sync_request(
            request,
            &mut SyncOwner {
                workspace: self,
                cx,
            },
        ) else {
            return;
        };
        cx.spawn(async move |workspace, cx| {
            let completion = pending.complete().await;
            let _ = workspace.update(cx, |workspace, cx| {
                backend.complete_sync(completion, &mut SyncOwner { workspace, cx });
            });
        })
        .detach();
    }
}

struct SyncOwner<'a, 'cx> {
    workspace: &'a mut WorkspaceApp,
    cx: &'a mut Context<'cx, WorkspaceApp>,
}

impl PublicMcpSyncOwner for SyncOwner<'_, '_> {
    fn stores(&mut self) -> PublicMcpSyncStores<'_> {
        let workspace = &mut self.workspace;
        PublicMcpSyncStores {
            connections: &mut workspace.connection_store,
            forwards: workspace.forwarding_service.registry(),
            settings: &mut workspace.settings_store,
            cloud: workspace.cloud_sync.read(self.cx).controller.store.state(),
        }
    }

    fn service(&self) -> CloudSyncOperationService {
        self.workspace
            .cloud_sync
            .read(self.cx)
            .controller
            .service
            .clone()
    }

    fn busy(&self) -> bool {
        self.workspace
            .cloud_sync
            .read(self.cx)
            .operation_in_flight()
    }

    fn effect(&mut self, effect: PublicMcpSyncEffect<'_>) {
        use PublicMcpSyncEffect::*;
        let (workspace, cx) = (&mut *self.workspace, &mut *self.cx);
        match effect {
            State(change) => {
                let save = workspace.cloud_sync.update(cx, |cloud, _| {
                    let controller = &mut cloud.controller;
                    let (save, clear_progress) =
                        change.apply(controller.store.state_mut(), &mut controller.active_action);
                    if clear_progress {
                        controller.progress = None;
                    }
                    save
                });
                if save {
                    workspace.save_cloud_sync_state(cx);
                }
            }
            Applied(previous) => workspace.refresh_causal_sync_owners(previous, cx),
            Published(outcome) => workspace.finish_causal_sync(outcome, cx),
            Restored(state) => {
                workspace
                    .cloud_sync
                    .update(cx, |cloud, _| cloud.controller.store.replace_state(state));
                workspace.terminal.update(cx, |terminal, _| {
                    terminal.quick_commands.store.reload_from_store()
                });
                workspace.bootstrap_native_plugin_runtime(cx);
                workspace.invalidate_cloud_sync_snapshot_caches(cx);
                workspace.refresh_cloud_sync_local_dirty_state(cx);
                workspace.save_cloud_sync_state(cx);
                cx.notify();
            }
            Failed(error) => {
                workspace
                    .cloud_sync
                    .update(cx, |cloud, _| cloud.controller.active_action = None);
                workspace.finish_cloud_sync_error("sync", error.to_owned(), cx);
            }
        }
    }
}
