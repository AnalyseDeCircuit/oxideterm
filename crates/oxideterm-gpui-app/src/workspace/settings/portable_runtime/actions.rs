use gpui::Context;
use oxideterm_gpui_settings_view::SettingsInput;
use zeroize::Zeroizing;

use super::{
    PortablePasswordDialogSnapshot, PortableSettingsAction, PortableSettingsDialog,
    PortableStatusRefresh, SettingsWorkspaceEntity, SettingsWorkspaceEvent, WorkspaceApp,
};

fn is_portable_password_input(input: SettingsInput) -> bool {
    matches!(
        input,
        SettingsInput::PortableCurrentPassword
            | SettingsInput::PortableNewPassword
            | SettingsInput::PortableConfirmPassword
    )
}

impl SettingsWorkspaceEntity {
    pub(in crate::workspace) fn portable_password_dialog_snapshot(
        &self,
    ) -> PortablePasswordDialogSnapshot {
        PortablePasswordDialogSnapshot {
            open: self.portable_dialog == Some(PortableSettingsDialog::ChangePassword),
            pending: self.portable_action_pending == Some(PortableSettingsAction::ChangePassword),
            error: self.portable_action_error.clone(),
            // GPUI text layout requires owned frame data; every secret copy is zeroized.
            current_password: Zeroizing::new(self.portable_current_password.to_string()),
            new_password: Zeroizing::new(self.portable_new_password.to_string()),
            confirm_password: Zeroizing::new(self.portable_confirm_password.to_string()),
            presence: self.portable_dialog_presence,
        }
    }

    pub(in crate::workspace) fn portable_focused_input(&self) -> Option<SettingsInput> {
        self.portable_focused_input
    }

    pub(in crate::workspace) fn portable_input_value(&self, input: SettingsInput) -> Option<&str> {
        match input {
            SettingsInput::PortableCurrentPassword => Some(&self.portable_current_password),
            SettingsInput::PortableNewPassword => Some(&self.portable_new_password),
            SettingsInput::PortableConfirmPassword => Some(&self.portable_confirm_password),
            _ => None,
        }
    }

    pub(in crate::workspace) fn focus_portable_password_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if !is_portable_password_input(input)
            || self.portable_dialog != Some(PortableSettingsDialog::ChangePassword)
        {
            return false;
        }
        self.portable_focused_input = Some(input);
        cx.notify();
        true
    }

    pub(in crate::workspace) fn blur_portable_password_input(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let changed = self.portable_focused_input.take().is_some();
        if changed {
            cx.notify();
        }
        changed
    }

    pub(in crate::workspace) fn replace_portable_password_input(
        &mut self,
        input: SettingsInput,
        replacement_range: Option<std::ops::Range<usize>>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.portable_focused_input != Some(input) {
            return false;
        }
        let Some(value) = self.portable_password_mut(input) else {
            return false;
        };
        oxideterm_editor_core::utf16::replace_utf16(value, replacement_range, text);
        self.portable_action_error = None;
        cx.notify();
        true
    }

    pub(in crate::workspace) fn pop_portable_password_input(
        &mut self,
        input: SettingsInput,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.portable_focused_input != Some(input) {
            return false;
        }
        let Some(value) = self.portable_password_mut(input) else {
            return false;
        };
        value.pop();
        self.portable_action_error = None;
        cx.notify();
        true
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
        self.portable_focused_input = None;
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
        self.portable_focused_input = None;
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

    fn portable_password_mut(&mut self, input: SettingsInput) -> Option<&mut String> {
        match input {
            SettingsInput::PortableCurrentPassword => Some(&mut self.portable_current_password),
            SettingsInput::PortableNewPassword => Some(&mut self.portable_new_password),
            SettingsInput::PortableConfirmPassword => Some(&mut self.portable_confirm_password),
            _ => None,
        }
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
        let key_store = self.ai.models.key_store.clone();
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
        if let Some(input) = self.focused_settings_input.take() {
            self.clear_settings_input_draft(input);
        }
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gpui::{AppContext, TestAppContext};

    use super::*;

    #[gpui::test]
    fn portable_password_focus_edit_and_close_are_entity_owned(cx: &mut TestAppContext) {
        let settings = cx.new(SettingsWorkspaceEntity::new);
        settings.update(cx, |settings, cx| {
            settings.open_portable_password_dialog(cx);
            assert!(
                settings.focus_portable_password_input(SettingsInput::PortableCurrentPassword, cx,)
            );
            assert!(settings.replace_portable_password_input(
                SettingsInput::PortableCurrentPassword,
                None,
                "current-secret",
                cx,
            ));

            let snapshot = settings.portable_password_dialog_snapshot();
            assert!(snapshot.open);
            assert_eq!(snapshot.current_password.as_str(), "current-secret");
            assert_eq!(
                settings.portable_focused_input(),
                Some(SettingsInput::PortableCurrentPassword)
            );

            settings.close_portable_password_dialog(std::time::Duration::ZERO, cx);
            let snapshot = settings.portable_password_dialog_snapshot();
            assert!(!snapshot.open);
            assert!(snapshot.current_password.is_empty());
            assert!(snapshot.new_password.is_empty());
            assert!(snapshot.confirm_password.is_empty());
            assert_eq!(settings.portable_focused_input(), None);
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
            settings.focus_portable_password_input(SettingsInput::PortableNewPassword, cx);
            settings.replace_portable_password_input(
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
