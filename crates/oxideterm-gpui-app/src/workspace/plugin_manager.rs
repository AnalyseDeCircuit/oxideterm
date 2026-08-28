use super::*;
use gpui::Div;
use oxideterm_gpui_ui::{
    ActionSlotRowOptions, StatusPillOptions, StatusTone, SurfaceKind, SurfaceOptions,
    SurfacePadding, action_slot_row, semantic_surface, status_pill,
    text_input::{TextInputContentAlign, TextInputView, text_input_with_content_align},
};
use std::process::Command;
use zeroize::Zeroizing;

const PLUGIN_MANAGER_SECTION_LIST_ITEM_COUNT: usize = 5;
pub(in crate::workspace) const PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX: usize = 3;
const PLUGIN_MANAGER_SECTION_LIST_ESTIMATED_HEIGHT: f32 = 220.0;
const PLUGIN_MANAGER_SECTION_LIST_OVERSCAN: usize = 1;
// Tauri PluginManagerView uses text-[11px] for URL hints and legal copy.
const PLUGIN_MANAGER_HINT_TEXT_SIZE: f32 = 11.0;
// Tauri plugin rows use tiny version pills and compact icon-only controls.
const PLUGIN_MANAGER_ROW_META_TEXT_SIZE: f32 = 10.0;
const PLUGIN_MANAGER_ACTION_ICON_SIZE: f32 = 14.0;
const PLUGIN_MANAGER_ROW_ACTION_SIZE: f32 = 28.0;
const PLUGIN_MANAGER_INLINE_INPUT_BASIS: f32 = 280.0;
const PLUGIN_MANAGER_TAB_BAR_WIDTH: f32 = 300.0; // Two equal header tabs preserve room for localized labels and badges.
#[cfg(windows)]
const PLUGIN_MANAGER_EXTERNAL_BRIDGE_CREATE_NO_WINDOW: u32 = 0x08000000;
const PLUGIN_MANAGER_TW_ALPHA_10: u32 = 0x1a;
const PLUGIN_MANAGER_TW_ALPHA_20: u32 = 0x33;
const PLUGIN_MANAGER_TW_ALPHA_30: u32 = 0x4d;
const PLUGIN_MANAGER_TW_ALPHA_40: u32 = 0x66;
const PLUGIN_MANAGER_TW_ALPHA_50: u32 = 0x80;
// When Tauri's background image is active, theme cards keep Tailwind-like
// translucent surfaces so the plugin page does not become an opaque block.
const PLUGIN_MANAGER_BG_ACTIVE_THEME_ALPHA: u32 = 0x66;
const PLUGIN_MANAGER_BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60;
const OFFICIAL_PLUGIN_MARKETPLACE_HOME: &str =
    "https://github.com/AnalyseDeCircuit/oxideterm-plugins";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum NativePluginManagerOperationStatus {
    Idle,
    Busy(String),
    Success(String),
    Error(String),
}

#[derive(PartialEq, Eq)]
pub(super) struct NativePluginPendingOverwrite {
    pub plugin_id: String,
    pub expected_id: Option<String>,
    pub download_url: Zeroizing<String>,
    pub checksum: Option<String>,
}

impl std::fmt::Debug for NativePluginPendingOverwrite {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NativePluginPendingOverwrite")
            .field("plugin_id", &self.plugin_id)
            .field("expected_id", &self.expected_id)
            .field("download_url", &"<redacted>")
            .field("checksum_present", &self.checksum.is_some())
            .finish()
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct NativePluginDiagnosticKey {
    plugin_dir: PathBuf,
    plugin_id: Option<String>,
    message: String,
}

impl From<&plugin_host::NativePluginDiagnostic> for NativePluginDiagnosticKey {
    fn from(diagnostic: &plugin_host::NativePluginDiagnostic) -> Self {
        // Use the complete diagnostic identity so dismissing one warning cannot hide a later, different failure.
        Self {
            plugin_dir: diagnostic.plugin_dir.clone(),
            plugin_id: diagnostic.plugin_id.clone(),
            message: diagnostic.message.clone(),
        }
    }
}

pub(in crate::workspace) enum NativePluginManagerDelivery {
    Install {
        expected_id: Option<String>,
        download_url: Zeroizing<String>,
        checksum: Option<String>,
        outcome: NativePluginInstallOutcome,
    },
    LoadMarketplace(Option<plugin_host::NativePluginRegistryIndex>),
    CheckUpdates(Option<Vec<plugin_host::NativePluginRegistryEntry>>),
}

pub(in crate::workspace) enum NativePluginInstallOutcome {
    Installed(plugin_host::NativePluginUrlInstallResult),
    Conflict { plugin_id: String },
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativePluginManagerTab {
    Installed,
    Marketplace,
}

fn native_plugin_manager_tab_index(tab: NativePluginManagerTab) -> usize {
    match tab {
        NativePluginManagerTab::Installed => 0,
        NativePluginManagerTab::Marketplace => 1,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum NativePluginMarketplaceLoadState {
    #[default]
    NotLoaded,
    Loading,
    Loaded,
    Failed,
}

/// Owns the native plugin management and plugin-sidebar UI state.
pub(super) struct NativePluginManagerState {
    pub(super) section_list_state: ListState,
    pub(super) active_tab: NativePluginManagerTab,
    pub(super) previous_tab: NativePluginManagerTab,
    pub(super) install_url_draft: String,
    pub(super) install_checksum_draft: String,
    pub(super) registry_url_draft: String,
    pub(super) marketplace_search_draft: String,
    pub(super) marketplace_entries: Vec<plugin_host::NativePluginRegistryEntry>,
    pub(super) marketplace_load_state: NativePluginMarketplaceLoadState,
    pub(super) available_updates: Vec<plugin_host::NativePluginRegistryEntry>,
    pub(super) operation_status: NativePluginManagerOperationStatus,
    pub(super) pending_overwrite: Option<NativePluginPendingOverwrite>,
    pub(super) expanded_plugin_ids: HashSet<String>,
    dismissed_diagnostic_keys: HashSet<NativePluginDiagnosticKey>,
    pub(super) active_sidebar_panel: Option<plugin_ui::NativePluginSidebarPanelSelection>,
}

impl NativePluginManagerState {
    pub(super) fn new() -> Self {
        Self {
            // Plugin Manager is a browser-style page with a small set of
            // variable-height sections, so it owns its virtual-list state.
            section_list_state: ListState::new(
                PLUGIN_MANAGER_SECTION_LIST_ITEM_COUNT,
                ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(PLUGIN_MANAGER_SECTION_LIST_ESTIMATED_HEIGHT),
                    PLUGIN_MANAGER_SECTION_LIST_OVERSCAN,
                )
                .overdraw(),
            )
            .measure_all(),
            active_tab: NativePluginManagerTab::Installed,
            previous_tab: NativePluginManagerTab::Installed,
            install_url_draft: String::new(),
            install_checksum_draft: String::new(),
            registry_url_draft: String::new(),
            marketplace_search_draft: String::new(),
            marketplace_entries: Vec::new(),
            marketplace_load_state: NativePluginMarketplaceLoadState::NotLoaded,
            available_updates: Vec::new(),
            operation_status: NativePluginManagerOperationStatus::Idle,
            pending_overwrite: None,
            expanded_plugin_ids: HashSet::new(),
            dismissed_diagnostic_keys: HashSet::new(),
            active_sidebar_panel: None,
        }
    }
}

impl WorkspaceApp {
    pub(in crate::workspace) fn plugin_manager_state<'a>(
        &self,
        cx: &'a App,
    ) -> &'a NativePluginManagerState {
        // The workspace reads manager presentation data without mirroring its
        // business state outside the plugin Entity.
        self.plugin_entity.read(cx).manager_state()
    }

