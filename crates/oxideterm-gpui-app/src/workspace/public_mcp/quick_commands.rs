use super::WorkspaceApp;
use gpui::Context;
use oxideterm_public_mcp::DomainRequest;

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_quick_commands_save(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        self.public_mcp
            .handle_public_mcp_quick_commands_save(request, || changed = true);
        if changed {
            self.public_mcp_quick_commands_changed(cx);
        }
    }

    pub(super) fn handle_public_mcp_quick_commands_remove(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        self.public_mcp
            .handle_public_mcp_quick_commands_remove(request, || changed = true);
        if changed {
            self.public_mcp_quick_commands_changed(cx);
        }
    }

    pub(super) fn handle_public_mcp_quick_commands_run(&mut self, request: DomainRequest) {
        let background = self.public_mcp.background().clone();
        self.public_mcp.handle_public_mcp_quick_commands_run(
            request,
            &self.connection_store,
            &background,
        );
    }

    fn public_mcp_quick_commands_changed(&mut self, cx: &mut Context<Self>) {
        self.terminal.update(cx, |terminal, _| {
            terminal.quick_commands.store.reload_from_store()
        });
        self.queue_cloud_sync_dirty_refresh(cx);
        cx.notify();
    }
}
