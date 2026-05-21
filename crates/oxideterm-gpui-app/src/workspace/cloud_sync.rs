use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    sync::mpsc::{self, TryRecvError},
};

use crate::workspace::ime::WorkspaceImeTarget;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Local, Utc};
use gpui::prelude::*;
use oxideterm_cloud_sync::{
    AuthMode, BackendType, CloudSyncSettings, CloudSyncStatus, ConflictStrategy,
    MAX_ROLLBACK_BACKUP_BYTES, PREVIEW_RECORD_LIMIT, STRUCTURED_MANIFEST_FORMAT,
    StructuredApplySelection, StructuredManifest, StructuredSectionRevisions,
    build_manifest_section_revisions, compute_structured_dirty_sections, merge_structured_baseline,
    operation::{
        ApplyLegacyPreviewOutcome, ApplyStructuredPreviewOutcome, LegacyPreview, StructuredPreview,
        UploadOptions, UploadOutcome,
    },
    progress::{CloudSyncProgress, CloudSyncProgressSink, CloudSyncProgressStage},
    secret_keys,
    secrets::{
        CloudSyncKeychainSecretProvider, SecretReadMode, backend_uses_auth_mode,
        backend_uses_basic, backend_uses_git_token, backend_uses_s3_credentials,
        backend_uses_token, get_action_secrets,
    },
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::{
        CloudSyncConflictDetails, CloudSyncHistoryEntry, CloudSyncHistorySummary,
        CloudSyncPersistedState, CloudSyncRollbackBackup, CloudSyncRollbackBackupMetadata,
    },
};
use oxideterm_connections::oxide_file::{
    ImportConflictStrategy, OxideExportOptions, OxideFile, OxideForwardRecord,
    export_connections_to_oxide_with_progress, preview_oxide_import_with_progress,
};
use oxideterm_gpui_settings_view::SettingsInput;
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions, toolbar_button,
};
use oxideterm_gpui_ui::select::select_trigger_focus_visible;
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};

use super::quick_commands::QuickCommandImportStrategy;
use super::session_manager::OxideClientStateImportOptions;
use super::*;

const CLOUD_SYNC_PANEL_PADDING: f32 = 16.0;
const CLOUD_SYNC_CARD_PADDING: f32 = 12.0;
const CLOUD_SYNC_CARD_GAP: f32 = 12.0;
const CLOUD_SYNC_GRID_GAP: f32 = 8.0;
const CLOUD_SYNC_STAT_PADDING: f32 = 8.0;
const CLOUD_SYNC_BG_MIX_ALPHA: u32 = 0x80;
const CLOUD_SYNC_LIST_BORDER_ALPHA: u32 = 0xA6;
const CLOUD_SYNC_LIST_BG_ALPHA: u32 = 0x8C;
const CLOUD_SYNC_SELECT_HIGHLIGHT_ALPHA: u32 = 0x26; // Radix SelectItem focus:bg-theme-bg-hover.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CloudSyncSelect {
    Backend,
    AuthMode,
    ConflictStrategy,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum CloudSyncSelectAction {
    Backend(BackendType),
    AuthMode(AuthMode),
    ConflictStrategy(ConflictStrategy),
}

