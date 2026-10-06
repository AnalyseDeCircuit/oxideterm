use super::*;
use gpui::Div;
use oxideterm_gpui_ui::{
    ActionSlotRowOptions, StatusTone, SurfaceKind, SurfaceOptions, SurfacePadding, action_slot_row,
    semantic_surface,
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
// Keep row actions aligned with the installed-plugin list.
const PLUGIN_MANAGER_ACTION_ICON_SIZE: f32 = 14.0;
const PLUGIN_MANAGER_ROW_ACTION_SIZE: f32 = 28.0;
const PLUGIN_MANAGER_INLINE_INPUT_BASIS: f32 = 280.0;
const PLUGIN_MANAGER_TAB_BAR_WIDTH: f32 = 300.0; // Two equal header tabs preserve room for localized labels and badges.
const DEFAULT_PLUGIN_PAGE_SIZE: usize = 10;
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
    CatalogHistories {
        expected: Vec<plugin_host::NativePluginRegistryEntry>,
        results: Vec<(
            String,
            Result<plugin_host::NativePluginRegistryEntry, String>,
        )>,
    },
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
    pagination: [PluginPagination; 2],
    pub(super) install_url_draft: String,
    pub(super) install_checksum_draft: String,
    pub(super) registry_url_draft: String,
    pub(super) marketplace_search_draft: String,
    marketplace_tag: Option<String>,
    installed_tag: Option<String>,
    marketplace_updates_only: bool,
    marketplace_expanded_ids: HashSet<String>,
    package_manager_expanded: bool,
    pub(super) custom_acp_expanded: bool,
    pub(super) marketplace_entries: Vec<plugin_host::NativePluginRegistryEntry>,
    pub(super) catalog_version: u32,
    pub(super) history_page_ids: Vec<String>,
    pub(super) pending_histories: HashSet<String>,
    pub(super) failed_histories: HashSet<String>,
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
            pagination: [PluginPagination::default(); 2],
            install_url_draft: String::new(),
            install_checksum_draft: String::new(),
            registry_url_draft: String::new(),
            marketplace_search_draft: String::new(),
            marketplace_tag: None,
            installed_tag: None,
            marketplace_updates_only: false,
            marketplace_expanded_ids: HashSet::new(),
            package_manager_expanded: false,
            custom_acp_expanded: false,
            marketplace_entries: Vec::new(),
            catalog_version: 1,
            history_page_ids: Vec::new(),
            pending_histories: HashSet::new(),
            failed_histories: HashSet::new(),
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

#[derive(Clone, Copy)]
struct PluginPagination {
    page: usize,
    page_size: usize,
    total: usize,
}

impl Default for PluginPagination {
    fn default() -> Self {
        Self {
            page: 0,
            page_size: DEFAULT_PLUGIN_PAGE_SIZE,
            total: 0,
        }
    }
}

impl PluginPagination {
    fn range(&mut self, total: usize) -> std::ops::Range<usize> {
        self.total = total;
        self.page = self.page.min(total.saturating_sub(1) / self.page_size);
        let start = self.page * self.page_size;
        start..start.saturating_add(self.page_size).min(total)
    }

    fn set_page_size(&mut self, page_size: usize) {
        // Keep the first visible plugin on screen when changing the page size.
        self.page = self.page * self.page_size / page_size;
        self.page_size = page_size;
    }

    fn page_count(&self) -> usize {
        self.total.div_ceil(self.page_size).max(1)
    }

    fn jump_target(&self, value: &str) -> Option<usize> {
        parse_plugin_page_size(value)
            .filter(|page| *page <= self.page_count())
            .map(|page| page - 1)
    }
}

fn parse_plugin_page_size(value: &str) -> Option<usize> {
    value.trim().parse::<usize>().ok().filter(|size| *size > 0)
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
        self.plugin_entity.update(cx, |plugins, _cx| {
            let manager = plugins.manager_state_mut();
            let filters = (
                manager.marketplace_search_draft.clone(),
                manager.marketplace_tag.clone(),
                manager.marketplace_updates_only,
            );
            let result = update(manager);
            if filters
                != (
                    manager.marketplace_search_draft.clone(),
                    manager.marketplace_tag.clone(),
                    manager.marketplace_updates_only,
                )
            {
                manager.pagination[1].page = 0;
            }
            result
        })
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

    pub(super) fn open_language_plugin(
        &mut self,
        language: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.file_manager.read(cx).dialog,
            Some(super::file_manager::FileManagerDialog::Preview { .. })
        ) {
            self.close_file_manager_dialog(cx);
        }
        let installed = self
            .plugin_entity
            .read(cx)
            .registry()
            .plugins()
            .iter()
            .position(|plugin| {
                plugin
                    .manifest
                    .contributes
                    .as_ref()
                    .and_then(|value| value.language.as_ref())
                    .is_some_and(|value| value.id == language)
            });
        self.open_plugin_manager_tab(window, cx);
        self.update_plugin_manager_state(cx, |manager| {
            manager.previous_tab = manager.active_tab;
            manager.active_tab = if installed.is_some() {
                NativePluginManagerTab::Installed
            } else {
                NativePluginManagerTab::Marketplace
            };
            manager.marketplace_search_draft = format!("com.oxideterm.language.{language}");
            manager.marketplace_tag = None;
            manager.marketplace_updates_only = false;
            if let Some(index) = installed {
                manager.installed_tag = None;
                manager.pagination[0].page = index / manager.pagination[0].page_size;
            }
            manager.section_list_state.splice(
                PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX
                    ..PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX + 1,
                1,
            );
        });
        if installed.is_none() {
            self.start_native_plugin_marketplace_load(cx);
        }
        cx.notify();
    }

    pub(super) fn open_remote_desktop_plugin(
        &mut self,
        protocol: oxideterm_remote_desktop::RemoteDesktopProtocol,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let protocol = match protocol {
            oxideterm_remote_desktop::RemoteDesktopProtocol::Rdp => "rdp",
            oxideterm_remote_desktop::RemoteDesktopProtocol::Vnc => "vnc",
        };
        self.open_plugin_manager_tab(window, cx);
        self.update_plugin_manager_state(cx, |manager| {
            manager.previous_tab = manager.active_tab;
            manager.active_tab = NativePluginManagerTab::Marketplace;
            manager.marketplace_search_draft = format!("com.oxideterm.remote-desktop.{protocol}");
            manager.marketplace_tag = Some("remote-connections".to_string());
            manager.marketplace_updates_only = false;
            manager.pagination[1].page = 0;
            manager.section_list_state.splice(
                PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX
                    ..PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX + 1,
                1,
            );
        });
        self.start_native_plugin_marketplace_load(cx);
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
            0 => oxideterm_gpui_ui::page_header(
                &self.tokens,
                self.i18n.t("plugin.manager_title"),
                Some(self.i18n.t(match self.plugin_manager_state(cx).active_tab {
                    NativePluginManagerTab::Installed => "plugin.manager_description",
                    NativePluginManagerTab::Marketplace => "plugin.marketplace_description",
                })),
                Some(self.render_native_plugin_tab_bar(has_background, cx)),
            )
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
        _has_background: bool,
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
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(
                div()
                    .flex()
                    .flex_wrap()
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
                            .when(
                                self.plugin_manager_state(cx).active_tab
                                    == NativePluginManagerTab::Installed,
                                |actions| {
                                    actions.child(self.render_native_plugin_action_button(
                                        LucideIcon::RefreshCw,
                                        self.i18n.t("plugin.refresh"),
                                        false,
                                        cx.listener(|this, _event, _window, cx| {
                                            let registry =
                                                plugin_host::NativePluginRegistry::discover(
                                                    this.settings_store.path(),
                                                );
                                            this.plugin_entity.update(cx, |plugins, _cx| {
                                                plugins.replace_registry(registry, _cx);
                                            });
                                            let refreshed = this.i18n.t("plugin.refresh");
                                            this.update_plugin_manager_state(cx, |manager| {
                                                manager.operation_status =
                                                    NativePluginManagerOperationStatus::Success(
                                                        refreshed,
                                                    );
                                            });
                                            cx.notify();
                                        }),
                                    ))
                                },
                            )
                            .when(
                                self.plugin_manager_state(cx).active_tab
                                    == NativePluginManagerTab::Marketplace,
                                |actions| {
                                    actions
                                        .child(self.render_native_plugin_action_button(
                                            LucideIcon::ExternalLink,
                                            self.i18n.t("plugin.marketplace_repository"),
                                            false,
                                            |_event, _window, cx| {
                                                cx.open_url(OFFICIAL_PLUGIN_MARKETPLACE_HOME);
                                            },
                                        ))
                                        .child(
                                            self.render_native_plugin_action_button(
                                                LucideIcon::RefreshCw,
                                                self.i18n.t("plugin.refresh"),
                                                self.plugin_entity
                                                    .read(cx)
                                                    .manager_operation_in_flight(),
                                                cx.listener(|this, _event, _window, cx| {
                                                    this.start_native_plugin_marketplace_load(cx);
                                                    cx.stop_propagation();
                                                }),
                                            ),
                                        )
                                },
                            ),
                    ),
            )
            .when_some(
                self.render_native_plugin_manager_status(cx),
                |panel, status| panel.child(status),
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
                registry
                    .plugins()
                    .iter()
                    .filter(|plugin| {
                        native_plugin_tag_matches(
                            native_plugin_tags_for_id(
                                &plugin.manifest.id,
                                &plugins.manager_state().marketplace_entries,
                                registry,
                            ),
                            plugins.manager_state().installed_tag.as_deref(),
                        )
                    })
                    .cloned()
                    .collect::<Vec<_>>(),
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
        let installed_filter_active = self.plugin_manager_state(cx).installed_tag.is_some();
        let card = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .min_h(px(260.0))
            .child(self.render_plugin_tag_filter(NativePluginManagerTab::Installed, cx))
            .when(
                self.plugin_manager_state(cx).installed_tag.as_deref() == Some("acp"),
                |card| card.child(self.ai_acp_agents_section(self.settings_store.settings(), cx)),
            );
        let range = self.update_plugin_manager_state(cx, |manager| {
            manager.pagination[0].range(plugin_rows.len())
        });

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
                                .child(self.i18n.t(if installed_filter_active {
                                    "plugin.no_search_results"
                                } else {
                                    "plugin.empty_title"
                                })),
                        )
                        .child(
                            div()
                                .max_w(px(560.0))
                                .text_center()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .line_height(px(20.0))
                                .text_color(rgb(theme.text_muted))
                                .child(if installed_filter_active {
                                    String::new()
                                } else {
                                    self.i18n.t("plugin.empty_description")
                                }),
                        ),
                )
                .into_any_element();
        }

        card.children(
            diagnostics
                .iter()
                .map(|diagnostic| self.render_native_plugin_diagnostic_row(diagnostic, cx)),
        )
        .child(
            div()
                .w_full()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .child(self.render_plugin_pagination(
                    NativePluginManagerTab::Installed,
                    plugin_rows.len(),
                    false,
                    cx,
                ))
                .children(plugin_rows[range].iter().map(|plugin| {
                    self.render_native_plugin_registry_row(plugin, has_background, cx)
                }))
                .child(self.render_plugin_pagination(
                    NativePluginManagerTab::Installed,
                    plugin_rows.len(),
                    true,
                    cx,
                )),
        )
        .into_any_element()
    }

    fn render_native_plugin_marketplace_content(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let expanded = self.plugin_manager_state(cx).package_manager_expanded
            || self.plugin_manager_state(cx).pending_overwrite.is_some();
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.render_native_plugin_marketplace(has_background, cx))
            .child(self.render_native_plugin_action_button(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                self.i18n.t("plugin.url_install_title"),
                false,
                cx.listener(|this, _event, _window, cx| {
                    this.update_plugin_manager_state(cx, |manager| {
                        manager.package_manager_expanded = !manager.package_manager_expanded;
                    });
                    cx.notify();
                }),
            ))
            .when(expanded, |content| {
                content
                    .child(self.render_native_plugin_package_manager(has_background, cx))
                    .child(self.render_native_plugin_url_disclaimer())
            })
            .into_any_element()
    }

    fn render_native_plugin_marketplace(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (entries, query, tag, updates_only, load_state) = {
            let manager = self.plugin_manager_state(cx);
            (
                manager.marketplace_entries.clone(),
                manager.marketplace_search_draft.clone(),
                manager.marketplace_tag.clone(),
                manager.marketplace_updates_only,
                manager.marketplace_load_state,
            )
        };
        let query = query.trim().to_lowercase();
        let plugins = self.plugin_entity.read(cx).registry().plugins();
        let visible_entries = entries
            .into_iter()
            .filter(|entry| {
                let installed_version = plugins
                    .iter()
                    .find(|plugin| plugin.manifest.id == entry.id)
                    .map(|plugin| plugin.manifest.version.as_str());
                native_plugin_marketplace_entry_visible(
                    entry,
                    &query,
                    tag.as_deref(),
                    updates_only,
                    installed_version,
                )
            })
            .collect::<Vec<_>>();
        let entry_count = visible_entries.len();
        let range = self
            .update_plugin_manager_state(cx, |manager| manager.pagination[1].range(entry_count));
        let requested = visible_entries[range.clone()]
            .iter()
            .map(|entry| entry.id.clone())
            .collect();
        self.plugin_entity
            .update(cx, |entity, _| entity.start_catalog_history_load(requested));

        let mut card = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                self.render_native_plugin_manager_icon_input(
                    LucideIcon::Search,
                    SettingsInput::NativePluginMarketplaceSearch,
                    self.i18n.t("plugin.search_placeholder"),
                    cx,
                )
                .w_full()
                .flex_none(),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .child(self.render_plugin_tag_filter(NativePluginManagerTab::Marketplace, cx))
                    .child(self.render_native_plugin_marketplace_filter(
                        self.i18n.t("plugin.marketplace_updates_only"),
                        updates_only,
                        cx.listener(|this, _event, _window, cx| {
                            this.update_plugin_manager_state(cx, |manager| {
                                manager.marketplace_updates_only = !manager.marketplace_updates_only
                            });
                            cx.notify();
                        }),
                    )),
            );

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
                NativePluginMarketplaceLoadState::Loaded
                    if query.is_empty() && tag.is_none() && !updates_only =>
                {
                    (
                        LucideIcon::Puzzle,
                        self.i18n.t("plugin.marketplace_empty"),
                        theme.text_muted,
                    )
                }
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
            .child(self.render_plugin_pagination(
                NativePluginManagerTab::Marketplace,
                entry_count,
                false,
                cx,
            ))
            .child(div().w_full().min_w(px(0.0)).flex().flex_col().children(
                visible_entries[range].iter().map(|entry| {
                    self.render_native_plugin_marketplace_row(entry, has_background, cx)
                }),
            ))
            .child(self.render_plugin_pagination(
                NativePluginManagerTab::Marketplace,
                entry_count,
                true,
                cx,
            ));
        card.into_any_element()
    }

    fn plugin_tag_label(&self, tag: &str) -> String {
        match tag {
            "language" => self.i18n.t("plugin.marketplace_languages"),
            "preview" => self.i18n.t("plugin.marketplace_previews"),
            "host-tools" => self.i18n.t("plugin.marketplace_tools"),
            "host-sources" => self.i18n.t("plugin.marketplace_sources"),
            "workspace" => self.i18n.t("plugin.marketplace_workspace"),
            "utilities" => self.i18n.t("plugin.marketplace_utilities"),
            "acp" => self.i18n.t("plugin.marketplace_acp"),
            "remote-connections" => self.i18n.t("plugin.marketplace_remote_connections"),
            _ => tag.to_string(),
        }
    }

    pub(in crate::workspace) fn open_mosh_plugin(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_plugin_manager_tab(window, cx);
        self.update_plugin_manager_state(cx, |manager| {
            manager.previous_tab = manager.active_tab;
            manager.active_tab = NativePluginManagerTab::Marketplace;
            manager.marketplace_search_draft = "com.oxideterm.terminal.mosh".into();
            manager.marketplace_tag = Some("remote-connections".into());
            manager.marketplace_updates_only = false;
            manager.pagination[1].page = 0;
            manager.section_list_state.splice(
                PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX
                    ..PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX + 1,
                1,
            );
        });
        self.start_native_plugin_marketplace_load(cx);
        cx.notify();
    }

    pub(in crate::workspace) fn open_acp_plugin_manager(
        &mut self,
        add_custom: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if add_custom {
            self.edit_settings(oxideterm_settings_model::ai_add_acp_agent, cx);
        }
        self.open_plugin_manager_tab(window, cx);
        self.update_plugin_manager_state(cx, |manager| {
            manager.previous_tab = manager.active_tab;
            manager.active_tab = NativePluginManagerTab::Installed;
            manager.installed_tag = Some("acp".into());
            manager.pagination[0].page = 0;
            manager.custom_acp_expanded |= add_custom;
            manager.section_list_state.splice(
                PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX
                    ..PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX + 1,
                1,
            );
        });
        cx.notify();
    }

    fn render_plugin_tag_filter(
        &self,
        tab: NativePluginManagerTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let plugins = self.plugin_entity.read(cx);
        let manager = plugins.manager_state();
        let registry = plugins.registry();
        let (tag, tag_lists) = match tab {
            NativePluginManagerTab::Installed => (
                &manager.installed_tag,
                registry
                    .plugins()
                    .iter()
                    .map(|plugin| {
                        native_plugin_tags_for_id(
                            &plugin.manifest.id,
                            &manager.marketplace_entries,
                            registry,
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
            NativePluginManagerTab::Marketplace => (
                &manager.marketplace_tag,
                manager
                    .marketplace_entries
                    .iter()
                    .map(|entry| entry.tags.as_deref().unwrap_or_default())
                    .collect(),
            ),
        };
        let tags = native_plugin_filter_tags(tag_lists.into_iter().flatten().map(String::as_str));
        let mut filters = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(self.tokens.spacing.two));
        for value in std::iter::once(None).chain(tags.into_iter().map(Some)) {
            let selected = tag == &value;
            let label = value
                .as_deref()
                .map(|tag| self.plugin_tag_label(tag))
                .unwrap_or_else(|| self.i18n.t("plugin.marketplace_all"));
            filters = filters.child(self.render_native_plugin_marketplace_filter(
                label,
                selected,
                cx.listener(move |this, _, _, cx| {
                    this.update_plugin_manager_state(cx, |manager| {
                        match tab {
                            NativePluginManagerTab::Installed => {
                                manager.installed_tag = value.clone()
                            }
                            NativePluginManagerTab::Marketplace => {
                                manager.marketplace_tag = value.clone()
                            }
                        }
                        manager.pagination[native_plugin_manager_tab_index(tab)].page = 0;
                        manager.section_list_state.splice(
                            PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX
                                ..PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX + 1,
                            1,
                        );
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        filters.into_any_element()
    }

    fn change_plugin_page(
        &self,
        tab: NativePluginManagerTab,
        page: usize,
        page_size: Option<usize>,
        cx: &mut Context<Self>,
    ) {
        self.update_plugin_manager_state(cx, |manager| {
            let pagination = &mut manager.pagination[native_plugin_manager_tab_index(tab)];
            if let Some(page_size) = page_size {
                pagination.set_page_size(page_size);
            } else {
                pagination.page = page;
            }
            manager.section_list_state.splice(
                PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX
                    ..PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX + 1,
                1,
            );
            manager.section_list_state.scroll_to(gpui::ListOffset {
                item_ix: PLUGIN_MANAGER_TABBED_CONTENT_SECTION_INDEX,
                offset_in_item: px(0.0),
            });
        });
        cx.notify();
    }

    pub(in crate::workspace) fn plugin_page_size(
        &self,
        tab: NativePluginManagerTab,
        cx: &Context<Self>,
    ) -> usize {
        self.plugin_manager_state(cx).pagination[native_plugin_manager_tab_index(tab)].page_size
    }

    pub(in crate::workspace) fn commit_plugin_page_size(&mut self, cx: &mut Context<Self>) {
        let tab = match self.focused_settings_input {
            Some(SettingsInput::NativePluginInstalledPageSize) => NativePluginManagerTab::Installed,
            Some(SettingsInput::NativePluginMarketplacePageSize) => {
                NativePluginManagerTab::Marketplace
            }
            _ => return,
        };
        if let Some(size) = parse_plugin_page_size(&self.settings_input_draft) {
            self.focused_settings_input = None;
            self.settings_input_draft.clear();
            self.clear_ime_selection();
            self.change_plugin_page(tab, 0, Some(size), cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn plugin_page_number(
        &self,
        tab: NativePluginManagerTab,
        cx: &Context<Self>,
    ) -> usize {
        self.plugin_manager_state(cx).pagination[native_plugin_manager_tab_index(tab)].page + 1
    }

    pub(in crate::workspace) fn commit_plugin_page_jump(&mut self, cx: &mut Context<Self>) {
        let tab = match self.focused_settings_input {
            Some(SettingsInput::NativePluginInstalledPageJump) => NativePluginManagerTab::Installed,
            Some(SettingsInput::NativePluginMarketplacePageJump) => {
                NativePluginManagerTab::Marketplace
            }
            _ => return,
        };
        let pagination =
            self.plugin_manager_state(cx).pagination[native_plugin_manager_tab_index(tab)];
        if let Some(page) = pagination.jump_target(&self.settings_input_draft) {
            self.focused_settings_input = None;
            self.settings_input_draft.clear();
            self.clear_ime_selection();
            self.change_plugin_page(tab, page, None, cx);
        }
        cx.notify();
    }

    fn render_plugin_pagination(
        &self,
        tab: NativePluginManagerTab,
        total: usize,
        is_footer: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pagination =
            self.plugin_manager_state(cx).pagination[native_plugin_manager_tab_index(tab)];
        let page = pagination.page;
        let page_count = pagination.page_count();
        let start = if total == 0 {
            0
        } else {
            page * pagination.page_size + 1
        };
        let end = (page * pagination.page_size)
            .saturating_add(pagination.page_size)
            .min(total);
        let input = match tab {
            NativePluginManagerTab::Installed => SettingsInput::NativePluginInstalledPageSize,
            NativePluginManagerTab::Marketplace => SettingsInput::NativePluginMarketplacePageSize,
        };
        let focused = self.focused_settings_input == Some(input);
        let invalid = focused && parse_plugin_page_size(&self.settings_input_draft).is_none();
        let mut pages = div().flex().flex_wrap().items_center().gap(px(4.0)).child(
            self.render_native_plugin_action_button(
                LucideIcon::ChevronLeft,
                self.i18n.t("plugin.previous_page"),
                page == 0,
                cx.listener(move |this, _, _, cx| {
                    this.change_plugin_page(tab, page.saturating_sub(1), None, cx)
                }),
            )
            .opacity(if page == 0 { 0.45 } else { 1.0 }),
        );
        let mut previous = None;
        for index in (0..page_count)
            .filter(|index| *index == 0 || *index + 1 == page_count || index.abs_diff(page) <= 1)
        {
            if previous.is_some_and(|previous| index > previous + 1) {
                pages = pages.child(div().px(px(4.0)).child("…"));
            }
            pages = pages.child(self.render_native_plugin_marketplace_filter(
                (index + 1).to_string(),
                index == page,
                cx.listener(move |this, _, _, cx| this.change_plugin_page(tab, index, None, cx)),
            ));
            previous = Some(index);
        }
        pages = pages.child(
            self.render_native_plugin_action_button(
                LucideIcon::ChevronRight,
                self.i18n.t("plugin.next_page"),
                page + 1 >= page_count,
                cx.listener(move |this, _, _, cx| this.change_plugin_page(tab, page + 1, None, cx)),
            )
            .opacity(if page + 1 >= page_count { 0.45 } else { 1.0 }),
        );
        if is_footer {
            let jump_input = match tab {
                NativePluginManagerTab::Installed => SettingsInput::NativePluginInstalledPageJump,
                NativePluginManagerTab::Marketplace => {
                    SettingsInput::NativePluginMarketplacePageJump
                }
            };
            let jump_focused = self.focused_settings_input == Some(jump_input);
            let jump_invalid =
                jump_focused && pagination.jump_target(&self.settings_input_draft).is_none();
            pages = pages.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(6.0))
                    .child(self.i18n.t("plugin.jump_to_page"))
                    .child(self.number_input(jump_input, (page + 1).to_string(), 64.0, cx))
                    .child(
                        self.i18n
                            .t("plugin.total_pages")
                            .replace("{{pages}}", &page_count.to_string()),
                    )
                    .child(self.render_native_plugin_action_button(
                        LucideIcon::ArrowRight,
                        self.i18n.t("plugin.jump_page"),
                        !jump_focused || jump_invalid,
                        cx.listener(|this, _, _, cx| this.commit_plugin_page_jump(cx)),
                    ))
                    .when(jump_invalid, |row| {
                        row.child(
                            div().text_color(rgb(self.tokens.ui.error)).child(
                                self.i18n
                                    .t("plugin.invalid_page_number")
                                    .replace("{{pages}}", &page_count.to_string()),
                            ),
                        )
                    }),
            );
        }

        div()
            .w_full()
            .py(px(8.0))
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        self.i18n
                            .t("plugin.page_range")
                            .replace("{{start}}", &start.to_string())
                            .replace("{{end}}", &end.to_string())
                            .replace("{{total}}", &total.to_string()),
                    )
                    .when(is_footer, |row| {
                        row.child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(4.0))
                                .child(self.i18n.t("plugin.per_page"))
                                .child(self.number_input(
                                    input,
                                    pagination.page_size.to_string(),
                                    76.0,
                                    cx,
                                ))
                                .child(self.render_native_plugin_action_button(
                                    LucideIcon::Check,
                                    self.i18n.t("plugin.apply_page_size"),
                                    !focused || invalid,
                                    cx.listener(|this, _, _, cx| this.commit_plugin_page_size(cx)),
                                ))
                                .when(invalid, |row| {
                                    row.child(
                                        div()
                                            .text_color(rgb(self.tokens.ui.error))
                                            .child(self.i18n.t("plugin.invalid_page_size")),
                                    )
                                }),
                        )
                    }),
            )
            .child(pages)
            .into_any_element()
    }

    fn render_native_plugin_marketplace_filter(
        &self,
        label: String,
        active: bool,
        listener: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        oxideterm_gpui_ui::action_chip(
            &self.tokens,
            label,
            None,
            oxideterm_gpui_ui::ActionChipOptions::new()
                .active(active)
                .font_size(self.tokens.metrics.ui_text_xs),
        )
        .on_mouse_down(MouseButton::Left, listener)
        .into_any_element()
    }

    fn render_native_plugin_marketplace_row(
        &self,
        entry: &plugin_host::NativePluginRegistryEntry,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let compatible = plugin_host::NativePluginRegistry::select_registry_release(entry);
        let latest = plugin_host::NativePluginRegistry::latest_registry_release(entry);
        let newer_requirement = latest
            .as_ref()
            .filter(|latest| {
                !plugin_host::NativePluginRegistry::registry_entry_supports_current_version(latest)
                    && compatible.as_ref().is_none_or(|selected| {
                        plugin_host::NativePluginRegistry::registry_entry_is_update(
                            latest,
                            &selected.version,
                        )
                    })
            })
            .map(|latest| {
                let requirement = latest
                    .engines
                    .as_ref()
                    .and_then(|engines| engines.oxideterm.as_deref())
                    .map(str::to_string)
                    .or_else(|| {
                        latest
                            .min_oxideterm_version
                            .as_ref()
                            .map(|version| format!(">={version}"))
                    })
                    .unwrap_or_default();
                self.i18n
                    .t("plugin.marketplace_newer_requires")
                    .replace("{{version}}", &latest.version)
                    .replace("{{requirement}}", &requirement)
            });
        let entry = compatible.as_ref().or(latest.as_ref()).unwrap_or(entry);
        let installed_version = self
            .plugin_entity
            .read(cx)
            .registry()
            .plugins()
            .iter()
            .find(|plugin| plugin.manifest.id == entry.id)
            .map(|plugin| plugin.manifest.version.clone());
        let update_available = compatible.is_some()
            && installed_version.as_deref().is_some_and(|version| {
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
        let availability = if entry.history_pending() {
            Some((
                self.i18n.t(
                    if self
                        .plugin_manager_state(cx)
                        .failed_histories
                        .contains(&entry.id)
                    {
                        "plugin.marketplace_load_error"
                    } else {
                        "plugin.loading_marketplace"
                    },
                ),
                StatusTone::Info,
            ))
        } else if !host_supported {
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
        } else {
            None
        };
        let capabilities = native_plugin_registry_capabilities_label(&self.i18n, entry);
        let expanded = self
            .plugin_manager_state(cx)
            .marketplace_expanded_ids
            .contains(&entry.id);
        let detail_id = entry.id.clone();
        let expected_id = entry.id.clone();
        let package_for_install = package;
        let homepage = entry
            .homepage
            .as_deref()
            .and_then(native_plugin_safe_https_url);
        let mut actions = Vec::with_capacity(3);
        actions.push(
            self.workspace_tooltip_icon_button(
                if expanded {
                    LucideIcon::ChevronDown
                } else {
                    LucideIcon::ChevronRight
                },
                self.tokens.metrics.ui_menu_icon_size,
                rgb(theme.text_muted),
                oxideterm_gpui_ui::IconButtonOptions::compact(PLUGIN_MANAGER_ROW_ACTION_SIZE),
                self.i18n.t(if expanded {
                    "plugin.hide_details"
                } else {
                    "plugin.show_details"
                }),
                "plugin-marketplace-details",
                false,
                cx.listener(move |this, _event, _window, cx| {
                    this.update_plugin_manager_state(cx, |manager| {
                        if !manager.marketplace_expanded_ids.insert(detail_id.clone()) {
                            manager.marketplace_expanded_ids.remove(&detail_id);
                        }
                    });
                    cx.notify();
                }),
                cx.entity(),
            )
            .into_any_element(),
        );
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

        let notice = newer_requirement
            .map(|message| (message, theme.warning))
            .or_else(|| {
                availability.map(|(message, tone)| {
                    (
                        message,
                        if tone == StatusTone::Error {
                            theme.error
                        } else {
                            theme.warning
                        },
                    )
                })
            });
        let summary = div()
            .min_w_0()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.one))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.two))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(entry.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(format!("v{}", entry.version)),
                    ),
            )
            .when_some(entry.description.clone(), |body, description| {
                body.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .when(!expanded, |text| text.truncate())
                        .child(description),
                )
            })
            .when_some(notice, |body, (message, color)| {
                body.child(
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(color))
                        .child(message),
                )
            });

        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .py(px(self.tokens.spacing.three))
            .border_b_1()
            .border_color(plugin_manager_theme_border_half(
                theme.border,
                has_background,
            ))
            .child(action_slot_row(
                &self.tokens,
                ActionSlotRowOptions::new()
                    .align_start()
                    .gap(self.tokens.spacing.three)
                    .trailing_gap(self.tokens.spacing.two),
                Some(Self::render_lucide_icon(
                    LucideIcon::Puzzle,
                    self.tokens.metrics.ui_menu_icon_size,
                    rgb(theme.text_muted),
                )),
                summary.into_any_element(),
                actions,
            ))
            .when(expanded, |row| {
                row.child(
                    div()
                        .ml(px(
                            self.tokens.metrics.ui_menu_icon_size + self.tokens.spacing.three
                        ))
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(self.tokens.spacing.one))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .line_height(px(self.tokens.metrics.ui_text_xs + 6.0))
                        .text_color(rgb(theme.text_muted))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(px(self.tokens.spacing.three))
                                .when_some(entry.author.clone(), |metadata, author| {
                                    metadata.child(
                                        self.i18n
                                            .t("plugin.by_author")
                                            .replace("{{author}}", &author),
                                    )
                                })
                                .when_some(homepage, |metadata, homepage| {
                                    metadata.child(
                                        oxideterm_gpui_ui::button::button_with(
                                            &self.tokens,
                                            self.i18n.t("plugin.marketplace_homepage"),
                                            oxideterm_gpui_ui::button::ButtonOptions {
                                                variant:
                                                    oxideterm_gpui_ui::button::ButtonVariant::Link,
                                                size: oxideterm_gpui_ui::button::ButtonSize::Sm,
                                                ..Default::default()
                                            },
                                        )
                                        .h_auto()
                                        .px_0()
                                        .gap(px(self.tokens.spacing.one))
                                        .text_size(px(self.tokens.metrics.ui_text_xs))
                                        .child(Self::render_lucide_icon(
                                            LucideIcon::ExternalLink,
                                            self.tokens.metrics.ui_text_xs,
                                            rgb(theme.accent),
                                        ))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            move |_event, _window, cx| {
                                                cx.open_url(&homepage);
                                                cx.stop_propagation();
                                            },
                                        ),
                                    )
                                }),
                        )
                        .child(self.render_native_plugin_detail_row(
                            self.i18n.t("plugin.create_plugin_id"),
                            entry.id.clone(),
                        ))
                        .when_some(
                            installed_version.filter(|version| version != &entry.version),
                            |details, version| {
                                details.child(
                                    self.i18n
                                        .t("plugin.marketplace_installed_version")
                                        .replace("{{version}}", &version),
                                )
                            },
                        )
                        .when_some(capabilities, |details, label| details.child(label))
                        .when(
                            entry
                                .tags
                                .as_ref()
                                .is_some_and(|tags| tags.iter().any(|tag| tag == "acp")),
                            |details| {
                                details.child(
                                    div()
                                        .whitespace_normal()
                                        .child(self.i18n.t("plugin.acp_setup_hint")),
                                )
                            },
                        ),
                )
            })
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
                    // The wrapping basis belongs to this horizontal form, not the shared input.
                    .child(
                        self.render_native_plugin_manager_icon_input(
                            LucideIcon::Download,
                            SettingsInput::NativePluginInstallUrl,
                            self.i18n.t("plugin.url_placeholder"),
                            cx,
                        )
                        .flex_1()
                        .flex_basis(px(PLUGIN_MANAGER_INLINE_INPUT_BASIS)),
                    )
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
            .child(
                self.render_native_plugin_manager_icon_input(
                    LucideIcon::Search,
                    SettingsInput::NativePluginRegistryUrl,
                    "https://example.com/registry.json".to_string(),
                    cx,
                )
                .flex_1()
                .flex_basis(px(PLUGIN_MANAGER_INLINE_INPUT_BASIS)),
            )
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
    ) -> Div {
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
    ) -> Div {
        let theme = self.tokens.ui;
        div()
            .relative()
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

        if overwrite {
            self.stop_acp_plugin(expected_id.as_deref(), cx);
            self.acp_entity.update(cx, |entity, _cx| {
                entity.begin_plugin_update(expected_id.as_deref())
            });
        }
        let mut retired_desktops = if overwrite {
            self.remote_desktop.update(cx, |desktops, cx| {
                desktops.stop_plugins(expected_id.as_deref(), cx)
            })
        } else {
            Vec::new()
        };
        if overwrite
            && expected_id
                .as_deref()
                .is_none_or(|id| id == "com.oxideterm.terminal.mosh")
        {
            retired_desktops.push(self.mosh_plugin_sessions.stop());
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
                retired_desktops,
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
                self.acp_entity
                    .update(cx, |entity, _cx| entity.finish_plugin_update());
                let settings_path = self.settings_store.path();
                let i18n = &self.i18n;
                let bootstrap_runtime = self.plugin_entity.update(cx, |plugins, _cx| {
                    plugins.apply_manager_deliveries(settings_path, i18n, _cx)
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
        has_background: bool,
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
        let plugin_name = plugin.manifest.name.clone();
        let expand_plugin_id = plugin.manifest.id.clone();
        let uninstall_plugin_id = plugin.manifest.id.clone();
        let reload_plugin_name = plugin.manifest.name.clone();
        let mut row = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.two))
            .py(px(self.tokens.spacing.three))
            .border_b_1()
            .border_color(plugin_manager_theme_border_half(
                theme.border,
                has_background,
            ))
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .gap(px(self.tokens.spacing.three))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .overflow_hidden()
                            .flex()
                            .items_start()
                            .gap(px(self.tokens.spacing.three))
                            .child(div().flex_shrink_0().child(Self::render_lucide_icon(
                                LucideIcon::Puzzle,
                                self.tokens.metrics.ui_menu_icon_size,
                                rgb(theme.text_muted),
                            )))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .overflow_hidden()
                                    .flex()
                                    .flex_col()
                                    .gap(px(self.tokens.spacing.one))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .items_center()
                                            .gap(px(self.tokens.spacing.two))
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
                                                    .flex_none()
                                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                                    .text_color(rgb(theme.text_muted))
                                                    .child(format!("v{}", plugin.manifest.version)),
                                            )
                                            .child(
                                                div()
                                                    .flex_none()
                                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                                    .text_color(
                                                        oxideterm_gpui_ui::status_pill_colors(
                                                            &self.tokens,
                                                            state_tone,
                                                            false,
                                                        )
                                                        .text,
                                                    )
                                                    .child(state_label),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .min_w(px(0.0))
                                            .when(!is_expanded, |description| {
                                                description.line_clamp(2)
                                            })
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
                            .justify_end()
                            .gap(px(self.tokens.spacing.two))
                            .child(self.workspace_tooltip_icon_button(
                                if is_expanded {
                                    LucideIcon::ChevronDown
                                } else {
                                    LucideIcon::ChevronRight
                                },
                                self.tokens.metrics.ui_menu_icon_size,
                                rgb(theme.text_muted),
                                oxideterm_gpui_ui::IconButtonOptions::compact(
                                    PLUGIN_MANAGER_ROW_ACTION_SIZE,
                                ),
                                self.i18n.t(if is_expanded {
                                    "plugin.hide_details"
                                } else {
                                    "plugin.show_details"
                                }),
                                "plugin-installed-details",
                                false,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.update_plugin_manager_state(cx, |manager| {
                                        if !manager
                                            .expanded_plugin_ids
                                            .insert(expand_plugin_id.clone())
                                        {
                                            manager.expanded_plugin_ids.remove(&expand_plugin_id);
                                        }
                                    });
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                                cx.entity(),
                            ))
                            .when(is_error || is_active, |right| {
                                right.child(self.render_native_plugin_row_icon_button(
                                    LucideIcon::RefreshCw,
                                    theme.text_muted,
                                    Some(cx.listener(move |this, _event, _window, cx| {
                                        let registry = plugin_host::NativePluginRegistry::discover(
                                            this.settings_store.path(),
                                        );
                                        this.plugin_entity.update(cx, |plugins, _cx| {
                                            plugins.replace_registry(registry, _cx);
                                        });
                                        this.bootstrap_native_plugin_runtime(cx);
                                        let success_template = this.i18n.t("plugin.reload_success");
                                        this.update_plugin_manager_state(cx, |manager| {
                                            manager.operation_status =
                                                NativePluginManagerOperationStatus::Success(
                                                    success_template
                                                        .replace("{{name}}", &reload_plugin_name),
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
                                        plugins.set_plugin_enabled(&plugin_id, next_enabled, _cx)
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
                                        this.bootstrap_native_plugin_runtime(cx);
                                        let success_key = if next_enabled {
                                            "plugin.enable_success"
                                        } else {
                                            "plugin.disable_success"
                                        };
                                        let message = this
                                            .i18n
                                            .t(success_key)
                                            .replace("{{name}}", &plugin_name);
                                        this.update_plugin_manager_state(cx, |manager| {
                                            manager.operation_status =
                                                NativePluginManagerOperationStatus::Success(
                                                    message,
                                                );
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
                                    if this.plugin_entity.read(cx).manager_operation_in_flight() {
                                        return;
                                    }
                                    this.stop_acp_plugin(Some(&uninstall_plugin_id), cx);
                                    let _ = this.plugin_entity.update(cx, |plugins, cx| {
                                        let result = plugins.set_plugin_enabled(
                                            &uninstall_plugin_id,
                                            false,
                                            cx,
                                        );
                                        if result.is_ok() {
                                            plugins
                                                .begin_remote_desktop_removal(&uninstall_plugin_id);
                                        }
                                        result
                                    });
                                    let mut workers =
                                        this.remote_desktop.update(cx, |desktops, cx| {
                                            desktops.stop_plugins(Some(&uninstall_plugin_id), cx)
                                        });
                                    if uninstall_plugin_id == "com.oxideterm.terminal.mosh" {
                                        workers.push(this.mosh_plugin_sessions.stop());
                                    }
                                    let uninstall_plugin_id = uninstall_plugin_id.clone();
                                    let receiver = this.plugin_entity.update(cx, |plugins, cx| {
                                        plugins.start_plugin_uninstall(
                                            uninstall_plugin_id.clone(),
                                            false,
                                            workers,
                                            cx,
                                        )
                                    });
                                    cx.spawn(async move |workspace, cx| {
                                        let result = receiver.await;
                                        let _ = workspace.update(cx, |this, cx| {
                                            if let Ok(Err(error)) = result {
                                                this.plugin_entity.update(cx, |plugins, _cx| {
                                                    plugins.registry_mut().record_manager_error(
                                                        uninstall_plugin_id,
                                                        error,
                                                    )
                                                });
                                            }
                                            this.bootstrap_native_plugin_runtime(cx);
                                            cx.notify();
                                        });
                                    })
                                    .detach();
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
                    .ml(px(
                        self.tokens.metrics.ui_menu_icon_size + self.tokens.spacing.three
                    ))
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
            row = row.child(self.render_native_plugin_expanded_details(plugin, cx));
        }
        row.into_any_element()
    }

    fn render_native_plugin_expanded_details(
        &self,
        plugin: &plugin_host::NativePluginInfo,
        cx: &mut Context<Self>,
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
            .ml(px(
                self.tokens.metrics.ui_menu_icon_size + self.tokens.spacing.three
            ))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(8.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .line_height(px(18.0))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(self.render_native_plugin_detail_row("ID", manifest.id.clone()))
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
                        .child(div().child(contribution_labels.join(" · "))),
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
                        permissions.child(
                            div()
                                .min_w(px(0.0))
                                .flex()
                                .flex_col()
                                .gap(px(6.0))
                                .children(permission_capabilities.into_iter().map(|capability| {
                                    let (label, is_trusted_process) = if capability
                                        == plugin_host::NATIVE_PLUGIN_TRUSTED_PROCESS_CAPABILITY
                                    {
                                        (self.i18n.t("plugin.permission_trusted_process"), true)
                                    } else {
                                        (capability, false)
                                    };
                                    div()
                                        .min_w(px(0.0))
                                        .whitespace_normal()
                                        .text_color(rgb(if is_trusted_process {
                                            theme.warning
                                        } else {
                                            theme.text_muted
                                        }))
                                        .child(label)
                                })),
                        )
                    })
                    .when(permission_requires_review, |permissions| {
                        permissions.child(
                            div()
                                .mt(px(2.0))
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
            .when(
                matches!(
                    plugin.runtime_plan,
                    plugin_host::NativePluginRuntimePlan::Acp { .. }
                ),
                |details| {
                    let settings = self.settings_store.settings();
                    details
                        .child(
                            div()
                                .min_w_0()
                                .whitespace_normal()
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("plugin.acp_setup_hint")),
                        )
                        .children(
                            settings
                                .ai
                                .acp_agents
                                .iter()
                                .enumerate()
                                .filter(|(_, agent)| {
                                    agent.plugin_id.as_deref() == Some(manifest.id.as_str())
                                })
                                .map(|(index, agent)| self.ai_acp_agent_card(index, agent, cx)),
                        )
                },
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

fn native_plugin_filter_tags<'a>(tags: impl Iterator<Item = &'a str>) -> Vec<String> {
    tags.map(native_plugin_category)
        .filter(|tag| !tag.is_empty())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn native_plugin_tag_matches(tags: &[String], selected: Option<&str>) -> bool {
    selected.is_none_or(|selected| {
        tags.iter()
            .any(|tag| native_plugin_category(tag) == native_plugin_category(selected))
    })
}

fn native_plugin_category(tag: &str) -> String {
    match tag.trim().to_lowercase().as_str() {
        // Published catalogs remain usable until their cached tags refresh.
        "remote-desktop" => "remote-connections".into(),
        tag => tag.to_string(),
    }
}

fn native_plugin_tags_for_id<'a>(
    id: &str,
    entries: &'a [plugin_host::NativePluginRegistryEntry],
    registry: &'a plugin_host::NativePluginRegistry,
) -> &'a [String] {
    entries
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| entry.tags.as_deref().unwrap_or_default())
        .unwrap_or_else(|| registry.catalog_tags(id))
}

fn native_plugin_marketplace_entry_visible(
    entry: &plugin_host::NativePluginRegistryEntry,
    query: &str,
    tag: Option<&str>,
    updates_only: bool,
    installed_version: Option<&str>,
) -> bool {
    native_plugin_marketplace_entry_matches(entry, query)
        && native_plugin_tag_matches(entry.tags.as_deref().unwrap_or_default(), tag)
        && (!updates_only
            || installed_version.is_some_and(|version| {
                plugin_host::NativePluginRegistry::select_registry_release(entry).is_some_and(
                    |entry| {
                        plugin_host::NativePluginRegistry::registry_entry_is_update(&entry, version)
                    },
                )
            }))
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
    if plugin_host::validate_native_plugin_host(&plugin.manifest).is_err() {
        return (
            i18n.t("plugin.marketplace_incompatible"),
            StatusTone::Warning,
        );
    }
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
    if plugin_host::validate_native_plugin_host(&plugin.manifest).is_err() {
        let requirement = plugin
            .manifest
            .engines
            .as_ref()
            .and_then(|engines| engines.oxideterm.as_deref())
            .unwrap_or_default();
        return Some(
            i18n.t("plugin.host_version_incompatible")
                .replace("{{requirement}}", requirement)
                .replace("{{current}}", env!("CARGO_PKG_VERSION")),
        );
    }
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

    #[gpui::test]
    fn plugin_manager_fields_receive_platform_text(cx: &mut gpui::TestAppContext) {
        let executable = std::env::current_exe().unwrap();
        let fixture_key = "OXIDETERM_PLUGIN_INPUT_TEST_DIR";
        let Some(fixture_dir) = std::env::var_os(fixture_key) else {
            // Workspace startup must not discover the user's settings or installed plugins.
            let directory = tempfile::tempdir_in(executable.parent().unwrap()).unwrap();
            let child = directory.path().join(executable.file_name().unwrap());
            std::fs::hard_link(&executable, &child).unwrap();
            std::fs::write(directory.path().join("portable"), []).unwrap();
            let output = std::process::Command::new(child)
                .arg(cx.test_function_name().unwrap())
                .arg("--nocapture")
                .env(fixture_key, directory.path())
                .env_remove("APPIMAGE")
                .current_dir(directory.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "plugin input regression failed:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        };
        let settings_path = default_settings_path();
        assert!(settings_path.starts_with(std::path::PathBuf::from(fixture_dir)));
        let mut settings = SettingsStore::load_from_path(settings_path).unwrap();
        settings.settings_mut().ssh_config.auto_load_hosts = false;
        settings.settings_mut().onboarding_completed = true;
        settings.save().unwrap();
        let (shell, cx) = cx.add_window_view(|window, cx| {
            let workspace = cx.new(|cx| WorkspaceApp::new(window, cx, None, None).unwrap());
            WorkspaceWindowShell::new(workspace, window, cx)
        });
        let workspace = shell.read_with(cx, |shell, _| shell.session_entity());
        cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_plugin_manager_tab(window, cx);
                workspace.focus_settings_input(
                    SettingsInput::NativePluginMarketplaceSearch,
                    String::new(),
                    cx,
                );
            });
            window.draw(cx).clear(cx);
        });
        cx.simulate_input("rust语言");
        workspace.read_with(cx, |workspace, cx| {
            assert_eq!(
                workspace.plugin_manager_state(cx).marketplace_search_draft,
                "rust语言"
            );
        });
        cx.simulate_keystrokes("backspace");
        workspace.read_with(cx, |workspace, cx| {
            assert_eq!(
                workspace.plugin_manager_state(cx).marketplace_search_draft,
                "rust语"
            );
        });
        for input in [
            SettingsInput::NativePluginInstalledPageSize,
            SettingsInput::NativePluginMarketplacePageSize,
        ] {
            workspace.update(cx, |workspace, cx| {
                workspace.focus_settings_input(input, String::new(), cx);
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            cx.simulate_input("25");
            cx.simulate_keystrokes("enter");
            workspace.update(cx, |workspace, cx| {
                assert_eq!(workspace.current_settings_input_value(input, cx), "25");
            });
        }
        let agent_index = cx.update(|window, cx| {
            workspace.update(cx, |workspace, cx| {
                workspace.open_acp_plugin_manager(true, window, cx);
                let index = workspace.settings_store.settings().ai.acp_agents.len() - 1;
                workspace.focus_settings_input(
                    SettingsInput::AiAcpAgentDisplayName(index),
                    String::new(),
                    cx,
                );
                index
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_input("测试代理");
        cx.simulate_keystrokes("enter");
        workspace.read_with(cx, |workspace, _cx| {
            assert_eq!(
                workspace.settings_store.settings().ai.acp_agents[agent_index].display_name,
                "测试代理"
            );
        });
    }

    #[test]
    fn plugin_pagination_keeps_order_and_clamps_after_removal() {
        let plugins = (0..23).collect::<Vec<_>>();
        let mut pagination = PluginPagination {
            page: 2,
            ..Default::default()
        };
        assert_eq!(&plugins[pagination.range(plugins.len())], &[20, 21, 22]);

        pagination.set_page_size(20);
        assert_eq!(&plugins[pagination.range(plugins.len())], &[20, 21, 22]);
        pagination.set_page_size(10);
        assert_eq!(&plugins[pagination.range(plugins.len())], &[20, 21, 22]);

        let remaining = &plugins[..12];
        assert_eq!(&remaining[pagination.range(remaining.len())], &[10, 11]);
        pagination.set_page_size(7);
        assert_eq!(
            &plugins[pagination.range(plugins.len())],
            &[7, 8, 9, 10, 11, 12, 13]
        );
        pagination.set_page_size(usize::MAX);
        assert_eq!(
            &plugins[pagination.range(plugins.len())],
            plugins.as_slice()
        );
        pagination.set_page_size(10);
        assert_eq!(pagination.range(0), 0..0);
        assert_eq!(
            &plugins[pagination.range(plugins.len())],
            &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
    }

    #[test]
    fn plugin_page_size_accepts_custom_positive_integers() {
        for (input, expected) in [
            ("7", Some(7)),
            (" 35 ", Some(35)),
            ("125", Some(125)),
            ("", None),
            ("0", None),
            ("-3", None),
            ("2.5", None),
            ("abc", None),
            ("999999999999999999999999999999999999", None),
        ] {
            assert_eq!(parse_plugin_page_size(input), expected, "{input:?}");
        }
    }

    #[test]
    fn plugin_page_jump_uses_filtered_count_and_current_page_size() {
        let mut pagination = PluginPagination::default();
        pagination.range(135);
        for (input, expected) in [
            ("1", Some(0)),
            (" 8 ", Some(7)),
            ("14", Some(13)),
            ("15", None),
            ("0", None),
            ("-1", None),
            ("2.5", None),
            ("", None),
            ("no", None),
        ] {
            assert_eq!(pagination.jump_target(input), expected, "{input:?}");
        }
        pagination.set_page_size(50);
        assert_eq!(pagination.jump_target("3"), Some(2));
        assert_eq!(pagination.jump_target("4"), None);
        pagination.range(12);
        assert_eq!(pagination.jump_target("1"), Some(0));
        assert_eq!(pagination.jump_target("2"), None);
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
            engines: None,
            releases: Vec::new(),
            history: None,
        }
    }

    #[test]
    fn dynamic_plugin_tags_filter_installed_ids_without_a_category_allowlist() {
        assert_eq!(
            native_plugin_filter_tags(
                ["remote-desktop", "remote-connections", "host-tools"].into_iter()
            ),
            ["host-tools", "remote-connections"]
        );
        assert!(native_plugin_tag_matches(
            &["remote-desktop".into()],
            Some("remote-connections")
        ));
        let registry = plugin_host::NativePluginRegistry::default();
        let mut first = registry_entry_with_capabilities(None);
        first.id = "com.example.first".into();
        first.tags = Some(vec![" Future-Tag ".into(), "preview".into(), "".into()]);
        let mut second = first.clone();
        second.id = "com.example.second".into();
        second.tags = Some(vec!["preview".into(), "PREVIEW".into()]);
        let entries = [first, second];
        assert_eq!(
            native_plugin_filter_tags(
                entries
                    .iter()
                    .flat_map(|entry| entry.tags.as_ref().unwrap())
                    .map(String::as_str)
            ),
            ["future-tag", "preview"]
        );
        for (selected, expected) in [
            (Some("future-tag"), vec!["com.example.first"]),
            (
                Some("preview"),
                vec!["com.example.first", "com.example.second"],
            ),
            (
                None,
                vec![
                    "com.example.first",
                    "com.example.local",
                    "com.example.second",
                ],
            ),
        ] {
            let actual = [
                "com.example.first",
                "com.example.local",
                "com.example.second",
            ]
            .into_iter()
            .filter(|id| {
                native_plugin_tag_matches(
                    native_plugin_tags_for_id(id, &entries, &registry),
                    selected,
                )
            })
            .collect::<Vec<_>>();
            assert_eq!(actual, expected, "{selected:?}");
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
    fn marketplace_filters_combine_search_tag_and_installed_version() {
        let entries = [
            ("older", "language", Some("1.1.0")),
            ("current", "language", Some("1.2.0")),
            ("newer", "language", Some("2.0.0")),
            ("uninstalled", "language", None),
            ("tool", "terminal", Some("1.1.0")),
            ("pdf", "preview", None),
        ]
        .map(|(id, tag, installed)| {
            let mut entry = registry_entry_with_capabilities(None);
            entry.id = id.into();
            entry.tags = Some(vec![tag.into()]);
            (entry, installed)
        });
        for (query, tag, updates_only, expected) in [
            ("", None, true, vec!["older", "tool"]),
            ("", Some("language"), true, vec!["older"]),
            ("", Some("preview"), false, vec!["pdf"]),
            ("", Some("preview"), true, vec![]),
            ("tool", Some("language"), true, vec![]),
            ("uninstalled", Some("language"), false, vec!["uninstalled"]),
            (
                "language",
                None,
                false,
                vec!["older", "current", "newer", "uninstalled"],
            ),
        ] {
            let actual = entries
                .iter()
                .filter(|(entry, installed)| {
                    native_plugin_marketplace_entry_visible(
                        entry,
                        query,
                        tag,
                        updates_only,
                        *installed,
                    )
                })
                .map(|(entry, _)| entry.id.as_str())
                .collect::<Vec<_>>();
            assert_eq!(
                actual, expected,
                "query={query}, tag={tag:?}, updates_only={updates_only}"
            );
        }
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
