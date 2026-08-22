// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

const CLOUD_SYNC_TAB_BAR_WIDTH: f32 = 396.0; // Three equal header tabs leave room for translated labels.
const CLOUD_SYNC_BODY_LINE_HEIGHT: f32 = 20.0; // Compact body copy must not override page-header text metrics.

impl WorkspaceApp {
    pub(in crate::workspace) fn open_cloud_sync_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::CloudSync)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::CloudSync,
                    title: self.i18n.t("plugin.cloud_sync.panel_title"),
                    title_source: TabTitleSource::I18nKey("plugin.cloud_sync.panel_title"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window, cx);
        self.persist_sidebar_settings(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn render_cloud_sync_sidebar_content(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

    pub(in crate::workspace) fn render_cloud_sync_surface(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.invalidate_cloud_sync_select_if_needed(cx);

        let theme = self.tokens.ui;
        let has_background = self.cloud_sync_has_background();
        self.cloud_sync.update(cx, |cloud_sync, _cx| {
            cloud_sync.sync_section_rows();
        });
        let state = self.cloud_sync.read(cx).view.section_list_state.clone();
        let spec = CloudSyncWorkspaceEntity::section_list_spec();
        let scroll_cloud_sync = self.cloud_sync.clone();
        let renderer = self.cloud_sync_page_renderer(cx);

        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id("cloud-sync-scroll")
                    .size_full()
                    .on_scroll_wheel(move |_event, _window, cx| {
                        scroll_cloud_sync.update(cx, |cloud_sync, cx| {
                            cloud_sync.close_select_for_scroll(cx);
                        });
                    })
                    .bg(cloud_sync_root_bg(theme.bg, has_background))
                    .text_color(rgb(theme.text))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| renderer.render_section_item(index, cx),
                    )),
            )
            .when_some(
                self.render_cloud_sync_select_overlay(cx),
                |surface, overlay| surface.child(overlay),
            )
            .into_any_element()
    }
}

impl CloudSyncPageRenderer {
    pub(super) fn intent_listener(
        &self,
        intent: CloudSyncUiIntent,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let cloud_sync = self.cloud_sync.clone();
        move |_event, _window, cx| {
            cloud_sync.update(cx, |_cloud_sync, cx| {
                cx.emit(CloudSyncWorkspaceEvent::UiIntent(intent.clone()));
                cx.stop_propagation();
            });
        }
    }

    pub(super) fn render_display_text_with_role(
        &self,
        role: SelectableTextRole,
        scope: &str,
        key: impl std::hash::Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        self.render
            .selectable_text(role, scope, key, text, color, cx)
    }

    pub(super) fn render_selectable_text_scoped(
        &self,
        scope: &str,
        key: impl std::hash::Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        self.render_display_text_with_role(
            SelectableTextRole::PlainDocument,
            scope,
            key,
            text,
            color,
            cx,
        )
    }

    pub(super) fn render_selectable_text(
        &self,
        id: u64,
        text: impl Into<String>,
        color: u32,
        cx: &mut App,
    ) -> AnyElement {
        self.render_display_text_with_role(
            SelectableTextRole::PlainDocument,
            "cloud-sync-text",
            id,
            text,
            color,
            cx,
        )
    }

