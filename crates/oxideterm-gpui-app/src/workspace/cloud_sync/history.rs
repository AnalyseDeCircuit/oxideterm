// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

impl WorkspaceApp {
    pub(super) fn cloud_sync_page_renderer(&self, cx: &mut Context<Self>) -> CloudSyncPageRenderer {
        let focused_input = self.focused_settings_input.filter(|input| {
            cloud_sync_form_input_value_ref(&self.cloud_sync.read(cx).view.form, *input).is_some()
        });
        let input = focused_input
            .map(|input| {
                let target = WorkspaceImeTarget::Settings(input);
                let mask = |value: &str| "•".repeat(value.encode_utf16().count());
                CloudSyncInputRenderProjection {
                    focused_input: Some(input),
                    active_value: Some(if input.is_secret() {
                        // The page receives only UTF-16-length geometry for an active secret.
                        mask(&self.settings_input_draft)
                    } else {
                        self.settings_input_draft.clone()
                    }),
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx).map(|text| {
                        if input.is_secret() {
                            mask(text)
                        } else {
                            text.to_owned()
                        }
                    }),
                    caret_visible: self.input_caret.visible(),
                }
            })
            .unwrap_or_default();
        let local_snapshot = {
            let cloud_sync = self.cloud_sync.read(cx);
            self.cloud_sync_local_snapshot(cloud_sync.controller.store.state(), cx)
                .map(Arc::new)
                .map_err(|error| Arc::<str>::from(error))
        };
        let local_field_diff = Arc::new(self.cloud_sync_local_field_diff_snapshot(cx));
        let upload_diff_items = match &local_snapshot {
            Ok(local_snapshot) => {
                let cloud_sync = self.cloud_sync.read(cx);
                Arc::new(self.cloud_sync_upload_diff_items_cached(
                    local_snapshot,
                    cloud_sync.controller.store.state(),
                    cx,
                ))
            }
            Err(_) => Arc::new(Vec::new()),
        };
        let active_tab = self.cloud_sync.read(cx).view.active_tab;
        let tab_transition_active = self.segmented_control_user_transition_active(
            selection_motion::CLOUD_SYNC_SWITCHER_ID,
            cloud_sync_tab_index(active_tab),
        );
        let upload_sensitive_summary = self
            .cloud_sync
            .read(cx)
            .view
            .upload_selection
            .as_ref()
            .and_then(|selection| self.cloud_sync_upload_sensitive_summary(selection, cx));
        // I18n clones share the catalog Arc; no locale table or secret draft is copied.
        CloudSyncPageRenderer {
            cloud_sync: self.cloud_sync.clone(),
            render: Arc::new(CloudSyncListRenderProjection {
                tokens: self.tokens,
                i18n: self.i18n.clone(),
                selectable_text: self.selectable_text_render_state(cx),
                has_background: self.cloud_sync_has_background(),
                input,
                local_snapshot,
                local_field_diff,
                upload_diff_items,
                mono_font_family: settings_mono_font_family(self.settings_store.settings()),
                tab_transition_active,
                upload_sensitive_summary,
            }),
        }
    }
}

