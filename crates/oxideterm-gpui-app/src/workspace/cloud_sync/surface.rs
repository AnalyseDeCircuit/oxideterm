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
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.main_window_tabs.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle, cx);
        self.reveal_active_tab(window);
        self.persist_sidebar_settings();
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
        self.poll_cloud_sync_delivery(cx);
        self.invalidate_cloud_sync_select_if_needed();

        let theme = self.tokens.ui;
        let has_background = self.cloud_sync_has_background();
        self.sync_cloud_sync_section_list_state();
        let state = self.cloud_sync.view.section_list_state.clone();
        let spec = self.cloud_sync_section_list_spec();
        let workspace = cx.entity();

        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id("cloud-sync-scroll")
                    .size_full()
                    .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                        if this.close_cloud_sync_select_for_scroll() {
                            cx.notify();
                        }
                    }))
                    .bg(cloud_sync_root_bg(theme.bg, has_background))
                    .text_color(rgb(theme.text))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            workspace.update(cx, |this, cx| {
                                this.render_cloud_sync_section_item(index, cx)
                            })
                        },
                    )),
            )
            .when_some(
                self.render_cloud_sync_select_overlay(cx),
                |surface, overlay| surface.child(overlay),
            )
            .into_any_element()
    }

    fn invalidate_cloud_sync_select_if_needed(&mut self) {
        let Some(open_select) = self.cloud_sync.view.open_select else {
            return;
        };
        let anchor_valid = self
            .select_anchors
            .contains_key(&Self::cloud_sync_select_anchor_id(open_select));
        if !anchor_valid {
            // Invalid live geometry closes the dropdown immediately.
            self.cloud_sync.view.open_select = None;
        }
    }

    pub(super) fn sync_cloud_sync_section_list_state(&mut self) {
        let spec = self.cloud_sync_section_list_spec();
        let signatures = self.cloud_sync_section_signatures();
        sync_tauri_variable_list_state_by_signatures(
            &self.cloud_sync.view.section_list_state,
            &mut self.cloud_sync.view.section_list_cache.borrow_mut(),
            "cloud-sync",
            &signatures,
            spec,
        );
    }

    pub(super) fn cloud_sync_section_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(CLOUD_SYNC_SECTION_LIST_ESTIMATED_HEIGHT),
            CLOUD_SYNC_SECTION_LIST_OVERSCAN,
        )
    }

    pub(super) fn cloud_sync_has_background(&self) -> bool {
        self.background_surface_active("cloud_sync")
    }

    pub(super) fn cloud_sync_sections(&self) -> Vec<CloudSyncSection> {
        cloud_sync_sections(
            self.cloud_sync.controller.store.state(),
            self.cloud_sync_has_pending_preview(),
            self.cloud_sync.view.active_tab,
        )
    }

    pub(super) fn cloud_sync_section_signatures(&self) -> Vec<u64> {
        self.cloud_sync_sections()
            .into_iter()
            .map(|section| self.cloud_sync_section_signature(section))
            .collect()
    }

    pub(super) fn cloud_sync_section_signature(&self, section: CloudSyncSection) -> u64 {
        cloud_sync_section_signature(
            section,
            self.cloud_sync.controller.store.state(),
            &self.cloud_sync.view.form.backend_type,
            &self.cloud_sync.view.form.auth_mode,
            &self.cloud_sync.view.form.default_conflict_strategy,
            self.cloud_sync.controller.delivery_rx.is_some(),
            self.cloud_sync_has_pending_preview(),
            self.cloud_sync.view.preview_selection.is_some(),
            self.cloud_sync.controller.progress.is_some(),
            self.cloud_sync.view.active_tab,
        )
    }

    pub(super) fn cloud_sync_has_pending_preview(&self) -> bool {
        self.cloud_sync.view.pending_preview.is_some()
            || self.cloud_sync.view.upload_preview.is_some()
    }

    pub(super) fn render_cloud_sync_section_item(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let sections = self.cloud_sync_sections();
        let Some(section) = sections.get(index).copied() else {
            return div().into_any_element();
        };
        let padding = self.tokens.metrics.settings_content_padding;
        let gap = self.tokens.metrics.settings_page_gap;
        let mut content = div().w_full().min_w(px(0.0)).px(px(padding)).pb(px(gap));
        if index == 0 {
            content = content.pt(px(padding));
        }
        if index + 1 == sections.len() {
            content = content.pb(px(padding));
        }
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .when(section != CloudSyncSection::Header, |item| {
                item.line_height(px(CLOUD_SYNC_BODY_LINE_HEIGHT))
            })
            .child(content.child(self.render_cloud_sync_section(section, cx)))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_section(
        &mut self,
        section: CloudSyncSection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.cloud_sync.controller.delivery_rx.is_some();
        let has_background = self.cloud_sync_has_background();
        match section {
            CloudSyncSection::Header => self.render_cloud_sync_header(cx),
            CloudSyncSection::Guide => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Configure {
                    self.render_cloud_sync_guide(&self.cloud_sync.view.form.backend_type, cx)
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::Status => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Overview {
                    self.render_cloud_sync_overview_card(
                        self.cloud_sync.controller.store.state(),
                        busy,
                        has_background,
                        cx,
                    )
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::Actions => div().into_any_element(),
            CloudSyncSection::Preview => {
                let state = self.cloud_sync.controller.store.state();
                if let Some(preview) = self.cloud_sync.view.upload_preview.as_ref() {
                    self.render_cloud_sync_upload_preview(preview, state, busy, cx)
                } else {
                    self.cloud_sync
                        .view
                        .pending_preview
                        .as_ref()
                        .map(|preview| self.render_cloud_sync_preview(preview, state, busy, cx))
                        .unwrap_or_else(|| div().into_any_element())
                }
            }
            CloudSyncSection::RecentHistory => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Overview {
                    self.render_cloud_sync_recent_history(cx)
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::Rollback => match self.cloud_sync.view.active_tab {
                CloudSyncTab::Overview => self.render_cloud_sync_recent_rollback_backups(busy, cx),
                CloudSyncTab::History => {
                    // History list setup mutates list state, so keep its snapshot local
                    // instead of cloning persisted data for every Cloud Sync row.
                    let state = self.cloud_sync.controller.store.state().clone();
                    self.render_cloud_sync_rollback_backups(&state, busy, cx)
                }
                CloudSyncTab::Configure => div().into_any_element(),
            },
            CloudSyncSection::History => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::History {
                    // The mutable nested list renderer requires an owned snapshot.
                    let state = self.cloud_sync.controller.store.state().clone();
                    self.render_cloud_sync_history(&state, cx)
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::ConfigConnection => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Configure {
                    self.render_cloud_sync_config_connection_card(cx)
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::ConfigScope => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Configure {
                    self.render_cloud_sync_scope_card(cx)
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::ConfigCoverage => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Configure {
                    self.render_cloud_sync_coverage_card(cx)
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::ConfigPreflight => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Configure {
                    self.render_cloud_sync_config_preflight_card(cx)
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::ConfigHealth => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Configure {
                    self.render_cloud_sync_health_card(cx)
                } else {
                    div().into_any_element()
                }
            }
            CloudSyncSection::ConfigNotes => {
                if self.cloud_sync.view.active_tab == CloudSyncTab::Configure {
                    self.render_cloud_sync_notes(cx)
                } else {
                    div().into_any_element()
                }
            }
        }
    }

    pub(super) fn cloud_sync_local_snapshot(
        &self,
        state: &CloudSyncPersistedState,
    ) -> std::result::Result<CloudSyncLocalSnapshot, String> {
        let generation = self.cloud_sync.view.snapshot_cache_generation.get();
        if let Some(cache) = self.cloud_sync.view.local_snapshot_cache.borrow().as_ref() {
            if cache.generation == generation {
                return cache.result.clone();
            }
        }
        let result = build_local_snapshot(
            &self.connection_store,
            &self.forwarding_registry,
            &self.settings_store,
            state.last_synced_structured_state.as_ref(),
            Some(&state.sync_scope),
        )
        .map_err(|error| error.to_string());
        *self.cloud_sync.view.local_snapshot_cache.borrow_mut() =
            Some(CloudSyncLocalSnapshotCache {
                generation,
                result: result.clone(),
            });
        result
    }

    pub(super) fn cloud_sync_upload_diff_items_cached(
        &self,
        snapshot: &CloudSyncLocalSnapshot,
        state: &CloudSyncPersistedState,
    ) -> Vec<CloudSyncSectionDiffItem> {
        let generation = self.cloud_sync.view.snapshot_cache_generation.get();
        if let Some(cache) = self.cloud_sync.view.upload_diff_cache.borrow().as_ref() {
            if cache.generation == generation {
                return cache.items.clone();
            }
        }
        let items = cloud_sync_upload_diff_items(snapshot, state);
        *self.cloud_sync.view.upload_diff_cache.borrow_mut() = Some(CloudSyncUploadDiffCache {
            generation,
            items: items.clone(),
        });
        items
    }

    pub(super) fn invalidate_cloud_sync_snapshot_caches(&self) {
        // Source mutations advance one explicit generation so render-time cache
        // hits never need to rescan stores or query filesystem metadata.
        self.cloud_sync.view.snapshot_cache_generation.set(
            self.cloud_sync
                .view
                .snapshot_cache_generation
                .get()
                .wrapping_add(1),
        );
        self.cloud_sync
            .view
            .local_snapshot_cache
            .borrow_mut()
            .take();
        self.cloud_sync.view.upload_diff_cache.borrow_mut().take();
    }

    pub(super) fn cloud_sync_local_field_diff_snapshot(&self) -> CloudSyncLocalFieldDiffSnapshot {
        let scope = normalize_sync_scope(
            Some(&self.cloud_sync.controller.store.state().sync_scope),
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
                .forwarding_registry
                .export_saved_forwards_snapshot()
                .ok(),
            quick_commands,
            serial_profiles: self.connection_store.export_serial_profiles_snapshot().ok(),
            app_settings_sections,
        }
    }

    pub(super) fn render_cloud_sync_header(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_cloud_sync_tab_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let render_tab = |tab: CloudSyncTab,
                          icon: LucideIcon,
                          label_key: &'static str,
                          active: bool,
                          this: &Self,
                          cx: &mut Context<Self>|
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
                cx.listener(move |this, _event, _window, cx| {
                    if this.cloud_sync.view.active_tab == CloudSyncTab::Configure
                        && tab != CloudSyncTab::Configure
                        && !this.persist_cloud_sync_configuration(false, cx)
                    {
                        cx.stop_propagation();
                        cx.notify();
                        return;
                    }
                    if this.cloud_sync.view.active_tab != tab {
                        this.cloud_sync.view.set_active_tab(tab);
                        this.begin_user_segmented_control_transition(
                            selection_motion::CLOUD_SYNC_SWITCHER_ID,
                            cloud_sync_tab_index(tab),
                            cx,
                        );
                    }
                    this.clear_cloud_sync_select_focus();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
        };

        let items = vec![
            render_tab(
                CloudSyncTab::Overview,
                LucideIcon::Cloud,
                "plugin.cloud_sync.tabs.overview",
                self.cloud_sync.view.active_tab == CloudSyncTab::Overview,
                self,
                cx,
            ),
            render_tab(
                CloudSyncTab::Configure,
                LucideIcon::Settings,
                "plugin.cloud_sync.tabs.configure",
                self.cloud_sync.view.active_tab == CloudSyncTab::Configure,
                self,
                cx,
            ),
            render_tab(
                CloudSyncTab::History,
                LucideIcon::Clock,
                "plugin.cloud_sync.tabs.history",
                self.cloud_sync.view.active_tab == CloudSyncTab::History,
                self,
                cx,
            ),
        ];
        let active_index = cloud_sync_tab_index(self.cloud_sync.view.active_tab);
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            selection_motion::CLOUD_SYNC_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(
                active_index,
                cloud_sync_tab_index(self.cloud_sync.view.previous_tab),
                3,
            )
            .user_transition_active(self.segmented_control_user_transition_active(
                selection_motion::CLOUD_SYNC_SWITCHER_ID,
                active_index,
            ))
            .has_background_image(self.cloud_sync_has_background())
            .compact(CLOUD_SYNC_TAB_BAR_WIDTH),
            items,
        )
        .into_any_element()
        .into_any_element()
    }

    pub(super) fn render_cloud_sync_overview_card(
        &self,
        state: &CloudSyncPersistedState,
        busy: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let settings = state.settings.clone();
        let local_snapshot = self.cloud_sync_local_snapshot(state);
        let backend_label = self
            .i18n
            .t(cloud_sync_backend_label_key(&settings.backend_type));
        let local_dirty = local_snapshot
            .as_ref()
            .map(|snapshot| {
                if snapshot.dirty.has_dirty {
                    self.i18n.t("plugin.cloud_sync.common.yes")
                } else {
                    self.i18n.t("plugin.cloud_sync.common.no")
                }
            })
            .unwrap_or_else(|_| self.i18n.t("plugin.cloud_sync.common.error"));
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
            .cloud_sync_plugin_card(has_background)
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
                            .child(self.i18n.t("plugin.cloud_sync.tabs.overview").to_uppercase()),
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
                                settings.namespace,
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
                                    cx.listener(
                                        |this: &mut WorkspaceApp,
                                         _event,
                                         _window,
                                         cx: &mut Context<WorkspaceApp>| {
                                            this.start_cloud_sync_github_oauth(cx);
                                            this.clear_cloud_sync_select_focus();
                                            cx.stop_propagation();
                                        },
                                    ),
                                ))
                            })
                            .when(show_microsoft_oauth, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    LucideIcon::KeyRound,
                                    "plugin.cloud_sync.actions.microsoft_oauth_login",
                                    CloudSyncActionTone::Muted,
                                    microsoft_oauth_disabled,
                                    cx.listener(
                                        |this: &mut WorkspaceApp,
                                         _event,
                                         _window,
                                         cx: &mut Context<WorkspaceApp>| {
                                            this.start_cloud_sync_microsoft_oauth(cx);
                                            this.clear_cloud_sync_select_focus();
                                            cx.stop_propagation();
                                        },
                                    ),
                                ))
                            })
                            .when(show_google_oauth, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    LucideIcon::KeyRound,
                                    "plugin.cloud_sync.actions.google_oauth_login",
                                    CloudSyncActionTone::Muted,
                                    google_oauth_disabled,
                                    cx.listener(
                                        |this: &mut WorkspaceApp,
                                         _event,
                                         _window,
                                         cx: &mut Context<WorkspaceApp>| {
                                            this.start_cloud_sync_google_oauth(cx);
                                            this.clear_cloud_sync_select_focus();
                                            cx.stop_propagation();
                                        },
                                    ),
                                ))
                            })
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::Upload,
                                "plugin.cloud_sync.actions.upload_now",
                                CloudSyncActionTone::Accent,
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
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::RefreshCw,
                                "plugin.cloud_sync.actions.check_remote",
                                CloudSyncActionTone::Muted,
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
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::Download,
                                "plugin.cloud_sync.actions.pull_preview",
                                CloudSyncActionTone::Muted,
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
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::RotateCcw,
                                "plugin.cloud_sync.actions.restore_backup",
                                CloudSyncActionTone::Muted,
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
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                LucideIcon::Save,
                                "plugin.cloud_sync.actions.save_settings",
                                CloudSyncActionTone::Muted,
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
                            )),
                    ),
            );

        if let Some(progress) = self.cloud_sync.controller.progress.as_ref() {
            card = card.child(self.render_cloud_sync_progress(progress, cx));
        }
        if let Some(error) = state.last_error.as_ref() {
            card = card.child(self.render_cloud_sync_error(error));
        }
        card.child(self.render_cloud_sync_meta(state, local_snapshot.as_ref().ok(), cx))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_overview_fact(
        &self,
        icon: LucideIcon,
        label_key: &'static str,
        value: String,
        cx: &mut Context<Self>,
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
            settings_mono_font_family(self.settings_store.settings()),
        )
    }

    pub(super) fn render_cloud_sync_guide_steps(&self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_cloud_sync_section_title(
        &self,
        key: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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

    pub(super) fn render_cloud_sync_error(&self, error: &str) -> AnyElement {
        cloud_sync_error_view(&self.tokens, self.format_cloud_sync_error(error))
    }

    pub(super) fn format_cloud_sync_error(&self, error: &str) -> String {
        match cloud_sync_error_message_spec(error) {
            CloudSyncErrorMessageSpec::Raw(message) => message,
            CloudSyncErrorMessageSpec::Key(key) => self.i18n.t(key),
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
}