    pub(super) fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        WorkspaceApp::render_lucide_icon(icon, size, color)
    }

    pub(super) fn cloud_sync_has_background(&self) -> bool {
        self.has_background
    }

    pub(super) fn i18n_replace(&self, key: &str, replacements: &[(&str, String)]) -> String {
        self.render.replace(key, replacements)
    }

    pub(super) fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: oxideterm_gpui_ui::button::ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Div {
        let actionable = !(options.button.disabled || options.loading);
        oxideterm_gpui_ui::button::toolbar_button(&self.tokens, label, icon, options)
            .when(actionable, |button| {
                button.on_mouse_down(MouseButton::Left, listener)
            })
            .when(!actionable, |button| {
                button.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation();
                })
            })
    }

    fn render_section_item(&self, index: usize, cx: &mut App) -> AnyElement {
        let Some((section, section_count)) = self.cloud_sync.read(cx).section_at(index) else {
            return div().into_any_element();
        };
        let padding = self.tokens.metrics.settings_content_padding;
        let gap = self.tokens.metrics.settings_page_gap;
        let mut content = div().w_full().min_w(px(0.0)).px(px(padding)).pb(px(gap));
        if index == 0 {
            content = content.pt(px(padding));
        }
        if index + 1 == section_count {
            content = content.pb(px(padding));
        }
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .when(section != CloudSyncSection::Header, |item| {
                item.line_height(px(CLOUD_SYNC_BODY_LINE_HEIGHT))
            })
            .child(content.child(self.render_section(section, cx)))
            .into_any_element()
    }

    fn render_section(&self, section: CloudSyncSection, cx: &mut App) -> AnyElement {
        let (busy, active_tab, backend_type) = {
            let cloud_sync = self.cloud_sync.read(cx);
            (
                cloud_sync.controller.delivery_rx.is_some(),
                cloud_sync.view.active_tab,
                cloud_sync.view.form.backend_type.clone(),
            )
        };
        match section {
            CloudSyncSection::Header => self.render_cloud_sync_header(cx),
            CloudSyncSection::Guide if active_tab == CloudSyncTab::Configure => {
                self.render_cloud_sync_guide(&backend_type, cx)
            }
            CloudSyncSection::Status if active_tab == CloudSyncTab::Overview => {
                self.render_cloud_sync_overview_card(busy, cx)
            }
            CloudSyncSection::Preview => self.render_pending_preview(busy, cx),
            CloudSyncSection::RecentHistory if active_tab == CloudSyncTab::Overview => {
                self.render_cloud_sync_recent_history(cx)
            }
            CloudSyncSection::Rollback => {
                let render = Arc::clone(&self.render);
                self.cloud_sync
                    .update(cx, |cloud_sync, cx| match active_tab {
                        CloudSyncTab::Overview => {
                            cloud_sync.render_recent_rollback_backups(render, busy, cx)
                        }
                        CloudSyncTab::History => {
                            cloud_sync.render_rollback_backup_list(render, busy, cx)
                        }
                        CloudSyncTab::Configure => div().into_any_element(),
                    })
            }
            CloudSyncSection::History if active_tab == CloudSyncTab::History => {
                let render = Arc::clone(&self.render);
                self.cloud_sync.update(cx, |cloud_sync, cx| {
                    cloud_sync.render_history_list(render, cx)
                })
            }
            CloudSyncSection::ConfigConnection if active_tab == CloudSyncTab::Configure => {
                self.render_cloud_sync_config_connection_card(cx)
            }
            CloudSyncSection::ConfigScope if active_tab == CloudSyncTab::Configure => {
                self.render_cloud_sync_scope_card(cx)
            }
            CloudSyncSection::ConfigCoverage if active_tab == CloudSyncTab::Configure => {
                self.render_cloud_sync_coverage_card(cx)
            }
            CloudSyncSection::ConfigPreflight if active_tab == CloudSyncTab::Configure => {
                self.render_cloud_sync_config_preflight_card(cx)
            }
            CloudSyncSection::ConfigHealth if active_tab == CloudSyncTab::Configure => {
                self.render_cloud_sync_health_card(cx)
            }
            CloudSyncSection::ConfigNotes if active_tab == CloudSyncTab::Configure => {
                self.render_cloud_sync_notes(cx)
            }
            CloudSyncSection::Actions
            | CloudSyncSection::Guide
            | CloudSyncSection::Status
            | CloudSyncSection::RecentHistory
            | CloudSyncSection::History
            | CloudSyncSection::ConfigConnection
            | CloudSyncSection::ConfigScope
            | CloudSyncSection::ConfigCoverage
            | CloudSyncSection::ConfigPreflight
            | CloudSyncSection::ConfigHealth
            | CloudSyncSection::ConfigNotes => div().into_any_element(),
        }
    }
}