#[derive(Clone, Debug)]
struct CloudSyncSelectOption {
    label: String,
    selected: bool,
    action: CloudSyncSelectAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SelectKeyDirection {
    Previous,
    Next,
}

#[derive(Clone, Debug)]
pub(super) struct CloudSyncFormDraft {
    pub(super) backend_type: BackendType,
    pub(super) auth_mode: AuthMode,
    pub(super) endpoint: String,
    pub(super) namespace: String,
    pub(super) s3_bucket: String,
    pub(super) s3_region: String,
    pub(super) git_repository: String,
    pub(super) git_branch: String,
    pub(super) auto_upload_enabled: bool,
    pub(super) auto_upload_interval_mins: String,
    pub(super) default_conflict_strategy: ConflictStrategy,
    pub(super) token: String,
    pub(super) git_token: String,
    pub(super) basic_username: String,
    pub(super) basic_password: String,
    pub(super) access_key_id: String,
    pub(super) secret_access_key: String,
    pub(super) session_token: String,
    pub(super) sync_password: String,
    pub(super) token_touched: bool,
    pub(super) git_token_touched: bool,
    pub(super) basic_username_touched: bool,
    pub(super) basic_password_touched: bool,
    pub(super) access_key_id_touched: bool,
    pub(super) secret_access_key_touched: bool,
    pub(super) session_token_touched: bool,
    pub(super) sync_password_touched: bool,
}

impl CloudSyncFormDraft {
    pub(super) fn from_settings(settings: &CloudSyncSettings) -> Self {
        Self {
            backend_type: settings.backend_type.clone(),
            auth_mode: settings.auth_mode.clone(),
            endpoint: settings.endpoint.clone(),
            namespace: settings.namespace.clone(),
            s3_bucket: settings.s3_bucket.clone(),
            s3_region: settings.s3_region.clone(),
            git_repository: settings.git_repository.clone(),
            git_branch: settings.git_branch.clone(),
            auto_upload_enabled: settings.auto_upload_enabled,
            auto_upload_interval_mins: settings.auto_upload_interval_mins.to_string(),
            default_conflict_strategy: settings.default_conflict_strategy.clone(),
            token: String::new(),
            git_token: String::new(),
            basic_username: String::new(),
            basic_password: String::new(),
            access_key_id: String::new(),
            secret_access_key: String::new(),
            session_token: String::new(),
            sync_password: String::new(),
            token_touched: false,
            git_token_touched: false,
            basic_username_touched: false,
            basic_password_touched: false,
            access_key_id_touched: false,
            secret_access_key_touched: false,
            session_token_touched: false,
            sync_password_touched: false,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum CloudSyncPendingPreview {
    Structured(StructuredPreview),
    Legacy {
        preview: LegacyPreview,
        source: CloudSyncPreviewSource,
    },
}

impl CloudSyncPendingPreview {
    fn is_backup(&self) -> bool {
        matches!(
            self,
            Self::Legacy {
                source: CloudSyncPreviewSource::Backup { .. },
                ..
            }
        )
    }
}

#[derive(Clone, Debug)]
pub(super) enum CloudSyncPreviewSource {
    Remote,
    Backup { id: String, created_at: String },
}

impl CloudSyncPreviewSource {
    fn is_backup(&self) -> bool {
        match self {
            Self::Remote => false,
            Self::Backup { id, created_at } => {
                let _ = (id, created_at);
                true
            }
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum CloudSyncConfirm {
    ImportPreview,
    ClearSecret { key: String, label: String },
    RestoreBackup { id: String, created_at: String },
}

pub(super) enum CloudSyncDelivery {
    Progress(CloudSyncProgress),
    RollbackBackupCreated(CloudSyncRollbackBackup),
    CheckFinished(CloudSyncActionResult<Option<oxideterm_cloud_sync::backend::RemoteMetadata>>),
    UploadFinished {
        action: CloudSyncUploadActionResult,
        automatic: bool,
    },
    PullPreviewFinished(CloudSyncActionResult<CloudSyncPendingPreview>),
    RestoreBackupPreviewFinished(CloudSyncActionResult<CloudSyncPendingPreview>),
    ApplyPreviewFinished(CloudSyncActionResult<CloudSyncApplyUiOutcome>),
}

pub(super) struct CloudSyncActionResult<T> {
    result: Result<T, String>,
    secret_hints: BTreeMap<String, bool>,
}

pub(super) struct CloudSyncUploadActionResult {
    result: Result<UploadOutcome, String>,
    remote_metadata: Option<oxideterm_cloud_sync::backend::RemoteMetadata>,
    revision_sequence_consumed: Option<u64>,
    secret_hints: BTreeMap<String, bool>,
}

pub(super) struct CloudSyncApplyUiOutcome {
    connection_store: ConnectionStore,
    settings_store: SettingsStore,
    outcome: CloudSyncApplyOutcome,
}

pub(super) enum CloudSyncApplyOutcome {
    Structured(ApplyStructuredPreviewOutcome),
    Legacy {
        preview: LegacyPreview,
        source: CloudSyncPreviewSource,
        selection: CloudSyncPreviewSelection,
        outcome: ApplyLegacyPreviewOutcome,
    },
}

#[derive(Clone, Debug)]
pub(super) struct CloudSyncPreviewSelection {
    import_connections: bool,
    selected_connection_names: BTreeSet<String>,
    import_app_settings: bool,
    selected_app_settings_sections: BTreeSet<String>,
    import_plugin_settings: bool,
    selected_plugin_ids: BTreeSet<String>,
    import_forwards: bool,
    conflict_strategy: ConflictStrategy,
}

impl CloudSyncPreviewSelection {
    fn from_preview(
        preview: &CloudSyncPendingPreview,
        default_conflict_strategy: ConflictStrategy,
    ) -> Self {
        let summary = cloud_sync_preview_summary(preview);
        let conflict_strategy = match preview {
            CloudSyncPendingPreview::Legacy {
                source: CloudSyncPreviewSource::Backup { .. },
                ..
            } => ConflictStrategy::Replace,
            _ => default_conflict_strategy,
        };
        Self {
            import_connections: summary.connections > 0,
            selected_connection_names: summary.connection_record_names(),
            import_app_settings: summary.has_app_settings,
            selected_app_settings_sections: summary
                .app_settings_sections
                .iter()
                .map(|section| section.id.clone())
                .collect(),
            import_plugin_settings: summary.plugin_settings_count > 0,
            selected_plugin_ids: summary.plugin_settings_by_plugin.keys().cloned().collect(),
            import_forwards: summary.forwards > 0,
            conflict_strategy,
        }
    }

    fn effective_import_connections(&self, summary: &CloudSyncPreviewSummary) -> bool {
        if !self.import_connections {
            return false;
        }
        let record_names = summary.connection_record_names();
        record_names.is_empty()
            || record_names
                .iter()
                .any(|name| self.selected_connection_names.contains(name))
    }

    fn selected_connection_names_for_import(
        &self,
        summary: &CloudSyncPreviewSummary,
    ) -> Option<Vec<String>> {
        if !self.import_connections {
            return Some(Vec::new());
        }
        let record_names = summary.connection_record_names();
        if record_names.is_empty() {
            return None;
        }
        Some(
            record_names
                .into_iter()
                .filter(|name| self.selected_connection_names.contains(name))
                .collect(),
        )
    }

    fn effective_import_app_settings(&self, summary: &CloudSyncPreviewSummary) -> bool {
        self.import_app_settings
            && (!self.selected_app_settings_sections.is_empty()
                || summary.app_settings_sections.is_empty())
    }

    fn effective_import_plugin_settings(&self) -> bool {
        self.import_plugin_settings && !self.selected_plugin_ids.is_empty()
    }

    fn can_apply(&self, summary: &CloudSyncPreviewSummary) -> bool {
        self.effective_import_connections(summary)
            || self.import_forwards
            || self.effective_import_app_settings(summary)
            || self.effective_import_plugin_settings()
    }

    fn structured_selection(&self) -> StructuredApplySelection {
        StructuredApplySelection {
            connections: self.import_connections,
            forwards: self.import_forwards,
            app_settings_sections: if self.import_app_settings {
                self.selected_app_settings_sections
                    .iter()
                    .cloned()
                    .collect()
            } else {
                Vec::new()
            },
            plugin_ids: if self.import_plugin_settings {
                self.selected_plugin_ids.iter().cloned().collect()
            } else {
                Vec::new()
            },
        }
    }

    fn selected_app_settings_hash_set(
        &self,
        summary: &CloudSyncPreviewSummary,
    ) -> Option<HashSet<String>> {
        if !self.effective_import_app_settings(summary) {
            return Some(HashSet::new());
        }
        if self.selected_app_settings_sections.is_empty() {
            None
        } else {
            Some(
                self.selected_app_settings_sections
                    .iter()
                    .cloned()
                    .collect(),
            )
        }
    }

    fn selected_plugin_hash_set(&self) -> Option<HashSet<String>> {
        if !self.import_plugin_settings {
            return Some(HashSet::new());
        }
        if self.selected_plugin_ids.is_empty() {
            Some(HashSet::new())
        } else {
            Some(self.selected_plugin_ids.iter().cloned().collect())
        }
    }
}

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
        div()
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px(px(self.tokens.metrics.empty_sidebar_padding_x))
            .text_color(rgb(theme.text_muted))
            .child(div().mb_3().child(Self::render_lucide_icon(
                LucideIcon::Cloud,
                self.tokens.metrics.empty_sidebar_icon_size,
                rgb(theme.text_muted),
            )))
            .child(
                div()
                    .w_full()
                    .text_center()
                    .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-sidebar-empty",
                        "title",
                        self.i18n.t("plugin.cloud_sync.panel_title"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .mt_1()
                    .w_full()
                    .text_center()
                    .text_size(px(self.tokens.metrics.empty_sidebar_subtitle_font_size))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-sidebar-empty",
                        "description",
                        self.i18n.t("plugin.cloud_sync.native_description"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.poll_cloud_sync_delivery(cx);

        let theme = self.tokens.ui;
        let state = self.cloud_sync_store.state().clone();
        let settings = state.settings.clone();
        let local_snapshot = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            state.last_synced_structured_state.as_ref(),
            Some(&state.sync_scope),
        );
        let backend_label = self.cloud_sync_backend_label(&settings);
        let busy = self.cloud_sync_rx.is_some();
        let has_rollback_backup = !state.rollback_backups.is_empty();
        let cloud_sync_scroll = self.selectable_text_scroll_handle("cloud-sync-scroll");

        div()
            .id("cloud-sync-scroll")
            .size_full()
            .selectable_overflow_y_scrollbar(&cloud_sync_scroll)
            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                if this.close_cloud_sync_select_for_scroll() {
                    cx.notify();
                }
            }))
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .p(px(CLOUD_SYNC_PANEL_PADDING))
                    .flex()
                    .flex_col()
                    .gap(px(CLOUD_SYNC_CARD_GAP))
                    .child(self.render_cloud_sync_header(&state, cx))
                    .child(self.render_cloud_sync_guide(&self.cloud_sync_form.backend_type, cx))
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_panel))
                            .p(px(CLOUD_SYNC_CARD_PADDING))
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .when_some(self.cloud_sync_progress.as_ref(), |card, progress| {
                                card.child(self.render_cloud_sync_progress(progress, cx))
                            })
                            .when_some(state.last_error.as_ref(), |card, error| {
                                card.child(self.render_cloud_sync_error(error))
                            })
                            .child(
                                div()
                                    .w_full()
                                    .min_w(px(0.0))
                                    .grid()
                                    .grid_cols(2)
                                    .gap(px(CLOUD_SYNC_GRID_GAP))
                                    .child(self.render_cloud_sync_fact(
                                        "plugin.cloud_sync.fields.backend",
                                        backend_label,
                                        cx,
                                    ))
                                    .child(self.render_cloud_sync_fact(
                                        "plugin.cloud_sync.fields.namespace",
                                        settings.namespace,
                                        cx,
                                    ))
                                    .child(
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
                                                .unwrap_or_else(|_| {
                                                    self.i18n.t("plugin.cloud_sync.common.error")
                                                }),
                                            cx,
                                        ),
                                    )
                                    .child(
                                        self.render_cloud_sync_fact(
                                            "plugin.cloud_sync.fields.last_sync",
                                            state
                                                .last_sync_at
                                                .as_deref()
                                                .map(cloud_sync_format_timestamp)
                                                .unwrap_or_else(|| "—".to_string()),
                                            cx,
                                        ),
                                    ),
                            )
                            .child(self.render_cloud_sync_meta(
                                &state,
                                local_snapshot.as_ref().ok(),
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .grid()
                            .grid_cols(2)
                            .gap(px(CLOUD_SYNC_GRID_GAP))
                            .child(self.render_cloud_sync_action_button(
                                "plugin.cloud_sync.actions.upload_now",
                                ButtonVariant::Default,
                                busy,
                                cx.listener(|this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                                    this.start_cloud_sync_upload(false, cx);
                                    this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                                }),
                            ))
                            .child(self.render_cloud_sync_action_button(
                                "plugin.cloud_sync.actions.check_remote",
                                ButtonVariant::Outline,
                                busy,
                                cx.listener(|this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                                    this.start_cloud_sync_check(cx);
                                    this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                                }),
                            ))
                            .child(self.render_cloud_sync_action_button(
                                "plugin.cloud_sync.actions.pull_preview",
                                ButtonVariant::Outline,
                                busy,
                                cx.listener(|this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                                    this.start_cloud_sync_pull_preview(cx);
                                    this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                                }),
                            ))
                            .child(self.render_cloud_sync_action_button(
                                "plugin.cloud_sync.actions.restore_backup",
                                ButtonVariant::Outline,
                                busy || !has_rollback_backup,
                                cx.listener(|this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                                    this.open_cloud_sync_restore_confirm(None);
                                    this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.render_cloud_sync_action_button(
                                "plugin.cloud_sync.actions.save_settings",
                                ButtonVariant::Outline,
                                busy,
                                cx.listener(|this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                                    this.save_cloud_sync_configuration(cx);
                                    this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                                    cx.notify();
                                }),
                            )),
                    )
                    .when_some(
                        self.cloud_sync_pending_preview.as_ref(),
                        |panel, preview| panel.child(self.render_cloud_sync_preview(preview, &state, busy, cx)),
                    )
                    .when(!state.rollback_backups.is_empty(), |panel| {
                        panel.child(self.render_cloud_sync_rollback_backups(&state, busy, cx))
                    })
                    .child(self.render_cloud_sync_history(&state, cx))
                    .child(self.render_cloud_sync_config(cx))
                    .child(self.render_cloud_sync_notes(local_snapshot.as_ref().ok(), cx)),
            )
            .into_any_element()
    }

    fn render_cloud_sync_header(
        &self,
        state: &CloudSyncPersistedState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-panel",
                        "title",
                        self.i18n.t("plugin.cloud_sync.panel_title").to_uppercase(),
                        theme.text,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.cloud_sync_status_label(state.status.clone())),
            )
            .into_any_element()
    }

    fn render_cloud_sync_guide(
        &self,
        backend_type: &BackendType,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let backend_key = format!("{backend_type:?}");
        let mut card = self
            .cloud_sync_card()
            .child(
                self.render_cloud_sync_section_title("plugin.cloud_sync.sections.quick_start", cx),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.render_selectable_text_scoped(
                        "cloud-sync-guide-title",
                        &backend_key,
                        self.cloud_sync_usage_guide_title(backend_type),
                        theme.text_heading,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "cloud-sync-guide-description",
                        &backend_key,
                        self.cloud_sync_usage_guide_description(backend_type),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(self.render_cloud_sync_guide_steps(cx));
        let examples = self.cloud_sync_usage_guide_examples(backend_type);
        if !examples.is_empty() {
            let mut example_card = div()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgba((theme.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
                .p(px(CLOUD_SYNC_CARD_PADDING))
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(rgb(theme.text_heading))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "cloud-sync-guide",
                            "example-title",
                            self.i18n.t("plugin.cloud_sync.guide.example_title"),
                            theme.text_heading,
                            cx,
                        )),
                );
            for (label, value) in examples {
                example_card = example_card.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .line_height(px(20.0))
                        .text_color(rgb(theme.text_muted))
                        .child(self.render_selectable_text_scoped(
                            "cloud-sync-guide-example-label",
                            (&label, &value),
                            format!("{label}:"),
                            theme.text_muted,
                            cx,
                        ))
                        .child(
                            div()
                                .font_family(settings_mono_font_family(
                                    self.settings_store.settings(),
                                ))
                                .text_color(rgb(theme.accent))
                                .child(self.render_selectable_text_scoped(
                                    "cloud-sync-guide-example-value",
                                    (&label, &value),
                                    value.clone(),
                                    theme.accent,
                                    cx,
                                )),
                        ),
                );
            }
            card = card.child(example_card);
        }
        if matches!(backend_type, BackendType::Webdav) {
            card = card.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.accent))
                    .child(
                        self.render_selectable_text_scoped(
                            "cloud-sync-guide-warning",
                            &backend_key,
                            self.i18n
                                .t("plugin.cloud_sync.guide.webdav_duplicate_warning"),
                            theme.accent,
                            cx,
                        ),
                    ),
            );
        }
        card.into_any_element()
    }

    fn render_cloud_sync_guide_steps(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let steps = [
            "plugin.cloud_sync.guide.step_choose_backend",
            "plugin.cloud_sync.guide.step_fill_fields",
            "plugin.cloud_sync.guide.step_save",
            "plugin.cloud_sync.guide.step_check",
            "plugin.cloud_sync.guide.step_upload",
            "plugin.cloud_sync.guide.step_pull",
        ];
        let mut list = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .pl(px(20.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .text_color(rgb(theme.text_muted));
        for (index, key) in steps.into_iter().enumerate() {
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
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text_heading))
            .child(self.render_display_text_with_role(
                SelectableTextRole::PlainDocument,
                "cloud-sync-section-title",
                key,
                self.i18n.t(key).to_uppercase(),
                self.tokens.ui.text_heading,
                cx,
            ))
            .into_any_element()
    }

    fn cloud_sync_card(&self) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(CLOUD_SYNC_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(10.0))
    }

    fn render_cloud_sync_action_button(
        &self,
        label_key: &str,
        variant: ButtonVariant,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        toolbar_button(
            &self.tokens,
            self.i18n.t(label_key),
            None,
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
            },
        )
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, listener)
        })
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
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .justify_between()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-progress",
                        "stage",
                        self.cloud_sync_progress_stage_label(progress.stage),
                        theme.text,
                        cx,
                    ))
                    .child(self.render_display_text_with_role(
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
                    )),
            )
            .child(
                div()
                    .h(px(4.0))
                    .w_full()
                    .rounded(px(999.0))
                    .bg(rgb(theme.bg_hover))
                    .overflow_hidden()
                    .child(
                        div()
                            .h_full()
                            .w(relative(ratio))
                            .rounded(px(999.0))
                            .bg(rgb(theme.accent)),
                    ),
            )
            .into_any_element()
    }

    fn render_cloud_sync_error(&self, error: &str) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.error))
            .bg(rgba((theme.error << 8) | 0x14))
            .px(px(12.0))
            .py(px(10.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .line_height(px(20.0))
            .text_color(rgb(theme.error))
            .child(self.format_cloud_sync_error(error))
            .into_any_element()
    }

    fn format_cloud_sync_error(&self, error: &str) -> String {
        let Some(code) = cloud_sync_error_code(error) else {
            return error.to_string();
        };
        match code {
            "missing_endpoint" => self.i18n.t("plugin.cloud_sync.errors.missing_endpoint"),
            "missing_namespace" => self.i18n.t("plugin.cloud_sync.errors.missing_namespace"),
            "missing_backend_token" => self
                .i18n
                .t("plugin.cloud_sync.errors.missing_backend_token"),
            "http_unauthorized" => self.i18n.t("plugin.cloud_sync.errors.http_unauthorized"),
            "network_request_failed" => self
                .i18n
                .t("plugin.cloud_sync.errors.network_request_failed"),
            "missing_git_repository" => self
                .i18n
                .t("plugin.cloud_sync.errors.missing_git_repository"),
            "missing_s3_bucket" => self.i18n.t("plugin.cloud_sync.errors.missing_s3_bucket"),
            "missing_s3_region" => self.i18n.t("plugin.cloud_sync.errors.missing_s3_region"),
            "missing_s3_access_key_id" => self
                .i18n
                .t("plugin.cloud_sync.errors.missing_s3_access_key_id"),
            "missing_s3_secret_access_key" => self
                .i18n
                .t("plugin.cloud_sync.errors.missing_s3_secret_access_key"),
            "missing_sync_password" => self
                .i18n
                .t("plugin.cloud_sync.errors.missing_sync_password"),
            "operation_in_progress" => self
                .i18n
                .t("plugin.cloud_sync.errors.operation_in_progress"),
            "secret_unlock_required" => self
                .i18n
                .t("plugin.cloud_sync.errors.secret_unlock_required"),
            "secret_access_cancelled" => self
                .i18n
                .t("plugin.cloud_sync.errors.secret_access_cancelled"),
            "secret_access_failed" => self.i18n.t("plugin.cloud_sync.errors.secret_access_failed"),
            "etag_conflict_detected" => self
                .i18n
                .t("plugin.cloud_sync.errors.etag_conflict_detected"),
            "remote_changed_before_upload" => self
                .i18n
                .t("plugin.cloud_sync.errors.remote_changed_before_upload"),
            "preflight_failed" => self.i18n.t("plugin.cloud_sync.errors.preflight_failed"),
            "remote_not_found" => self.i18n.t("plugin.cloud_sync.errors.remote_not_found"),
            "snapshot_too_large" => self.i18n_replace(
                "plugin.cloud_sync.errors.snapshot_too_large",
                &[(
                    "limit",
                    cloud_sync_snapshot_limit_bytes(error)
                        .map(format_cloud_sync_bytes)
                        .unwrap_or_else(|| "—".to_string()),
                )],
            ),
            _ => error.to_string(),
        }
    }

    fn render_cloud_sync_meta(
        &self,
        state: &CloudSyncPersistedState,
        local_snapshot: Option<&CloudSyncLocalSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let counts = local_snapshot
            .map(|snapshot| {
                format!(
                    "{} / {}",
                    snapshot.connections_record_count, snapshot.forwards_record_count
                )
            })
            .unwrap_or_else(|| "—".to_string());
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(18.0))
            .text_color(rgb(theme.text_muted))
            .child(
                self.render_cloud_sync_meta_line(
                    "plugin.cloud_sync.fields.remote_revision",
                    state
                        .last_known_remote_revision
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                    cx,
                ),
            )
            .child(
                self.render_cloud_sync_meta_line(
                    "plugin.cloud_sync.fields.remote_device",
                    state
                        .remote_device_id
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                    cx,
                ),
            )
            .child(
                self.render_cloud_sync_meta_line(
                    "plugin.cloud_sync.fields.remote_updated_at",
                    state
                        .remote_updated_at
                        .as_deref()
                        .map(cloud_sync_format_timestamp)
                        .unwrap_or_else(|| "—".to_string()),
                    cx,
                ),
            )
            .child(self.render_cloud_sync_meta_line(
                "plugin.cloud_sync.fields.local_counts",
                counts,
                cx,
            ))
            .into_any_element()
    }

    fn render_cloud_sync_meta_line(
        &self,
        label_key: &str,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.i18n.t(label_key);
        let text = format!("{label}: {value}");
        div()
            .min_w(px(0.0))
            .overflow_hidden()
            .child(self.render_selectable_text(
                crate::workspace::selectable_text::selectable_text_id(
                    "cloud-sync-meta",
                    (&label, &value),
                ),
                text,
                self.tokens.ui.text_muted,
                cx,
            ))
            .into_any_element()
    }

    fn render_cloud_sync_preview(
        &self,
        preview: &CloudSyncPendingPreview,
        state: &CloudSyncPersistedState,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let summary = cloud_sync_preview_summary(preview);
        let selection = self
            .cloud_sync_preview_selection
            .clone()
            .unwrap_or_else(|| {
                CloudSyncPreviewSelection::from_preview(
                    preview,
                    state.settings.default_conflict_strategy.clone(),
                )
            });
        let can_apply = selection.can_apply(&summary);
        let source_is_backup = match preview {
            CloudSyncPendingPreview::Structured(_) => false,
            CloudSyncPendingPreview::Legacy { source, .. } => source.is_backup(),
        };
        let mut card = div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(CLOUD_SYNC_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-preview-title",
                        if source_is_backup {
                            "rollback"
                        } else {
                            "import"
                        },
                        self.i18n.t(if source_is_backup {
                            "plugin.cloud_sync.sections.rollback_preview"
                        } else {
                            "plugin.cloud_sync.sections.import_preview"
                        }),
                        theme.text_heading,
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(CLOUD_SYNC_GRID_GAP))
                    .child(self.render_cloud_sync_fact(
                        "plugin.cloud_sync.preview.connection_count",
                        summary.connections.to_string(),
                        cx,
                    ))
                    .child(self.render_cloud_sync_fact(
                        "plugin.cloud_sync.preview.total_forwards",
                        summary.forwards.to_string(),
                        cx,
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(CLOUD_SYNC_GRID_GAP))
                    .child(self.render_cloud_sync_fact(
                        "plugin.cloud_sync.preview.plugin_settings_label",
                        summary.plugin_settings_count.to_string(),
                        cx,
                    ))
                    .child(self.render_cloud_sync_fact(
                        "plugin.cloud_sync.preview.embedded_keys_label",
                        if summary.has_embedded_keys {
                            self.i18n.t("plugin.cloud_sync.common.yes")
                        } else {
                            self.i18n.t("plugin.cloud_sync.common.no")
                        },
                        cx,
                    )),
            );
        if !source_is_backup && state.local_dirty {
            card = card.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.accent))
                    .child(
                        self.i18n
                            .t("plugin.cloud_sync.preview.local_changes_warning"),
                    ),
            );
        }
        card = card.child(self.render_cloud_sync_preview_selection(&summary, &selection, cx));
        if !summary.forward_details.is_empty() {
            card = card.child(self.render_cloud_sync_forward_details(&summary.forward_details, cx));
        }
        for (action, records) in summary.grouped_records() {
            if !records.is_empty() {
                card = card
                    .child(self.render_cloud_sync_record_group(action, &records, &selection, cx));
            }
        }
        card.child(
            div()
                .w_full()
                .grid()
                .grid_cols(2)
                .gap(px(CLOUD_SYNC_GRID_GAP))
                .child(self.render_cloud_sync_action_button(
                    if source_is_backup {
                        "plugin.cloud_sync.actions.restore_selected_backup"
                    } else {
                        "plugin.cloud_sync.actions.import_preview"
                    },
                    ButtonVariant::Default,
                    busy || !can_apply,
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
                ))
                .child(self.render_cloud_sync_action_button(
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
                )),
        )
        .into_any_element()
    }

    fn render_cloud_sync_preview_selection(
        &self,
        summary: &CloudSyncPreviewSummary,
        selection: &CloudSyncPreviewSelection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut block = div().flex().flex_col().gap(px(8.0));
        if summary.connections > 0 {
            block = block.child(self.render_cloud_sync_check_row(
                self.i18n_replace(
                    "plugin.cloud_sync.preview.toggle_connections",
                    &[("count", summary.connections.to_string())],
                ),
                None,
                selection.import_connections,
                false,
                cx.listener(
                    |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                        let all_connection_names = this
                            .cloud_sync_pending_preview
                            .as_ref()
                            .map(cloud_sync_preview_summary)
                            .map(|summary| summary.connection_record_names())
                            .unwrap_or_default();
                        if let Some(selection) = this.cloud_sync_preview_selection.as_mut() {
                            selection.import_connections = !selection.import_connections;
                            if selection.import_connections
                                && selection.selected_connection_names.is_empty()
                            {
                                selection.selected_connection_names = all_connection_names;
                            }
                        }
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ));
        }
        if summary.has_app_settings {
            block = block.child(self.render_cloud_sync_check_row(
                self.i18n.t("plugin.cloud_sync.preview.toggle_app_settings"),
                None,
                selection.import_app_settings,
                false,
                cx.listener(
                    |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                        if let Some(selection) = this.cloud_sync_preview_selection.as_mut() {
                            selection.import_app_settings = !selection.import_app_settings;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ));
            for section in &summary.app_settings_sections {
                let section_id = section.id.clone();
                block = block.child(
                    self.render_cloud_sync_check_row(
                        cloud_sync_app_settings_section_label(&self.i18n, &section.id),
                        Some(self.i18n_replace(
                            "plugin.cloud_sync.preview.section_field_count",
                            &[("count", section.field_count.to_string())],
                        )),
                        selection.import_app_settings
                            && selection
                                .selected_app_settings_sections
                                .contains(&section.id),
                        !selection.import_app_settings,
                        cx.listener(
                            move |this: &mut WorkspaceApp,
                                  _event,
                                  _window,
                                  cx: &mut Context<WorkspaceApp>| {
                                if let Some(selection) = this.cloud_sync_preview_selection.as_mut()
                                {
                                    if !selection.selected_app_settings_sections.remove(&section_id)
                                    {
                                        selection
                                            .selected_app_settings_sections
                                            .insert(section_id.clone());
                                    }
                                }
                                cx.stop_propagation();
                                cx.notify();
                            },
                        ),
                        cx,
                    ),
                );
            }
        }
        if summary.plugin_settings_count > 0 {
            block = block.child(self.render_cloud_sync_check_row(
                self.i18n_replace(
                    "plugin.cloud_sync.preview.toggle_plugin_settings",
                    &[("count", summary.plugin_settings_count.to_string())],
                ),
                None,
                selection.import_plugin_settings,
                false,
                cx.listener(
                    |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                        if let Some(selection) = this.cloud_sync_preview_selection.as_mut() {
                            selection.import_plugin_settings = !selection.import_plugin_settings;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ));
            for (plugin_id, count) in &summary.plugin_settings_by_plugin {
                let plugin_id_for_toggle = plugin_id.clone();
                block = block.child(self.render_cloud_sync_check_row(
                    plugin_id.clone(),
                    Some(self.i18n_replace(
                        "plugin.cloud_sync.preview.plugin_settings",
                        &[("count", count.to_string())],
                    )),
                    selection.import_plugin_settings
                        && selection.selected_plugin_ids.contains(plugin_id),
                    !selection.import_plugin_settings,
                    cx.listener(
                        move |this: &mut WorkspaceApp,
                              _event,
                              _window,
                              cx: &mut Context<WorkspaceApp>| {
                            if let Some(selection) = this.cloud_sync_preview_selection.as_mut() {
                                if !selection.selected_plugin_ids.remove(&plugin_id_for_toggle) {
                                    selection
                                        .selected_plugin_ids
                                        .insert(plugin_id_for_toggle.clone());
                                }
                            }
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                    cx,
                ));
            }
        }
        if summary.forwards > 0 {
            block = block.child(self.render_cloud_sync_check_row(
                self.i18n_replace(
                    "plugin.cloud_sync.preview.toggle_forwards",
                    &[("count", summary.forwards.to_string())],
                ),
                None,
                selection.import_forwards,
                false,
                cx.listener(
                    |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                        if let Some(selection) = this.cloud_sync_preview_selection.as_mut() {
                            selection.import_forwards = !selection.import_forwards;
                        }
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ));
        }
        block.into_any_element()
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
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .when(!disabled, |row| {
                row.cursor_pointer()
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .child(
                div()
                    .size(px(16.0))
                    .rounded(px(999.0))
                    .border_1()
                    .border_color(if checked {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .when(checked, |mark| mark.bg(rgb(theme.accent)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(11.0))
                    .text_color(rgb(theme.bg))
                    .child(if checked { "✓" } else { "" }),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_color(if disabled {
                                rgb(theme.text_muted)
                            } else {
                                rgb(theme.text)
                            })
                            // Preview rows toggle on the row, matching Tauri checkbox row select-none labels.
                            .child(self.render_display_text_with_role(
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
                            )),
                    )
                    .when_some(meta, |row, meta| {
                        let meta_key = meta.clone();
                        row.child(div().text_color(rgb(theme.text_muted)).child(
                            self.render_display_text_with_role(
                                SelectableTextRole::NonSelectable,
                                "cloud-sync-preview-check-meta",
                                meta_key,
                                meta,
                                theme.text_muted,
                                cx,
                            ),
                        ))
                    }),
            )
            .into_any_element()
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
        for detail in details.iter().take(PREVIEW_RECORD_LIMIT) {
            block = block.child(self.render_cloud_sync_list_item(
                detail.description.clone(),
                Some(format!(
                    "{} · {}",
                    detail.owner_connection_name, detail.direction
                )),
                cx,
            ));
        }
        if details.len() > PREVIEW_RECORD_LIMIT {
            block =
                block.child(self.render_cloud_sync_list_more(details.len() - PREVIEW_RECORD_LIMIT));
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
        let mut block = self.render_cloud_sync_preview_block(
            self.i18n.t(match action {
                "import" => "plugin.cloud_sync.preview.will_import",
                "merge" => "plugin.cloud_sync.preview.will_merge",
                "replace" => "plugin.cloud_sync.preview.will_replace",
                "skip" => "plugin.cloud_sync.preview.will_skip",
                "rename" => "plugin.cloud_sync.preview.will_rename",
                _ => "plugin.cloud_sync.preview.records_header",
            }),
            cx,
        );
        for record in records.iter().take(PREVIEW_RECORD_LIMIT) {
            let meta = Some(self.format_cloud_sync_preview_record(record));
            if record.resource == "connection" {
                let name = record.name.clone();
                block = block.child(self.render_cloud_sync_check_row(
                    record.name.clone(),
                    meta,
                    selection.import_connections
                        && selection.selected_connection_names.contains(&record.name),
                    !selection.import_connections,
                    cx.listener(
                        move |this: &mut WorkspaceApp,
                              _event,
                              _window,
                              cx: &mut Context<WorkspaceApp>| {
                            if let Some(selection) = this.cloud_sync_preview_selection.as_mut() {
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
            } else {
                block =
                    block.child(self.render_cloud_sync_list_item(record.name.clone(), meta, cx));
            }
        }
        if records.len() > PREVIEW_RECORD_LIMIT {
            block =
                block.child(self.render_cloud_sync_list_more(records.len() - PREVIEW_RECORD_LIMIT));
        }
        block.into_any_element()
    }

    fn render_cloud_sync_preview_block(&self, title: String, cx: &mut Context<Self>) -> gpui::Div {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgba((theme.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
            .p(px(CLOUD_SYNC_STAT_PADDING))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.render_selectable_text_scoped(
                        "cloud-sync-preview-block-title",
                        title.clone(),
                        title,
                        theme.text_heading,
                        cx,
                    )),
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
        let mut title_el = div()
            .min_w(px(0.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(rgb(theme.text))
            .child(self.render_selectable_text_scoped(
                "cloud-sync-list-title",
                title.clone(),
                title,
                theme.text,
                cx,
            ));
        if mono {
            title_el =
                title_el.font_family(settings_mono_font_family(self.settings_store.settings()));
        }
        div()
            .w_full()
            .min_w(px(0.0))
            .py(px(4.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(title_el)
            .when_some(meta, |item, meta| {
                item.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(self.render_selectable_text_scoped(
                            "cloud-sync-list-meta",
                            meta.clone(),
                            meta,
                            theme.text_muted,
                            cx,
                        )),
                )
            })
            .into_any_element()
    }

    fn render_cloud_sync_list_more(&self, count: usize) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(self.i18n_replace(
                "plugin.cloud_sync.preview.more_items",
                &[("count", count.to_string())],
            ))
            .into_any_element()
    }

    fn format_cloud_sync_preview_record(&self, record: &CloudSyncPreviewRecord) -> String {
        let key = match record.action.as_str() {
            "import" => "plugin.cloud_sync.preview.record_import",
            "rename" => "plugin.cloud_sync.preview.record_rename",
            "skip" => "plugin.cloud_sync.preview.record_skip",
            "replace" => "plugin.cloud_sync.preview.record_replace",
            "merge" => "plugin.cloud_sync.preview.record_merge",
            _ => return record.reason_code.clone(),
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
        &self,
        state: &CloudSyncPersistedState,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut card =
            self.cloud_sync_card()
                .child(self.render_cloud_sync_section_title(
                    "plugin.cloud_sync.sections.rollback_backups",
                    cx,
                ));
        for backup in &state.rollback_backups {
            let id = backup.id.clone();
            let created_at = backup.created_at.clone();
            let summary = backup
                .metadata
                .as_ref()
                .map(|metadata| {
                    self.i18n_replace(
                        "plugin.cloud_sync.backup.summary_line",
                        &[
                            ("connections", metadata.num_connections.to_string()),
                            ("forwards", metadata.forwards.to_string()),
                            (
                                "pluginSettingsCount",
                                metadata.plugin_settings_count.to_string(),
                            ),
                            ("size", format_cloud_sync_bytes(backup.size_bytes)),
                        ],
                    )
                })
                .unwrap_or_else(|| format_cloud_sync_bytes(backup.size_bytes));
            card = card.child(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgba((self.tokens.ui.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
                    .p(px(CLOUD_SYNC_STAT_PADDING))
                    .flex()
                    .items_center()
                    .justify_between()
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
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "cloud-sync-rollback-backup",
                                        (id.as_str(), "created-at"),
                                        created_at.clone(),
                                        self.tokens.ui.text,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "cloud-sync-rollback-backup",
                                        (id.as_str(), "summary"),
                                        summary,
                                        self.tokens.ui.text_muted,
                                        cx,
                                    )),
                            ),
                    )
                    .child(self.render_cloud_sync_inline_button(
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
                    )),
            );
        }
        card.into_any_element()
    }

    fn render_cloud_sync_history(
        &self,
        state: &CloudSyncPersistedState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut card = div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(CLOUD_SYNC_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-history",
                        "title",
                        self.i18n.t("plugin.cloud_sync.sections.sync_history"),
                        theme.text_heading,
                        cx,
                    )),
            );
        if state.sync_history.is_empty() {
            card = card.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-history",
                        "empty",
                        self.i18n.t("plugin.cloud_sync.history_empty"),
                        theme.text_muted,
                        cx,
                    )),
            );
        } else {
            for entry in state.sync_history.iter().take(10) {
                card = card.child(self.render_cloud_sync_history_entry(entry, cx));
            }
        }
        card.into_any_element()
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
                (
                    "pluginSettingsCount",
                    entry.summary.plugin_settings_count.to_string(),
                ),
            ],
        );
        div()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | CLOUD_SYNC_LIST_BORDER_ALPHA))
            .bg(rgba((theme.bg << 8) | CLOUD_SYNC_LIST_BG_ALPHA))
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.render_selectable_text(
                        crate::workspace::selectable_text::selectable_text_id(
                            "cloud-sync-history-action",
                            (&entry.id, &entry.action),
                        ),
                        self.cloud_sync_history_action_label(&entry.action),
                        theme.text,
                        cx,
                    )),
            )
            .child(
                div()
                    .line_height(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_text(
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
                    )),
            )
            .when_some(entry.error.as_ref(), |item, error| {
                item.child(
                    div()
                        .line_height(px(18.0))
                        .text_color(rgb(theme.error))
                        .child(self.render_selectable_text(
                            crate::workspace::selectable_text::selectable_text_id(
                                "cloud-sync-history-error",
                                (&entry.id, error),
                            ),
                            self.format_cloud_sync_error(error),
                            theme.error,
                            cx,
                        )),
                )
            })
            .into_any_element()
    }

    fn cloud_sync_history_action_label(&self, action: &str) -> String {
        match action {
            "upload" => self.i18n.t("plugin.cloud_sync.history.action_upload"),
            "pull" => self.i18n.t("plugin.cloud_sync.history.action_pull"),
            "restore" => self.i18n.t("plugin.cloud_sync.history.action_restore"),
            _ => action.to_string(),
        }
    }

    fn render_cloud_sync_notes(
        &self,
        local_snapshot: Option<&CloudSyncLocalSnapshot>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(CLOUD_SYNC_CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-notes",
                        "title",
                        self.i18n.t("plugin.cloud_sync.sections.notes"),
                        theme.text_heading,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        self.i18n_replace(
                            "plugin.cloud_sync.native_scope_summary",
                            &[(
                                "sections",
                                local_snapshot
                                    .map(|snapshot| snapshot.scope.app_settings_sections.join(", "))
                                    .unwrap_or_default(),
                            )],
                        ),
                    ),
            )
            .into_any_element()
    }

    fn render_cloud_sync_config(&self, cx: &mut Context<Self>) -> AnyElement {
        let form = &self.cloud_sync_form;
        let backend = &form.backend_type;
        let auth_mode = &form.auth_mode;
        let show_auth_mode = backend_uses_auth_mode(backend);
        let show_token = backend_uses_token(backend, auth_mode);
        let show_git_token = backend_uses_git_token(backend);
        let show_basic = backend_uses_basic(backend, auth_mode);
        let show_s3 = backend_uses_s3_credentials(backend);
        let show_git = matches!(backend, BackendType::Git);
        let show_endpoint = !matches!(backend, BackendType::Dropbox);
        let namespace_label = if matches!(backend, BackendType::Dropbox | BackendType::Git) {
            "plugin.cloud_sync.settings.path_prefix"
        } else if matches!(backend, BackendType::S3) {
            "plugin.cloud_sync.settings.object_prefix"
        } else {
            "plugin.cloud_sync.settings.namespace"
        };
        let endpoint_placeholder = match backend {
            BackendType::S3 => "plugin.cloud_sync.placeholders.endpoint_s3",
            BackendType::Git => "plugin.cloud_sync.placeholders.endpoint_git",
            BackendType::HttpJson => "plugin.cloud_sync.placeholders.endpoint_http_json",
            BackendType::Dropbox => "plugin.cloud_sync.placeholders.endpoint_http_json",
            BackendType::Webdav => "plugin.cloud_sync.placeholders.endpoint_webdav",
        };
        let token_label = if matches!(backend, BackendType::Dropbox) {
            "plugin.cloud_sync.settings.access_token"
        } else {
            "plugin.cloud_sync.settings.token"
        };

        let mut card = self
            .cloud_sync_card()
            .child(self.render_cloud_sync_section_title(
                "plugin.cloud_sync.sections.connection_settings",
                cx,
            ))
            .child(self.render_cloud_sync_backend_select(cx));
        if show_auth_mode {
            card = card.child(self.render_cloud_sync_auth_mode_select(cx));
        }
        if show_endpoint {
            card = card.child(self.render_cloud_sync_text_field(
                "plugin.cloud_sync.settings.endpoint",
                SettingsInput::CloudSyncEndpoint,
                endpoint_placeholder,
                false,
                cx,
            ));
        }
        card = card.child(self.render_cloud_sync_text_field(
            namespace_label,
            SettingsInput::CloudSyncNamespace,
            "plugin.cloud_sync.placeholders.namespace",
            false,
            cx,
        ));
        if show_git {
            card = card
                .child(self.render_cloud_sync_text_field(
                    "plugin.cloud_sync.settings.git_repository",
                    SettingsInput::CloudSyncGitRepository,
                    "plugin.cloud_sync.placeholders.git_repository",
                    false,
                    cx,
                ))
                .child(self.render_cloud_sync_text_field(
                    "plugin.cloud_sync.settings.git_branch",
                    SettingsInput::CloudSyncGitBranch,
                    "plugin.cloud_sync.placeholders.git_branch",
                    false,
                    cx,
                ));
        }
        if show_s3 {
            card = card
                .child(self.render_cloud_sync_text_field(
                    "plugin.cloud_sync.settings.s3_bucket",
                    SettingsInput::CloudSyncS3Bucket,
                    "plugin.cloud_sync.placeholders.s3_bucket",
                    false,
                    cx,
                ))
                .child(self.render_cloud_sync_text_field(
                    "plugin.cloud_sync.settings.s3_region",
                    SettingsInput::CloudSyncS3Region,
                    "plugin.cloud_sync.placeholders.s3_region",
                    false,
                    cx,
                ));
        }
        if show_token {
            card = card.child(self.render_cloud_sync_secret_field(
                token_label,
                SettingsInput::CloudSyncToken,
                "plugin.cloud_sync.placeholders.token",
                secret_keys::TOKEN,
                cx,
            ));
        }
        if show_git_token {
            card = card.child(self.render_cloud_sync_secret_field(
                "plugin.cloud_sync.settings.git_access_token",
                SettingsInput::CloudSyncGitToken,
                "plugin.cloud_sync.placeholders.git_access_token",
                secret_keys::GIT_TOKEN,
                cx,
            ));
        }
        if show_basic {
            card = card
                .child(self.render_cloud_sync_secret_field(
                    "plugin.cloud_sync.settings.basic_username",
                    SettingsInput::CloudSyncBasicUsername,
                    "plugin.cloud_sync.placeholders.username",
                    secret_keys::BASIC_USERNAME,
                    cx,
                ))
                .child(self.render_cloud_sync_secret_field(
                    "plugin.cloud_sync.settings.basic_password",
                    SettingsInput::CloudSyncBasicPassword,
                    "plugin.cloud_sync.placeholders.password",
                    secret_keys::BASIC_PASSWORD,
                    cx,
                ));
        }
        if show_s3 {
            card = card
                .child(self.render_cloud_sync_secret_field(
                    "plugin.cloud_sync.settings.access_key_id",
                    SettingsInput::CloudSyncAccessKeyId,
                    "plugin.cloud_sync.placeholders.access_key_id",
                    secret_keys::ACCESS_KEY_ID,
                    cx,
                ))
                .child(self.render_cloud_sync_secret_field(
                    "plugin.cloud_sync.settings.secret_access_key",
                    SettingsInput::CloudSyncSecretAccessKey,
                    "plugin.cloud_sync.placeholders.secret_access_key",
                    secret_keys::SECRET_ACCESS_KEY,
                    cx,
                ))
                .child(self.render_cloud_sync_secret_field(
                    "plugin.cloud_sync.settings.session_token",
                    SettingsInput::CloudSyncSessionToken,
                    "plugin.cloud_sync.placeholders.session_token",
                    secret_keys::SESSION_TOKEN,
                    cx,
                ));
        }
        card.child(self.render_cloud_sync_secret_field(
            "plugin.cloud_sync.settings.sync_password",
            SettingsInput::CloudSyncSyncPassword,
            "plugin.cloud_sync.placeholders.sync_password",
            secret_keys::SYNC_PASSWORD,
            cx,
        ))
        .child(self.render_cloud_sync_toggle(
            "plugin.cloud_sync.settings.auto_upload_enabled",
            form.auto_upload_enabled,
            cx.listener(
                |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.cloud_sync_form.auto_upload_enabled =
                        !this.cloud_sync_form.auto_upload_enabled;
                    this.clear_cloud_sync_select_focus();
                    cx.stop_propagation();
                    cx.notify();
                },
            ),
            cx,
        ))
        .child(self.render_cloud_sync_text_field(
            "plugin.cloud_sync.settings.auto_upload_interval",
            SettingsInput::CloudSyncAutoUploadInterval,
            "60",
            false,
            cx,
        ))
        .child(self.render_cloud_sync_conflict_select(cx))
        .into_any_element()
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
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
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
                    )),
            )
            .child(text_input_anchor_probe(
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
            ))
            .into_any_element()
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
        let mut row = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .gap(px(8.0))
            .items_end()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.render_cloud_sync_text_field(
                        label_key,
                        input,
                        placeholder,
                        true,
                        cx,
                    )),
            );
        if stored {
            let label = self.i18n.t(label_key);
            row = row.child(self.render_cloud_sync_inline_button(
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
                        this.cloud_sync_confirm_focused_action = Some(ConfirmDialogAction::Cancel);
                        this.clear_cloud_sync_select_focus();
                        cx.stop_propagation();
                        cx.notify();
                    },
                ),
                cx,
            ));
        }
        row.into_any_element()
    }

    fn render_cloud_sync_backend_select(&self, cx: &mut Context<Self>) -> AnyElement {
        self.render_cloud_sync_select_field(
            "plugin.cloud_sync.settings.backend_type",
            CloudSyncSelect::Backend,
            self.cloud_sync_backend_label(&CloudSyncSettings {
                backend_type: self.cloud_sync_form.backend_type.clone(),
                ..CloudSyncSettings::default()
            }),
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
        match select {
            CloudSyncSelect::Backend => [
                (BackendType::Webdav, "plugin.cloud_sync.backend.webdav"),
                (BackendType::HttpJson, "plugin.cloud_sync.backend.http_json"),
                (BackendType::Dropbox, "plugin.cloud_sync.backend.dropbox"),
                (BackendType::Git, "plugin.cloud_sync.backend.git"),
                (BackendType::S3, "plugin.cloud_sync.backend.s3"),
            ]
            .into_iter()
            .map(|(backend, label_key)| CloudSyncSelectOption {
                label: self.i18n.t(label_key),
                selected: self.cloud_sync_form.backend_type == backend,
                action: CloudSyncSelectAction::Backend(backend),
            })
            .collect(),
            CloudSyncSelect::AuthMode => [
                (AuthMode::Bearer, "plugin.cloud_sync.auth.bearer"),
                (AuthMode::Basic, "plugin.cloud_sync.auth.basic"),
                (AuthMode::None, "plugin.cloud_sync.auth.none"),
            ]
            .into_iter()
            .map(|(auth_mode, label_key)| CloudSyncSelectOption {
                label: self.i18n.t(label_key),
                selected: self.cloud_sync_form.auth_mode == auth_mode,
                action: CloudSyncSelectAction::AuthMode(auth_mode),
            })
            .collect(),
            CloudSyncSelect::ConflictStrategy => [
                (ConflictStrategy::Merge, "plugin.cloud_sync.conflict.merge"),
                (
                    ConflictStrategy::Replace,
                    "plugin.cloud_sync.conflict.replace",
                ),
                (ConflictStrategy::Skip, "plugin.cloud_sync.conflict.skip"),
                (
                    ConflictStrategy::Rename,
                    "plugin.cloud_sync.conflict.rename",
                ),
            ]
            .into_iter()
            .map(|(strategy, label_key)| CloudSyncSelectOption {
                label: self.i18n.t(label_key),
                selected: self.cloud_sync_form.default_conflict_strategy == strategy,
                action: CloudSyncSelectAction::ConflictStrategy(strategy),
            })
            .collect(),
        }
    }

    fn cloud_sync_selected_option_index(&self, select: CloudSyncSelect) -> usize {
        self.cloud_sync_select_options(select)
            .iter()
            .position(|option| option.selected)
            .unwrap_or(0)
    }

    fn cloud_sync_focusable_selects(&self) -> Vec<CloudSyncSelect> {
        let mut selects = vec![CloudSyncSelect::Backend];
        if backend_uses_auth_mode(&self.cloud_sync_form.backend_type) {
            selects.push(CloudSyncSelect::AuthMode);
        }
        selects.push(CloudSyncSelect::ConflictStrategy);
        selects
    }

    fn move_cloud_sync_select_focus(&mut self, select: CloudSyncSelect, forward: bool) {
        let selects = self.cloud_sync_focusable_selects();
        self.cloud_sync_focused_select = next_cloud_sync_select_focus(&selects, select, forward);
    }

    fn open_cloud_sync_select_for_keyboard(&mut self, select: CloudSyncSelect) {
        self.cloud_sync_focused_select = Some(select);
        self.cloud_sync_select_focus_origin = Some(browser_behavior::BrowserFocusOrigin::Keyboard);
        self.cloud_sync_open_select = Some(select);
        self.cloud_sync_select_highlighted =
            Some((select, self.cloud_sync_selected_option_index(select)));
    }

    fn clear_cloud_sync_select_focus(&mut self) {
        // Browser focus leaves a Radix Select trigger when the user activates a
        // sibling input/button. Keep popup and focus-ring ownership paired.
        self.cloud_sync_open_select = None;
        self.cloud_sync_focused_select = None;
        self.cloud_sync_select_focus_origin = None;
        self.cloud_sync_select_highlighted = None;
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

        if let Some(select) = self.cloud_sync_open_select {
            return self.handle_open_cloud_sync_select_key(select, event, cx);
        }

        let Some(select) = self.cloud_sync_focused_select else {
            return false;
        };

        match event.keystroke.key.as_str() {
            "escape" => {
                self.cloud_sync_focused_select = None;
                self.cloud_sync_select_focus_origin = None;
                cx.notify();
                true
            }
            "tab" => {
                // Radix returns focus to the trigger before browser focus moves
                // onward. Native keeps the owner explicit so the visual ring and
                // next-select order stay in sync without DOM tab stops.
                self.move_cloud_sync_select_focus(select, !event.keystroke.modifiers.shift);
                self.cloud_sync_select_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "enter" | "space" | " " | "arrowdown" | "down" => {
                self.open_cloud_sync_select_for_keyboard(select);
                cx.notify();
                true
            }
            _ => false,
        }
    }

    fn handle_open_cloud_sync_select_key(
        &mut self,
        select: CloudSyncSelect,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let options = self.cloud_sync_select_options(select);
        if options.is_empty() {
            return false;
        }
        let current = self
            .cloud_sync_select_highlighted
            .filter(|(highlighted_select, _)| *highlighted_select == select)
            .map(|(_, index)| index)
            .unwrap_or_else(|| self.cloud_sync_selected_option_index(select));

        match event.keystroke.key.as_str() {
            "escape" => {
                self.cloud_sync_open_select = None;
                self.cloud_sync_select_highlighted = None;
                self.cloud_sync_focused_select = Some(select);
                self.cloud_sync_select_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "tab" => {
                // Tauri/Radix Select closes before Tab leaves the trigger. Walk
                // the visible Cloud Sync select order here so hidden auth-mode
                // controls are never reachable by keyboard focus.
                self.cloud_sync_open_select = None;
                self.cloud_sync_select_highlighted = None;
                self.move_cloud_sync_select_focus(select, !event.keystroke.modifiers.shift);
                self.cloud_sync_select_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "arrowdown" | "down" => {
                self.cloud_sync_select_highlighted = Some((
                    select,
                    radix_select_next_index(current, options.len(), SelectKeyDirection::Next),
                ));
                cx.notify();
                true
            }
            "arrowup" | "up" => {
                self.cloud_sync_select_highlighted = Some((
                    select,
                    radix_select_next_index(current, options.len(), SelectKeyDirection::Previous),
                ));
                cx.notify();
                true
            }
            "home" => {
                self.cloud_sync_select_highlighted = Some((select, 0));
                cx.notify();
                true
            }
            "end" => {
                self.cloud_sync_select_highlighted = Some((select, options.len() - 1));
                cx.notify();
                true
            }
            "enter" | "space" | " " => {
                let action = options
                    .get(current.min(options.len() - 1))
                    .map(|option| option.action.clone());
                if let Some(action) = action {
                    self.cloud_sync_select_focus_origin =
                        Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                    self.apply_cloud_sync_select_action(action, cx);
                }
                true
            }
            _ => false,
        }
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
        let label = self.i18n.t(label_key);
        let mut group = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "cloud-sync-select-label",
                        label_key,
                        label,
                        theme.text_muted,
                        cx,
                    )),
            );
        let open = self.cloud_sync_open_select == Some(select);
        let focused = self.cloud_sync_focused_select == Some(select);
        let focus_visible = focused
            && self
                .cloud_sync_select_focus_origin
                .is_some_and(|origin| origin.is_focus_visible());
        group = group.child(select_trigger_focus_visible(
            &self.tokens,
            div()
                .w_full()
                .h(px(36.0))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(if open || focused {
                    rgb(theme.accent)
                } else {
                    rgb(theme.border)
                })
                .bg(rgba((theme.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
                .px(px(12.0))
                .flex()
                .items_center()
                .justify_between()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .text_color(rgb(theme.text))
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(
                        move |this: &mut WorkspaceApp,
                              _event,
                              _window,
                              cx: &mut Context<WorkspaceApp>| {
                            this.cloud_sync_focused_select = Some(select);
                            this.cloud_sync_select_focus_origin =
                                Some(browser_behavior::BrowserFocusOrigin::Pointer);
                            this.cloud_sync_open_select = if this.cloud_sync_open_select
                                == Some(select)
                            {
                                this.cloud_sync_select_highlighted = None;
                                None
                            } else {
                                this.cloud_sync_select_highlighted =
                                    Some((select, this.cloud_sync_selected_option_index(select)));
                                Some(select)
                            };
                            cx.stop_propagation();
                            cx.notify();
                        },
                    ),
                )
                // Select trigger/options are controls; labels stay outside read-only selection ownership.
                .child(self.render_display_text_with_role(
                    SelectableTextRole::NonSelectable,
                    "cloud-sync-select-value",
                    format!("{select:?}"),
                    value,
                    theme.text,
                    cx,
                ))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.text_muted))
                        .child("⌄"),
                ),
            focus_visible,
        ));
        if open {
            let highlighted = self
                .cloud_sync_select_highlighted
                .filter(|(highlighted_select, _)| *highlighted_select == select)
                .map(|(_, index)| index)
                .unwrap_or_else(|| self.cloud_sync_selected_option_index(select));
            let mut menu = div()
                .w_full()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.bg_panel))
                .overflow_hidden()
                // Cloud Sync selects sit inside a settings-like scroll view; a
                // wheel over the open menu should not scroll the page behind it.
                .on_scroll_wheel(|_, _, cx| cx.stop_propagation());
            for (index, option) in options.into_iter().enumerate() {
                let label = option.label.clone();
                let option_key = option.label.clone();
                let selected = option.selected;
                let action = option.action.clone();
                let option_highlighted = highlighted == index;
                menu = menu.child(
                    div()
                        .w_full()
                        .h(px(36.0))
                        .px(px(12.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(if selected {
                            rgb(theme.accent)
                        } else {
                            rgb(theme.text)
                        })
                        .bg(if option_highlighted {
                            rgba((theme.bg_hover << 8) | CLOUD_SYNC_SELECT_HIGHLIGHT_ALPHA)
                        } else if selected {
                            rgba((theme.accent << 8) | 0x1f)
                        } else {
                            rgba(0x00000000)
                        })
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(self.tokens.ui.bg_hover)))
                        .on_mouse_move(cx.listener(
                            move |this, _event: &MouseMoveEvent, _window, cx| {
                                if this.cloud_sync_select_highlighted != Some((select, index)) {
                                    this.cloud_sync_select_highlighted = Some((select, index));
                                    cx.notify();
                                }
                            },
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.cloud_sync_select_focus_origin =
                                    Some(browser_behavior::BrowserFocusOrigin::Pointer);
                                this.apply_cloud_sync_select_action(action.clone(), cx);
                                cx.stop_propagation();
                            }),
                        )
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::NonSelectable,
                            "cloud-sync-select-option",
                            option_key,
                            label,
                            if selected { theme.accent } else { theme.text },
                            cx,
                        ))
                        .when(selected, |row| row.child("✓")),
                );
            }
            group = group.child(menu);
        }
        group.into_any_element()
    }

    fn render_cloud_sync_toggle(
        &self,
        label_key: &str,
        checked: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .py(px(2.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(theme.text_muted))
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, listener)
            // Toggle labels are control text, so they match Tauri select-none behavior.
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-toggle-label",
                label_key,
                self.i18n.t(label_key),
                theme.text_muted,
                cx,
            ))
            .child(
                div()
                    .w(px(16.0))
                    .h(px(16.0))
                    .rounded(px(2.0))
                    .border_1()
                    .border_color(if checked {
                        rgb(theme.accent)
                    } else {
                        rgb(theme.border)
                    })
                    .bg(if checked {
                        rgb(theme.accent)
                    } else {
                        rgba(0x00000000)
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |box_el| {
                        box_el.child(
                            div()
                                .text_size(px(11.0))
                                .text_color(rgb(theme.bg))
                                .child("✓"),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_cloud_sync_inline_button(
        &self,
        label_key: &str,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .h(px(36.0))
            .px(px(12.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(theme.text_muted))
            .cursor_pointer()
            .hover(|style| {
                style
                    .bg(rgb(self.tokens.ui.bg_hover))
                    .text_color(rgb(self.tokens.ui.text))
            })
            .on_mouse_down(MouseButton::Left, listener)
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "cloud-sync-inline-button",
                label_key,
                self.i18n.t(label_key),
                theme.text_muted,
                cx,
            ))
            .into_any_element()
    }

    fn save_cloud_sync_configuration(&mut self, cx: &mut Context<Self>) {
        let interval = self
            .cloud_sync_form
            .auto_upload_interval_mins
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(60.0);
        let auth_mode = match self.cloud_sync_form.backend_type {
            BackendType::Dropbox => AuthMode::Bearer,
            BackendType::Git | BackendType::S3 => AuthMode::None,
            BackendType::Webdav | BackendType::HttpJson => self.cloud_sync_form.auth_mode.clone(),
        };
        let settings = CloudSyncSettings {
            backend_type: self.cloud_sync_form.backend_type.clone(),
            auth_mode,
            endpoint: self.cloud_sync_form.endpoint.trim().to_string(),
            namespace: if matches!(
                self.cloud_sync_form.backend_type,
                BackendType::Git | BackendType::S3
            ) {
                self.cloud_sync_form.namespace.trim().to_string()
            } else {
                let namespace = self.cloud_sync_form.namespace.trim();
                if namespace.is_empty() {
                    CloudSyncSettings::default().namespace
                } else {
                    namespace.to_string()
                }
            },
            s3_bucket: self.cloud_sync_form.s3_bucket.trim().to_string(),
            s3_region: {
                let region = self.cloud_sync_form.s3_region.trim();
                if region.is_empty() {
                    CloudSyncSettings::default().s3_region
                } else {
                    region.to_string()
                }
            },
            git_repository: self.cloud_sync_form.git_repository.trim().to_string(),
            git_branch: {
                let branch = self.cloud_sync_form.git_branch.trim();
                if branch.is_empty() {
                    CloudSyncSettings::default().git_branch
                } else {
                    branch.to_string()
                }
            },
            auto_upload_enabled: self.cloud_sync_form.auto_upload_enabled,
            auto_upload_interval_mins: interval,
            default_conflict_strategy: self.cloud_sync_form.default_conflict_strategy.clone(),
        };
        let mut provider = CloudSyncKeychainSecretProvider::new(
            self.cloud_sync_store.state().secret_hints.clone(),
        );
        let secret_result = self.store_cloud_sync_touched_secrets(&mut provider);
        self.cloud_sync_store.state_mut().settings = settings;
        self.cloud_sync_store.state_mut().secret_hints = provider.hints().clone();
        self.cloud_sync_form.auto_upload_interval_mins = cloud_sync_number_string(interval);
        self.reset_cloud_sync_secret_drafts();
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

    fn store_cloud_sync_touched_secrets(
        &mut self,
        provider: &mut CloudSyncKeychainSecretProvider,
    ) -> anyhow::Result<()> {
        let form = &self.cloud_sync_form;
        if form.token_touched {
            provider.store_secret(secret_keys::TOKEN, non_empty_secret(&form.token))?;
        }
        if form.git_token_touched {
            provider.store_secret(secret_keys::GIT_TOKEN, non_empty_secret(&form.git_token))?;
        }
        if form.basic_username_touched {
            provider.store_secret(
                secret_keys::BASIC_USERNAME,
                non_empty_secret(&form.basic_username),
            )?;
        }
        if form.basic_password_touched {
            provider.store_secret(
                secret_keys::BASIC_PASSWORD,
                non_empty_secret(&form.basic_password),
            )?;
        }
        if form.access_key_id_touched {
            provider.store_secret(
                secret_keys::ACCESS_KEY_ID,
                non_empty_secret(&form.access_key_id),
            )?;
        }
        if form.secret_access_key_touched {
            provider.store_secret(
                secret_keys::SECRET_ACCESS_KEY,
                non_empty_secret(&form.secret_access_key),
            )?;
        }
        if form.session_token_touched {
            provider.store_secret(
                secret_keys::SESSION_TOKEN,
                non_empty_secret(&form.session_token),
            )?;
        }
        if form.sync_password_touched {
            provider.store_secret(
                secret_keys::SYNC_PASSWORD,
                non_empty_secret(&form.sync_password),
            )?;
        }
        Ok(())
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

    fn reset_cloud_sync_secret_drafts(&mut self) {
        self.cloud_sync_form.token.clear();
        self.cloud_sync_form.git_token.clear();
        self.cloud_sync_form.basic_username.clear();
        self.cloud_sync_form.basic_password.clear();
        self.cloud_sync_form.access_key_id.clear();
        self.cloud_sync_form.secret_access_key.clear();
        self.cloud_sync_form.session_token.clear();
        self.cloud_sync_form.sync_password.clear();
        self.cloud_sync_form.token_touched = false;
        self.cloud_sync_form.git_token_touched = false;
        self.cloud_sync_form.basic_username_touched = false;
        self.cloud_sync_form.basic_password_touched = false;
        self.cloud_sync_form.access_key_id_touched = false;
        self.cloud_sync_form.secret_access_key_touched = false;
        self.cloud_sync_form.session_token_touched = false;
        self.cloud_sync_form.sync_password_touched = false;
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

    fn cloud_sync_usage_guide_title(&self, backend_type: &BackendType) -> String {
        match backend_type {
            BackendType::Webdav => self.i18n.t("plugin.cloud_sync.guide.webdav_title"),
            BackendType::HttpJson => self.i18n.t("plugin.cloud_sync.guide.http_json_title"),
            BackendType::Dropbox => self.i18n.t("plugin.cloud_sync.backend.dropbox"),
            BackendType::Git => self.i18n.t("plugin.cloud_sync.backend.git"),
            BackendType::S3 => self.i18n.t("plugin.cloud_sync.backend.s3"),
        }
    }

    fn cloud_sync_usage_guide_description(&self, backend_type: &BackendType) -> String {
        match backend_type {
            BackendType::Webdav => self.i18n.t("plugin.cloud_sync.guide.webdav_description"),
            BackendType::HttpJson => self.i18n.t("plugin.cloud_sync.guide.http_json_description"),
            BackendType::Dropbox => self.i18n.t("plugin.cloud_sync.notes.backend_dropbox"),
            BackendType::Git => self.i18n.t("plugin.cloud_sync.notes.backend_git"),
            BackendType::S3 => self.i18n.t("plugin.cloud_sync.notes.backend_s3"),
        }
    }

    fn cloud_sync_usage_guide_examples(&self, backend_type: &BackendType) -> Vec<(String, String)> {
        match backend_type {
            BackendType::Webdav => vec![
                (
                    self.i18n.t("plugin.cloud_sync.settings.endpoint"),
                    self.i18n
                        .t("plugin.cloud_sync.guide.webdav_example_endpoint"),
                ),
                (
                    self.i18n.t("plugin.cloud_sync.settings.namespace"),
                    self.i18n
                        .t("plugin.cloud_sync.guide.webdav_example_namespace"),
                ),
                (
                    self.i18n.t("plugin.cloud_sync.settings.basic_username"),
                    self.i18n
                        .t("plugin.cloud_sync.guide.webdav_example_username"),
                ),
                (
                    self.i18n.t("plugin.cloud_sync.settings.basic_password"),
                    self.i18n
                        .t("plugin.cloud_sync.guide.webdav_example_password"),
                ),
            ],
            BackendType::HttpJson => vec![
                (
                    self.i18n.t("plugin.cloud_sync.settings.endpoint"),
                    self.i18n
                        .t("plugin.cloud_sync.guide.http_json_example_endpoint"),
                ),
                (
                    self.i18n.t("plugin.cloud_sync.settings.namespace"),
                    self.i18n
                        .t("plugin.cloud_sync.guide.http_json_example_namespace"),
                ),
            ],
            BackendType::Dropbox | BackendType::Git | BackendType::S3 => Vec::new(),
        }
    }

    fn render_cloud_sync_fact(
        &self,
        label_key: &str,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key).to_uppercase();
        div()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg << 8) | CLOUD_SYNC_BG_MIX_ALPHA))
            .p(px(CLOUD_SYNC_STAT_PADDING))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-fact-label",
                        label_key,
                        label.clone(),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.tokens.ui.text))
                    .when(cloud_sync_value_prefers_mono(&value), |item| {
                        item.font_family(settings_mono_font_family(self.settings_store.settings()))
                    })
                    .child(self.render_selectable_text(
                        crate::workspace::selectable_text::selectable_text_id(
                            "cloud-sync-fact",
                            (&label, &value),
                        ),
                        value,
                        self.tokens.ui.text,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn open_cloud_sync_import_confirm(&mut self) {
        if self.cloud_sync_pending_preview.is_none() {
            return;
        }
        self.cloud_sync_confirm = Some(CloudSyncConfirm::ImportPreview);
        self.cloud_sync_confirm_focused_action = Some(ConfirmDialogAction::Cancel);
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
            self.cloud_sync_confirm_focused_action = Some(ConfirmDialogAction::Cancel);
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
            None => {}
        }
    }

    pub(super) fn handle_cloud_sync_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.cloud_sync_confirm.is_none()
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
        {
            return false;
        }

        let focused = self
            .cloud_sync_confirm_focused_action
            .unwrap_or(ConfirmDialogAction::Cancel);
        match event.keystroke.key.as_str() {
            "escape" => {
                self.cancel_cloud_sync_confirm();
                cx.notify();
                true
            }
            "tab" | "arrowleft" | "left" | "arrowright" | "right" => {
                // Tauri footer buttons are ordinary DOM buttons in a modal
                // focus loop. With two actions, Tab and arrow keys both expose
                // the same explicit native focus-visible target.
                self.cloud_sync_confirm_focused_action =
                    Some(next_confirm_dialog_footer_focus(Some(focused), true));
                cx.notify();
                true
            }
            "enter" | "space" | " " => {
                match focused {
                    ConfirmDialogAction::Cancel => self.cancel_cloud_sync_confirm(),
                    ConfirmDialogAction::Confirm => self.confirm_cloud_sync_confirm(cx),
                }
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn render_cloud_sync_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(confirm) = self.cloud_sync_confirm.clone() else {
            return div().into_any_element();
        };
        let (variant, title, description, confirm_label): (
            ConfirmDialogVariant,
            String,
            Option<String>,
            String,
        ) = match confirm {
            CloudSyncConfirm::ImportPreview => (
                ConfirmDialogVariant::Default,
                self.i18n.t("plugin.cloud_sync.confirm.import_title"),
                None,
                self.i18n.t("plugin.cloud_sync.actions.import_preview"),
            ),
            CloudSyncConfirm::ClearSecret { label, .. } => (
                ConfirmDialogVariant::Danger,
                self.i18n.t("plugin.cloud_sync.confirm.clear_secret_title"),
                Some(self.i18n_replace(
                    "plugin.cloud_sync.confirm.clear_secret_description",
                    &[("label", label)],
                )),
                self.i18n.t("plugin.cloud_sync.actions.clear_secret"),
            ),
            CloudSyncConfirm::RestoreBackup { created_at, .. } => (
                ConfirmDialogVariant::Default,
                self.i18n
                    .t("plugin.cloud_sync.confirm.restore_backup_title"),
                Some(self.i18n_replace(
                    "plugin.cloud_sync.confirm.restore_backup_description",
                    &[("createdAt", created_at)],
                )),
                self.i18n.t("plugin.cloud_sync.actions.restore_backup"),
            ),
        };
        confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant,
                title: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "cloud-sync-confirm",
                        "title",
                        title,
                        self.tokens.ui.text_heading,
                        cx,
                    ))
                    .into_any_element(),
                description: description.map(|text| {
                    div()
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "cloud-sync-confirm",
                            "description",
                            text,
                            self.tokens.ui.text_muted,
                            cx,
                        ))
                        .into_any_element()
                }),
                cancel_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "cloud-sync-confirm",
                        "cancel",
                        self.i18n.t("common.cancel"),
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "cloud-sync-confirm",
                        "confirm",
                        confirm_label,
                        self.tokens.ui.text,
                        cx,
                    ))
                    .into_any_element(),
            },
            self.cloud_sync_confirm_focused_action,
            cx.listener(
                |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.cancel_cloud_sync_confirm();
                    cx.stop_propagation();
                    cx.notify();
                },
            ),
            cx.listener(
                |this: &mut WorkspaceApp, _event, _window, cx: &mut Context<WorkspaceApp>| {
                    this.confirm_cloud_sync_confirm(cx);
                    cx.stop_propagation();
                    cx.notify();
                },
            ),
        )
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
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let mut progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            let result = service
                .check_remote(
                    &settings,
                    &mut provider,
                    skip_if_busy,
                    false,
                    Some(&mut progress),
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(CloudSyncDelivery::CheckFinished(CloudSyncActionResult {
                result,
                secret_hints: provider.hints().clone(),
            }));
        });
    }

    fn start_cloud_sync_upload(&mut self, force: bool, cx: &mut Context<Self>) {
        self.start_cloud_sync_upload_with_options(force, false, false, cx);
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
        let raw_sync_scope = self.cloud_sync_store.state().sync_scope.clone();
        let connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let settings_store = self.settings_store.clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.cloud_sync_active_action = Some("upload");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let mut progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            let (result, remote_metadata, revision_sequence_consumed) = match service
                .upload_now(
                    &connection_store,
                    &forwarding_registry,
                    &settings_store,
                    &settings,
                    &mut provider,
                    UploadOptions {
                        force,
                        device_id,
                        revision_sequence,
                        previous_remote_revision,
                        previous_remote_sections,
                        last_synced_structured_state,
                        raw_sync_scope: Some(raw_sync_scope),
                        automatic,
                        skip_if_busy,
                        ..UploadOptions::default()
                    },
                    Some(&mut progress),
                )
                .await
            {
                Ok(Some(outcome)) => (Ok(outcome), None, None),
                Ok(None) => return,
                Err(error) => (
                    Err(error.to_string()),
                    error.remote_metadata,
                    error.revision_sequence_consumed,
                ),
            };
            let _ = tx.send(CloudSyncDelivery::UploadFinished {
                action: CloudSyncUploadActionResult {
                    result,
                    remote_metadata,
                    revision_sequence_consumed,
                    secret_hints: provider.hints().clone(),
                },
                automatic,
            });
        });
    }

    fn start_cloud_sync_pull_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            self.mark_cloud_sync_operation_in_progress();
            return;
        }
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync_store.state_mut().last_error = None;
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_preview_selection = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let connection_store = self.connection_store.clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.cloud_sync_active_action = Some("pull");
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let mut progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            let result = match service
                .pull_structured_preview(
                    &connection_store,
                    &settings,
                    &mut provider,
                    Some(&mut progress),
                )
                .await
            {
                Ok(Some(preview)) => Ok(CloudSyncPendingPreview::Structured(preview)),
                Ok(None) => service
                    .pull_legacy_preview(
                        &connection_store,
                        &settings,
                        &mut provider,
                        settings.default_conflict_strategy.clone(),
                        Some(&mut progress),
                    )
                    .await
                    .map(|preview| CloudSyncPendingPreview::Legacy {
                        preview,
                        source: CloudSyncPreviewSource::Remote,
                    }),
                Err(error) => Err(error),
            }
            .map_err(|error| error.to_string());
            let _ = tx.send(CloudSyncDelivery::PullPreviewFinished(
                CloudSyncActionResult {
                    result,
                    secret_hints: provider.hints().clone(),
                },
            ));
        });
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
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let mut progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            progress(CloudSyncProgress {
                stage: CloudSyncProgressStage::Validating,
                current: 1.0,
                total: 2.0,
                message: None,
            });
            let result =
                match get_action_secrets(&settings, &mut provider, true, SecretReadMode::Prompt) {
                    Ok(secrets) => {
                        let password = secrets.sync_password.unwrap_or_default();
                        if non_empty_secret(&password).is_none() {
                            Err("missing_sync_password: cloud sync password is required"
                                .to_string())
                        } else {
                            preview_cloud_sync_rollback_backup(
                                &connection_store,
                                backup.clone(),
                                &password,
                                Some(&mut progress),
                            )
                            .map(|preview| CloudSyncPendingPreview::Legacy {
                                preview,
                                source: CloudSyncPreviewSource::Backup {
                                    id: backup.id,
                                    created_at: backup.created_at,
                                },
                            })
                            .map_err(|error| error.to_string())
                        }
                    }
                    Err(error) => Err(error.to_string()),
                };
            progress(CloudSyncProgress {
                stage: CloudSyncProgressStage::Done,
                current: 2.0,
                total: 2.0,
                message: None,
            });
            let _ = tx.send(CloudSyncDelivery::RestoreBackupPreviewFinished(
                CloudSyncActionResult {
                    result,
                    secret_hints: provider.hints().clone(),
                },
            ));
        });
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
        let create_rollback_backup = self.cloud_sync_store.state().local_dirty
            && matches!(
                &preview,
                CloudSyncPendingPreview::Structured(_)
                    | CloudSyncPendingPreview::Legacy {
                        source: CloudSyncPreviewSource::Remote,
                        ..
                    }
            );
        let apply_total_units =
            cloud_sync_apply_total_units(&preview, &selection, create_rollback_backup);
        self.cloud_sync_store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let mut connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let mut settings_store = self.settings_store.clone();
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
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            let apply_requires_password = match &preview {
                CloudSyncPendingPreview::Structured(preview) => {
                    !preview.app_settings_entries.is_empty()
                        || !preview.plugin_settings_entries.is_empty()
                }
                CloudSyncPendingPreview::Legacy { .. } => true,
            };
            let needs_sync_password = apply_requires_password || create_rollback_backup;
            let sync_password = if needs_sync_password {
                match get_action_secrets(&settings, &mut provider, true, SecretReadMode::Prompt) {
                    Ok(secrets) => {
                        let password = secrets.sync_password.unwrap_or_default();
                        if non_empty_secret(&password).is_some() {
                            Some(password)
                        } else {
                            let _ = tx.send(CloudSyncDelivery::ApplyPreviewFinished(
                                CloudSyncActionResult {
                                    result: Err(
                                        "missing_sync_password: cloud sync password is required"
                                            .to_string(),
                                    ),
                                    secret_hints: provider.hints().clone(),
                                },
                            ));
                            return;
                        }
                    }
                    Err(error) => {
                        let _ = tx.send(CloudSyncDelivery::ApplyPreviewFinished(
                            CloudSyncActionResult {
                                result: Err(error.to_string()),
                                secret_hints: provider.hints().clone(),
                            },
                        ));
                        return;
                    }
                }
            } else {
                match get_action_secrets(&settings, &mut provider, false, SecretReadMode::Prompt) {
                    Ok(_) => None,
                    Err(error) => {
                        let _ = tx.send(CloudSyncDelivery::ApplyPreviewFinished(
                            CloudSyncActionResult {
                                result: Err(error.to_string()),
                                secret_hints: provider.hints().clone(),
                            },
                        ));
                        return;
                    }
                }
            };
            if create_rollback_backup {
                progress(CloudSyncProgress {
                    stage: CloudSyncProgressStage::CreatingBackup,
                    current: 0.1,
                    total: apply_total_units,
                    message: None,
                });
                match create_cloud_sync_rollback_backup(
                    &connection_store,
                    &forwarding_registry,
                    &settings_store,
                    &settings,
                    source_revision.clone(),
                    sync_password.as_deref(),
                ) {
                    Ok(Some(backup)) => {
                        let _ = tx.send(CloudSyncDelivery::RollbackBackupCreated(backup));
                    }
                    Ok(None) => {}
                    Err(error) => {
                        let _ = tx.send(CloudSyncDelivery::ApplyPreviewFinished(
                            CloudSyncActionResult {
                                result: Err(error.to_string()),
                                secret_hints: provider.hints().clone(),
                            },
                        ));
                        return;
                    }
                }
                progress(CloudSyncProgress {
                    stage: CloudSyncProgressStage::CreatingBackup,
                    current: 1.0,
                    total: apply_total_units,
                    message: None,
                });
            }
            let mut apply_progress = |update: CloudSyncProgress| {
                let offset = if create_rollback_backup { 1.0 } else { 0.0 };
                progress(CloudSyncProgress {
                    stage: update.stage,
                    current: (offset + update.current).min(apply_total_units),
                    total: apply_total_units,
                    message: update.message,
                });
            };
            let result = match preview {
                CloudSyncPendingPreview::Structured(preview) => service
                    .apply_structured_preview(
                        &mut connection_store,
                        &forwarding_registry,
                        &mut settings_store,
                        &settings,
                        preview,
                        selection.structured_selection(),
                        selection.conflict_strategy.clone(),
                        sync_password.as_deref(),
                        Some(&mut apply_progress),
                    )
                    .map(|outcome| {
                        CloudSyncApplyOutcome::Structured(
                            outcome.expect("cloud sync structured apply unexpectedly skipped"),
                        )
                    }),
                CloudSyncPendingPreview::Legacy { preview, source } => {
                    let summary = cloud_sync_preview_summary(&CloudSyncPendingPreview::Legacy {
                        preview: preview.clone(),
                        source: source.clone(),
                    });
                    service
                        .apply_legacy_preview(
                            &mut connection_store,
                            &settings,
                            &preview,
                            sync_password.as_deref(),
                            selection.effective_import_connections(&summary),
                            selection.selected_connection_names_for_import(&summary),
                            selection.import_forwards,
                            selection.conflict_strategy.clone(),
                            Some(&mut apply_progress),
                        )
                        .map(|outcome| CloudSyncApplyOutcome::Legacy {
                            preview,
                            source,
                            selection: selection.clone(),
                            outcome: outcome.expect("cloud sync legacy apply unexpectedly skipped"),
                        })
                }
            }
            .map(|outcome| CloudSyncApplyUiOutcome {
                connection_store,
                settings_store,
                outcome,
            })
            .map_err(|error| error.to_string());
            let _ = tx.send(CloudSyncDelivery::ApplyPreviewFinished(
                CloudSyncActionResult {
                    result,
                    secret_hints: provider.hints().clone(),
                },
            ));
        });
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
            if self.cloud_sync_upload_after_current {
                self.cloud_sync_upload_after_current = false;
                self.start_cloud_sync_upload_with_options(false, true, true, cx);
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
        let previous_remote_revision = self
            .cloud_sync_store
            .state()
            .last_known_remote_revision
            .clone();
        let previous_remote_sections = self
            .cloud_sync_store
            .state()
            .last_synced_remote_sections
            .clone();
        if let Some(metadata) = metadata {
            let remote_updated = metadata.revision.as_ref().is_some_and(|revision| {
                previous_remote_revision
                    .as_ref()
                    .map_or(true, |previous| previous != revision)
            });
            persist_remote_metadata(self.cloud_sync_store.state_mut(), &metadata);
            let dirty = build_local_snapshot(
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
            .map(|snapshot| snapshot.dirty);
            if let Some(dirty) = dirty.as_ref() {
                self.cloud_sync_store.state_mut().local_dirty = dirty.has_dirty;
                self.cloud_sync_store.state_mut().local_dirty_sections =
                    Some(dirty.dirty_sections.clone());
            }
            let conflict = dirty.as_ref().is_some_and(|dirty| {
                if !dirty.has_dirty || !metadata.exists {
                    return false;
                }
                if metadata.format.as_deref() != Some(STRUCTURED_MANIFEST_FORMAT) {
                    return remote_updated;
                }
                has_cloud_sync_structured_conflict(
                    &dirty.dirty_sections,
                    self.cloud_sync_store
                        .state()
                        .remote_section_revisions
                        .as_ref(),
                    previous_remote_sections.as_ref(),
                )
            });
            let status = if conflict {
                CloudSyncStatus::Conflict
            } else if remote_updated {
                CloudSyncStatus::RemoteUpdate
            } else {
                CloudSyncStatus::Idle
            };
            let conflict_details = conflict.then(|| CloudSyncConflictDetails {
                revision: self
                    .cloud_sync_store
                    .state()
                    .last_known_remote_revision
                    .clone(),
                device_id: self.cloud_sync_store.state().remote_device_id.clone(),
                updated_at: self.cloud_sync_store.state().remote_updated_at.clone(),
            });
            let conflict_error = conflict.then(|| {
                self.i18n_replace(
                    "plugin.cloud_sync.errors.remote_update_conflict_hint",
                    &[(
                        "revision",
                        self.cloud_sync_store
                            .state()
                            .last_known_remote_revision
                            .clone()
                            .unwrap_or_else(|| "—".to_string()),
                    )],
                )
            });
            self.cloud_sync_store.state_mut().status = status;
            self.cloud_sync_store.state_mut().conflict_details = conflict_details;
            self.cloud_sync_store
                .state_mut()
                .auto_upload_blocked_by_conflict = conflict;
            self.cloud_sync_store.state_mut().last_error = conflict_error;
        } else {
            self.cloud_sync_store.state_mut().status = CloudSyncStatus::Idle;
            self.cloud_sync_store.state_mut().last_error = None;
        }
        self.cloud_sync_store.state_mut().last_check_at = Some(now);
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_upload(&mut self, outcome: UploadOutcome, automatic: bool) {
        let remote_sections = build_manifest_section_revisions(&outcome.manifest);
        let revision = outcome.manifest.revision.clone();
        let uploaded_at = outcome.manifest.uploaded_at.clone();
        let history_summary = history_summary_from_manifest(&outcome.manifest);
        {
            let state = self.cloud_sync_store.state_mut();
            state.status = CloudSyncStatus::Idle;
            state.last_error = None;
            state.revision_seq = state.revision_seq.max(outcome.revision_sequence);
            state.last_sync_at = Some(uploaded_at.clone());
            state.last_upload_at = Some(uploaded_at);
            state.last_known_remote_revision = Some(revision.clone());
            state.last_known_remote_etag = outcome.etag.clone();
            state.remote_format = Some(outcome.manifest.format.clone());
            state.remote_section_revisions = Some(remote_sections.clone());
            state.remote_updated_at = Some(outcome.manifest.uploaded_at.clone());
            state.remote_device_id = Some(outcome.manifest.device_id.clone());
            state.remote_exists = true;
            state.last_synced_local_metadata = Some(outcome.local_snapshot.metadata.clone());
            state.last_synced_structured_state =
                Some(outcome.local_snapshot.dirty.current_state.clone());
            state.last_synced_remote_sections = Some(remote_sections);
            state.local_dirty = false;
            state.local_dirty_sections = Some(outcome.local_snapshot.dirty.dirty_sections.clone());
            state.auto_upload_blocked_by_conflict = false;
            state.conflict_details = None;
            state.append_history(CloudSyncHistoryEntry::new(
                "upload",
                history_summary,
                true,
                None,
                Some(revision.clone()),
            ));
        }
        self.cloud_sync_progress = None;
        self.cloud_sync_pending_preview = None;
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
        let remote_revision = self
            .cloud_sync_store
            .state()
            .last_known_remote_revision
            .clone();
        let is_upload_conflict = error
            .trim_start()
            .starts_with("remote_changed_before_upload");
        let history_summary = self.cloud_sync_upload_failure_summary();
        {
            let state = self.cloud_sync_store.state_mut();
            state.status = CloudSyncStatus::Error;
            state.last_error = Some(display_error.clone());
            if is_upload_conflict {
                state.auto_upload_blocked_by_conflict = true;
                state.conflict_details = Some(CloudSyncConflictDetails {
                    revision: state.last_known_remote_revision.clone(),
                    device_id: state.remote_device_id.clone(),
                    updated_at: state.remote_updated_at.clone(),
                });
            }
            state.append_history(CloudSyncHistoryEntry::new(
                "upload",
                history_summary,
                false,
                Some(display_error),
                remote_revision,
            ));
        }
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_pull_preview(&mut self, preview: CloudSyncPendingPreview) {
        match &preview {
            CloudSyncPendingPreview::Structured(preview) => {
                persist_remote_metadata(
                    self.cloud_sync_store.state_mut(),
                    &preview.remote_metadata,
                );
                self.cloud_sync_store.state_mut().status = CloudSyncStatus::Idle;
            }
            CloudSyncPendingPreview::Legacy {
                preview,
                source: CloudSyncPreviewSource::Remote,
            } => {
                persist_remote_metadata(
                    self.cloud_sync_store.state_mut(),
                    &preview.remote_metadata,
                );
                self.cloud_sync_store.state_mut().status = CloudSyncStatus::Idle;
            }
            CloudSyncPendingPreview::Legacy {
                source: CloudSyncPreviewSource::Backup { .. },
                ..
            } => {
                self.cloud_sync_store.state_mut().status = CloudSyncStatus::Idle;
            }
        }
        self.cloud_sync_store.state_mut().last_error = None;
        self.cloud_sync_preview_selection = Some(CloudSyncPreviewSelection::from_preview(
            &preview,
            self.cloud_sync_store
                .state()
                .settings
                .default_conflict_strategy
                .clone(),
        ));
        self.cloud_sync_pending_preview = Some(preview);
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
        let now = Utc::now().to_rfc3339();
        let remote_sections = build_manifest_section_revisions(&outcome.manifest);
        let previous_local_baseline = self
            .cloud_sync_store
            .state()
            .last_synced_structured_state
            .clone();
        let previous_remote_baseline = self
            .cloud_sync_store
            .state()
            .last_synced_remote_sections
            .clone();
        let was_conflict_blocked = self
            .cloud_sync_store
            .state()
            .auto_upload_blocked_by_conflict;
        let applied_full_remote =
            structured_apply_covers_full_remote(&outcome.manifest, &outcome.selection);
        let should_trigger_upload_after = was_conflict_blocked && !applied_full_remote;
        let local_snapshot = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            previous_local_baseline.as_ref(),
            Some(&self.cloud_sync_store.state().sync_scope),
        )
        .unwrap_or_else(|_| outcome.local_snapshot.clone());
        let next_local_baseline = merge_structured_baseline(
            previous_local_baseline.as_ref(),
            &local_snapshot.dirty.current_state,
            &outcome.selection,
        );
        let next_remote_baseline = merge_structured_remote_baseline(
            previous_remote_baseline.as_ref(),
            &remote_sections,
            &outcome.selection,
        );
        let dirty_after = compute_structured_dirty_sections(
            &local_snapshot.metadata,
            Some(&next_local_baseline),
            &local_snapshot.scope,
        );
        let next_known_revision = if applied_full_remote {
            Some(outcome.manifest.revision.clone())
        } else {
            self.cloud_sync_store
                .state()
                .last_known_remote_revision
                .clone()
        };
        let next_known_etag = if applied_full_remote {
            outcome.remote_metadata.etag.clone()
        } else {
            self.cloud_sync_store.state().last_known_remote_etag.clone()
        };
        {
            let state = self.cloud_sync_store.state_mut();
            state.status = CloudSyncStatus::Idle;
            state.last_error = None;
            state.last_sync_at = Some(now);
            state.last_known_remote_revision = next_known_revision;
            state.last_known_remote_etag = next_known_etag;
            state.remote_format = Some(outcome.manifest.format.clone());
            state.remote_section_revisions = Some(remote_sections.clone());
            state.remote_updated_at = Some(outcome.manifest.uploaded_at.clone());
            state.remote_device_id = Some(outcome.manifest.device_id.clone());
            state.remote_exists = true;
            state.last_synced_local_metadata = Some(local_snapshot.metadata.clone());
            state.last_synced_structured_state = Some(next_local_baseline);
            state.last_synced_remote_sections = Some(next_remote_baseline);
            state.local_dirty = dirty_after.has_dirty;
            state.local_dirty_sections = Some(dirty_after.dirty_sections.clone());
            state.auto_upload_blocked_by_conflict =
                dirty_after.has_dirty && !applied_full_remote && was_conflict_blocked;
            if !state.auto_upload_blocked_by_conflict {
                state.conflict_details = None;
            }
            state.append_history(CloudSyncHistoryEntry::new(
                "pull",
                outcome.content_summary.clone(),
                true,
                None,
                Some(outcome.manifest.revision),
            ));
        }
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_preview_selection = None;
        self.cloud_sync_progress = None;
        if should_trigger_upload_after {
            self.cloud_sync_upload_after_current = true;
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
        let summary = cloud_sync_preview_summary(&CloudSyncPendingPreview::Legacy {
            preview: preview.clone(),
            source: source.clone(),
        });
        let effective_import_connections = selection.effective_import_connections(&summary);
        let options = OxideClientStateImportOptions {
            oxide_options: oxideterm_connections::oxide_file::OxideImportOptions {
                selected_names: selection.selected_connection_names_for_import(&summary),
                conflict_strategy: import_strategy_from_cloud_settings(
                    selection.conflict_strategy.clone(),
                ),
                import_forwards: selection.import_forwards,
                import_portable_secrets: effective_import_connections,
                ..oxideterm_connections::oxide_file::OxideImportOptions::default()
            },
            import_quick_commands: true,
            quick_command_strategy: QuickCommandImportStrategy::Merge,
            import_plugin_settings: selection.effective_import_plugin_settings(),
            selected_plugin_ids: selection.selected_plugin_hash_set(),
            import_app_settings: selection.effective_import_app_settings(&summary),
            selected_app_settings_sections: selection.selected_app_settings_hash_set(&summary),
        };
        let imported_forwards = if options.oxide_options.import_forwards {
            self.apply_oxide_import_forward_records(&mut outcome.envelope)
        } else {
            0
        };
        outcome.envelope.imported_forwards = imported_forwards;
        let (_imported_quick_commands, _skipped_quick_commands, _quick_command_errors) = self
            .apply_oxide_import_quick_commands(
                outcome.envelope.quick_commands_json.as_deref(),
                options.import_quick_commands,
                options.quick_command_strategy,
            );
        self.apply_oxide_import_plugin_settings(
            &outcome.envelope.plugin_settings,
            options.import_plugin_settings,
            options.selected_plugin_ids.as_ref(),
        );
        self.apply_oxide_import_app_settings(
            outcome.envelope.app_settings_json.as_deref(),
            options.import_app_settings,
            options.selected_app_settings_sections.as_ref(),
            cx,
        );
        if options.oxide_options.import_portable_secrets {
            self.apply_oxide_import_portable_secrets(&mut outcome.envelope);
        }

        let local_snapshot = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            None,
            Some(&self.cloud_sync_store.state().sync_scope),
        );
        let now = Utc::now().to_rfc3339();
        let was_conflict_blocked = self
            .cloud_sync_store
            .state()
            .auto_upload_blocked_by_conflict;
        let applied_full_remote = matches!(source, CloudSyncPreviewSource::Remote)
            && legacy_apply_covers_full_remote(&summary, &selection);
        let should_trigger_upload_after = matches!(source, CloudSyncPreviewSource::Remote)
            && was_conflict_blocked
            && !applied_full_remote;
        let previous_revision = self
            .cloud_sync_store
            .state()
            .last_known_remote_revision
            .clone();
        let previous_etag = self.cloud_sync_store.state().last_known_remote_etag.clone();
        {
            let state = self.cloud_sync_store.state_mut();
            state.status = CloudSyncStatus::Idle;
            state.last_error = None;
            if applied_full_remote {
                state.last_sync_at = Some(now);
                state.last_known_remote_revision = preview.remote_metadata.revision.clone();
                state.last_known_remote_etag = preview.remote_metadata.etag.clone();
            } else {
                state.last_known_remote_revision = previous_revision;
                state.last_known_remote_etag = previous_etag;
            }
            if matches!(source, CloudSyncPreviewSource::Remote) {
                state.remote_format = preview.remote_metadata.format.clone();
                state.remote_section_revisions = preview.remote_metadata.section_revisions.clone();
                state.remote_updated_at = preview.remote_metadata.uploaded_at.clone();
                state.remote_device_id = preview.remote_metadata.device_id.clone();
                state.remote_exists = preview.remote_metadata.exists;
            }
            if let Ok(snapshot) = local_snapshot.as_ref() {
                if applied_full_remote {
                    state.last_synced_local_metadata = Some(snapshot.metadata.clone());
                    state.last_synced_structured_state = Some(snapshot.dirty.current_state.clone());
                }
                state.local_dirty = !applied_full_remote && snapshot.dirty.has_dirty;
                state.local_dirty_sections = Some(snapshot.dirty.dirty_sections.clone());
            }
            state.auto_upload_blocked_by_conflict = should_trigger_upload_after;
            if !state.auto_upload_blocked_by_conflict {
                state.conflict_details = None;
            }
            let action = if source.is_backup() {
                "restore"
            } else {
                "pull"
            };
            state.append_history(CloudSyncHistoryEntry::new(
                action,
                history_summary_from_legacy_preview(&preview),
                true,
                None,
                preview.remote_metadata.revision.clone(),
            ));
        }
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_preview_selection = None;
        self.cloud_sync_progress = None;
        if should_trigger_upload_after {
            self.cloud_sync_upload_after_current = true;
        }
        self.save_cloud_sync_state();
        if source.is_backup() {
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.restore_success_title"),
                Some(self.i18n_replace(
                    "plugin.cloud_sync.toast.restore_success_description",
                    &[
                        ("imported", outcome.envelope.imported.to_string()),
                        ("merged", outcome.envelope.merged.to_string()),
                    ],
                )),
                TerminalNoticeVariant::Success,
            );
        } else {
            self.push_cloud_sync_toast(
                self.i18n.t("plugin.cloud_sync.toast.pull_success_title"),
                Some(self.i18n_replace(
                    "plugin.cloud_sync.toast.pull_success_description",
                    &[
                        ("imported", outcome.envelope.imported.to_string()),
                        ("merged", outcome.envelope.merged.to_string()),
                    ],
                )),
                TerminalNoticeVariant::Success,
            );
        }
    }

    fn finish_cloud_sync_error(&mut self, action: &str, error: String) {
        let display_error = self.format_cloud_sync_error(&error);
        let remote_revision = self
            .cloud_sync_store
            .state()
            .last_known_remote_revision
            .clone();
        let is_upload_conflict = action == "upload"
            && error
                .trim_start()
                .starts_with("remote_changed_before_upload");
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::Error;
        self.cloud_sync_store.state_mut().last_error = Some(display_error.clone());
        if is_upload_conflict {
            let state = self.cloud_sync_store.state_mut();
            state.auto_upload_blocked_by_conflict = true;
            state.conflict_details = Some(CloudSyncConflictDetails {
                revision: state.last_known_remote_revision.clone(),
                device_id: state.remote_device_id.clone(),
                updated_at: state.remote_updated_at.clone(),
            });
        }
        if action == "upload" {
            let history_summary = self.cloud_sync_upload_failure_summary();
            self.cloud_sync_store
                .state_mut()
                .append_history(CloudSyncHistoryEntry::new(
                    action,
                    history_summary,
                    false,
                    Some(display_error.clone()),
                    remote_revision,
                ));
        }
        self.cloud_sync_progress = None;
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
            has_app_settings: true,
            plugin_settings_count: 0,
        }
    }

    fn save_cloud_sync_state(&mut self) {
        if let Err(error) = self.cloud_sync_store.save() {
            self.cloud_sync_store.state_mut().last_error = Some(error.to_string());
        }
    }

    fn cloud_sync_backend_label(&self, settings: &CloudSyncSettings) -> String {
        match settings.backend_type {
            BackendType::Webdav => self.i18n.t("plugin.cloud_sync.backend.webdav"),
            BackendType::HttpJson => self.i18n.t("plugin.cloud_sync.backend.http_json"),
            BackendType::Dropbox => self.i18n.t("plugin.cloud_sync.backend.dropbox"),
            BackendType::S3 => self.i18n.t("plugin.cloud_sync.backend.s3"),
            BackendType::Git => self.i18n.t("plugin.cloud_sync.backend.git"),
        }
    }

    fn cloud_sync_status_label(&self, status: CloudSyncStatus) -> String {
        match status {
            CloudSyncStatus::Idle => self.i18n.t("plugin.cloud_sync.status.ready"),
            CloudSyncStatus::Uploading => self.i18n.t("plugin.cloud_sync.status.uploading"),
            CloudSyncStatus::Checking => self.i18n.t("plugin.cloud_sync.status.checking"),
            CloudSyncStatus::RemoteUpdate => self.i18n.t("plugin.cloud_sync.status.remote_update"),
            CloudSyncStatus::Conflict => self.i18n.t("plugin.cloud_sync.status.conflict"),
            CloudSyncStatus::Error => self.i18n.t("plugin.cloud_sync.status.error"),
        }
    }

    fn cloud_sync_progress_stage_label(&self, stage: CloudSyncProgressStage) -> String {
        match stage {
            CloudSyncProgressStage::FetchMetadata => {
                self.i18n.t("plugin.cloud_sync.progress.fetch_metadata")
            }
            CloudSyncProgressStage::Preflight => {
                self.i18n.t("plugin.cloud_sync.progress.preflight")
            }
            CloudSyncProgressStage::Exporting => {
                self.i18n.t("plugin.cloud_sync.progress.exporting")
            }
            CloudSyncProgressStage::UploadingBlob => {
                self.i18n.t("plugin.cloud_sync.progress.uploading_blob")
            }
            CloudSyncProgressStage::Downloading => {
                self.i18n.t("plugin.cloud_sync.progress.downloading")
            }
            CloudSyncProgressStage::Validating => {
                self.i18n.t("plugin.cloud_sync.progress.validating")
            }
            CloudSyncProgressStage::PreviewingImport => {
                self.i18n.t("plugin.cloud_sync.progress.previewing_import")
            }
            CloudSyncProgressStage::Importing => {
                self.i18n.t("plugin.cloud_sync.progress.importing")
            }
            CloudSyncProgressStage::CreatingBackup => {
                self.i18n.t("plugin.cloud_sync.progress.creating_backup")
            }
            CloudSyncProgressStage::Done => self.i18n.t("plugin.cloud_sync.progress.done"),
            _ => self.i18n.t("plugin.cloud_sync.progress.done"),
        }
    }
}

