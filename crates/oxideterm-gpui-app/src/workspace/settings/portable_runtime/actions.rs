impl WorkspaceApp {
    fn ensure_portable_settings_snapshot(&mut self, cx: &mut Context<Self>) {
        if self.portable_status_snapshot.is_none()
            && self.portable_status_error.is_none()
            && self.portable_exportable_secret_count.is_none()
        {
            self.refresh_portable_settings_snapshot(false, cx);
        }
    }

    fn refresh_portable_settings_snapshot(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.portable_settings_refresh_pending {
            return;
        }
        if !force
            && (self.portable_status_snapshot.is_some() || self.portable_status_error.is_some())
            && self.portable_exportable_secret_count.is_some()
        {
            return;
        }

        self.portable_settings_refresh_pending = true;
        let runtime = self.forwarding_runtime.clone();
        let key_store = self.ai_key_store.clone();
        let ai_providers = self.settings_store.settings().ai.providers.clone();

        cx.spawn(async move |weak, cx| {
            let result = runtime
                .spawn_blocking(move || {
                    let status =
                        oxideterm_portable_runtime::portable_status_snapshot()
                            .map_err(|error| error.to_string());
                    let secret_count =
                        oxideterm_ai::provider_views(&ai_providers)
                            .into_iter()
                            .filter(|provider| key_store.has_provider_key(&provider.id))
                            .count();
                    (status, secret_count)
                })
                .await
                .map_err(|error| error.to_string());

            let _ = weak.update(cx, |this, cx| {
                this.portable_settings_refresh_pending = false;
                match result {
                    Ok((Ok(status), secret_count)) => {
                        this.portable_status_snapshot = Some(status);
                        this.portable_status_error = None;
                        this.portable_exportable_secret_count = Some(secret_count);
                    }
                    Ok((Err(error), secret_count)) => {
                        this.portable_status_snapshot = None;
                        this.portable_status_error = Some(error);
                        this.portable_exportable_secret_count = Some(secret_count);
                    }
                    Err(error) => {
                        this.portable_status_snapshot = None;
                        this.portable_status_error = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_portable_password_change_dialog(&mut self, cx: &mut Context<Self>) {
        self.close_portable_password_change_dialog(cx);
        self.portable_settings_dialog = Some(PortableSettingsDialog::ChangePassword);
        self.portable_settings_action_error = None;
        cx.notify();
    }

    fn close_portable_password_change_dialog(&mut self, cx: &mut Context<Self>) {
        self.portable_settings_dialog = None;
        self.portable_settings_action_pending = None;
        self.portable_settings_action_error = None;
        self.focused_settings_input = None;
        self.clear_portable_password_drafts();
        cx.notify();
    }

    fn clear_portable_password_drafts(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.portable_current_password);
        zeroize::Zeroize::zeroize(&mut self.portable_new_password);
        zeroize::Zeroize::zeroize(&mut self.portable_confirm_password);
        self.portable_current_password.clear();
        self.portable_new_password.clear();
        self.portable_confirm_password.clear();
    }

    fn submit_portable_password_change(&mut self, cx: &mut Context<Self>) {
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
        let success_title = self.i18n.t("settings_view.general.portable_password_changed");
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
                        this.portable_settings_dialog = None;
                        this.portable_settings_action_error = None;
                        this.portable_status_snapshot = None;
                        this.portable_status_error = None;
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
