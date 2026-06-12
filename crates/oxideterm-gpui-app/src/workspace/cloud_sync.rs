use std::sync::mpsc::{self, TryRecvError};

use crate::workspace::ime::WorkspaceImeTarget;
use chrono::Utc;
use gpui::prelude::*;
use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, CloudSyncStatus, ConflictStrategy,
    OXIDE_APP_SETTINGS_SECTION_IDS, RawSyncScope, normalize_sync_scope,
    operation::{
        ApplyLegacyPreviewOutcome, ApplyStructuredPreviewOutcome, LegacyPreview, UploadOptions,
        UploadOutcome,
    },
    progress::CloudSyncProgress,
    secrets::{CloudSyncKeychainSecretProvider, backend_uses_auth_mode},
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::{
        CloudSyncHistoryEntry, CloudSyncHistorySummary, CloudSyncPersistedState,
        CloudSyncRollbackBackup,
    },
};
use oxideterm_gpui_cloud_sync::{
    CLOUD_SYNC_FIELD_REDACTED_VALUE, CLOUD_SYNC_GUIDE_STEP_KEYS, CloudSyncApplyOutcome,
    CloudSyncApplyUiOutcome, CloudSyncConfigRow, CloudSyncConfirmDescription,
    CloudSyncCoverageDetail, CloudSyncCoverageStatus, CloudSyncDiffLabel,
    CloudSyncErrorMessageSpec, CloudSyncFieldDiffItem, CloudSyncFieldDiffStatus,
    CloudSyncForwardDetail, CloudSyncGuideExampleElements, CloudSyncHealthStatus,
    CloudSyncLocalDiffStatus, CloudSyncLocalFieldDiffSnapshot, CloudSyncPreviewBodySection,
    CloudSyncPreviewFactValue, CloudSyncPreviewImpactItem, CloudSyncPreviewRecord,
    CloudSyncPreviewRecordRow, CloudSyncPreviewSelectionAction, CloudSyncPreviewSelectionLabel,
    CloudSyncPreviewSource, CloudSyncPreviewSummary, CloudSyncRemoteDiffStatus,
    CloudSyncRollbackBackupSummarySpec, CloudSyncSection, CloudSyncSectionDiffItem,
    CloudSyncSelectAction, CloudSyncSelectKeyEffect, CloudSyncSelectKeyState,
    CloudSyncSelectOption, CloudSyncUploadSelectionAction,
    close_cloud_sync_select_on_container_scroll, cloud_sync_action_grid, cloud_sync_action_panel,
    cloud_sync_app_settings_section_label_key, cloud_sync_apply_diff_items,
    cloud_sync_apply_field_diff_items, cloud_sync_backend_label_key, cloud_sync_card,
    cloud_sync_check_row, cloud_sync_config_rows, cloud_sync_confirm_copy_spec,
    cloud_sync_conflict_info, cloud_sync_coverage_model, cloud_sync_error_message_spec,
    cloud_sync_error_view, cloud_sync_fact_card, cloud_sync_fact_grid, cloud_sync_field_row,
    cloud_sync_focusable_selects, cloud_sync_form_grid, cloud_sync_format_timestamp,
    cloud_sync_forward_detail_rows, cloud_sync_guide_card, cloud_sync_guide_spec,
    cloud_sync_header, cloud_sync_health_items, cloud_sync_history_action_label_key,
    cloud_sync_history_card, cloud_sync_history_empty, cloud_sync_history_entry,
    cloud_sync_history_signature, cloud_sync_inline_button_options, cloud_sync_legacy_apply_plan,
    cloud_sync_list_item, cloud_sync_list_more, cloud_sync_main_action_grid, cloud_sync_meta_line,
    cloud_sync_notes_card, cloud_sync_platform_label, cloud_sync_preview_block,
    cloud_sync_preview_card, cloud_sync_preview_card_model, cloud_sync_preview_record_group_model,
    cloud_sync_preview_record_label_key, cloud_sync_preview_summary,
    cloud_sync_progress_stage_label_key, cloud_sync_progress_unit, cloud_sync_progress_view,
    cloud_sync_rollback_backup_row, cloud_sync_rollback_backup_signature,
    cloud_sync_rollback_backup_summary_spec, cloud_sync_secret_row, cloud_sync_section_item,
    cloud_sync_section_signature, cloud_sync_section_title, cloud_sync_sections,
    cloud_sync_select_field, cloud_sync_select_label_key, cloud_sync_select_menu,
    cloud_sync_select_option, cloud_sync_select_options as cloud_sync_select_option_specs,
    cloud_sync_select_trigger,
    cloud_sync_selected_option_index as cloud_sync_selected_option_spec_index,
    cloud_sync_settings_from_form, cloud_sync_should_create_rollback_backup,
    cloud_sync_sidebar_empty, cloud_sync_status_card, cloud_sync_status_label_key,
    cloud_sync_status_list, cloud_sync_status_row, cloud_sync_toggle, cloud_sync_toggle_grid,
    cloud_sync_upload_diff_items, cloud_sync_upload_field_diff_items,
    cloud_sync_value_prefers_mono, cloud_sync_version_info_rows, deliver_cloud_sync_apply_preview,
    deliver_cloud_sync_check, deliver_cloud_sync_pull_preview,
    deliver_cloud_sync_restore_backup_preview, deliver_cloud_sync_upload,
    deliver_cloud_sync_upload_preview, finish_cloud_sync_automatic_upload_error_state,
    finish_cloud_sync_check_state, finish_cloud_sync_error_state,
    finish_cloud_sync_pull_preview_state, finish_cloud_sync_upload_state,
    finish_legacy_cloud_sync_apply_state, finish_structured_cloud_sync_apply_state,
    handle_cloud_sync_select_key as reduce_cloud_sync_select_key,
    normalize_cloud_sync_interval_draft, persist_remote_metadata, reset_cloud_sync_secret_drafts,
    store_cloud_sync_touched_secrets,
};
pub(super) use oxideterm_gpui_cloud_sync::{
    CloudSyncConfirm, CloudSyncDelivery, CloudSyncPendingPreview, CloudSyncPreviewSelection,
    CloudSyncSelect, CloudSyncUploadSelection,
};
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::button::ButtonVariant;
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};

use super::quick_commands::QuickCommandImportStrategy;
use super::*;

mod confirm_dialog;