fn persist_remote_metadata(
    state: &mut CloudSyncPersistedState,
    metadata: &oxideterm_cloud_sync::backend::RemoteMetadata,
) {
    state.remote_exists = metadata.exists;
    state.remote_format = metadata.format.clone();
    state.remote_section_revisions = metadata.section_revisions.clone();
    state.last_known_remote_revision = metadata.revision.clone();
    state.last_known_remote_etag = metadata.etag.clone();
    state.remote_updated_at = metadata.uploaded_at.clone();
    state.remote_device_id = metadata.device_id.clone();
}

fn structured_apply_covers_full_remote(
    manifest: &oxideterm_cloud_sync::StructuredManifest,
    selection: &StructuredApplySelection,
) -> bool {
    (manifest.sections.connections.is_none() || selection.connections)
        && (manifest.sections.forwards.is_none() || selection.forwards)
        && manifest
            .sections
            .app_settings
            .keys()
            .all(|section_id| selection.app_settings_sections.contains(section_id))
        && manifest
            .sections
            .plugin_settings
            .keys()
            .filter(|plugin_id| plugin_id.as_str() != oxideterm_cloud_sync::CLOUD_SYNC_PLUGIN_ID)
            .all(|plugin_id| selection.plugin_ids.contains(plugin_id))
}