    pub(in crate::workspace) fn update_plugin_manager_state<R>(
        &self,
        cx: &mut Context<Self>,
        update: impl FnOnce(&mut NativePluginManagerState) -> R,
    ) -> R {
        self.plugin_entity
            .update(cx, |plugins, _cx| update(plugins.manager_state_mut()))
    }

    pub(super) fn open_plugin_manager_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.bootstrap_native_plugin_runtime(cx);
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::PluginManager)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::PluginManager,
                    title: self.i18n.t("plugin.manager_title"),
                    title_source: TabTitleSource::I18nKey("plugin.manager_title"),
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

    pub(super) fn render_plugin_manager_surface(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.bootstrap_native_plugin_runtime(cx);
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("plugin_manager");
        let state = self.plugin_manager_state(cx).section_list_state.clone();
        let workspace = cx.entity();
        let spec = TauriVirtualListSpec::new(
            px(PLUGIN_MANAGER_SECTION_LIST_ESTIMATED_HEIGHT),
            PLUGIN_MANAGER_SECTION_LIST_OVERSCAN,
        );
        div()
            .id("plugin-manager-scroll")
            .size_full()
            .min_w(px(0.0))
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
        let mut content = div().w_full().min_w(px(0.0)).px(px(padding)).pb(px(gap));
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
            .child(content.child(self.render_plugin_manager_section(index, cx)))
            .into_any_element()
    }

