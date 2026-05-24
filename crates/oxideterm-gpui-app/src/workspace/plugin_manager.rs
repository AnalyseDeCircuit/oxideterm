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
            2 => self.render_native_plugin_registry_card(),
            _ => div().into_any_element(),
        }
    }

    fn render_native_plugin_registry_card(&self) -> AnyElement {
        let theme = self.tokens.ui;
        let plugin_rows = self.plugin_registry.plugins();
        let card = div()
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
            .gap(px(12.0))
            .min_h(px(260.0));

        if plugin_rows.is_empty() {
            return card
                .items_center()
                .justify_center()
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
                .into_any_element();
        }

        card.child(
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
                                .child("已发现插件"),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .line_height(px(20.0))
                                .text_color(rgb(theme.text_muted))
                                .child("native 只读取 manifest 和贡献点；legacy JS 插件不会在 GPUI 中执行。"),
                        ),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(format!("{} 个", plugin_rows.len())),
                ),
        )
        .children(plugin_rows.iter().map(|plugin| {
            self.render_native_plugin_registry_row(plugin)
        }))
        .into_any_element()
    }

    fn render_native_plugin_registry_row(
        &self,
        plugin: &plugin_host::NativePluginInfo,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (state_label, state_color, note) = match &plugin.runtime_plan {
            plugin_host::NativePluginRuntimePlan::ManifestOnly => {
                ("manifest", theme.text_muted, "仅声明贡献点")
            }
            plugin_host::NativePluginRuntimePlan::Wasm { .. } => {
                ("wasm", theme.success, "等待 native WASI runtime 接入")
            }
            plugin_host::NativePluginRuntimePlan::Process { .. } => {
                ("process", theme.success, "等待 native process runtime 接入")
            }
            plugin_host::NativePluginRuntimePlan::UnsupportedLegacyJs { .. } => {
                ("legacy-js", theme.warning, "Tauri ESM 插件：已发现但不执行")
            }
        };
        let contribution_summary = native_plugin_contribution_summary(&plugin.manifest);
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .p(px(14.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .text_size(px(self.tokens.metrics.ui_text_base))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(plugin.manifest.name.clone()),
                            )
                            .child(
                                div()
                                    .rounded_full()
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(state_color))
                                    .bg(rgb(theme.bg_card))
                                    .child(state_label),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .line_height(px(20.0))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                plugin
                                    .manifest
                                    .description
                                    .clone()
                                    .unwrap_or_else(|| plugin.manifest.id.clone()),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(contribution_summary),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .text_right()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child(format!("v{}", plugin.manifest.version))
                    .child(div().child(note)),
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

fn native_plugin_contribution_summary(manifest: &plugin_host::NativePluginManifest) -> String {
    let Some(contributes) = &manifest.contributes else {
        return "无声明贡献点".to_string();
    };

    let mut parts = Vec::new();
    if let Some(tabs) = &contributes.tabs {
        if !tabs.is_empty() {
            parts.push(format!("标签页 {}", tabs.len()));
        }
    }
    if let Some(sidebar_panels) = &contributes.sidebar_panels {
        if !sidebar_panels.is_empty() {
            parts.push(format!("侧边栏 {}", sidebar_panels.len()));
        }
    }
    if let Some(settings) = &contributes.settings {
        if !settings.is_empty() {
            parts.push(format!("设置 {}", settings.len()));
        }
    }
    if let Some(ai_tools) = &contributes.ai_tools {
        if !ai_tools.is_empty() {
            parts.push(format!("AI 工具 {}", ai_tools.len()));
        }
    }
    if contributes.terminal_hooks.as_ref().is_some_and(|hooks| {
        hooks.input_interceptor == Some(true) || hooks.output_processor == Some(true)
    }) {
        parts.push("终端 hook".to_string());
    }

    if parts.is_empty() {
        "无声明贡献点".to_string()
    } else {
        parts.join(" / ")
    }
}