fn merge_structured_remote_baseline(
    previous: Option<&StructuredSectionRevisions>,
    next: &StructuredSectionRevisions,
    selection: &StructuredApplySelection,
) -> StructuredSectionRevisions {
    let mut merged = previous.cloned().unwrap_or_default();
    if selection.connections {
        merged.connections = next.connections.clone();
    }
    if selection.forwards {
        merged.forwards = next.forwards.clone();
    }
    for section_id in &selection.app_settings_sections {
        if let Some(revision) = next.app_settings.get(section_id) {
            merged
                .app_settings
                .insert(section_id.clone(), revision.clone());
        }
    }
    for plugin_id in &selection.plugin_ids {
        if let Some(revision) = next.plugin_settings.get(plugin_id) {
            merged
                .plugin_settings
                .insert(plugin_id.clone(), revision.clone());
        }
    }
    merged
}

fn legacy_apply_covers_full_remote(
    summary: &CloudSyncPreviewSummary,
    selection: &CloudSyncPreviewSelection,
) -> bool {
    let remote_connection_names = summary.connection_record_names();
    let remote_app_section_ids = summary
        .app_settings_sections
        .iter()
        .map(|section| section.id.as_str())
        .collect::<Vec<_>>();
    let remote_plugin_ids = summary
        .plugin_settings_by_plugin
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();

    (summary.connections == 0
        || (selection.import_connections
            && (remote_connection_names.is_empty()
                || remote_connection_names
                    .iter()
                    .all(|name| selection.selected_connection_names.contains(name)))))
        && (summary.forwards == 0 || selection.import_forwards)
        && (!summary.has_app_settings
            || (selection.effective_import_app_settings(summary)
                && remote_app_section_ids
                    .iter()
                    .all(|id| selection.selected_app_settings_sections.contains(*id))))
        && (remote_plugin_ids.is_empty()
            || (selection.effective_import_plugin_settings()
                && remote_plugin_ids
                    .iter()
                    .all(|id| selection.selected_plugin_ids.contains(*id))))
}

