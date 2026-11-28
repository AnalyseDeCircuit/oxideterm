use super::*;
use oxideterm_gpui_settings_view::credentials_page::CredentialRequest;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_privilege_credentials_settings(
        &mut self,
        scope_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(SettingsTab::Privilege, cx);
        });
        if let Some(scope_id) = scope_id {
            self.settings_workspace.update(cx, |settings, cx| {
                settings.set_privilege_scope(scope_id, cx)
            });
            self.close_settings_select();
        }
        self.open_settings(window, cx);
        cx.notify();
    }

    pub(super) fn handle_credential_request(
        &mut self,
        request: CredentialRequest,
        cx: &mut Context<Self>,
    ) {
        let outcome = self.settings_workspace.update(cx, |settings, cx| {
            settings.apply_credential_request(
                request,
                &self.settings_store.settings().discovery_sources,
                &mut self.connection_store,
                &self.i18n,
                cx,
            )
        });
        if outcome.close_select {
            self.close_settings_select();
        }
        if outcome.clear_ime_selection {
            self.clear_ime_selection();
        }
        if outcome.store_changed {
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }
}
