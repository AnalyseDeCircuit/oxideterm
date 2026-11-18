use gpui::Context;

use super::{PortableStatusRefresh, WorkspaceApp};

impl WorkspaceApp {
    pub(in crate::workspace::settings) fn handle_portable_settings_action(
        &mut self,
        request: oxideterm_gpui_settings_view::portable_runtime::PortableSettingsRequest,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_settings_view::portable_runtime::PortableSettingsRequest::*;
        match request {
            ToggleAutoUnlock(enabled) => self.set_portable_auto_unlock_enabled(enabled, cx),
            ChangePassword => self.open_portable_password_change_dialog(cx),
            Export => self.open_oxide_export_portable_migration_dialog(cx),
            Import => self.open_oxide_import_portable_migration_dialog(cx),
        }
    }

    pub(in crate::workspace) fn ensure_portable_settings_snapshot(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.refresh_portable_settings_snapshot(false, cx);
    }

    pub(in crate::workspace) fn refresh_portable_settings_snapshot(
        &mut self,
        force: bool,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        let key_store = self.ai_entity.read(cx).key_store().clone();
        let ai_providers = self.settings_store.settings().ai.providers.clone();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.start_portable_status_refresh(
                force,
                runtime,
                move || {
                    let status = oxideterm_portable_runtime::portable_status_snapshot()
                        .map_err(|error| error.to_string());
                    let exportable_secret_count = oxideterm_ai::provider_views(&ai_providers)
                        .into_iter()
                        .filter(|provider| key_store.has_provider_key(&provider.id))
                        .count();
                    PortableStatusRefresh {
                        status,
                        exportable_secret_count,
                    }
                },
                cx,
            );
        });
    }

    pub(in crate::workspace) fn open_portable_password_change_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.ime_marked_text = None;
        self.clear_ime_selection();
        self.settings_workspace.update(cx, |settings, cx| {
            settings.open_portable_password_dialog(cx)
        });
    }

    pub(in crate::workspace) fn close_portable_password_change_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.ime_marked_text = None;
        self.clear_ime_selection();
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_portable_password_dialog(delay, cx);
        });
    }

    pub(in crate::workspace) fn submit_portable_password_change(&mut self, cx: &mut Context<Self>) {
        let runtime = self.forwarding_runtime.clone();
        let dialog_exit_delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        let too_short_error = self
            .i18n
            .t("settings_view.general.portable_password_too_short");
        let mismatch_error = self
            .i18n
            .t("settings_view.general.portable_password_mismatch");
        self.settings_workspace.update(cx, |settings, cx| {
            settings.submit_portable_password_change(
                runtime,
                dialog_exit_delay,
                too_short_error,
                mismatch_error,
                cx,
            );
        });
    }

    pub(in crate::workspace) fn set_portable_auto_unlock_enabled(
        &mut self,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        let runtime = self.forwarding_runtime.clone();
        let action_error = self
            .i18n
            .t("settings_view.general.portable_auto_unlock_action_failed");
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_portable_auto_unlock_enabled(runtime, enabled, action_error, cx);
        });
    }
}