    fn render_plugin_manager_section(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.background_surface_active("plugin_manager");
        match index {
            // Keep page-level navigation beside the title and wrap it as one group on narrow views.
            0 => div()
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
                                // Page titles use regular weight so the large type does not look over-emphasized.
                                .font_weight(gpui::FontWeight::NORMAL)
                                .text_color(rgb(theme.text_heading))
                                .child(self.i18n.t("plugin.manager_title")),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_base))
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("plugin.manager_description")),
                        ),
                )
                .child(self.render_native_plugin_tab_bar(has_background, cx))
                .into_any_element(),
            1 => div()
                .w_full()
                .h(px(1.0))
                .bg(rgb(theme.border))
                .into_any_element(),
            2 => self.render_native_plugin_actions_card(has_background, cx),
            PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX => {
                self.render_native_plugin_tabbed_content(has_background, cx)
            }
            4 => self.render_native_plugin_compatibility_notice(has_background),
            _ => div().into_any_element(),
        }
    }

    fn render_native_plugin_compatibility_notice(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
        self.native_plugin_card_surface(has_background)
            .flex()
            .items_start()
            .gap(px(12.0))
            .child(Self::render_lucide_icon(
                LucideIcon::Info,
                18.0,
                rgb(theme.accent),
            ))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("plugin.compatibility_title")),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.manager_compatibility_notice")),
                    ),
            )
            .into_any_element()
    }

    fn native_plugin_card_surface(&self, has_background: bool) -> Div {
        semantic_surface(
            &self.tokens,
            SurfaceOptions::new(SurfaceKind::Inspector)
                .padding(SurfacePadding::Spacious)
                .has_background_image(has_background),
        )
        .w_full()
        .min_w(px(0.0))
    }

    fn render_native_plugin_actions_card(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (plugin_count, active_count) = {
            let plugins = self.plugin_entity.read(cx);
            let plugin_rows = plugins.registry().plugins();
            (
                plugin_rows.len(),
                plugin_rows
                    .iter()
                    .filter(|plugin| plugin.state == plugin_host::NativePluginState::Active)
                    .count(),
            )
        };
        self.native_plugin_card_surface(has_background)
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
                                        rgb(theme.success),
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
                                LucideIcon::FolderOpen,
                                self.i18n.t("plugin.open_plugins_dir"),
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    if let Err(error) = open_native_plugins_dir(
                                        this.settings_store.path(),
                                        &this.i18n,
                                    ) {
                                        this.update_plugin_manager_state(cx, |manager| {
                                            manager.operation_status =
                                                NativePluginManagerOperationStatus::Error(error);
                                        });
                                    }
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.render_native_plugin_action_button(
                                LucideIcon::RefreshCw,
                                self.i18n.t("plugin.refresh"),
                                false,
                                cx.listener(|this, _event, _window, cx| {
                                    let registry = plugin_host::NativePluginRegistry::discover(
                                        this.settings_store.path(),
                                    );
                                    this.plugin_entity.update(cx, |plugins, _cx| {
                                        plugins.replace_registry(registry);
                                    });
                                    let refreshed = this.i18n.t("plugin.refresh");
                                    this.update_plugin_manager_state(cx, |manager| {
                                        manager.operation_status =
                                            NativePluginManagerOperationStatus::Success(refreshed);
                                    });
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
        match self.plugin_manager_state(cx).active_tab {
            NativePluginManagerTab::Installed => {
                self.render_native_plugin_installed_card(has_background, cx)
            }
            NativePluginManagerTab::Marketplace => {
                self.render_native_plugin_marketplace_content(has_background, cx)
            }
        }
    }

    fn render_native_plugin_tab_bar(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let plugin_count = self.plugin_entity.read(cx).registry().plugins().len();
        let (update_count, active_tab, previous_tab) = {
            let manager = self.plugin_manager_state(cx);
            (
                manager.available_updates.len(),
                manager.active_tab,
                manager.previous_tab,
            )
        };
        let items = vec![
            self.render_native_plugin_tab_button(
                NativePluginManagerTab::Installed,
                LucideIcon::Puzzle,
                self.i18n.t("plugin.tab_installed"),
                Some(plugin_count.to_string()),
                has_background,
                cx,
            ),
            self.render_native_plugin_tab_button(
                NativePluginManagerTab::Marketplace,
                LucideIcon::Network,
                self.i18n.t("plugin.tab_marketplace"),
                (update_count > 0)
                    .then(|| format!("{update_count} {}", self.i18n.t("plugin.updates"))),
                has_background,
                cx,
            ),
        ];
        let active_index = native_plugin_manager_tab_index(active_tab);
        let previous_index = native_plugin_manager_tab_index(previous_tab);
        oxideterm_gpui_ui::segmented_control(
            &self.tokens,
            selection_motion::PLUGIN_MANAGER_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(active_index, previous_index, 2)
                .user_transition_active(self.segmented_control_user_transition_active(
                    selection_motion::PLUGIN_MANAGER_SWITCHER_ID,
                    active_index,
                ))
                .has_background_image(has_background)
                .compact(PLUGIN_MANAGER_TAB_BAR_WIDTH),
            items,
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
        let active = self.plugin_manager_state(cx).active_tab == tab;
        let content = div()
            .w_full()
            .py(px(2.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(8.0))
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
            .when_some(badge, |content, badge| {
                content.child(
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
            });
        oxideterm_gpui_ui::segmented_control_item_content(
            &self.tokens,
            active,
            content.into_any_element(),
        )
        .font_weight(gpui::FontWeight::MEDIUM)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                let changed = this.update_plugin_manager_state(cx, |manager| {
                    if manager.active_tab == tab {
                        return false;
                    }
                    manager.previous_tab = manager.active_tab;
                    manager.active_tab = tab;
                    true
                });
                if changed {
                    this.begin_user_segmented_control_transition(
                        selection_motion::PLUGIN_MANAGER_SWITCHER_ID,
                        native_plugin_manager_tab_index(tab),
                        cx,
                    );
                    if tab == NativePluginManagerTab::Marketplace
                        && matches!(
                            this.plugin_manager_state(cx).marketplace_load_state,
                            NativePluginMarketplaceLoadState::NotLoaded
                                | NativePluginMarketplaceLoadState::Failed
                        )
                    {
                        this.start_native_plugin_marketplace_load(cx);
                    }
                }
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_native_plugin_installed_card(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (plugin_rows, diagnostics) = {
            let plugins = self.plugin_entity.read(cx);
            let registry = plugins.registry();
            let dismissed_diagnostic_keys = &plugins.manager_state().dismissed_diagnostic_keys;
            (
                registry.plugins().to_vec(),
                registry
                    .diagnostics()
                    .iter()
                    .filter(|diagnostic| {
                        native_plugin_diagnostic_is_visible(diagnostic, dismissed_diagnostic_keys)
                    })
                    .cloned()
                    .collect::<Vec<_>>(),
            )
        };
        let card = self
            .native_plugin_card_surface(has_background)
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
                    .map(|diagnostic| self.render_native_plugin_diagnostic_row(diagnostic, cx)),
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

    fn render_native_plugin_marketplace_content(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.render_native_plugin_marketplace(has_background, cx))
            .child(self.render_native_plugin_package_manager(has_background, cx))
            .child(self.render_native_plugin_url_disclaimer())
            .into_any_element()
    }

    fn render_native_plugin_marketplace(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (entries, query, load_state, busy) = {
            let manager = self.plugin_manager_state(cx);
            (
                manager.marketplace_entries.clone(),
                manager.marketplace_search_draft.clone(),
                manager.marketplace_load_state,
                matches!(
                    manager.operation_status,
                    NativePluginManagerOperationStatus::Busy(_)
                ),
            )
        };
        let query = query.trim().to_lowercase();
        let visible_entries = entries
            .into_iter()
            .filter(|entry| native_plugin_marketplace_entry_matches(entry, &query))
            .collect::<Vec<_>>();
        let entry_count = visible_entries.len();

        let mut card = self
            .native_plugin_card_surface(has_background)
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.i18n.t("plugin.marketplace_title")),
                            )
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .line_height(px(18.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("plugin.marketplace_description")),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(self.render_native_plugin_action_button(
                                LucideIcon::ExternalLink,
                                self.i18n.t("plugin.marketplace_repository"),
                                false,
                                |_event, _window, cx| {
                                    cx.open_url(OFFICIAL_PLUGIN_MARKETPLACE_HOME);
                                },
                            ))
                            .child(self.render_native_plugin_action_button(
                                LucideIcon::RefreshCw,
                                self.i18n.t("plugin.refresh"),
                                busy,
                                cx.listener(|this, _event, _window, cx| {
                                    this.start_native_plugin_marketplace_load(cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            )
            .child(self.render_native_plugin_manager_icon_input(
                LucideIcon::Search,
                SettingsInput::NativePluginMarketplaceSearch,
                self.i18n.t("plugin.search_placeholder"),
                cx,
            ));

        if visible_entries.is_empty() {
            let (icon, message, color) = match load_state {
                NativePluginMarketplaceLoadState::NotLoaded
                | NativePluginMarketplaceLoadState::Loading => (
                    LucideIcon::RefreshCw,
                    self.i18n.t("plugin.loading_marketplace"),
                    theme.text_muted,
                ),
                NativePluginMarketplaceLoadState::Failed => (
                    LucideIcon::ShieldAlert,
                    self.i18n.t("plugin.marketplace_load_error"),
                    theme.error,
                ),
                NativePluginMarketplaceLoadState::Loaded if query.is_empty() => (
                    LucideIcon::Puzzle,
                    self.i18n.t("plugin.marketplace_empty"),
                    theme.text_muted,
                ),
                NativePluginMarketplaceLoadState::Loaded => (
                    LucideIcon::Search,
                    self.i18n.t("plugin.no_search_results"),
                    theme.text_muted,
                ),
            };
            return card
                .child(
                    div()
                        .min_h(px(150.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(10.0))
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(color))
                        .child(Self::render_lucide_icon(icon, 24.0, rgb(color)))
                        .child(message),
                )
                .into_any_element();
        }

        card = card
            .child(
                div()
                    .text_size(px(PLUGIN_MANAGER_HINT_TEXT_SIZE))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        self.i18n
                            .t("plugin.marketplace_result_count")
                            .replace("{{count}}", &entry_count.to_string()),
                    ),
            )
            .children(
                visible_entries.iter().map(|entry| {
                    self.render_native_plugin_marketplace_row(entry, has_background, cx)
                }),
            );
        card.into_any_element()
    }

    fn render_native_plugin_marketplace_row(
        &self,
        entry: &plugin_host::NativePluginRegistryEntry,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let installed_version = self
            .plugin_entity
            .read(cx)
            .registry()
            .plugins()
            .iter()
            .find(|plugin| plugin.manifest.id == entry.id)
            .map(|plugin| plugin.manifest.version.clone());
        let update_available = installed_version.as_deref().is_some_and(|version| {
            plugin_host::NativePluginRegistry::registry_entry_is_update(entry, version)
        });
        let host_supported =
            plugin_host::NativePluginRegistry::registry_entry_supports_current_host(entry);
        let version_supported =
            plugin_host::NativePluginRegistry::registry_entry_supports_current_version(entry);
        let package = plugin_host::NativePluginRegistry::resolve_registry_package(entry).ok();
        let package_verified = package
            .as_ref()
            .is_some_and(|package| !package.checksum.trim().is_empty());
        let busy = self.plugin_entity.read(cx).manager_operation_in_flight();
        let installed = installed_version.is_some();
        let action_disabled = busy
            || !host_supported
            || !version_supported
            || !package_verified
            || (installed && !update_available);
        let (action_label, action_icon) = if update_available {
            (self.i18n.t("plugin.update"), LucideIcon::RefreshCw)
        } else if installed {
            (self.i18n.t("plugin.installed"), LucideIcon::CheckCircle)
        } else {
            (self.i18n.t("plugin.install"), LucideIcon::Download)
        };
        let availability = if !host_supported {
            Some((
                self.i18n.t("plugin.marketplace_platform_unavailable"),
                StatusTone::Warning,
            ))
        } else if !version_supported {
            Some((
                entry
                    .min_oxideterm_version
                    .as_ref()
                    .map(|version| {
                        self.i18n
                            .t("plugin.marketplace_requires_version")
                            .replace("{{version}}", version)
                    })
                    .unwrap_or_else(|| self.i18n.t("plugin.marketplace_incompatible")),
                StatusTone::Warning,
            ))
        } else if !package_verified {
            Some((
                self.i18n.t("plugin.marketplace_unverified"),
                StatusTone::Error,
            ))
        } else if update_available {
            Some((self.i18n.t("plugin.update_available"), StatusTone::Success))
        } else if installed {
            Some((self.i18n.t("plugin.installed"), StatusTone::Neutral))
        } else {
            None
        };
        let capabilities = native_plugin_registry_capabilities_label(&self.i18n, entry);
        let expected_id = entry.id.clone();
        let package_for_install = package.clone();
        let homepage = entry
            .homepage
            .as_deref()
            .and_then(native_plugin_safe_https_url);
        let mut actions = Vec::with_capacity(2);
        if let Some(homepage) = homepage {
            actions.push(self.render_native_plugin_manager_button(
                LucideIcon::ExternalLink,
                self.i18n.t("plugin.marketplace_homepage"),
                false,
                move |_event, _window, cx| {
                    cx.open_url(&homepage);
                },
            ));
        }
        actions.push(self.render_native_plugin_manager_button(
            action_icon,
            action_label,
            action_disabled,
            cx.listener(move |this, _event, _window, cx| {
                let Some(package) = package_for_install.clone() else {
                    return;
                };
                this.start_native_plugin_package_install(
                    Some(expected_id.clone()),
                    Zeroizing::new(package.download_url),
                    Some(package.checksum),
                    installed,
                    cx,
                );
            }),
        ));

        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(plugin_manager_theme_border_half(
                theme.border,
                has_background,
            ))
            .bg(plugin_manager_theme_panel_bg(
                theme.bg_panel,
                has_background,
            ))
            .p(px(14.0))
            .child(action_slot_row(
                &self.tokens,
                ActionSlotRowOptions::new().align_start().gap(12.0),
                Some(Self::render_lucide_icon(
                    LucideIcon::Puzzle,
                    18.0,
                    rgb(theme.accent),
                )),
                div()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(entry.name.clone()),
                            )
                            .child(
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .bg(plugin_manager_theme_alpha(
                                        theme.accent,
                                        PLUGIN_MANAGER_TW_ALPHA_10,
                                    ))
                                    .px(px(6.0))
                                    .py(px(2.0))
                                    .text_size(px(PLUGIN_MANAGER_ROW_META_TEXT_SIZE))
                                    .text_color(rgb(theme.accent))
                                    .child(format!("v{}", entry.version)),
                            )
                            .when_some(availability, |header, (label, tone)| {
                                header.child(status_pill(
                                    &self.tokens,
                                    label,
                                    StatusPillOptions::new(tone).compact(),
                                ))
                            }),
                    )
                    .when_some(entry.description.clone(), |body, description| {
                        body.child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .line_height(px(18.0))
                                .text_color(rgb(theme.text_muted))
                                .child(description),
                        )
                    })
                    .when_some(entry.author.clone(), |body, author| {
                        body.child(
                            div()
                                .text_size(px(PLUGIN_MANAGER_HINT_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .child(
                                    self.i18n
                                        .t("plugin.by_author")
                                        .replace("{{author}}", &author),
                                ),
                        )
                    })
                    .when_some(capabilities, |body, capabilities| {
                        body.child(
                            div()
                                .text_size(px(PLUGIN_MANAGER_HINT_TEXT_SIZE))
                                .line_height(px(18.0))
                                .text_color(rgb(theme.text_muted))
                                .child(capabilities),
                        )
                    })
                    .into_any_element(),
                actions,
            ))
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

    fn render_native_plugin_package_manager(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (busy, install_url_empty, pending_plugin_id, available_updates) = {
            let manager = self.plugin_manager_state(cx);
            (
                matches!(
                    manager.operation_status,
                    NativePluginManagerOperationStatus::Busy(_)
                ),
                manager.install_url_draft.trim().is_empty(),
                manager
                    .pending_overwrite
                    .as_ref()
                    .map(|pending| pending.plugin_id.clone()),
                manager.available_updates.clone(),
            )
        };
        self.native_plugin_card_surface(has_background)
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
                    .min_w(px(0.0))
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.render_native_plugin_manager_icon_input(
                        LucideIcon::Download,
                        SettingsInput::NativePluginInstallUrl,
                        self.i18n.t("plugin.url_placeholder"),
                        cx,
                    ))
                    .child(div().ml_auto().flex_none().child(
                        self.render_native_plugin_manager_button(
                            LucideIcon::Download,
                            self.i18n.t("plugin.install"),
                            busy || install_url_empty,
                            cx.listener(|this, _event, _window, cx| {
                                let (download_url, checksum) =
                                    this.update_plugin_manager_state(cx, |manager| {
                                        let download_url = Zeroizing::new(std::mem::take(
                                            &mut manager.install_url_draft,
                                        ));
                                        let checksum = normalized_optional_string(
                                            &manager.install_checksum_draft,
                                        );
                                        manager.install_checksum_draft.clear();
                                        (download_url, checksum)
                                    });
                                this.start_native_plugin_package_install(
                                    None,
                                    download_url,
                                    checksum,
                                    false,
                                    cx,
                                );
                            }),
                        ),
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
            .when_some(pending_plugin_id, |panel, pending_plugin_id| {
                panel.child(
                    div()
                        .w_full()
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgb(theme.warning))
                        .bg(rgb(theme.bg_card))
                        .p(px(10.0))
                        .child(action_slot_row(
                            &self.tokens,
                            ActionSlotRowOptions::new().gap(10.0).trailing_gap(8.0),
                            None,
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .line_height(px(18.0))
                                .text_color(rgb(theme.warning))
                                .child(
                                    self.i18n
                                        .t("plugin.url_conflict_desc")
                                        .replace("{{pluginId}}", &pending_plugin_id),
                                )
                                .into_any_element(),
                            vec![
                                self.render_native_plugin_manager_text_button(
                                    self.i18n.t("common.actions.cancel"),
                                    false,
                                    cx.listener(|this, _event, _window, cx| {
                                        this.update_plugin_manager_state(cx, |manager| {
                                            manager.pending_overwrite = None;
                                            manager.operation_status =
                                                NativePluginManagerOperationStatus::Idle;
                                        });
                                        cx.notify();
                                    }),
                                ),
                                self.render_native_plugin_manager_text_button(
                                    self.i18n.t("plugin.url_conflict_confirm"),
                                    busy,
                                    cx.listener(|this, _event, _window, cx| {
                                        let pending = this
                                            .update_plugin_manager_state(cx, |manager| {
                                                manager.pending_overwrite.take()
                                            });
                                        let Some(pending) = pending else {
                                            return;
                                        };
                                        this.start_native_plugin_package_install(
                                            pending.expected_id,
                                            pending.download_url,
                                            pending.checksum,
                                            true,
                                            cx,
                                        );
                                    }),
                                ),
                            ],
                        )),
                )
            })
            .child(self.render_native_plugin_registry_fetch_row(cx))
            .when(!available_updates.is_empty(), |panel| {
                panel.child(
                    div().w_full().flex().flex_col().gap(px(8.0)).children(
                        available_updates
                            .iter()
                            .map(|entry| self.render_native_plugin_update_row(entry, cx)),
                    ),
                )
            })
            .when_some(
                self.render_native_plugin_manager_status(cx),
                |panel, status| panel.child(status),
            )
            .into_any_element()
    }

    fn render_native_plugin_registry_fetch_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let manager = self.plugin_manager_state(cx);
        let busy = matches!(
            manager.operation_status,
            NativePluginManagerOperationStatus::Busy(_)
        );
        let registry_url_empty = manager.registry_url_draft.trim().is_empty();
        div()
            .w_full()
            .min_w(px(0.0))
            .pt(px(8.0))
            .border_t_1()
            .border_color(plugin_manager_theme_alpha(
                theme.border,
                PLUGIN_MANAGER_TW_ALPHA_40,
            ))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(12.0))
            .child(self.render_native_plugin_manager_icon_input(
                LucideIcon::Search,
                SettingsInput::NativePluginRegistryUrl,
                "https://example.com/registry.json".to_string(),
                cx,
            ))
            .child(
                div()
                    .ml_auto()
                    .flex_none()
                    .child(self.render_native_plugin_manager_button(
                        LucideIcon::RefreshCw,
                        self.i18n.t("plugin.refresh"),
                        busy || registry_url_empty,
                        cx.listener(|this, _event, _window, cx| {
                            this.start_native_plugin_update_check(cx);
                        }),
                    )),
            )
            .into_any_element()
    }

    fn render_native_plugin_action_button(
        &self,
        icon: LucideIcon,
        label: String,
        disabled: bool,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let text_color = theme.text_muted;
        let hover_bg = rgb(theme.bg_panel);
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
            .child(self.render_native_plugin_manager_text_input(input, placeholder, cx))
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
            // The basis creates a wrapping breakpoint while min-width zero
            // still lets the wrapped input fit exceptionally narrow panes.
            .flex_basis(px(PLUGIN_MANAGER_INLINE_INPUT_BASIS))
            .min_w(px(0.0))
            .max_w_full()
            .child(
                div()
                    .absolute()
                    .left(px(12.0))
                    .top(px(10.0))
                    .child(Self::render_lucide_icon(icon, 16.0, rgb(theme.text_muted))),
            )
            .child(self.render_native_plugin_manager_text_input(input, placeholder, cx))
            .into_any_element()
    }

    fn render_native_plugin_manager_text_input(
        &self,
        input: SettingsInput,
        placeholder: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = self.focused_settings_input == Some(input);
        let display_value = if focused {
            self.settings_input_draft.clone()
        } else {
            self.current_settings_input_value(input, cx)
        };
        let target = WorkspaceImeTarget::Settings(input);
        // These fields are not persisted settings, but routing them through the
        // shared settings IME path keeps Plugin Manager text behavior consistent
        // with the other native form fields.
        self.text_input_with_workspace_ime(
            target,
            text_input_with_content_align(
                &self.tokens,
                TextInputView {
                    value: &display_value,
                    placeholder,
                    focused,
                    caret_visible: self.input_caret.visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
                TextInputContentAlign::Start,
            )
            .w_full()
            .min_w(px(0.0)),
            move |this, cx| {
                let current = this.current_settings_input_value(input, cx);
                this.focus_settings_input(input, current, cx);
            },
            cx,
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
            .flex_none()
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
            .whitespace_nowrap()
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
            self.plugin_manager_state(cx).operation_status,
            NativePluginManagerOperationStatus::Busy(_)
        );
        let package = plugin_host::NativePluginRegistry::resolve_registry_package(entry).ok();
        let expected_id = entry.id.clone();
        let package_available = package
            .as_ref()
            .is_some_and(|package| !package.checksum.trim().is_empty());
        let capabilities = native_plugin_registry_capabilities_label(&self.i18n, entry);
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_card))
            .p(px(10.0))
            .child(action_slot_row(
                &self.tokens,
                ActionSlotRowOptions::new().gap(10.0),
                None,
                div()
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
                    })
                    .into_any_element(),
                vec![self.render_native_plugin_manager_button(
                    LucideIcon::Download,
                    self.i18n.t("plugin.update"),
                    busy || !package_available,
                    cx.listener(move |this, _event, _window, cx| {
                        let Some(package) = package.clone() else {
                            return;
                        };
                        // Registry entries remain visible after a click; move
                        // only the resolved public package into the worker boundary.
                        this.start_native_plugin_package_install(
                            Some(expected_id.clone()),
                            Zeroizing::new(package.download_url),
                            Some(package.checksum),
                            false,
                            cx,
                        );
                    }),
                )],
            ))
            .into_any_element()
    }

    fn render_native_plugin_manager_status(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let theme = self.tokens.ui;
        let (icon, color, message) = match &self.plugin_manager_state(cx).operation_status {
            // The dedicated disclaimer card below already owns the idle-state guidance.
            NativePluginManagerOperationStatus::Idle => return None,
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
        Some(
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
                .into_any_element(),
        )
    }

    fn start_native_plugin_package_install(
        &mut self,
        expected_id: Option<String>,
        download_url: Zeroizing<String>,
        checksum: Option<String>,
        overwrite: bool,
        cx: &mut Context<Self>,
    ) {
        if download_url.trim().is_empty() {
            let message = self.i18n.t("plugin.url_invalid");
            self.update_plugin_manager_state(cx, |manager| {
                manager.operation_status = NativePluginManagerOperationStatus::Error(message);
            });
            cx.notify();
            return;
        }
        if self.plugin_entity.read(cx).manager_operation_in_flight() {
            let message = self.i18n.t("plugin.installing");
            self.update_plugin_manager_state(cx, |manager| {
                manager.operation_status = NativePluginManagerOperationStatus::Busy(message);
            });
            cx.notify();
            return;
        }

        let settings_path = self.settings_store.path().to_path_buf();
        let message = self.i18n.t("plugin.installing");
        self.update_plugin_manager_state(cx, |manager| {
            manager.operation_status = NativePluginManagerOperationStatus::Busy(message);
            if overwrite {
                manager.pending_overwrite = None;
            }
        });
        let started = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.start_package_install(
                settings_path,
                expected_id,
                download_url,
                checksum,
                overwrite,
            )
        });
        debug_assert!(started, "manager operation gate changed before start");
    }

    fn start_native_plugin_update_check(&mut self, cx: &mut Context<Self>) {
        let registry_url = self.update_plugin_manager_state(cx, |manager| {
            Zeroizing::new(std::mem::take(&mut manager.registry_url_draft))
        });
        if registry_url.trim().is_empty() {
            let message = self.i18n.t("plugin.registry_error");
            self.update_plugin_manager_state(cx, |manager| {
                manager.operation_status = NativePluginManagerOperationStatus::Error(message);
            });
            cx.notify();
            return;
        }
        if self.plugin_entity.read(cx).manager_operation_in_flight() {
            let message = self.i18n.t("plugin.loading_registry");
            self.update_plugin_manager_state(cx, |manager| {
                manager.operation_status = NativePluginManagerOperationStatus::Busy(message);
            });
            cx.notify();
            return;
        }

        let installed = self
            .plugin_entity
            .read(cx)
            .registry()
            .plugins()
            .iter()
            .map(|plugin| plugin_host::NativePluginInstalledInfo {
                id: plugin.manifest.id.clone(),
                version: plugin.manifest.version.clone(),
            })
            .collect::<Vec<_>>();
        let message = self.i18n.t("plugin.loading_registry");
        self.update_plugin_manager_state(cx, |manager| {
            manager.operation_status = NativePluginManagerOperationStatus::Busy(message);
        });
        let started = self.plugin_entity.update(cx, |plugins, _cx| {
            plugins.start_update_check(registry_url, installed)
        });
        debug_assert!(started, "manager operation gate changed before start");
    }

    fn start_native_plugin_marketplace_load(&mut self, cx: &mut Context<Self>) {
        if self.plugin_entity.read(cx).manager_operation_in_flight() {
            return;
        }
        let message = self.i18n.t("plugin.loading_marketplace");
        self.update_plugin_manager_state(cx, |manager| {
            manager.marketplace_load_state = NativePluginMarketplaceLoadState::Loading;
            manager.operation_status = NativePluginManagerOperationStatus::Busy(message);
        });
        let started = self
            .plugin_entity
            .update(cx, |plugins, _cx| plugins.start_marketplace_load());
        debug_assert!(started, "manager operation gate changed before start");
        cx.notify();
    }

    pub(in crate::workspace) fn handle_plugin_workspace_event(
        &mut self,
        event: &plugin_entity::PluginWorkspaceEvent,
        window_handle: AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        match event {
            plugin_entity::PluginWorkspaceEvent::ManagerDeliveryReady => {
                let settings_path = self.settings_store.path();
                let i18n = &self.i18n;
                let bootstrap_runtime = self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.apply_manager_deliveries(settings_path, i18n)
                });
                if bootstrap_runtime {
                    self.bootstrap_native_plugin_runtime(cx);
                }
                cx.notify();
            }
            plugin_entity::PluginWorkspaceEvent::RuntimeRequestsReady => {
                self.schedule_native_plugin_runtime_request_apply(window_handle, cx);
            }
            plugin_entity::PluginWorkspaceEvent::RuntimeSubscriptionSampleDue => {
                self.sample_native_plugin_subscriptions(cx);
            }
            plugin_entity::PluginWorkspaceEvent::RuntimeIntentsReady => {
                self.apply_native_plugin_runtime_intents(cx);
            }
            plugin_entity::PluginWorkspaceEvent::OxideImportIntentsReady => {
                self.apply_native_plugin_oxide_import_intents(cx);
            }
        }
    }

    fn render_native_plugin_diagnostic_row(
        &self,
        diagnostic: &plugin_host::NativePluginDiagnostic,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let diagnostic_key = NativePluginDiagnosticKey::from(diagnostic);
        let title = diagnostic
            .plugin_id
            .clone()
            .unwrap_or_else(|| diagnostic.plugin_dir.display().to_string());
        let message = native_plugin_diagnostic_message(&self.i18n, &diagnostic.message);
        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgb(theme.error))
            .bg(rgb(theme.bg_panel))
            .p(px(14.0))
            .child(action_slot_row(
                &self.tokens,
                ActionSlotRowOptions::new().align_start().gap(10.0),
                Some(Self::render_lucide_icon(
                    LucideIcon::AlertTriangle,
                    16.0,
                    rgb(theme.error),
                )),
                div()
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
                            .child(message),
                    )
                    .into_any_element(),
                vec![self.render_native_plugin_row_icon_button(
                    LucideIcon::X,
                    theme.text_muted,
                    Some(cx.listener(move |this, _event, _window, cx| {
                        this.update_plugin_manager_state(cx, |manager| {
                            manager
                                .dismissed_diagnostic_keys
                                .insert(diagnostic_key.clone());
                            // Re-measure only the installed/browse content row after its alert count changes.
                            manager.section_list_state.splice(
                                PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX
                                    ..PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX + 1,
                                1,
                            );
                        });
                        cx.stop_propagation();
                        cx.notify();
                    })),
                )],
            ))
            .into_any_element()
    }

    fn render_native_plugin_registry_row(
        &self,
        plugin: &plugin_host::NativePluginInfo,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (state_label, state_tone) = native_plugin_status_badge(&self.i18n, plugin);
        let error_message = native_plugin_visible_error(&self.i18n, plugin);
        let is_expanded = self
            .plugin_manager_state(cx)
            .expanded_plugin_ids
            .contains(&plugin.manifest.id);
        let is_active = plugin_host::native_plugin_state_is_active_like(plugin.state);
        let is_disabled = plugin.state == plugin_host::NativePluginState::Disabled;
        let is_error = plugin_host::native_plugin_state_is_error_like(plugin.state);
        let next_enabled = if !is_active && !is_disabled {
            false
        } else {
            is_disabled
        };
        let toggle_color = if next_enabled {
            theme.text_muted
        } else if is_active {
            theme.success
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
                                        this.update_plugin_manager_state(cx, |manager| {
                                            if !manager
                                                .expanded_plugin_ids
                                                .insert(expand_plugin_id.clone())
                                            {
                                                manager
                                                    .expanded_plugin_ids
                                                    .remove(&expand_plugin_id);
                                            }
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                )
                                .child(self.render_animated_chevron(
                                    (
                                        gpui::SharedString::from(format!(
                                            "native-plugin-chevron-{}",
                                            plugin.manifest.id
                                        )),
                                        is_expanded as usize,
                                    ),
                                    is_expanded,
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
                                        .child(status_pill(
                                            &self.tokens,
                                            state_label,
                                            StatusPillOptions::new(state_tone).compact(),
                                        )),
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
                                    let registry = plugin_host::NativePluginRegistry::discover(
                                        this.settings_store.path(),
                                    );
                                    this.plugin_entity.update(cx, |plugins, _cx| {
                                        plugins.replace_registry(registry);
                                    });
                                    this.bootstrap_native_plugin_runtime(cx);
                                    let success_template = this.i18n.t("plugin.reload_success");
                                    this.update_plugin_manager_state(cx, |manager| {
                                        manager.operation_status =
                                            NativePluginManagerOperationStatus::Success(
                                                success_template
                                                    .replace("{{name}}", &reload_plugin_id),
                                            );
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                })),
                            ))
                        })
                        .child(self.render_native_plugin_row_icon_button(
                            LucideIcon::Power,
                            toggle_color,
                            Some(cx.listener(move |this, _event, _window, cx| {
                                let result = this.plugin_entity.update(cx, |plugins, _cx| {
                                    plugins.set_plugin_enabled(&plugin_id, next_enabled)
                                });
                                if let Err(error) = result {
                                    this.update_plugin_manager_state(cx, |manager| {
                                        manager.operation_status =
                                            NativePluginManagerOperationStatus::Error(
                                                error.clone(),
                                            );
                                    });
                                    this.plugin_entity.update(cx, |plugins, _cx| {
                                        plugins
                                            .registry_mut()
                                            .record_manager_error(plugin_id.clone(), error);
                                    });
                                } else {
                                    if next_enabled {
                                        this.bootstrap_native_plugin_runtime(cx);
                                    }
                                    let success_key = if next_enabled {
                                        "plugin.enable_success"
                                    } else {
                                        "plugin.disable_success"
                                    };
                                    let message =
                                        this.i18n.t(success_key).replace("{{name}}", &plugin_id);
                                    this.update_plugin_manager_state(cx, |manager| {
                                        manager.operation_status =
                                            NativePluginManagerOperationStatus::Success(message);
                                    });
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
                                let result = this.plugin_entity.update(cx, |plugins, _cx| {
                                    plugins.uninstall_plugin(&uninstall_plugin_id, false)
                                });
                                if let Err(error) = result {
                                    this.plugin_entity.update(cx, |plugins, _cx| {
                                        plugins.registry_mut().record_manager_error(
                                            uninstall_plugin_id.clone(),
                                            error,
                                        );
                                    });
                                }
                                cx.stop_propagation();
                                cx.notify();
                            })),
                        )),
                ),
        );
        if let Some(error_message) = error_message {
            let copy_error_message = error_message.clone();
            row = row.child(
                div()
                    .ml(px(28.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(plugin_manager_palette_alpha(
                        theme.error,
                        PLUGIN_MANAGER_TW_ALPHA_20,
                    ))
                    .bg(plugin_manager_palette_alpha(
                        theme.error,
                        PLUGIN_MANAGER_TW_ALPHA_10,
                    ))
                    .px(px(12.0))
                    .py(px(10.0))
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(theme.error))
                    .child(Self::render_lucide_icon(
                        LucideIcon::AlertTriangle,
                        14.0,
                        rgb(theme.error),
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .whitespace_normal()
                            .child(error_message),
                    )
                    .child(self.render_native_plugin_row_icon_button(
                        LucideIcon::Copy,
                        theme.error,
                        Some(cx.listener(move |this, _event, _window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                copy_error_message.clone(),
                            ));
                            let message = this.i18n.t("plugin.error_copied");
                            this.update_plugin_manager_state(cx, |manager| {
                                manager.operation_status =
                                    NativePluginManagerOperationStatus::Success(message);
                            });
                            cx.stop_propagation();
                            cx.notify();
                        })),
                    )),
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
        let NativePluginPermissionDetails {
            capabilities: permission_capabilities,
            requires_review: permission_requires_review,
        } = native_plugin_permission_details(plugin);
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
                                    .rounded(px(self.tokens.radii.sm))
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
            .child(
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
                            .child(self.i18n.t("plugin.detail_permissions")),
                    )
                    .when(permission_capabilities.is_empty(), |permissions| {
                        permissions.child(self.i18n.t("plugin.permission_none"))
                    })
                    .when(!permission_capabilities.is_empty(), |permissions| {
                        permissions.child(div().flex().flex_wrap().gap(px(6.0)).children(
                            permission_capabilities.into_iter().map(|capability| {
                                let (label, is_trusted_process) = if capability
                                    == plugin_host::NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY
                                {
                                    (self.i18n.t("plugin.permission_trusted_process"), true)
                                } else {
                                    (capability, false)
                                };
                                div()
                                    .rounded(px(self.tokens.radii.sm))
                                    .bg(plugin_manager_theme_alpha(
                                        if is_trusted_process {
                                            theme.warning
                                        } else {
                                            theme.accent
                                        },
                                        PLUGIN_MANAGER_TW_ALPHA_10,
                                    ))
                                    .px(px(8.0))
                                    .py(px(2.0))
                                    .text_size(px(PLUGIN_MANAGER_ROW_META_TEXT_SIZE))
                                    .text_color(rgb(if is_trusted_process {
                                        theme.warning
                                    } else {
                                        theme.accent
                                    }))
                                    .child(label)
                            }),
                        ))
                    })
                    .when(permission_requires_review, |permissions| {
                        permissions.child(
                            div()
                                .mt(px(2.0))
                                .rounded(px(self.tokens.radii.sm))
                                .border_1()
                                .border_color(plugin_manager_theme_alpha(
                                    theme.warning,
                                    PLUGIN_MANAGER_TW_ALPHA_30,
                                ))
                                .bg(plugin_manager_theme_alpha(
                                    theme.warning,
                                    PLUGIN_MANAGER_TW_ALPHA_10,
                                ))
                                .px(px(10.0))
                                .py(px(8.0))
                                .flex()
                                .items_start()
                                .gap(px(8.0))
                                .text_color(rgb(theme.warning))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::AlertTriangle,
                                    14.0,
                                    rgb(theme.warning),
                                ))
                                .child(
                                    div()
                                        .min_w(px(0.0))
                                        .flex_1()
                                        .whitespace_normal()
                                        .child(self.i18n.t("plugin.permission_review_warning")),
                                ),
                        )
                    }),
            )
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