fn cloud_sync_apply_total_units(
    preview: &CloudSyncPendingPreview,
    selection: &CloudSyncPreviewSelection,
    create_rollback_backup: bool,
) -> f64 {
    let rollback_units = usize::from(create_rollback_backup);
    let import_units = match preview {
        CloudSyncPendingPreview::Structured(preview) => {
            let structured_selection = selection.structured_selection();
            usize::from(structured_selection.connections && preview.connections_snapshot.is_some())
                + usize::from(structured_selection.forwards && preview.forwards_snapshot.is_some())
                + structured_selection
                    .app_settings_sections
                    .iter()
                    .filter(|section_id| preview.app_settings_entries.contains_key(*section_id))
                    .count()
                + structured_selection
                    .plugin_ids
                    .iter()
                    .filter(|plugin_id| preview.plugin_settings_entries.contains_key(*plugin_id))
                    .count()
        }
        CloudSyncPendingPreview::Legacy { .. } => 1,
    };
    (rollback_units + import_units).max(1) as f64
}

fn history_summary_from_manifest(manifest: &StructuredManifest) -> CloudSyncHistorySummary {
    CloudSyncHistorySummary {
        connections: manifest
            .sections
            .connections
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        forwards: manifest
            .sections
            .forwards
            .as_ref()
            .and_then(|entry| entry.record_count)
            .unwrap_or(0),
        has_app_settings: !manifest.sections.app_settings.is_empty(),
        plugin_settings_count: manifest.sections.plugin_settings.len(),
    }
}

