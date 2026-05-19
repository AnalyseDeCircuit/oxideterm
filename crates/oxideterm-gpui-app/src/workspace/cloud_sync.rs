use std::{
    collections::BTreeMap,
    sync::mpsc::{self, TryRecvError},
};

use chrono::Utc;
use gpui_component::scroll::ScrollableElement;
use oxideterm_cloud_sync::{
    BackendType, CloudSyncSettings, CloudSyncStatus, ConflictStrategy, STRUCTURED_MANIFEST_FORMAT,
    StructuredSectionRevisions, build_manifest_section_revisions, merge_structured_baseline,
    operation::{
        ApplyLegacyPreviewOutcome, ApplyStructuredPreviewOutcome, LegacyPreview, StructuredPreview,
        UploadOptions, UploadOutcome,
    },
    progress::{CloudSyncProgress, CloudSyncProgressStage},
    secrets::CloudSyncKeychainSecretProvider,
    service::{CloudSyncLocalSnapshot, build_local_snapshot},
    state::{
        CloudSyncConflictDetails, CloudSyncHistoryEntry, CloudSyncHistorySummary,
        CloudSyncPersistedState,
    },
};
use oxideterm_gpui_ui::button::{
    ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, button_with,
};

use super::quick_commands::QuickCommandImportStrategy;
use super::session_manager::OxideClientStateImportOptions;
use super::*;

#[derive(Clone, Debug)]
pub(super) enum CloudSyncPendingPreview {
    Structured(StructuredPreview),
    Legacy(LegacyPreview),
}

pub(super) enum CloudSyncDelivery {
    Progress(CloudSyncProgress),
    CheckFinished(CloudSyncActionResult<Option<oxideterm_cloud_sync::backend::RemoteMetadata>>),
    UploadFinished(CloudSyncActionResult<UploadOutcome>),
    PullPreviewFinished(CloudSyncActionResult<CloudSyncPendingPreview>),
    ApplyPreviewFinished(CloudSyncActionResult<CloudSyncApplyUiOutcome>),
}