fn native_plugin_marketplace_entry_matches(
    entry: &plugin_host::NativePluginRegistryEntry,
    query: &str,
) -> bool {
    query.is_empty()
        || entry.id.to_lowercase().contains(query)
        || entry.name.to_lowercase().contains(query)
        || entry
            .description
            .as_deref()
            .is_some_and(|description| description.to_lowercase().contains(query))
        || entry
            .author
            .as_deref()
            .is_some_and(|author| author.to_lowercase().contains(query))
        || entry
            .tags
            .as_ref()
            .is_some_and(|tags| tags.iter().any(|tag| tag.to_lowercase().contains(query)))
}

fn native_plugin_safe_https_url(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    (parsed.scheme() == "https" && parsed.username().is_empty() && parsed.password().is_none())
        .then(|| url.to_string())
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

#[derive(Debug, PartialEq, Eq)]
struct NativePluginPermissionDetails {
    capabilities: Vec<String>,
    requires_review: bool,
}

fn native_plugin_permission_details(
    plugin: &plugin_host::NativePluginInfo,
) -> NativePluginPermissionDetails {
    // Discovery validates permission declarations, so an error here reflects an
    // already surfaced invalid manifest and must not invent a partial grant list.
    let capabilities =
        plugin_host::native_plugin_requested_capabilities(&plugin.manifest, &plugin.runtime_plan)
            .unwrap_or_default();
    let requires_review = plugin_host::native_plugin_requires_permission_review(
        &plugin.manifest,
        &plugin.runtime_plan,
        &plugin.config,
    );
    NativePluginPermissionDetails {
        capabilities,
        requires_review,
    }
}

fn native_plugin_status_badge(
    i18n: &I18n,
    plugin: &plugin_host::NativePluginInfo,
) -> (String, StatusTone) {
    match plugin.state {
        plugin_host::NativePluginState::Active
        | plugin_host::NativePluginState::ReadyManifestOnly
        | plugin_host::NativePluginState::ReadyWasm
        | plugin_host::NativePluginState::ReadyProcess => {
            (i18n.t("plugin.status.active"), StatusTone::Success)
        }
        plugin_host::NativePluginState::Loading => {
            (i18n.t("plugin.status.loading"), StatusTone::Warning)
        }
        plugin_host::NativePluginState::Error | plugin_host::NativePluginState::AutoDisabled => {
            (i18n.t("plugin.status.error"), StatusTone::Error)
        }
        plugin_host::NativePluginState::Disabled => {
            (i18n.t("plugin.status.disabled"), StatusTone::Warning)
        }
        plugin_host::NativePluginState::UnsupportedLegacyJs => {
            (i18n.t("plugin.status.inactive"), StatusTone::Warning)
        }
        plugin_host::NativePluginState::Discovered => {
            (i18n.t("plugin.status.inactive"), StatusTone::Neutral)
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
    let Some(error) = plugin.config.last_error.as_deref() else {
        return Some(i18n.t("plugin.load_failed_default"));
    };
    if plugin_host::native_plugin_error_has_code(
        error,
        plugin_runtime::WASM_RUNTIME_UNAVAILABLE_CODE,
    ) {
        return Some(i18n.t("plugin.wasm_runtime_unavailable"));
    }
    if error == plugin_entity::NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC {
        return Some(i18n.t("plugin.runtime_failure"));
    }
    Some(error.to_string())
}

fn native_plugin_diagnostic_message(i18n: &I18n, message: &str) -> String {
    if plugin_host::native_plugin_error_has_code(
        message,
        plugin_runtime::WASM_RUNTIME_UNAVAILABLE_CODE,
    ) {
        return i18n.t("plugin.wasm_runtime_unavailable");
    }
    if message == plugin_entity::NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC {
        return i18n.t("plugin.runtime_failure");
    }
    message.to_string()
}

fn native_plugin_diagnostic_is_visible(
    diagnostic: &plugin_host::NativePluginDiagnostic,
    dismissed: &HashSet<NativePluginDiagnosticKey>,
) -> bool {
    !dismissed.contains(&NativePluginDiagnosticKey::from(diagnostic))
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
        let mut command = Command::new("explorer");
        configure_plugin_manager_external_bridge(&mut command);
        command.arg(&plugins_dir).status()
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

#[cfg(target_os = "windows")]
fn configure_plugin_manager_external_bridge(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    // Explorer is the visible target; hide the short-lived launcher process so
    // opening the plugins directory does not flash a console.
    command.creation_flags(PLUGIN_MANAGER_EXTERNAL_BRIDGE_CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
fn configure_plugin_manager_external_bridge(command: &mut Command) {
    let _ = command;
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

    fn plugin_with_permissions(
        runtime_plan: plugin_host::NativePluginRuntimePlan,
        capabilities: &[&str],
        config: plugin_host::NativePluginConfigEntry,
    ) -> plugin_host::NativePluginInfo {
        // Keep permission presentation tests independent from filesystem discovery.
        let manifest: plugin_host::NativePluginManifest =
            serde_json::from_value(serde_json::json!({
                "id": "com.example.permissions",
                "name": "Permissions",
                "version": "1.0.0",
                "permissions": { "capabilities": capabilities }
            }))
            .expect("test manifest should deserialize");
        plugin_host::NativePluginInfo {
            manifest,
            install_dir: PathBuf::from("plugins/permissions"),
            runtime_plan,
            state: plugin_host::NativePluginState::Disabled,
            config,
        }
    }

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
            packages: Vec::new(),
        }
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

    #[test]
    fn runtime_failure_diagnostics_use_localized_copy() {
        let i18n = I18n::new(Locale::En);

        assert_eq!(
            native_plugin_diagnostic_message(
                &i18n,
                plugin_entity::NATIVE_PLUGIN_RUNTIME_FAILURE_DIAGNOSTIC,
            ),
            "Plugin runtime operation failed."
        );
    }

    #[test]
    fn pending_overwrite_debug_redacts_package_url() {
        let pending = NativePluginPendingOverwrite {
            plugin_id: "com.example.demo".to_string(),
            expected_id: None,
            download_url: Zeroizing::new(
                "https://token@example.invalid/demo.zip?auth=secret".to_string(),
            ),
            checksum: Some("sha256:abc".to_string()),
        };

        let debug = format!("{pending:?}");
        assert!(debug.contains("com.example.demo"));
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("token"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn permission_details_show_declared_capabilities_until_approved() {
        let plugin = plugin_with_permissions(
            plugin_host::NativePluginRuntimePlan::Wasm {
                entry: "plugin.wasm".to_string(),
            },
            &["terminal.content.read", "terminal.input.write"],
            plugin_host::NativePluginConfigEntry::default(),
        );

        let details = native_plugin_permission_details(&plugin);
        assert_eq!(
            details.capabilities,
            vec![
                "terminal.content.read".to_string(),
                "terminal.input.write".to_string()
            ]
        );
        assert!(details.requires_review);
    }

    #[test]
    fn permission_details_mark_process_plugins_as_trusted_native_code() {
        let plugin = plugin_with_permissions(
            plugin_host::NativePluginRuntimePlan::Process {
                entry: "plugin-bin".to_string(),
            },
            &[],
            plugin_host::NativePluginConfigEntry::default(),
        );

        let details = native_plugin_permission_details(&plugin);
        assert_eq!(
            details.capabilities,
            vec![plugin_host::NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY.to_string()]
        );
        assert!(details.requires_review);
    }

    #[test]
    fn permission_details_hide_review_warning_after_matching_approval() {
        let config = plugin_host::NativePluginConfigEntry {
            approved_capabilities: vec!["terminal.content.read".to_string()],
            approved_for_version: Some("1.0.0".to_string()),
            approved_runtime_kind: Some("wasm".to_string()),
            ..plugin_host::NativePluginConfigEntry::default()
        };
        let plugin = plugin_with_permissions(
            plugin_host::NativePluginRuntimePlan::Wasm {
                entry: "plugin.wasm".to_string(),
            },
            &["terminal.content.read"],
            config,
        );

        assert!(!native_plugin_permission_details(&plugin).requires_review);
    }

    #[test]
    fn dismissing_plugin_diagnostic_hides_only_the_exact_warning() {
        let warning = plugin_host::NativePluginDiagnostic {
            plugin_dir: PathBuf::from("plugins/demo"),
            plugin_id: Some("com.example.demo".to_string()),
            message: "legacy runtime".to_string(),
        };
        let mut dismissed = HashSet::new();

        assert!(native_plugin_diagnostic_is_visible(&warning, &dismissed));
        dismissed.insert(NativePluginDiagnosticKey::from(&warning));
        assert!(!native_plugin_diagnostic_is_visible(&warning, &dismissed));

        let replacement = plugin_host::NativePluginDiagnostic {
            message: "invalid manifest".to_string(),
            ..warning
        };
        assert!(native_plugin_diagnostic_is_visible(
            &replacement,
            &dismissed
        ));
    }
}
