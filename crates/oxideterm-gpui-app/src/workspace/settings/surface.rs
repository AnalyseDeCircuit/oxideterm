impl WorkspaceApp {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_tab(window, cx);
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let close_active_settings_tab = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Settings);
        self.active_surface = ActiveSurface::Terminal;
        self.close_settings_select();
        self.focused_settings_input = None;
        self.settings_slider_drag = None;
        if close_active_settings_tab {
            self.close_active_tab(window, cx);
            return;
        }
        self.focus_active_pane(window, cx);
        cx.notify();
    }

    pub(super) fn render_settings_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let has_settings_background = self.settings_background_active();
        div()
            .size_full()
            .relative()
            .flex()
            .flex_row()
            .bg(if has_settings_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text))
            .child(self.render_settings_nav(cx))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .relative()
                    .child(self.render_settings_section_list_scroll(cx)),
            )
            .when_some(self.render_ai_mcp_add_server_dialog(cx), |surface, modal| {
                surface.child(modal)
            })
            .when_some(
                self.render_knowledge_create_collection_dialog(cx),
                |surface, modal| surface.child(modal),
            )
            .when_some(
                self.render_knowledge_new_document_dialog(cx),
                |surface, modal| surface.child(modal),
            )
            .when_some(
                self.render_knowledge_delete_confirm_dialog(cx),
                |surface, modal| surface.child(modal),
            )
            .when(self.settings_page.keybinding_reset_all_confirm_open, |surface| {
                surface.child(self.render_keybinding_reset_all_confirm_dialog(cx))
            })
            .when_some(self.render_settings_select_overlay(cx), |surface, overlay| {
                surface.child(overlay)
            })
            .when_some(self.render_theme_editor_modal(cx), |surface, modal| {
                surface.child(modal)
            })
            .when_some(self.render_portable_password_change_dialog(cx), |surface, modal| {
                surface.child(modal)
            })
            .when_some(self.session_manager.oxide_import_dialog.as_ref(), |surface, _| {
                surface.child(self.render_oxide_import_dialog(cx))
            })
            .when_some(self.session_manager.oxide_export_dialog.as_ref(), |surface, _| {
                surface.child(self.render_oxide_export_dialog(cx))
            })
            .into_any_element()
    }

    fn render_settings_section_list_scroll(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.sync_settings_section_list_state();
        let state = self.settings_section_list_state.clone();
        let workspace = cx.entity();
        let spec = self.settings_section_list_spec();
        // All settings pages now share the same variable-height section list.
        // This matches the browser/TanStack virtualizer direction and avoids
        // keeping a full flex tree mounted just because a tab is inside Settings.
        div()
            .id("settings-content-scroll")
            .size_full()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                this.pause_settings_caret_blink_during_scroll();
                // Tauri only closes an open select on page scroll. When no select is
                // visible, keep wheel scrolling free of state writes so large settings
                // pages do not rebuild just to maintain stale overlay anchors.
                if browser_behavior::close_browser_trigger_select_on_container_scroll(
                    &mut this.open_settings_select,
                    &mut this.settings_select_focus_origin,
                ) {
                    this.clear_settings_select_anchors();
                    cx.notify();
                }
            }))
            .child(tauri_virtual_list(state, spec, move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    this.render_settings_section_list_item(index, cx)
                })
            }))
            .into_any_element()
    }

    fn render_settings_section_list_item(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if self.settings_page.active_tab == SettingsTab::Ai {
            return self.render_settings_ai_section_item(index, cx);
        }

        let padding = self.tokens.metrics.settings_content_padding;
        let gap = self.tokens.metrics.settings_page_gap;
        let outer_max_width = self.settings_content_outer_max_width();
        let section_index = index.saturating_sub(SETTINGS_SECTION_HEADER_ITEM_COUNT);
        let mut item = div()
            .w_full()
            .min_w(px(0.0))
            .max_w(px(outer_max_width))
            .mx_auto()
            .px(px(padding))
            .pb(px(gap));
        if index == 0 {
            item = item.pt(px(padding));
        }
        if index + 1 == self.settings_section_list_item_count() {
            item = item.pb(px(padding));
        }

        item.child(if index == 0 {
            self.render_settings_virtual_header(self.settings_page.active_tab, cx)
        } else {
            self.render_settings_tab_section(self.settings_page.active_tab, section_index, cx)
        })
        .into_any_element()
    }

    fn render_settings_ai_section_item(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        self.normalize_ai_execution_profiles_for_settings_render();
        let item = match index {
            0 => self.render_settings_virtual_header(SettingsTab::Ai, cx),
            1 => {
                let settings = self.settings_store.settings();
                self.ai_general_settings_card(settings, cx)
            }
            2 => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_execution_profiles_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            3 => {
                let provider_views = self.ai_provider_views_for_settings_render(cx);
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_provider_settings_section(&provider_views, cx),
                    settings.ai.enabled,
                )
            }
            4 => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_context_controls_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            5 => {
                let settings = self.settings_store.settings();
                let provider_views = ai_provider_views(settings);
                self.ai_disabled_settings_card(
                    self.ai_system_prompt_section(settings, &provider_views, cx),
                    settings.ai.enabled,
                )
            }
            6 => {
                let settings = self.settings_store.settings();
                self.ai_disabled_settings_card(
                    self.ai_tool_use_section(settings, cx),
                    settings.ai.enabled,
                )
            }
            _ => div().into_any_element(),
        };

        self.wrap_settings_section_list_item(index, item)
    }

    fn wrap_settings_section_list_item(&self, index: usize, child: AnyElement) -> AnyElement {
        let padding = self.tokens.metrics.settings_content_padding;
        let gap = self.tokens.metrics.settings_page_gap;
        let outer_max_width = self.settings_content_outer_max_width();
        let mut item = div()
            .w_full()
            .min_w(px(0.0))
            .max_w(px(outer_max_width))
            .mx_auto()
            .px(px(padding))
            .pb(px(gap));
        if index == 0 {
            item = item.pt(px(padding));
        }
        if index + 1 == self.settings_section_list_item_count() {
            item = item.pb(px(padding));
        }
        item.child(child).into_any_element()
    }

    fn settings_content_outer_max_width(&self) -> f32 {
        // Tauri SettingsView uses `max-w-4xl mx-auto p-10`: the 4xl max-width
        // applies to the content box and padding is added outside it. GPUI's
        // max_w constrains this padded wrapper directly, so include both sides
        // of settings_content_padding or every settings page becomes visually
        // narrower and more fixed-width than the browser version.
        self.tokens.metrics.settings_content_max_width
            + self.tokens.metrics.settings_content_padding * 2.0
    }

    fn render_settings_virtual_header(&self, tab: SettingsTab, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .child(self.render_settings_page_header(tab, cx))
            .child(separator(&self.tokens, SeparatorOrientation::Horizontal))
            .into_any_element()
    }

    fn ai_provider_views_for_settings_render(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Vec<AiProviderView> {
        let provider_views = ai_provider_views(self.settings_store.settings());
        self.ensure_ai_provider_key_statuses_for_views(&provider_views, cx);
        provider_views
    }

    fn sync_settings_section_list_state(&mut self) {
        let spec = self.settings_section_list_spec();
        let identity = self.settings_section_list_identity();
        let signatures = self.settings_section_list_signatures();
        sync_tauri_variable_list_state_by_signatures(
            &self.settings_section_list_state,
            &mut self.settings_section_list_cache.borrow_mut(),
            &identity,
            &signatures,
            spec,
        );
    }

    fn settings_section_list_spec(&self) -> TauriVirtualListSpec {
        if self.settings_page.active_tab == SettingsTab::Ai {
            TauriVirtualListSpec::new(
                px(AI_SETTINGS_SECTION_ESTIMATED_HEIGHT),
                SETTINGS_SECTION_LIST_OVERSCAN,
            )
        } else {
            TauriVirtualListSpec::new(
                px(SETTINGS_SECTION_LIST_ESTIMATED_HEIGHT),
                SETTINGS_SECTION_LIST_OVERSCAN,
            )
        }
    }

    fn settings_section_list_identity(&self) -> String {
        // Tauri keys virtual rows by tab plus the nested tab/filter state that
        // can change the section set. Preserve that identity before asking GPUI
        // ListState to reuse measured variable-height rows.
        settings_model_section_list_identity(
            self.settings_page.active_tab,
            self.settings_page.terminal_page,
            &format!("{:?}", self.settings_page.keybinding_scope_filter),
            self.settings_page.keybinding_search_query.trim(),
        )
    }

    fn settings_section_list_signatures(&self) -> Vec<u64> {
        (0..self.settings_section_list_item_count())
            .map(|index| self.settings_section_signature(index))
            .collect()
    }

    fn settings_section_signature(&self, index: usize) -> u64 {
        let mut hasher = DefaultHasher::new();
        // GPUI caches variable-row measurements. Hash only states that can
        // change section height so ListState remeasures affected rows without
        // serializing the entire settings file on every scroll render.
        format!("{:?}", self.settings_page.active_tab).hash(&mut hasher);
        index.hash(&mut hasher);
        let settings = self.settings_store.settings();

        match self.settings_page.active_tab {
            SettingsTab::General => {
                self.settings_page.cli_companion_loading.hash(&mut hasher);
                self.settings_page.cli_companion_error.is_some().hash(&mut hasher);
                self.settings_page.cli_companion_status.hash(&mut hasher);
            }
            SettingsTab::Terminal => {
                format!("{:?}", self.settings_page.terminal_page).hash(&mut hasher);
                settings
                    .terminal
                    .command_bar
                    .focus_handoff_commands
                    .len()
                    .hash(&mut hasher);
            }
            SettingsTab::Local => {
                settings.local_terminal.oh_my_posh_enabled.hash(&mut hasher);
                settings.local_terminal.default_shell_id.hash(&mut hasher);
                self.local_shells.len().hash(&mut hasher);
            }
            SettingsTab::Sftp => {
                settings.sftp.speed_limit_enabled.hash(&mut hasher);
            }
            SettingsTab::Connections => {
                self.connection_store.connections().len().hash(&mut hasher);
                self.settings_connection_groups_signature().hash(&mut hasher);
                self.settings_page.settings_connection_status.is_some().hash(&mut hasher);
            }
            SettingsTab::Portable => {
                self.portable_settings_refresh_pending.hash(&mut hasher);
                self.portable_status_error.is_some().hash(&mut hasher);
                self.portable_exportable_secret_count.hash(&mut hasher);
                if let Some(status) = self.portable_status_snapshot.as_ref() {
                    status.is_portable.hash(&mut hasher);
                    format!("{:?}", status.status).hash(&mut hasher);
                    status.is_unlocked.hash(&mut hasher);
                }
            }
            SettingsTab::Ai => {
                settings.ai.enabled.hash(&mut hasher);
                settings.ai.providers.len().hash(&mut hasher);
                self.settings_page.ai_provider_settings_expanded.hash(&mut hasher);
                self.settings_page.ai_tool_use_expanded.hash(&mut hasher);
                self.settings_page.ai_context_windows_expanded.hash(&mut hasher);
                self.settings_page.ai_model_reasoning_expanded.hash(&mut hasher);
                hash_string_bool_map(&self.settings_page.expanded_ai_providers, &mut hasher);
                hash_string_set(&self.settings_page.expanded_ai_provider_models, &mut hasher);
                hash_string_set(&self.settings_page.expanded_ai_context_providers, &mut hasher);
                hash_string_set(&self.settings_page.expanded_ai_model_reasoning_providers, &mut hasher);
            }
            SettingsTab::Knowledge => {
                self.settings_page.knowledge_selected_collection_id.hash(&mut hasher);
                self.settings_page.knowledge_error.is_some().hash(&mut hasher);
                self.settings_page.knowledge_import_progress.hash(&mut hasher);
                self.settings_page.knowledge_embedding_progress.hash(&mut hasher);
                self.settings_page.knowledge_reindex_progress.hash(&mut hasher);
            }
            SettingsTab::Keybindings => {
                format!("{:?}", self.settings_page.keybinding_scope_filter).hash(&mut hasher);
                self.settings_page.keybinding_search_query.trim().hash(&mut hasher);
                settings.keybindings.overrides.len().hash(&mut hasher);
            }
            _ => {}
        }

        hasher.finish()
    }

    fn settings_connection_groups_signature(&self) -> usize {
        self.connection_store
            .connections()
            .iter()
            .map(|connection| connection.group.as_deref().unwrap_or_default().len())
            .sum()
    }

    fn settings_section_list_item_count(&self) -> usize {
        settings_model_section_list_item_count(
            self.settings_page.active_tab,
            self.settings_dynamic_section_counts(),
        )
    }

    fn settings_dynamic_section_counts(&self) -> SettingsDynamicSectionCounts {
        SettingsDynamicSectionCounts {
            terminal_page: self.settings_page.terminal_page,
            visible_keybinding_scope_count: self.visible_keybinding_scope_count(),
            knowledge_has_error: self.settings_page.knowledge_error.is_some(),
            knowledge_has_selected_collection: self.knowledge_has_selected_collection(),
        }
    }

    fn visible_keybinding_scope_count(&self) -> usize {
        let query = self.settings_page.keybinding_search_query.trim().to_lowercase();
        [
            crate::keybindings::ActionScope::Global,
            crate::keybindings::ActionScope::Terminal,
            crate::keybindings::ActionScope::Split,
            crate::keybindings::ActionScope::Palette,
        ]
        .into_iter()
        .filter(|scope| {
            crate::keybindings::ACTION_DEFINITIONS
                .iter()
                .filter(|definition| definition.scope == *scope)
                .filter(|definition| {
                    settings_keybinding_scope_matches(
                        self.settings_page.keybinding_scope_filter,
                        definition.scope,
                    )
                })
                .any(|definition| {
                    if query.is_empty() {
                        return true;
                    }
                    let label = self.i18n.t(&definition.label_key()).to_lowercase();
                    label.contains(&query) || definition.id.to_lowercase().contains(&query)
                })
        })
        .count()
    }

    fn knowledge_has_selected_collection(&self) -> bool {
        let collections =
            oxideterm_ai::rag_list_collections(&self.ai_rag_store, None).unwrap_or_default();
        self.settings_page
            .knowledge_selected_collection_id
            .as_deref()
            .filter(|id| collections.iter().any(|collection| collection.id == *id))
            .or_else(|| collections.first().map(|collection| collection.id.as_str()))
            .is_some()
    }

    fn render_settings_tab_section(
        &mut self,
        tab: SettingsTab,
        section_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Section virtualization only pays off if item rendering is lazy.
        // Dispatch by section index instead of constructing the old full
        // settings Vec and discarding every non-visible card.
        match tab {
            SettingsTab::General => self.settings_general_section(section_index, cx),
            SettingsTab::Portable => self.settings_portable_section(section_index, cx),
            SettingsTab::Terminal => self.settings_terminal_section(section_index, cx),
            SettingsTab::Appearance => self.settings_appearance_section(section_index, cx),
            SettingsTab::Local => self.settings_local_section(section_index, cx),
            SettingsTab::Connections => self.settings_connections_section(section_index, cx),
            SettingsTab::Ssh => self.settings_ssh_section(section_index),
            SettingsTab::Reconnect => self.settings_reconnect_section(section_index, cx),
            SettingsTab::Sftp => self.settings_sftp_section(section_index, cx),
            SettingsTab::Ide => self.settings_ide_section(section_index, cx),
            SettingsTab::Ai => div().into_any_element(),
            SettingsTab::Knowledge => self.settings_knowledge_section(section_index, cx),
            SettingsTab::Keybindings => self.settings_keybindings_section(section_index, cx),
            SettingsTab::Help => self.settings_help_section(section_index, cx),
        }
    }

    fn clear_settings_select_anchors(&mut self) {
        self.select_anchors
            .retain(|id, _| matches!(id, SelectAnchorId::NewConnectionGroup));
    }

    fn pause_settings_caret_blink_during_scroll(&mut self) {
        if self.focused_settings_input.is_none() {
            return;
        }
        // Browser caret blinking is compositor-local. Native blinking repaints
        // the workspace, so keep the caret visible while a settings scroll is
        // active and let blinking resume shortly after scrolling stops.
        self.settings_caret_blink_pause_until =
            Some(Instant::now() + Duration::from_millis(180));
        self.new_connection_caret_visible = true;
    }

    fn render_settings_nav(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let settings_nav_scroll = self.selectable_text_scroll_handle("settings-nav-scroll");
        let mut nav = div()
            .w(px(self.tokens.metrics.settings_nav_width))
            .h_full()
            .flex()
            .flex_col()
            .pt(px(24.0))
            .pb_4()
            .bg(self.settings_panel_background(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border));

        nav = nav.child(
            div()
                .px(px(20.0))
                .mb(px(24.0))
                .text_size(px(20.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text_heading))
                .child(self.i18n.t("settings_view.title")),
        );

        let mut list = div()
            .id("settings-nav-scroll")
            .size_full()
            .min_h(px(0.0))
            .selectable_overflow_y_scroll(&settings_nav_scroll)
            .px_3()
            .flex()
            .flex_col();

        for (group_index, group) in SettingsTab::groups().iter().enumerate() {
            if group_index > 0 {
                list = list.child(
                    div()
                        .py_2()
                        .child(separator(&self.tokens, SeparatorOrientation::Horizontal)),
                );
            }
            for tab in *group {
                list = list.child(self.render_settings_nav_item(*tab, cx));
            }
        }

        nav.child(
            div()
                .flex_1()
                .min_h(px(0.0))
                .relative()
                .child(list)
                .child(selectable_vertical_scrollbar_layer(
                    "settings-nav-scrollbar",
                    &settings_nav_scroll,
                )),
        )
        .into_any_element()
    }

    fn render_settings_nav_item(&self, tab: SettingsTab, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.settings_page.active_tab == tab;
        div()
            .h(px(40.0))
            .w_full()
            .mb(px(4.0))
            .px_3()
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if active {
                rgba((theme.border << 8) | 0xff)
            } else {
                rgba(0x00000000)
            })
            .bg(if active {
                rgb(theme.bg_panel)
            } else {
                rgba(0x00000000)
            })
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(rgb(if active {
                theme.text_heading
            } else {
                theme.text
            }))
            .cursor_pointer()
            .hover(move |item| {
                if active {
                    item
                } else {
                    item.bg(rgba((theme.bg_hover << 8) | 0x80))
                }
            })
            .child(Self::render_lucide_icon(
                settings_tab_lucide(tab.icon()),
                18.0,
                rgb(theme.accent),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(self.i18n.t(tab.label_key())),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.settings_page.set_active_tab(tab);
                    this.close_settings_select();
                    this.focused_settings_input = None;
                    this.settings_slider_drag = None;
                    this.clear_ime_selection();
                    if tab == SettingsTab::General {
                        this.refresh_cli_companion_status(cx);
                    }
                    if tab == SettingsTab::Portable {
                        this.refresh_portable_settings_snapshot(true, cx);
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_settings_page_header(
        &self,
        tab: SettingsTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let title = self.i18n.t(tab.title_key());
        let description = self.i18n.t(tab.description_key());
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(24.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(self.tokens.ui.text_heading))
                    .child(self.render_selectable_text_scoped(
                        "settings-page-title",
                        tab.title_key(),
                        title,
                        self.tokens.ui.text_heading,
                        cx,
                    )),
            )
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_base))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_selectable_text_scoped(
                        "settings-page-description",
                        tab.description_key(),
                        description,
                        self.tokens.ui.text_muted,
                        cx,
                    )),
            )
            .when(tab == SettingsTab::Keybindings, |header| {
                let note = self.i18n.t("settings_view.keybindings.intl_keyboard_note");
                header.child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgba((self.tokens.ui.text_muted << 8) | 0xb3))
                        .child(self.render_selectable_text_scoped(
                            "settings-keybindings-note",
                            "keybindings",
                            note,
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                )
            })
            .into_any_element()
    }

    pub(in crate::workspace) fn edit_settings(
        &mut self,
        edit: impl FnOnce(&mut PersistedSettings),
        cx: &mut Context<Self>,
    ) {
        let previous_settings = self.settings_store.settings().clone();
        edit(self.settings_store.settings_mut());
        let settings = self.settings_store.settings().clone();
        self.apply_loaded_settings_to_runtime(&settings, cx);
        let _ = self.settings_store.save();
        self.settings_store_last_modified = settings_store_modified_time(self.settings_store.path());
        self.emit_native_plugin_settings_events(&previous_settings, &settings, cx);
        self.sync_tab_titles(cx);
        cx.notify();
    }

    pub(super) fn reload_after_external_sync(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let previous_settings = self.settings_store.settings().clone();
        let settings_path = self.settings_store.path().to_path_buf();
        let connection_path = self.connection_store.path().to_path_buf();
        let next_settings = SettingsStore::load_from_path(settings_path, None)
            .map_err(|error| format!("Failed to reload settings after external sync: {error}"))?;
        let next_connections = ConnectionStore::load(connection_path)
            .map_err(|error| format!("Failed to reload connections after external sync: {error}"))?;
        let settings = next_settings.settings().clone();
        self.settings_store = next_settings;
        self.connection_store = next_connections;
        self.settings_store_last_modified = settings_store_modified_time(self.settings_store.path());
        self.connection_store_last_modified =
            settings_store_modified_time(self.connection_store.path());
        // External sync mutates persisted stores outside the GPUI controls.
        // Re-apply the same runtime side effects used by edit_settings instead
        // of relying on stale in-memory settings or browser-style stores.
        self.apply_loaded_settings_to_runtime(&settings, cx);
        self.emit_native_plugin_settings_events(&previous_settings, &settings, cx);
        self.queue_cloud_sync_dirty_refresh(cx);
        self.sync_tab_titles(cx);
        cx.notify();
        Ok(())
    }

    pub(super) fn poll_external_settings_store_changes(&mut self, cx: &mut Context<Self>) {
        let settings_modified = settings_store_modified_time(self.settings_store.path());
        let connections_modified = settings_store_modified_time(self.connection_store.path());
        let settings_changed = settings_modified != self.settings_store_last_modified;
        let connections_changed = connections_modified != self.connection_store_last_modified;
        if !settings_changed && !connections_changed {
            return;
        }

        // CLI writes and external tools mutate the same persisted stores as Tauri's
        // browser settingsStore. Reload through the cloud-sync path so terminal,
        // IDE, SFTP, theme, plugin, and sidebar runtime side effects stay aligned.
        if self.reload_after_external_sync(cx).is_err() {
            self.settings_store_last_modified = settings_modified;
            self.connection_store_last_modified = connections_modified;
        }
    }

    fn apply_loaded_settings_to_runtime(
        &mut self,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        self.i18n
            .set_locale(locale_from_settings(settings.general.language));
        self.tokens = tokens_from_settings(&settings);
        self.render_policy = compute_render_policy(
            self.render_profile_override
                .unwrap_or(settings.appearance.render_profile),
            &self.detected_graphics,
        );
        // Settings changes can flip the render profile while a modal is open;
        // update the shared backdrop gate before the next top-layer render.
        set_tauri_backdrop_blur_allowed(self.render_policy.allow_background_blur);
        self.background_image_cache
            .set_byte_limit(self.render_policy.image_cache_bytes);
        self.sftp_transfer_manager
            .apply_settings(sftp_runtime_settings_from_settings(&settings));
        self.ssh_registry.set_idle_timeout(Some(Duration::from_secs(
            settings.connection_pool.idle_timeout_secs as u64,
        )));
        self.reconnect_orchestrator.configure(
            reconnect_timing_from_settings(&settings),
            reconnect_max_attempts_from_settings(&settings),
        );
        self.ai_agent_fs
            .set_mode(crate::workspace::ide::node_agent_mode_from_settings(&settings));
        self.sidebar_collapsed = settings.sidebar_ui.collapsed;
        self.sidebar_width = settings.sidebar_ui.width as f32;
        self.ai_sidebar_width =
            (settings.sidebar_ui.ai_sidebar_width as f32).clamp(AI_SIDEBAR_MIN_WIDTH, AI_SIDEBAR_MAX_WIDTH);
        let panes = self
            .panes
            .iter()
            .map(|(pane_id, pane)| (*pane_id, pane.clone()))
            .collect::<Vec<_>>();
        for (pane_id, pane) in panes {
            let preferences = self.terminal_preferences_for_pane(pane_id);
            let _ = pane.update(cx, |pane, cx| {
                pane.set_preferences(preferences, cx);
            });
        }
        // Tauri's IDE reads Settings.ide live from settingsStore. Native IDE
        // surfaces keep their own GPUI owners, so push typography/wrap/autosave
        // changes into each open surface after the settings store changes.
        self.apply_ide_runtime_settings_to_surfaces(cx);
    }

    fn emit_native_plugin_settings_events(
        &mut self,
        previous_settings: &PersistedSettings,
        settings: &PersistedSettings,
        cx: &mut Context<Self>,
    ) {
        if previous_settings.terminal.theme != settings.terminal.theme {
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_APP_THEME_CHANGED_EVENT,
                serde_json::json!({
                    "theme": crate::workspace::plugin_lifecycle::native_plugin_theme_snapshot(
                        &settings.terminal.theme
                    ),
                }),
                cx,
            );
        }

        if previous_settings.general.language != settings.general.language {
            let language = settings.general.language.as_str();
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_I18N_LANGUAGE_CHANGED_EVENT,
                serde_json::json!({ "language": language }),
                cx,
            );
        }

        let previous_value = serde_json::to_value(previous_settings).unwrap_or_else(|_| {
            serde_json::json!({})
        });
        let current_value = serde_json::to_value(settings).unwrap_or_else(|_| {
            serde_json::json!({})
        });
        if previous_value != current_value {
            // Tauri exposes app.onSettingsChange as an application-level
            // snapshot callback. Native sends the same immutable snapshot over
            // the plugin event channel after persistence succeeds.
            self.emit_native_plugin_event_to_subscribers(
                plugin_host::NATIVE_PLUGIN_APP_SETTINGS_CHANGED_EVENT,
                serde_json::json!({ "settings": current_value }),
                cx,
            );
        }
    }
}

fn hash_string_set(values: &HashSet<String>, hasher: &mut impl Hasher) {
    let mut values = values.iter().collect::<Vec<_>>();
    values.sort();
    for value in values {
        value.hash(hasher);
    }
}

fn hash_string_bool_map(values: &HashMap<String, bool>, hasher: &mut impl Hasher) {
    let mut values = values.iter().collect::<Vec<_>>();
    values.sort_by(|(left, _), (right, _)| left.cmp(right));
    for (key, value) in values {
        key.hash(hasher);
        value.hash(hasher);
    }
}
