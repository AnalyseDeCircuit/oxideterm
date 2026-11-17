use super::*;

impl CloudSyncRuntime<'_> {
    pub(super) fn save_cloud_sync_configuration(&mut self, cx: &mut App) {
        self.persist_cloud_sync_configuration(true, cx);
    }

    pub fn persist_cloud_sync_configuration(
        &mut self,
        show_success_toast: bool,
        cx: &mut App,
    ) -> bool {
        if self.cloud_sync.read(cx).operation_in_flight() {
            return false;
        }
        self.cancel_causal_sync(cx);
        self.invalidate_cloud_sync_snapshot_caches(cx);
        let (mut settings, interval) = {
            let cloud_sync = self.cloud_sync.read(cx);
            cloud_sync_settings_from_form(&cloud_sync.view.form)
        };
        settings.local_file_mode = self.cloud_sync.read(cx).view.local_file_mode;
        settings.sync_password_ref = self
            .cloud_sync
            .read(cx)
            .controller
            .store
            .state()
            .settings
            .sync_password_ref
            .clone();
        let current = self
            .cloud_sync
            .read(cx)
            .controller
            .store
            .state()
            .settings
            .clone();
        let destination_changed = !settings.same_destination(&current);
        let mut provider = CloudSyncKeychainSecretProvider::new(
            self.cloud_sync
                .read(cx)
                .controller
                .store
                .state()
                .secret_hints
                .clone(),
        );
        let changing_password = (|| -> anyhow::Result<bool> {
            let cloud_sync = self.cloud_sync.read(cx);
            let state = cloud_sync.controller.store.state();
            let password_touched = !settings.local_file_mode
                && !destination_changed
                && cloud_sync.view.form.sync_password_touched
                && state
                    .secret_hints
                    .get(
                        state
                            .settings
                            .sync_password_ref
                            .as_deref()
                            .unwrap_or(secret_keys::SYNC_PASSWORD),
                    )
                    .copied()
                    .unwrap_or(false);
            if !password_touched {
                return Ok(false);
            }
            // Restoring the bound password is a settings repair, not another rotation.
            let requires_rotation = oxideterm_cloud_sync::operation::CloudSyncOperationService::local_sync_password_requires_rotation(
                self.settings_store.path(),
                &current,
                &mut provider,
                cloud_sync.view.form.sync_password.trim(),
            )?;
            Ok(requires_rotation.unwrap_or(state.remote_exists))
        })();
        let changing_password = match changing_password {
            Ok(value) => value,
            Err(error) => {
                self.finish_cloud_sync_error("sync", error.to_string(), cx);
                return false;
            }
        };
        if changing_password {
            self.cloud_sync.update(cx, |cloud_sync, cx| {
                cloud_sync.view.confirm = Some(CloudSyncConfirm::ChangePassword);
                cloud_sync.view.confirm_presence.reopen();
                cloud_sync.view.confirm_focused_action = None;
                cx.notify();
            });
            return false;
        }
        let mut secret_handoff = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.form.take_secret_handoff()
        });
        let secret_result = (|| -> anyhow::Result<()> {
            if !settings.local_file_mode {
                if let Some(value) = secret_handoff.sync_password.as_deref() {
                    let reference = format!("sync-v3-password-{}", uuid::Uuid::new_v4());
                    provider.store_secret(&reference, crate::non_empty_secret(value))?;
                    settings.sync_password_ref = Some(reference);
                }
                secret_handoff.sync_password = None;
            }
            store_cloud_sync_touched_secrets(&secret_handoff, &mut provider)
        })();
        if let Err(error) = secret_result {
            // Credential-store failures contain operation context and platform
            // status details, never the submitted secret values.
            let error_message = format!("{error:#}");
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.view.form.restore_secret_handoff(secret_handoff);
                cloud_sync.controller.store.state_mut().last_error = Some(error_message.clone());
            });
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.settings_saved_failed_title"),
                Some(error_message),
                CloudSyncNoticeKind::Error,
                cx,
            );
            return false;
        }
        let save_result = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            let previous = cloud_sync.controller.store.state().clone();
            cloud_sync.controller.store.state_mut().settings = settings;
            cloud_sync.controller.store.state_mut().secret_hints = provider.hints().clone();
            if destination_changed {
                oxideterm_cloud_sync::state_transitions::reset_destination_state(
                    cloud_sync.controller.store.state_mut(),
                );
            }
            let result = cloud_sync.controller.store.save();
            if result.is_err() {
                *cloud_sync.controller.store.state_mut() = previous;
            }
            result
        });
        if let Err(error) = save_result {
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.controller.store.state_mut().last_error = Some(error.to_string());
            });
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.settings_saved_failed_title"),
                Some(error.to_string()),
                CloudSyncNoticeKind::Error,
                cx,
            );
            return false;
        } else {
            // Successful synchronous persistence drops and zeroizes the
            // transferred buffers before the UI draft is normalized.
            drop(secret_handoff);
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                normalize_cloud_sync_interval_draft(&mut cloud_sync.view.form, interval);
                reset_cloud_sync_secret_drafts(&mut cloud_sync.view.form);
                cloud_sync.controller.store.state_mut().last_error = None;
            });
            if show_success_toast {
                self.push_cloud_sync_toast(
                    self.i18n.t("plugin.cloud_sync.toast.settings_saved_title"),
                    None,
                    CloudSyncNoticeKind::Success,
                    cx,
                );
            }
            self.reschedule_cloud_sync_auto_upload(cx);
            self.queue_cloud_sync_dirty_refresh(cx);
        }
        true
    }
}