impl WorkspaceApp {
    pub(super) fn open_cloud_sync_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self.tabs.iter().find(|tab| tab.kind == TabKind::CloudSync)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::CloudSync,
                title: self.i18n.t("plugin.cloud_sync.panel_title"),
                title_source: TabTitleSource::I18nKey("plugin.cloud_sync.panel_title"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn render_cloud_sync_sidebar_content(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        cloud_sync_sidebar_empty(
            &self.tokens,
            Self::render_lucide_icon(
                LucideIcon::Cloud,
                self.tokens.metrics.empty_sidebar_icon_size,
                rgb(theme.text_muted),
            ),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-sidebar-empty",
                "title",
                self.i18n.t("plugin.cloud_sync.panel_title"),
                theme.text_muted,
                cx,
            ),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-sidebar-empty",
                "description",
                self.i18n.t("plugin.cloud_sync.native_description"),
                theme.text_muted,
                cx,
            ),
        )
    }

    pub(super) fn render_cloud_sync_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.poll_cloud_sync_delivery(cx);

        let theme = self.tokens.ui;
        self.sync_cloud_sync_section_list_state();
        let state = self.cloud_sync_section_list_state.clone();
        let spec = self.cloud_sync_section_list_spec();
        let workspace = cx.entity();

        div()
            .id("cloud-sync-scroll")
            .size_full()
            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                if this.close_cloud_sync_select_for_scroll() {
                    cx.notify();
                }
            }))
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_cloud_sync_section_item(index, cx)
                    })
                },
            ))
            .into_any_element()
    }

    fn sync_cloud_sync_section_list_state(&mut self) {
        let spec = self.cloud_sync_section_list_spec();
        let signatures = self.cloud_sync_section_signatures();
        sync_tauri_variable_list_state_by_signatures(
            &self.cloud_sync_section_list_state,
            &mut self.cloud_sync_section_list_cache.borrow_mut(),
            "cloud-sync",
            &signatures,
            spec,
        );
    }

    fn cloud_sync_section_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_SECTION_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_SECTION_LIST_OVERSCAN,
        )
    }

    fn cloud_sync_sections(&self) -> Vec<CloudSyncSection> {
        cloud_sync_sections(
            self.cloud_sync_store.state(),
            self.cloud_sync_has_pending_preview(),
        )
    }

    fn cloud_sync_section_signatures(&self) -> Vec<u64> {
        self.cloud_sync_sections()
            .into_iter()
            .map(|section| self.cloud_sync_section_signature(section))
            .collect()
    }

    fn cloud_sync_section_signature(&self, section: CloudSyncSection) -> u64 {
        cloud_sync_section_signature(
            section,
            self.cloud_sync_store.state(),
            &self.cloud_sync_form.backend_type,
            &self.cloud_sync_form.auth_mode,
            &self.cloud_sync_form.default_conflict_strategy,
            self.cloud_sync_rx.is_some(),
            self.cloud_sync_has_pending_preview(),
            self.cloud_sync_preview_selection.is_some(),
            self.cloud_sync_progress.is_some(),
        )
    }

    fn cloud_sync_has_pending_preview(&self) -> bool {
        self.cloud_sync_pending_preview.is_some() || self.cloud_sync_upload_preview.is_some()
    }

    fn render_cloud_sync_section_item(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sections = self.cloud_sync_sections();
        let Some(section) = sections.get(index).copied() else {
            return div().into_any_element();
        };
        let child = self.render_cloud_sync_section(section, cx);
        cloud_sync_section_item(&self.tokens, index, sections.len(), child)
    }

    fn render_cloud_sync_section(
        &mut self,
        section: CloudSyncSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.cloud_sync_store.state().clone();
        let busy = self.cloud_sync_rx.is_some();
        match section {
            CloudSyncSection::Header => self.render_cloud_sync_header(&state, cx),
            CloudSyncSection::Guide => {
                self.render_cloud_sync_guide(&self.cloud_sync_form.backend_type, cx)
            }
            CloudSyncSection::Status => self.render_cloud_sync_status_card(&state, cx),
            CloudSyncSection::Actions => self.render_cloud_sync_actions(&state, busy, cx),
            CloudSyncSection::Preview => {
                if let Some(preview) = self.cloud_sync_upload_preview.as_ref() {
                    self.render_cloud_sync_upload_preview(preview, &state, busy, cx)
                } else {
                    self.cloud_sync_pending_preview
                        .as_ref()
                        .map(|preview| self.render_cloud_sync_preview(preview, &state, busy, cx))
                        .unwrap_or_else(|| div().into_any_element())
                }
            }
            CloudSyncSection::Rollback => self.render_cloud_sync_rollback_backups(&state, busy, cx),
            CloudSyncSection::History => self.render_cloud_sync_history(&state, cx),
            CloudSyncSection::Config => self.render_cloud_sync_config(cx),
            CloudSyncSection::Notes => {
                let local_snapshot = self.cloud_sync_local_snapshot(&state);
                self.render_cloud_sync_notes(local_snapshot.as_ref().ok(), cx)
            }
        }
    }

    fn cloud_sync_local_snapshot(
        &self,
        state: &CloudSyncPersistedState,
    ) -> std::result::Result<CloudSyncLocalSnapshot, String> {
        build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            state.last_synced_structured_state.as_ref(),
            Some(&state.sync_scope),
        )
        .map_err(|error| error.to_string())
    }

    fn cloud_sync_local_field_diff_snapshot(&self) -> CloudSyncLocalFieldDiffSnapshot {
        let scope = normalize_sync_scope(Some(&self.cloud_sync_store.state().sync_scope), &[]);
        let app_settings_sections = if scope.sync_app_settings {
            scope
                .app_settings_sections
                .iter()
                .filter_map(|section_id| {
                    let selected = std::collections::HashSet::from([section_id.clone()]);
                    oxideterm_settings::export_oxide_settings_snapshot_json(
                        self.settings_store.settings(),
                        Some(&selected),
                        scope.include_local_terminal_env_vars,
                    )
                    .ok()
                    .and_then(|json| {
                        oxideterm_connections::oxide_file::preview_oxide_app_settings_sections(
                            &json,
                        )
                        .into_iter()
                        .find(|section| section.id == *section_id)
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let quick_commands =
            oxideterm_quick_commands::export_snapshot_json(self.settings_store.path())
                .ok()
                .and_then(|json| {
                    serde_json::from_str::<oxideterm_quick_commands::QuickCommandsSnapshot>(&json)
                        .ok()
                });
        CloudSyncLocalFieldDiffSnapshot {
            connections: self
                .connection_store
                .export_saved_connections_snapshot()
                .ok(),
            forwards: self
                .forwarding_registry
                .export_saved_forwards_snapshot()
                .ok(),
            quick_commands,
            serial_profiles: self.connection_store.export_serial_profiles_snapshot().ok(),
            app_settings_sections,
        }
    }

    fn render_cloud_sync_status_card(
        &self,
        state: &CloudSyncPersistedState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = state.settings.clone();
        let local_snapshot = self.cloud_sync_local_snapshot(state);
        let backend_label = self
            .i18n
            .t(cloud_sync_backend_label_key(&settings.backend_type));

        let facts = cloud_sync_fact_grid([
            self.render_cloud_sync_fact("plugin.cloud_sync.fields.backend", backend_label, cx),
            self.render_cloud_sync_fact(
                "plugin.cloud_sync.fields.namespace",
                settings.namespace,
                cx,
            ),
            self.render_cloud_sync_fact(
                "plugin.cloud_sync.fields.local_dirty",
                local_snapshot
                    .as_ref()
                    .map(|snapshot| {
                        if snapshot.dirty.has_dirty {
                            self.i18n.t("plugin.cloud_sync.common.yes")
                        } else {
                            self.i18n.t("plugin.cloud_sync.common.no")
                        }
                    })
                    .unwrap_or_else(|_| self.i18n.t("plugin.cloud_sync.common.error")),
                cx,
            ),
            self.render_cloud_sync_fact(
                "plugin.cloud_sync.fields.last_sync",
                state
                    .last_sync_at
                    .as_deref()
                    .map(cloud_sync_format_timestamp)
                    .unwrap_or_else(|| "—".to_string()),
                cx,
            ),
        ]);
        cloud_sync_status_card(
            &self.tokens,
            self.cloud_sync_progress
                .as_ref()
                .map(|progress| self.render_cloud_sync_progress(progress, cx)),
            state
                .last_error
                .as_ref()
                .map(|error| self.render_cloud_sync_error(error)),
            facts,
            self.render_cloud_sync_meta(state, local_snapshot.as_ref().ok(), cx),
        )
    }

    fn render_cloud_sync_actions(
        &self,
        state: &CloudSyncPersistedState,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let has_rollback_backup = !state.rollback_backups.is_empty();
        cloud_sync_action_panel(
            &self.tokens,
            cloud_sync_main_action_grid([
                self.render_cloud_sync_action_button(
                    "plugin.cloud_sync.actions.upload_now",
                    ButtonVariant::Default,
                    busy,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.start_cloud_sync_upload_preview(cx);
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                        },
                    ),
                ),
                self.render_cloud_sync_action_button(
                    "plugin.cloud_sync.actions.check_remote",
                    ButtonVariant::Outline,
                    busy,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.start_cloud_sync_check(cx);
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                        },
                    ),
                ),
                self.render_cloud_sync_action_button(
                    "plugin.cloud_sync.actions.pull_preview",
                    ButtonVariant::Outline,
                    busy,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.start_cloud_sync_pull_preview(cx);
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                        },
                    ),
                ),
                self.render_cloud_sync_action_button(
                    "plugin.cloud_sync.actions.restore_backup",
                    ButtonVariant::Outline,
                    busy || !has_rollback_backup,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.open_cloud_sync_restore_confirm(None);
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                ),
                self.render_cloud_sync_action_button(
                    "plugin.cloud_sync.actions.save_settings",
                    ButtonVariant::Outline,
                    busy,
                    cx.listener(
                        |this: &mut WorkspaceApp,
                         _event,
                         _window,
                         cx: &mut Context<WorkspaceApp>| {
                            this.save_cloud_sync_configuration(cx);
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                ),
            ]),
        )
    }

    fn render_cloud_sync_header(
        &self,
        state: &CloudSyncPersistedState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        cloud_sync_header(
            &self.tokens,
            Self::render_lucide_icon(LucideIcon::Cloud, 16.0, rgb(theme.accent)),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-panel",
                "title",
                self.i18n.t("plugin.cloud_sync.panel_title"),
                theme.text,
                cx,
            ),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-panel",
                "subtitle",
                self.i18n.t("plugin.cloud_sync.native_description"),
                theme.text_muted,
                cx,
            ),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-panel",
                "status",
                self.i18n
                    .t(cloud_sync_status_label_key(state.status.clone())),
                theme.accent,
                cx,
            ),
        )
    }

    fn render_cloud_sync_guide(
        &self,
        backend_type: &BackendType,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let backend_key = format!("{backend_type:?}");
        let guide = cloud_sync_guide_spec(backend_type);
        let examples = guide
            .examples
            .into_iter()
            .map(|example| {
                let label = self.i18n.t(example.label_key);
                let value = self.i18n.t(example.value_key);
                CloudSyncGuideExampleElements {
                    label: self.render_selectable_text_scoped(
                        "cloud-sync-guide-example-label",
                        (&label, &value),
                        format!("{label}:"),
                        theme.text_muted,
                        cx,
                    ),
                    value: self.render_selectable_text_scoped(
                        "cloud-sync-guide-example-value",
                        (&label, &value),
                        value.clone(),
                        theme.accent,
                        cx,
                    ),
                }
            })
            .collect::<Vec<_>>();
        cloud_sync_guide_card(
            &self.tokens,
            self.render_cloud_sync_section_title("plugin.cloud_sync.sections.quick_start", cx),
            self.render_selectable_text_scoped(
                "cloud-sync-guide-title",
                &backend_key,
                self.i18n.t(guide.title_key),
                theme.text_heading,
                cx,
            ),
            self.render_selectable_text_scoped(
                "cloud-sync-guide-description",
                &backend_key,
                self.i18n.t(guide.description_key),
                theme.text_muted,
                cx,
            ),
            self.render_cloud_sync_guide_steps(cx),
            Some(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-guide",
                "example-title",
                self.i18n.t("plugin.cloud_sync.guide.example_title"),
                theme.text_heading,
                cx,
            )),
            examples,
            guide.warning_key.map(|warning_key| {
                self.render_selectable_text_scoped(
                    "cloud-sync-guide-warning",
                    &backend_key,
                    self.i18n.t(warning_key),
                    theme.accent,
                    cx,
                )
            }),
            settings_mono_font_family(self.settings_store.settings()),
        )
    }

    fn render_cloud_sync_guide_steps(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .pl(px(20.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .text_color(rgb(theme.text_muted));
        for (index, key) in CLOUD_SYNC_GUIDE_STEP_KEYS.iter().copied().enumerate() {
            list = list.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(self.render_selectable_text_scoped(
                        "cloud-sync-guide-step-index",
                        key,
                        format!("{}.", index + 1),
                        theme.text_muted,
                        cx,
                    ))
                    .child(self.render_selectable_text_scoped(
                        "cloud-sync-guide-step",
                        key,
                        self.i18n.t(key),
                        theme.text_muted,
                        cx,
                    )),
            );
        }
        list.into_any_element()
    }

    fn render_cloud_sync_section_title(&self, key: &str, cx: &mut Context<Self>) -> AnyElement {
        cloud_sync_section_title(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-section-title",
                key,
                self.i18n.t(key).to_uppercase(),
                self.tokens.ui.text_heading,
                cx,
            ),
        )
    }

    fn render_cloud_sync_action_button(
        &self,
        label_key: &str,
        variant: ButtonVariant,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            oxideterm_gpui_cloud_sync::cloud_sync_button_options(variant, disabled),
            listener,
        )
        .into_any_element()
    }

    fn render_cloud_sync_progress(
        &self,
        progress: &CloudSyncProgress,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let ratio = if progress.total <= 0.0 {
            0.0
        } else {
            (progress.current as f32 / progress.total as f32).clamp(0.0, 1.0)
        };
        cloud_sync_progress_view(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-progress",
                "stage",
                self.i18n
                    .t(cloud_sync_progress_stage_label_key(progress.stage)),
                theme.text,
                cx,
            ),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-progress",
                "count",
                format!(
                    "{}/{}",
                    cloud_sync_progress_unit(progress.current),
                    cloud_sync_progress_unit(progress.total)
                ),
                theme.text,
                cx,
            ),
            ratio,
        )
    }

    fn render_cloud_sync_error(&self, error: &str) -> AnyElement {
        cloud_sync_error_view(&self.tokens, self.format_cloud_sync_error(error))
    }

    fn format_cloud_sync_error(&self, error: &str) -> String {
        match cloud_sync_error_message_spec(error) {
            CloudSyncErrorMessageSpec::Raw(message) => message,
            CloudSyncErrorMessageSpec::Key(key) => self.i18n.t(key),
            CloudSyncErrorMessageSpec::SnapshotTooLarge { limit } => self.i18n_replace(
                "plugin.cloud_sync.errors.snapshot_too_large",
                &[("limit", limit.unwrap_or_else(|| "—".to_string()))],
            ),
        }
    }

    fn render_cloud_sync_meta(
        &self,
        state: &CloudSyncPersistedState,
        local_snapshot: Option<&CloudSyncLocalSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let counts = local_snapshot.map(|snapshot| {
            format!(
                "{} / {}",
                snapshot.connections_record_count, snapshot.forwards_record_count
            )
        });
        let version_rows = cloud_sync_version_info_rows(state, counts);
        let version_title = self.render_selectable_text_scoped(
            "cloud-sync-version-info",
            "title",
            self.i18n.t("plugin.cloud_sync.sections.version_info"),
            self.tokens.ui.text_heading,
            cx,
        );
        let version_block = cloud_sync_status_list(
            &self.tokens,
            version_title,
            version_rows
                .into_iter()
                .map(|row| self.render_cloud_sync_meta_line(row.label_key, row.value, cx)),
        );
        let mut block = div().flex().flex_col().gap(px(8.0)).child(version_block);
        if let Some(conflict) = cloud_sync_conflict_info(state) {
            let conflict_title = self.render_selectable_text_scoped(
                "cloud-sync-conflict-info",
                "title",
                self.i18n.t("plugin.cloud_sync.conflict.details_title"),
                self.tokens.ui.text_heading,
                cx,
            );
            let mut rows = conflict
                .rows
                .into_iter()
                .map(|row| self.render_cloud_sync_meta_line(row.label_key, row.value, cx))
                .collect::<Vec<_>>();
            rows.insert(
                0,
                cloud_sync_meta_line(self.render_selectable_text_scoped(
                    "cloud-sync-conflict-info",
                    "plain-summary",
                    self.cloud_sync_conflict_plain_summary(state),
                    self.tokens.ui.text,
                    cx,
                )),
            );
            rows.push(cloud_sync_meta_line(self.render_selectable_text_scoped(
                "cloud-sync-conflict-info",
                "recommendation",
                self.i18n.t(conflict.recommendation_key),
                self.tokens.ui.accent,
                cx,
            )));
            block = block.child(cloud_sync_status_list(&self.tokens, conflict_title, rows));
        }
        block.into_any_element()
    }

    fn cloud_sync_conflict_plain_summary(&self, state: &CloudSyncPersistedState) -> String {
        let remote_device = state
            .conflict_details
            .as_ref()
            .and_then(|details| details.device_id.clone())
            .or_else(|| state.remote_device_id.clone())
            .unwrap_or_else(|| "—".to_string());
        let remote_time = state
            .conflict_details
            .as_ref()
            .and_then(|details| details.updated_at.clone())
            .or_else(|| state.remote_updated_at.clone())
            .map(|value| cloud_sync_format_timestamp(&value))
            .unwrap_or_else(|| "—".to_string());
        let local_time = state
            .last_upload_at
            .as_ref()
            .map(|value| cloud_sync_format_timestamp(value))
            .unwrap_or_else(|| "—".to_string());
        self.i18n_replace(
            "plugin.cloud_sync.conflict.plain_summary",
            &[
                ("remoteDevice", remote_device),
                ("remoteTime", remote_time),
                ("localTime", local_time),
            ],
        )
    }

    fn render_cloud_sync_meta_line(
        &self,
        label_key: &str,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        let text = format!("{label}: {value}");
        cloud_sync_meta_line(self.render_selectable_text(
            crate::workspace::selectable_text::selectable_text_id(
                "cloud-sync-meta",
                (&label, &value),
            ),
            text,
            self.tokens.ui.text_muted,
            cx,
        ))
    }

    fn render_cloud_sync_preview(
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
            self.cloud_sync_preview_selection.as_ref(),
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
                            this.cloud_sync_pending_preview = None;
                            this.cloud_sync_preview_selection = None;
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                ),
            ]),
        )
    }

    fn render_cloud_sync_upload_preview(
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
        if let Some(selection) = self.cloud_sync_upload_selection.as_ref() {
            body.push(self.render_cloud_sync_upload_selection(selection, cx));
        }
        if let Ok(local_snapshot) = self.cloud_sync_local_snapshot(state) {
            let mut preview_state = state.clone();
            if let CloudSyncPendingPreview::Structured(preview) = remote_preview {
                preview_state.remote_exists = true;
                preview_state.remote_section_revisions =
                    Some(preview.manifest.section_revisions.clone());
            }
            if let Some(selection) = self.cloud_sync_upload_selection.as_ref() {
                preview_state.sync_scope = selection.raw_scope(&state.sync_scope);
            }
            let section_diff_items = cloud_sync_upload_diff_items(&local_snapshot, &preview_state);
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
            .cloud_sync_upload_selection
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
                            this.cloud_sync_upload_preview = None;
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
                            this.cloud_sync_upload_preview = None;
                            this.cloud_sync_upload_selection = None;
                            this.clear_cloud_sync_select_focus();
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                ),
            ]),
        )
    }

    fn render_cloud_sync_preview_impact(
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

    fn render_cloud_sync_remote_sensitive_summary(
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

    fn render_cloud_sync_upload_selection(
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

    fn cloud_sync_upload_section_visible(
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

    fn cloud_sync_upload_selection_meta(
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

    fn cloud_sync_upload_sensitive_summary(
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
                .filter(|provider| self.ai_key_store.has_provider_key(&provider.id))
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

    fn render_cloud_sync_section_diff_card(
        &self,
        identity: &'static str,
        title_key: &'static str,
        items: &[CloudSyncSectionDiffItem],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.render_selectable_text_scoped(
            identity,
            "title",
            self.i18n.t(title_key),
            self.tokens.ui.text_heading,
            cx,
        );
        let rows = items
            .iter()
            .map(|item| self.render_cloud_sync_section_diff_row(item, cx));
        cloud_sync_status_list(&self.tokens, title, rows)
    }

    fn render_cloud_sync_section_diff_row(
        &self,
        item: &CloudSyncSectionDiffItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.cloud_sync_diff_label(&item.label);
        let local_status = self
            .i18n
            .t(self.cloud_sync_local_diff_status_key(item.local_status));
        let remote_status = self
            .i18n
            .t(self.cloud_sync_remote_diff_status_key(item.remote_status));
        let detail = item.count.map_or_else(
            || {
                self.i18n_replace(
                    "plugin.cloud_sync.preflight.diff_detail",
                    &[
                        ("local", local_status.clone()),
                        ("remote", remote_status.clone()),
                    ],
                )
            },
            |count| {
                self.i18n_replace(
                    "plugin.cloud_sync.preflight.diff_detail_with_count",
                    &[
                        ("local", local_status.clone()),
                        ("remote", remote_status.clone()),
                        ("count", count.to_string()),
                    ],
                )
            },
        );
        let accent = !matches!(
            item.local_status,
            CloudSyncLocalDiffStatus::Unchanged | CloudSyncLocalDiffStatus::Excluded
        );
        cloud_sync_status_row(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-section-diff-row",
                (label.clone(), "label"),
                label,
                self.tokens.ui.text,
                cx,
            ),
            Some(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-section-diff-row",
                (
                    self.cloud_sync_local_diff_status_key(item.local_status),
                    "detail",
                ),
                detail,
                self.tokens.ui.text_muted,
                cx,
            )),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-section-diff-row",
                (
                    self.cloud_sync_local_diff_status_key(item.local_status),
                    "status",
                ),
                local_status,
                self.tokens.ui.accent,
                cx,
            ),
            accent,
        )
    }

    fn cloud_sync_diff_label(&self, label: &CloudSyncDiffLabel) -> String {
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

    fn cloud_sync_local_diff_status_key(&self, status: CloudSyncLocalDiffStatus) -> &'static str {
        match status {
            CloudSyncLocalDiffStatus::Added => "plugin.cloud_sync.preflight.local_added",
            CloudSyncLocalDiffStatus::Modified => "plugin.cloud_sync.preflight.local_modified",
            CloudSyncLocalDiffStatus::Deleted => "plugin.cloud_sync.preflight.local_deleted",
            CloudSyncLocalDiffStatus::Unchanged => "plugin.cloud_sync.preflight.local_unchanged",
            CloudSyncLocalDiffStatus::Excluded => "plugin.cloud_sync.preflight.local_excluded",
        }
    }

    fn cloud_sync_remote_diff_status_key(&self, status: CloudSyncRemoteDiffStatus) -> &'static str {
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

    fn render_cloud_sync_apply_field_diff_card(
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

    fn render_cloud_sync_apply_field_diff_item(
        &self,
        item: &CloudSyncFieldDiffItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(action) = self.cloud_sync_apply_action_for_field_item(item) else {
            return self.render_cloud_sync_field_diff_item(item, cx);
        };
        let checked = self
            .cloud_sync_preview_selection
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

    fn cloud_sync_apply_action_for_field_item(
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

    fn cloud_sync_apply_field_item_checked(
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

    fn render_cloud_sync_upload_field_diff_card(
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

    fn render_cloud_sync_upload_field_diff_item(
        &self,
        item: &CloudSyncFieldDiffItem,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(action) = self.cloud_sync_upload_action_for_field_item(item) else {
            return self.render_cloud_sync_field_diff_item(item, cx);
        };
        let checked = self
            .cloud_sync_upload_selection
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

    fn cloud_sync_upload_action_for_field_item(
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

    fn render_cloud_sync_field_diff_item(
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
                    fields.child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n_replace(
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
                            )),
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

    fn cloud_sync_field_diff_item_name(&self, item: &CloudSyncFieldDiffItem) -> String {
        if item.section_label_key == "plugin.cloud_sync.settings.sync_app_settings" {
            cloud_sync_app_settings_section_label_key(&item.item_name)
                .map(|key| self.i18n.t(key))
                .unwrap_or_else(|| item.item_name.clone())
        } else {
            item.item_name.clone()
        }
    }

    fn cloud_sync_field_diff_status_key(&self, status: CloudSyncFieldDiffStatus) -> &'static str {
        match status {
            CloudSyncFieldDiffStatus::Added => "plugin.cloud_sync.field_diff.status_added",
            CloudSyncFieldDiffStatus::Modified => "plugin.cloud_sync.field_diff.status_modified",
            CloudSyncFieldDiffStatus::Deleted => "plugin.cloud_sync.field_diff.status_deleted",
        }
    }

    fn cloud_sync_field_diff_value(&self, value: Option<&str>) -> String {
        match value {
            Some(CLOUD_SYNC_FIELD_REDACTED_VALUE) => {
                self.i18n.t("plugin.cloud_sync.field_diff.redacted")
            }
            Some(value) if !value.trim().is_empty() => value.to_string(),
            _ => self.i18n.t("plugin.cloud_sync.field_diff.empty_value"),
        }
    }

    fn render_cloud_sync_preview_selection(
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

    fn apply_cloud_sync_preview_selection_action(
        &mut self,
        action: CloudSyncPreviewSelectionAction,
    ) {
        let all_connection_names = self
            .cloud_sync_pending_preview
            .as_ref()
            .map(cloud_sync_preview_summary)
            .map(|summary| summary.connection_record_names())
            .unwrap_or_default();
        if let Some(selection) = self.cloud_sync_preview_selection.as_mut() {
            selection.apply_action(action, all_connection_names);
        }
    }

    fn apply_cloud_sync_upload_selection_action(&mut self, action: CloudSyncUploadSelectionAction) {
        if let Some(selection) = self.cloud_sync_upload_selection.as_mut() {
            selection.apply_action(action);
        }
    }

    fn cloud_sync_preview_selection_label(&self, label: CloudSyncPreviewSelectionLabel) -> String {
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

    fn cloud_sync_preview_fact_value(&self, value: &CloudSyncPreviewFactValue) -> String {
        match value {
            CloudSyncPreviewFactValue::Count(count) => count.to_string(),
            CloudSyncPreviewFactValue::YesNo(true) => self.i18n.t("plugin.cloud_sync.common.yes"),
            CloudSyncPreviewFactValue::YesNo(false) => self.i18n.t("plugin.cloud_sync.common.no"),
        }
    }

    fn render_cloud_sync_check_row(
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

    fn render_cloud_sync_forward_details(
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

    fn render_cloud_sync_record_group(
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
                                if let Some(selection) = this.cloud_sync_preview_selection.as_mut()
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

    fn render_cloud_sync_preview_block(&self, title: String, cx: &mut Context<Self>) -> gpui::Div {
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

    fn render_cloud_sync_list_item(
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

    fn render_cloud_sync_list_more(&self, count: usize) -> AnyElement {
        cloud_sync_list_more(
            &self.tokens,
            self.i18n_replace(
                "plugin.cloud_sync.preview.more_items",
                &[("count", count.to_string())],
            ),
        )
    }

    fn format_cloud_sync_preview_record(&self, record: &CloudSyncPreviewRecord) -> String {
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

    fn render_cloud_sync_rollback_backups(
        &mut self,
        state: &CloudSyncPersistedState,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_cloud_sync_rollback_backup_list_state(&state.rollback_backups);
        let state_handle = self.cloud_sync_rollback_backup_list_state.clone();
        let spec = self.cloud_sync_rollback_backup_list_spec();
        let workspace = cx.entity();
        let list_height =
            state.rollback_backups.len() as f32 * CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT;
        cloud_sync_card(&self.tokens)
            .child(
                self.render_cloud_sync_section_title(
                    "plugin.cloud_sync.sections.rollback_backups",
                    cx,
                ),
            )
            .child(div().h(px(list_height)).child(tauri_virtual_list(
                state_handle,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_cloud_sync_rollback_backup_item(index, busy, cx)
                    })
                },
            )))
            .into_any_element()
    }

    fn sync_cloud_sync_rollback_backup_list_state(&self, backups: &[CloudSyncRollbackBackup]) {
        let signatures = backups
            .iter()
            .map(cloud_sync_rollback_backup_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.cloud_sync_rollback_backup_list_state,
            &mut self.cloud_sync_rollback_backup_list_cache.borrow_mut(),
            "cloud-sync-rollback-backups",
            &signatures,
            self.cloud_sync_rollback_backup_list_spec(),
        );
    }

    fn cloud_sync_rollback_backup_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_ROLLBACK_BACKUP_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_ROLLBACK_BACKUP_LIST_OVERSCAN,
        )
    }

    fn render_cloud_sync_rollback_backup_item(
        &self,
        index: usize,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.cloud_sync_store.state().clone();
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
        cloud_sync_rollback_backup_row(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-rollback-backup",
                (id.as_str(), "created-at"),
                created_at.clone(),
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
            self.render_cloud_sync_inline_button(
                "plugin.cloud_sync.actions.restore_backup",
                cx.listener(
                    move |this: &mut WorkspaceApp,
                          _event,
                          _window,
                          cx: &mut Context<WorkspaceApp>| {
                        if !busy {
                            this.open_cloud_sync_restore_confirm(Some((
                                id.clone(),
                                created_at.clone(),
                            )));
                        }
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ),
        )
    }

    fn render_cloud_sync_history(
        &mut self,
        state: &CloudSyncPersistedState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
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
            let state_handle = self.cloud_sync_history_list_state.clone();
            let spec = self.cloud_sync_history_list_spec();
            let workspace = cx.entity();
            let list_count = state.sync_history.len().min(10);
            div()
                .h(px(
                    list_count as f32 * CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT
                ))
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
        cloud_sync_history_card(&self.tokens, title, body)
    }

    fn sync_cloud_sync_history_list_state(&self, history: &[CloudSyncHistoryEntry]) {
        let signatures = history
            .iter()
            .take(10)
            .map(cloud_sync_history_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.cloud_sync_history_list_state,
            &mut self.cloud_sync_history_list_cache.borrow_mut(),
            "cloud-sync-history",
            &signatures,
            self.cloud_sync_history_list_spec(),
        );
    }

    fn cloud_sync_history_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_HISTORY_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_HISTORY_LIST_OVERSCAN,
        )
    }

    fn render_cloud_sync_history_list_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.cloud_sync_store.state().clone();
        let Some(entry) = state.sync_history.get(index).cloned() else {
            return div().into_any_element();
        };
        div()
            .pb(px(8.0))
            .child(self.render_cloud_sync_history_entry(&entry, cx))
            .into_any_element()
    }

    fn render_cloud_sync_history_entry(
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

    fn cloud_sync_history_action_label(&self, action: &str) -> String {
        cloud_sync_history_action_label_key(action)
            .map(|key| self.i18n.t(key))
            .unwrap_or_else(|| action.to_string())
    }

    fn render_cloud_sync_notes(
        &self,
        local_snapshot: Option<&CloudSyncLocalSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        cloud_sync_notes_card(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-notes",
                "title",
                self.i18n.t("plugin.cloud_sync.sections.notes"),
                theme.text_heading,
                cx,
            ),
            self.i18n_replace(
                "plugin.cloud_sync.native_scope_summary",
                &[(
                    "sections",
                    local_snapshot
                        .map(|snapshot| snapshot.scope.app_settings_sections.join(", "))
                        .unwrap_or_default(),
                )],
            ),
        )
    }

    fn render_cloud_sync_config(&self, cx: &mut Context<Self>) -> AnyElement {
        let form = &self.cloud_sync_form;
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
                            this.cloud_sync_form.auto_upload_enabled =
                                !this.cloud_sync_form.auto_upload_enabled;
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
        let connection_card = cloud_sync_card(&self.tokens)
            .child(self.render_cloud_sync_section_title(
                "plugin.cloud_sync.sections.connection_settings",
                cx,
            ))
            .child(cloud_sync_form_grid(connection_rows));
        let state = self.cloud_sync_store.state();
        let upload_diff = self
            .cloud_sync_local_snapshot(state)
            .ok()
            .map(|snapshot| cloud_sync_upload_diff_items(&snapshot, state));

        let mut content = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(connection_card)
            .child(self.render_cloud_sync_scope_card(cx))
            .child(self.render_cloud_sync_coverage_card(cx));
        if let Some(upload_diff) = upload_diff.as_ref().filter(|items| !items.is_empty()) {
            content = content.child(
                cloud_sync_card(&self.tokens)
                    .child(self.render_cloud_sync_section_title(
                        "plugin.cloud_sync.sections.sync_preflight",
                        cx,
                    ))
                    .child(self.render_cloud_sync_section_diff_card(
                        "cloud-sync-upload-diff",
                        "plugin.cloud_sync.preflight.upload_diff_title",
                        upload_diff,
                        cx,
                    )),
            );
        }
        content
            .child(self.render_cloud_sync_health_card(cx))
            .into_any_element()
    }

    fn render_cloud_sync_health_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.cloud_sync_store.state();
        let rows = cloud_sync_health_items(&self.cloud_sync_form, state)
            .into_iter()
            .map(|item| {
                self.render_cloud_sync_health_row(item.label_key, item.detail_key, item.status, cx)
            })
            .collect::<Vec<_>>();

        cloud_sync_card(&self.tokens)
            .child(
                self.render_cloud_sync_section_title("plugin.cloud_sync.sections.sync_health", cx),
            )
            .child(cloud_sync_status_list(
                &self.tokens,
                self.render_selectable_text_scoped(
                    "cloud-sync-health-title",
                    "title",
                    self.i18n.t("plugin.cloud_sync.health.title"),
                    self.tokens.ui.text_heading,
                    cx,
                ),
                rows,
            ))
            .into_any_element()
    }

    fn render_cloud_sync_health_row(
        &self,
        label_key: &'static str,
        detail_key: &'static str,
        status: CloudSyncHealthStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let status_key = self.cloud_sync_health_status_key(status);
        cloud_sync_status_row(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-health-row",
                (label_key, "label"),
                self.i18n.t(label_key),
                self.tokens.ui.text,
                cx,
            ),
            Some(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-health-row",
                (label_key, "detail"),
                self.i18n.t(detail_key),
                self.tokens.ui.text_muted,
                cx,
            )),
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-health-row",
                (label_key, "status"),
                self.i18n.t(status_key),
                self.tokens.ui.accent,
                cx,
            ),
            status != CloudSyncHealthStatus::Fail,
        )
    }

    fn cloud_sync_health_status_key(&self, status: CloudSyncHealthStatus) -> &'static str {
        match status {
            CloudSyncHealthStatus::Pass => "plugin.cloud_sync.health.status_pass",
            CloudSyncHealthStatus::Warning => "plugin.cloud_sync.health.status_warning",
            CloudSyncHealthStatus::Fail => "plugin.cloud_sync.health.status_fail",
        }
    }

    fn render_cloud_sync_coverage_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let state = self.cloud_sync_store.state();
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
        cloud_sync_card(&self.tokens)
            .child(
                self.render_cloud_sync_section_title(
                    "plugin.cloud_sync.sections.sync_coverage",
                    cx,
                ),
            )
            .child(cloud_sync_status_list(
                &self.tokens,
                self.render_selectable_text_scoped(
                    "cloud-sync-coverage-title",
                    "title",
                    self.i18n.t("plugin.cloud_sync.coverage.title"),
                    self.tokens.ui.text_heading,
                    cx,
                ),
                rows,
            ))
            .into_any_element()
    }

    fn render_cloud_sync_status_row(
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

    fn cloud_sync_coverage_status_key(&self, status: CloudSyncCoverageStatus) -> &'static str {
        match status {
            CloudSyncCoverageStatus::Included => "plugin.cloud_sync.coverage.status_included",
            CloudSyncCoverageStatus::Excluded => "plugin.cloud_sync.coverage.status_excluded",
            CloudSyncCoverageStatus::Partial => "plugin.cloud_sync.coverage.status_partial",
        }
    }

    fn cloud_sync_coverage_detail(&self, detail: CloudSyncCoverageDetail) -> String {
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

    fn render_cloud_sync_scope_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let raw_scope = &self.cloud_sync_store.state().sync_scope;
        let scope = normalize_sync_scope(Some(raw_scope), &[]);
        let mut toggles = vec![
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_connections",
                scope.sync_connections,
                |scope, next| scope.sync_connections = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_forwards",
                scope.sync_forwards,
                |scope, next| scope.sync_forwards = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_quick_commands",
                scope.sync_quick_commands,
                |scope, next| scope.sync_quick_commands = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_serial_profiles",
                scope.sync_serial_profiles,
                |scope, next| scope.sync_serial_profiles = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_sensitive_credentials",
                scope.sync_sensitive_credentials,
                |scope, next| scope.sync_sensitive_credentials = Some(next),
                cx,
            ),
            self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.sync_app_settings",
                scope.sync_app_settings,
                |scope, next| scope.sync_app_settings = Some(next),
                cx,
            ),
        ];

        if scope.sync_app_settings {
            for section_id in OXIDE_APP_SETTINGS_SECTION_IDS {
                let section_id = (*section_id).to_string();
                let label = cloud_sync_app_settings_section_label_key(&section_id)
                    .map(|key| self.i18n.t(key))
                    .unwrap_or_else(|| section_id.clone());
                toggles.push(self.render_cloud_sync_scope_section_toggle(
                    format!("cloud-sync-scope-section-{section_id}"),
                    label,
                    scope.app_settings_sections.contains(&section_id),
                    section_id,
                    cx,
                ));
            }
            toggles.push(self.render_cloud_sync_scope_bool_toggle(
                "plugin.cloud_sync.settings.include_local_terminal_env_vars",
                scope.include_local_terminal_env_vars,
                |scope, next| scope.include_local_terminal_env_vars = Some(next),
                cx,
            ));
        }

        toggles.push(self.render_cloud_sync_scope_bool_toggle(
            "plugin.cloud_sync.settings.sync_plugin_settings",
            scope.sync_plugin_settings,
            |scope, next| scope.sync_plugin_settings = Some(next),
            cx,
        ));

        cloud_sync_card(&self.tokens)
            .child(
                self.render_cloud_sync_section_title("plugin.cloud_sync.sections.sync_scope", cx),
            )
            .child(cloud_sync_toggle_grid(&self.tokens, toggles))
            .into_any_element()
    }

    fn render_cloud_sync_scope_bool_toggle(
        &self,
        label_key: &'static str,
        checked: bool,
        update: fn(&mut RawSyncScope, bool),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_cloud_sync_toggle(
            label_key,
            checked,
            cx.listener(
                move |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    if label_key == "plugin.cloud_sync.settings.sync_sensitive_credentials"
                        && !checked
                    {
                        this.cloud_sync_confirm = Some(CloudSyncConfirm::EnableSensitiveSync);
                        // Pointer-opened confirms should not paint a footer focus state
                        // until keyboard navigation explicitly enters the footer.
                        this.cloud_sync_confirm_focused_action = None;
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    update(&mut this.cloud_sync_store.state_mut().sync_scope, !checked);
                    this.finish_cloud_sync_scope_edit(cx);
                    cx.stop_propagation();
                },
            ),
            cx,
        )
    }

    fn render_cloud_sync_scope_section_toggle(
        &self,
        label_identity: String,
        label: String,
        checked: bool,
        section_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        cloud_sync_toggle(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-scope-section-toggle",
                label_identity,
                label,
                theme.text_muted,
                cx,
            ),
            checked,
            cx.listener(
                move |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.toggle_cloud_sync_app_settings_section(&section_id);
                    this.finish_cloud_sync_scope_edit(cx);
                    cx.stop_propagation();
                },
            ),
        )
    }

    fn toggle_cloud_sync_app_settings_section(&mut self, section_id: &str) {
        let mut sections =
            normalize_sync_scope(Some(&self.cloud_sync_store.state().sync_scope), &[])
                .app_settings_sections;
        if sections.iter().any(|section| section == section_id) {
            sections.retain(|section| section != section_id);
        } else {
            sections.push(section_id.to_string());
        }
        self.cloud_sync_store
            .state_mut()
            .sync_scope
            .app_settings_sections = Some(sections);
    }

    fn finish_cloud_sync_scope_edit(&mut self, cx: &mut Context<Self>) {
        self.clear_cloud_sync_select_focus();
        self.refresh_cloud_sync_local_dirty_state();
        self.save_cloud_sync_state();
        cx.notify();
    }

    fn render_cloud_sync_text_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder_key: &str,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let focused = self.focused_settings_input == Some(input);
        let value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.current_settings_input_value(input)
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        cloud_sync_field_row(
            &self.tokens,
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme.text_muted))
                .child(self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "cloud-sync-text-field-label",
                    label_key,
                    self.i18n.t(label_key),
                    theme.text_muted,
                    cx,
                ))
                .into_any_element(),
            text_input_anchor_probe(
                target.anchor_id(),
                text_input(
                    &self.tokens,
                    TextInputView {
                        value: &value,
                        placeholder: self.i18n.t(placeholder_key),
                        focused,
                        caret_visible: self.new_connection_caret_visible,
                        secret,
                        selected_all: false,
                        selected_range: self.ime_selected_range_for_target(target),
                        marked_text: self.marked_text_for_target(target),
                    },
                )
                .w_full()
                .min_w(px(0.0))
                .cursor(CursorStyle::IBeam)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        let current = this.current_settings_input_value(input);
                        this.focus_settings_input(input, current, cx);
                        this.ime_marked_text = None;
                        window.focus(&this.focus_handle);
                        this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                        this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener(
                    |this, event: &gpui::MouseMoveEvent, window, cx| {
                        this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                    },
                )),
                move |anchor, _window, cx| {
                    let _ = workspace.update(cx, |this, cx| {
                        this.update_text_input_anchor(anchor, cx);
                    });
                },
            )
            .into_any_element(),
        )
    }

    fn render_cloud_sync_secret_field(
        &self,
        label_key: &str,
        input: SettingsInput,
        placeholder_key: &str,
        secret_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stored = self
            .cloud_sync_store
            .state()
            .secret_hints
            .get(secret_key)
            .copied()
            .unwrap_or(false);
        let placeholder = if stored {
            "plugin.cloud_sync.placeholders.secret_stored"
        } else {
            placeholder_key
        };
        let action = if stored {
            let label = self.i18n.t(label_key);
            Some(self.render_cloud_sync_inline_button(
                "plugin.cloud_sync.actions.clear_secret",
                cx.listener(
                    move |this: &mut WorkspaceApp,
                          _event,
                          _window,
                          cx: &mut Context<WorkspaceApp>| {
                        this.cloud_sync_confirm = Some(CloudSyncConfirm::ClearSecret {
                            key: secret_key.to_string(),
                            label: label.clone(),
                        });
                        // Pointer-opened confirms should not paint a footer focus state
                        // until keyboard navigation explicitly enters the footer.
                        this.cloud_sync_confirm_focused_action = None;
                        this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ))
        } else {
            None
        };
        cloud_sync_secret_row(
            self.render_cloud_sync_text_field(label_key, input, placeholder, true, cx),
            action,
        )
    }

    fn render_cloud_sync_backend_select(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_cloud_sync_select_field(
            "plugin.cloud_sync.settings.backend_type",
            CloudSyncSelect::Backend,
            self.i18n.t(cloud_sync_backend_label_key(
                &self.cloud_sync_form.backend_type,
            )),
            cx,
            self.cloud_sync_select_options(CloudSyncSelect::Backend),
        )
    }

    fn render_cloud_sync_auth_mode_select(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = match self.cloud_sync_form.auth_mode {
            AuthMode::Bearer => self.i18n.t("plugin.cloud_sync.auth.bearer"),
            AuthMode::Basic => self.i18n.t("plugin.cloud_sync.auth.basic"),
            AuthMode::None => self.i18n.t("plugin.cloud_sync.auth.none"),
        };
        self.render_cloud_sync_select_field(
            "plugin.cloud_sync.settings.auth_mode",
            CloudSyncSelect::AuthMode,
            current,
            cx,
            self.cloud_sync_select_options(CloudSyncSelect::AuthMode),
        )
    }

    fn render_cloud_sync_conflict_select(&self, cx: &mut Context<Self>) -> AnyElement {
        let current = match self.cloud_sync_form.default_conflict_strategy {
            ConflictStrategy::Merge => self.i18n.t("plugin.cloud_sync.conflict.merge"),
            ConflictStrategy::Replace => self.i18n.t("plugin.cloud_sync.conflict.replace"),
            ConflictStrategy::Skip => self.i18n.t("plugin.cloud_sync.conflict.skip"),
            ConflictStrategy::Rename => self.i18n.t("plugin.cloud_sync.conflict.rename"),
        };
        self.render_cloud_sync_select_field(
            "plugin.cloud_sync.settings.default_conflict_strategy",
            CloudSyncSelect::ConflictStrategy,
            current,
            cx,
            self.cloud_sync_select_options(CloudSyncSelect::ConflictStrategy),
        )
    }

    fn cloud_sync_select_options(&self, select: CloudSyncSelect) -> Vec<CloudSyncSelectOption> {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync_form.backend_type.clone(),
            auth_mode: self.cloud_sync_form.auth_mode.clone(),
            default_conflict_strategy: self.cloud_sync_form.default_conflict_strategy.clone(),
            ..CloudSyncSettings::default()
        };
        cloud_sync_select_option_specs(&settings, select)
            .into_iter()
            .map(|option| CloudSyncSelectOption {
                label: self.i18n.t(cloud_sync_select_label_key(option.label_key)),
                selected: option.selected,
                action: option.action,
            })
            .collect()
    }

    fn cloud_sync_selected_option_index(&self, select: CloudSyncSelect) -> usize {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync_form.backend_type.clone(),
            auth_mode: self.cloud_sync_form.auth_mode.clone(),
            default_conflict_strategy: self.cloud_sync_form.default_conflict_strategy.clone(),
            ..CloudSyncSettings::default()
        };
        cloud_sync_selected_option_spec_index(&settings, select)
    }

    fn cloud_sync_focusable_selects(&self) -> Vec<CloudSyncSelect> {
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync_form.backend_type.clone(),
            auth_mode: self.cloud_sync_form.auth_mode.clone(),
            default_conflict_strategy: self.cloud_sync_form.default_conflict_strategy.clone(),
            ..CloudSyncSettings::default()
        };
        cloud_sync_focusable_selects(&settings)
    }

    fn toggle_cloud_sync_select_from_pointer(&mut self, select: CloudSyncSelect) {
        let selected_index = self.cloud_sync_selected_option_index(select);
        browser_behavior::toggle_browser_highlighted_select_from_pointer(
            &mut self.cloud_sync_open_select,
            &mut self.cloud_sync_focused_select,
            &mut self.cloud_sync_select_focus_origin,
            &mut self.cloud_sync_select_highlighted,
            select,
            selected_index,
        );
    }

    fn clear_cloud_sync_select_focus(&mut self) {
        browser_behavior::clear_browser_highlighted_select_focus(
            &mut self.cloud_sync_open_select,
            &mut self.cloud_sync_focused_select,
            &mut self.cloud_sync_select_focus_origin,
            &mut self.cloud_sync_select_highlighted,
        );
    }

    fn close_cloud_sync_select_for_scroll(&mut self) -> bool {
        close_cloud_sync_select_on_container_scroll(
            &mut self.cloud_sync_open_select,
            &mut self.cloud_sync_focused_select,
            &mut self.cloud_sync_select_highlighted,
        )
    }

    fn apply_cloud_sync_select_action(
        &mut self,
        action: CloudSyncSelectAction,
        cx: &mut Context<Self>,
    ) {
        // Tauri's Radix Select uses the same onValueChange path for mouse and
        // keyboard selection. Keep native mutations centralized so Enter and
        // pointer clicks cannot drift apart.
        let trigger_select = match action {
            CloudSyncSelectAction::Backend(backend) => {
                self.cloud_sync_form.backend_type = backend.clone();
                if matches!(backend, BackendType::Dropbox) {
                    self.cloud_sync_form.auth_mode = AuthMode::Bearer;
                } else if matches!(backend, BackendType::Git | BackendType::S3) {
                    self.cloud_sync_form.auth_mode = AuthMode::None;
                }
                CloudSyncSelect::Backend
            }
            CloudSyncSelectAction::AuthMode(auth_mode) => {
                self.cloud_sync_form.auth_mode = auth_mode;
                CloudSyncSelect::AuthMode
            }
            CloudSyncSelectAction::ConflictStrategy(strategy) => {
                self.cloud_sync_form.default_conflict_strategy = strategy;
                CloudSyncSelect::ConflictStrategy
            }
        };
        self.cloud_sync_open_select = None;
        self.cloud_sync_focused_select = Some(trigger_select);
        self.cloud_sync_select_highlighted = None;
        cx.notify();
    }

    pub(super) fn handle_cloud_sync_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let effect = reduce_cloud_sync_select_key(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            CloudSyncSelectKeyState {
                open_select: self.cloud_sync_open_select,
                focused_select: self.cloud_sync_focused_select,
                highlighted_option: self.cloud_sync_select_highlighted,
            },
            &self.cloud_sync_focusable_selects(),
            |select| self.cloud_sync_selected_option_index(select),
            |select| self.cloud_sync_select_options(select).len(),
        );
        let CloudSyncSelectKeyEffect::Handled {
            state,
            keyboard_focus_origin,
            selected_action_index,
        } = effect
        else {
            return false;
        };
        self.cloud_sync_open_select = state.open_select;
        self.cloud_sync_focused_select = state.focused_select;
        self.cloud_sync_select_highlighted = state.highlighted_option;
        if keyboard_focus_origin {
            self.cloud_sync_select_focus_origin =
                Some(browser_behavior::BrowserFocusOrigin::Keyboard);
        }
        if let (Some(select), Some(index)) = (self.cloud_sync_focused_select, selected_action_index)
        {
            if let Some(action) = self
                .cloud_sync_select_options(select)
                .get(index)
                .map(|option| option.action.clone())
            {
                self.apply_cloud_sync_select_action(action, cx);
            }
        }
        cx.notify();
        true
    }

    fn render_cloud_sync_select_field(
        &self,
        label_key: &str,
        select: CloudSyncSelect,
        value: String,
        cx: &mut Context<Self>,
        options: Vec<CloudSyncSelectOption>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let open = self.cloud_sync_open_select == Some(select);
        let focused = self.cloud_sync_focused_select == Some(select);
        let focus_visible =
            browser_behavior::browser_focus_visible(focused, self.cloud_sync_select_focus_origin);
        let menu = if open {
            let highlighted = self
                .cloud_sync_select_highlighted
                .filter(|(highlighted_select, _)| *highlighted_select == select)
                .map(|(_, index)| index)
                .unwrap_or_else(|| self.cloud_sync_selected_option_index(select));
            let option_rows = options
                .into_iter()
                .enumerate()
                .map(|(index, option)| {
                    let option_key = option.label.clone();
                    let label = option.label.clone();
                    let selected = option.selected;
                    let action = option.action.clone();
                    let option_highlighted = highlighted == index;
                    cloud_sync_select_option(
                        &self.tokens,
                        selected,
                        option_highlighted,
                        self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "cloud-sync-select-option",
                            option_key,
                            label,
                            if selected { theme.accent } else { theme.text },
                            cx,
                        ),
                        cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                            if this.cloud_sync_select_highlighted != Some((select, index)) {
                                this.cloud_sync_select_highlighted = Some((select, index));
                                cx.notify();
                            }
                        }),
                        cx.listener(move |this, _event, _window, cx| {
                            this.cloud_sync_select_focus_origin =
                                Some(browser_behavior::BrowserFocusOrigin::Pointer);
                            this.apply_cloud_sync_select_action(action.clone(), cx);
                            cx.stop_propagation();
                        }),
                    )
                })
                .collect::<Vec<_>>();
            Some(cloud_sync_select_menu(&self.tokens, option_rows))
        } else {
            None
        };
        cloud_sync_select_field(
            &self.tokens,
            self.render_selectable_text_scoped(
                "cloud-sync-select-label",
                label_key,
                self.i18n.t(label_key),
                theme.text_muted,
                cx,
            ),
            self.render_cloud_sync_select_trigger(select, value, open, focused, focus_visible, cx),
            menu,
        )
    }

    fn render_cloud_sync_select_trigger(
        &self,
        select: CloudSyncSelect,
        value: String,
        open: bool,
        focused: bool,
        focus_visible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        cloud_sync_select_trigger(
            &self.tokens,
            open,
            focused,
            focus_visible,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-select-value",
                format!("{select:?}"),
                value,
                theme.text,
                cx,
            ),
            cx.listener(
                move |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.toggle_cloud_sync_select_from_pointer(select);
                    cx.stop_propagation();
                    cx.notify();
                },
            ),
        )
    }

    fn render_cloud_sync_toggle(
        &self,
        label_key: &str,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Toggle labels are control text, so they match Tauri select-none behavior.
        cloud_sync_toggle(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-toggle-label",
                label_key,
                self.i18n.t(label_key),
                theme.text_muted,
                cx,
            ),
            checked,
            listener,
        )
    }

    fn render_cloud_sync_inline_button(
        &self,
        label_key: &str,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        // Cloud Sync inline actions are shadcn-style outline buttons in Tauri;
        // keep their chrome on the shared toolbar primitive instead of local
        // div/button styling.
        self.workspace_toolbar_action_button(
            self.i18n.t(label_key),
            None,
            cloud_sync_inline_button_options(&self.tokens),
            listener,
        )
        .into_any_element()
    }

    fn save_cloud_sync_configuration(&mut self, cx: &mut Context<Self>) {
        let (settings, interval) = cloud_sync_settings_from_form(&self.cloud_sync_form);
        let mut provider = CloudSyncKeychainSecretProvider::new(
            self.cloud_sync_store.state().secret_hints.clone(),
        );
        let secret_result = store_cloud_sync_touched_secrets(&self.cloud_sync_form, &mut provider);
        self.cloud_sync_store.state_mut().settings = settings;
        self.cloud_sync_store.state_mut().secret_hints = provider.hints().clone();
        normalize_cloud_sync_interval_draft(&mut self.cloud_sync_form, interval);
        reset_cloud_sync_secret_drafts(&mut self.cloud_sync_form);
        if let Err(error) = secret_result.and_then(|_| self.cloud_sync_store.save()) {
            self.cloud_sync_store.state_mut().last_error = Some(error.to_string());
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.settings_saved_failed_title"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
            );
        } else {
            self.cloud_sync_store.state_mut().last_error = None;
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.settings_saved_title"),
                None,
                TerminalNoticeVariant::Success,
            );
            self.reschedule_cloud_sync_auto_upload(cx);
            self.queue_cloud_sync_dirty_refresh(cx);
        }
    }

    fn clear_cloud_sync_secret(&mut self, secret_key: &str) {
        let mut provider = CloudSyncKeychainSecretProvider::new(
            self.cloud_sync_store.state().secret_hints.clone(),
        );
        if let Err(error) = provider.store_secret(secret_key, None) {
            self.cloud_sync_store.state_mut().last_error = Some(error.to_string());
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.secret_cleared_failed_title"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
            );
            return;
        }
        self.cloud_sync_store.state_mut().secret_hints = provider.hints().clone();
        self.cloud_sync_store.state_mut().last_error = None;
        if let Err(error) = self.cloud_sync_store.save() {
            self.cloud_sync_store.state_mut().last_error = Some(error.to_string());
            self.push_cloud_sync_toast(
                self.i18n
                    .t("plugin.cloud_sync.toast.secret_cleared_failed_title"),
                Some(error.to_string()),
                TerminalNoticeVariant::Error,
            );
        } else {
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.secret_cleared_title"),
                None,
                TerminalNoticeVariant::Success,
            );
        }
    }

    fn push_cloud_sync_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    ) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description,
            status_text: None,
            progress: None,
            variant,
        });
    }

    fn render_cloud_sync_fact(
        &self,
        label_key: &str,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key).to_uppercase();
        cloud_sync_fact_card(
            &self.tokens,
            self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-fact-label",
                label_key,
                label.clone(),
                theme.text_muted,
                cx,
            ),
            self.render_selectable_text(
                crate::workspace::selectable_text::selectable_text_id(
                    "cloud-sync-fact",
                    (&label, &value),
                ),
                value.clone(),
                self.tokens.ui.text,
                cx,
            ),
            cloud_sync_value_prefers_mono(&value),
            Some(settings_mono_font_family(self.settings_store.settings())),
        )
    }

    fn open_cloud_sync_import_confirm(&mut self) {
        if self.cloud_sync_pending_preview.is_none() {
            return;
        }
        self.cloud_sync_confirm = Some(CloudSyncConfirm::ImportPreview);
        self.cloud_sync_confirm_focused_action = None;
    }

    fn open_cloud_sync_restore_confirm(&mut self, backup: Option<(String, String)>) {
        let selected = backup.or_else(|| {
            self.cloud_sync_store
                .state()
                .rollback_backups
                .first()
                .map(|backup| (backup.id.clone(), backup.created_at.clone()))
        });
        if let Some((id, created_at)) = selected {
            self.cloud_sync_confirm = Some(CloudSyncConfirm::RestoreBackup { id, created_at });
            self.cloud_sync_confirm_focused_action = None;
        }
    }

    fn cancel_cloud_sync_confirm(&mut self) {
        self.cloud_sync_confirm = None;
        self.cloud_sync_confirm_focused_action = None;
    }

    fn confirm_cloud_sync_confirm(&mut self, cx: &mut Context<Self>) {
        let confirm = self.cloud_sync_confirm.take();
        self.cloud_sync_confirm_focused_action = None;
        match confirm {
            Some(CloudSyncConfirm::ImportPreview) => self.start_cloud_sync_apply_preview(cx),
            Some(CloudSyncConfirm::ClearSecret { key, .. }) => self.clear_cloud_sync_secret(&key),
            Some(CloudSyncConfirm::RestoreBackup { id, .. }) => {
                self.start_cloud_sync_restore_backup(id, cx)
            }
            Some(CloudSyncConfirm::EnableSensitiveSync) => {
                self.cloud_sync_store
                    .state_mut()
                    .sync_scope
                    .sync_sensitive_credentials = Some(true);
                self.finish_cloud_sync_scope_edit(cx);
            }
            None => {}
        }
    }

    pub(super) fn bootstrap_cloud_sync_controller(&mut self, cx: &mut Context<Self>) {
        self.cloud_sync_store
            .state_mut()
            .ensure_device_id(cloud_sync_platform_label());
        self.refresh_cloud_sync_local_dirty_state();
        self.save_cloud_sync_state();
        self.reschedule_cloud_sync_auto_upload(cx);
        let settings = self.cloud_sync_store.state().settings.clone();
        if backend_uses_auth_mode(&settings.backend_type)
            && !settings.endpoint.trim().is_empty()
            && matches!(settings.auth_mode, AuthMode::None)
        {
            self.start_cloud_sync_check_with_options(true, cx);
        }
    }

    fn reschedule_cloud_sync_auto_upload(&mut self, cx: &mut Context<Self>) {
        self.cloud_sync_auto_upload_generation =
            self.cloud_sync_auto_upload_generation.wrapping_add(1);
        if !self.cloud_sync_store.state().settings.auto_upload_enabled {
            return;
        }
        let generation = self.cloud_sync_auto_upload_generation;
        cx.spawn(async move |weak, cx| {
            loop {
                let wait = weak
                    .update(cx, |this, _cx| {
                        if this.cloud_sync_auto_upload_generation != generation
                            || !this.cloud_sync_store.state().settings.auto_upload_enabled
                        {
                            return None;
                        }
                        let interval = this
                            .cloud_sync_store
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
                        if this.cloud_sync_auto_upload_generation != generation
                            || !this.cloud_sync_store.state().settings.auto_upload_enabled
                        {
                            return false;
                        }
                        this.refresh_cloud_sync_local_dirty_state();
                        let state = this.cloud_sync_store.state();
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

    fn refresh_cloud_sync_local_dirty_state(&mut self) {
        let Ok(snapshot) = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            self.cloud_sync_store
                .state()
                .last_synced_structured_state
                .as_ref(),
            Some(&self.cloud_sync_store.state().sync_scope),
        ) else {
            return;
        };
        self.cloud_sync_store.state_mut().local_dirty = snapshot.dirty.has_dirty;
        self.cloud_sync_store.state_mut().local_dirty_sections =
            Some(snapshot.dirty.dirty_sections);
    }

    pub(super) fn queue_cloud_sync_dirty_refresh(&mut self, cx: &mut Context<Self>) {
        self.cloud_sync_dirty_refresh_generation =
            self.cloud_sync_dirty_refresh_generation.wrapping_add(1);
        let generation = self.cloud_sync_dirty_refresh_generation;
        self.cloud_sync_dirty_refresh_scheduled = true;
        cx.spawn(async move |weak, cx| {
            Timer::after(Duration::from_millis(300)).await;
            let _ = weak.update(cx, |this, cx| {
                if this.cloud_sync_dirty_refresh_generation != generation {
                    return;
                }
                this.cloud_sync_dirty_refresh_scheduled = false;
                this.refresh_cloud_sync_local_dirty_state();
                this.save_cloud_sync_state();
                cx.notify();
            });
        })
        .detach();
    }

    fn start_cloud_sync_check(&mut self, cx: &mut Context<Self>) {
        self.start_cloud_sync_check_with_options(false, cx);
    }

    fn start_cloud_sync_check_with_options(&mut self, skip_if_busy: bool, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            if !skip_if_busy {
                self.mark_cloud_sync_operation_in_progress();
            }
            return;
        }
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync_store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.cloud_sync_active_action = Some("check");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(deliver_cloud_sync_check(
            tx,
            service,
            settings,
            hints,
            skip_if_busy,
        ));
    }

    fn start_cloud_sync_upload_with_options(
        &mut self,
        force: bool,
        automatic: bool,
        skip_if_busy: bool,
        cx: &mut Context<Self>,
    ) {
        if self.cloud_sync_rx.is_some() {
            if !skip_if_busy {
                self.mark_cloud_sync_operation_in_progress();
            }
            return;
        }
        let (device_id, revision_sequence) = {
            let state = self.cloud_sync_store.state_mut();
            let device_id = state.ensure_device_id(cloud_sync_platform_label());
            let revision_sequence = state.revision_seq + 1;
            state.last_error = None;
            (device_id, revision_sequence)
        };
        self.cloud_sync_upload_preview = None;
        self.cloud_sync_upload_selection = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let previous_remote_sections = self
            .cloud_sync_store
            .state()
            .last_synced_remote_sections
            .clone();
        let previous_remote_revision = self
            .cloud_sync_store
            .state()
            .last_known_remote_revision
            .clone();
        let last_synced_structured_state = self
            .cloud_sync_store
            .state()
            .last_synced_structured_state
            .clone();
        let upload_selection = (!automatic)
            .then(|| self.cloud_sync_upload_selection.clone())
            .flatten();
        let raw_sync_scope = upload_selection
            .as_ref()
            .map(|selection| selection.raw_scope(&self.cloud_sync_store.state().sync_scope))
            .unwrap_or_else(|| self.cloud_sync_store.state().sync_scope.clone());
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
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.cloud_sync_active_action = Some("upload");
        self.cloud_sync_upload_selection = None;
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

    fn start_cloud_sync_upload_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync_store.state_mut().last_error = None;
        self.cloud_sync_upload_preview = None;
        self.cloud_sync_upload_selection = None;
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_preview_selection = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let previous_remote_sections = self
            .cloud_sync_store
            .state()
            .last_synced_remote_sections
            .clone();
        let connection_store = self.connection_store.clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.cloud_sync_active_action = Some("upload_preview");
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

    fn collect_cloud_sync_sensitive_portable_secrets(
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
                .filter(|provider_id| self.ai_key_store.has_provider_key(provider_id))
                .collect::<Vec<_>>();
        self.ai_key_store
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

    fn start_cloud_sync_pull_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync_store.state_mut().last_error = None;
        self.cloud_sync_upload_preview = None;
        self.cloud_sync_upload_selection = None;
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_preview_selection = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let previous_remote_sections = self
            .cloud_sync_store
            .state()
            .last_synced_remote_sections
            .clone();
        let connection_store = self.connection_store.clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.cloud_sync_active_action = Some("pull");
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

    fn start_cloud_sync_restore_backup(&mut self, backup_id: String, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        let Some(backup) = self
            .cloud_sync_store
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
        self.cloud_sync_store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let connection_store = self.connection_store.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.cloud_sync_active_action = Some("restore");
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

    fn start_cloud_sync_apply_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        let Some(preview) = self.cloud_sync_pending_preview.clone() else {
            return;
        };
        let selection = self
            .cloud_sync_preview_selection
            .clone()
            .unwrap_or_else(|| {
                CloudSyncPreviewSelection::from_preview(
                    &preview,
                    self.cloud_sync_store
                        .state()
                        .settings
                        .default_conflict_strategy
                        .clone(),
                )
            });
        let create_rollback_backup = cloud_sync_should_create_rollback_backup(
            &preview,
            self.cloud_sync_store.state().local_dirty,
        );
        self.cloud_sync_store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let settings_store = self.settings_store.clone();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let source_revision = self
            .cloud_sync_store
            .state()
            .last_known_remote_revision
            .clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.cloud_sync_active_action = Some("apply");
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

    fn schedule_cloud_sync_poll(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync_polling {
            return;
        }
        self.cloud_sync_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(Duration::from_millis(50)).await;
                let keep_polling = weak
                    .update(cx, |this, cx| {
                        this.poll_cloud_sync_delivery(cx);
                        this.cloud_sync_polling
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_cloud_sync_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.cloud_sync_rx.as_ref() else {
            self.cloud_sync_polling = false;
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
            self.cloud_sync_rx = None;
            self.cloud_sync_polling = false;
            self.cloud_sync_active_action = None;
            if matches!(
                self.cloud_sync_store.state().status,
                CloudSyncStatus::Uploading | CloudSyncStatus::Checking
            ) {
                self.cloud_sync_store.state_mut().status = CloudSyncStatus::Idle;
                self.save_cloud_sync_state();
            }
            if let Some(automatic) = self.cloud_sync_upload_after_current.take() {
                self.start_cloud_sync_upload_with_options(false, automatic, true, cx);
            }
        }
        cx.notify();
    }

    fn handle_cloud_sync_delivery(&mut self, delivery: CloudSyncDelivery, cx: &mut Context<Self>) {
        match delivery {
            CloudSyncDelivery::Progress(progress) => {
                if self.cloud_sync_active_action == Some("upload")
                    && self.cloud_sync_store.state().status != CloudSyncStatus::Uploading
                {
                    self.cloud_sync_store.state_mut().status = CloudSyncStatus::Uploading;
                    self.cloud_sync_store.state_mut().last_error = None;
                    self.save_cloud_sync_state();
                }
                self.cloud_sync_progress = Some(progress);
            }
            CloudSyncDelivery::RollbackBackupCreated(backup) => {
                self.cloud_sync_store
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
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(metadata) => self.finish_cloud_sync_check(metadata),
                    Err(error) => self.finish_cloud_sync_error("check", error),
                }
            }
            CloudSyncDelivery::UploadFinished { action, automatic } => {
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                if let Some(metadata) = action.remote_metadata.as_ref() {
                    persist_remote_metadata(self.cloud_sync_store.state_mut(), metadata);
                }
                if let Some(sequence) = action.revision_sequence_consumed {
                    let revision_seq = self.cloud_sync_store.state().revision_seq.max(sequence);
                    self.cloud_sync_store.state_mut().revision_seq = revision_seq;
                }
                match action.result {
                    Ok(outcome) => self.finish_cloud_sync_upload(outcome, automatic),
                    Err(error) => {
                        if automatic {
                            self.finish_cloud_sync_automatic_upload_error(error);
                        } else {
                            self.finish_cloud_sync_error("upload", error);
                        }
                    }
                }
            }
            CloudSyncDelivery::UploadPreviewFinished(action) => {
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(preview) => self.finish_cloud_sync_upload_preview(preview),
                    Err(error) if error.starts_with("remote_not_found") => {
                        self.cloud_sync_upload_after_current = Some(false);
                    }
                    Err(error) => self.finish_cloud_sync_error("upload_preview", error),
                }
            }
            CloudSyncDelivery::PullPreviewFinished(action) => {
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(preview) => self.finish_cloud_sync_pull_preview(preview),
                    Err(error) => self.finish_cloud_sync_error("pull", error),
                }
            }
            CloudSyncDelivery::RestoreBackupPreviewFinished(action) => {
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(preview) => self.finish_cloud_sync_pull_preview(preview),
                    Err(error) => self.finish_cloud_sync_error("restore", error),
                }
            }
            CloudSyncDelivery::ApplyPreviewFinished(action) => {
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(outcome) => self.finish_cloud_sync_apply_preview(outcome, cx),
                    Err(error) => self.finish_cloud_sync_error("apply", error),
                }
            }
        }
    }

    fn finish_cloud_sync_check(
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
                    self.cloud_sync_store
                        .state()
                        .last_synced_structured_state
                        .as_ref(),
                    Some(&self.cloud_sync_store.state().sync_scope),
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
            self.cloud_sync_store.state_mut(),
            metadata.as_ref(),
            dirty.as_ref(),
            Some(conflict_error),
            now,
        );
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_upload(&mut self, outcome: UploadOutcome, automatic: bool) {
        let revision = finish_cloud_sync_upload_state(self.cloud_sync_store.state_mut(), &outcome);
        self.cloud_sync_progress = None;
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_upload_preview = None;
        self.cloud_sync_upload_selection = None;
        self.save_cloud_sync_state();
        if !automatic {
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.upload_success_title"),
                Some(revision),
                TerminalNoticeVariant::Success,
            );
        }
    }

    fn finish_cloud_sync_automatic_upload_error(&mut self, error: String) {
        let display_error = self.format_cloud_sync_error(&error);
        let history_summary = self.cloud_sync_upload_failure_summary();
        finish_cloud_sync_automatic_upload_error_state(
            self.cloud_sync_store.state_mut(),
            &error,
            display_error,
            history_summary,
        );
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_pull_preview(&mut self, preview: CloudSyncPendingPreview) {
        finish_cloud_sync_pull_preview_state(self.cloud_sync_store.state_mut(), &preview);
        self.cloud_sync_preview_selection = Some(CloudSyncPreviewSelection::from_preview(
            &preview,
            self.cloud_sync_store
                .state()
                .settings
                .default_conflict_strategy
                .clone(),
        ));
        self.cloud_sync_upload_preview = None;
        self.cloud_sync_upload_selection = None;
        self.cloud_sync_pending_preview = Some(preview);
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_upload_preview(&mut self, preview: CloudSyncPendingPreview) {
        finish_cloud_sync_pull_preview_state(self.cloud_sync_store.state_mut(), &preview);
        let scope = normalize_sync_scope(Some(&self.cloud_sync_store.state().sync_scope), &[]);
        let local = self.cloud_sync_local_field_diff_snapshot();
        self.cloud_sync_upload_selection = Some(
            CloudSyncUploadSelection::from_scope_and_local_snapshot(&scope, &local),
        );
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_preview_selection = None;
        self.cloud_sync_upload_preview = Some(preview);
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_apply_preview(
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

    fn finish_structured_cloud_sync_apply(&mut self, outcome: ApplyStructuredPreviewOutcome) {
        let mut outcome = outcome;
        if let Some(envelope) = outcome.sensitive_credentials_envelope.as_mut() {
            self.apply_oxide_import_portable_secrets(envelope);
        }
        let previous_local_baseline = self
            .cloud_sync_store
            .state()
            .last_synced_structured_state
            .clone();
        let local_snapshot = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            previous_local_baseline.as_ref(),
            Some(&self.cloud_sync_store.state().sync_scope),
        )
        .unwrap_or_else(|_| outcome.local_snapshot.clone());
        let should_trigger_upload_after = finish_structured_cloud_sync_apply_state(
            self.cloud_sync_store.state_mut(),
            &outcome,
            &local_snapshot,
            Utc::now().to_rfc3339(),
        );
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_upload_preview = None;
        self.cloud_sync_upload_selection = None;
        self.cloud_sync_preview_selection = None;
        self.cloud_sync_progress = None;
        if should_trigger_upload_after {
            self.cloud_sync_upload_after_current = Some(true);
        }
        self.save_cloud_sync_state();
        self.push_cloud_sync_toast(
            self.i18n.t("plugin.cloud_sync.toast.pull_success_title"),
            Some(self.i18n_replace(
                "plugin.cloud_sync.toast.pull_success_description",
                &[
                    ("imported", outcome.content_summary.connections.to_string()),
                    ("merged", "0".to_string()),
                ],
            )),
            TerminalNoticeVariant::Success,
        );
    }

    fn finish_legacy_cloud_sync_apply(
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

        let local_snapshot = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            None,
            Some(&self.cloud_sync_store.state().sync_scope),
        );
        let should_trigger_upload_after = finish_legacy_cloud_sync_apply_state(
            self.cloud_sync_store.state_mut(),
            &preview,
            &source,
            &selection,
            local_snapshot.as_ref().ok(),
            Utc::now().to_rfc3339(),
        );
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_upload_preview = None;
        self.cloud_sync_upload_selection = None;
        self.cloud_sync_preview_selection = None;
        self.cloud_sync_progress = None;
        if should_trigger_upload_after {
            self.cloud_sync_upload_after_current = Some(true);
        }
        self.save_cloud_sync_state();
        let copy = plan.success_copy;
        self.push_cloud_sync_toast(
            self.i18n.t(copy.title_key),
            Some(self.i18n_replace(
                copy.description_key,
                &[
                    ("imported", outcome.envelope.imported.to_string()),
                    ("merged", outcome.envelope.merged.to_string()),
                ],
            )),
            TerminalNoticeVariant::Success,
        );
    }

    fn finish_cloud_sync_error(&mut self, action: &str, error: String) {
        let display_error = self.format_cloud_sync_error(&error);
        let upload_history_summary =
            (action == "upload").then(|| self.cloud_sync_upload_failure_summary());
        finish_cloud_sync_error_state(
            self.cloud_sync_store.state_mut(),
            action,
            &error,
            display_error.clone(),
            upload_history_summary,
        );
        self.cloud_sync_progress = None;
        if action == "upload_preview" {
            self.cloud_sync_upload_preview = None;
            self.cloud_sync_upload_selection = None;
        }
        self.save_cloud_sync_state();
        let title_key = match action {
            "upload" => Some("plugin.cloud_sync.toast.upload_failed_title"),
            "apply" => Some(
                if self
                    .cloud_sync_pending_preview
                    .as_ref()
                    .is_some_and(CloudSyncPendingPreview::is_backup)
                {
                    "plugin.cloud_sync.toast.restore_failed_title"
                } else {
                    "plugin.cloud_sync.toast.pull_failed_title"
                },
            ),
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

    fn mark_cloud_sync_operation_in_progress(&mut self) {
        self.cloud_sync_store.state_mut().last_error = Some(
            self.i18n
                .t("plugin.cloud_sync.errors.operation_in_progress"),
        );
        self.save_cloud_sync_state();
    }

    fn cloud_sync_upload_failure_summary(&self) -> CloudSyncHistorySummary {
        CloudSyncHistorySummary {
            connections: self.connection_store.connections().len(),
            forwards: self.forwarding_registry.list_all_saved_forwards().len(),
            quick_commands: 0,
            serial_profiles: self.connection_store.serial_profiles().len(),
            sensitive_credentials: 0,
            has_app_settings: true,
            plugin_settings_count: 0,
        }
    }

    fn save_cloud_sync_state(&mut self) {
        if let Err(error) = self.cloud_sync_store.save() {
            self.cloud_sync_store.state_mut().last_error = Some(error.to_string());
        }
    }
}