impl CloudSyncPageRenderer {
    pub(super) fn render_cloud_sync_recent_history(&self, cx: &mut App) -> AnyElement {
        let recent = self
            .cloud_sync
            .read(cx)
            .controller
            .store
            .state()
            .sync_history
            .iter()
            .rev()
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        let theme = self.tokens.ui;
        let title =
            self.render_cloud_sync_section_title("plugin.cloud_sync.overview.recent_history", cx);
        let view_all = self.render.inline_button(
            "plugin.cloud_sync.overview.view_all_history",
            self.intent_listener(CloudSyncUiIntent::SelectTab {
                tab: CloudSyncTab::History,
            }),
        );
        let body = if recent.is_empty() {
            cloud_sync_history_empty(
                &self.tokens,
                self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "cloud-sync-recent-history",
                    "empty",
                    self.i18n.t("plugin.cloud_sync.history_empty"),
                    theme.text_muted,
                    cx,
                ),
            )
        } else {
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
                    .when(!recent.is_empty(), |header| header.child(view_all)),
            )
            .child(body)
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_history_entry(
        &self,
        entry: &CloudSyncHistoryEntry,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let summary = self.i18n_replace(
            "plugin.cloud_sync.history.summary_line",
            &[
                ("connections", entry.summary.connections.to_string()),
                ("forwards", entry.summary.forwards.to_string()),
                ("quickCommands", entry.summary.quick_commands.to_string()),
                ("serialProfiles", entry.summary.serial_profiles.to_string()),
                ("telnetProfiles", entry.summary.telnet_profiles.to_string()),
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

    pub(super) fn render_cloud_sync_notes(&self, cx: &mut App) -> AnyElement {
        let theme = self.tokens.ui;
        let sections = self
            .local_snapshot
            .as_ref()
            .ok()
            .map(Arc::as_ref)
            .map(|snapshot| {
                snapshot
                    .scope
                    .app_settings_sections
                    .iter()
                    .map(|section| {
                        cloud_sync_app_settings_section_label_key(section)
                            .map(|key| self.i18n.t(key))
                            .unwrap_or_else(|| section.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join(" · ")
            })
            .filter(|sections| !sections.trim().is_empty())
            .unwrap_or_else(|| "—".to_string());
        self.cloud_sync_plugin_card(self.cloud_sync_has_background())
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
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

    pub(super) fn render_cloud_sync_config_preflight_card(&self, cx: &mut App) -> AnyElement {
        if self.upload_diff_items.is_empty() {
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
                &self.upload_diff_items,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_health_card(&self, cx: &mut App) -> AnyElement {
        let health_items = {
            let cloud_sync = self.cloud_sync.read(cx);
            cloud_sync_health_items(&cloud_sync.view.form, cloud_sync.controller.store.state())
        };
        let rows = health_items
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
        cx: &mut App,
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
                                SelectableTextRole::NonSelectable,
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

    pub(super) fn render_cloud_sync_coverage_card(&self, cx: &mut App) -> AnyElement {
        let coverage_items = {
            let cloud_sync = self.cloud_sync.read(cx);
            cloud_sync_coverage_model(&cloud_sync.controller.store.state().sync_scope)
        };
        let rows = coverage_items
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
        cx: &mut App,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        let status_key = self.cloud_sync_coverage_status_key(status);
        cloud_sync_status_row(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
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
                SelectableTextRole::NonSelectable,
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

impl CloudSyncListRenderProjection {
    pub(super) fn replace(&self, key: &str, replacements: &[(&str, String)]) -> String {
        let mut text = self.i18n.t(key);
        for (name, value) in replacements {
            text = text.replace(&format!("{{{{{name}}}}}"), value);
        }
        text
    }

    pub(super) fn format_error(&self, error: &str) -> String {
        match cloud_sync_error_message_spec(error) {
            CloudSyncErrorMessageSpec::Raw(message) => message,
            CloudSyncErrorMessageSpec::Key(key) => self.i18n.t(key),
            CloudSyncErrorMessageSpec::KeyWithDetail { key, detail } => {
                format!("{} ({detail})", self.i18n.t(key))
            }
            CloudSyncErrorMessageSpec::SnapshotTooLarge { limit } => self.replace(
                "plugin.cloud_sync.errors.snapshot_too_large",
                &[("limit", limit.unwrap_or_else(|| "—".to_string()))],
            ),
        }
    }

    pub(super) fn selectable_text(
        &self,
        role: SelectableTextRole,
        scope: &str,
        key: impl std::hash::Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        self.selectable_text.render_display_text_with_role_in_group(
            role,
            crate::workspace::selectable_text::selectable_document_group_id(),
            scope,
            key,
            0,
            text,
            color,
            cx,
        )
    }

    pub(super) fn section_title(&self, key: &str, cx: &mut App) -> AnyElement {
        cloud_sync_section_title(
            &self.tokens,
            self.selectable_text(
                SelectableTextRole::NonSelectable,
                "cloud-sync-section-title",
                key,
                self.i18n.t(key).to_uppercase(),
                self.tokens.ui.text_heading,
                cx,
            ),
        )
    }

    pub(super) fn plugin_card(&self) -> Div {
        semantic_surface(
            &self.tokens,
            SurfaceOptions::new(SurfaceKind::Inspector)
                .padding(SurfacePadding::Spacious)
                .has_background_image(self.has_background),
        )
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(16.0))
    }

    pub(super) fn inline_button(
        &self,
        label_key: &str,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let options = cloud_sync_inline_button_options(&self.tokens);
        let actionable = !(options.button.disabled || options.loading);
        // Keep disabled activation semantics identical to the shared workspace wrapper.
        oxideterm_gpui_ui::button::toolbar_button(
            &self.tokens,
            self.i18n.t(label_key),
            None,
            options,
        )
        .when(actionable, |button| {
            button.on_mouse_down(MouseButton::Left, listener)
        })
        .when(!actionable, |button| {
            button.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
        })
        .into_any_element()
    }
}

impl CloudSyncWorkspaceEntity {
    fn rollback_backup_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_ROLLBACK_BACKUP_LIST_OVERSCAN,
        )
    }

    fn history_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_HISTORY_LIST_OVERSCAN,
        )
    }

    fn sync_rollback_rows(&mut self) {
        let signatures = self
            .controller
            .store
            .state()
            .rollback_backups
            .iter()
            .map(cloud_sync_rollback_backup_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.view.rollback_backup_list_state,
            &mut self.view.rollback_backup_list_cache.borrow_mut(),
            "cloud-sync-rollback-backups",
            &signatures,
            Self::rollback_backup_list_spec(),
        );
    }

    fn sync_history_rows(&mut self) {
        let signatures = self
            .controller
            .store
            .state()
            .sync_history
            .iter()
            .map(cloud_sync_history_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.view.history_list_state,
            &mut self.view.history_list_cache.borrow_mut(),
            "cloud-sync-history",
            &signatures,
            Self::history_list_spec(),
        );
    }

    fn render_rollback_backup_row(
        &self,
        index: usize,
        busy: bool,
        show_management: bool,
        render: &CloudSyncListRenderProjection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(backup) = self.controller.store.state().rollback_backups.get(index) else {
            return div().into_any_element();
        };
        // The virtual callback only derives strings for this visible row.
        let signature = cloud_sync_rollback_backup_signature(backup);
        let id = backup.id.clone();
        let created_at = backup.created_at.clone();
        let summary = match cloud_sync_rollback_backup_summary_spec(backup) {
            CloudSyncRollbackBackupSummarySpec::Metadata {
                connections,
                forwards,
                quick_commands,
                serial_profiles,
                telnet_profiles,
                sensitive_credentials,
                plugin_settings_count,
                size,
            } => render.replace(
                "plugin.cloud_sync.backup.summary_line",
                &[
                    ("connections", connections.to_string()),
                    ("forwards", forwards.to_string()),
                    ("quickCommands", quick_commands.to_string()),
                    ("serialProfiles", serial_profiles.to_string()),
                    ("telnetProfiles", telnet_profiles.to_string()),
                    ("sensitiveCredentials", sensitive_credentials.to_string()),
                    ("pluginSettingsCount", plugin_settings_count.to_string()),
                    ("size", size),
                ],
            ),
            CloudSyncRollbackBackupSummarySpec::SizeOnly(size) => size,
        };
        let mut actions = div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(render.inline_button(
                "plugin.cloud_sync.actions.restore_backup",
                cx.listener(move |_cloud_sync, _event, _window, cx| {
                    if !busy {
                        cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                            CloudSyncUiIntent::RestoreRollbackBackup { signature },
                        ));
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        if show_management {
            actions = actions.child(render.inline_button(
                "plugin.cloud_sync.actions.delete_backup",
                cx.listener(move |_cloud_sync, _event, _window, cx| {
                    if !busy {
                        cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                            CloudSyncUiIntent::DeleteRollbackBackup { signature },
                        ));
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        cloud_sync_rollback_backup_row(
            &render.tokens,
            render.selectable_text(
                SelectableTextRole::PlainDocument,
                "cloud-sync-rollback-backup",
                (id.as_str(), "created-at"),
                created_at,
                render.tokens.ui.text,
                cx,
            ),
            render.selectable_text(
                SelectableTextRole::PlainDocument,
                "cloud-sync-rollback-backup",
                (id.as_str(), "summary"),
                summary,
                render.tokens.ui.text_muted,
                cx,
            ),
            actions.into_any_element(),
        )
    }

    fn render_history_row(
        &self,
        index: usize,
        render: &CloudSyncListRenderProjection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = self.controller.store.state().sync_history.get(index) else {
            return div().into_any_element();
        };
        // History payloads stay in the Entity store; only this visible row is formatted.
        let action = cloud_sync_history_action_label_key(&entry.action)
            .map(|key| render.i18n.t(key))
            .unwrap_or_else(|| entry.action.clone());
        let timestamp = cloud_sync_format_timestamp(&entry.timestamp);
        let summary = render.replace(
            "plugin.cloud_sync.history.summary_line",
            &[
                ("connections", entry.summary.connections.to_string()),
                ("forwards", entry.summary.forwards.to_string()),
                ("quickCommands", entry.summary.quick_commands.to_string()),
                ("serialProfiles", entry.summary.serial_profiles.to_string()),
                ("telnetProfiles", entry.summary.telnet_profiles.to_string()),
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
        let error = entry
            .error
            .as_deref()
            .map(|error| render.format_error(error));
        div()
            .pb(px(8.0))
            .child(cloud_sync_history_entry(
                &render.tokens,
                render.selectable_text(
                    SelectableTextRole::PlainDocument,
                    "cloud-sync-history-action",
                    (entry.id.as_str(), entry.action.as_str()),
                    action,
                    render.tokens.ui.text,
                    cx,
                ),
                render.selectable_text(
                    SelectableTextRole::PlainDocument,
                    "cloud-sync-history-summary",
                    (entry.id.as_str(), timestamp.as_str()),
                    format!("{timestamp} · {summary}"),
                    render.tokens.ui.text_muted,
                    cx,
                ),
                error.as_ref().map(|error| {
                    render.selectable_text(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-history-error",
                        (entry.id.as_str(), error.as_str()),
                        error.clone(),
                        render.tokens.ui.error,
                        cx,
                    )
                }),
            ))
            .into_any_element()
    }

    pub(super) fn render_rollback_backup_list(
        &mut self,
        render: Arc<CloudSyncListRenderProjection>,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_rollback_rows();
        let row_count = self.controller.store.state().rollback_backups.len();
        let state = self.view.rollback_backup_list_state.clone();
        let cloud_sync = cx.entity();
        let list_render = Arc::clone(&render);
        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(render.section_title("plugin.cloud_sync.sections.rollback_backups", cx))
            .when(row_count != 0, |header| {
                header.child(render.inline_button(
                    "plugin.cloud_sync.actions.clear_backups",
                    cx.listener(move |_cloud_sync, _event, _window, cx| {
                        if !busy {
                            cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                                CloudSyncUiIntent::ClearRollbackBackups,
                            ));
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            });
        render
            .plugin_card()
            .child(header)
            .child(
                div()
                    .h(px(
                        row_count as f32 * CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT
                    ))
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .child(tauri_virtual_list(
                        state,
                        Self::rollback_backup_list_spec(),
                        move |index, _window, cx| {
                            cloud_sync.update(cx, |cloud_sync, cx| {
                                cloud_sync.render_rollback_backup_row(
                                    index,
                                    busy,
                                    true,
                                    &list_render,
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_recent_rollback_backups(
        &mut self,
        render: Arc<CloudSyncListRenderProjection>,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_rollback_rows();
        let row_count = self.controller.store.state().rollback_backups.len();
        if row_count == 0 {
            return div().into_any_element();
        }
        let mut card = render
            .plugin_card()
            .child(render.section_title("plugin.cloud_sync.sections.rollback_backups", cx));
        for index in 0..row_count.min(3) {
            card = card.child(self.render_rollback_backup_row(index, busy, false, &render, cx));
        }
        card.into_any_element()
    }

    pub(super) fn render_history_list(
        &mut self,
        render: Arc<CloudSyncListRenderProjection>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_history_rows();
        let busy = self.controller.delivery_rx.is_some();
        let row_count = self.controller.store.state().sync_history.len();
        let body = if row_count == 0 {
            cloud_sync_history_empty(
                &render.tokens,
                render.selectable_text(
                    SelectableTextRole::NonSelectable,
                    "cloud-sync-history",
                    "empty",
                    render.i18n.t("plugin.cloud_sync.history_empty"),
                    render.tokens.ui.text_muted,
                    cx,
                ),
            )
        } else {
            let state = self.view.history_list_state.clone();
            let cloud_sync = cx.entity();
            let list_render = Arc::clone(&render);
            div()
                .h(px(
                    row_count as f32 * CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT
                ))
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                .child(tauri_virtual_list(
                    state,
                    Self::history_list_spec(),
                    move |index, _window, cx| {
                        cloud_sync.update(cx, |cloud_sync, cx| {
                            cloud_sync.render_history_row(index, &list_render, cx)
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
            .child(render.selectable_text(
                SelectableTextRole::NonSelectable,
                "cloud-sync-history",
                "title",
                render.i18n.t("plugin.cloud_sync.sections.sync_history"),
                render.tokens.ui.text_heading,
                cx,
            ))
            .when(row_count != 0, |header| {
                header.child(render.inline_button(
                    "plugin.cloud_sync.actions.clear_history",
                    cx.listener(move |_cloud_sync, _event, _window, cx| {
                        if !busy {
                            cx.emit(CloudSyncWorkspaceEvent::UiIntent(
                                CloudSyncUiIntent::ClearHistory,
                            ));
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ))
            });
        render
            .plugin_card()
            .child(header)
            .child(body)
            .into_any_element()
    }
}
