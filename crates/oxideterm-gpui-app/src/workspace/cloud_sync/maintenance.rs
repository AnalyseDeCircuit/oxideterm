// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn clear_cloud_sync_secret(&mut self, secret_key: &str, cx: &mut Context<Self>) {
        self.invalidate_cloud_sync_snapshot_caches(cx);
        let mut provider = CloudSyncKeychainSecretProvider::new(
            self.cloud_sync
                .read(cx)
                .controller
                .store
                .state()
                .secret_hints
                .clone(),
        );
        if let Err(error) = provider.store_secret(secret_key, None) {
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.controller.store.state_mut().last_error = Some(error.to_string());
            });
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.secret_cleared_failed_title"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        }
        let save_result = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.store.state_mut().secret_hints = provider.hints().clone();
            cloud_sync.controller.store.state_mut().last_error = None;
            cloud_sync.controller.store.save()
        });
        if let Err(error) = save_result {
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.controller.store.state_mut().last_error = Some(error.to_string());
            });
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.secret_cleared_failed_title"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
                cx,
            );
        } else {
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.secret_cleared_title"),
                None,
                TerminalNoticeVariant::Success,
                cx,
            );
        }
    }

    pub(super) fn push_cloud_sync_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    pub(super) fn finish_cloud_sync_scope_edit(&mut self, cx: &mut Context<Self>) {
        // Scope edits are Entity-owned; the root only refreshes external source
        // projections and persists the resulting Cloud Sync state.
        self.refresh_cloud_sync_local_dirty_state(cx);
        self.save_cloud_sync_state(cx);
        cx.notify();
    }

    pub(super) fn open_cloud_sync_import_confirm(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.read(cx).view.pending_preview.is_none() {
            return;
        }
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.confirm = Some(CloudSyncConfirm::ImportPreview);
            cloud_sync.view.confirm_presence.reopen();
            cloud_sync.view.confirm_focused_action = None;
        });
    }

    pub(super) fn open_cloud_sync_force_upload_confirm(&mut self, cx: &mut Context<Self>) {
        let has_preview = {
            let cloud_sync = self.cloud_sync.read(cx);
            cloud_sync.view.upload_preview.is_some() || cloud_sync.view.pending_preview.is_some()
        };
        if !has_preview {
            return;
        }
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.confirm = Some(CloudSyncConfirm::ForceUpload);
            cloud_sync.view.confirm_presence.reopen();
            cloud_sync.view.confirm_focused_action = None;
        });
    }

    pub(super) fn open_cloud_sync_restore_confirm(
        &mut self,
        backup: Option<(String, String)>,
        cx: &mut Context<Self>,
    ) {
        let selected = backup.or_else(|| {
            self.cloud_sync
                .read(cx)
                .controller
                .store
                .state()
                .rollback_backups
                .first()
                .map(|backup| (backup.id.clone(), backup.created_at.clone()))
        });
        if let Some((id, created_at)) = selected {
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.view.confirm = Some(CloudSyncConfirm::RestoreBackup { id, created_at });
                cloud_sync.view.confirm_presence.reopen();
                cloud_sync.view.confirm_focused_action = None;
            });
        }
    }

    pub(super) fn open_cloud_sync_delete_backup_confirm(
        &mut self,
        id: String,
        created_at: String,
        cx: &mut Context<Self>,
    ) {
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.confirm = Some(CloudSyncConfirm::DeleteBackup { id, created_at });
            cloud_sync.view.confirm_presence.reopen();
            cloud_sync.view.confirm_focused_action = None;
        });
    }

    pub(super) fn open_cloud_sync_clear_backups_confirm(&mut self, cx: &mut Context<Self>) {
        if self
            .cloud_sync
            .read(cx)
            .controller
            .store
            .state()
            .rollback_backups
            .is_empty()
        {
            return;
        }
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.confirm = Some(CloudSyncConfirm::ClearBackups);
            cloud_sync.view.confirm_presence.reopen();
            cloud_sync.view.confirm_focused_action = None;
        });
    }

    pub(super) fn open_cloud_sync_clear_history_confirm(&mut self, cx: &mut Context<Self>) {
        if self
            .cloud_sync
            .read(cx)
            .controller
            .store
            .state()
            .sync_history
            .is_empty()
        {
            return;
        }
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.confirm = Some(CloudSyncConfirm::ClearHistory);
            cloud_sync.view.confirm_presence.reopen();
            cloud_sync.view.confirm_focused_action = None;
        });
    }

    pub(super) fn cancel_cloud_sync_confirm(&mut self, cx: &mut Context<Self>) {
        self.begin_cloud_sync_confirm_exit(cx);
    }

    pub(super) fn confirm_cloud_sync_confirm(&mut self, cx: &mut Context<Self>) {
        let confirm = self.cloud_sync.read(cx).view.confirm.clone();
        if !self.begin_cloud_sync_confirm_exit(cx) {
            return;
        }
        match confirm {
            Some(CloudSyncConfirm::ImportPreview) => self.start_cloud_sync_apply_preview(cx),
            Some(CloudSyncConfirm::ForceUpload) => {
                self.start_cloud_sync_upload_with_options(true, false, false, cx)
            }
            Some(CloudSyncConfirm::ClearSecret { key, .. }) => {
                self.clear_cloud_sync_secret(&key, cx)
            }
            Some(CloudSyncConfirm::RestoreBackup { id, .. }) => {
                self.start_cloud_sync_restore_backup(id, cx)
            }
            Some(CloudSyncConfirm::DeleteBackup { id, .. }) => {
                self.delete_cloud_sync_rollback_backup(&id, cx)
            }
            Some(CloudSyncConfirm::ClearBackups) => self.clear_cloud_sync_rollback_backups(cx),
            Some(CloudSyncConfirm::ClearHistory) => self.clear_cloud_sync_history(cx),
            Some(CloudSyncConfirm::EnableSensitiveSync) => {
                self.cloud_sync.update(cx, |cloud_sync, _cx| {
                    cloud_sync
                        .controller
                        .store
                        .state_mut()
                        .sync_scope
                        .sync_sensitive_credentials = Some(true);
                });
                self.finish_cloud_sync_scope_edit(cx);
            }
            None => {}
        }
    }

    fn begin_cloud_sync_confirm_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(generation) = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.confirm_presence.begin_exit()
        }) else {
            return false;
        };
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.view.confirm_focused_action = None;
        });
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if delay.is_zero() {
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                if cloud_sync.view.confirm_presence.finish_exit(generation) {
                    cloud_sync.view.confirm = None;
                }
            });
            return true;
        }
        // Keep the immutable confirmation payload mounted for the exit frame.
        self.cloud_sync.update(cx, |cloud_sync, cx| {
            cloud_sync.schedule_confirm_exit(generation, delay, cx);
        });
        true
    }

    pub(super) fn delete_cloud_sync_rollback_backup(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.cloud_sync.read(cx).controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress(cx);
            return;
        }
        let removed = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync
                .controller
                .store
                .state_mut()
                .remove_rollback_backup(id)
        });
        if removed {
            self.clear_cloud_sync_preview_for_deleted_backup(id, cx);
            self.save_cloud_sync_state(cx);
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.rollback_backup_deleted_title"),
                None,
                TerminalNoticeVariant::Success,
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn clear_cloud_sync_rollback_backups(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.read(cx).controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress(cx);
            return;
        }
        let removed = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync
                .controller
                .store
                .state_mut()
                .clear_rollback_backups()
        });
        if removed > 0 {
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.view.pending_preview = cloud_sync
                    .view
                    .pending_preview
                    .take()
                    .filter(|preview| !preview.is_backup());
                cloud_sync.view.preview_selection = None;
            });
            self.save_cloud_sync_state(cx);
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.rollback_backups_cleared_title"),
                None,
                TerminalNoticeVariant::Success,
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn clear_cloud_sync_history(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.read(cx).controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress(cx);
            return;
        }
        let removed = self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.controller.store.state_mut().clear_history()
        });
        if removed > 0 {
            self.save_cloud_sync_state(cx);
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.history_cleared_title"),
                None,
                TerminalNoticeVariant::Success,
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn clear_cloud_sync_preview_for_deleted_backup(
        &mut self,
        backup_id: &str,
        cx: &mut Context<Self>,
    ) {
        // A deleted backup cannot remain selected as the pending import preview.
        let pending_matches_deleted_backup = self
            .cloud_sync
            .read(cx)
            .view
            .pending_preview
            .as_ref()
            .is_some_and(|preview| match preview {
                CloudSyncPendingPreview::Legacy {
                    source: CloudSyncPreviewSource::Backup { id, .. },
                    ..
                } => id.as_str() == backup_id,
                _ => false,
            });
        if pending_matches_deleted_backup {
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.view.pending_preview = None;
                cloud_sync.view.preview_selection = None;
            });
        }
    }
}