fn history_summary_from_legacy_preview(preview: &LegacyPreview) -> CloudSyncHistorySummary {
    CloudSyncHistorySummary {
        connections: preview.metadata.num_connections,
        forwards: preview.preview.total_forwards,
        has_app_settings: preview.preview.has_app_settings,
        plugin_settings_count: preview.preview.plugin_settings_count,
    }
}

fn cloud_sync_number_string(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

#[derive(Clone, Debug, Default)]
struct CloudSyncPreviewSummary {
    connections: usize,
    forwards: usize,
    has_app_settings: bool,
    app_settings_sections: Vec<CloudSyncAppSettingsSection>,
    plugin_settings_count: usize,
    plugin_settings_by_plugin: BTreeMap<String, usize>,
    has_embedded_keys: bool,
    forward_details: Vec<CloudSyncForwardDetail>,
    records: Vec<CloudSyncPreviewRecord>,
}

#[derive(Clone, Debug)]
struct CloudSyncAppSettingsSection {
    id: String,
    field_count: usize,
}

#[derive(Clone, Debug)]
struct CloudSyncForwardDetail {
    owner_connection_name: String,
    direction: String,
    description: String,
}

#[derive(Clone, Debug)]
struct CloudSyncPreviewRecord {
    resource: String,
    name: String,
    action: String,
    reason_code: String,
    target_name: Option<String>,
}

impl CloudSyncPreviewSummary {
    fn grouped_records(&self) -> Vec<(&'static str, Vec<CloudSyncPreviewRecord>)> {
        ["import", "merge", "replace", "skip", "rename"]
            .into_iter()
            .map(|action| {
                (
                    action,
                    self.records
                        .iter()
                        .filter(|record| record.action == action)
                        .cloned()
                        .collect(),
                )
            })
            .collect()
    }

    fn connection_record_names(&self) -> BTreeSet<String> {
        self.records
            .iter()
            .filter(|record| record.resource == "connection")
            .map(|record| record.name.clone())
            .collect()
    }
}

fn cloud_sync_preview_summary(preview: &CloudSyncPendingPreview) -> CloudSyncPreviewSummary {
    match preview {
        CloudSyncPendingPreview::Structured(preview) => {
            let connections = preview
                .connections_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0);
            let forwards = preview
                .forwards_snapshot
                .as_ref()
                .map(|snapshot| snapshot.records.len())
                .unwrap_or(0);
            let plugin_settings_by_plugin = preview
                .plugin_settings_entries
                .keys()
                .map(|id| {
                    (
                        id.clone(),
                        preview.plugin_settings_counts.get(id).copied().unwrap_or(0),
                    )
                })
                .collect();
            let plugin_settings_count = preview.plugin_settings_counts.values().sum();
            CloudSyncPreviewSummary {
                connections,
                forwards,
                has_app_settings: !preview.app_settings_entries.is_empty(),
                app_settings_sections: preview
                    .app_settings_entries
                    .keys()
                    .map(|id| {
                        let field_count = preview
                            .app_settings_sections
                            .get(id)
                            .map(|section| section.field_keys.len())
                            .unwrap_or(0);
                        CloudSyncAppSettingsSection {
                            id: id.clone(),
                            field_count,
                        }
                    })
                    .collect(),
                plugin_settings_count,
                plugin_settings_by_plugin,
                has_embedded_keys: false,
                forward_details: Vec::new(),
                records: Vec::new(),
            }
        }
        CloudSyncPendingPreview::Legacy { preview, .. } => CloudSyncPreviewSummary {
            connections: preview.metadata.num_connections,
            forwards: preview.preview.total_forwards,
            has_app_settings: preview.preview.has_app_settings,
            app_settings_sections: preview
                .preview
                .app_settings_sections
                .iter()
                .map(|section| CloudSyncAppSettingsSection {
                    id: section.id.clone(),
                    field_count: section.field_keys.len(),
                })
                .collect(),
            plugin_settings_count: preview.preview.plugin_settings_count,
            plugin_settings_by_plugin: preview
                .preview
                .plugin_settings_by_plugin
                .iter()
                .map(|(plugin_id, count)| (plugin_id.clone(), *count))
                .collect(),
            has_embedded_keys: preview.preview.has_embedded_keys,
            forward_details: preview
                .preview
                .forward_details
                .iter()
                .map(|detail| CloudSyncForwardDetail {
                    owner_connection_name: detail.owner_connection_name.clone(),
                    direction: detail.direction.clone(),
                    description: detail.description.clone(),
                })
                .collect(),
            records: preview
                .preview
                .records
                .iter()
                .map(|record| CloudSyncPreviewRecord {
                    resource: record.resource.clone(),
                    name: record.name.clone(),
                    action: record.action.clone(),
                    reason_code: record.reason_code.clone(),
                    target_name: record.target_name.clone(),
                })
                .collect(),
        },
    }
}

