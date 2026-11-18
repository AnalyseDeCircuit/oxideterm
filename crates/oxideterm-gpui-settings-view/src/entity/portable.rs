use super::*;

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

impl SettingsWorkspaceEntity {
    pub fn portable_password_dialog_open(&self) -> bool {
        self.portable_dialog == Some(PortableSettingsDialog::ChangePassword)
    }

    pub fn portable_password_dialog_phase(&self) -> oxideterm_gpui_ui::motion::ExitPhase {
        self.portable_dialog_presence.phase()
    }

    pub fn portable_password_dialog_snapshot(&self) -> PortablePasswordDialogSnapshot {
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

    pub fn open_portable_password_dialog(&mut self, cx: &mut Context<Self>) {
        self.portable_dialog_exit_task = None;
        self.portable_dialog_presence.reopen();
        self.portable_dialog = Some(PortableSettingsDialog::ChangePassword);
        self.portable_action_error = None;
        cx.notify();
    }

    pub fn close_portable_password_dialog(
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

    pub fn submit_portable_password_change(
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

    pub fn portable_action_error(&self) -> Option<&str> {
        self.portable_action_error.as_deref()
    }

    pub fn portable_auto_unlock_pending(&self) -> bool {
        self.portable_action_pending == Some(PortableSettingsAction::AutoUnlock)
    }

    pub fn set_portable_auto_unlock_enabled(
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
