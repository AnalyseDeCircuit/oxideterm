use super::*;
use oxideterm_gpui_ui::text_input::{
    TextInputContentAlign, TextInputView, text_input_anchor_probe, text_input_with_content_align,
};
use std::{process::Command, sync::mpsc};

const PLUGIN_ID_CONFLICT_ERROR_PREFIX: &str = "PLUGIN_ID_CONFLICT:";
const PLUGIN_MANAGER_DELIVERY_POLL_INTERVAL: Duration = Duration::from_millis(50);
// Tauri PluginManagerView uses text-[11px] for URL hints and legal copy.
const PLUGIN_MANAGER_HINT_TEXT_SIZE: f32 = 11.0;
// Tauri plugin rows use tiny version pills and compact icon-only controls.
const PLUGIN_MANAGER_ROW_META_TEXT_SIZE: f32 = 10.0;
const PLUGIN_MANAGER_ACTION_ICON_SIZE: f32 = 14.0;
const PLUGIN_MANAGER_ROW_ACTION_SIZE: f32 = 28.0;
const PLUGIN_MANAGER_TW_ALPHA_10: u32 = 0x1a;
const PLUGIN_MANAGER_TW_ALPHA_20: u32 = 0x33;
const PLUGIN_MANAGER_TW_ALPHA_30: u32 = 0x4d;
const PLUGIN_MANAGER_TW_ALPHA_40: u32 = 0x66;
const PLUGIN_MANAGER_TW_ALPHA_50: u32 = 0x80;
// When Tauri's background image is active, theme cards keep Tailwind-like
// translucent surfaces so the plugin page does not become an opaque block.
const PLUGIN_MANAGER_BG_ACTIVE_THEME_ALPHA: u32 = 0x66;
const PLUGIN_MANAGER_BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf;
const PLUGIN_MANAGER_BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60;
const PLUGIN_MANAGER_TW_GREEN_400: u32 = 0x4ade80;
const PLUGIN_MANAGER_TW_GREEN_500: u32 = 0x22c55e;
const PLUGIN_MANAGER_TW_RED_400: u32 = 0xf87171;
const PLUGIN_MANAGER_TW_RED_500: u32 = 0xef4444;
const PLUGIN_MANAGER_TW_YELLOW_500: u32 = 0xeab308;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NativePluginManagerOperationStatus {
    Idle,
    Busy(String),
    Success(String),
    Error(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NativePluginPendingOverwrite {
    pub plugin_id: String,
    pub download_url: String,
    pub checksum: Option<String>,
}

pub(super) enum NativePluginManagerDelivery {
    Install {
        download_url: String,
        checksum: Option<String>,
        result: Result<plugin_host::NativePluginUrlInstallResult, String>,
    },
    CheckUpdates(Result<Vec<plugin_host::NativePluginRegistryEntry>, String>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativePluginManagerTab {
    Installed,
    Browse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativePluginManagerActionButtonTone {
    Accent,
    Muted,
}

impl WorkspaceApp {
    pub(super) fn open_plugin_manager_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bootstrap_native_plugin_runtime(cx);
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
        self.needs_active_pane_focus = false;
        window.focus(&self.focus_handle);
        self.reveal_active_tab(window);
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn render_plugin_manager_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.bootstrap_native_plugin_runtime(cx);
        let theme = self.tokens.ui;
        let has_background = self
            .terminal_background_preferences("plugin_manager")
            .is_some();
        let state = self.plugin_manager_section_list_state.clone();
        let workspace = cx.entity();
        let spec = TauriVirtualListSpec::new(
            px(PLUGIN_MANAGER_SECTION_LIST_ESTIMATED_HEIGHT),
            PLUGIN_MANAGER_SECTION_LIST_OVERSCAN,
        );
        div()
            .id("plugin-manager-scroll")
            .size_full()
            .bg(plugin_manager_root_bg(theme.bg, has_background))
            .text_color(rgb(theme.text))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    workspace.update(cx, |this, cx| {
                        this.render_plugin_manager_section_item(index, cx)
                    })
                },
            ))
            .into_any_element()
    }

    fn render_plugin_manager_section_item(
        &self,
        index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let padding = self.tokens.metrics.settings_content_padding;
        let gap = self.tokens.metrics.settings_page_gap;
        let outer_max_width = self.plugin_manager_content_outer_max_width();
        let mut content = div()
            .w_full()
            .min_w(px(0.0))
            .max_w(px(outer_max_width))
            .px(px(padding))
            .pb(px(gap));
        if index == 0 {
            content = content.pt(px(padding));
        }
        if index + 1 == PLUGIN_MANAGER_SECTION_LIST_ITEM_COUNT {
            content = content.pb(px(padding));
        }
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .justify_center()
            .child(content.child(self.render_plugin_manager_section(index, cx)))
            .into_any_element()
    }

    fn plugin_manager_content_outer_max_width(&self) -> f32 {
        // Tauri PluginManagerView uses `max-w-4xl mx-auto p-10`: the content box
        // is capped at 4xl and padding lives outside that width. GPUI list rows
        // need an explicit centered wrapper, otherwise max-width rows can stick
        // to the left edge in wide/fullscreen windows.
        self.tokens.metrics.settings_content_max_width
            + self.tokens.metrics.settings_content_padding * 2.0
    }

    fn render_plugin_manager_section(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self
            .terminal_background_preferences("plugin_manager")
            .is_some();
        match index {
            0 => div()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_2xl))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_heading))
                        .child(self.i18n.t("plugin.manager_title")),
                )
                .child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_base))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("plugin.manager_description")),
                )
                .into_any_element(),
            1 => div()
                .w_full()
                .h(px(1.0))
                .bg(rgb(theme.border))
                .into_any_element(),
            2 => self.render_native_plugin_actions_card(has_background, cx),
            3 => self.render_native_plugin_tabbed_content(has_background, cx),
            _ => div().into_any_element(),
        }
    }

    fn render_native_plugin_actions_card(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let plugin_count = self.plugin_registry.plugins().len();
        let active_count = self
            .plugin_registry
            .plugins()
            .iter()
            .filter(|plugin| plugin.state == plugin_host::NativePluginState::Active)
            .count();
        div()
            .w_full()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(plugin_manager_theme_border(theme.border, has_background))
            .bg(plugin_manager_theme_card_bg(theme.bg_card, has_background))
            // Tauri PluginManagerView uses the same SettingsView action card:
            // rounded-lg border bg-theme-bg-card p-5 with compact text buttons.
            .shadow(oxideterm_gpui_ui::tauri_card_shadow(theme.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t("plugin.manager_title").to_uppercase()),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::Puzzle,
                                        16.0,
                                        rgb(theme.accent),
                                    ))
                                    .child(
                                        self.i18n
                                            .t("plugin.footer")
                                            .replace("{{count}}", &plugin_count.to_string()),
                                    ),
                            )
                            .child(div().child("·"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(Self::render_lucide_icon(
                                        LucideIcon::CheckCircle,
                                        14.0,
                                        plugin_manager_palette_alpha(
                                            PLUGIN_MANAGER_TW_GREEN_400,
                                            0xff,
                                        ),
                                    ))
                                    .child(
                                        self.i18n
                                            .t("plugin.active_count")
                                            .replace("{{count}}", &active_count.to_string()),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(self.render_native_plugin_action_button(
                                LucideIcon::Plus,
                                self.i18n.t("plugin.create_plugin"),
                                NativePluginManagerActionButtonTone::Accent,
                                false,
                                |_event, _window, cx| {
                                    cx.stop_propagation();
                                },
                            ))
                            .child(self.render_native_plugin_action_button(
                                LucideIcon::FolderOpen,
                                self.i18n.t("plugin.open_plugins_dir"),
                                NativePluginManagerActionButtonTone::Muted,
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    if let Err(error) = open_native_plugins_dir(
                                        this.settings_store.path(),
                                        &this.i18n,
                                    ) {
                                        this.plugin_manager_operation_status =
                                            NativePluginManagerOperationStatus::Error(error);
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.render_native_plugin_action_button(
                                LucideIcon::RefreshCw,
                                self.i18n.t("plugin.refresh"),
                                NativePluginManagerActionButtonTone::Muted,
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    this.plugin_registry =
                                        plugin_host::NativePluginRegistry::discover(
                                            this.settings_store.path(),
                                        );
                                    this.plugin_manager_operation_status =
                                        NativePluginManagerOperationStatus::Success(
                                            this.i18n.t("plugin.refresh"),
                                        );
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_native_plugin_tabbed_content(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(24.0))
            .child(self.render_native_plugin_tab_bar(has_background, cx))
            .child(match self.plugin_manager_active_tab {
                NativePluginManagerTab::Installed => {
                    self.render_native_plugin_installed_card(has_background, cx)
                }
                NativePluginManagerTab::Browse => self.render_native_plugin_browse_content(cx),
            })
            .into_any_element()
    }

    fn render_native_plugin_tab_bar(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let plugin_count = self.plugin_registry.plugins().len();
        let update_count = self.plugin_manager_available_updates.len();
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(self.render_native_plugin_tab_button(
                NativePluginManagerTab::Installed,
                LucideIcon::Puzzle,
                self.i18n.t("plugin.tab_installed"),
                Some(plugin_count.to_string()),
                has_background,
                cx,
            ))
            .child(
                self.render_native_plugin_tab_button(
                    NativePluginManagerTab::Browse,
                    LucideIcon::Network,
                    self.i18n.t("plugin.tab_browse"),
                    (update_count > 0)
                        .then(|| format!("{update_count} {}", self.i18n.t("plugin.updates"))),
                    has_background,
                    cx,
                ),
            )
            .into_any_element()
    }

    fn render_native_plugin_tab_button(
        &self,
        tab: NativePluginManagerTab,
        icon: LucideIcon,
        label: String,
        badge: Option<String>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.plugin_manager_active_tab == tab;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(if active {
                plugin_manager_theme_border(theme.border, has_background)
            } else {
                plugin_manager_root_bg(theme.bg, has_background)
            })
            .bg(if active {
                plugin_manager_theme_panel_bg(theme.bg_panel, has_background)
            } else {
                plugin_manager_root_bg(theme.bg, has_background)
            })
            .px(px(16.0))
            .py(px(8.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(if active { theme.text } else { theme.text_muted }))
            .cursor(CursorStyle::PointingHand)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.plugin_manager_active_tab = tab;
                    cx.notify();
                }),
            )
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(if active {
                    theme.accent
                } else {
                    theme.text_muted
                }),
            ))
            .child(label)
            .when_some(badge, |button, badge| {
                button.child(
                    div()
                        .ml(px(4.0))
                        .rounded(px(self.tokens.radii.sm))
                        .border_1()
                        .border_color(if active {
                            rgb(theme.accent)
                        } else {
                            plugin_manager_theme_border_half(theme.border, has_background)
                        })
                        .bg(if active {
                            plugin_manager_theme_alpha(theme.accent, PLUGIN_MANAGER_TW_ALPHA_10)
                        } else {
                            plugin_manager_theme_panel_bg(theme.bg_panel, has_background)
                        })
                        .px(px(6.0))
                        .py(px(2.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        }))
                        .child(badge),
                )
            })
            .into_any_element()
    }

    fn render_native_plugin_installed_card(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let plugin_rows = self.plugin_registry.plugins().to_vec();
        let diagnostics = self.plugin_registry.diagnostics().to_vec();
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(plugin_manager_theme_border(theme.border, has_background))
            .bg(plugin_manager_theme_card_bg(theme.bg_card, has_background))
            // PluginManagerView uses bg-theme-bg-card, which carries
            // --theme-card-shadow in the Tauri theme.
            .shadow(oxideterm_gpui_ui::tauri_card_shadow(theme.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .min_h(px(260.0));

        if plugin_rows.is_empty() && diagnostics.is_empty() {
            return card
                .child(
                    div()
                        .min_h(px(180.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(10.0))
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
                                .child(self.i18n.t("plugin.empty_title")),
                        )
                        .child(
                            div()
                                .max_w(px(560.0))
                                .text_center()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .line_height(px(20.0))
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("plugin.empty_description")),
                        ),
                )
                .into_any_element();
        }

        let mut card = card
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t("plugin.empty_title")),
            )
            .children(
                diagnostics
                    .iter()
                    .map(|diagnostic| self.render_native_plugin_diagnostic_row(diagnostic)),
            );
        for (index, plugin) in plugin_rows.iter().enumerate() {
            card = card.child(self.render_native_plugin_registry_row(plugin, has_background, cx));
            if index + 1 < plugin_rows.len() {
                card = card.child(
                    div()
                        .w_full()
                        .h(px(1.0))
                        .bg(plugin_manager_theme_border_half(
                            theme.border,
                            has_background,
                        )),
                );
            }
        }
        card.into_any_element()
    }

    fn render_native_plugin_browse_content(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.render_native_plugin_package_manager(cx))
            .child(self.render_native_plugin_url_disclaimer())
            .into_any_element()
    }

    fn render_native_plugin_url_disclaimer(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(plugin_manager_theme_alpha(
                theme.border,
                PLUGIN_MANAGER_TW_ALPHA_40,
            ))
            .bg(plugin_manager_theme_alpha(
                theme.bg_panel,
                PLUGIN_MANAGER_TW_ALPHA_30,
            ))
            .p(px(16.0))
            .text_size(px(PLUGIN_MANAGER_HINT_TEXT_SIZE))
            .line_height(px(18.0))
            .text_color(rgb(theme.text_muted))
            .child(self.i18n.t("plugin.url_disclaimer"))
            .into_any_element()
    }

    fn render_native_plugin_package_manager(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let busy = matches!(
            self.plugin_manager_operation_status,
            NativePluginManagerOperationStatus::Busy(_)
        );
        div()
            .w_full()
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_card))
            // Tauri Browse tab shows URL install as its own SettingsView card.
            .shadow(oxideterm_gpui_ui::tauri_card_shadow(theme.bg_card))
            .p(px(self.tokens.metrics.settings_card_padding))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("plugin.url_install_title")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.url_install_desc")),
                    )
                    .child(
                        div()
                            .text_size(px(PLUGIN_MANAGER_HINT_TEXT_SIZE))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.url_version_hint")),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.render_native_plugin_manager_icon_input(
                        LucideIcon::Download,
                        SettingsInput::NativePluginInstallUrl,
                        self.i18n.t("plugin.url_placeholder"),
                        cx,
                    ))
                    .child(self.render_native_plugin_manager_button(
                        LucideIcon::Download,
                        self.i18n.t("plugin.install"),
                        busy || self.plugin_manager_install_url_draft.trim().is_empty(),
                        cx.listener(|this, _event, _window, cx| {
                            let download_url = this.plugin_manager_install_url_draft.clone();
                            let checksum = normalized_optional_string(
                                &this.plugin_manager_install_checksum_draft,
                            );
                            this.start_native_plugin_package_install(
                                download_url,
                                checksum,
                                false,
                                cx,
                            );
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(PLUGIN_MANAGER_HINT_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.url_checksum_label")),
                    )
                    .child(self.render_native_plugin_manager_labeled_input(
                        String::new(),
                        SettingsInput::NativePluginInstallChecksum,
                        self.i18n.t("plugin.url_checksum_placeholder"),
                        520.0,
                        cx,
                    ))
                    .child(
                        div()
                            .text_size(px(PLUGIN_MANAGER_HINT_TEXT_SIZE))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.url_checksum_hint")),
                    ),
            )
            .when_some(
                self.plugin_manager_pending_overwrite.as_ref(),
                |panel, pending| {
                    let confirm_download_url = pending.download_url.clone();
                    let confirm_checksum = pending.checksum.clone();
                    panel.child(
                        div()
                            .w_full()
                            .rounded(px(self.tokens.radii.md))
                            .border_1()
                            .border_color(rgb(theme.warning))
                            .bg(rgb(theme.bg_card))
                            .p(px(10.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .min_w(px(0.0))
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .line_height(px(18.0))
                                    .text_color(rgb(theme.warning))
                                    .child(
                                        self.i18n
                                            .t("plugin.url_conflict_desc")
                                            .replace("{{pluginId}}", &pending.plugin_id),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .gap(px(8.0))
                                    .child(self.render_native_plugin_manager_text_button(
                                        self.i18n.t("common.actions.cancel"),
                                        false,
                                        cx.listener(|this, _event, _window, cx| {
                                            this.plugin_manager_pending_overwrite = None;
                                            this.plugin_manager_operation_status =
                                                NativePluginManagerOperationStatus::Idle;
                                            cx.notify();
                                        }),
                                    ))
                                    .child(self.render_native_plugin_manager_text_button(
                                        self.i18n.t("plugin.url_conflict_confirm"),
                                        busy,
                                        cx.listener(move |this, _event, _window, cx| {
                                            this.start_native_plugin_package_install(
                                                confirm_download_url.clone(),
                                                confirm_checksum.clone(),
                                                true,
                                                cx,
                                            );
                                        }),
                                    )),
                            ),
                    )
                },
            )
            .child(self.render_native_plugin_registry_fetch_row(cx))
            .when(!self.plugin_manager_available_updates.is_empty(), |panel| {
                panel.child(
                    div().w_full().flex().flex_col().gap(px(8.0)).children(
                        self.plugin_manager_available_updates
                            .iter()
                            .map(|entry| self.render_native_plugin_update_row(entry, cx)),
                    ),
                )
            })
            .child(self.render_native_plugin_manager_status())
            .into_any_element()
    }

    fn render_native_plugin_registry_fetch_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let busy = matches!(
            self.plugin_manager_operation_status,
            NativePluginManagerOperationStatus::Busy(_)
        );
        div()
            .w_full()
            .pt(px(8.0))
            .border_t_1()
            .border_color(plugin_manager_theme_alpha(
                theme.border,
                PLUGIN_MANAGER_TW_ALPHA_40,
            ))
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(self.render_native_plugin_manager_icon_input(
                LucideIcon::Search,
                SettingsInput::NativePluginRegistryUrl,
                "https://example.com/registry.json".to_string(),
                cx,
            ))
            .child(self.render_native_plugin_manager_button(
                LucideIcon::RefreshCw,
                self.i18n.t("plugin.refresh"),
                busy || self.plugin_manager_registry_url_draft.trim().is_empty(),
                cx.listener(|this, _event, _window, cx| {
                    this.start_native_plugin_update_check(cx);
                }),
            ))
            .into_any_element()
    }

    fn render_native_plugin_action_button(
        &self,
        icon: LucideIcon,
        label: String,
        tone: NativePluginManagerActionButtonTone,
        disabled: bool,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (text_color, hover_bg) = match tone {
            NativePluginManagerActionButtonTone::Accent => (
                theme.accent,
                plugin_manager_theme_alpha(theme.accent, PLUGIN_MANAGER_TW_ALPHA_10),
            ),
            NativePluginManagerActionButtonTone::Muted => (theme.text_muted, rgb(theme.bg_panel)),
        };
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_card))
            .px(px(12.0))
            .py(px(6.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(if disabled {
                theme.text_muted
            } else {
                text_color
            }))
            .cursor(if disabled {
                CursorStyle::Arrow
            } else {
                CursorStyle::PointingHand
            })
            .when(!disabled, |button| {
                button
                    .hover(move |button| button.bg(hover_bg))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .child(Self::render_lucide_icon(
                icon,
                PLUGIN_MANAGER_ACTION_ICON_SIZE,
                rgb(if disabled {
                    theme.text_muted
                } else {
                    text_color
                }),
            ))
            .child(label)
            .into_any_element()
    }

    fn render_native_plugin_row_icon_button(
        &self,
        icon: LucideIcon,
        color: u32,
        listener: Option<impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let button = div()
            .size(px(PLUGIN_MANAGER_ROW_ACTION_SIZE))
            .rounded(px(self.tokens.radii.md))
            .flex()
            .items_center()
            .justify_center()
            .text_color(rgb(color))
            .cursor(if listener.is_some() {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .hover(move |button| button.bg(rgb(theme.bg_panel)))
            .child(Self::render_lucide_icon(
                icon,
                PLUGIN_MANAGER_ACTION_ICON_SIZE,
                rgb(color),
            ));
        if let Some(listener) = listener {
            button
                .on_mouse_down(MouseButton::Left, listener)
                .into_any_element()
        } else {
            button.into_any_element()
        }
    }

    fn render_native_plugin_manager_labeled_input(
        &self,
        label: String,
        input: SettingsInput,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .flex_col()
            .gap(px(5.0))
            .min_w(px(0.0))
            .when(!label.is_empty(), |field| {
                field.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_muted))
                        .child(label),
                )
            })
            .child(self.render_native_plugin_manager_text_input(input, placeholder, width, cx))
            .into_any_element()
    }

    fn render_native_plugin_manager_icon_input(
        &self,
        icon: LucideIcon,
        input: SettingsInput,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .relative()
            .flex_1()
            .min_w(px(0.0))
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top(px(10.0))
                    .child(Self::render_lucide_icon(icon, 16.0, rgb(theme.text_muted))),
            )
            .child(self.render_native_plugin_manager_text_input(input, placeholder, 640.0, cx))
            .into_any_element()
    }

    fn render_native_plugin_manager_text_input(
        &self,
        input: SettingsInput,
        placeholder: String,
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.current_settings_input_value(input)
        };
        let target = WorkspaceImeTarget::Settings(input);
        let workspace = cx.entity();
        // These fields are not persisted settings, but routing them through the
        // shared settings IME path keeps Plugin Manager text behavior identical
        // to Tauri-style form fields already used elsewhere in GPUI.
        text_input_anchor_probe(
            target.anchor_id(),
            text_input_with_content_align(
                &self.tokens,
                TextInputView {
                    value: &display_value,
                    placeholder,
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
                TextInputContentAlign::Start,
            )
            .w(px(width))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    let current = this.current_settings_input_value(input);
                    this.focus_settings_input(input, current, cx);
                    this.ime_marked_text = None;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(
                |this, event: &gpui::MouseMoveEvent, window, cx| {
                    this.update_ime_selection_drag_from_mouse_move(event, window, cx);
                },
            )),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_native_plugin_manager_button(
        &self,
        icon: LucideIcon,
        label: String,
        disabled: bool,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(if disabled {
                theme.bg_card
            } else {
                theme.accent
            }))
            .px(px(10.0))
            .py(px(7.0))
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(if disabled { theme.text_muted } else { theme.bg }))
            .cursor(if disabled {
                CursorStyle::Arrow
            } else {
                CursorStyle::PointingHand
            })
            .when(!disabled, |button| {
                button.on_mouse_down(MouseButton::Left, listener)
            })
            .child(Self::render_lucide_icon(
                icon,
                13.0,
                rgb(if disabled { theme.text_muted } else { theme.bg }),
            ))
            .child(label)
            .into_any_element()
    }

    fn render_native_plugin_manager_text_button(
        &self,
        label: String,
        disabled: bool,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_card))
            .px(px(10.0))
            .py(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(if disabled {
                theme.text_muted
            } else {
                theme.text
            }))
            .cursor(if disabled {
                CursorStyle::Arrow
            } else {
                CursorStyle::PointingHand
            })
            .when(!disabled, |button| {
                button.on_mouse_down(MouseButton::Left, listener)
            })
            .child(label)
            .into_any_element()
    }

    fn render_native_plugin_update_row(
        &self,
        entry: &plugin_host::NativePluginRegistryEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let busy = matches!(
            self.plugin_manager_operation_status,
            NativePluginManagerOperationStatus::Busy(_)
        );
        let download_url = entry.download_url.clone();
        let checksum = entry.checksum.clone();
        let capabilities = native_plugin_registry_capabilities_label(&self.i18n, entry);
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_card))
            .p(px(10.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(10.0))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(format!("{} v{}", entry.name, entry.version)),
                    )
                    .when_some(entry.description.as_ref(), |label, description| {
                        label.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .line_height(px(18.0))
                                .text_color(rgb(theme.text_muted))
                                .child(description.clone()),
                        )
                    })
                    .when_some(capabilities, |label, capabilities| {
                        label.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .line_height(px(18.0))
                                .text_color(rgb(theme.text_muted))
                                .child(capabilities),
                        )
                    }),
            )
            .child(self.render_native_plugin_manager_button(
                LucideIcon::Download,
                self.i18n.t("plugin.update"),
                busy,
                cx.listener(move |this, _event, _window, cx| {
                    this.start_native_plugin_package_install(
                        download_url.clone(),
                        checksum.clone(),
                        false,
                        cx,
                    );
                }),
            ))
            .into_any_element()
    }

    fn render_native_plugin_manager_status(&self) -> AnyElement {
        let theme = self.tokens.ui;
        let (icon, color, message) = match &self.plugin_manager_operation_status {
            NativePluginManagerOperationStatus::Idle => (
                LucideIcon::ShieldCheck,
                theme.text_muted,
                self.i18n.t("plugin.url_disclaimer"),
            ),
            NativePluginManagerOperationStatus::Busy(message) => {
                (LucideIcon::RefreshCw, theme.warning, message.clone())
            }
            NativePluginManagerOperationStatus::Success(message) => {
                (LucideIcon::CheckCircle, theme.success, message.clone())
            }
            NativePluginManagerOperationStatus::Error(message) => {
                (LucideIcon::ShieldAlert, theme.error, message.clone())
            }
        };
        div()
            .w_full()
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(18.0))
            .text_color(rgb(color))
            .child(Self::render_lucide_icon(icon, 14.0, rgb(color)))
            .child(message)
            .into_any_element()
    }

    fn start_native_plugin_package_install(
        &mut self,
        download_url: String,
        checksum: Option<String>,
        overwrite: bool,
        cx: &mut Context<Self>,
    ) {
        let download_url = download_url.trim().to_string();
        if download_url.is_empty() {
            self.plugin_manager_operation_status =
                NativePluginManagerOperationStatus::Error(self.i18n.t("plugin.url_invalid"));
            cx.notify();
            return;
        }
        if self.plugin_manager_delivery_rx.is_some() {
            self.plugin_manager_operation_status =
                NativePluginManagerOperationStatus::Busy(self.i18n.t("plugin.installing"));
            cx.notify();
            return;
        }

        let settings_path = self.settings_store.path().to_path_buf();
        let (tx, rx) = mpsc::channel();
        self.plugin_manager_delivery_rx = Some(rx);
        self.plugin_manager_operation_status =
            NativePluginManagerOperationStatus::Busy(self.i18n.t("plugin.installing"));
        if overwrite {
            self.plugin_manager_pending_overwrite = None;
        }
        self.schedule_native_plugin_manager_delivery_poll(cx);
        let delivery_download_url = download_url.clone();
        let delivery_checksum = checksum.clone();
        self.forwarding_runtime.spawn(async move {
            let result = plugin_host::NativePluginRegistry::install_plugin_package_from_url(
                &settings_path,
                &download_url,
                checksum.as_deref(),
                overwrite,
            )
            .await;
            let _ = tx.send(NativePluginManagerDelivery::Install {
                download_url: delivery_download_url,
                checksum: delivery_checksum,
                result,
            });
        });
    }

    fn start_native_plugin_update_check(&mut self, cx: &mut Context<Self>) {
        let registry_url = self.plugin_manager_registry_url_draft.trim().to_string();
        if registry_url.is_empty() {
            self.plugin_manager_operation_status =
                NativePluginManagerOperationStatus::Error(self.i18n.t("plugin.registry_error"));
            cx.notify();
            return;
        }
        if self.plugin_manager_delivery_rx.is_some() {
            self.plugin_manager_operation_status =
                NativePluginManagerOperationStatus::Busy(self.i18n.t("plugin.loading_registry"));
            cx.notify();
            return;
        }

        let installed = self
            .plugin_registry
            .plugins()
            .iter()
            .map(|plugin| plugin_host::NativePluginInstalledInfo {
                id: plugin.manifest.id.clone(),
                version: plugin.manifest.version.clone(),
            })
            .collect::<Vec<_>>();
        let (tx, rx) = mpsc::channel();
        self.plugin_manager_delivery_rx = Some(rx);
        self.plugin_manager_operation_status =
            NativePluginManagerOperationStatus::Busy(self.i18n.t("plugin.loading_registry"));
        self.schedule_native_plugin_manager_delivery_poll(cx);
        self.forwarding_runtime.spawn(async move {
            let result =
                match plugin_host::NativePluginRegistry::fetch_plugin_registry(&registry_url).await
                {
                    Ok(index) => Ok(plugin_host::NativePluginRegistry::check_plugin_updates(
                        index, &installed,
                    )),
                    Err(error) => Err(error),
                };
            let _ = tx.send(NativePluginManagerDelivery::CheckUpdates(result));
        });
    }

    fn schedule_native_plugin_manager_delivery_poll(&mut self, cx: &mut Context<Self>) {
        if self.plugin_manager_delivery_polling {
            return;
        }
        self.plugin_manager_delivery_polling = true;
        cx.spawn(async move |weak, cx| {
            loop {
                Timer::after(PLUGIN_MANAGER_DELIVERY_POLL_INTERVAL).await;
                let keep_polling = weak
                    .update(cx, |this, cx| {
                        this.poll_native_plugin_manager_delivery(cx);
                        this.plugin_manager_delivery_polling
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    fn poll_native_plugin_manager_delivery(&mut self, cx: &mut Context<Self>) {
        let Some(rx) = self.plugin_manager_delivery_rx.as_ref() else {
            self.plugin_manager_delivery_polling = false;
            return;
        };
        let mut deliveries = Vec::new();
        let mut disconnected = false;
        loop {
            match rx.try_recv() {
                Ok(delivery) => deliveries.push(delivery),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }
        for delivery in deliveries {
            self.handle_native_plugin_manager_delivery(delivery, cx);
        }
        if disconnected {
            self.plugin_manager_delivery_rx = None;
            self.plugin_manager_delivery_polling = false;
        }
        cx.notify();
    }

    fn handle_native_plugin_manager_delivery(
        &mut self,
        delivery: NativePluginManagerDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery {
            NativePluginManagerDelivery::Install {
                download_url,
                checksum,
                result,
            } => self.handle_native_plugin_install_result(download_url, checksum, result, cx),
            NativePluginManagerDelivery::CheckUpdates(result) => match result {
                Ok(updates) => {
                    let update_count = updates.len();
                    self.plugin_manager_available_updates = updates;
                    self.plugin_manager_operation_status =
                        NativePluginManagerOperationStatus::Success(format!(
                            "{update_count} {}",
                            self.i18n.t("plugin.updates")
                        ));
                }
                Err(error) => {
                    self.plugin_manager_operation_status =
                        NativePluginManagerOperationStatus::Error(error);
                }
            },
        }
    }

    fn handle_native_plugin_install_result(
        &mut self,
        download_url: String,
        checksum: Option<String>,
        result: Result<plugin_host::NativePluginUrlInstallResult, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(result) => {
                let installed_id = result.manifest.id.clone();
                self.plugin_registry =
                    plugin_host::NativePluginRegistry::discover(self.settings_store.path());
                self.bootstrap_native_plugin_runtime(cx);
                self.plugin_manager_available_updates
                    .retain(|entry| entry.id != installed_id);
                self.plugin_manager_pending_overwrite = None;
                self.plugin_manager_operation_status = NativePluginManagerOperationStatus::Success(
                    self.i18n
                        .t("plugin.url_install_success")
                        .replace("{{name}}", &result.manifest.name),
                );
            }
            Err(error) => {
                if let Some(plugin_id) = native_plugin_conflict_id(&error) {
                    // Tauri asks before overwriting an existing package. Native
                    // keeps the pending request so the confirmation button can
                    // retry with the same URL/checksum without retyping.
                    self.plugin_manager_pending_overwrite = Some(NativePluginPendingOverwrite {
                        plugin_id,
                        download_url,
                        checksum,
                    });
                    self.plugin_manager_operation_status =
                        NativePluginManagerOperationStatus::Error(
                            self.i18n.t("plugin.url_conflict_title"),
                        );
                } else {
                    self.plugin_manager_operation_status =
                        NativePluginManagerOperationStatus::Error(error);
                }
            }
        }
        cx.notify();
    }

    fn render_native_plugin_diagnostic_row(
        &self,
        diagnostic: &plugin_host::NativePluginDiagnostic,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let title = diagnostic
            .plugin_id
            .clone()
            .unwrap_or_else(|| diagnostic.plugin_dir.display().to_string());
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.error))
            .bg(rgb(theme.bg_panel))
            .p(px(14.0))
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(Self::render_lucide_icon(
                LucideIcon::AlertTriangle,
                16.0,
                rgb(theme.error),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.error))
                            .child(diagnostic.message.clone()),
                    ),
            )
            .into_any_element()
    }

    fn render_native_plugin_registry_row(
        &self,
        plugin: &plugin_host::NativePluginInfo,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (state_label, state_color) = native_plugin_status_badge(&self.i18n, plugin, theme);
        let error_message = native_plugin_visible_error(&self.i18n, plugin);
        let is_expanded = self
            .plugin_manager_expanded_plugin_ids
            .contains(&plugin.manifest.id);
        let is_active = native_plugin_is_active_like(plugin.state);
        let is_disabled = plugin.state == plugin_host::NativePluginState::Disabled;
        let is_error = native_plugin_is_error_like(plugin.state);
        let next_enabled = if !is_active && !is_disabled {
            false
        } else {
            is_disabled
        };
        let toggle_color = if next_enabled {
            theme.text_muted
        } else if is_active {
            PLUGIN_MANAGER_TW_GREEN_500
        } else {
            theme.text_muted
        };
        let plugin_id = plugin.manifest.id.clone();
        let expand_plugin_id = plugin.manifest.id.clone();
        let uninstall_plugin_id = plugin.manifest.id.clone();
        let reload_plugin_id = plugin.manifest.id.clone();
        // Tauri keeps plugin details collapsed by default. Native mirrors that
        // visual shape here; settings/details remain available through later
        // expansion work instead of being shown under every row.
        let mut row = div().w_full().flex().flex_col().gap(px(12.0)).child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        // Tauri's min-w-0 left column must also be flex-bounded
                        // in GPUI; otherwise long descriptions can overlap and
                        // intercept clicks intended for the right action group.
                        .flex_1()
                        .min_w(px(0.0))
                        .overflow_hidden()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .flex_shrink_0()
                                .text_color(rgb(theme.text_muted))
                                .cursor(CursorStyle::PointingHand)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _event, _window, cx| {
                                        if !this
                                            .plugin_manager_expanded_plugin_ids
                                            .insert(expand_plugin_id.clone())
                                        {
                                            this.plugin_manager_expanded_plugin_ids
                                                .remove(&expand_plugin_id);
                                        }
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                                .child(Self::render_lucide_icon(
                                    if is_expanded {
                                        LucideIcon::ChevronDown
                                    } else {
                                        LucideIcon::ChevronRight
                                    },
                                    16.0,
                                    rgb(theme.text_muted),
                                )),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .min_w(px(0.0))
                                                .truncate()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text))
                                                .child(plugin.manifest.name.clone()),
                                        )
                                        .child(
                                            div()
                                                .rounded(px(self.tokens.radii.sm))
                                                .bg(plugin_manager_theme_alpha(
                                                    theme.accent,
                                                    PLUGIN_MANAGER_TW_ALPHA_20,
                                                ))
                                                .px(px(6.0))
                                                .py(px(2.0))
                                                .text_size(px(PLUGIN_MANAGER_ROW_META_TEXT_SIZE))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.accent))
                                                .child(format!("v{}", plugin.manifest.version)),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .items_center()
                                                .gap(px(6.0))
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .text_color(rgb(theme.text_muted))
                                                .child(
                                                    div()
                                                        .size(px(8.0))
                                                        .rounded_full()
                                                        .bg(rgb(state_color)),
                                                )
                                                .child(state_label),
                                        ),
                                )
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .max_h(px(36.0))
                                        .overflow_hidden()
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .line_height(px(18.0))
                                        .text_color(rgb(theme.text_muted))
                                        .child(
                                            plugin
                                                .manifest
                                                .description
                                                .clone()
                                                .unwrap_or_else(|| plugin.manifest.id.clone()),
                                        ),
                                ),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .when(is_error || is_active, |right| {
                            right.child(self.render_native_plugin_row_icon_button(
                                LucideIcon::RefreshCw,
                                theme.text_muted,
                                Some(cx.listener(move |this, _event, _window, cx| {
                                    this.plugin_registry =
                                        plugin_host::NativePluginRegistry::discover(
                                            this.settings_store.path(),
                                        );
                                    this.bootstrap_native_plugin_runtime(cx);
                                    let success_template = this.i18n.t("plugin.reload_success");
                                    this.plugin_manager_operation_status =
                                        NativePluginManagerOperationStatus::Success(
                                            success_template.replace("{{name}}", &reload_plugin_id),
                                        );
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                            ))
                        })
                        .child(self.render_native_plugin_row_icon_button(
                            LucideIcon::Power,
                            toggle_color,
                            Some(cx.listener(move |this, _event, _window, cx| {
                                if let Err(error) = this
                                    .plugin_registry
                                    .set_plugin_enabled(&plugin_id, next_enabled)
                                {
                                    this.plugin_manager_operation_status =
                                        NativePluginManagerOperationStatus::Error(error.clone());
                                    this.plugin_registry
                                        .record_manager_error(plugin_id.clone(), error);
                                } else {
                                    if next_enabled {
                                        this.bootstrap_native_plugin_runtime(cx);
                                    }
                                    let success_key = if next_enabled {
                                        "plugin.enable_success"
                                    } else {
                                        "plugin.disable_success"
                                    };
                                    this.plugin_manager_operation_status =
                                        NativePluginManagerOperationStatus::Success(
                                            this.i18n
                                                .t(success_key)
                                                .replace("{{name}}", &plugin_id),
                                        );
                                }
                                cx.stop_propagation();
                                cx.notify();
                            })),
                        ))
                        .child(self.render_native_plugin_row_icon_button(
                            LucideIcon::Trash2,
                            theme.text_muted,
                            Some(cx.listener(move |this, _event, _window, cx| {
                                // Tauri's row deletes through the plugin API and leaves
                                // storage cleanup to the manager flow. Native mirrors the
                                // file removal path while preserving settings for now.
                                if let Err(error) = this
                                    .plugin_registry
                                    .uninstall_plugin(&uninstall_plugin_id, false)
                                {
                                    this.plugin_registry
                                        .record_manager_error(uninstall_plugin_id.clone(), error);
                                }
                                cx.stop_propagation();
                                cx.notify();
                            })),
                        )),
                ),
        );
        if let Some(error_message) = error_message {
            row = row.child(
                div()
                    .ml(px(28.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(plugin_manager_palette_alpha(
                        PLUGIN_MANAGER_TW_RED_500,
                        PLUGIN_MANAGER_TW_ALPHA_20,
                    ))
                    .bg(plugin_manager_palette_alpha(
                        PLUGIN_MANAGER_TW_RED_500,
                        PLUGIN_MANAGER_TW_ALPHA_10,
                    ))
                    .px(px(12.0))
                    .py(px(10.0))
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(plugin_manager_palette_alpha(
                        PLUGIN_MANAGER_TW_RED_400,
                        0xff,
                    ))
                    .child(Self::render_lucide_icon(
                        LucideIcon::AlertTriangle,
                        14.0,
                        plugin_manager_palette_alpha(PLUGIN_MANAGER_TW_RED_400, 0xff),
                    ))
                    .child(div().min_w(px(0.0)).child(error_message)),
            );
        }
        if is_expanded {
            row = row.child(self.render_native_plugin_expanded_details(plugin));
        }
        row.into_any_element()
    }

    fn render_native_plugin_expanded_details(
        &self,
        plugin: &plugin_host::NativePluginInfo,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let manifest = &plugin.manifest;
        let contribution_labels = native_plugin_contribution_labels(&self.i18n, manifest);
        let main_entry = manifest.main.clone().unwrap_or_else(|| "-".to_string());
        let required_version = manifest
            .engines
            .as_ref()
            .and_then(|engines| engines.oxideterm.clone());

        div()
            .ml(px(28.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(plugin_manager_theme_alpha(
                theme.border,
                PLUGIN_MANAGER_TW_ALPHA_50,
            ))
            .bg(plugin_manager_theme_alpha(
                theme.bg_panel,
                PLUGIN_MANAGER_TW_ALPHA_30,
            ))
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(18.0))
            .text_color(rgb(theme.text_muted))
            .when_some(manifest.description.clone(), |panel, description| {
                panel.child(div().text_color(rgb(theme.text_muted)).child(description))
            })
            // Tauri PluginRow renders a compact two-column detail grid. GPUI
            // mirrors that with fixed labels and flexible values.
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(self.render_native_plugin_detail_row("ID", manifest.id.clone()))
                    .child(self.render_native_plugin_detail_row(
                        self.i18n.t("plugin.detail_version"),
                        manifest.version.clone(),
                    ))
                    .child(self.render_native_plugin_detail_row(
                        self.i18n.t("plugin.detail_entry"),
                        main_entry,
                    ))
                    .when_some(manifest.author.clone(), |details, author| {
                        details.child(self.render_native_plugin_detail_row(
                            self.i18n.t("plugin.detail_author"),
                            author,
                        ))
                    })
                    .when_some(required_version, |details, version| {
                        details.child(self.render_native_plugin_detail_row(
                            self.i18n.t("plugin.detail_requires"),
                            format!("OxideTerm {version}"),
                        ))
                    }),
            )
            .when(!contribution_labels.is_empty(), |panel| {
                panel.child(
                    div()
                        .pt(px(8.0))
                        .border_t_1()
                        .border_color(plugin_manager_theme_alpha(
                            theme.border,
                            PLUGIN_MANAGER_TW_ALPHA_30,
                        ))
                        .flex()
                        .flex_col()
                        .gap(px(6.0))
                        .child(
                            div()
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(theme.text))
                                .child(self.i18n.t("plugin.detail_contributes")),
                        )
                        .child(div().flex().flex_wrap().gap(px(6.0)).children(
                            contribution_labels.into_iter().map(|label| {
                                div()
                                    .rounded_full()
                                    .bg(plugin_manager_theme_alpha(
                                        theme.accent,
                                        PLUGIN_MANAGER_TW_ALPHA_10,
                                    ))
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .text_size(px(PLUGIN_MANAGER_ROW_META_TEXT_SIZE))
                                    .text_color(rgb(theme.accent))
                                    .child(label)
                            }),
                        )),
                )
            })
            .into_any_element()
    }

    fn render_native_plugin_detail_row(
        &self,
        label: impl Into<String>,
        value: String,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = label.into();
        div()
            .flex()
            .items_start()
            .gap(px(16.0))
            .child(
                div()
                    .w(px(72.0))
                    .flex_shrink_0()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(label),
            )
            .child(div().min_w(px(0.0)).flex_1().child(value))
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

fn normalized_optional_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn native_plugin_conflict_id(error: &str) -> Option<String> {
    error
        .strip_prefix(PLUGIN_ID_CONFLICT_ERROR_PREFIX)
        .map(str::trim)
        .filter(|plugin_id| !plugin_id.is_empty())
        .map(str::to_string)
}

fn native_plugin_registry_capabilities_label(
    i18n: &I18n,
    entry: &plugin_host::NativePluginRegistryEntry,
) -> Option<String> {
    let capabilities = entry.capabilities_summary.as_ref()?;
    if capabilities.is_empty() {
        return None;
    }
    Some(
        i18n.t("plugin.registry_capabilities")
            .replace("{{capabilities}}", &capabilities.join(" / ")),
    )
}

fn native_plugin_contribution_labels(
    i18n: &I18n,
    manifest: &plugin_host::NativePluginManifest,
) -> Vec<String> {
    let Some(contributes) = manifest.contributes.as_ref() else {
        return Vec::new();
    };

    let mut labels = Vec::new();
    if let Some(tabs) = &contributes.tabs
        && !tabs.is_empty()
    {
        labels.push(
            i18n.t("plugin.contrib_tabs")
                .replace("{{count}}", &tabs.len().to_string()),
        );
    }
    if let Some(sidebar_panels) = &contributes.sidebar_panels
        && !sidebar_panels.is_empty()
    {
        labels.push(
            i18n.t("plugin.contrib_sidebar_panels")
                .replace("{{count}}", &sidebar_panels.len().to_string()),
        );
    }
    if let Some(settings) = &contributes.settings
        && !settings.is_empty()
    {
        labels.push(
            i18n.t("plugin.contrib_settings")
                .replace("{{count}}", &settings.len().to_string()),
        );
    }
    if let Some(terminal_hooks) = &contributes.terminal_hooks {
        if terminal_hooks.input_interceptor == Some(true) {
            labels.push(i18n.t("plugin.contrib_input_interceptor"));
        }
        if terminal_hooks.output_processor == Some(true) {
            labels.push(i18n.t("plugin.contrib_output_processor"));
        }
        if let Some(shortcuts) = &terminal_hooks.shortcuts
            && !shortcuts.is_empty()
        {
            labels.push(
                i18n.t("plugin.contrib_shortcuts")
                    .replace("{{count}}", &shortcuts.len().to_string()),
            );
        }
    }
    if let Some(connection_hooks) = &contributes.connection_hooks
        && !connection_hooks.is_empty()
    {
        labels.push(
            i18n.t("plugin.contrib_connection_hooks")
                .replace("{{count}}", &connection_hooks.len().to_string()),
        );
    }
    labels
}

fn native_plugin_is_active_like(state: plugin_host::NativePluginState) -> bool {
    matches!(
        state,
        plugin_host::NativePluginState::Active
            | plugin_host::NativePluginState::ReadyManifestOnly
            | plugin_host::NativePluginState::ReadyWasm
            | plugin_host::NativePluginState::ReadyProcess
    )
}

fn native_plugin_is_error_like(state: plugin_host::NativePluginState) -> bool {
    matches!(
        state,
        plugin_host::NativePluginState::Error | plugin_host::NativePluginState::AutoDisabled
    )
}

fn native_plugin_status_badge(
    i18n: &I18n,
    plugin: &plugin_host::NativePluginInfo,
    theme: AppUiColors,
) -> (String, u32) {
    match plugin.state {
        plugin_host::NativePluginState::Active
        | plugin_host::NativePluginState::ReadyManifestOnly
        | plugin_host::NativePluginState::ReadyWasm
        | plugin_host::NativePluginState::ReadyProcess => {
            (i18n.t("plugin.status.active"), PLUGIN_MANAGER_TW_GREEN_500)
        }
        plugin_host::NativePluginState::Loading => (i18n.t("plugin.status.loading"), theme.warning),
        plugin_host::NativePluginState::Error | plugin_host::NativePluginState::AutoDisabled => {
            (i18n.t("plugin.status.error"), PLUGIN_MANAGER_TW_RED_400)
        }
        plugin_host::NativePluginState::Disabled => (
            i18n.t("plugin.status.disabled"),
            PLUGIN_MANAGER_TW_YELLOW_500,
        ),
        plugin_host::NativePluginState::UnsupportedLegacyJs => (
            i18n.t("plugin.status.inactive"),
            PLUGIN_MANAGER_TW_YELLOW_500,
        ),
        plugin_host::NativePluginState::Discovered => {
            (i18n.t("plugin.status.inactive"), theme.text_muted)
        }
    }
}

fn native_plugin_visible_error(
    i18n: &I18n,
    plugin: &plugin_host::NativePluginInfo,
) -> Option<String> {
    if !matches!(
        plugin.state,
        plugin_host::NativePluginState::Error | plugin_host::NativePluginState::AutoDisabled
    ) {
        return None;
    }
    Some(
        plugin
            .config
            .last_error
            .clone()
            .unwrap_or_else(|| i18n.t("plugin.load_failed_default")),
    )
}

fn open_native_plugins_dir(settings_path: &std::path::Path, i18n: &I18n) -> Result<(), String> {
    let plugins_dir = plugin_host::native_plugins_dir(settings_path);
    std::fs::create_dir_all(&plugins_dir).map_err(|error| {
        i18n.t("plugin.open_dir_create_failed")
            .replace("{{message}}", &error.to_string())
    })?;
    let status = if cfg!(target_os = "macos") {
        Command::new("open").arg(&plugins_dir).status()
    } else if cfg!(target_os = "windows") {
        Command::new("explorer").arg(&plugins_dir).status()
    } else {
        Command::new("xdg-open").arg(&plugins_dir).status()
    }
    .map_err(|error| {
        i18n.t("plugin.open_dir_failed")
            .replace("{{message}}", &error.to_string())
    })?;
    if status.success() {
        Ok(())
    } else {
        Err(i18n
            .t("plugin.open_dir_status_failed")
            .replace("{{status}}", &status.to_string()))
    }
}

fn plugin_manager_root_bg(color: u32, has_background: bool) -> Rgba {
    if has_background {
        plugin_manager_palette_alpha(0x000000, 0x00)
    } else {
        rgb(color)
    }
}

// Tauri switches bg-theme-* surfaces to alpha-backed colors under
// data-bg-active; these helpers keep that contract centralized for native.
fn plugin_manager_theme_panel_bg(color: u32, has_background: bool) -> Rgba {
    plugin_manager_theme_card_bg(color, has_background)
}

fn plugin_manager_theme_card_bg(color: u32, has_background: bool) -> Rgba {
    oxideterm_gpui_ui::surface::color_for_background(
        color,
        has_background,
        PLUGIN_MANAGER_BG_ACTIVE_THEME_ALPHA,
    )
}

fn plugin_manager_theme_border(color: u32, has_background: bool) -> Rgba {
    oxideterm_gpui_ui::surface::color_for_background(
        color,
        has_background,
        PLUGIN_MANAGER_BG_ACTIVE_BORDER_ALPHA,
    )
}

fn plugin_manager_theme_border_half(color: u32, has_background: bool) -> Rgba {
    oxideterm_gpui_ui::surface::color_for_background_or_alpha(
        color,
        has_background,
        PLUGIN_MANAGER_BG_ACTIVE_BORDER_HALF_ALPHA,
        PLUGIN_MANAGER_TW_ALPHA_50,
    )
}

fn plugin_manager_theme_alpha(color: u32, alpha: u32) -> Rgba {
    rgba((color << 8) | alpha)
}

fn plugin_manager_palette_alpha(color: u32, alpha: u32) -> Rgba {
    rgba((color << 8) | alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_entry_with_capabilities(
        capabilities_summary: Option<Vec<String>>,
    ) -> plugin_host::NativePluginRegistryEntry {
        plugin_host::NativePluginRegistryEntry {
            id: "com.example.demo".to_string(),
            name: "Demo".to_string(),
            description: None,
            author: None,
            version: "1.2.0".to_string(),
            min_oxideterm_version: None,
            download_url: "https://example.invalid/demo.zip".to_string(),
            checksum: None,
            size: None,
            tags: None,
            capabilities_summary,
            homepage: None,
            updated_at: None,
        }
    }

    #[test]
    fn plugin_manager_conflict_error_preserves_plugin_id() {
        assert_eq!(
            native_plugin_conflict_id("PLUGIN_ID_CONFLICT:com.example.demo").as_deref(),
            Some("com.example.demo")
        );
        assert!(native_plugin_conflict_id("checksum mismatch").is_none());
    }

    #[test]
    fn plugin_manager_renders_registry_capabilities_summary() {
        let i18n = I18n::new(Locale::En);
        let entry = registry_entry_with_capabilities(Some(vec![
            "terminal read".to_string(),
            "status item".to_string(),
        ]));
        assert_eq!(
            native_plugin_registry_capabilities_label(&i18n, &entry).as_deref(),
            Some("Capabilities: terminal read / status item")
        );

        let entry = registry_entry_with_capabilities(Some(Vec::new()));
        assert!(native_plugin_registry_capabilities_label(&i18n, &entry).is_none());
    }
}