impl WorkspaceApp {
    fn invalidate_cloud_sync_select_if_needed(&mut self, cx: &mut Context<Self>) {
        let Some(open_select) = self.cloud_sync.read(cx).view.open_select else {
            return;
        };
        let anchor_valid = self
            .select_anchors
            .contains_key(&Self::cloud_sync_select_anchor_id(open_select));
        if !anchor_valid {
            // Invalid live geometry closes the dropdown immediately.
            self.cloud_sync.update(cx, |cloud_sync, _cx| {
                cloud_sync.view.open_select = None;
            });
        }
    }

    pub(super) fn cloud_sync_has_background(&self) -> bool {
        self.background_surface_active("cloud_sync")
    }

    pub(super) fn cloud_sync_local_snapshot(
        &self,
        state: &CloudSyncPersistedState,
        cx: &App,
    ) -> std::result::Result<CloudSyncLocalSnapshot, String> {
        let generation = self
            .cloud_sync
            .read(cx)
            .view
            .snapshot_cache_generation
            .get();
        if let Some(cache) = self
            .cloud_sync
            .read(cx)
            .view
            .local_snapshot_cache
            .borrow()
            .as_ref()
        {
            if cache.generation == generation {
                return cache.result.clone();
            }
        }
        let result = build_local_snapshot(
            &self.connection_store,
            self.forwarding_service.registry(),
            &self.settings_store,
            state.last_synced_structured_state.as_ref(),
            Some(&state.sync_scope),
        )
        .map_err(|error| error.to_string());
        *self
            .cloud_sync
            .read(cx)
            .view
            .local_snapshot_cache
            .borrow_mut() = Some(CloudSyncLocalSnapshotCache {
            generation,
            result: result.clone(),
        });
        result
    }

    pub(super) fn cloud_sync_upload_diff_items_cached(
        &self,
        snapshot: &CloudSyncLocalSnapshot,
        state: &CloudSyncPersistedState,
        cx: &App,
    ) -> Vec<CloudSyncSectionDiffItem> {
        let generation = self
            .cloud_sync
            .read(cx)
            .view
            .snapshot_cache_generation
            .get();
        if let Some(cache) = self
            .cloud_sync
            .read(cx)
            .view
            .upload_diff_cache
            .borrow()
            .as_ref()
        {
            if cache.generation == generation {
                return cache.items.clone();
            }
        }
        let items = cloud_sync_upload_diff_items(snapshot, state);
        *self.cloud_sync.read(cx).view.upload_diff_cache.borrow_mut() =
            Some(CloudSyncUploadDiffCache {
                generation,
                items: items.clone(),
            });
        items
    }

    pub(in crate::workspace) fn invalidate_cloud_sync_snapshot_caches(&self, cx: &App) {
        // Source mutations advance one explicit generation so render-time cache
        // hits never need to rescan stores or query filesystem metadata.
        self.cloud_sync.read(cx).view.snapshot_cache_generation.set(
            self.cloud_sync
                .read(cx)
                .view
                .snapshot_cache_generation
                .get()
                .wrapping_add(1),
        );
        self.cloud_sync
            .read(cx)
            .view
            .local_snapshot_cache
            .borrow_mut()
            .take();
        self.cloud_sync
            .read(cx)
            .view
            .upload_diff_cache
            .borrow_mut()
            .take();
    }

    pub(super) fn cloud_sync_local_field_diff_snapshot(
        &self,
        cx: &App,
    ) -> CloudSyncLocalFieldDiffSnapshot {
        let scope = normalize_sync_scope(
            Some(&self.cloud_sync.read(cx).controller.store.state().sync_scope),
            &[],
        );
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
                .forwarding_service
                .registry()
                .export_saved_forwards_snapshot()
                .ok(),
            quick_commands,
            serial_profiles: self.connection_store.export_serial_profiles_snapshot().ok(),
            telnet_profiles: self.connection_store.export_telnet_profiles_snapshot().ok(),
            mosh_profiles: self.connection_store.export_mosh_profiles_snapshot().ok(),
            remote_desktop_profiles: self
                .connection_store
                .export_remote_desktop_profiles_snapshot()
                .ok(),
            app_settings_sections,
        }
    }
}

