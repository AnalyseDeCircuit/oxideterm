use super::*;

const CLOUD_SYNC_TAB_BAR_WIDTH: f32 = 396.0; // Three equal header tabs leave room for translated labels.
const CLOUD_SYNC_BODY_LINE_HEIGHT: f32 = 20.0; // Compact body copy must not override page-header text metrics.

impl CloudSyncPageRenderer {
    pub fn render_panel(
        self,
        anchor: Option<OverlayAnchor>,
        dismiss_overlays: impl Fn(&mut Window, &mut App) + 'static,
        cx: &mut App,
    ) -> AnyElement {
        self.cloud_sync.update(cx, |sync, _| {
            if anchor.is_none() && sync.view.open_select.is_some() {
                sync.view.open_select = None;
            }
            sync.sync_section_rows();
        });
        let state = self.cloud_sync.read(cx).view.section_list_state.clone();
        let entity = self.cloud_sync.clone();
        let overlay = self.render_cloud_sync_select_overlay(anchor, dismiss_overlays, cx);
        let theme = self.tokens.ui;
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id("cloud-sync-scroll")
                    .size_full()
                    .on_scroll(move |_, _, cx| {
                        entity.update(cx, |sync, cx| sync.close_select_for_scroll(cx));
                    })
                    .bg(cloud_sync_root_bg(theme.bg, self.has_background))
                    .text_color(rgb(theme.text))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .child(tauri_virtual_list(
                        state,
                        CloudSyncWorkspaceEntity::section_list_spec(),
                        move |index, _, cx| self.render_section_item(index, cx),
                    )),
            )
            .children(overlay)
            .into_any_element()
    }

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

    pub(super) fn render_lucide_icon(icon: &'static str, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon)
            .size(px(size))
            .text_color(color)
            .into_any_element()
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
                cloud_sync.operation_in_flight(),
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
                let render = std::rc::Rc::clone(&self.render);
                self.cloud_sync
                    .update(cx, |cloud_sync, cx| match active_tab {
                        CloudSyncTab::Overview => super::history::render_recent_rollback_backups(
                            cloud_sync, render, busy, cx,
                        ),
                        CloudSyncTab::History => super::history::render_rollback_backup_list(
                            cloud_sync, render, busy, cx,
                        ),
                        CloudSyncTab::Configure => div().into_any_element(),
                    })
            }
            CloudSyncSection::History if active_tab == CloudSyncTab::History => {
                let render = std::rc::Rc::clone(&self.render);
                self.cloud_sync.update(cx, |cloud_sync, cx| {
                    super::history::render_history_list(cloud_sync, render, cx)
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
impl CloudSyncPageRenderer {
    pub(super) fn render_cloud_sync_header(&self, cx: &mut App) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .child(oxideterm_gpui_ui::page_header(
                &self.tokens,
                self.i18n.t("plugin.cloud_sync.panel_title"),
                Some(self.i18n.t("plugin.cloud_sync.native_description")),
                Some(self.render_cloud_sync_tab_bar(cx)),
            ))
            .child(div().w_full().h(px(1.0)).bg(rgb(self.tokens.ui.border)))
            .into_any_element()
    }

    pub(super) fn render_cloud_sync_tab_bar(&self, cx: &mut App) -> AnyElement {
        let theme = self.tokens.ui;
        let render_tab = |tab: CloudSyncTab,
                          icon: &'static str,
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
                "lucide/cloud.svg",
                "plugin.cloud_sync.tabs.overview",
                self.cloud_sync.read(cx).view.active_tab == CloudSyncTab::Overview,
                self,
            ),
            render_tab(
                CloudSyncTab::Configure,
                "lucide/settings.svg",
                "plugin.cloud_sync.tabs.configure",
                self.cloud_sync.read(cx).view.active_tab == CloudSyncTab::Configure,
                self,
            ),
            render_tab(
                CloudSyncTab::History,
                "lucide/clock.svg",
                "plugin.cloud_sync.tabs.history",
                self.cloud_sync.read(cx).view.active_tab == CloudSyncTab::History,
                self,
            ),
        ];
        let tab_bar_width = oxideterm_gpui_ui::estimate_segmented_control_width(
            &self.tokens,
            CLOUD_SYNC_TAB_BAR_WIDTH,
            540.0,
            true,
            [
                (self.i18n.t("plugin.cloud_sync.tabs.overview"), None),
                (self.i18n.t("plugin.cloud_sync.tabs.configure"), None),
                (self.i18n.t("plugin.cloud_sync.tabs.history"), None),
            ]
            .iter()
            .map(|(label, badge)| (label.as_str(), *badge)),
        );
        let active_index = cloud_sync_tab_index(self.cloud_sync.read(cx).view.active_tab);
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            CLOUD_SYNC_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(
                active_index,
                cloud_sync_tab_index(self.cloud_sync.read(cx).view.previous_tab),
                3,
            )
            .user_transition_active(self.tab_transition_active)
            .has_background_image(self.cloud_sync_has_background())
            .compact(tab_bar_width),
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
        let local_snapshot = self.diff.local_snapshot.as_ref().ok().map(Arc::as_ref);
        let local_file = cloud_sync.view.local_file_mode;
        let backend_label = self.i18n.t(if local_file {
            "plugin.cloud_sync.backend.local_file"
        } else {
            cloud_sync_backend_label_key(&settings.backend_type)
        });
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
        let show_github_oauth =
            !local_file && matches!(settings.backend_type, BackendType::GithubGist);
        let github_oauth_disabled = busy || settings.github_oauth_client_id.trim().is_empty();
        let show_microsoft_oauth =
            !local_file && matches!(settings.backend_type, BackendType::OneDrive);
        let microsoft_oauth_disabled = busy || settings.microsoft_oauth_client_id.trim().is_empty();
        let show_google_oauth =
            !local_file && matches!(settings.backend_type, BackendType::GoogleDrive);
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
                                "lucide/server.svg",
                                "plugin.cloud_sync.fields.backend",
                                backend_label,
                                cx,
                            ))
                            .when(!local_file, |facts| {
                                facts.child(self.render_cloud_sync_overview_fact(
                                    "lucide/hash.svg",
                                    "plugin.cloud_sync.fields.namespace",
                                    settings.namespace.clone(),
                                    cx,
                                ))
                            })
                            .when(!local_file, |facts| {
                                facts.child(self.render_cloud_sync_overview_fact(
                                    "lucide/activity.svg",
                                    "plugin.cloud_sync.fields.local_dirty",
                                    local_dirty,
                                    cx,
                                ))
                            })
                            .when(!local_file, |facts| {
                                facts.child(self.render_cloud_sync_overview_fact(
                                    "lucide/clock.svg",
                                    "plugin.cloud_sync.fields.last_sync",
                                    last_sync,
                                    cx,
                                ))
                            }),
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
                                    "lucide/key-round.svg",
                                    "plugin.cloud_sync.actions.github_oauth_login",
                                    CloudSyncActionTone::Muted,
                                    github_oauth_disabled,
                                    self.intent_listener(CloudSyncUiIntent::StartGithubOauth),
                                ))
                            })
                            .when(show_microsoft_oauth, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    "lucide/key-round.svg",
                                    "plugin.cloud_sync.actions.microsoft_oauth_login",
                                    CloudSyncActionTone::Muted,
                                    microsoft_oauth_disabled,
                                    self.intent_listener(CloudSyncUiIntent::StartMicrosoftOauth),
                                ))
                            })
                            .when(show_google_oauth, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    "lucide/key-round.svg",
                                    "plugin.cloud_sync.actions.google_oauth_login",
                                    CloudSyncActionTone::Muted,
                                    google_oauth_disabled,
                                    self.intent_listener(CloudSyncUiIntent::StartGoogleOauth),
                                ))
                            })
                            .child(self.render_cloud_sync_toolbar_button(
                                if local_file {
                                    "lucide/upload.svg"
                                } else {
                                    "lucide/refresh-cw.svg"
                                },
                                if local_file {
                                    "plugin.cloud_sync.actions.export_local"
                                } else {
                                    "plugin.cloud_sync.causal.sync"
                                },
                                CloudSyncActionTone::Accent,
                                busy,
                                self.intent_listener(CloudSyncUiIntent::StartUploadPreview),
                            ))
                            .when(!local_file, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    "lucide/download.svg",
                                    "plugin.cloud_sync.causal.view_remote",
                                    CloudSyncActionTone::Muted,
                                    busy,
                                    self.intent_listener(CloudSyncUiIntent::ViewRemote),
                                ))
                            })
                            .when(local_file, |toolbar| {
                                toolbar.child(self.render_cloud_sync_toolbar_button(
                                    "lucide/download.svg",
                                    "plugin.cloud_sync.actions.import_local",
                                    CloudSyncActionTone::Muted,
                                    busy,
                                    self.intent_listener(CloudSyncUiIntent::PullPreview),
                                ))
                            })
                            .child(self.render_cloud_sync_toolbar_button(
                                "lucide/rotate-ccw.svg",
                                "plugin.cloud_sync.actions.restore_backup",
                                CloudSyncActionTone::Muted,
                                busy || !has_rollback_backup,
                                self.intent_listener(CloudSyncUiIntent::RestoreLatestBackup),
                            ))
                            .child(self.render_cloud_sync_toolbar_button(
                                "lucide/save.svg",
                                "plugin.cloud_sync.actions.save_settings",
                                CloudSyncActionTone::Muted,
                                busy,
                                self.intent_listener(CloudSyncUiIntent::SaveConfiguration),
                            )),
                    ),
            );

        if let Some(progress) = cloud_sync.controller.progress.as_ref() {
            card = card.child(self.render_cloud_sync_progress(progress, cx));
        }
        if let Some(error) = state.last_error.as_ref() {
            card = card.child(self.render_cloud_sync_error(error));
        }
        card.when(!local_file, |card| {
            card.child(self.render_cloud_sync_meta(state, local_snapshot, cx))
        })
        .into_any_element()
    }

    pub(super) fn render_cloud_sync_overview_fact(
        &self,
        icon: &'static str,
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
                            SelectableTextRole::NonSelectable,
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
                                selectable_text_id(
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
                    label: self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
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
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
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
                SelectableTextRole::NonSelectable,
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
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
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
                SelectableTextRole::NonSelectable,
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
            cloud_sync_button_options(variant, disabled),
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
        icon: &'static str,
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
                SelectableTextRole::NonSelectable,
                "cloud-sync-progress",
                "stage",
                self.i18n
                    .t(cloud_sync_progress_stage_label_key(progress.stage)),
                theme.text,
                cx,
            ),
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
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
        crate::runtime::format_cloud_sync_error(&self.i18n, error)
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
        let version_title = self.render_display_text_with_role(
            SelectableTextRole::NonSelectable,
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
            let conflict_title = self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
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
            selectable_text_id("cloud-sync-meta", (&label, &value)),
            text,
            self.tokens.ui.text_muted,
            cx,
        ))
    }
}
