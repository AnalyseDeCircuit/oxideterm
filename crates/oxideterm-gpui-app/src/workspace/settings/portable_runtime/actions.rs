use gpui::Context;
use oxideterm_gpui_terminal::TerminalNoticeVariant;
use zeroize::Zeroizing;

use super::{PortableSettingsAction, PortableSettingsDialog, PortableStatusRefresh, WorkspaceApp};

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
        self.portable_settings_dialog_presence.reopen();
        self.portable_settings_dialog = Some(PortableSettingsDialog::ChangePassword);
        self.portable_settings_action_error = None;
        cx.notify();
    }

    pub(in crate::workspace) fn close_portable_password_change_dialog(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.focused_settings_input = None;
        let Some(generation) = self.portable_settings_dialog_presence.begin_exit() else {
            return;
        };
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        if delay.is_zero() {
            self.portable_settings_dialog = None;
            self.portable_settings_action_pending = None;
            self.portable_settings_action_error = None;
            self.clear_portable_password_drafts();
            self.portable_settings_dialog_presence.reopen();
            cx.notify();
            return;
        }
        cx.spawn(async move |weak, cx| {
            gpui::Timer::after(delay).await;
            let _ = weak.update(cx, |this, cx| {
                if this
                    .portable_settings_dialog_presence
                    .finish_exit(generation)
                {
                    this.portable_settings_dialog = None;
                    this.portable_settings_action_pending = None;
                    this.portable_settings_action_error = None;
                    this.clear_portable_password_drafts();
                    this.portable_settings_dialog_presence.reopen();
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::workspace) fn clear_portable_password_drafts(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.portable_current_password);
        zeroize::Zeroize::zeroize(&mut self.portable_new_password);
        zeroize::Zeroize::zeroize(&mut self.portable_confirm_password);
        self.portable_current_password.clear();
        self.portable_new_password.clear();
        self.portable_confirm_password.clear();
    }

    pub(in crate::workspace) fn submit_portable_password_change(&mut self, cx: &mut Context<Self>) {
        if self.portable_settings_action_pending.is_some() {
            return;
        }
        if self.portable_new_password.len() < 6 {
            self.portable_settings_action_error = Some(
                self.i18n
                    .t("settings_view.general.portable_password_too_short"),
            );
            cx.notify();
            return;
        }
        if self.portable_new_password != self.portable_confirm_password {
            self.portable_settings_action_error = Some(
                self.i18n
                    .t("settings_view.general.portable_password_mismatch"),
            );
            cx.notify();
            return;
        }

        let current_password = Zeroizing::new(std::mem::take(&mut self.portable_current_password));
        let new_password = Zeroizing::new(std::mem::take(&mut self.portable_new_password));
        zeroize::Zeroize::zeroize(&mut self.portable_confirm_password);
        self.portable_confirm_password.clear();
        self.settings_input_draft.clear();
        self.focused_settings_input = None;
        self.portable_settings_action_pending = Some(PortableSettingsAction::ChangePassword);
        self.portable_settings_action_error = None;

        let runtime = self.forwarding_runtime.clone();
        let success_title = self
            .i18n
            .t("settings_view.general.portable_password_changed");
        cx.spawn(async move |weak, cx| {
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
            let _ = weak.update(cx, |this, cx| {
                this.portable_settings_action_pending = None;
                match result {
                    Ok(()) => {
                        this.close_portable_password_change_dialog(cx);
                        this.portable_settings_action_error = None;
                        this.settings_workspace.update(cx, |settings, cx| {
                            settings.invalidate_portable_status(cx);
                        });
                        this.push_ai_settings_toast(success_title, TerminalNoticeVariant::Success);
                        this.refresh_portable_settings_snapshot(true, cx);
                    }
                    Err(error) => {
                        this.portable_settings_action_error = Some(error.clone());
                        this.push_ai_settings_toast(error, TerminalNoticeVariant::Error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
}
