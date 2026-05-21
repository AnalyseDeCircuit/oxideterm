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

    pub(super) fn render_plugin_manager_surface(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .id("plugin-manager-scroll")
            .size_full()
            .selectable_overflow_y_scrollbar(
                &self.selectable_text_scroll_handle("plugin-manager-scroll"),
            )
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
                    .child(
                        div()
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
                            ),
                    )
                    .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
                    .child(
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .rounded(px(self.tokens.radii.lg))
                            .border_1()
                            .border_color(rgb(theme.border))
                            .bg(rgb(theme.bg_card))
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
                            ),
                    ),
            )
            .into_any_element()
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