pub(super) struct CloudSyncActionResult<T> {
    result: Result<T, String>,
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
        outcome: ApplyLegacyPreviewOutcome,
    },
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
        self.active_sidebar_section = SidebarSection::Extensions;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        self.persist_sidebar_settings();
        cx.notify();
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
        let has_preview = self.cloud_sync_pending_preview.is_some();

        div()
            .size_full()
            .overflow_y_scrollbar()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .w_full()
                    .max_w(px(self.tokens.metrics.settings_content_max_width))
                    .mx_auto()
                    .p(px(self.tokens.metrics.settings_content_padding))
                    .flex()
                    .flex_col()
                    .gap(px(self.tokens.metrics.settings_page_gap))
                    .child(self.render_cloud_sync_header())
                    .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
                    .child(
                        div()
                            .w_full()
                            .rounded(px(self.tokens.radii.lg))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_card))
                            .p(px(self.tokens.metrics.settings_card_padding))
                            .flex()
                            .flex_col()
                            .gap(px(16.0))
                            .child(self.render_cloud_sync_status_header(&state, busy, cx))
                            .child(
                                div()
                                    .flex()
                                    .flex_row()
                                    .flex_wrap()
                                    .gap(px(8.0))
                                    .child(self.render_cloud_sync_action_button(
                                        "plugin.cloud_sync.actions.check",
                                        ButtonVariant::Outline,
                                        busy,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.start_cloud_sync_check(cx);
                                            cx.stop_propagation();
                                        }),
                                    ))
                                    .child(self.render_cloud_sync_action_button(
                                        "plugin.cloud_sync.actions.upload",
                                        ButtonVariant::Default,
                                        busy,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.start_cloud_sync_upload(false, cx);
                                            cx.stop_propagation();
                                        }),
                                    ))
                                    .child(self.render_cloud_sync_action_button(
                                        "plugin.cloud_sync.actions.pull_preview",
                                        ButtonVariant::Outline,
                                        busy,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.start_cloud_sync_pull_preview(cx);
                                            cx.stop_propagation();
                                        }),
                                    ))
                                    .child(self.render_cloud_sync_action_button(
                                        "plugin.cloud_sync.actions.apply_preview",
                                        ButtonVariant::Outline,
                                        busy || !has_preview,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.start_cloud_sync_apply_preview(cx);
                                            cx.stop_propagation();
                                        }),
                                    )),
                            )
                            .when_some(self.cloud_sync_progress.as_ref(), |card, progress| {
                                card.child(self.render_cloud_sync_progress(progress))
                            })
                            .when_some(state.last_error.as_ref(), |card, error| {
                                card.child(self.render_cloud_sync_error(error))
                            })
                            .child(
                                div()
                                    .grid()
                                    .grid_cols(2)
                                    .gap(px(12.0))
                                    .child(self.render_cloud_sync_fact(
                                        "plugin.cloud_sync.fields.backend",
                                        backend_label,
                                    ))
                                    .child(self.render_cloud_sync_fact(
                                        "plugin.cloud_sync.fields.namespace",
                                        settings.namespace,
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
                                        ),
                                    )
                                    .child(
                                        self.render_cloud_sync_fact(
                                            "plugin.cloud_sync.fields.remote_revision",
                                            state
                                                .last_known_remote_revision
                                                .clone()
                                                .unwrap_or_else(|| "—".to_string()),
                                        ),
                                    ),
                            )
                            .child(
                                div()
                                    .grid()
                                    .grid_cols(2)
                                    .gap(px(12.0))
                                    .child(
                                        self.render_cloud_sync_fact(
                                            "plugin.cloud_sync.fields.connections",
                                            local_snapshot
                                                .as_ref()
                                                .map(|snapshot| {
                                                    format!(
                                                        "{} / {}",
                                                        snapshot.connections_record_count,
                                                        snapshot
                                                            .metadata
                                                            .saved_connections_revision
                                                            .as_deref()
                                                            .unwrap_or("—")
                                                    )
                                                })
                                                .unwrap_or_else(|error| error.to_string()),
                                        ),
                                    )
                                    .child(
                                        self.render_cloud_sync_fact(
                                            "plugin.cloud_sync.fields.forwards",
                                            local_snapshot
                                                .as_ref()
                                                .map(|snapshot| {
                                                    format!(
                                                        "{} / {}",
                                                        snapshot.forwards_record_count,
                                                        snapshot
                                                            .metadata
                                                            .saved_forwards_revision
                                                            .as_deref()
                                                            .unwrap_or("—")
                                                    )
                                                })
                                                .unwrap_or_else(|error| error.to_string()),
                                        ),
                                    ),
                            )
                            .child(self.render_cloud_sync_timestamps(&state))
                            .when_some(self.cloud_sync_pending_preview.as_ref(), |card, preview| {
                                card.child(self.render_cloud_sync_preview(preview))
                            })
                            .child(self.render_cloud_sync_history(&state))
                            .child(self.render_cloud_sync_notes(local_snapshot.as_ref().ok())),
                    ),
            )
            .into_any_element()
    }

    fn render_cloud_sync_header(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::RefreshCw,
                        24.0,
                        rgb(theme.accent),
                    ))
                    .child(
                        div()
                            .text_size(px(24.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("plugin.cloud_sync.panel_title")),
                    ),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("plugin.cloud_sync.native_description")),
            )
            .into_any_element()
    }

    fn render_cloud_sync_status_header(
        &self,
        state: &CloudSyncPersistedState,
        busy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(
                                self.i18n
                                    .t("plugin.cloud_sync.sections.connection_settings"),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.cloud_sync.native_service_ready")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(5.0))
                            .rounded(px(self.tokens.radii.lg))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(match state.status {
                                CloudSyncStatus::Conflict | CloudSyncStatus::Error => theme.error,
                                CloudSyncStatus::Uploading | CloudSyncStatus::Checking => {
                                    theme.accent
                                }
                                _ => theme.text_muted,
                            }))
                            .child(self.cloud_sync_status_label(state.status.clone())),
                    )
                    .when(busy, |row| {
                        row.child(self.render_cloud_sync_action_button(
                            "plugin.cloud_sync.actions.refresh",
                            ButtonVariant::Outline,
                            false,
                            cx.listener(|this, _event, _window, cx| {
                                this.poll_cloud_sync_delivery(cx);
                                cx.stop_propagation();
                            }),
                        ))
                    }),
            )
            .into_any_element()
    }

    fn render_cloud_sync_action_button(
        &self,
        label_key: &str,
        variant: ButtonVariant,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        button_with(
            &self.tokens,
            self.i18n.t(label_key),
            ButtonOptions {
                variant,
                size: ButtonSize::Sm,
                radius: ButtonRadius::Md,
                disabled,
            },
        )
        .when(!disabled, |button| {
            button
                .cursor_pointer()
                .on_mouse_down(MouseButton::Left, listener)
        })
        .into_any_element()
    }

    fn render_cloud_sync_progress(&self, progress: &CloudSyncProgress) -> AnyElement {
        let theme = self.tokens.ui;
        let ratio = if progress.total == 0 {
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
                    .child(self.cloud_sync_progress_stage_label(progress.stage))
                    .child(format!("{}/{}", progress.current, progress.total)),
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
            .child(error.to_string())
            .into_any_element()
    }

    fn render_cloud_sync_timestamps(&self, state: &CloudSyncPersistedState) -> AnyElement {
        div()
            .grid()
            .grid_cols(3)
            .gap(px(12.0))
            .child(
                self.render_cloud_sync_fact(
                    "plugin.cloud_sync.fields.last_sync",
                    state
                        .last_sync_at
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                ),
            )
            .child(
                self.render_cloud_sync_fact(
                    "plugin.cloud_sync.fields.last_upload",
                    state
                        .last_upload_at
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                ),
            )
            .child(
                self.render_cloud_sync_fact(
                    "plugin.cloud_sync.fields.last_check",
                    state
                        .last_check_at
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                ),
            )
            .into_any_element()
    }

    fn render_cloud_sync_preview(&self, preview: &CloudSyncPendingPreview) -> AnyElement {
        let theme = self.tokens.ui;
        let (revision, connections, forwards, has_app_settings, plugin_settings, units) =
            match preview {
                CloudSyncPendingPreview::Structured(preview) => (
                    preview.manifest.revision.clone(),
                    preview
                        .connections_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.records.len())
                        .unwrap_or(0),
                    preview
                        .forwards_snapshot
                        .as_ref()
                        .map(|snapshot| snapshot.records.len())
                        .unwrap_or(0),
                    !preview.app_settings_entries.is_empty(),
                    preview.plugin_settings_entries.len(),
                    count_structured_preview_units(preview),
                ),
                CloudSyncPendingPreview::Legacy(preview) => (
                    preview
                        .remote_metadata
                        .revision
                        .clone()
                        .unwrap_or_else(|| "—".to_string()),
                    preview.metadata.num_connections,
                    preview.preview.total_forwards,
                    preview.preview.has_app_settings,
                    preview.preview.plugin_settings_count,
                    count_legacy_preview_units(preview),
                ),
            };
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(14.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t("plugin.cloud_sync.sections.pending_preview")),
            )
            .child(
                self.render_cloud_sync_fact("plugin.cloud_sync.fields.remote_revision", revision),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(12.0))
                    .child(self.render_cloud_sync_fact(
                        "plugin.cloud_sync.fields.connections",
                        connections.to_string(),
                    ))
                    .child(self.render_cloud_sync_fact(
                        "plugin.cloud_sync.fields.forwards",
                        forwards.to_string(),
                    )),
            )
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(12.0))
                    .child(self.render_cloud_sync_fact(
                        "plugin.cloud_sync.preview.app_settings",
                        if has_app_settings {
                            self.i18n.t("plugin.cloud_sync.common.yes")
                        } else {
                            self.i18n.t("plugin.cloud_sync.common.no")
                        },
                    ))
                    .child(self.render_cloud_sync_fact(
                        "plugin.cloud_sync.preview.plugin_settings_label",
                        plugin_settings.to_string(),
                    )),
            )
            .child(
                self.render_cloud_sync_fact(
                    "plugin.cloud_sync.fields.upload_units",
                    units.to_string(),
                ),
            )
            .into_any_element()
    }

    fn render_cloud_sync_history(&self, state: &CloudSyncPersistedState) -> AnyElement {
        let theme = self.tokens.ui;
        let mut card = div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(14.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t("plugin.cloud_sync.sections.history")),
            );
        if state.sync_history.is_empty() {
            card = card.child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("plugin.cloud_sync.history_empty")),
            );
        } else {
            for entry in state.sync_history.iter().take(5) {
                card = card.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(2.0))
                                .child(
                                    div()
                                        .text_color(rgb(theme.text))
                                        .child(format!("{} · {}", entry.action, entry.timestamp)),
                                )
                                .child(div().text_color(rgb(theme.text_muted)).child(format!(
                                    "{} / {} / {}",
                                    entry.summary.connections,
                                    entry.summary.forwards,
                                    entry.summary.plugin_settings_count
                                ))),
                        )
                        .child(
                            div()
                                .text_color(if entry.success {
                                    rgb(theme.text_muted)
                                } else {
                                    rgb(theme.error)
                                })
                                .child(
                                    entry
                                        .remote_revision
                                        .clone()
                                        .or_else(|| entry.error.clone())
                                        .unwrap_or_else(|| "—".to_string()),
                                ),
                        ),
                );
            }
        }
        card.into_any_element()
    }

    fn render_cloud_sync_notes(
        &self,
        local_snapshot: Option<&CloudSyncLocalSnapshot>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(14.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t("plugin.cloud_sync.sections.notes")),
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

    fn render_cloud_sync_fact(&self, label_key: &str, value: String) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .px(px(12.0))
            .py(px(10.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t(label_key)),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text))
                    .child(value),
            )
            .into_any_element()
    }

    fn start_cloud_sync_check(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
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
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let mut progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            let result = service
                .check_remote(&settings, &mut provider, false, false, Some(&mut progress))
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(CloudSyncDelivery::CheckFinished(CloudSyncActionResult {
                result,
                secret_hints: provider.hints().clone(),
            }));
        });
    }

    fn start_cloud_sync_upload(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            return;
        }
        let (device_id, revision_sequence) = {
            let state = self.cloud_sync_store.state_mut();
            let device_id = state.ensure_device_id(cloud_sync_platform_label());
            let revision_sequence = state.next_revision_sequence();
            state.status = CloudSyncStatus::Uploading;
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
        let connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let settings_store = self.settings_store.clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let mut progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            let result = service
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
                        previous_remote_sections,
                        ..UploadOptions::default()
                    },
                    Some(&mut progress),
                )
                .await
                .map(|outcome| outcome.expect("cloud sync upload unexpectedly skipped"))
                .map_err(|error| error.to_string());
            let _ = tx.send(CloudSyncDelivery::UploadFinished(CloudSyncActionResult {
                result,
                secret_hints: provider.hints().clone(),
            }));
        });
    }

    fn start_cloud_sync_pull_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            return;
        }
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::Checking;
        self.cloud_sync_store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let connection_store = self.connection_store.clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let mut progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            let result = match service
                .pull_structured_preview(&settings, &mut provider, Some(&mut progress))
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
                    .map(CloudSyncPendingPreview::Legacy),
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

    fn start_cloud_sync_apply_preview(&mut self, cx: &mut Context<Self>) {
        if self.cloud_sync_rx.is_some() {
            return;
        }
        let Some(preview) = self.cloud_sync_pending_preview.clone() else {
            return;
        };
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::Uploading;
        self.cloud_sync_store.state_mut().last_error = None;
        self.save_cloud_sync_state();
        let mut connection_store = self.connection_store.clone();
        let forwarding_registry = self.forwarding_registry.clone();
        let mut settings_store = self.settings_store.clone();
        let settings = self.cloud_sync_store.state().settings.clone();
        let hints = self.cloud_sync_store.state().secret_hints.clone();
        let service = self.cloud_sync_service.clone();
        let (tx, rx) = mpsc::channel();
        self.cloud_sync_rx = Some(rx);
        self.schedule_cloud_sync_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let mut provider = CloudSyncKeychainSecretProvider::new(hints);
            let progress_tx = tx.clone();
            let mut progress = move |progress| {
                let _ = progress_tx.send(CloudSyncDelivery::Progress(progress));
            };
            let result = match preview {
                CloudSyncPendingPreview::Structured(preview) => service
                    .apply_structured_preview(
                        &mut connection_store,
                        &forwarding_registry,
                        &mut settings_store,
                        &settings,
                        preview,
                        &mut provider,
                        Some(&mut progress),
                    )
                    .map(|outcome| {
                        CloudSyncApplyOutcome::Structured(
                            outcome.expect("cloud sync structured apply unexpectedly skipped"),
                        )
                    }),
                CloudSyncPendingPreview::Legacy(preview) => service
                    .apply_legacy_preview(
                        &mut connection_store,
                        &settings,
                        &preview,
                        &mut provider,
                        settings.default_conflict_strategy.clone(),
                        Some(&mut progress),
                    )
                    .map(|outcome| CloudSyncApplyOutcome::Legacy {
                        preview,
                        outcome: outcome.expect("cloud sync legacy apply unexpectedly skipped"),
                    }),
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
            if matches!(
                self.cloud_sync_store.state().status,
                CloudSyncStatus::Uploading | CloudSyncStatus::Checking
            ) {
                self.cloud_sync_store.state_mut().status = CloudSyncStatus::Idle;
                self.save_cloud_sync_state();
            }
        }
        cx.notify();
    }

    fn handle_cloud_sync_delivery(&mut self, delivery: CloudSyncDelivery, cx: &mut Context<Self>) {
        match delivery {
            CloudSyncDelivery::Progress(progress) => {
                self.cloud_sync_progress = Some(progress);
            }
            CloudSyncDelivery::CheckFinished(action) => {
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(metadata) => self.finish_cloud_sync_check(metadata),
                    Err(error) => self.finish_cloud_sync_error("check", error),
                }
            }
            CloudSyncDelivery::UploadFinished(action) => {
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(outcome) => self.finish_cloud_sync_upload(outcome),
                    Err(error) => self.finish_cloud_sync_error("upload", error),
                }
            }
            CloudSyncDelivery::PullPreviewFinished(action) => {
                self.cloud_sync_store.state_mut().secret_hints = action.secret_hints;
                match action.result {
                    Ok(preview) => self.finish_cloud_sync_pull_preview(preview),
                    Err(error) => self.finish_cloud_sync_error("pull", error),
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
        let previous_remote_sections = self
            .cloud_sync_store
            .state()
            .last_synced_remote_sections
            .clone();
        if let Some(metadata) = metadata {
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
                    return true;
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
            } else if self.cloud_sync_store.state().remote_exists {
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
            self.cloud_sync_store.state_mut().status = status;
            self.cloud_sync_store.state_mut().conflict_details = conflict_details;
        } else {
            self.cloud_sync_store.state_mut().status = CloudSyncStatus::Idle;
        }
        self.cloud_sync_store.state_mut().last_check_at = Some(now);
        self.cloud_sync_store.state_mut().last_error = None;
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_upload(&mut self, outcome: UploadOutcome) {
        let now = Utc::now().to_rfc3339();
        let remote_sections = build_manifest_section_revisions(&outcome.manifest);
        {
            let state = self.cloud_sync_store.state_mut();
            state.status = CloudSyncStatus::Idle;
            state.last_error = None;
            state.last_sync_at = Some(now.clone());
            state.last_upload_at = Some(now);
            state.last_known_remote_revision = Some(outcome.manifest.revision.clone());
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
                history_summary_from_snapshot(&outcome.local_snapshot),
                true,
                None,
                Some(outcome.manifest.revision),
            ));
        }
        self.cloud_sync_progress = None;
        self.cloud_sync_pending_preview = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_pull_preview(&mut self, preview: CloudSyncPendingPreview) {
        let remote_metadata = match &preview {
            CloudSyncPendingPreview::Structured(preview) => &preview.remote_metadata,
            CloudSyncPendingPreview::Legacy(preview) => &preview.remote_metadata,
        };
        persist_remote_metadata(self.cloud_sync_store.state_mut(), remote_metadata);
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::RemoteUpdate;
        self.cloud_sync_store.state_mut().last_error = None;
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
            CloudSyncApplyOutcome::Legacy { preview, outcome } => {
                self.finish_legacy_cloud_sync_apply(preview, outcome, cx)
            }
        }
    }

    fn finish_structured_cloud_sync_apply(&mut self, outcome: ApplyStructuredPreviewOutcome) {
        let now = Utc::now().to_rfc3339();
        let remote_sections = outcome.manifest.section_revisions.clone();
        {
            let state = self.cloud_sync_store.state_mut();
            state.status = CloudSyncStatus::Idle;
            state.last_error = None;
            state.last_sync_at = Some(now);
            state.last_known_remote_revision = Some(outcome.manifest.revision.clone());
            state.last_known_remote_etag = outcome.remote_metadata.etag.clone();
            state.remote_format = Some(outcome.manifest.format.clone());
            state.remote_section_revisions = Some(remote_sections.clone());
            state.remote_updated_at = Some(outcome.manifest.uploaded_at.clone());
            state.remote_device_id = Some(outcome.manifest.device_id.clone());
            state.remote_exists = true;
            state.last_synced_local_metadata = Some(outcome.local_snapshot.metadata.clone());
            state.last_synced_structured_state = Some(merge_structured_baseline(
                state.last_synced_structured_state.as_ref(),
                &outcome.local_snapshot.dirty.current_state,
                &outcome.selection,
            ));
            state.last_synced_remote_sections = Some(remote_sections);
            state.local_dirty = false;
            state.local_dirty_sections = Some(outcome.local_snapshot.dirty.dirty_sections.clone());
            state.auto_upload_blocked_by_conflict = false;
            state.conflict_details = None;
            state.append_history(CloudSyncHistoryEntry::new(
                "pull",
                history_summary_from_snapshot(&outcome.local_snapshot),
                true,
                None,
                Some(outcome.manifest.revision),
            ));
        }
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_legacy_cloud_sync_apply(
        &mut self,
        preview: LegacyPreview,
        mut outcome: ApplyLegacyPreviewOutcome,
        cx: &mut Context<Self>,
    ) {
        let options = OxideClientStateImportOptions {
            oxide_options: oxideterm_connections::oxide_file::OxideImportOptions {
                conflict_strategy: import_strategy_from_cloud_settings(
                    self.cloud_sync_store
                        .state()
                        .settings
                        .default_conflict_strategy
                        .clone(),
                ),
                import_forwards: true,
                import_portable_secrets: true,
                ..oxideterm_connections::oxide_file::OxideImportOptions::default()
            },
            import_quick_commands: true,
            quick_command_strategy: QuickCommandImportStrategy::Merge,
            import_plugin_settings: true,
            selected_plugin_ids: None,
            import_app_settings: true,
            selected_app_settings_sections: None,
        };
        let imported_forwards = self.apply_oxide_import_forward_records(&mut outcome.envelope);
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
        self.apply_oxide_import_portable_secrets(&mut outcome.envelope);

        let local_snapshot = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            None,
            Some(&self.cloud_sync_store.state().sync_scope),
        );
        let now = Utc::now().to_rfc3339();
        {
            let state = self.cloud_sync_store.state_mut();
            state.status = CloudSyncStatus::Idle;
            state.last_error = None;
            state.last_sync_at = Some(now);
            state.last_known_remote_revision = preview.remote_metadata.revision.clone();
            state.last_known_remote_etag = preview.remote_metadata.etag.clone();
            state.remote_format = preview.remote_metadata.format.clone();
            state.remote_section_revisions = preview.remote_metadata.section_revisions.clone();
            state.remote_updated_at = preview.remote_metadata.uploaded_at.clone();
            state.remote_device_id = preview.remote_metadata.device_id.clone();
            state.remote_exists = preview.remote_metadata.exists;
            if let Ok(snapshot) = local_snapshot.as_ref() {
                state.last_synced_local_metadata = Some(snapshot.metadata.clone());
                state.last_synced_structured_state = Some(snapshot.dirty.current_state.clone());
                state.local_dirty = false;
                state.local_dirty_sections = Some(snapshot.dirty.dirty_sections.clone());
            }
            state.auto_upload_blocked_by_conflict = false;
            state.conflict_details = None;
            state.append_history(CloudSyncHistoryEntry::new(
                "pull",
                history_summary_from_legacy_preview(&preview),
                true,
                None,
                preview.remote_metadata.revision.clone(),
            ));
        }
        self.cloud_sync_pending_preview = None;
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
    }

    fn finish_cloud_sync_error(&mut self, action: &str, error: String) {
        let remote_revision = self
            .cloud_sync_store
            .state()
            .last_known_remote_revision
            .clone();
        self.cloud_sync_store.state_mut().status = CloudSyncStatus::Error;
        self.cloud_sync_store.state_mut().last_error = Some(error.clone());
        self.cloud_sync_store
            .state_mut()
            .append_history(CloudSyncHistoryEntry::new(
                action,
                CloudSyncHistorySummary::default(),
                false,
                Some(error),
                remote_revision,
            ));
        self.cloud_sync_progress = None;
        self.save_cloud_sync_state();
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
                self.i18n.t("plugin.cloud_sync.progress.uploading")
            }
            CloudSyncProgressStage::Downloading => {
                self.i18n.t("plugin.cloud_sync.progress.downloading")
            }
            CloudSyncProgressStage::PreviewingImport => {
                self.i18n.t("plugin.cloud_sync.progress.previewing")
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

fn history_summary_from_snapshot(snapshot: &CloudSyncLocalSnapshot) -> CloudSyncHistorySummary {
    CloudSyncHistorySummary {
        connections: snapshot.connections_record_count,
        forwards: snapshot.forwards_record_count,
        has_app_settings: snapshot.scope.sync_app_settings,
        plugin_settings_count: snapshot.metadata.plugin_settings_revisions.len(),
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

fn count_structured_preview_units(preview: &StructuredPreview) -> usize {
    usize::from(preview.connections_snapshot.is_some())
        + usize::from(preview.forwards_snapshot.is_some())
        + preview.app_settings_entries.len()
        + preview.plugin_settings_entries.len()
}

fn count_legacy_preview_units(preview: &LegacyPreview) -> usize {
    usize::from(preview.metadata.num_connections > 0)
        + usize::from(preview.preview.total_forwards > 0)
        + usize::from(preview.preview.has_app_settings)
        + usize::from(preview.preview.plugin_settings_count > 0)
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