impl CloudSyncPageRenderer {
    pub(super) fn render_cloud_sync_header(&self, cx: &mut App) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .min_w(px(280.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_2xl))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "cloud-sync-panel",
                                        "title",
                                        self.i18n.t("plugin.cloud_sync.panel_title"),
                                        theme.text_heading,
                                        cx,
                                    )),
                            )
                            .child(
                                div()
                                    .max_w(px(680.0))
                                    .text_size(px(self.tokens.metrics.ui_text_base))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.render_display_text_with_role(
                                        SelectableTextRole::PlainDocument,
                                        "cloud-sync-panel",
                                        "subtitle",
                                        self.i18n.t("plugin.cloud_sync.native_description"),
                                        theme.text_muted,
                                        cx,
                                    )),
                            ),
                    )
                    .child(self.render_cloud_sync_tab_bar(cx)),
            )
            .child(
                // Match the Plugin Manager header rhythm with a full-width rule.
                div().w_full().h(px(1.0)).bg(rgb(theme.border)),
            )
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_tab_bar(&self, cx: &mut App) -> AnyElement {
        let theme = self.tokens.ui;
        let render_tab = |tab: CloudSyncTab,
                          icon: LucideIcon,
                          label_key: &'static str,
                          active: bool,
                          this: &Self|
         -> AnyElement {
            let content = div()
                .w_full()
                .py(px(2.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(7.0))
                .child(Self::render_lucide_icon(
                    icon,
                    16.0,
                    rgb(if active {
                        theme.accent
                    } else {
                        theme.text_muted
                    }),
                ))
                .child(this.i18n.t(label_key));
            oxideterm_gpui_ui::segmented_control_item_content(
                &this.tokens,
                active,
                content.into_any_element(),
            )
            .on_mouse_down(
                MouseButton::Left,
                this.intent_listener(CloudSyncUiIntent::SelectTab { tab }),
            )
            .into_any_element()
        };

        let items = vec![
            render_tab(
                CloudSyncTab::Overview,
                LucideIcon::Cloud,
                "plugin.cloud_sync.tabs.overview",
                self.cloud_sync.read(cx).view.active_tab == CloudSyncTab::Overview,
                self,
            ),
            render_tab(
                CloudSyncTab::Configure,
                LucideIcon::Settings,
                "plugin.cloud_sync.tabs.configure",
                self.cloud_sync.read(cx).view.active_tab == CloudSyncTab::Configure,
                self,
            ),
            render_tab(
                CloudSyncTab::History,
                LucideIcon::Clock,
                "plugin.cloud_sync.tabs.history",
                self.cloud_sync.read(cx).view.active_tab == CloudSyncTab::History,
                self,
            ),
        ];
        let active_index = cloud_sync_tab_index(self.cloud_sync.read(cx).view.active_tab);
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            selection_motion::CLOUD_SYNC_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(
                active_index,
                cloud_sync_tab_index(self.cloud_sync.read(cx).view.previous_tab),
                3,
            )
            .user_transition_active(self.tab_transition_active)
            .has_background_image(self.cloud_sync_has_background())
            .compact(CLOUD_SYNC_TAB_BAR_WIDTH),
            items,
        )
        .into_any_element()
        .into_any_element()
    }

    pub(super) fn render_cloud_sync_overview_card(&self, busy: bool, cx: &mut App) -> AnyElement {
        let cloud_sync_entity = self.cloud_sync.clone();
        cloud_sync_entity.update(cx, |cloud_sync, cx| {
            self.render_cloud_sync_overview_card_from_entity(cloud_sync, busy, cx)
        })
    }

    fn render_cloud_sync_overview_card_from_entity(
        &self,
        cloud_sync: &CloudSyncWorkspaceEntity,
        busy: bool,
        cx: &mut App,
    ) -> AnyElement {
        let state = cloud_sync.controller.store.state();
        let theme = self.tokens.ui;
        let settings = &state.settings;
        let local_snapshot = self.local_snapshot.as_ref().ok().map(Arc::as_ref);
        let backend_label = self
            .i18n
            .t(cloud_sync_backend_label_key(&settings.backend_type));
        let local_dirty = local_snapshot
            .map(|snapshot| {
                if snapshot.dirty.has_dirty {
                    self.i18n.t("plugin.cloud_sync.common.yes")
                } else {
                    self.i18n.t("plugin.cloud_sync.common.no")
                }
            })
            .unwrap_or_else(|| self.i18n.t("plugin.cloud_sync.common.error"));
        let last_sync = state
            .last_sync_at
            .as_deref()
            .map(cloud_sync_format_timestamp)
            .unwrap_or_else(|| "—".to_string());
        let has_rollback_backup = !state.rollback_backups.is_empty();
        let show_github_oauth = matches!(settings.backend_type, BackendType::GithubGist);
        let github_oauth_disabled = busy || settings.github_oauth_client_id.trim().is_empty();
        let show_microsoft_oauth = matches!(settings.backend_type, BackendType::OneDrive);
        let microsoft_oauth_disabled = busy || settings.microsoft_oauth_client_id.trim().is_empty();
        let show_google_oauth = matches!(settings.backend_type, BackendType::GoogleDrive);
        let google_oauth_disabled = busy || settings.google_oauth_client_id.trim().is_empty();

        let mut card = self
            .cloud_sync_plugin_card(self.has_background)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(
                                self.i18n
                                    .t("plugin.cloud_sync.tabs.overview")
                                    .to_uppercase(),
                            ),
                    )
                    .child(
                        self.render_cloud_sync_status_chip(
                            self.i18n
                                .t(cloud_sync_status_label_key(state.status.clone())),
                            CloudSyncTone::Accent,
                        ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_wrap()
                            .gap(px(10.0))
                            .child(self.render_cloud_sync_overview_fact(
                                LucideIcon::Server,
                                "plugin.cloud_sync.fields.backend",
                                backend_label,
                                cx,
                            ))
                            .child(self.render_cloud_sync_overview_fact(
                                LucideIcon::Hash,
                                "plugin.cloud_sync.fields.namespace",
                                settings.namespace.clone(),
                                cx,
                            ))
                            .child(self.render_cloud_sync_overview_fact(
                                LucideIcon::Activity,
                                "plugin.cloud_sync.fields.local_dirty",
                                local_dirty,
                                cx,
                            ))
                            .child(self.render_cloud_sync_overview_fact(
                                LucideIcon::Clock,
                                "plugin.cloud_sync.fields.last_sync",
                                last_sync,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(300.0))
                            .flex_1()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .when(show_github_oauth, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    LucideIcon::KeyRound,
                                    "plugin.cloud_sync.actions.github_oauth_login",
                                    CloudSyncActionTone::Muted,
                                    github_oauth_disabled,
                                    self.intent_listener(CloudSyncUiIntent::StartGithubOauth),
                                ))
                            })
                            .when(show_microsoft_oauth, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    LucideIcon::KeyRound,
                                    "plugin.cloud_sync.actions.microsoft_oauth_login",
                                    CloudSyncActionTone::Muted,
                                    microsoft_oauth_disabled,
                                    self.intent_listener(CloudSyncUiIntent::StartMicrosoftOauth),
                                ))
                            })
                            .when(show_google_oauth, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    LucideIcon::KeyRound,
                                    "plugin.cloud_sync.actions.google_oauth_login",
                                    CloudSyncActionTone::Muted,
                                    google_oauth_disabled,
                                    self.intent_listener(CloudSyncUiIntent::StartGoogleOauth),
                                ))
                            })
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::Upload,
                                "plugin.cloud_sync.actions.upload_now",
                                CloudSyncActionTone::Accent,
                                busy,
                                self.intent_listener(CloudSyncUiIntent::StartUploadPreview),
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::RefreshCw,
                                "plugin.cloud_sync.actions.check_remote",
                                CloudSyncActionTone::Muted,
                                busy,
                                self.intent_listener(CloudSyncUiIntent::CheckRemote),
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::Download,
                                "plugin.cloud_sync.actions.pull_preview",
                                CloudSyncActionTone::Muted,
                                busy,
                                self.intent_listener(CloudSyncUiIntent::PullPreview),
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::RotateCcw,
                                "plugin.cloud_sync.actions.restore_backup",
                                CloudSyncActionTone::Muted,
                                busy || !has_rollback_backup,
                                self.intent_listener(CloudSyncUiIntent::RestoreLatestBackup),
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::Save,
                                "plugin.cloud_sync.actions.save_settings",
                                CloudSyncActionTone::Muted,
                                busy,
                                self.intent_listener(CloudSyncUiIntent::SaveConfiguration),
                            )),
                    ),
            )
            .child(
                div()
                    .pt(px(12.0))
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w(px(240.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("plugin.cloud_sync.sections.local_backup")),
                            )
                            .child(
                                div().text_color(rgb(theme.text_muted)).child(
                                    self.i18n
                                        .t("plugin.cloud_sync.sections.local_backup_description"),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::Upload,
                                "plugin.cloud_sync.actions.import_local",
                                CloudSyncActionTone::Muted,
                                busy,
                                self.intent_listener(CloudSyncUiIntent::ImportLocalBackup),
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::Download,
                                "plugin.cloud_sync.actions.export_local",
                                CloudSyncActionTone::Muted,
                                busy,
                                self.intent_listener(CloudSyncUiIntent::ExportLocalBackup),
                            )),
                    ),
            );

        if let Some(progress) = cloud_sync.controller.progress.as_ref() {
            card = card.child(self.render_cloud_sync_progress(progress, cx));
        }
        if let Some(error) = state.last_error.as_ref() {
            card = card.child(self.render_cloud_sync_error(error));
        }
        card.child(self.render_cloud_sync_meta(state, local_snapshot, cx))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_overview_fact(
        &self,
        icon: LucideIcon,
        label_key: &'static str,
        value: String,
        cx: &mut App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key);
        div()
            .min_w(px(170.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(icon, 15.0, rgb(theme.accent)))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(div().text_color(rgb(theme.text_muted)).child(
                        self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "cloud-sync-overview-fact-label",
                            label_key,
                            label,
                            theme.text_muted,
                            cx,
                        ),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(self.render_selectable_text(
                                crate::workspace::selectable_text::selectable_text_id(
                                    "cloud-sync-overview-fact-value",
                                    (label_key, &value),
                                ),
                                value,
                                theme.text,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_guide(
        &self,
        backend_type: &BackendType,
        cx: &mut App,
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
            self.cloud_sync_has_background(),
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
            self.mono_font_family.clone(),
        )
    }

    pub(super) fn render_cloud_sync_guide_steps(&self, cx: &mut App) -> AnyElement {
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

    pub(super) fn render_cloud_sync_section_title(&self, key: &str, cx: &mut App) -> AnyElement {
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

    pub(super) fn render_cloud_sync_action_button(
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

    pub(super) fn cloud_sync_plugin_card(&self, has_background: bool) -> Div {
        semantic_surface(
            &self.tokens,
            SurfaceOptions::new(SurfaceKind::Inspector)
                .padding(SurfacePadding::Spacious)
                .has_background_image(has_background),
        )
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(16.0))
    }

    pub(super) fn render_cloud_sync_toolbar_button(
        &self,
        icon: LucideIcon,
        label_key: &'static str,
        tone: CloudSyncActionTone,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let color = if disabled {
            theme.text_muted
        } else {
            tone.color(&self.tokens)
        };
        let bg = match tone {
            CloudSyncActionTone::Accent if !disabled => {
                cloud_sync_theme_alpha(theme.accent, CLOUD_SYNC_TW_ALPHA_10)
            }
            _ => cloud_sync_theme_panel_bg(theme.bg_panel, self.cloud_sync_has_background()),
        };
        let mut button = div()
            .min_w(px(120.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if disabled {
                cloud_sync_theme_border_half(theme.border, self.cloud_sync_has_background())
            } else {
                cloud_sync_theme_alpha(color, CLOUD_SYNC_TW_ALPHA_40)
            })
            .bg(bg)
            .px(px(12.0))
            .py(px(7.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(7.0))
            .whitespace_nowrap()
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .font_weight(FontWeight::MEDIUM)
            .text_color(if disabled {
                rgba((theme.text_muted << 8) | CLOUD_SYNC_TW_ALPHA_50)
            } else {
                rgb(color)
            })
            .child(Self::render_lucide_icon(
                icon,
                15.0,
                if disabled {
                    rgba((theme.text_muted << 8) | CLOUD_SYNC_TW_ALPHA_50)
                } else {
                    rgb(color)
                },
            ))
            .child(self.i18n.t(label_key));
        if disabled {
            button = button.cursor(CursorStyle::Arrow);
        } else {
            button = button
                .cursor(CursorStyle::PointingHand)
                .on_mouse_down(MouseButton::Left, listener);
        }
        button.into_any_element()
    }

    pub(super) fn render_cloud_sync_status_chip(
        &self,
        label: String,
        tone: CloudSyncTone,
    ) -> AnyElement {
        status_pill(
            &self.tokens,
            label,
            StatusPillOptions::new(cloud_sync_status_tone(tone)).strong(),
        )
        .into_any_element()
    }

    pub(super) fn render_cloud_sync_progress(
        &self,
        progress: &CloudSyncProgress,
        cx: &mut App,
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

    pub(super) fn render_cloud_sync_error(&self, error: &str) -> AnyElement {
        cloud_sync_error_view(&self.tokens, self.format_cloud_sync_error(error))
    }

    pub(super) fn format_cloud_sync_error(&self, error: &str) -> String {
        match cloud_sync_error_message_spec(error) {
            CloudSyncErrorMessageSpec::Raw(message) => message,
            CloudSyncErrorMessageSpec::Key(key) => self.i18n.t(key),
            CloudSyncErrorMessageSpec::KeyWithDetail { key, detail } => {
                format!("{} ({detail})", self.i18n.t(key))
            }
            CloudSyncErrorMessageSpec::SnapshotTooLarge { limit } => self.i18n_replace(
                "plugin.cloud_sync.errors.snapshot_too_large",
                &[("limit", limit.unwrap_or_else(|| "—".to_string()))],
            ),
        }
    }

    pub(super) fn render_cloud_sync_meta(
        &self,
        state: &CloudSyncPersistedState,
        local_snapshot: Option<&CloudSyncLocalSnapshot>,
        cx: &mut App,
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
        let version_block = self.render_cloud_sync_meta_group(
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
            block = block.child(self.render_cloud_sync_meta_group(conflict_title, rows));
        }
        block.into_any_element()
    }

    pub(super) fn render_cloud_sync_meta_group(
        &self,
        title: AnyElement,
        rows: impl IntoIterator<Item = AnyElement>,
    ) -> AnyElement {
        // Metadata already lives inside the overview card, so this group must not
        // introduce another bordered surface around its rows.
        rows.into_iter()
            .fold(
                div()
                    .w_full()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(self.tokens.ui.text_heading))
                            .child(title),
                    ),
                |group, row| group.child(row),
            )
            .into_any_element()
    }

    pub(super) fn cloud_sync_conflict_plain_summary(
        &self,
        state: &CloudSyncPersistedState,
    ) -> String {
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

    pub(super) fn render_cloud_sync_meta_line(
        &self,
        label_key: &str,
        value: String,
        cx: &mut App,
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
}
