impl WorkspaceApp {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_tab(window, cx);
    }

    pub(super) fn close_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let close_active_settings_tab = self
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::Settings);
        self.active_surface = ActiveSurface::Terminal;
        self.open_settings_select = None;
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
        let settings_content_scroll =
            self.selectable_text_scroll_handle("settings-content-scroll");
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
            .child(self.render_settings_nav(has_settings_background, cx))
            .child(
                div()
                    .id("settings-content-scroll")
                    .flex_1()
                    .min_w(px(0.0))
                    .min_h(px(0.0))
                    .selectable_overflow_y_scrollbar(&settings_content_scroll)
                    .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                        // GPUI can advance scroll state without rebuilding the settings view,
                        // so cached trigger bounds must not survive a settings scroll.
                        let had_open_select = this.open_settings_select.take().is_some();
                        this.clear_settings_select_anchors();
                        if had_open_select {
                            cx.notify();
                        }
                    }))
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            // Tauri SettingsView uses max-w-4xl mx-auto p-10 for the content rail.
                            .max_w(px(self.tokens.metrics.settings_content_max_width))
                            .mx_auto()
                            .p(px(self.tokens.metrics.settings_content_padding))
                            .child(self.render_settings_tab_content(cx)),
                    ),
            )
            .when_some(self.render_ai_mcp_add_server_dialog(cx), |surface, modal| {
                surface.child(modal)
            })
            .when_some(self.render_knowledge_create_collection_dialog(cx), |surface, modal| {
                surface.child(modal)
            })
            .when_some(self.render_knowledge_new_document_dialog(cx), |surface, modal| {
                surface.child(modal)
            })
            .when_some(self.render_knowledge_delete_confirm_dialog(cx), |surface, modal| {
                surface.child(modal)
            })
            .when(self.keybinding_reset_all_confirm_open, |surface| {
                surface.child(self.render_keybinding_reset_all_confirm_dialog(cx))
            })
            .when_some(self.render_settings_select_overlay(cx), |surface, overlay| {
                surface.child(overlay)
            })
            .when_some(self.render_theme_editor_modal(cx), |surface, modal| {
                surface.child(modal)
            })
            .into_any_element()
    }

    fn clear_settings_select_anchors(&mut self) {
        self.select_anchors
            .retain(|id, _| matches!(id, SelectAnchorId::NewConnectionGroup));
    }

    fn render_settings_nav(
        &self,
        has_settings_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut nav = div()
            .w(px(self.tokens.metrics.settings_nav_width))
            .h_full()
            .flex()
            .flex_col()
            .pt(px(24.0))
            .pb_4()
            .bg(if has_settings_background {
                rgba((theme.bg_panel << 8) | alpha_byte(self.tokens.metrics.panel_vibrancy_alpha))
            } else {
                rgb(theme.bg_panel)
            })
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
            .flex_1()
            .min_h(px(0.0))
            .selectable_overflow_y_scrollbar(
                &self.selectable_text_scroll_handle("settings-nav-scroll"),
            )
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

        nav.child(list).into_any_element()
    }

    fn render_settings_nav_item(&self, tab: SettingsTab, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_settings_tab == tab;
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
                item.bg(if active {
                    rgb(theme.bg_panel)
                } else {
                    rgb(theme.bg_hover)
                })
            })
            .child(Self::render_lucide_icon(
                settings_tab_lucide(tab.icon()),
                16.0,
                rgb(theme.text),
            ))
            .child(self.i18n.t(tab.label_key()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.active_settings_tab = tab;
                    this.active_surface = ActiveSurface::Settings;
                    this.open_settings_select = None;
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn render_settings_tab_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .relative()
            .flex()
            .flex_col()
            .gap(px(self.tokens.metrics.settings_page_gap))
            .child(self.render_settings_page_header(self.active_settings_tab, cx))
            .child(separator(&self.tokens, SeparatorOrientation::Horizontal))
            .children(match self.active_settings_tab {
                SettingsTab::General => self.settings_general(cx),
                SettingsTab::Portable => self.settings_portable(cx),
                SettingsTab::Terminal => self.settings_terminal(cx),
                SettingsTab::Appearance => self.settings_appearance(cx),
                SettingsTab::Local => self.settings_local(cx),
                SettingsTab::Connections => self.settings_connections(cx),
                SettingsTab::Ssh => self.settings_ssh(),
                SettingsTab::Reconnect => self.settings_reconnect(cx),
                SettingsTab::Sftp => self.settings_sftp(cx),
                SettingsTab::Ide => self.settings_ide(cx),
                SettingsTab::Ai => self.settings_ai(cx),
                SettingsTab::Knowledge => self.settings_knowledge(cx),
                SettingsTab::Keybindings => self.settings_keybindings(cx),
                SettingsTab::Help => self.settings_help(cx),
            })
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
        edit(self.settings_store.settings_mut());
        let settings = self.settings_store.settings().clone();
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
        let _ = self.settings_store.save();
        self.sync_tab_titles(cx);
        cx.notify();
    }
}
