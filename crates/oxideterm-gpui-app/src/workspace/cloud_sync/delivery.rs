// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn bootstrap_cloud_sync_controller(&mut self, cx: &mut Context<Self>) {
        self.cloud_sync
            .controller
            .store
            .state_mut()
            .ensure_device_id(cloud_sync_platform_label());
        self.refresh_cloud_sync_local_dirty_state();
        self.save_cloud_sync_state();
        self.reschedule_cloud_sync_auto_upload(cx);
        let settings = self.cloud_sync.controller.store.state().settings.clone();
        if backend_uses_auth_mode(&settings.backend_type)
            && !settings.endpoint.trim().is_empty()
            && matches!(settings.auth_mode, AuthMode::None)
        {
            self.start_cloud_sync_check_with_options(true, cx);
        }
    }

    pub(super) fn reschedule_cloud_sync_auto_upload(&mut self, cx: &mut Context<Self>) {
        self.cloud_sync.controller.auto_upload_generation = self
            .cloud_sync
            .controller
            .auto_upload_generation
            .wrapping_add(1);
        if !self
            .cloud_sync
            .controller
            .store
            .state()
            .settings
            .auto_upload_enabled
        {
            return;
        }
        let generation = self.cloud_sync.controller.auto_upload_generation;
        cx.spawn(async move |weak, cx| {
            loop {
                let wait = weak
                    .update(cx, |this, _cx| {
                        if this.cloud_sync.controller.auto_upload_generation != generation
                            || !this
                                .cloud_sync
                                .controller
                                .store
                                .state()
                                .settings
                                .auto_upload_enabled
                        {
                            return None;
                        }
                        let interval = this
                            .cloud_sync
                            .controller
                            .store
                            .state()
                            .settings
                            .auto_upload_interval_mins
                            .max(5.0);
                        Some(Duration::from_secs_f64(interval * 60.0))
                    })
                    .ok()
                    .flatten();
                let Some(wait) = wait else {
                    break;
                };
                Timer::after(wait).await;
                let keep_running = weak
                    .update(cx, |this, cx| {
                        if this.cloud_sync.controller.auto_upload_generation != generation
                            || !this
                                .cloud_sync
                                .controller
                                .store
                                .state()
                                .settings
                                .auto_upload_enabled
                        {
                            return false;
                        }
                        this.refresh_cloud_sync_local_dirty_state();
                        let state = this.cloud_sync.controller.store.state();
                        if !state.local_dirty
                            || state.auto_upload_blocked_by_conflict
                            || state.status == CloudSyncStatus::Uploading
                        {
                            this.save_cloud_sync_state();
                            return true;
                        }
                        this.start_cloud_sync_upload_with_options(false, true, true, cx);
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn refresh_cloud_sync_local_dirty_state(&mut self) {
        self.invalidate_cloud_sync_snapshot_caches();
        let Ok(snapshot) = self.cloud_sync_local_snapshot(self.cloud_sync.controller.store.state())
        else {
            return;
        };
        self.cloud_sync.controller.store.state_mut().local_dirty = snapshot.dirty.has_dirty;
        self.cloud_sync
            .controller
            .store
            .state_mut()
            .local_dirty_sections = Some(snapshot.dirty.dirty_sections);
    }

    pub(in crate::workspace) fn queue_cloud_sync_dirty_refresh(&mut self, cx: &mut Context<Self>) {
        // Invalidate immediately at the mutation boundary; the debounced refresh
        // may run later, but visible preview data must not reuse the old generation.
        self.invalidate_cloud_sync_snapshot_caches();
        self.cloud_sync.controller.dirty_refresh_generation = self
            .cloud_sync
            .controller
            .dirty_refresh_generation
            .wrapping_add(1);
        let generation = self.cloud_sync.controller.dirty_refresh_generation;
        self.cloud_sync.controller.dirty_refresh_scheduled = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(300)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.cloud_sync.controller.dirty_refresh_generation != generation {
                    return;
                }
                this.cloud_sync.controller.dirty_refresh_scheduled = false;
                this.refresh_cloud_sync_local_dirty_state();
                this.save_cloud_sync_state();
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn start_cloud_sync_check(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        if !self.persist_cloud_sync_configuration(false, cx) {
            return;
        }
        self.start_cloud_sync_check_with_options(false, cx);
    }

    pub(super) fn start_cloud_sync_github_oauth(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        self.save_cloud_sync_configuration(cx);
        let client_id = self
            .cloud_sync
            .controller
            .store
            .state()
            .settings
            .github_oauth_client_id
            .trim()
            .to_string();
        if client_id.is_empty() {
            self.finish_cloud_sync_error(
                "github_oauth",
                "missing_github_oauth_client_id: GitHub OAuth client ID is not configured"
                    .to_string(),
            );
            return;
        }
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("github_oauth");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime
            .spawn(deliver_cloud_sync_github_oauth(tx, client_id, hints));
    }

    pub(super) fn start_cloud_sync_microsoft_oauth(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        self.save_cloud_sync_configuration(cx);
        let client_id = self
            .cloud_sync
            .controller
            .store
            .state()
            .settings
            .microsoft_oauth_client_id
            .trim()
            .to_string();
        if client_id.is_empty() {
            self.finish_cloud_sync_error(
                "microsoft_oauth",
                "missing_microsoft_oauth_client_id: Microsoft OAuth client ID is not configured"
                    .to_string(),
            );
            return;
        }
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("microsoft_oauth");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime
            .spawn(deliver_cloud_sync_microsoft_oauth(tx, client_id, hints));
    }

    pub(super) fn start_cloud_sync_google_oauth(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        self.save_cloud_sync_configuration(cx);
        let client_id = self
            .cloud_sync
            .controller
            .store
            .state()
            .settings
            .google_oauth_client_id
            .trim()
            .to_string();
        if client_id.is_empty() {
            self.finish_cloud_sync_error(
                "google_oauth",
                "missing_google_oauth_client_id: Google OAuth client ID is not configured"
                    .to_string(),
            );
            return;
        }
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("google_oauth");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime
            .spawn(deliver_cloud_sync_google_oauth(tx, client_id, hints));
    }

    pub(super) fn start_cloud_sync_check_with_options(
        &mut self,
        skip_if_busy: bool,
        cx: &mut Context<Self>,
    ) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            if !skip_if_busy {
                self.mark_cloud_sync_operation_in_progress();
            }
            return;
        }
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync.controller.store.state().settings.clone();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let service = self.cloud_sync.controller.service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("check");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(deliver_cloud_sync_check(
            tx,
            service,
            settings,
            hints,
            skip_if_busy,
        ));
    }

    pub(super) fn start_cloud_sync_upload_with_options(
        &mut self,
        force: bool,
        automatic: bool,
        skip_if_busy: bool,
        cx: &mut Context<Self>,
    ) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            if !skip_if_busy {
                self.mark_cloud_sync_operation_in_progress();
            }
            return;
        }
        let (device_id, revision_sequence) = {
            let state = self.cloud_sync.controller.store.state_mut();
            let device_id = state.ensure_device_id(cloud_sync_platform_label());
            let revision_sequence = state.revision_seq + 1;
            state.last_error = None;
            (device_id, revision_sequence)
        };
        self.cloud_sync.view.upload_preview = None;
        self.cloud_sync.view.upload_selection = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync.controller.store.state().settings.clone();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let previous_remote_sections = self
            .cloud_sync
            .controller
            .store
            .state()
            .last_synced_remote_sections
            .clone();
        let previous_remote_revision = self
            .cloud_sync
            .controller
            .store
            .state()
            .last_known_remote_revision
            .clone();
        let last_synced_structured_state = self
            .cloud_sync
            .controller
            .store
            .state()
            .last_synced_structured_state
            .clone();
        let upload_selection = (!automatic)
            .then(|| self.cloud_sync.view.upload_selection.clone())
            .flatten();
        let raw_sync_scope = upload_selection
            .as_ref()
            .map(|selection| {
                selection.raw_scope(&self.cloud_sync.controller.store.state().sync_scope)
            })
            .unwrap_or_else(|| self.cloud_sync.controller.store.state().sync_scope.clone());
        let item_filter = upload_selection
            .as_ref()
            .map(CloudSyncUploadSelection::item_filter)
            .unwrap_or_default();
        let portable_secrets =
            match self.collect_cloud_sync_sensitive_portable_secrets(&raw_sync_scope) {
                Ok(secrets) => secrets,
                Err(error) => {
                    self.finish_cloud_sync_error("upload", error);
                    return;
                }
            };
        let connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let settings_store = self.settings_store.clone();
        let service = self.cloud_sync.controller.service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("upload");
        self.cloud_sync.view.upload_selection = None;
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(deliver_cloud_sync_upload(
            tx,
            service,
            connection_store,
            forwarding_registry,
            settings_store,
            settings,
            hints,
            UploadOptions {
                force,
                device_id,
                revision_sequence,
                previous_remote_revision,
                previous_remote_sections,
                last_synced_structured_state,
                raw_sync_scope: Some(raw_sync_scope),
                item_filter,
                portable_secrets,
                automatic,
                skip_if_busy,
                ..UploadOptions::default()
            },
            automatic,
        ));
    }

    pub(super) fn start_cloud_sync_upload_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        if !self.persist_cloud_sync_configuration(false, cx) {
            return;
        }
        if matches!(
            self.cloud_sync
                .controller
                .store
                .state()
                .settings
                .backend_type,
            BackendType::GithubGist
        ) && self
            .cloud_sync
            .controller
            .store
            .state()
            .settings
            .git_repository
            .trim()
            .is_empty()
        {
            self.start_cloud_sync_upload_with_options(false, false, false, cx);
            return;
        }
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.cloud_sync.view.upload_preview = None;
        self.cloud_sync.view.upload_selection = None;
        self.cloud_sync.view.pending_preview = None;
        self.cloud_sync.view.preview_selection = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync.controller.store.state().settings.clone();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let previous_remote_sections = self
            .cloud_sync
            .controller
            .store
            .state()
            .last_synced_remote_sections
            .clone();
        let connection_store = self.connection_store.clone();
        let service = self.cloud_sync.controller.service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("upload_preview");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime
            .spawn(deliver_cloud_sync_upload_preview(
                tx,
                service,
                connection_store,
                settings,
                hints,
                previous_remote_sections,
            ));
    }

    pub(super) fn collect_cloud_sync_sensitive_portable_secrets(
        &self,
        raw_sync_scope: &RawSyncScope,
    ) -> Result<Vec<oxideterm_connections::oxide_file::EncryptedPortableSecret>, String> {
        let scope = normalize_sync_scope(Some(raw_sync_scope), &[]);
        if !scope.sync_sensitive_credentials {
            return Ok(Vec::new());
        }
        let provider_ids =
            oxideterm_ai::provider_views(&self.settings_store.settings().ai.providers)
                .into_iter()
                .map(|provider| provider.id)
                .filter(|provider_id| self.ai.models.key_store.has_provider_key(provider_id))
                .collect::<Vec<_>>();
        self.ai
            .models
            .key_store
            .get_provider_keys(&provider_ids)
            .map_err(|error| error.to_string())
            .map(|secrets| {
                secrets
                    .into_iter()
                    .map(|(id, secret)| {
                        oxideterm_connections::oxide_file::EncryptedPortableSecret {
                            kind: "ai_provider_key".to_string(),
                            id,
                            secret,
                        }
                    })
                    .collect()
            })
    }

    pub(super) fn start_cloud_sync_pull_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        if !self.persist_cloud_sync_configuration(false, cx) {
            return;
        }
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.cloud_sync.view.upload_preview = None;
        self.cloud_sync.view.upload_selection = None;
        self.cloud_sync.view.pending_preview = None;
        self.cloud_sync.view.preview_selection = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync.controller.store.state().settings.clone();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let previous_remote_sections = self
            .cloud_sync
            .controller
            .store
            .state()
            .last_synced_remote_sections
            .clone();
        let connection_store = self.connection_store.clone();
        let service = self.cloud_sync.controller.service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("pull");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime
            .spawn(deliver_cloud_sync_pull_preview(
                tx,
                service,
                connection_store,
                settings,
                hints,
                previous_remote_sections,
            ));
    }

    pub(super) fn start_cloud_sync_restore_backup(
        &mut self,
        backup_id: String,
        cx: &mut Context<Self>,
    ) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        if !self.persist_cloud_sync_configuration(false, cx) {
            return;
        }
        let Some(backup) = self
            .cloud_sync
            .controller
            .store
            .state()
            .rollback_backups
            .iter()
            .find(|backup| backup.id == backup_id)
            .cloned()
        else {
            self.finish_cloud_sync_error(
                "restore",
                self.i18n
                    .t("plugin.cloud_sync.errors.rollback_backup_missing"),
            );
            return;
        };
        // Tauri keeps the current panel state visible while a rollback backup
        // is being previewed; only the progress affordance changes until the
        // preview succeeds or fails.
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync.controller.store.state().settings.clone();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let connection_store = self.connection_store.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("restore");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime
            .spawn(deliver_cloud_sync_restore_backup_preview(
                tx,
                connection_store,
                settings,
                hints,
                backup,
            ));
    }

    pub(super) fn start_cloud_sync_apply_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.controller.delivery_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        let Some(preview) = self.cloud_sync.view.pending_preview.clone() else {
            return;
        };
        let selection = self
            .cloud_sync
            .view
            .preview_selection
            .clone()
            .unwrap_or_else(|| {
                CloudSyncPreviewSelection::from_preview(
                    &preview,
                    self.cloud_sync
                        .controller
                        .store
                        .state()
                        .settings
                        .default_conflict_strategy
                        .clone(),
                )
            });
        let create_rollback_backup = cloud_sync_should_create_rollback_backup(
            &preview,
            self.cloud_sync.controller.store.state().local_dirty,
        );
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let settings_store = self.settings_store.clone();
        let settings = self.cloud_sync.controller.store.state().settings.clone();
        let hints = self
            .cloud_sync
            .controller
            .store
            .state()
            .secret_hints
            .clone();
        let source_revision = self
            .cloud_sync
            .controller
            .store
            .state()
            .last_known_remote_revision
            .clone();
        let service = self.cloud_sync.controller.service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync.controller.delivery_rx = Some(rx);
        self.cloud_sync.controller.active_action = Some("apply");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime
            .spawn(deliver_cloud_sync_apply_preview(
                tx,
                service,
                connection_store,
                forwarding_registry,
                settings_store,
                settings,
                hints,
                source_revision,
                preview,
                selection,
                create_rollback_backup,
            ));
    }

    pub(super) fn schedule_cloud_sync_poll(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync.controller.polling {
            return;
        }
        self.cloud_sync.controller.polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(50)).await;
                let keep_polling = weak
                    .update(cx, |this, cx| {
                        this.poll_cloud_sync_delivery(cx);
                        this.cloud_sync.controller.polling
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn poll_cloud_sync_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.cloud_sync.controller.delivery_rx.as_ref() else {
            self.cloud_sync.controller.polling = false;
            return;
        };
        let mut deliveries = Vec::new();
        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(delivery) => deliveries.push(delivery),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        for delivery in deliveries {
            self.handle_cloud_sync_delivery(delivery, cx);
        }
        if disconnected {
            self.cloud_sync.controller.delivery_rx = None;
            self.cloud_sync.controller.polling = false;
            self.cloud_sync.controller.active_action = None;
            if matches!(
                self.cloud_sync.controller.store.state().status,
                CloudSyncStatus::Uploading | CloudSyncStatus::Checking
            ) {
                self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Idle;
                self.save_cloud_sync_state();
            }
            if self.cloud_sync.controller.pull_preview_after_current {
                self.cloud_sync.controller.pull_preview_after_current = false;
                self.start_cloud_sync_pull_preview(cx);
            } else if let Some(automatic) = self.cloud_sync.controller.upload_after_current.take() {
                self.start_cloud_sync_upload_with_options(false, automatic, true, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn handle_cloud_sync_delivery(
        &mut self,
        delivery: CloudSyncDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery {
            CloudSyncDelivery::Progress(progress) => {
                if self.cloud_sync.controller.active_action == Some("upload")
                    && self.cloud_sync.controller.store.state().status != CloudSyncStatus::Uploading
                {
                    self.cloud_sync.controller.store.state_mut().status =
                        CloudSyncStatus::Uploading;
                    self.cloud_sync.controller.store.state_mut().last_error = None;
                    self.save_cloud_sync_state();
                }
                self.cloud_sync.controller.progress = Some(progress);
            }
            CloudSyncDelivery::RollbackBackupCreated(backup) => {
                self.cloud_sync
                    .controller
                    .store
                    .state_mut()
                    .append_rollback_backup(backup);
                self.save_cloud_sync_state();
                self.push_cloud_sync_toast(
                    self.i18n
                        .t("plugin.cloud_sync.toast.rollback_backup_available"),
                    None,
                    TerminalNoticeVariant::Success,
                );
            }
            CloudSyncDelivery::CheckFinished(action) => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(metadata) => self.finish_cloud_sync_check(metadata),
                    Err(error) => self.finish_cloud_sync_error("check", error),
                }
            }
            CloudSyncDelivery::UploadFinished { action, automatic } => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                if let Some(metadata) = action.remote_metadata.as_ref() {
                    persist_remote_metadata(self.cloud_sync.controller.store.state_mut(), metadata);
                }
                if let Some(sequence) = action.revision_sequence_consumed {
                    let revision_seq = self
                        .cloud_sync
                        .controller
                        .store
                        .state()
                        .revision_seq
                        .max(sequence);
                    self.cloud_sync.controller.store.state_mut().revision_seq = revision_seq;
                }
                match action.result {
                    Ok(outcome) => self.finish_cloud_sync_upload(outcome, automatic),
                    Err(error) => {
                        if automatic {
                            self.finish_cloud_sync_automatic_upload_error(error);
                        } else if is_cloud_sync_remote_changed_before_upload(&error) {
                            self.finish_cloud_sync_upload_conflict_for_preview(error);
                        } else {
                            self.finish_cloud_sync_error("upload", error);
                        }
                    }
                }
            }
            CloudSyncDelivery::UploadPreviewFinished(action) => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(preview) => self.finish_cloud_sync_upload_preview(preview),
                    Err(error) if error.starts_with("remote_not_found") => {
                        self.cloud_sync.controller.upload_after_current = Some(false);
                    }
                    Err(error) => self.finish_cloud_sync_error("upload_preview", error),
                }
            }
            CloudSyncDelivery::PullPreviewFinished(action) => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(preview) => self.finish_cloud_sync_pull_preview(preview),
                    Err(error) => self.finish_cloud_sync_error("pull", error),
                }
            }
            CloudSyncDelivery::RestoreBackupPreviewFinished(action) => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(preview) => self.finish_cloud_sync_pull_preview(preview),
                    Err(error) => self.finish_cloud_sync_error("restore", error),
                }
            }
            CloudSyncDelivery::ApplyPreviewFinished(action) => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(outcome) => self.finish_cloud_sync_apply_preview(outcome, cx),
                    Err(error) => self.finish_cloud_sync_error("apply", error),
                }
            }
            CloudSyncDelivery::GithubOauthCode(prompt) => {
                cx.open_url(&prompt.verification_uri);
                self.push_cloud_sync_toast(
                    self.i18n
                        .t("plugin.cloud_sync.toast.github_oauth_code_title"),
                    Some(self.i18n_replace(
                        "plugin.cloud_sync.toast.github_oauth_code_description",
                        &[
                            ("code", prompt.user_code),
                            ("url", prompt.verification_uri),
                            ("seconds", prompt.expires_in.to_string()),
                        ],
                    )),
                    TerminalNoticeVariant::Default,
                );
            }
            CloudSyncDelivery::GithubOauthFinished(action) => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(()) => self.finish_cloud_sync_github_oauth(),
                    Err(error) => self.finish_cloud_sync_error("github_oauth", error),
                }
            }
            CloudSyncDelivery::MicrosoftOauthCode(prompt) => {
                cx.open_url(&prompt.verification_uri);
                self.push_cloud_sync_toast(
                    self.i18n
                        .t("plugin.cloud_sync.toast.microsoft_oauth_code_title"),
                    Some(self.i18n_replace(
                        "plugin.cloud_sync.toast.microsoft_oauth_code_description",
                        &[
                            ("code", prompt.user_code),
                            ("url", prompt.verification_uri),
                            ("seconds", prompt.expires_in.to_string()),
                        ],
                    )),
                    TerminalNoticeVariant::Default,
                );
            }
            CloudSyncDelivery::MicrosoftOauthFinished(action) => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(()) => self.finish_cloud_sync_microsoft_oauth(),
                    Err(error) => self.finish_cloud_sync_error("microsoft_oauth", error),
                }
            }
            CloudSyncDelivery::GoogleOauthUrl(prompt) => {
                cx.open_url(&prompt.authorization_url);
                self.push_cloud_sync_toast(
                    self.i18n
                        .t("plugin.cloud_sync.toast.google_oauth_url_title"),
                    Some(self.i18n_replace(
                        "plugin.cloud_sync.toast.google_oauth_url_description",
                        &[("seconds", prompt.expires_in.to_string())],
                    )),
                    TerminalNoticeVariant::Default,
                );
            }
            CloudSyncDelivery::GoogleOauthFinished(action) => {
                self.cloud_sync.controller.store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(()) => self.finish_cloud_sync_google_oauth(),
                    Err(error) => self.finish_cloud_sync_error("google_oauth", error),
                }
            }
        }
    }

    pub(super) fn finish_cloud_sync_check(
        &mut self,
        metadata: Option<oxideterm_cloud_sync::backend::RemoteMetadata>,
    ) {
        let now = Utc::now().to_rfc3339();
        let dirty = metadata
            .as_ref()
            .and_then(|_| {
                build_local_snapshot(
                    &self.connection_store,
                    &self.forwarding_registry,
                    &self.settings_store,
                    self.cloud_sync
                        .controller
                        .store
                        .state()
                        .last_synced_structured_state
                        .as_ref(),
                    Some(&self.cloud_sync.controller.store.state().sync_scope),
                )
                .ok()
            })
            .map(|snapshot| snapshot.dirty);
        let conflict_error = self.i18n_replace(
            "plugin.cloud_sync.errors.remote_update_conflict_hint",
            &[(
                "revision",
                metadata
                    .as_ref()
                    .and_then(|metadata| metadata.revision.clone())
                    .unwrap_or_else(|| "—".to_string()),
            )],
        );
        finish_cloud_sync_check_state(
            self.cloud_sync.controller.store.state_mut(),
            metadata.as_ref(),
            dirty.as_ref(),
            Some(conflict_error),
            now,
        );
        self.cloud_sync.controller.progress = None;
        self.save_cloud_sync_state();
    }

    pub(super) fn finish_cloud_sync_upload(&mut self, outcome: UploadOutcome, automatic: bool) {
        if let Some(gist_id) = outcome.created_remote_id.as_ref() {
            self.cloud_sync
                .controller
                .store
                .state_mut()
                .settings
                .git_repository = gist_id.clone();
            self.cloud_sync.view.form.git_repository = gist_id.clone();
        }
        let revision =
            finish_cloud_sync_upload_state(self.cloud_sync.controller.store.state_mut(), &outcome);
        self.cloud_sync.controller.progress = None;
        self.cloud_sync.view.pending_preview = None;
        self.cloud_sync.view.upload_preview = None;
        self.cloud_sync.view.upload_selection = None;
        self.save_cloud_sync_state();
        if !automatic {
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.upload_success_title"),
                Some(revision),
                TerminalNoticeVariant::Success,
            );
        }
    }

    pub(super) fn finish_cloud_sync_github_oauth(&mut self) {
        self.cloud_sync.controller.progress = None;
        self.cloud_sync.view.form.git_token.clear();
        self.cloud_sync.view.form.git_token_touched = false;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Idle;
        self.save_cloud_sync_state();
        self.push_cloud_sync_toast(
            self.i18n
                .t("plugin.cloud_sync.toast.github_oauth_success_title"),
            None,
            TerminalNoticeVariant::Success,
        );
    }

    pub(super) fn finish_cloud_sync_microsoft_oauth(&mut self) {
        self.cloud_sync.controller.progress = None;
        // Microsoft OAuth tokens are persisted by the delivery task into the
        // keychain; clear the generic token draft so UI memory does not retain it.
        self.cloud_sync.view.form.token.clear();
        self.cloud_sync.view.form.token_touched = false;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Idle;
        self.save_cloud_sync_state();
        self.push_cloud_sync_toast(
            self.i18n
                .t("plugin.cloud_sync.toast.microsoft_oauth_success_title"),
            None,
            TerminalNoticeVariant::Success,
        );
    }

    pub(super) fn finish_cloud_sync_google_oauth(&mut self) {
        self.cloud_sync.controller.progress = None;
        // Google OAuth tokens are persisted by the delivery task into the
        // keychain; clear the generic token draft so UI memory does not retain it.
        self.cloud_sync.view.form.token.clear();
        self.cloud_sync.view.form.token_touched = false;
        self.cloud_sync.controller.store.state_mut().last_error = None;
        self.cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Idle;
        self.save_cloud_sync_state();
        self.push_cloud_sync_toast(
            self.i18n
                .t("plugin.cloud_sync.toast.google_oauth_success_title"),
            None,
            TerminalNoticeVariant::Success,
        );
    }

    pub(super) fn finish_cloud_sync_automatic_upload_error(&mut self, error: String) {
        let display_error = self.format_cloud_sync_error(&error);
        let history_summary = self.cloud_sync_upload_failure_summary();
        finish_cloud_sync_automatic_upload_error_state(
            self.cloud_sync.controller.store.state_mut(),
            &error,
            display_error,
            history_summary,
        );
        self.cloud_sync.controller.progress = None;
        self.save_cloud_sync_state();
    }

    pub(super) fn finish_cloud_sync_upload_conflict_for_preview(&mut self, error: String) {
        let display_error = self.format_cloud_sync_error(&error);
        let history_summary = self.cloud_sync_upload_failure_summary();
        finish_cloud_sync_error_state(
            self.cloud_sync.controller.store.state_mut(),
            "upload",
            &error,
            display_error,
            Some(history_summary),
        );
        self.cloud_sync.controller.progress = None;
        self.cloud_sync.controller.upload_after_current = None;
        self.cloud_sync.controller.pull_preview_after_current = true;
        self.save_cloud_sync_state();
    }

    pub(super) fn finish_cloud_sync_pull_preview(&mut self, preview: CloudSyncPendingPreview) {
        finish_cloud_sync_pull_preview_state(
            self.cloud_sync.controller.store.state_mut(),
            &preview,
        );
        self.cloud_sync.view.preview_selection = Some(CloudSyncPreviewSelection::from_preview(
            &preview,
            self.cloud_sync
                .controller
                .store
                .state()
                .settings
                .default_conflict_strategy
                .clone(),
        ));
        self.cloud_sync.view.upload_preview = None;
        self.cloud_sync.view.upload_selection = None;
        self.cloud_sync.view.pending_preview = Some(preview);
        self.cloud_sync.controller.progress = None;
        self.save_cloud_sync_state();
    }

    pub(super) fn finish_cloud_sync_upload_preview(&mut self, preview: CloudSyncPendingPreview) {
        finish_cloud_sync_pull_preview_state(
            self.cloud_sync.controller.store.state_mut(),
            &preview,
        );
        let scope = normalize_sync_scope(
            Some(&self.cloud_sync.controller.store.state().sync_scope),
            &[],
        );
        let local = self.cloud_sync_local_field_diff_snapshot();
        self.cloud_sync.view.upload_selection = Some(
            CloudSyncUploadSelection::from_scope_and_local_snapshot(&scope, &local),
        );
        self.cloud_sync.view.pending_preview = None;
        self.cloud_sync.view.preview_selection = None;
        self.cloud_sync.view.upload_preview = Some(preview);
        self.cloud_sync.controller.progress = None;
        self.save_cloud_sync_state();
    }

    pub(super) fn finish_cloud_sync_apply_preview(
        &mut self,
        ui_outcome: CloudSyncApplyUiOutcome,
        cx: &mut Context<Self>,
    ) {
        self.connection_store = ui_outcome.connection_store;
        self.settings_store = ui_outcome.settings_store;
        match ui_outcome.outcome {
            CloudSyncApplyOutcome::Structured(outcome) => {
                self.finish_structured_cloud_sync_apply(outcome)
            }
            CloudSyncApplyOutcome::Legacy {
                preview,
                source,
                selection,
                outcome,
            } => self.finish_legacy_cloud_sync_apply(preview, source, selection, outcome, cx),
        }
    }

    pub(super) fn finish_structured_cloud_sync_apply(
        &mut self,
        outcome: ApplyStructuredPreviewOutcome,
    ) {
        let mut outcome = outcome;
        // Structured apply persists Quick Commands through the domain crate, so refresh the
        // GPUI projection before any later UI edit can overwrite the newly synchronized file.
        self.quick_commands.reload_from_store();
        if let Some(envelope) = outcome.sensitive_credentials_envelope.as_mut() {
            self.apply_oxide_import_portable_secrets(envelope);
        }
        let sensitive_restore_description = outcome
            .sensitive_credentials_envelope
            .as_ref()
            .and_then(|envelope| self.cloud_sync_sensitive_restore_description(envelope));
        let previous_local_baseline = self
            .cloud_sync
            .controller
            .store
            .state()
            .last_synced_structured_state
            .clone();
        let local_snapshot = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            previous_local_baseline.as_ref(),
            Some(&self.cloud_sync.controller.store.state().sync_scope),
        )
        .unwrap_or_else(|_| outcome.local_snapshot.clone());
        let should_trigger_upload_after = finish_structured_cloud_sync_apply_state(
            self.cloud_sync.controller.store.state_mut(),
            &outcome,
            &local_snapshot,
            Utc::now().to_rfc3339(),
        );
        self.cloud_sync.view.pending_preview = None;
        self.cloud_sync.view.upload_preview = None;
        self.cloud_sync.view.upload_selection = None;
        self.cloud_sync.view.preview_selection = None;
        self.cloud_sync.controller.progress = None;
        if should_trigger_upload_after {
            self.cloud_sync.controller.upload_after_current = Some(true);
        }
        self.save_cloud_sync_state();
        let mut description = self.i18n_replace(
            "plugin.cloud_sync.toast.pull_success_description",
            &[
                ("imported", outcome.content_summary.connections.to_string()),
                ("merged", "0".to_string()),
            ],
        );
        if let Some(sensitive_restore_description) = sensitive_restore_description {
            description.push('\n');
            description.push_str(&sensitive_restore_description);
        }
        self.push_cloud_sync_toast(
            self.i18n.t("plugin.cloud_sync.toast.pull_success_title"),
            Some(description),
            TerminalNoticeVariant::Success,
        );
    }

    pub(super) fn finish_legacy_cloud_sync_apply(
        &mut self,
        preview: LegacyPreview,
        source: CloudSyncPreviewSource,
        selection: CloudSyncPreviewSelection,
        mut outcome: ApplyLegacyPreviewOutcome,
        cx: &mut Context<Self>,
    ) {
        let plan = cloud_sync_legacy_apply_plan(&preview, &source, &selection);
        let cloud_options = plan.import_options;
        let imported_forwards = if cloud_options.oxide_options.import_forwards {
            self.apply_oxide_import_forward_records(&mut outcome.envelope)
        } else {
            0
        };
        outcome.envelope.imported_forwards = imported_forwards;
        let (_imported_quick_commands, _skipped_quick_commands, _quick_command_errors) = self
            .apply_oxide_import_quick_commands(
                outcome.envelope.quick_commands_json.as_deref(),
                selection.import_quick_commands,
                QuickCommandImportStrategy::Merge,
            );
        self.apply_oxide_import_plugin_settings(
            &outcome.envelope.plugin_settings,
            cloud_options.import_plugin_settings,
            cloud_options.selected_plugin_ids.as_ref(),
        );
        self.apply_oxide_import_app_settings(
            outcome.envelope.app_settings_json.as_deref(),
            cloud_options.import_app_settings,
            cloud_options.selected_app_settings_sections.as_ref(),
            cx,
        );
        if cloud_options.oxide_options.import_portable_secrets {
            self.apply_oxide_import_portable_secrets(&mut outcome.envelope);
        }
        let sensitive_restore_description =
            self.cloud_sync_sensitive_restore_description(&outcome.envelope);

        let local_snapshot = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            None,
            Some(&self.cloud_sync.controller.store.state().sync_scope),
        );
        let should_trigger_upload_after = finish_legacy_cloud_sync_apply_state(
            self.cloud_sync.controller.store.state_mut(),
            &preview,
            &source,
            &selection,
            local_snapshot.as_ref().ok(),
            Utc::now().to_rfc3339(),
        );
        self.cloud_sync.view.pending_preview = None;
        self.cloud_sync.view.upload_preview = None;
        self.cloud_sync.view.upload_selection = None;
        self.cloud_sync.view.preview_selection = None;
        self.cloud_sync.controller.progress = None;
        if should_trigger_upload_after {
            self.cloud_sync.controller.upload_after_current = Some(true);
        }
        self.save_cloud_sync_state();
        let copy = plan.success_copy;
        let mut description = self.i18n_replace(
            copy.description_key,
            &[
                ("imported", outcome.envelope.imported.to_string()),
                ("merged", outcome.envelope.merged.to_string()),
            ],
        );
        if let Some(sensitive_restore_description) = sensitive_restore_description {
            description.push('\n');
            description.push_str(&sensitive_restore_description);
        }
        self.push_cloud_sync_toast(
            self.i18n.t(copy.title_key),
            Some(description),
            TerminalNoticeVariant::Success,
        );
    }

    pub(super) fn cloud_sync_sensitive_restore_description(
        &self,
        envelope: &oxideterm_connections::oxide_file::ImportResultEnvelope,
    ) -> Option<String> {
        let total = envelope.restored_connection_passwords
            + envelope.restored_key_passphrases
            + envelope.restored_managed_keys
            + envelope.restored_managed_key_passphrases
            + envelope.restored_privilege_credentials
            + envelope.imported_portable_secrets
            + envelope.skipped_sensitive_credentials
            + envelope.skipped_portable_secrets;
        (total > 0).then(|| {
            self.i18n_replace(
                "plugin.cloud_sync.toast.sensitive_restore_description",
                &[
                    (
                        "passwords",
                        envelope.restored_connection_passwords.to_string(),
                    ),
                    (
                        "keyPassphrases",
                        envelope.restored_key_passphrases.to_string(),
                    ),
                    ("managedKeys", envelope.restored_managed_keys.to_string()),
                    (
                        "managedKeyPassphrases",
                        envelope.restored_managed_key_passphrases.to_string(),
                    ),
                    (
                        "privilegeCredentials",
                        envelope.restored_privilege_credentials.to_string(),
                    ),
                    ("aiKeys", envelope.imported_portable_secrets.to_string()),
                    (
                        "skippedCredentials",
                        envelope.skipped_sensitive_credentials.to_string(),
                    ),
                    (
                        "skippedAiKeys",
                        envelope.skipped_portable_secrets.to_string(),
                    ),
                ],
            )
        })
    }

    pub(super) fn finish_cloud_sync_error(&mut self, action: &str, error: String) {
        let display_error = self.format_cloud_sync_error(&error);
        let upload_history_summary =
            (action == "upload").then(|| self.cloud_sync_upload_failure_summary());
        finish_cloud_sync_error_state(
            self.cloud_sync.controller.store.state_mut(),
            action,
            &error,
            display_error.clone(),
            upload_history_summary,
        );
        self.cloud_sync.controller.progress = None;
        if action == "upload_preview" {
            self.cloud_sync.view.upload_preview = None;
            self.cloud_sync.view.upload_selection = None;
        }
        self.save_cloud_sync_state();
        let title_key = match action {
            "upload" => Some("plugin.cloud_sync.toast.upload_failed_title"),
            "apply" => Some(
                if self
                    .cloud_sync
                    .view
                    .pending_preview
                    .as_ref()
                    .is_some_and(CloudSyncPendingPreview::is_backup)
                {
                    "plugin.cloud_sync.toast.restore_failed_title"
                } else {
                    "plugin.cloud_sync.toast.pull_failed_title"
                },
            ),
            "github_oauth" => Some("plugin.cloud_sync.toast.github_oauth_failed_title"),
            "microsoft_oauth" => Some("plugin.cloud_sync.toast.microsoft_oauth_failed_title"),
            "google_oauth" => Some("plugin.cloud_sync.toast.google_oauth_failed_title"),
            _ => None,
        };
        if let Some(title_key) = title_key {
            self.push_cloud_sync_toast(
                self.i18n.t(title_key),
                Some(display_error),
                TerminalNoticeVariant::Error,
            );
        }
    }

    pub(super) fn mark_cloud_sync_operation_in_progress(&mut self) {
        self.cloud_sync.controller.store.state_mut().last_error = Some(
            self.i18n
                .t("plugin.cloud_sync.errors.operation_in_progress"),
        );
        self.save_cloud_sync_state();
    }

    pub(super) fn cloud_sync_upload_failure_summary(&self) -> CloudSyncHistorySummary {
        CloudSyncHistorySummary {
            connections: self.connection_store.connections().len(),
            forwards: self.forwarding_registry.list_all_saved_forwards().len(),
            quick_commands: 0,
            serial_profiles: self.connection_store.serial_profiles().len(),
            raw_tcp_profiles: self.connection_store.raw_tcp_profiles().len(),
            raw_udp_profiles: self.connection_store.raw_udp_profiles().len(),
            sensitive_credentials: 0,
            has_app_settings: true,
            plugin_settings_count: 0,
        }
    }

    pub(super) fn save_cloud_sync_state(&mut self) {
        self.invalidate_cloud_sync_snapshot_caches();
        if let Err(error) = self.cloud_sync.controller.store.save() {
            self.cloud_sync.controller.store.state_mut().last_error = Some(error.to_string());
        }
    }
}
