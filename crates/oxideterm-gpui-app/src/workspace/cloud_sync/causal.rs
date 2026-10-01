// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use oxideterm_cloud_sync::operation::{PreparedSync, SyncOutcome};

impl WorkspaceApp {
    pub(super) fn start_cloud_sync_password_change(&mut self, cx: &mut Context<Self>) {
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
    pub(in crate::workspace) fn start_causal_sync(
        &mut self,
        automatic: bool,
        cx: &mut Context<Self>,
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
            &mut self.connection_store,
            &mut self.settings_store,
            self.forwarding_service.registry(),
            &mut provider,
        ) {
            Ok(0) => {}
            Ok(_) => self.refresh_causal_sync_owners(&previous, cx),
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
        self.forwarding_runtime
            .spawn(oxideterm_gpui_cloud_sync::deliver_causal_sync(
                tx,
                service,
                self.connection_store.clone(),
                self.forwarding_service.registry().clone(),
                self.settings_store.clone(),
                state.settings,
                provider.hints().clone(),
                scope,
                Default::default(),
                automatic,
            ));
    }

    pub(super) fn receive_causal_plan(
        &mut self,
        prepared: PreparedSync,
        automatic: bool,
        cx: &mut Context<Self>,
    ) {
        let summary = match prepared.summary() {
            Ok(summary) => summary,
            Err(error) => {
                self.finish_cloud_sync_error("sync", error.to_string(), cx);
                return;
            }
        };
        let apply_now = automatic && summary.conflicts.is_empty();
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
            let workspace = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = workspace.update(cx, |workspace, cx| workspace.apply_causal_sync(cx));
            });
        }
    }

    pub(in crate::workspace) fn apply_causal_sync(&mut self, cx: &mut Context<Self>) {
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
            &mut self.connection_store,
            self.forwarding_service.registry(),
            &mut self.settings_store,
        ) {
            Ok(applied) => applied,
            Err(error) => {
                self.cancel_causal_sync(cx);
                self.finish_cloud_sync_error("sync", error.to_string(), cx);
                return;
            }
        };
        self.refresh_causal_sync_owners(&previous, cx);
        let tx = self.cloud_sync.update(cx, |cloud_sync, cx| {
            cloud_sync.view.causal_summary = None;
            cloud_sync.view.causal_choices.clear();
            cloud_sync.view.causal_conflicts.clear();
            cloud_sync.controller.store.state_mut().status = CloudSyncStatus::Uploading;
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
                oxideterm_gpui_cloud_sync::CloudSyncActionResult {
                    result,
                    secret_hints: provider.hints().clone(),
                },
            ));
        });
    }

    pub(in crate::workspace) fn refresh_causal_sync_owners(
        &mut self,
        previous: &oxideterm_settings::PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        let settings = self.settings_store.settings().clone();
        self.apply_loaded_settings_to_runtime(previous, &settings, cx);
        self.ai_entity.read(cx).key_store().clear_cache();
        self.terminal.update(cx, |terminal, _| {
            terminal.quick_commands.store.reload_from_store()
        });
        self.settings_workspace.update(cx, |settings, _| {
            settings.acknowledge_external_store_state()
        });
        self.sync_ssh_config_sync_service();
        self.refresh_ai_skill_registry();
        self.emit_native_plugin_settings_events(previous, &settings, cx);
        self.sync_tab_titles(cx);
        self.invalidate_cloud_sync_snapshot_caches(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn finish_causal_sync(
        &mut self,
        outcome: &SyncOutcome,
        cx: &mut Context<Self>,
    ) {
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
    }

    pub(super) fn cancel_causal_sync(&mut self, cx: &mut Context<Self>) {
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

impl CloudSyncPageRenderer {
    pub(super) fn render_causal_preview(
        &self,
        summary: &oxideterm_cloud_sync::operation::SyncPlanSummary,
        previews: &[oxideterm_cloud_sync::operation::SyncConflictPreview],
        choices: &std::collections::BTreeMap<usize, usize>,
        busy: bool,
        cx: &mut App,
    ) -> AnyElement {
        let mut body = Vec::new();
        for (index, conflict) in summary.conflicts.iter().enumerate() {
            let preview = previews.get(index);
            let title = div()
                .child(
                    preview
                        .and_then(|preview| preview.name.clone())
                        .unwrap_or_else(|| self.i18n.t("plugin.cloud_sync.causal.configuration")),
                )
                .into_any_element();
            let mut buttons = div().flex().flex_wrap().gap(px(8.0));
            for (candidate, _) in conflict.candidates.iter().enumerate() {
                let label = format!(
                    "{} {}{}",
                    self.i18n.t("plugin.cloud_sync.causal.candidate"),
                    candidate + 1,
                    if choices.get(&index) == Some(&candidate) {
                        " ✓"
                    } else {
                        ""
                    }
                );
                let value = preview
                    .and_then(|preview| preview.candidates.get(candidate))
                    .and_then(|value| value.as_ref())
                    .map(|value| {
                        if conflict.field.is_presence() {
                            self.i18n.t(if value.as_str() == "true" {
                                "plugin.cloud_sync.causal.keep"
                            } else {
                                "common.delete"
                            })
                        } else {
                            value.to_string()
                        }
                    })
                    .unwrap_or_else(|| self.i18n.t("plugin.cloud_sync.causal.protected"));
                let label = if preview.is_some_and(|preview| preview.current == Some(candidate)) {
                    format!(
                        "{label} · {}",
                        self.i18n.t("plugin.cloud_sync.causal.current")
                    )
                } else {
                    label
                };
                buttons = buttons.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(div().max_w(px(360.0)).child(value))
                        .child(
                            self.workspace_toolbar_action_button(
                                label,
                                None,
                                oxideterm_gpui_cloud_sync::cloud_sync_button_options(
                                    ButtonVariant::Outline,
                                    busy,
                                ),
                                self.intent_listener(CloudSyncUiIntent::ChooseCausal {
                                    conflict: index,
                                    candidate,
                                }),
                            )
                            .into_any_element(),
                        ),
                );
            }
            body.push(cloud_sync_list_item(
                &self.tokens,
                title,
                Some(buttons.into_any_element()),
                false,
                None,
            ));
        }
        let actions = div()
            .flex()
            .gap(px(8.0))
            .child(self.render_cloud_sync_action_button(
                "plugin.cloud_sync.causal.apply",
                ButtonVariant::Default,
                busy,
                self.intent_listener(CloudSyncUiIntent::ApplyCausal),
            ))
            .child(self.render_cloud_sync_action_button(
                "plugin.cloud_sync.actions.cancel_preview",
                ButtonVariant::Outline,
                busy,
                self.intent_listener(CloudSyncUiIntent::CancelCausal),
            ))
            .into_any_element();
        cloud_sync_preview_card(
            &self.tokens,
            self.has_background,
            div()
                .child(self.i18n.t("plugin.cloud_sync.causal.preview"))
                .into_any_element(),
            [self.render_cloud_sync_fact(
                "plugin.cloud_sync.causal.changes",
                summary.changed_fields.to_string(),
                cx,
            )],
            if summary.upgrading {
                Some(self.i18n.t("plugin.cloud_sync.causal.upgrade"))
            } else if !summary.conflicts.is_empty() {
                Some(self.i18n.t("plugin.cloud_sync.causal.conflict_hint"))
            } else {
                None
            },
            body,
            actions,
        )
    }
}
