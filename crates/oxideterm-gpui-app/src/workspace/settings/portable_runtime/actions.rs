use gpui::Context;
use zeroize::Zeroizing;

use super::{
    PortablePasswordDialogSnapshot, PortableSettingsAction, PortableSettingsDialog,
    PortableStatusRefresh, SettingsWorkspaceEntity, SettingsWorkspaceEvent, WorkspaceApp,
};

impl SettingsWorkspaceEntity {
    pub(in crate::workspace) fn portable_password_dialog_open(&self) -> bool {
        self.portable_dialog == Some(PortableSettingsDialog::ChangePassword)
    }

    pub(in crate::workspace) fn portable_password_dialog_phase(
        &self,
    ) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.portable_dialog_presence.phase()
    }

    pub(in crate::workspace) fn portable_password_dialog_snapshot(
        &self,
    ) -> PortablePasswordDialogSnapshot {
        PortablePasswordDialogSnapshot {
            open: self.portable_dialog == Some(PortableSettingsDialog::ChangePassword),
            pending: self.portable_action_pending == Some(PortableSettingsAction::ChangePassword),
            error: self.portable_action_error.clone(),
            // Rendering borrows the secret from the Entity and needs only this
            // non-secret flag to enable the submit action.
            current_password_present: !self.portable_current_password.is_empty(),
            presence: self.portable_dialog_presence,
        }
    }

    pub(in crate::workspace) fn open_portable_password_dialog(&mut self, cx: &mut Context<Self>) {
        self.portable_dialog_exit_task = None;
        self.portable_dialog_presence.reopen();
        self.portable_dialog = Some(PortableSettingsDialog::ChangePassword);
        self.portable_action_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn close_portable_password_dialog(
        &mut self,
        delay: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        self.settings_focused_input = None;
        let Some(generation) = self.portable_dialog_presence.begin_exit() else {
            return;
        };
        if delay.is_zero() {
            self.finish_portable_password_dialog_exit(generation, cx);
            return;
        }
        self.portable_dialog_exit_task = Some(cx.spawn(async move |settings, cx| {
            gpui::Timer::after(delay).await;
            let _ = settings.update(cx, |settings, cx| {
                settings.finish_portable_password_dialog_exit(generation, cx);
            });
        }));
        cx.notify();
    }

    pub(in crate::workspace) fn submit_portable_password_change(
        &mut self,
        runtime: std::sync::Arc<tokio::runtime::Runtime>,
        dialog_exit_delay: std::time::Duration,
        too_short_error: String,
        mismatch_error: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.portable_action_pending.is_some() {
            return false;
        }
        if self.portable_new_password.len() < 6 {
            self.portable_action_error = Some(too_short_error);
            cx.notify();
            return false;
        }
        if self.portable_new_password != self.portable_confirm_password {
            self.portable_action_error = Some(mismatch_error);
            cx.notify();
            return false;
        }

        let current_password = std::mem::replace(
            &mut self.portable_current_password,
            Zeroizing::new(String::new()),
        );
        let new_password = std::mem::replace(
            &mut self.portable_new_password,
            Zeroizing::new(String::new()),
        );
        zeroize::Zeroize::zeroize(&mut *self.portable_confirm_password);
        self.settings_focused_input = None;
        self.portable_action_pending = Some(PortableSettingsAction::ChangePassword);
        self.portable_action_error = None;

        self.portable_action_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime
                .spawn_blocking(move || {
                    oxideterm_portable_runtime::keystore::change_portable_keystore_password(
                        current_password.as_str(),
                        new_password.as_str(),
                    )
                    .map_err(|error| error.to_string())?;
                    oxideterm_portable_runtime::portable_status_snapshot()
                        .map(|_| ())
                        .map_err(|error| error.to_string())
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);
            let _ = settings.update(cx, |settings, cx| {
                settings.portable_action_task = None;
                settings.portable_action_pending = None;
                match result {
                    Ok(()) => {
                        settings.portable_action_error = None;
                        settings.invalidate_portable_status(cx);
                        settings.close_portable_password_dialog(dialog_exit_delay, cx);
                        cx.emit(SettingsWorkspaceEvent::PortablePasswordChangeFinished {
                            success: true,
                        });
                    }
                    Err(error) => {
                        settings.portable_action_error = Some(error);
                        cx.emit(SettingsWorkspaceEvent::PortablePasswordChangeFinished {
                            success: false,
                        });
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
        true
    }

    pub(in crate::workspace) fn portable_action_error(&self) -> Option<&str> {
        self.portable_action_error.as_deref()
    }

    pub(in crate::workspace) fn portable_auto_unlock_pending(&self) -> bool {
        self.portable_action_pending == Some(PortableSettingsAction::AutoUnlock)
    }

    pub(in crate::workspace) fn set_portable_auto_unlock_enabled(
        &mut self,
        runtime: std::sync::Arc<tokio::runtime::Runtime>,
        enabled: bool,
        action_error: String,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.portable_action_pending.is_some() {
            return false;
        }
        self.portable_action_pending = Some(PortableSettingsAction::AutoUnlock);
        self.portable_action_error = None;
        self.portable_action_task = Some(cx.spawn(async move |settings, cx| {
            let result = runtime
                .spawn_blocking(move || {
                    if enabled {
                        oxideterm_portable_runtime::keystore::enable_portable_auto_unlock()
                    } else {
                        oxideterm_portable_runtime::keystore::disable_portable_auto_unlock()
                    }
                })
                .await
                .map_err(|_| ())
                .and_then(|result| result.map_err(|_| ()));
            let _ = settings.update(cx, |settings, cx| {
                settings.portable_action_task = None;
                settings.portable_action_pending = None;
                match result {
                    Ok(()) => {
                        settings.portable_action_error = None;
                        settings.invalidate_portable_status(cx);
                    }
                    Err(()) => settings.portable_action_error = Some(action_error),
                }
                cx.notify();
            });
        }));
        cx.notify();
        true
    }

    fn finish_portable_password_dialog_exit(&mut self, generation: u64, cx: &mut Context<Self>) {
        if !self.portable_dialog_presence.finish_exit(generation) {
            return;
        }
        self.portable_dialog_exit_task = None;
        self.portable_dialog = None;
        self.portable_action_pending = None;
        self.portable_action_error = None;
        self.clear_portable_passwords();
        self.portable_dialog_presence.reopen();
        cx.notify();
    }

    fn clear_portable_passwords(&mut self) {
        zeroize::Zeroize::zeroize(&mut *self.portable_current_password);
        zeroize::Zeroize::zeroize(&mut *self.portable_new_password);
        zeroize::Zeroize::zeroize(&mut *self.portable_confirm_password);
    }
}

impl WorkspaceApp {
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext, TestAppContext};
    use oxideterm_gpui_settings_view::SettingsInput;

    use super::*;

    #[gpui::test]
    fn portable_password_focus_edit_and_close_are_entity_owned(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.open_portable_password_dialog(cx);
            assert!(
                settings.focus_settings_entity_input(SettingsInput::PortableCurrentPassword, cx,)
            );
            assert!(settings.replace_settings_entity_input(
                SettingsInput::PortableCurrentPassword,
                None,
                "current-secret",
                cx,
            ));

            let snapshot = settings.portable_password_dialog_snapshot();
            assert!(snapshot.open);
            assert!(snapshot.current_password_present);
            assert_eq!(
                settings.settings_entity_input_value(SettingsInput::PortableCurrentPassword),
                Some("current-secret")
            );
            assert_eq!(
                settings.settings_entity_focused_input(),
                Some(SettingsInput::PortableCurrentPassword)
            );

            settings.close_portable_password_dialog(std::time::Duration::ZERO, cx);
            let snapshot = settings.portable_password_dialog_snapshot();
            assert!(!snapshot.open);
            assert!(!snapshot.current_password_present);
            assert_eq!(
                settings.settings_entity_input_value(SettingsInput::PortableCurrentPassword),
                Some("")
            );
            assert_eq!(
                settings.settings_entity_input_value(SettingsInput::PortableNewPassword),
                Some("")
            );
            assert_eq!(
                settings.settings_entity_input_value(SettingsInput::PortableConfirmPassword),
                Some("")
            );
            assert_eq!(settings.settings_entity_focused_input(), None);
        });
    }

    #[gpui::test]
    fn portable_password_validation_stays_inside_entity(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        let runtime = Arc::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()
                .expect("test runtime"),
        );
        settings.update(cx, |settings, cx| {
            settings.open_portable_password_dialog(cx);
            settings.focus_settings_entity_input(SettingsInput::PortableNewPassword, cx);
            settings.replace_settings_entity_input(
                SettingsInput::PortableNewPassword,
                None,
                "short",
                cx,
            );

            assert!(!settings.submit_portable_password_change(
                runtime,
                std::time::Duration::ZERO,
                "too short".to_string(),
                "mismatch".to_string(),
                cx,
            ));
            assert_eq!(settings.portable_action_error(), Some("too short"));
            assert_eq!(settings.portable_action_pending, None);
        });
    }
}
