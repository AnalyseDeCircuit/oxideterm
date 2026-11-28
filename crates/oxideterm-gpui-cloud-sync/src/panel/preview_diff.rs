use super::*;

impl CloudSyncPageRenderer {
    pub(super) fn render_cloud_sync_section_diff_card(
        &self,
        identity: &'static str,
        title_key: &'static str,
        items: &[CloudSyncSectionDiffItem],
        cx: &mut App,
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
        cx: &mut App,
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
        cx: &mut App,
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
        cx: &mut App,
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
        cx: &mut App,
    ) -> AnyElement {
        self.render_cloud_sync_tone_chip(
            local_diff_tone(status),
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
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
        cx: &mut App,
    ) -> AnyElement {
        self.render_cloud_sync_tone_chip(
            remote_diff_tone(status),
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
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
        cx: &mut App,
    ) -> AnyElement {
        self.render_cloud_sync_tone_chip(
            health_tone(status),
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
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
        selection: &CloudSyncPreviewSelection,
        cx: &mut App,
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
            .map(|item| self.render_cloud_sync_apply_field_diff_item(item, selection, cx));
        cloud_sync_status_list(&self.tokens, title, rows)
    }

    pub(super) fn render_cloud_sync_apply_field_diff_item(
        &self,
        item: &CloudSyncFieldDiffItem,
        selection: &CloudSyncPreviewSelection,
        cx: &mut App,
    ) -> AnyElement {
        let Some(action) = self.cloud_sync_apply_action_for_field_item(item) else {
            return self.render_cloud_sync_field_diff_item(item, cx);
        };
        let checked = self.cloud_sync_apply_field_item_checked(selection, &action);
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
            self.preview_selection_listener(action),
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
            "plugin.cloud_sync.settings.sync_container_profiles" => Some(
                CloudSyncPreviewSelectionAction::ToggleContainerProfileItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_ble_profiles" => Some(
                CloudSyncPreviewSelectionAction::ToggleBleProfileItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_telnet_profiles" => Some(
                CloudSyncPreviewSelectionAction::ToggleTelnetProfileItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_mosh_profiles" => Some(
                CloudSyncPreviewSelectionAction::ToggleMoshProfileItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_remote_desktop_profiles" => Some(
                CloudSyncPreviewSelectionAction::ToggleRemoteDesktopProfileItem(
                    item.item_key.clone(),
                ),
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
            CloudSyncPreviewSelectionAction::ToggleContainerProfileItem(id) => {
                selection.selected_container_profile_ids.contains(id)
            }
            CloudSyncPreviewSelectionAction::ToggleBleProfileItem(id) => {
                selection.selected_ble_profile_ids.contains(id)
            }
            CloudSyncPreviewSelectionAction::ToggleTelnetProfileItem(id) => {
                selection.selected_telnet_profile_ids.contains(id)
            }
            CloudSyncPreviewSelectionAction::ToggleMoshProfileItem(id) => {
                selection.selected_mosh_profile_ids.contains(id)
            }
            CloudSyncPreviewSelectionAction::ToggleRemoteDesktopProfileItem(id) => {
                selection.selected_remote_desktop_profile_ids.contains(id)
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
        selection: Option<&CloudSyncUploadSelection>,
        cx: &mut App,
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
            .map(|item| self.render_cloud_sync_upload_field_diff_item(item, selection, cx));
        cloud_sync_status_list(&self.tokens, title, rows)
    }

    pub(super) fn render_cloud_sync_upload_field_diff_item(
        &self,
        item: &CloudSyncFieldDiffItem,
        selection: Option<&CloudSyncUploadSelection>,
        cx: &mut App,
    ) -> AnyElement {
        let Some(action) = self.cloud_sync_upload_action_for_field_item(item) else {
            return self.render_cloud_sync_field_diff_item(item, cx);
        };
        let checked = selection.is_none_or(|selection| selection.is_item_checked(&action));
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
            self.upload_selection_listener(action),
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
            "plugin.cloud_sync.settings.sync_container_profiles" => Some(
                CloudSyncUploadSelectionAction::ToggleContainerProfileItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_ble_profiles" => Some(
                CloudSyncUploadSelectionAction::ToggleBleProfileItem(item.item_key.clone()),
            ),
            "plugin.cloud_sync.settings.sync_telnet_profiles" => Some(
                CloudSyncUploadSelectionAction::ToggleTelnetProfileItem(item.item_key.clone()),
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
        cx: &mut App,
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
                SelectableTextRole::NonSelectable,
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
}
