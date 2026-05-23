use super::*;

impl WorkspaceApp {
    pub(super) fn open_plugin_manager_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::PluginManager)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::PluginManager,
                title: self.i18n.t("plugin.manager_title"),
                title_source: TabTitleSource::I18nKey("plugin.manager_title"),
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

    pub(super) fn render_plugin_manager_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let state = self.plugin_manager_section_list_state.clone();
        let workspace = cx.entity();
        let spec = TauriVirtualListSpec::new(
            px(PLUGIN_MANAGER_SECTION_LIST_ESTIMATED_HEIGHT),
            PLUGIN_MANAGER_SECTION_LIST_OVERSCAN,
        );
        div()
            .id("plugin-manager-scroll")
            .size_full()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, _cx| {
                        this.render_plugin_manager_section_item(index)
                    })
                },
            ))
            .into_any_element()
    }

    fn render_plugin_manager_section_item(&self, index: usize) -> AnyElement {
        let padding = self.tokens.metrics.settings_content_padding;
        let gap = self.tokens.metrics.settings_page_gap;
        div()
            .w_full()
            .max_w(px(self.tokens.metrics.settings_content_max_width))
            .mx_auto()
            .px(px(padding))
            .pb(px(gap))
            .when(index == 0, |item| item.pt(px(padding)))
            .when(
                index + 1 == PLUGIN_MANAGER_SECTION_LIST_ITEM_COUNT,
                |item| item.pb(px(padding)),
            )
            .child(self.render_plugin_manager_section(index))
            .into_any_element()
    }

    fn render_plugin_manager_section(&self, index: usize) -> AnyElement {
        let theme = self.tokens.ui;
        match index {
            0 => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(24.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_heading))
                        .child(self.i18n.t("plugin.manager_title")),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_base))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("plugin.native_description")),
                )
                .into_any_element(),
            1 => div()
                .w_full()
                .h(px(1.0))
                .bg(rgb(theme.border))
                .into_any_element(),
            2 => div()
                .w_full()
                .min_w(px(0.0))
                .rounded(px(self.tokens.radii.lg))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.bg_card))
                // PluginManagerView uses bg-theme-bg-card, which carries
                // --theme-card-shadow in the Tauri theme.
                .shadow(oxideterm_gpui_ui::tauri_card_shadow(theme.bg_card))
                .p(px(self.tokens.metrics.settings_card_padding))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(12.0))
                .min_h(px(260.0))
                .child(Self::render_lucide_icon(
                    LucideIcon::Puzzle,
                    36.0,
                    rgb(theme.text_muted),
                ))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_base))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t("plugin.native_empty_title")),
                )
                .child(
                    div()
                        .max_w(px(560.0))
                        .text_center()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .line_height(px(20.0))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("plugin.native_empty_description")),
                )
                .child(
                    div()
                        .mt(px(6.0))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("plugin.native_runtime_note"))
                        .child(self.i18n.t("plugin.native_webview_note"))
                        .child(self.i18n.t("plugin.native_api_note")),
                )
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }

    pub(super) fn render_plugin_sidebar_placeholder(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .px(px(self.tokens.metrics.empty_sidebar_padding_x))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .w_full()
                    .h(px(self.tokens.metrics.empty_sidebar_height))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .child(div().mb_3().child(Self::render_lucide_icon(
                        LucideIcon::Puzzle,
                        self.tokens.metrics.empty_sidebar_icon_size,
                        rgb(theme.text_muted),
                    )))
                    .child(
                        div()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_title_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.native_sidebar_empty_title")),
                    )
                    .child(
                        div()
                            .mt_1()
                            .w_full()
                            .text_center()
                            .text_size(px(self.tokens.metrics.empty_sidebar_subtitle_font_size))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.native_sidebar_empty_description")),
                    ),
            )
            .into_any_element()
    }
}