fn cloud_sync_app_settings_section_label(i18n: &I18n, section_id: &str) -> String {
    let key = match section_id {
        "general" => "plugin.cloud_sync.preview.section_general",
        "terminalAppearance" => "plugin.cloud_sync.preview.section_terminal_appearance",
        "terminalBehavior" => "plugin.cloud_sync.preview.section_terminal_behavior",
        "appearance" => "plugin.cloud_sync.preview.section_appearance",
        "connections" => "plugin.cloud_sync.preview.section_connections",
        "fileAndEditor" => "plugin.cloud_sync.preview.section_file_and_editor",
        "localTerminal" => "plugin.cloud_sync.preview.section_local_terminal",
        _ => return section_id.to_string(),
    };
    i18n.t(key)
}

fn cloud_sync_error_code(error: &str) -> Option<&str> {
    let trimmed = error.trim();
    let code = trimmed
        .split_once(':')
        .map(|(code, _)| code.trim())
        .unwrap_or(trimmed);
    if cloud_sync_error_is_unauthorized(code) {
        return Some("http_unauthorized");
    }
    match code {
        "operation_in_progress"
        | "missing_endpoint"
        | "missing_namespace"
        | "missing_backend_token"
        | "network_request_failed"
        | "missing_git_repository"
        | "missing_s3_bucket"
        | "missing_s3_region"
        | "missing_s3_access_key_id"
        | "missing_s3_secret_access_key"
        | "missing_sync_password"
        | "etag_conflict_detected"
        | "remote_changed_before_upload"
        | "preflight_failed"
        | "snapshot_too_large"
        | "remote_not_found" => Some(code),
        _ => {
            let lower = trimmed.to_ascii_lowercase();
            if lower.starts_with("secret unlock required") {
                Some("secret_unlock_required")
            } else if lower.starts_with("secret access cancelled")
                || lower.contains("authentication canceled")
                || lower.contains("authentication cancelled")
            {
                Some("secret_access_cancelled")
            } else if lower.starts_with("secret access failed") {
                Some("secret_access_failed")
            } else {
                None
            }
        }
    }
}

