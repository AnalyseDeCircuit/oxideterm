// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn render_cloud_sync_rollback_backups(
        &mut self,
        state: &CloudSyncPersistedState,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_cloud_sync_rollback_backup_list_state(&state.rollback_backups);
        let state_handle = self.cloud_sync.view.rollback_backup_list_state.clone();
        let spec = self.cloud_sync_rollback_backup_list_spec();
        let workspace = cx.entity();
        let list_height =
            state.rollback_backups.len() as f32 * CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT;
        let title =
            self.render_cloud_sync_section_title("plugin.cloud_sync.sections.rollback_backups", cx);
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(title)
            .when(!state.rollback_backups.is_empty(), |header| {
                header.child(self.render_cloud_sync_inline_button(
                    "plugin.cloud_sync.actions.clear_backups",
                    cx.listener(
                        move |this: &mut WorkspaceApp,
                              _event,
                              _window,
                              cx: &mut Context<WorkspaceApp>| {
                            if !busy {
                                this.open_cloud_sync_clear_backups_confirm();
                            }
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                    cx,
                ))
            });
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(header)
            .child(
                div()
                    .h(px(list_height))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(tauri_virtual_list(
                        state_handle,
                        spec,
                        move |index, _window, cx| {
                            workspace.update(cx, |this, cx| {
                                this.render_cloud_sync_rollback_backup_item(index, busy, true, cx)
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    pub(super) fn sync_cloud_sync_rollback_backup_list_state(
        &self,
        backups: &[CloudSyncRollbackBackup],
    ) {
        let signatures = backups
            .iter()
            .map(cloud_sync_rollback_backup_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.cloud_sync.view.rollback_backup_list_state,
            &mut self.cloud_sync.view.rollback_backup_list_cache.borrow_mut(),
            "cloud-sync-rollback-backups",
            &signatures,
            self.cloud_sync_rollback_backup_list_spec(),
        );
    }

    pub(super) fn cloud_sync_rollback_backup_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_ROLLBACK_BACKUP_LIST_OVERSCAN,
        )
    }

    pub(super) fn render_cloud_sync_rollback_backup_item(
        &self,
        index: usize,
        busy: bool,
        show_management: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.cloud_sync.controller.store.state().clone();
        let Some(backup) = state.rollback_backups.get(index).cloned() else {
            return div().into_any_element();
        };
        let id = backup.id.clone();
        let created_at = backup.created_at.clone();
        let summary = match cloud_sync_rollback_backup_summary_spec(&backup) {
            CloudSyncRollbackBackupSummarySpec::Metadata {
                connections,
                forwards,
                quick_commands,
                serial_profiles,
                sensitive_credentials,
                plugin_settings_count,
                size,
            } => self.i18n_replace(
                "plugin.cloud_sync.backup.summary_line",
                &[
                    ("connections", connections.to_string()),
                    ("forwards", forwards.to_string()),
                    ("quickCommands", quick_commands.to_string()),
                    ("serialProfiles", serial_profiles.to_string()),
                    ("sensitiveCredentials", sensitive_credentials.to_string()),
                    ("pluginSettingsCount", plugin_settings_count.to_string()),
                    ("size", size),
                ],
            ),
            CloudSyncRollbackBackupSummarySpec::SizeOnly(size) => size,
        };
        let restore_id = id.clone();
        let restore_created_at = created_at.clone();
        let delete_id = id.clone();
        let delete_created_at = created_at.clone();
        let mut actions =
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(self.render_cloud_sync_inline_button(
                    "plugin.cloud_sync.actions.restore_backup",
                    cx.listener(
                        move |this: &mut WorkspaceApp,
                              _event,
                              _window,
                              cx: &mut Context<WorkspaceApp>| {
                            if !busy {
                                this.open_cloud_sync_restore_confirm(Some((
                                    restore_id.clone(),
                                    restore_created_at.clone(),
                                )));
                            }
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                    cx,
                ));
        if show_management {
            actions = actions.child(self.render_cloud_sync_inline_button(
                "plugin.cloud_sync.actions.delete_backup",
                cx.listener(
                    move |this: &mut WorkspaceApp,
                          _event,
                          _window,
                          cx: &mut Context<WorkspaceApp>| {
                        if !busy {
                            this.open_cloud_sync_delete_backup_confirm(
                                delete_id.clone(),
                                delete_created_at.clone(),
                            );
                        }
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ));
        }
        cloud_sync_rollback_backup_row(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-rollback-backup",
                (id.as_str(), "created-at"),
                created_at,
                self.tokens.ui.text,
                cx,
            ),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-rollback-backup",
                (id.as_str(), "summary"),
                summary,
                self.tokens.ui.text_muted,
                cx,
            ),
            actions.into_any_element(),
        )
    }

    pub(super) fn render_cloud_sync_history(
        &mut self,
        state: &CloudSyncPersistedState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let busy = self.cloud_sync.controller.delivery_rx.is_some();
        let title = self.render_display_text_with_role(
            SelectableTextRole::PlainDocument,
            "cloud-sync-history",
            "title",
            self.i18n.t("plugin.cloud_sync.sections.sync_history"),
            theme.text_heading,
            cx,
        );
        let body = if state.sync_history.is_empty() {
            cloud_sync_history_empty(
                &self.tokens,
                self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "cloud-sync-history",
                    "empty",
                    self.i18n.t("plugin.cloud_sync.history_empty"),
                    theme.text_muted,
                    cx,
                ),
            )
        } else {
            self.sync_cloud_sync_history_list_state(&state.sync_history);
            let state_handle = self.cloud_sync.view.history_list_state.clone();
            let spec = self.cloud_sync_history_list_spec();
            let workspace = cx.entity();
            let list_count = state.sync_history.len();
            div()
                .h(px(
                    list_count as f32 * CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT
                ))
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(tauri_virtual_list(
                    state_handle,
                    spec,
                    move |index, _window, cx| {
                        workspace.update(cx, |this, cx| {
                            this.render_cloud_sync_history_list_item(index, cx)
                        })
                    },
                ))
                .into_any_element()
        };
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(title)
            .when(!state.sync_history.is_empty(), |header| {
                header.child(self.render_cloud_sync_inline_button(
                    "plugin.cloud_sync.actions.clear_history",
                    cx.listener(
                        move |this: &mut WorkspaceApp,
                              _event,
                              _window,
                              cx: &mut Context<WorkspaceApp>| {
                            if !busy {
                                this.open_cloud_sync_clear_history_confirm();
                            }
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                    cx,
                ))
            });
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(header)
            .child(body)
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_recent_history(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.cloud_sync.controller.store.state().clone();
        let theme = self.tokens.ui;
        let title =
            self.render_cloud_sync_section_title("plugin.cloud_sync.overview.recent_history", cx);
        let view_all = self.render_cloud_sync_inline_button(
            "plugin.cloud_sync.overview.view_all_history",
            cx.listener(|this, _event, _window, cx| {
                if this.cloud_sync.view.active_tab != CloudSyncTab::History {
                    this.cloud_sync.view.set_active_tab(CloudSyncTab::History);
                    this.begin_user_segmented_control_transition(
                        selection_motion::CLOUD_SYNC_SWITCHER_ID,
                        cloud_sync_tab_index(CloudSyncTab::History),
                        cx,
                    );
                }
                this.clear_cloud_sync_select_focus();
                cx.stop_propagation();
                cx.notify();
            }),
            cx,
        );
        let body = if state.sync_history.is_empty() {
            cloud_sync_history_empty(
                &self.tokens,
                self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "cloud-sync-recent-history",
                    "empty",
                    self.i18n.t("plugin.cloud_sync.history_empty"),
                    theme.text_muted,
                    cx,
                ),
            )
        } else {
            let recent = state.sync_history.iter().rev().take(3).collect::<Vec<_>>();
            recent
                .iter()
                .fold(div().flex().flex_col().gap(px(8.0)), |list, entry| {
                    list.child(self.render_cloud_sync_history_entry(entry, cx))
                })
                .into_any_element()
        };
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(title)
                    .when(!state.sync_history.is_empty(), |header| {
                        header.child(view_all)
                    }),
            )
            .child(body)
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_recent_rollback_backups(
        &self,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.cloud_sync.controller.store.state().clone();
        if state.rollback_backups.is_empty() {
            return div().into_any_element();
        }
        let mut card = self
            .cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(self.render_cloud_sync_section_title(
                "plugin.cloud_sync.sections.rollback_backups",
                cx,
            ));
        for index in 0..state.rollback_backups.len().min(3) {
            card = card.child(self.render_cloud_sync_rollback_backup_item(index, busy, false, cx));
        }
        card.into_any_element()
    }

    pub(super) fn sync_cloud_sync_history_list_state(&self, history: &[CloudSyncHistoryEntry]) {
        let signatures = history
            .iter()
            .map(cloud_sync_history_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.cloud_sync.view.history_list_state,
            &mut self.cloud_sync.view.history_list_cache.borrow_mut(),
            "cloud-sync-history",
            &signatures,
            self.cloud_sync_history_list_spec(),
        );
    }

    pub(super) fn cloud_sync_history_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_HISTORY_LIST_OVERSCAN,
        )
    }

    pub(super) fn render_cloud_sync_history_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.cloud_sync.controller.store.state().clone();
        let Some(entry) = state.sync_history.get(index).cloned() else {
            return div().into_any_element();
        };
        div()
            .pb(px(8.0))
            .child(self.render_cloud_sync_history_entry(&entry, cx))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_history_entry(
        &self,
        entry: &CloudSyncHistoryEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let summary = self.i18n_replace(
            "plugin.cloud_sync.history.summary_line",
            &[
                ("connections", entry.summary.connections.to_string()),
                ("forwards", entry.summary.forwards.to_string()),
                ("quickCommands", entry.summary.quick_commands.to_string()),
                ("serialProfiles", entry.summary.serial_profiles.to_string()),
                (
                    "sensitiveCredentials",
                    entry.summary.sensitive_credentials.to_string(),
                ),
                (
                    "pluginSettingsCount",
                    entry.summary.plugin_settings_count.to_string(),
                ),
            ],
        );
        cloud_sync_history_entry(
            &self.tokens,
            self.render_selectable_text(
                crate::workspace::selectable_text::selectable_text_id(
                    "cloud-sync-history-action",
                    (&entry.id, &entry.action),
                ),
                self.cloud_sync_history_action_label(&entry.action),
                theme.text,
                cx,
            ),
            self.render_selectable_text(
                crate::workspace::selectable_text::selectable_text_id(
                    "cloud-sync-history-summary",
                    (&entry.id, &entry.timestamp),
                ),
                format!(
                    "{} · {}",
                    cloud_sync_format_timestamp(&entry.timestamp),
                    summary
                ),
                theme.text_muted,
                cx,
            ),
            entry.error.as_ref().map(|error| {
                self.render_selectable_text(
                    crate::workspace::selectable_text::selectable_text_id(
                        "cloud-sync-history-error",
                        (&entry.id, error),
                    ),
                    self.format_cloud_sync_error(error),
                    theme.error,
                    cx,
                )
            }),
        )
    }

    pub(super) fn cloud_sync_history_action_label(&self, action: &str) -> String {
        cloud_sync_history_action_label_key(action)
            .map(|key| self.i18n.t(key))
            .unwrap_or_else(|| action.to_string())
    }

    pub(super) fn render_cloud_sync_notes(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.cloud_sync.controller.store.state();
        let local_snapshot = self.cloud_sync_local_snapshot(state).ok();
        let theme = self.tokens.ui;
        let sections = local_snapshot
            .map(|snapshot| {
                snapshot
                    .scope
                    .app_settings_sections
                    .iter()
                    .map(|section| {
                        cloud_sync_app_settings_section_label_key(section)
                            .map(|key| self.i18n.t(key))
                            .unwrap_or_else(|| section.clone())
                    })
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .filter(|sections| !sections.trim().is_empty())
            .unwrap_or_else(|| "—".to_string());
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-notes",
                "title",
                self.i18n.t("plugin.cloud_sync.sections.notes"),
                theme.text_heading,
                cx,
            ))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n_replace(
                        "plugin.cloud_sync.native_scope_summary",
                        &[("sections", sections)],
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_config_connection_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let form = &self.cloud_sync.view.form;
        let mut connection_rows = Vec::new();
        for row in cloud_sync_config_rows(&form.backend_type, &form.auth_mode) {
            connection_rows.push(match row {
                CloudSyncConfigRow::BackendSelect => self.render_cloud_sync_backend_select(cx),
                CloudSyncConfigRow::AuthModeSelect => self.render_cloud_sync_auth_mode_select(cx),
                CloudSyncConfigRow::Text(field) => self.render_cloud_sync_text_field(
                    field.label_key,
                    field.input,
                    field.placeholder_key,
                    false,
                    cx,
                ),
                CloudSyncConfigRow::Secret(field) => self.render_cloud_sync_secret_field(
                    field.label_key,
                    field.input,
                    field.placeholder_key,
                    field.secret_key,
                    cx,
                ),
                CloudSyncConfigRow::AutoUploadToggle => self.render_cloud_sync_toggle(
                    "plugin.cloud_sync.settings.auto_upload_enabled",
                    form.auto_upload_enabled,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.cloud_sync.view.form.auto_upload_enabled =
                                !this.cloud_sync.view.form.auto_upload_enabled;
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                    cx,
                ),
                CloudSyncConfigRow::ConflictSelect => self.render_cloud_sync_conflict_select(cx),
            });
        }
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(self.render_cloud_sync_section_title(
                "plugin.cloud_sync.sections.connection_settings",
                cx,
            ))
            .child(cloud_sync_form_grid(connection_rows))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_config_preflight_card(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.cloud_sync.controller.store.state();
        let Some(local_snapshot) = self.cloud_sync_local_snapshot(state).ok() else {
            return div().into_any_element();
        };
        let upload_diff = self.cloud_sync_upload_diff_items_cached(&local_snapshot, state);
        if upload_diff.is_empty() {
            return div().into_any_element();
        }
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(
                self.render_cloud_sync_section_title(
                    "plugin.cloud_sync.sections.sync_preflight",
                    cx,
                ),
            )
            .child(self.render_cloud_sync_section_diff_flat(
                "cloud-sync-upload-diff",
                "plugin.cloud_sync.preflight.upload_diff_title",
                &upload_diff,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_health_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.cloud_sync.controller.store.state();
        let rows = cloud_sync_health_items(&self.cloud_sync.view.form, state)
            .into_iter()
            .map(|item| {
                self.render_cloud_sync_health_row(item.label_key, item.detail_key, item.status, cx)
            })
            .collect::<Vec<_>>();

        let theme = self.tokens.ui;
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(
                self.render_cloud_sync_section_title("plugin.cloud_sync.sections.sync_health", cx),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .line_height(px(18.0))
                    .child(self.render_selectable_text_scoped(
                        "cloud-sync-health-title",
                        "title",
                        self.i18n.t("plugin.cloud_sync.health.title"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .gap_x(px(16.0))
                    .gap_y(px(0.0))
                    .children(rows),
            )
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_health_row(
        &self,
        label_key: &'static str,
        detail_key: &'static str,
        status: CloudSyncHealthStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status_key = self.cloud_sync_health_status_key(status);
        let theme = self.tokens.ui;
        div()
            .min_w(px(260.0))
            .flex_1()
            // Health checks are rows inside the outer inspector surface, not
            // independent cards. A divider preserves scanability responsively.
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0x40))
            .py(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "cloud-sync-health-row",
                                (label_key, "label"),
                                self.i18n.t(label_key),
                                theme.text,
                                cx,
                            )),
                    )
                    .child(self.render_cloud_sync_health_chip(status, self.i18n.t(status_key), cx)),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-health-row",
                        (label_key, "detail"),
                        self.i18n.t(detail_key),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn cloud_sync_health_status_key(
        &self,
        status: CloudSyncHealthStatus,
    ) -> &'static str {
        match status {
            CloudSyncHealthStatus::Pass => "plugin.cloud_sync.health.status_pass",
            CloudSyncHealthStatus::Warning => "plugin.cloud_sync.health.status_warning",
            CloudSyncHealthStatus::Fail => "plugin.cloud_sync.health.status_fail",
        }
    }

    pub(super) fn render_cloud_sync_coverage_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.cloud_sync.controller.store.state();
        let rows = cloud_sync_coverage_model(&state.sync_scope)
            .into_iter()
            .map(|item| {
                self.render_cloud_sync_status_row(
                    item.label_key,
                    Some(self.cloud_sync_coverage_detail(item.detail)),
                    item.status,
                    cx,
                )
            })
            .collect::<Vec<_>>();
        let theme = self.tokens.ui;
        let coverage_title = self.render_selectable_text_scoped(
            "cloud-sync-coverage-title",
            "title",
            self.i18n.t("plugin.cloud_sync.coverage.title"),
            theme.text_heading,
            cx,
        );
        // The outer inspector surface owns the section chrome; this list only
        // provides hierarchy and spacing for its status rows.
        let coverage_list = rows.into_iter().fold(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .pb(px(4.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_heading))
                        .child(coverage_title),
                ),
            |list, row| list.child(row),
        );
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(
                self.render_cloud_sync_section_title(
                    "plugin.cloud_sync.sections.sync_coverage",
                    cx,
                ),
            )
            .child(coverage_list)
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_status_row(
        &self,
        label_key: &'static str,
        detail: Option<String>,
        status: CloudSyncCoverageStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        let status_key = self.cloud_sync_coverage_status_key(status);
        cloud_sync_status_row(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-status-row",
                (label_key, "label"),
                label,
                self.tokens.ui.text,
                cx,
            ),
            detail.map(|detail| {
                self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "cloud-sync-status-row",
                    (label_key, "detail"),
                    detail,
                    self.tokens.ui.text_muted,
                    cx,
                )
            }),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-status-row",
                (label_key, "status"),
                self.i18n.t(status_key),
                self.tokens.ui.accent,
                cx,
            ),
            status != CloudSyncCoverageStatus::Excluded,
        )
    }

    pub(super) fn cloud_sync_coverage_status_key(
        &self,
        status: CloudSyncCoverageStatus,
    ) -> &'static str {
        match status {
            CloudSyncCoverageStatus::Included => "plugin.cloud_sync.coverage.status_included",
            CloudSyncCoverageStatus::Excluded => "plugin.cloud_sync.coverage.status_excluded",
            CloudSyncCoverageStatus::Partial => "plugin.cloud_sync.coverage.status_partial",
        }
    }

    pub(super) fn cloud_sync_coverage_detail(&self, detail: CloudSyncCoverageDetail) -> String {
        match detail {
            CloudSyncCoverageDetail::Static(key) => self.i18n.t(key),
            CloudSyncCoverageDetail::AppSettingsSections(section_ids) => {
                if section_ids.is_empty() {
                    return self
                        .i18n
                        .t("plugin.cloud_sync.coverage.app_settings_disabled_detail");
                }
                let sections = section_ids
                    .into_iter()
                    .map(|section_id| {
                        cloud_sync_app_settings_section_label_key(&section_id)
                            .map(|key| self.i18n.t(key))
                            .unwrap_or(section_id)
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                self.i18n_replace(
                    "plugin.cloud_sync.coverage.app_settings_sections_detail",
                    &[("sections", sections)],
                )
            }
            CloudSyncCoverageDetail::PluginSettings(plugin_ids) => match plugin_ids {
                None => self
                    .i18n
                    .t("plugin.cloud_sync.coverage.plugin_settings_all_detail"),
                Some(ids) if ids.is_empty() => self
                    .i18n
                    .t("plugin.cloud_sync.coverage.plugin_settings_disabled_detail"),
                Some(ids) => self.i18n_replace(
                    "plugin.cloud_sync.coverage.plugin_settings_selected_detail",
                    &[("plugins", ids.join(", "))],
                ),
            },
        }
    }
}
