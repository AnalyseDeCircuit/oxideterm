use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn toggle_settings_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.settings_workspace.read(cx).settings_search_open() {
            self.close_settings_search(cx);
            return;
        }

        self.settings_workspace
            .update(cx, SettingsWorkspaceEntity::open_settings_search);
        self.focus_settings_input(SettingsInput::SettingsSearch, String::new(), cx);
        self.ime_marked_text = None;
        window.focus(&self.focus_handle, cx);
        cx.notify();
    }

    pub(in crate::workspace) fn close_settings_search(&mut self, cx: &mut Context<Self>) {
        self.settings_workspace.update(cx, |settings, cx| {
            settings.close_settings_search(true, cx);
        });
        self.clear_ime_selection();
        self.ime_marked_text = None;
        self.show_active_input_caret(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn activate_first_settings_search_result(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let query = self
            .settings_workspace
            .read(cx)
            .settings_search_query()
            .to_string();
        let Some(result) = settings_search_results(&self.i18n, &query)
            .into_iter()
            .find(|result| {
                result.section != SettingsSectionId::KnowledgeDocuments
                    || self.knowledge_has_selected_collection(cx)
            })
        else {
            return false;
        };
        self.activate_settings_search_result(result, cx);
        true
    }

    fn activate_settings_search_result(
        &mut self,
        result: SettingsSearchResult,
        cx: &mut Context<Self>,
    ) {
        self.ai_entity.update(cx, |ai, cx| {
            let section = match result.section {
                SettingsSectionId::AiProvidersSettings => {
                    Some(AiSettingsViewSection::ProviderSettings)
                }
                SettingsSectionId::AiToolsToolUse => Some(AiSettingsViewSection::ToolUse),
                SettingsSectionId::AiContextModelWindows => {
                    Some(AiSettingsViewSection::ContextWindows)
                }
                _ => None,
            };
            if let Some(section) = section {
                if !ai.settings_section_expanded(section) {
                    ai.toggle_settings_section(section, cx);
                }
                for provider in ai_provider_views(self.settings_store.settings()) {
                    if section == AiSettingsViewSection::ProviderSettings {
                        if !ai.settings_provider_expanded(&provider.id, false) {
                            ai.toggle_settings_provider_expanded(&provider.id, false, cx);
                        }
                        if !ai.settings_provider_models_expanded(&provider.id) {
                            ai.toggle_settings_provider_models(&provider.id, cx);
                        }
                    } else if section == AiSettingsViewSection::ContextWindows
                        && !ai.settings_context_provider_expanded(&provider.id)
                    {
                        ai.toggle_settings_context_provider(&provider.id, cx);
                    }
                }
            }
            if result.section == SettingsSectionId::KnowledgeEmbeddingConfig {
                ai.expand_knowledge_embedding_config();
                cx.notify();
            }
        });
        let tab = result.tab;
        if result.section == SettingsSectionId::AppearanceBackground {
            // Reveal collapsible background sub-sections so jumped matches remain visible.
            self.settings_workspace.update(cx, |entity, cx| {
                entity.background_effects_expanded = Some(true);
                entity.background_interactions_expanded = Some(true);
                cx.notify();
            });
        }
        if result.section == SettingsSectionId::TerminalAwarenessTriggers {
            self.terminal_trigger_settings_pane = None;
        }
        self.settings_workspace.update(cx, |settings, cx| {
            settings.set_active_tab(tab, cx);
            if let Some(page) = result.terminal_page {
                settings.set_terminal_page(page, cx);
            }
            if let Some(page) = result.ai_page {
                settings.set_ai_page(page, cx);
            }
            settings.close_settings_search(true, cx);
        });
        self.close_settings_select();
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        self.clear_ime_selection();
        self.ime_marked_text = None;
        if tab == SettingsTab::General {
            self.refresh_cli_companion_status(cx);
            #[cfg(not(target_os = "macos"))]
            self.refresh_launch_at_login_status(cx);
        }
        if tab == SettingsTab::Portable {
            self.refresh_portable_settings_snapshot(true, cx);
        }
        self.sync_settings_section_list_state(cx);
        if let Some(item_ix) = settings_section_list_item_index(
            tab,
            result.section,
            self.settings_dynamic_section_counts(cx),
        ) {
            self.settings_section_list_state
                .scroll_to(gpui::ListOffset {
                    item_ix,
                    offset_in_item: px(0.0),
                });
        }
        cx.notify();
    }

    pub(in crate::workspace) fn render_settings_search_panel(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let settings = self.settings_workspace.read(cx);
        let query = settings.settings_search_query();
        let target = WorkspaceImeTarget::Settings(SettingsInput::SettingsSearch);
        let workspace = cx.entity();
        // The navigation surface already owns its image tint; do not stack a second one.
        let search_input = self
            .sidebar_search_row(rgba(0x00000000))
            .child(
                self.text_input_with_workspace_ime(
                    target,
                    self.workspace_text_input(
                        target,
                        TextInputView {
                            value: query,
                            placeholder: self.i18n.t("settings_view.search.placeholder"),
                            focused: settings.settings_entity_focused_input()
                                == Some(SettingsInput::SettingsSearch),
                            caret_visible: self.input_caret.visible(),
                            secret: false,
                            selected_all: false,
                            selected_range: self.ime_selected_range_for_target(target, cx),
                            marked_text: self.marked_text_for_target(target, cx),
                        },
                    )
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .px_0()
                    .border_0()
                    .rounded_none()
                    .bg(rgba(0x00000000))
                    .text_size(px(self.tokens.metrics.sidebar_title_font_size))
                    .line_height(px(20.0)),
                    |this, cx| {
                        this.focus_settings_input(SettingsInput::SettingsSearch, String::new(), cx);
                    },
                    cx,
                ),
            )
            .when(!query.is_empty(), |input| {
                let clear_workspace = workspace.clone();
                input.child(self.workspace_tooltip_icon_button(
                    LucideIcon::X,
                    13.0,
                    rgb(self.tokens.ui.text_muted),
                    IconButtonOptions {
                        hover_background: Some(rgb(self.tokens.ui.bg_hover)),
                        ..IconButtonOptions::opaque_toolbar(28.0, ButtonRadius::Sm)
                    },
                    self.i18n.t("settings_view.search.clear"),
                    "settings-search-clear",
                    true,
                    move |_event, _window, cx| {
                        let _ = clear_workspace.update(cx, |this, cx| {
                            this.settings_workspace.update(cx, |settings, cx| {
                                settings.clear_settings_search_query(cx);
                            });
                            this.clear_ime_selection();
                            this.show_active_input_caret(cx);
                        });
                        cx.stop_propagation();
                    },
                    workspace.clone(),
                ))
            });
        let results = settings_search_results(&self.i18n, query)
            .into_iter()
            .filter(|result| {
                result.section != SettingsSectionId::KnowledgeDocuments
                    || self.knowledge_has_selected_collection(cx)
            })
            .collect::<Vec<_>>();
        let result_scroll = self.selectable_text_scroll_handle("settings-search-results-scroll");
        let mut result_list = div()
            .id("settings-search-results-scroll")
            .size_full()
            .min_h(px(0.0))
            .selectable_overflow_y_scroll(&result_scroll)
            .pt_3()
            .px_2()
            .pb_3()
            .flex()
            .flex_col()
            .gap(px(2.0));

        if query.trim().is_empty() || results.is_empty() {
            let message_key = if query.trim().is_empty() {
                "settings_view.search.hint"
            } else {
                "settings_view.search.no_results"
            };
            result_list = result_list.child(
                div()
                    .flex_1()
                    .min_h(px(120.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Search,
                        20.0,
                        rgb(self.tokens.ui.text_muted),
                    ))
                    .child(self.i18n.t(message_key)),
            );
        } else {
            for result in results {
                let tab = result.tab;
                let action_result = result.clone();
                let row = entity_list_row(
                    &self.tokens,
                    EntityListRowOptions::new()
                        .compact()
                        .has_background_image(self.settings_background_active()),
                    Some(
                        Self::render_lucide_icon(
                            settings_tab_lucide(tab.icon()),
                            16.0,
                            rgb(self.tokens.ui.accent),
                        )
                        .into_any_element(),
                    ),
                    div()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(self.tokens.ui.text))
                        .child(result.label)
                        .into_any_element(),
                    Some(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(result.breadcrumb)
                            .into_any_element(),
                    ),
                    Vec::new(),
                    Vec::new(),
                )
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.activate_settings_search_result(action_result.clone(), cx);
                        cx.stop_propagation();
                    }),
                );
                result_list = result_list.child(row);
            }
        }

        div()
            .flex_1()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(search_input)
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .relative()
                    .child(result_list)
                    .child(selectable_vertical_scrollbar_layer(
                        "settings-search-results-scrollbar",
                        &result_scroll,
                    )),
            )
            .into_any_element()
    }
}