fn cloud_sync_error_is_unauthorized(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    (lower.starts_with("http_") || lower.starts_with("webdav_")) && lower.contains("401")
}

fn cloud_sync_snapshot_limit_bytes(error: &str) -> Option<usize> {
    let (_, after_max) = error.split_once("max ")?;
    let digits = after_max
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

fn format_cloud_sync_bytes(bytes: usize) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn preview_cloud_sync_rollback_backup(
    connection_store: &ConnectionStore,
    backup: CloudSyncRollbackBackup,
    password: &str,
    progress: Option<&mut dyn CloudSyncProgressSink>,
) -> anyhow::Result<LegacyPreview> {
    let bytes = BASE64.decode(backup.bytes_base64.as_bytes())?;
    let metadata = OxideFile::from_bytes(&bytes)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .metadata;
    let mut noop = |_| {};
    let progress = progress.unwrap_or(&mut noop);
    let preview = preview_oxide_import_with_progress(
        connection_store,
        &bytes,
        password,
        ImportConflictStrategy::Replace,
        |_stage, current, total| {
            let fraction = if total == 0 {
                0.0
            } else {
                (current as f64 / total as f64).clamp(0.0, 1.0)
            };
            progress.report(CloudSyncProgress {
                stage: CloudSyncProgressStage::PreviewingImport,
                current: (1.0 + fraction).min(2.0),
                total: 2.0,
                message: None,
            });
        },
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(LegacyPreview {
        remote_metadata: oxideterm_cloud_sync::backend::RemoteMetadata::default(),
        bytes,
        metadata,
        preview,
    })
}

fn create_cloud_sync_rollback_backup(
    connection_store: &ConnectionStore,
    forwarding_registry: &ForwardingRegistry,
    settings_store: &SettingsStore,
    _settings: &CloudSyncSettings,
    source_revision: Option<String>,
    sync_password: Option<&str>,
) -> anyhow::Result<Option<CloudSyncRollbackBackup>> {
    let has_local_data = !connection_store.connections().is_empty()
        || !forwarding_registry.list_all_saved_forwards().is_empty();
    if !has_local_data {
        return Ok(None);
    }
    let Some(password) = sync_password.and_then(non_empty_secret) else {
        anyhow::bail!("missing_sync_password: cloud sync password is required");
    };
    let connection_ids = connection_store
        .connections()
        .iter()
        .map(|connection| connection.id.clone())
        .collect::<Vec<_>>();
    let selected_ids = connection_ids
        .iter()
        .cloned()
        .collect::<std::collections::HashSet<_>>();
    let app_settings_json = oxideterm_settings::export_oxide_settings_snapshot_json(
        settings_store.settings(),
        None,
        true,
    )?;
    let plugin_settings =
        oxideterm_cloud_sync::plugin_settings::load_plugin_settings(settings_store.path())
            .map_err(anyhow::Error::msg)?;
    let forwards = forwarding_registry
        .list_all_saved_forwards()
        .into_iter()
        .filter_map(|forward| {
            let owner_id = forward.owner_connection_id?;
            selected_ids
                .contains(&owner_id)
                .then(|| OxideForwardRecord {
                    connection_id: owner_id,
                    forward_type: match forward.forward_type {
                        ForwardType::Local => "local".to_string(),
                        ForwardType::Remote => "remote".to_string(),
                        ForwardType::Dynamic => "dynamic".to_string(),
                    },
                    bind_address: forward.rule.bind_address,
                    bind_port: forward.rule.bind_port,
                    target_host: forward.rule.target_host,
                    target_port: forward.rule.target_port,
                    description: Some(forward.rule.description),
                    auto_start: forward.auto_start,
                })
        })
        .collect::<Vec<_>>();
    let bytes = export_connections_to_oxide_with_progress(
        connection_store,
        &connection_ids,
        &password,
        OxideExportOptions {
            description: Some("Oxide Cloud Sync rollback backup".to_string()),
            embed_keys: false,
            app_settings_json: Some(app_settings_json),
            plugin_settings,
            forwards,
            ..OxideExportOptions::default()
        },
        |_stage, _current, _total| {},
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if bytes.len() > MAX_ROLLBACK_BACKUP_BYTES {
        anyhow::bail!(
            "rollback_backup_too_large: local rollback backup is too large ({} > {})",
            bytes.len(),
            MAX_ROLLBACK_BACKUP_BYTES
        );
    }
    let metadata = OxideFile::from_bytes(&bytes)
        .map_err(|error| anyhow::anyhow!(error.to_string()))?
        .metadata;
    let preview = preview_oxide_import_with_progress(
        connection_store,
        &bytes,
        &password,
        ImportConflictStrategy::Replace,
        |_stage, _current, _total| {},
    )
    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    Ok(Some(CloudSyncRollbackBackup {
        id: uuid::Uuid::new_v4().to_string(),
        created_at: Utc::now().to_rfc3339(),
        source_revision,
        size_bytes: bytes.len(),
        bytes_base64: BASE64.encode(bytes),
        metadata: Some(CloudSyncRollbackBackupMetadata {
            num_connections: metadata.num_connections,
            connection_names: metadata.connection_names,
            has_app_settings: metadata
                .has_app_settings
                .unwrap_or(preview.has_app_settings),
            plugin_settings_count: preview.plugin_settings_count,
            forwards: preview.total_forwards,
        }),
    }))
}

fn import_strategy_from_cloud_settings(
    strategy: ConflictStrategy,
) -> oxideterm_connections::oxide_file::ImportConflictStrategy {
    match strategy {
        ConflictStrategy::Merge => oxideterm_connections::oxide_file::ImportConflictStrategy::Merge,
        ConflictStrategy::Replace => {
            oxideterm_connections::oxide_file::ImportConflictStrategy::Replace
        }
        ConflictStrategy::Skip => oxideterm_connections::oxide_file::ImportConflictStrategy::Skip,
        ConflictStrategy::Rename => {
            oxideterm_connections::oxide_file::ImportConflictStrategy::Rename
        }
    }
}

fn has_cloud_sync_structured_conflict(
    dirty: &oxideterm_cloud_sync::StructuredDirtySections,
    remote: Option<&StructuredSectionRevisions>,
    previous: Option<&StructuredSectionRevisions>,
) -> bool {
    let Some(previous) = previous else {
        return dirty.connections
            || dirty.forwards
            || dirty.app_settings.values().any(|value| *value)
            || dirty.plugin_settings.values().any(|value| *value);
    };
    let remote = remote.cloned().unwrap_or_default();
    if dirty.connections && remote.connections != previous.connections {
        return true;
    }
    if dirty.forwards && remote.forwards != previous.forwards {
        return true;
    }
    dirty.app_settings.iter().any(|(section_id, value)| {
        *value && remote.app_settings.get(section_id) != previous.app_settings.get(section_id)
    }) || dirty.plugin_settings.iter().any(|(plugin_id, value)| {
        *value && remote.plugin_settings.get(plugin_id) != previous.plugin_settings.get(plugin_id)
    })
}

fn cloud_sync_value_prefers_mono(value: &str) -> bool {
    value != "—"
        && value.chars().count() >= 16
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | ':' | '/' | '.'))
}

fn cloud_sync_format_timestamp(value: &str) -> String {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| {
            timestamp
                .with_timezone(&Local)
                .format("%Y/%-m/%-d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| value.to_string())
}

fn cloud_sync_progress_unit(value: f64) -> String {
    if (value.fract()).abs() < f64::EPSILON {
        format!("{}", value as usize)
    } else {
        format!("{value:.1}")
    }
}

fn non_empty_secret(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn cloud_sync_platform_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "native"
    }
}

fn radix_select_next_index(
    current: usize,
    option_count: usize,
    direction: SelectKeyDirection,
) -> usize {
    if option_count == 0 {
        return 0;
    }
    match direction {
        SelectKeyDirection::Previous => current.saturating_sub(1),
        SelectKeyDirection::Next => (current + 1).min(option_count - 1),
    }
}

fn next_cloud_sync_select_focus(
    selects: &[CloudSyncSelect],
    current: CloudSyncSelect,
    forward: bool,
) -> Option<CloudSyncSelect> {
    let index = selects.iter().position(|candidate| *candidate == current)?;
    if forward {
        selects.get(index + 1).copied()
    } else {
        index
            .checked_sub(1)
            .and_then(|previous| selects.get(previous).copied())
    }
}

fn close_cloud_sync_select_on_container_scroll(
    open_select: &mut Option<CloudSyncSelect>,
    focused_select: &mut Option<CloudSyncSelect>,
    highlighted_option: &mut Option<(CloudSyncSelect, usize)>,
) -> bool {
    let Some(select) = open_select.take() else {
        return false;
    };

    // Radix Select closes its content when an owning scroll container moves,
    // but the trigger remains the browser focus anchor for the visible ring and
    // the next keyboard action. Keep that routing explicit for native GPUI.
    *focused_select = Some(select);
    *highlighted_option = None;
    true
}

#[cfg(test)]
mod cloud_sync_preview_selection_tests {
    use super::*;

    fn connection_record(name: &str) -> CloudSyncPreviewRecord {
        CloudSyncPreviewRecord {
            resource: "connection".to_string(),
            name: name.to_string(),
            action: "import".to_string(),
            reason_code: "new".to_string(),
            target_name: None,
        }
    }

    fn summary_with_connections(names: &[&str]) -> CloudSyncPreviewSummary {
        CloudSyncPreviewSummary {
            connections: names.len(),
            records: names.iter().map(|name| connection_record(name)).collect(),
            ..CloudSyncPreviewSummary::default()
        }
    }

    #[test]
    fn legacy_preview_selection_exports_selected_connection_names() {
        let summary = summary_with_connections(&["Prod", "Staging"]);
        let mut selection = CloudSyncPreviewSelection {
            import_connections: true,
            selected_connection_names: BTreeSet::from(["Prod".to_string()]),
            import_app_settings: false,
            selected_app_settings_sections: BTreeSet::new(),
            import_plugin_settings: false,
            selected_plugin_ids: BTreeSet::new(),
            import_forwards: false,
            conflict_strategy: ConflictStrategy::Rename,
        };

        assert_eq!(
            selection.selected_connection_names_for_import(&summary),
            Some(vec!["Prod".to_string()])
        );
        assert!(selection.can_apply(&summary));
        assert!(!legacy_apply_covers_full_remote(&summary, &selection));

        selection
            .selected_connection_names
            .insert("Staging".to_string());
        assert!(legacy_apply_covers_full_remote(&summary, &selection));
    }

    #[test]
    fn legacy_preview_selection_disables_connection_import_when_none_checked() {
        let summary = summary_with_connections(&["Prod"]);
        let selection = CloudSyncPreviewSelection {
            import_connections: true,
            selected_connection_names: BTreeSet::new(),
            import_app_settings: false,
            selected_app_settings_sections: BTreeSet::new(),
            import_plugin_settings: false,
            selected_plugin_ids: BTreeSet::new(),
            import_forwards: false,
            conflict_strategy: ConflictStrategy::Rename,
        };

        assert_eq!(
            selection.selected_connection_names_for_import(&summary),
            Some(Vec::new())
        );
        assert!(!selection.can_apply(&summary));
        assert!(!legacy_apply_covers_full_remote(&summary, &selection));
    }

    #[test]
    fn radix_select_keyboard_navigation_clamps_like_native_select() {
        assert_eq!(
            radix_select_next_index(0, 3, SelectKeyDirection::Previous),
            0
        );
        assert_eq!(radix_select_next_index(0, 3, SelectKeyDirection::Next), 1);
        assert_eq!(radix_select_next_index(2, 3, SelectKeyDirection::Next), 2);
        assert_eq!(radix_select_next_index(0, 0, SelectKeyDirection::Next), 0);
    }

    #[test]
    fn cloud_sync_select_focus_tabs_only_through_visible_controls() {
        let webdav_selects = [
            CloudSyncSelect::Backend,
            CloudSyncSelect::AuthMode,
            CloudSyncSelect::ConflictStrategy,
        ];
        let hidden_auth_selects = [CloudSyncSelect::Backend, CloudSyncSelect::ConflictStrategy];

        assert_eq!(
            next_cloud_sync_select_focus(&webdav_selects, CloudSyncSelect::Backend, true),
            Some(CloudSyncSelect::AuthMode)
        );
        assert_eq!(
            next_cloud_sync_select_focus(&hidden_auth_selects, CloudSyncSelect::Backend, true),
            Some(CloudSyncSelect::ConflictStrategy)
        );
        assert_eq!(
            next_cloud_sync_select_focus(
                &hidden_auth_selects,
                CloudSyncSelect::ConflictStrategy,
                true
            ),
            None
        );
        assert_eq!(
            next_cloud_sync_select_focus(&webdav_selects, CloudSyncSelect::AuthMode, false),
            Some(CloudSyncSelect::Backend)
        );
    }

    #[test]
    fn cloud_sync_select_scroll_close_preserves_trigger_focus() {
        let mut open_select = Some(CloudSyncSelect::ConflictStrategy);
        let mut focused_select = Some(CloudSyncSelect::Backend);
        let mut highlighted_option = Some((CloudSyncSelect::ConflictStrategy, 1));

        assert!(close_cloud_sync_select_on_container_scroll(
            &mut open_select,
            &mut focused_select,
            &mut highlighted_option,
        ));
        assert_eq!(open_select, None);
        assert_eq!(focused_select, Some(CloudSyncSelect::ConflictStrategy));
        assert_eq!(highlighted_option, None);

        assert!(!close_cloud_sync_select_on_container_scroll(
            &mut open_select,
            &mut focused_select,
            &mut highlighted_option,
        ));
        assert_eq!(focused_select, Some(CloudSyncSelect::ConflictStrategy));
    }
}
