// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn render_cloud_sync_preview(
        &self,
        preview: &CloudSyncPendingPreview,
        state: &CloudSyncPersistedState,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let model = cloud_sync_preview_card_model(
            preview,
            state,
            self.cloud_sync.view.preview_selection.as_ref(),
        );
        let title = self.render_display_text_with_role(
            SelectableTextRole::PlainDocument,
            "cloud-sync-preview-title",
            model.copy.title_identity,
            self.i18n.t(model.copy.title_key),
            theme.text_heading,
            cx,
        );
        let fact_rows = model
            .fact_rows
            .iter()
            .map(|row| {
                cloud_sync_fact_grid(row.iter().map(|fact| {
                    self.render_cloud_sync_fact(
                        fact.label_key,
                        self.cloud_sync_preview_fact_value(&fact.value),
                        cx,
                    )
                }))
            })
            .collect::<Vec<_>>();
        let warning = model.copy.warning_key.map(|key| self.i18n.t(key));
        let mut body = Vec::new();
        let local_snapshot = self.cloud_sync_local_snapshot(state).ok();
        let apply_diff_items =
            cloud_sync_apply_diff_items(preview, &model.selection, local_snapshot.as_ref());
        if !apply_diff_items.is_empty() {
            body.push(self.render_cloud_sync_section_diff_card(
                "cloud-sync-apply-diff",
                "plugin.cloud_sync.preflight.apply_diff_title",
                &apply_diff_items,
                cx,
            ));
        }
        let field_diff_items = cloud_sync_apply_field_diff_items(
            preview,
            &model.selection,
            &self.cloud_sync_local_field_diff_snapshot(),
        );
        if !field_diff_items.is_empty() {
            body.push(self.render_cloud_sync_apply_field_diff_card(&field_diff_items, cx));
        }
        if let Some(card) = self.render_cloud_sync_remote_sensitive_summary(preview, cx) {
            body.push(card);
        }
        if !model.impact_items.is_empty() {
            body.push(self.render_cloud_sync_preview_impact(&model.impact_items, cx));
        }
        for section in &model.body_sections {
            body.push(match section {
                CloudSyncPreviewBodySection::Selection => {
                    self.render_cloud_sync_preview_selection(&model.summary, &model.selection, cx)
                }
                CloudSyncPreviewBodySection::ForwardDetails(details) => {
                    self.render_cloud_sync_forward_details(&details, cx)
                }
                CloudSyncPreviewBodySection::RecordGroup { action, records } => {
                    self.render_cloud_sync_record_group(action, &records, &model.selection, cx)
                }
            });
        }
        cloud_sync_preview_card(
            &self.tokens,
            self.cloud_sync_has_background(),
            title,
            fact_rows,
            warning,
            body,
            cloud_sync_action_grid([
                self.render_cloud_sync_action_button(
                    model.copy.apply_label_key,
                    ButtonVariant::Default,
                    busy || !model.can_apply,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.open_cloud_sync_import_confirm();
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                ),
                self.render_cloud_sync_action_button(
                    "plugin.cloud_sync.actions.cancel_preview",
                    ButtonVariant::Outline,
                    busy,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.cloud_sync.view.pending_preview = None;
                            this.cloud_sync.view.preview_selection = None;
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                ),
            ]),
        )
    }

    pub(super) fn render_cloud_sync_upload_preview(
        &self,
        remote_preview: &CloudSyncPendingPreview,
        state: &CloudSyncPersistedState,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let title = self.render_display_text_with_role(
            SelectableTextRole::PlainDocument,
            "cloud-sync-upload-preview-title",
            "upload",
            self.i18n.t("plugin.cloud_sync.sections.upload_preview"),
            theme.text_heading,
            cx,
        );
        let mut body = Vec::new();
        if let Some(selection) = self.cloud_sync.view.upload_selection.as_ref() {
            body.push(self.render_cloud_sync_upload_selection(selection, cx));
        }
        if let Ok(local_snapshot) = self.cloud_sync_local_snapshot(state) {
            let mut preview_state = state.clone();
            if let CloudSyncPendingPreview::Structured(preview) = remote_preview {
                preview_state.remote_exists = true;
                preview_state.remote_section_revisions =
                    Some(preview.manifest.section_revisions.clone());
            }
            if let Some(selection) = self.cloud_sync.view.upload_selection.as_ref() {
                preview_state.sync_scope = selection.raw_scope(&state.sync_scope);
            }
            let section_diff_items =
                self.cloud_sync_upload_diff_items_cached(&local_snapshot, &preview_state);
            if !section_diff_items.is_empty() {
                body.push(self.render_cloud_sync_section_diff_card(
                    "cloud-sync-upload-preview-diff",
                    "plugin.cloud_sync.preflight.upload_diff_title",
                    &section_diff_items,
                    cx,
                ));
            }
        }
        let raw_scope = self
            .cloud_sync
            .view
            .upload_selection
            .as_ref()
            .map(|selection| selection.raw_scope(&state.sync_scope))
            .unwrap_or_else(|| state.sync_scope.clone());
        let scope = normalize_sync_scope(Some(&raw_scope), &[]);
        let field_diff_items = cloud_sync_upload_field_diff_items(
            remote_preview,
            &self.cloud_sync_local_field_diff_snapshot(),
            &scope,
        );
        if !field_diff_items.is_empty() {
            body.push(self.render_cloud_sync_upload_field_diff_card(&field_diff_items, cx));
        }
        let summary = cloud_sync_preview_summary(remote_preview);
        let fact_rows = if matches!(remote_preview, CloudSyncPendingPreview::Structured(_)) {
            Vec::new()
        } else {
            vec![cloud_sync_fact_grid([
                self.render_cloud_sync_fact(
                    "plugin.cloud_sync.preview.connection_count",
                    summary.connections.to_string(),
                    cx,
                ),
                self.render_cloud_sync_fact(
                    "plugin.cloud_sync.preview.quick_commands_label",
                    summary.quick_commands.to_string(),
                    cx,
                ),
            ])]
        };
        cloud_sync_preview_card(
            &self.tokens,
            self.cloud_sync_has_background(),
            title,
            fact_rows,
            None,
            body,
            cloud_sync_action_grid([
                self.render_cloud_sync_action_button(
                    "plugin.cloud_sync.actions.upload_now",
                    ButtonVariant::Default,
                    busy,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.cloud_sync.view.upload_preview = None;
                            this.start_cloud_sync_upload_with_options(false, false, false, cx);
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                ),
                self.render_cloud_sync_action_button(
                    "plugin.cloud_sync.actions.cancel_preview",
                    ButtonVariant::Outline,
                    busy,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.cloud_sync.view.upload_preview = None;
                            this.cloud_sync.view.upload_selection = None;
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                ),
            ]),
        )
    }

    pub(super) fn render_cloud_sync_preview_impact(
        &self,
        items: &[CloudSyncPreviewImpactItem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.render_selectable_text_scoped(
            "cloud-sync-preview-impact-title",
            "title",
            self.i18n.t("plugin.cloud_sync.preview.apply_plan_title"),
            self.tokens.ui.text_heading,
            cx,
        );
        let rows = items.iter().map(|item| {
            self.render_cloud_sync_status_row(
                item.label_key,
                Some(self.i18n_replace(
                    "plugin.cloud_sync.preview.item_count",
                    &[("count", item.count.to_string())],
                )),
                item.status,
                cx,
            )
        });
        cloud_sync_status_list(&self.tokens, title, rows)
    }

    pub(super) fn render_cloud_sync_remote_sensitive_summary(
        &self,
        preview: &CloudSyncPendingPreview,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let (connections, portable_secrets) = match preview {
            CloudSyncPendingPreview::Structured(preview) => {
                let preview = preview.sensitive_credentials_preview.as_ref()?;
                (preview.total_connections, preview.portable_secret_count)
            }
            CloudSyncPendingPreview::Legacy { preview, .. } => (
                preview.preview.total_connections,
                preview.metadata.portable_secret_count.unwrap_or(0),
            ),
        };
        if connections == 0 && portable_secrets == 0 {
            return None;
        }
        let title = self.render_selectable_text_scoped(
            "cloud-sync-sensitive-summary-title",
            "title",
            self.i18n
                .t("plugin.cloud_sync.preview.sensitive_summary_title"),
            self.tokens.ui.text_heading,
            cx,
        );
        let summary = self.i18n_replace(
            "plugin.cloud_sync.preview.sensitive_remote_summary",
            &[
                ("connections", connections.to_string()),
                ("portableSecrets", portable_secrets.to_string()),
            ],
        );
        Some(cloud_sync_status_list(
            &self.tokens,
            title,
            [cloud_sync_meta_line(self.render_selectable_text_scoped(
                "cloud-sync-sensitive-summary",
                "summary",
                summary,
                self.tokens.ui.text,
                cx,
            ))],
        ))
    }

    pub(super) fn render_cloud_sync_upload_selection(
        &self,
        selection: &CloudSyncUploadSelection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.render_selectable_text_scoped(
            "cloud-sync-upload-selection-title",
            "title",
            self.i18n.t("plugin.cloud_sync.preview.upload_scope_title"),
            self.tokens.ui.text_heading,
            cx,
        );
        let rows = [
            (
                "plugin.cloud_sync.settings.sync_connections",
                CloudSyncUploadSelectionAction::ToggleConnections,
            ),
            (
                "plugin.cloud_sync.settings.sync_forwards",
                CloudSyncUploadSelectionAction::ToggleForwards,
            ),
            (
                "plugin.cloud_sync.settings.sync_quick_commands",
                CloudSyncUploadSelectionAction::ToggleQuickCommands,
            ),
            (
                "plugin.cloud_sync.settings.sync_serial_profiles",
                CloudSyncUploadSelectionAction::ToggleSerialProfiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_sensitive_credentials",
                CloudSyncUploadSelectionAction::ToggleSensitiveCredentials,
            ),
            (
                "plugin.cloud_sync.settings.sync_app_settings",
                CloudSyncUploadSelectionAction::ToggleAppSettings,
            ),
            (
                "plugin.cloud_sync.settings.sync_plugin_settings",
                CloudSyncUploadSelectionAction::TogglePluginSettings,
            ),
        ];
        let mut block = div().flex().flex_col().gap(px(8.0));
        for (label_key, action) in rows {
            if !self.cloud_sync_upload_section_visible(selection, &action) {
                continue;
            }
            let checked = selection.is_item_checked(&action);
            block = block.child(self.render_cloud_sync_check_row(
                self.i18n.t(label_key),
                self.cloud_sync_upload_selection_meta(selection, &action),
                checked,
                false,
                cx.listener(move |this, _event, _window, cx| {
                    this.apply_cloud_sync_upload_selection_action(action.clone());
                    cx.stop_propagation();
                    cx.notify();
                }),
                cx,
            ));
        }
        cloud_sync_status_list(&self.tokens, title, [block.into_any_element()])
    }

    pub(super) fn cloud_sync_upload_section_visible(
        &self,
        selection: &CloudSyncUploadSelection,
        action: &CloudSyncUploadSelectionAction,
    ) -> bool {
        match action {
            CloudSyncUploadSelectionAction::ToggleConnections => {
                selection.sync_connections || !selection.connection_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleForwards => {
                selection.sync_forwards || !selection.forward_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleQuickCommands => {
                selection.sync_quick_commands || !selection.quick_command_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleSerialProfiles => {
                selection.sync_serial_profiles || !selection.serial_profile_item_ids.is_empty()
            }
            CloudSyncUploadSelectionAction::ToggleSensitiveCredentials => {
                selection.sync_sensitive_credentials
            }
            CloudSyncUploadSelectionAction::ToggleAppSettings => selection.sync_app_settings,
            CloudSyncUploadSelectionAction::TogglePluginSettings => selection.sync_plugin_settings,
            _ => true,
        }
    }

    pub(super) fn cloud_sync_upload_selection_meta(
        &self,
        selection: &CloudSyncUploadSelection,
        action: &CloudSyncUploadSelectionAction,
    ) -> Option<String> {
        let count = match action {
            CloudSyncUploadSelectionAction::ToggleConnections => {
                selection.connection_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleForwards => selection.forward_item_ids.len(),
            CloudSyncUploadSelectionAction::ToggleQuickCommands => {
                selection.quick_command_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleSerialProfiles => {
                selection.serial_profile_item_ids.len()
            }
            CloudSyncUploadSelectionAction::ToggleSensitiveCredentials => {
                return self.cloud_sync_upload_sensitive_summary(selection);
            }
            CloudSyncUploadSelectionAction::ToggleAppSettings => {
                selection.selected_app_settings_sections.len()
            }
            _ => 0,
        };
        (count > 0).then(|| {
            self.i18n_replace(
                "plugin.cloud_sync.preview.item_count",
                &[("count", count.to_string())],
            )
        })
    }

    pub(super) fn cloud_sync_upload_sensitive_summary(
        &self,
        selection: &CloudSyncUploadSelection,
    ) -> Option<String> {
        if !selection.sync_sensitive_credentials {
            return None;
        }
        let connection_ids = self
            .connection_store
            .connections()
            .iter()
            .map(|connection| connection.id.clone())
            .filter(|connection_id| {
                selection
                    .selected_connection_ids
                    .as_ref()
                    .is_none_or(|ids| ids.contains(connection_id))
            })
            .collect::<Vec<_>>();
        let portable_secret_count =
            oxideterm_ai::provider_views(&self.settings_store.settings().ai.providers)
                .into_iter()
                .filter(|provider| self.ai.models.key_store.has_provider_key(&provider.id))
                .count();
        let preflight = oxideterm_connections::oxide_file::preflight_export(
            &self.connection_store,
            &connection_ids,
            true,
            true,
            portable_secret_count,
        );
        Some(self.i18n_replace(
            "plugin.cloud_sync.preview.sensitive_upload_summary",
            &[
                (
                    "passwords",
                    preflight.connections_with_passwords.to_string(),
                ),
                ("keyPassphrases", preflight.key_passphrase_count.to_string()),
                ("managedKeys", preflight.managed_key_count.to_string()),
                (
                    "managedKeyPassphrases",
                    preflight.managed_key_passphrase_count.to_string(),
                ),
                (
                    "portableSecrets",
                    preflight.portable_secret_count.to_string(),
                ),
            ],
        ))
    }

    pub(super) fn render_cloud_sync_section_diff_card(
        &self,
        identity: &'static str,
        title_key: &'static str,
        items: &[CloudSyncSectionDiffItem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.render_cloud_sync_section_diff_content(identity, title_key, items, false, cx)
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x55))
            .bg(rgba((theme.bg_panel << 8) | 0x5F))
            .p(px(12.0))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_section_diff_flat(
        &self,
        identity: &'static str,
        title_key: &'static str,
        items: &[CloudSyncSectionDiffItem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_cloud_sync_section_diff_content(identity, title_key, items, true, cx)
            .into_any_element()
    }

    fn render_cloud_sync_section_diff_content(
        &self,
        identity: &'static str,
        title_key: &'static str,
        items: &[CloudSyncSectionDiffItem],
        flat_rows: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        // Preview dialogs may need their own framed block, while configuration
        // pages reuse the same content inside an existing inspector surface.
        let theme = self.tokens.ui;
        let title = self.render_selectable_text_scoped(
            identity,
            "title",
            self.i18n.t(title_key),
            theme.text_muted,
            cx,
        );
        let rows = items
            .iter()
            .map(|item| self.render_cloud_sync_section_diff_row(item, flat_rows, cx))
            .collect::<Vec<_>>();
        let grid = rows.into_iter().fold(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_wrap()
                .gap(px(8.0)),
            |grid, row| {
                grid.child(
                    div()
                        .min_w(px(CLOUD_SYNC_SECTION_DIFF_ITEM_MIN_WIDTH))
                        .flex_1()
                        .child(row),
                )
            },
        );
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_muted))
                    .child(title),
            )
            .child(grid)
    }

    pub(super) fn render_cloud_sync_section_diff_row(
        &self,
        item: &CloudSyncSectionDiffItem,
        flat: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.cloud_sync_diff_label(&item.label);
        let local_status = self
            .i18n
            .t(self.cloud_sync_local_diff_status_key(item.local_status));
        let remote_status = self
            .i18n
            .t(self.cloud_sync_remote_diff_status_key(item.remote_status));
        let theme = self.tokens.ui;
        let count = item.count.map(|count| {
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-section-diff-row",
                (label.clone(), "count"),
                self.i18n_replace(
                    "plugin.cloud_sync.preview.item_count",
                    &[("count", count.to_string())],
                ),
                theme.text_muted,
                cx,
            )
        });
        div()
            .w_full()
            .min_w(px(0.0))
            .py(px(8.0))
            .when(!flat, |row| {
                row.rounded(px(self.tokens.radii.sm))
                    .bg(rgba((theme.bg_card << 8) | 0x66))
                    .px(px(10.0))
            })
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "cloud-sync-section-diff-row",
                                (label.clone(), "label"),
                                label,
                                theme.text,
                                cx,
                            )),
                    )
                    .when_some(count, |content, count| {
                        content.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(count),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .justify_end()
                    .gap(px(6.0))
                    .child(self.render_cloud_sync_local_diff_chip(
                        item.local_status,
                        local_status,
                        cx,
                    ))
                    .child(self.render_cloud_sync_remote_diff_chip(
                        item.remote_status,
                        remote_status,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn cloud_sync_diff_label(&self, label: &CloudSyncDiffLabel) -> String {
        match label {
            CloudSyncDiffLabel::Key(key) => self.i18n.t(key),
            CloudSyncDiffLabel::AppSettingsSection(section_id) => {
                cloud_sync_app_settings_section_label_key(section_id)
                    .map(|key| self.i18n.t(key))
                    .unwrap_or_else(|| section_id.clone())
            }
            CloudSyncDiffLabel::PluginSettings(plugin_id) => self.i18n_replace(
                "plugin.cloud_sync.preflight.plugin_settings_item",
                &[("plugin", plugin_id.clone())],
            ),
        }
    }

    pub(super) fn cloud_sync_local_diff_status_key(
        &self,
        status: CloudSyncLocalDiffStatus,
    ) -> &'static str {
        match status {
            CloudSyncLocalDiffStatus::Added => "plugin.cloud_sync.preflight.local_added",
            CloudSyncLocalDiffStatus::Modified => "plugin.cloud_sync.preflight.local_modified",
            CloudSyncLocalDiffStatus::Deleted => "plugin.cloud_sync.preflight.local_deleted",
            CloudSyncLocalDiffStatus::Unchanged => "plugin.cloud_sync.preflight.local_unchanged",
            CloudSyncLocalDiffStatus::Excluded => "plugin.cloud_sync.preflight.local_excluded",
        }
    }

    pub(super) fn cloud_sync_remote_diff_status_key(
        &self,
        status: CloudSyncRemoteDiffStatus,
    ) -> &'static str {
        match status {
            CloudSyncRemoteDiffStatus::Creates => "plugin.cloud_sync.preflight.remote_creates",
            CloudSyncRemoteDiffStatus::Overwrites => {
                "plugin.cloud_sync.preflight.remote_overwrites"
            }
            CloudSyncRemoteDiffStatus::Unchanged => "plugin.cloud_sync.preflight.remote_unchanged",
            CloudSyncRemoteDiffStatus::RemovedByScope => {
                "plugin.cloud_sync.preflight.remote_removed_by_scope"
            }
            CloudSyncRemoteDiffStatus::Excluded => "plugin.cloud_sync.preflight.remote_excluded",
            CloudSyncRemoteDiffStatus::Unknown => "plugin.cloud_sync.preflight.remote_unknown",
        }
    }

    pub(super) fn render_cloud_sync_local_diff_chip(
        &self,
        status: CloudSyncLocalDiffStatus,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_cloud_sync_tone_chip(
            local_diff_tone(status),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-section-diff-chip",
                (self.cloud_sync_local_diff_status_key(status), "local"),
                label,
                local_diff_tone(status).color(&self.tokens),
                cx,
            ),
        )
    }

    pub(super) fn render_cloud_sync_remote_diff_chip(
        &self,
        status: CloudSyncRemoteDiffStatus,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_cloud_sync_tone_chip(
            remote_diff_tone(status),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-section-diff-chip",
                (self.cloud_sync_remote_diff_status_key(status), "remote"),
                label,
                remote_diff_tone(status).color(&self.tokens),
                cx,
            ),
        )
    }

    pub(super) fn render_cloud_sync_health_chip(
        &self,
        status: CloudSyncHealthStatus,
        label: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_cloud_sync_tone_chip(
            health_tone(status),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-health-chip",
                (self.cloud_sync_health_status_key(status), "status"),
                label,
                health_tone(status).color(&self.tokens),
                cx,
            ),
        )
    }

    pub(super) fn render_cloud_sync_tone_chip(
        &self,
        tone: CloudSyncTone,
        label: AnyElement,
    ) -> AnyElement {
        status_pill_element(
            &self.tokens,
            label,
            StatusPillOptions::new(cloud_sync_status_tone(tone)).compact(),
        )
        .into_any_element()
    }

    pub(super) fn render_cloud_sync_apply_field_diff_card(
        &self,
        items: &[CloudSyncFieldDiffItem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.render_selectable_text_scoped(
            "cloud-sync-apply-field-diff",
            "title",
            self.i18n.t("plugin.cloud_sync.field_diff.title"),
            self.tokens.ui.text_heading,
            cx,
        );
        let rows = items
            .iter()
            .map(|item| self.render_cloud_sync_apply_field_diff_item(item, cx));
        cloud_sync_status_list(&self.tokens, title, rows)
    }

    pub(super) fn render_cloud_sync_apply_field_diff_item(
        &self,
        item: &CloudSyncFieldDiffItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(action) = self.cloud_sync_apply_action_for_field_item(item) else {
            return self.render_cloud_sync_field_diff_item(item, cx);
        };
        let checked = self
            .cloud_sync
            .view
            .preview_selection
            .as_ref()
            .is_none_or(|selection| self.cloud_sync_apply_field_item_checked(selection, &action));
        let label = self.cloud_sync_field_diff_item_name(item);
        let meta = Some(
            self.i18n_replace(
                "plugin.cloud_sync.field_diff.item_action_meta",
                &[
                    ("section", self.i18n.t(item.section_label_key)),
                    (
                        "status",
                        self.i18n
                            .t(self.cloud_sync_field_diff_status_key(item.status)),
                    ),
                ],
            ),
        );
        self.render_cloud_sync_check_row(
            label,
            meta,
            checked,
            false,
            cx.listener(move |this, _event, _window, cx| {
                this.apply_cloud_sync_preview_selection_action(action.clone());
                cx.stop_propagation();
                cx.notify();
            }),
            cx,
        )
    }

    pub(super) fn cloud_sync_apply_action_for_field_item(
        &self,
        item: &CloudSyncFieldDiffItem,
    ) -> Option<CloudSyncPreviewSelectionAction> {
        match item.section_label_key {
            "plugin.cloud_sync.settings.sync_connections" => Some(
                CloudSyncPreviewSelectionAction::ToggleConnectionItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_forwards" => Some(
                CloudSyncPreviewSelectionAction::ToggleForwardItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_quick_commands" => Some(
                CloudSyncPreviewSelectionAction::ToggleQuickCommandItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_serial_profiles" => Some(
                CloudSyncPreviewSelectionAction::ToggleSerialProfileItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_app_settings" => Some(
                CloudSyncPreviewSelectionAction::ToggleAppSettingsSection(item.item_key.clone()),
            ),
            _ => None,
        }
    }

    pub(super) fn cloud_sync_apply_field_item_checked(
        &self,
        selection: &CloudSyncPreviewSelection,
        action: &CloudSyncPreviewSelectionAction,
    ) -> bool {
        match action {
            CloudSyncPreviewSelectionAction::ToggleConnectionItem(id) => {
                selection.selected_connection_ids.contains(id)
            }
            CloudSyncPreviewSelectionAction::ToggleForwardItem(id) => {
                selection.selected_forward_ids.contains(id)
            }
            CloudSyncPreviewSelectionAction::ToggleQuickCommandItem(id) => {
                selection.selected_quick_command_ids.contains(id)
            }
            CloudSyncPreviewSelectionAction::ToggleSerialProfileItem(id) => {
                selection.selected_serial_profile_ids.contains(id)
            }
            CloudSyncPreviewSelectionAction::ToggleAppSettingsSection(id) => {
                selection.selected_app_settings_sections.contains(id)
            }
            _ => true,
        }
    }

    pub(super) fn render_cloud_sync_upload_field_diff_card(
        &self,
        items: &[CloudSyncFieldDiffItem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.render_selectable_text_scoped(
            "cloud-sync-upload-field-diff",
            "title",
            self.i18n.t("plugin.cloud_sync.field_diff.title"),
            self.tokens.ui.text_heading,
            cx,
        );
        let rows = items
            .iter()
            .map(|item| self.render_cloud_sync_upload_field_diff_item(item, cx));
        cloud_sync_status_list(&self.tokens, title, rows)
    }

    pub(super) fn render_cloud_sync_upload_field_diff_item(
        &self,
        item: &CloudSyncFieldDiffItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(action) = self.cloud_sync_upload_action_for_field_item(item) else {
            return self.render_cloud_sync_field_diff_item(item, cx);
        };
        let checked = self
            .cloud_sync
            .view
            .upload_selection
            .as_ref()
            .is_none_or(|selection| selection.is_item_checked(&action));
        let label = self.cloud_sync_field_diff_item_name(item);
        let meta = Some(
            self.i18n_replace(
                "plugin.cloud_sync.field_diff.item_action_meta",
                &[
                    ("section", self.i18n.t(item.section_label_key)),
                    (
                        "status",
                        self.i18n
                            .t(self.cloud_sync_field_diff_status_key(item.status)),
                    ),
                ],
            ),
        );
        self.render_cloud_sync_check_row(
            label,
            meta,
            checked,
            false,
            cx.listener(move |this, _event, _window, cx| {
                this.apply_cloud_sync_upload_selection_action(action.clone());
                cx.stop_propagation();
                cx.notify();
            }),
            cx,
        )
    }

    pub(super) fn cloud_sync_upload_action_for_field_item(
        &self,
        item: &CloudSyncFieldDiffItem,
    ) -> Option<CloudSyncUploadSelectionAction> {
        match item.section_label_key {
            "plugin.cloud_sync.settings.sync_connections" => Some(
                CloudSyncUploadSelectionAction::ToggleConnectionItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_forwards" => Some(
                CloudSyncUploadSelectionAction::ToggleForwardItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_quick_commands" => Some(
                CloudSyncUploadSelectionAction::ToggleQuickCommandItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_serial_profiles" => Some(
                CloudSyncUploadSelectionAction::ToggleSerialProfileItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_app_settings" => Some(
                CloudSyncUploadSelectionAction::ToggleAppSettingsSection(item.item_key.clone()),
            ),
            _ => None,
        }
    }

    pub(super) fn render_cloud_sync_field_diff_item(
        &self,
        item: &CloudSyncFieldDiffItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let status_key = self.cloud_sync_field_diff_status_key(item.status);
        let item_name = self.cloud_sync_field_diff_item_name(item);
        let fields =
            item.fields
                .iter()
                .fold(div().flex().flex_col().gap(px(2.0)), |fields, field| {
                    let mut field_text = self.i18n_replace(
                        "plugin.cloud_sync.field_diff.field_change",
                        &[
                            ("field", self.i18n.t(field.label_key)),
                            (
                                "before",
                                self.cloud_sync_field_diff_value(field.before.as_deref()),
                            ),
                            (
                                "after",
                                self.cloud_sync_field_diff_value(field.after.as_deref()),
                            ),
                        ],
                    );
                    if let Some(outcome) = field.merge_outcome {
                        field_text.push_str(" · ");
                        field_text.push_str(
                            &self
                                .i18n
                                .t(self.cloud_sync_field_merge_outcome_key(outcome)),
                        );
                    }
                    fields.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text_muted))
                            .child(field_text),
                    )
                });
        let detail = div()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t(item.section_label_key)),
            )
            .when(!item.fields.is_empty(), |detail| detail.child(fields));
        cloud_sync_status_row(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-field-diff-row",
                (item.section_label_key, item.item_name.as_str(), "label"),
                item_name,
                self.tokens.ui.text,
                cx,
            ),
            Some(detail.into_any_element()),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-field-diff-row",
                (item.section_label_key, item.item_name.as_str(), "status"),
                self.i18n.t(status_key),
                self.tokens.ui.accent,
                cx,
            ),
            true,
        )
    }

    pub(super) fn cloud_sync_field_diff_item_name(&self, item: &CloudSyncFieldDiffItem) -> String {
        if item.section_label_key == "plugin.cloud_sync.settings.sync_app_settings" {
            cloud_sync_app_settings_section_label_key(&item.item_name)
                .map(|key| self.i18n.t(key))
                .unwrap_or_else(|| item.item_name.clone())
        } else {
            item.item_name.clone()
        }
    }

    pub(super) fn cloud_sync_field_diff_status_key(
        &self,
        status: CloudSyncFieldDiffStatus,
    ) -> &'static str {
        match status {
            CloudSyncFieldDiffStatus::Added => "plugin.cloud_sync.field_diff.status_added",
            CloudSyncFieldDiffStatus::Modified => "plugin.cloud_sync.field_diff.status_modified",
            CloudSyncFieldDiffStatus::Deleted => "plugin.cloud_sync.field_diff.status_deleted",
        }
    }

    pub(super) fn cloud_sync_field_merge_outcome_key(
        &self,
        outcome: CloudSyncFieldMergeOutcome,
    ) -> &'static str {
        match outcome {
            CloudSyncFieldMergeOutcome::Remote => "plugin.cloud_sync.field_diff.outcome_remote",
            CloudSyncFieldMergeOutcome::Local => "plugin.cloud_sync.field_diff.outcome_local",
            CloudSyncFieldMergeOutcome::Merged => "plugin.cloud_sync.field_diff.outcome_merged",
            CloudSyncFieldMergeOutcome::ConflictLocal => {
                "plugin.cloud_sync.field_diff.outcome_conflict_local"
            }
            CloudSyncFieldMergeOutcome::ConflictRemote => {
                "plugin.cloud_sync.field_diff.outcome_conflict_remote"
            }
        }
    }

    pub(super) fn cloud_sync_field_diff_value(&self, value: Option<&str>) -> String {
        match value {
            Some(CLOUD_SYNC_FIELD_REDACTED_VALUE) => {
                self.i18n.t("plugin.cloud_sync.field_diff.redacted")
            }
            Some(value) if !value.trim().is_empty() => value.to_string(),
            _ => self.i18n.t("plugin.cloud_sync.field_diff.empty_value"),
        }
    }

    pub(super) fn render_cloud_sync_preview_selection(
        &self,
        summary: &CloudSyncPreviewSummary,
        selection: &CloudSyncPreviewSelection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut block = div().flex().flex_col().gap(px(8.0));
        for row in selection.preview_rows(summary) {
            let action = row.action.clone();
            block = block.child(
                self.render_cloud_sync_check_row(
                    self.cloud_sync_preview_selection_label(row.label),
                    row.meta
                        .map(|label| self.cloud_sync_preview_selection_label(label)),
                    row.checked,
                    row.disabled,
                    cx.listener(move |this, _event, _window, cx| {
                        this.apply_cloud_sync_preview_selection_action(action.clone());
                        cx.stop_propagation();
                        cx.notify();
                    }),
                    cx,
                ),
            );
        }
        block.into_any_element()
    }

    pub(super) fn apply_cloud_sync_preview_selection_action(
        &mut self,
        action: CloudSyncPreviewSelectionAction,
    ) {
        let all_connection_names = self
            .cloud_sync
            .view
            .pending_preview
            .as_ref()
            .map(cloud_sync_preview_summary)
            .map(|summary| summary.connection_record_names())
            .unwrap_or_default();
        if let Some(selection) = self.cloud_sync.view.preview_selection.as_mut() {
            selection.apply_action(action, all_connection_names);
        }
    }

    pub(super) fn apply_cloud_sync_upload_selection_action(
        &mut self,
        action: CloudSyncUploadSelectionAction,
    ) {
        if let Some(selection) = self.cloud_sync.view.upload_selection.as_mut() {
            selection.apply_action(action);
        }
    }

    pub(super) fn cloud_sync_preview_selection_label(
        &self,
        label: CloudSyncPreviewSelectionLabel,
    ) -> String {
        match label {
            CloudSyncPreviewSelectionLabel::I18nCount {
                key,
                count_name,
                count,
            } => self.i18n_replace(key, &[(count_name, count.to_string())]),
            CloudSyncPreviewSelectionLabel::AppSettings => {
                self.i18n.t("plugin.cloud_sync.preview.toggle_app_settings")
            }
            CloudSyncPreviewSelectionLabel::AppSettingsSection { section_id } => {
                cloud_sync_app_settings_section_label_key(&section_id)
                    .map(|key| self.i18n.t(key))
                    .unwrap_or(section_id)
            }
            CloudSyncPreviewSelectionLabel::PluginId(plugin_id) => plugin_id,
        }
    }

    pub(super) fn cloud_sync_preview_fact_value(
        &self,
        value: &CloudSyncPreviewFactValue,
    ) -> String {
        match value {
            CloudSyncPreviewFactValue::Count(count) => count.to_string(),
            CloudSyncPreviewFactValue::YesNo(true) => self.i18n.t("plugin.cloud_sync.common.yes"),
            CloudSyncPreviewFactValue::YesNo(false) => self.i18n.t("plugin.cloud_sync.common.no"),
        }
    }

    pub(super) fn render_cloud_sync_check_row(
        &self,
        label: String,
        meta: Option<String>,
        checked: bool,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label_key = label.clone();
        // Preview rows toggle on the row, matching Tauri checkbox row select-none labels.
        cloud_sync_check_row(
            &self.tokens,
            checked,
            disabled,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-preview-check-label",
                label_key,
                label,
                if disabled {
                    theme.text_muted
                } else {
                    theme.text
                },
                cx,
            ),
            meta.map(|meta| {
                let meta_key = meta.clone();
                self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "cloud-sync-preview-check-meta",
                    meta_key,
                    meta,
                    theme.text_muted,
                    cx,
                )
            }),
            listener,
        )
    }

    pub(super) fn render_cloud_sync_forward_details(
        &self,
        details: &[CloudSyncForwardDetail],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut block = self.render_cloud_sync_preview_block(
            self.i18n
                .t("plugin.cloud_sync.preview.forward_details_title"),
            cx,
        );
        let model = cloud_sync_forward_detail_rows(details);
        for row in model.rows {
            block = block.child(self.render_cloud_sync_list_item(row.title, Some(row.meta), cx));
        }
        if model.overflow_count > 0 {
            block = block.child(self.render_cloud_sync_list_more(model.overflow_count));
        }
        block.into_any_element()
    }

    pub(super) fn render_cloud_sync_record_group(
        &self,
        action: &'static str,
        records: &[CloudSyncPreviewRecord],
        selection: &CloudSyncPreviewSelection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = cloud_sync_preview_record_group_model(action, records, selection);
        let mut block = self.render_cloud_sync_preview_block(self.i18n.t(model.title_key), cx);
        for row in model.rows {
            match row {
                CloudSyncPreviewRecordRow::Connection {
                    record,
                    checked,
                    disabled,
                } => {
                    let name = record.name.clone();
                    let meta = Some(self.format_cloud_sync_preview_record(&record));
                    block = block.child(self.render_cloud_sync_check_row(
                        record.name.clone(),
                        meta,
                        checked,
                        disabled,
                        cx.listener(
                            move |this: &mut WorkspaceApp,
                                  _event,
                                  _window,
                                  cx: &mut Context<WorkspaceApp>| {
                                if let Some(selection) =
                                    this.cloud_sync.view.preview_selection.as_mut()
                                {
                                    if !selection.selected_connection_names.remove(&name) {
                                        selection.selected_connection_names.insert(name.clone());
                                    }
                                }
                                cx.stop_propagation();
                                cx.notify();
                            },
                        ),
                        cx,
                    ));
                }
                CloudSyncPreviewRecordRow::Item { record } => {
                    let meta = Some(self.format_cloud_sync_preview_record(&record));
                    block = block.child(self.render_cloud_sync_list_item(record.name, meta, cx));
                }
            }
        }
        if model.overflow_count > 0 {
            block = block.child(self.render_cloud_sync_list_more(model.overflow_count));
        }
        block.into_any_element()
    }

    pub(super) fn render_cloud_sync_preview_block(
        &self,
        title: String,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        cloud_sync_preview_block(
            &self.tokens,
            self.render_selectable_text_scoped(
                "cloud-sync-preview-block-title",
                title.clone(),
                title,
                theme.text_heading,
                cx,
            ),
        )
    }

    pub(super) fn render_cloud_sync_list_item(
        &self,
        title: String,
        meta: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mono = cloud_sync_value_prefers_mono(&title);
        cloud_sync_list_item(
            &self.tokens,
            self.render_selectable_text_scoped(
                "cloud-sync-list-title",
                title.clone(),
                title,
                theme.text,
                cx,
            ),
            meta.map(|meta| {
                self.render_selectable_text_scoped(
                    "cloud-sync-list-meta",
                    meta.clone(),
                    meta,
                    theme.text_muted,
                    cx,
                )
            }),
            mono,
            Some(settings_mono_font_family(self.settings_store.settings())),
        )
    }

    pub(super) fn render_cloud_sync_list_more(&self, count: usize) -> AnyElement {
        cloud_sync_list_more(
            &self.tokens,
            self.i18n_replace(
                "plugin.cloud_sync.preview.more_items",
                &[("count", count.to_string())],
            ),
        )
    }

    pub(super) fn format_cloud_sync_preview_record(
        &self,
        record: &CloudSyncPreviewRecord,
    ) -> String {
        let Some(key) = cloud_sync_preview_record_label_key(record.action.as_str()) else {
            return record.reason_code.clone();
        };
        self.i18n_replace(
            key,
            &[
                ("name", record.name.clone()),
                (
                    "target",
                    record
                        .target_name
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                ),
            ],
        )
    }
}
