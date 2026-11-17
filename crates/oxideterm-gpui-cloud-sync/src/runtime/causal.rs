use super::*;

impl CloudSyncRuntime<'_> {
    pub(super) fn start_cloud_sync_password_change(&mut self, cx: &mut App) {
        if self.cloud_sync.read(cx).operation_in_flight() {
            return;
        }
        let current = self
            .cloud_sync
            .read(cx)
            .controller
            .store
            .state()
            .settings
            .clone();
        let (mut proposed, _) = cloud_sync_settings_from_form(&self.cloud_sync.read(cx).view.form);
        proposed.sync_password_ref = current.sync_password_ref.clone();
        let mut handoff = self.cloud_sync.update(cx, |cloud_sync, _| {
            cloud_sync.view.form.take_secret_handoff()
        });
        let other_secrets = handoff.token.is_some()
            || handoff.git_token.is_some()
            || handoff.basic_username.is_some()
            || handoff.basic_password.is_some()
            || handoff.access_key_id.is_some()
            || handoff.secret_access_key.is_some()
            || handoff.session_token.is_some();
        if proposed != current || other_secrets {
            self.cloud_sync.update(cx, |cloud_sync, _| {
                cloud_sync.view.form.restore_secret_handoff(handoff)
            });
            self.finish_cloud_sync_error("sync", "password_change_separate_settings".into(), cx);
            return;
        }
        let Some(password) = handoff.sync_password.take() else {
            return;
        };
        if password.chars().count() < 6 {
            self.finish_cloud_sync_error("sync", "password_too_short".into(), cx);
            return;
        }
        self.start_causal_sync(true, cx);
        if self.cloud_sync.read(cx).operation_in_flight() {
            self.cloud_sync.update(cx, |cloud_sync, _| {
                cloud_sync.controller.password_change = Some(password)
            });
        }
    }
    pub fn start_causal_sync(&mut self, automatic: bool, cx: &mut App) {
        self.start_causal_sync_with_direction(automatic, SyncDirection::Merge, cx);
    }

    pub(super) fn start_causal_sync_with_direction(
        &mut self,
        automatic: bool,
        direction: SyncDirection,
        cx: &mut App,
    ) {
        if self.cloud_sync.read(cx).operation_in_flight() {
            return;
        }
        if automatic && self.cloud_sync.read(cx).controller.causal_pending.is_some() {
            return;
        }
        self.cancel_causal_sync(cx);
        self.cloud_sync.update(cx, |cloud_sync, _| {
            cloud_sync.view.pending_preview = None;
            cloud_sync.view.upload_preview = None;
            cloud_sync.view.preview_selection = None;
            cloud_sync.view.upload_selection = None;
        });
        let state = self.cloud_sync.read(cx).controller.store.state().clone();
        let mut provider = CloudSyncKeychainSecretProvider::new(state.secret_hints.clone());
        let previous = self.settings_store.settings().clone();
        match oxideterm_cloud_sync::sync_v3::RecoveryJournal::recover_pending_on_owner(
            self.connection_store,
            self.settings_store,
            self.forwarding_registry,
            &mut provider,
        ) {
            Ok(0) => {}
            Ok(_) => self
                .effects
                .push(CloudSyncHostEffect::RefreshOwners(Box::new(previous))),
            Err(error) => {
                self.finish_cloud_sync_error("sync", error.to_string(), cx);
                return;
            }
        }
        let scope = state.sync_scope(&[]);
        let service = self.cloud_sync.read(cx).controller.service.clone();
        let tx = self.cloud_sync.update(cx, |cloud_sync, cx| {
            cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Checking;
            cloud_sync.controller.store.state_mut().last_error = None;
            cloud_sync.begin_delivery("sync", cx)
        });
        self.forwarding_runtime.spawn(crate::deliver_causal_sync(
            tx,
            service,
            self.connection_store.clone(),
            self.forwarding_registry.clone(),
            self.settings_store.clone(),
            state.settings,
            provider.hints().clone(),
            scope,
            Default::default(),
            automatic,
            direction,
        ));
    }

    pub(super) fn receive_causal_plan(
        &mut self,
        mut prepared: PreparedSync,
        automatic: bool,
        cx: &mut App,
    ) {
        let changing_password = self
            .cloud_sync
            .read(cx)
            .controller
            .password_change
            .is_some();
        if !changing_password && !prepared.has_remote() {
            prepared.set_direction(SyncDirection::Upload);
        } else if !changing_password && prepared.first_sync() {
            prepared.set_direction(SyncDirection::Download);
        }
        let summary = match prepared.summary() {
            Ok(summary) => summary,
            Err(error) => {
                self.finish_cloud_sync_error("sync", error.to_string(), cx);
                return;
            }
        };
        let apply_now =
            automatic && (!summary.first_sync || changing_password) && summary.conflicts.is_empty();
        let previews = match prepared.conflict_previews() {
            Ok(previews) => previews,
            Err(error) => {
                self.finish_cloud_sync_error("sync", error.to_string(), cx);
                return;
            }
        };
        self.cloud_sync.update(cx, |cloud_sync, cx| {
            cloud_sync.controller.causal_pending = Some(prepared);
            cloud_sync.view.causal_summary = Some(summary);
            cloud_sync.view.causal_conflicts = previews;
            cloud_sync.controller.active_action = None;
            cloud_sync.controller.store.state_mut().status = if cloud_sync
                .view
                .causal_summary
                .as_ref()
                .unwrap()
                .conflicts
                .is_empty()
            {
                CloudSyncStatus::RemoteUpdate
            } else {
                CloudSyncStatus::Conflict
            };
            cx.notify();
        });
        if apply_now {
            // The prepare worker's disconnect must be handled before publishing.
            let entity = self.cloud_sync.downgrade();
            cx.defer(move |cx| {
                let _ = entity.update(cx, |_, cx| {
                    cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                        CloudSyncUiIntent::ApplyCausal,
                    ));
                });
            });
        }
    }

    pub(super) fn choose_cloud_sync_direction(&mut self, direction: SyncDirection, cx: &mut App) {
        let result = self
            .cloud_sync
            .update(cx, |cloud_sync, cx| -> anyhow::Result<()> {
                if cloud_sync.operation_in_flight() {
                    return Ok(());
                }
                if cloud_sync.controller.password_change.is_some() {
                    return Ok(());
                }
                if cloud_sync
                    .view
                    .causal_summary
                    .as_ref()
                    .is_some_and(|summary| summary.direction == direction)
                {
                    return Ok(());
                }
                let Some(prepared) = cloud_sync.controller.causal_pending.as_mut() else {
                    return Ok(());
                };
                prepared.set_direction(direction);
                cloud_sync.view.causal_summary = Some(prepared.summary()?);
                cloud_sync.view.causal_conflicts = prepared.conflict_previews()?;
                cloud_sync.view.causal_choices.clear();
                cloud_sync.controller.store.state_mut().status = if cloud_sync
                    .view
                    .causal_summary
                    .as_ref()
                    .is_some_and(|summary| !summary.conflicts.is_empty())
                {
                    CloudSyncStatus::Conflict
                } else {
                    CloudSyncStatus::RemoteUpdate
                };
                cx.notify();
                Ok(())
            });
        if let Err(error) = result {
            self.finish_cloud_sync_error("sync", error.to_string(), cx);
        }
    }

    pub fn apply_causal_sync(&mut self, cx: &mut App) {
        let pending = self.cloud_sync.update(cx, |cloud_sync, _| {
            cloud_sync.controller.causal_pending.take()
        });
        let Some(mut prepared) = pending else {
            return;
        };
        let (summary, choices) = {
            let state = self.cloud_sync.read(cx);
            (
                state.view.causal_summary.clone(),
                state.view.causal_choices.clone(),
            )
        };
        let direction = summary
            .as_ref()
            .map(|summary| summary.direction)
            .unwrap_or_default();
        if let Some(summary) = summary {
            for (index, candidate) in choices {
                if let Some(conflict) = summary.conflicts.get(index)
                    && let Err(error) = prepared.choose_candidate(conflict.clone(), candidate)
                {
                    self.finish_cloud_sync_error("sync", error.to_string(), cx);
                    return;
                }
            }
        }
        let previous = self.settings_store.settings().clone();
        let applied = match prepared.apply(
            self.connection_store,
            self.forwarding_registry,
            self.settings_store,
        ) {
            Ok(applied) => applied,
            Err(error) => {
                self.cancel_causal_sync(cx);
                self.finish_cloud_sync_error("sync", error.to_string(), cx);
                return;
            }
        };
        self.effects
            .push(CloudSyncHostEffect::RefreshOwners(Box::new(previous)));
        let tx = self.cloud_sync.update(cx, |cloud_sync, cx| {
            cloud_sync.view.causal_summary = None;
            cloud_sync.view.causal_choices.clear();
            cloud_sync.view.causal_conflicts.clear();
            cloud_sync.controller.store.state_mut().status = if direction == SyncDirection::Download
            {
                CloudSyncStatus::RemoteUpdate
            } else {
                CloudSyncStatus::Uploading
            };
            cloud_sync.begin_delivery("sync", cx)
        });
        let (password, hints) = self.cloud_sync.update(cx, |cloud_sync, _| {
            (
                cloud_sync.controller.password_change.take(),
                cloud_sync.controller.store.state().secret_hints.clone(),
            )
        });
        let settings_path = self.settings_store.path().to_path_buf();
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let result = if let Some(password) = password {
                applied
                    .change_password(password, &mut provider, &settings_path)
                    .await
            } else {
                applied.publish().await
            }
            .map_err(|error| error.to_string());
            let _ = tx.send(CloudSyncDelivery::CausalFinished(
                crate::CloudSyncActionResult {
                    result,
                    secret_hints: provider.hints().clone(),
                },
            ));
        });
    }

    pub fn finish_causal_sync(&mut self, outcome: &SyncOutcome, cx: &mut App) {
        let saved = self.cloud_sync.update(cx, |cloud_sync, _| {
            let previous = cloud_sync.controller.store.state().clone();
            let state = cloud_sync.controller.store.state_mut();
            oxideterm_cloud_sync::state_transitions::finish_causal_sync_state(
                state,
                outcome,
                Utc::now().to_rfc3339(),
            );
            if let Err(error) = cloud_sync.controller.store.save() {
                *cloud_sync.controller.store.state_mut() = previous;
                return Err(error.to_string());
            }
            if let Some(id) = &outcome.created_remote_id {
                cloud_sync.view.form.git_repository = id.clone();
            }
            if outcome.switched_settings.is_some() {
                cloud_sync.view.form = CloudSyncFormDraft::from_settings(
                    &cloud_sync.controller.store.state().settings,
                );
            }
            cloud_sync.controller.active_action = None;
            Ok(())
        });
        if let Err(error) = saved {
            self.finish_cloud_sync_error("sync", error, cx);
            return;
        }
        self.refresh_cloud_sync_local_dirty_state(cx);
        if outcome.downloaded {
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.pull_success_title"),
                None,
                CloudSyncNoticeKind::Success,
                cx,
            );
        }
    }

    pub(super) fn cancel_causal_sync(&mut self, cx: &mut App) {
        self.cloud_sync.update(cx, |cloud_sync, cx| {
            cloud_sync.controller.causal_pending = None;
            cloud_sync.controller.password_change = None;
            cloud_sync.view.causal_summary = None;
            cloud_sync.view.causal_choices.clear();
            cloud_sync.view.causal_conflicts.clear();
            cx.notify();
        });
    }
}
