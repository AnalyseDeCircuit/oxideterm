use crate::workspace::terminal_command_bar::{
    SavedConsoleProfileKind, delete_saved_console_profile,
};
use crate::workspace::{TerminalHistoryStore, WorkspaceApp};
use gpui::Context;
use oxideterm_connections::ConnectionStore;
use oxideterm_public_mcp::DomainRequest;
use oxideterm_public_mcp_host::{
    CONNECTION_KEY_DESKTOP_PREFIX, CONNECTION_KEY_MOSH_PREFIX, CONNECTION_KEY_SERIAL_PREFIX,
    CONNECTION_KEY_SSH_PREFIX, CONNECTION_KEY_TELNET_PREFIX, public_store_error,
};

impl WorkspaceApp {
    pub(super) fn handle_public_mcp_connection_directory(&mut self, request: DomainRequest) {
        let background = self.public_mcp.background().clone();
        self.public_mcp.read_connection_directory(&background, &self.connection_store, request);
    }

    pub(super) fn handle_public_mcp_save_connection(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        self.public_mcp.handle_public_mcp_save_connection(
            request,
            &mut self.connection_store,
            || changed = true,
        );
        if changed {
            self.queue_cloud_sync_dirty_refresh(cx);
            cx.notify();
        }
    }

    pub(super) fn handle_public_mcp_remove_connection(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        self.public_mcp.handle_public_mcp_remove_connection(
            request,
            &mut self.connection_store,
            || changed = true,
            |store, key| remove_profile(store, &self.terminal_history_store, key),
        );
        if changed {
            self.queue_cloud_sync_dirty_refresh(cx);
            cx.notify();
        }
    }

    pub(super) fn handle_public_mcp_credential_status(&mut self, request: DomainRequest) {
        self.public_mcp
            .handle_public_mcp_credential_status(request, &self.connection_store);
    }

    pub(super) fn handle_public_mcp_store_credential(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        self.public_mcp.handle_public_mcp_store_credential(
            request,
            &mut self.connection_store,
            || changed = true,
        );
        if changed {
            self.queue_cloud_sync_dirty_refresh(cx);
            cx.notify();
        }
    }

    pub(super) fn handle_public_mcp_forget_credential(
        &mut self,
        request: DomainRequest,
        cx: &mut Context<Self>,
    ) {
        let mut changed = false;
        self.public_mcp.handle_public_mcp_forget_credential(
            request,
            &mut self.connection_store,
            || changed = true,
        );
        if changed {
            self.queue_cloud_sync_dirty_refresh(cx);
            cx.notify();
        }
    }
}

fn remove_profile(
    store: &mut ConnectionStore,
    terminal_history_store: &TerminalHistoryStore,
    key: &str,
) -> Result<bool, String> {
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_SSH_PREFIX) {
        return store.delete(id).map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_SERIAL_PREFIX) {
        return delete_saved_console_profile(
            store,
            terminal_history_store,
            SavedConsoleProfileKind::Serial,
            id,
        )
        .map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_TELNET_PREFIX) {
        return delete_saved_console_profile(
            store,
            terminal_history_store,
            SavedConsoleProfileKind::Telnet,
            id,
        )
        .map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_MOSH_PREFIX) {
        return delete_saved_console_profile(
            store,
            terminal_history_store,
            SavedConsoleProfileKind::Mosh,
            id,
        )
        .map_err(public_store_error);
    }
    if let Some(id) = key.strip_prefix(CONNECTION_KEY_DESKTOP_PREFIX) {
        return store
            .delete_remote_desktop_profile(id)
            .map_err(public_store_error);
    }
    Err("The saved connection type is unsupported".to_owned())
}
