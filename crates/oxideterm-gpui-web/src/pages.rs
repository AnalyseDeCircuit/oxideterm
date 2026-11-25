//! Desktop-parity main workspace pages for Runtime/Dashboard, Plugin Manager,
//! and Cloud Sync in OxideTerm Web.

use std::collections::HashSet;

use crate::host::Workspace;
use gpui::{prelude::*, *};
use oxideterm_gpui_ui::{
    SegmentedControlOptions, SurfaceKind, SurfaceOptions, SurfacePadding,
    badge::{StatusPillOptions, StatusTone, status_pill},
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions, button_with,
        toolbar_button,
    },
    icons::LucideIcon,
    motion::{MotionDuration, animated_chevron, fade_in, slide_fade_in_y},
    page_header, segmented_control, segmented_control_item_content, semantic_surface,
    status_pill_colors,
    switch::switch,
    typography::{MonospaceDatumOptions, MonospaceDatumTone, monospace_datum},
};
use oxideterm_settings::PluginMarketplaceSort;
use oxideterm_workspace::TabKind;

const MONITOR_EMERALD_DARK: u32 = 0x10b981;
const MONITOR_BLUE: u32 = 0x3b82f6;
const MONITOR_AMBER: u32 = 0xf59e0b;
const MONITOR_RED: u32 = 0xef4444;

const PLUGIN_MANAGER_TAB_BAR_WIDTH: f32 = 340.0;
const CLOUD_SYNC_TAB_BAR_WIDTH: f32 = 396.0;
const OFFICIAL_PLUGIN_MARKETPLACE_HOME: &str = "https://github.com/AnalyseDeCircuit/OxideTerm";

const PLUGIN_TAGS: &[(&str, &str)] = &[
    ("language", "plugin.marketplace_languages"),
    ("preview", "plugin.marketplace_previews"),
    ("host-tools", "plugin.marketplace_tools"),
    ("host-sources", "plugin.marketplace_sources"),
    ("workspace", "plugin.marketplace_workspace"),
    ("utilities", "plugin.marketplace_utilities"),
    ("acp", "plugin.marketplace_acp"),
    (
        "remote-connections",
        "plugin.marketplace_remote_connections",
    ),
    ("backgrounds", "plugin.marketplace_backgrounds"),
];

const CLOUD_SYNC_GUIDE_STEP_KEYS: &[&str] = &[
    "plugin.cloud_sync.guide.step_choose_backend",
    "plugin.cloud_sync.guide.step_fill_fields",
    "plugin.cloud_sync.guide.step_save",
    "plugin.cloud_sync.guide.step_check",
    "plugin.cloud_sync.guide.step_upload",
    "plugin.cloud_sync.guide.step_pull",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeSection {
    Overview,
    Topology,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginManagerTab {
    Installed,
    Marketplace,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudSyncTab {
    Overview,
    Configure,
    History,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudSyncBackend {
    Webdav,
    HttpJson,
    Dropbox,
    OneDrive,
    GoogleDrive,
    GithubGist,
    S3,
    Git,
    LocalFile,
}

impl CloudSyncBackend {
    const ALL: [Self; 9] = [
        Self::Webdav,
        Self::HttpJson,
        Self::Dropbox,
        Self::OneDrive,
        Self::GoogleDrive,
        Self::GithubGist,
        Self::S3,
        Self::Git,
        Self::LocalFile,
    ];

    fn label_key(self) -> &'static str {
        match self {
            Self::Webdav => "plugin.cloud_sync.backend.webdav",
            Self::HttpJson => "plugin.cloud_sync.backend.http_json",
            Self::Dropbox => "plugin.cloud_sync.backend.dropbox",
            Self::OneDrive => "plugin.cloud_sync.backend.onedrive",
            Self::GoogleDrive => "plugin.cloud_sync.backend.google_drive",
            Self::GithubGist => "plugin.cloud_sync.backend.github_gist",
            Self::S3 => "plugin.cloud_sync.backend.s3",
            Self::Git => "plugin.cloud_sync.backend.git",
            Self::LocalFile => "plugin.cloud_sync.backend.local_file",
        }
    }

    fn is_local_file(self) -> bool {
        matches!(self, Self::LocalFile)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloudSyncConflictStrategy {
    Merge,
    Replace,
    Skip,
    Rename,
}

impl CloudSyncConflictStrategy {
    const ALL: [Self; 4] = [Self::Merge, Self::Replace, Self::Skip, Self::Rename];

    fn label_key(self) -> &'static str {
        match self {
            Self::Merge => "plugin.cloud_sync.conflict.merge",
            Self::Replace => "plugin.cloud_sync.conflict.replace",
            Self::Skip => "plugin.cloud_sync.conflict.skip",
            Self::Rename => "plugin.cloud_sync.conflict.rename",
        }
    }
}

#[derive(Clone)]
pub struct WebPluginEntry {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub latest_version: &'static str,
    pub author: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
    pub installed: bool,
    pub enabled: bool,
    pub updated_order: u32,
    pub listed_order: u32,
}

#[derive(Clone)]
pub struct CloudSyncHistoryEntry {
    pub action: &'static str,
    pub timestamp: &'static str,
    pub connections: usize,
    pub forwards: usize,
    pub quick_commands: usize,
    pub serial_profiles: usize,
    pub ble_profiles: usize,
    pub telnet_profiles: usize,
    pub sensitive_credentials: usize,
    pub plugin_settings_count: usize,
}

#[derive(Clone)]
pub struct CloudSyncRollbackBackup {
    pub id: u32,
    pub created_at: &'static str,
    pub connections: usize,
    pub forwards: usize,
    pub quick_commands: usize,
    pub serial_profiles: usize,
    pub ble_profiles: usize,
    pub telnet_profiles: usize,
    pub sensitive_credentials: usize,
    pub plugin_settings_count: usize,
    pub size: &'static str,
}

pub struct PagesState {
    pub runtime_section: RuntimeSection,
    pub runtime_previous_section: RuntimeSection,

    // Plugin Manager state
    pub plugin_tab: PluginManagerTab,
    pub plugin_previous_tab: PluginManagerTab,
    pub plugin_installed_tag: Option<&'static str>,
    pub plugin_marketplace_tag: Option<&'static str>,
    pub plugin_marketplace_search: String,
    pub plugin_search_focused: bool,
    pub plugin_marketplace_sort: PluginMarketplaceSort,
    pub plugin_updates_only: bool,
    pub plugin_url_expanded: bool,
    pub plugin_expanded_ids: HashSet<&'static str>,
    pub plugin_status_banner: Option<String>,
    pub plugin_banner_generation: usize,
    pub plugins: Vec<WebPluginEntry>,

    // Cloud Sync state
    pub sync_tab: CloudSyncTab,
    pub sync_previous_tab: CloudSyncTab,
    pub sync_backend: CloudSyncBackend,
    pub sync_conflict: CloudSyncConflictStrategy,
    pub sync_namespace: String,
    pub sync_endpoint: String,
    pub sync_auto_upload: bool,
    pub sync_local_dirty: bool,
    pub sync_last_sync: Option<&'static str>,
    pub sync_connections: bool,
    pub sync_forwards: bool,
    pub sync_quick_commands: bool,
    pub sync_serial_profiles: bool,
    pub sync_ble_profiles: bool,
    pub sync_telnet_profiles: bool,
    pub sync_mosh_profiles: bool,
    pub sync_remote_desktop_profiles: bool,
    pub sync_sensitive_credentials: bool,
    pub sync_app_settings: bool,
    pub sync_plugin_settings: bool,
    pub sync_history: Vec<CloudSyncHistoryEntry>,
    pub sync_backups: Vec<CloudSyncRollbackBackup>,
    pub sync_notice: Option<String>,
}

impl Default for PagesState {
    fn default() -> Self {
        Self {
            runtime_section: RuntimeSection::Overview,
            runtime_previous_section: RuntimeSection::Overview,

            plugin_tab: PluginManagerTab::Installed,
            plugin_previous_tab: PluginManagerTab::Installed,
            plugin_installed_tag: None,
            plugin_marketplace_tag: None,
            plugin_marketplace_search: String::new(),
            plugin_search_focused: false,
            plugin_marketplace_sort: PluginMarketplaceSort::Name,
            plugin_updates_only: false,
            plugin_url_expanded: false,
            plugin_expanded_ids: HashSet::new(),
            plugin_status_banner: None,
            plugin_banner_generation: 0,
            plugins: vec![
                WebPluginEntry {
                    id: "com.oxideterm.ble-uart",
                    name: "Web Bluetooth UART",
                    version: "2.4.1",
                    latest_version: "2.4.1",
                    author: "OxideTerm",
                    category: "remote-connections",
                    summary: "Direct browser Web Bluetooth GATT serial console (NUS / HM-10 / transparent UART).",
                    installed: true,
                    enabled: true,
                    updated_order: 6,
                    listed_order: 6,
                },
                WebPluginEntry {
                    id: "com.oxideterm.host-tools",
                    name: "Host Diagnostics Suite",
                    version: "2.4.0",
                    latest_version: "2.4.1",
                    author: "OxideTerm",
                    category: "host-tools",
                    summary: "Live CPU, memory, processes, GPU, Docker, systemd services, ports, logs, and Redis panels.",
                    installed: true,
                    enabled: true,
                    updated_order: 7,
                    listed_order: 5,
                },
                WebPluginEntry {
                    id: "com.oxideterm.oxidesens",
                    name: "OxideSens Assistant",
                    version: "2.4.1",
                    latest_version: "2.4.1",
                    author: "OxideTerm",
                    category: "workspace",
                    summary: "Context-aware terminal AI assistant supporting OpenAI-compatible and Anthropic providers.",
                    installed: true,
                    enabled: true,
                    updated_order: 5,
                    listed_order: 4,
                },
                WebPluginEntry {
                    id: "com.oxideterm.lang.rust",
                    name: "Rust & TOML Syntax",
                    version: "1.3.0",
                    latest_version: "1.3.0",
                    author: "OxideTerm",
                    category: "language",
                    summary: "Tree-sitter grammar and symbol highlighting for Rust crates and Cargo manifests.",
                    installed: false,
                    enabled: false,
                    updated_order: 4,
                    listed_order: 3,
                },
                WebPluginEntry {
                    id: "com.oxideterm.preview.csv",
                    name: "Table & CSV Inspector",
                    version: "1.1.2",
                    latest_version: "1.1.2",
                    author: "OxideTerm",
                    category: "preview",
                    summary: "Structured column sorting and filtering preview for CSV/TSV files in SFTP.",
                    installed: false,
                    enabled: false,
                    updated_order: 3,
                    listed_order: 2,
                },
                WebPluginEntry {
                    id: "com.oxideterm.remote-desktop.rdp",
                    name: "Remote Desktop (RDP / VNC)",
                    version: "2.1.0",
                    latest_version: "2.1.0",
                    author: "OxideTerm",
                    category: "remote-connections",
                    summary: "Desktop-native RDP and VNC graphical sessions via IronRDP and RFB.",
                    installed: false,
                    enabled: false,
                    updated_order: 2,
                    listed_order: 1,
                },
                WebPluginEntry {
                    id: "com.oxideterm.backgrounds.nord",
                    name: "Atmospheric Backgrounds",
                    version: "1.0.4",
                    latest_version: "1.0.4",
                    author: "OxideTerm",
                    category: "backgrounds",
                    summary: "Curated low-contrast dark and light workspace backdrops.",
                    installed: false,
                    enabled: false,
                    updated_order: 1,
                    listed_order: 7,
                },
            ],

            sync_tab: CloudSyncTab::Overview,
            sync_previous_tab: CloudSyncTab::Overview,
            sync_backend: CloudSyncBackend::Webdav,
            sync_conflict: CloudSyncConflictStrategy::Merge,
            sync_namespace: "oxideterm-default".into(),
            sync_endpoint: "https://dav.example.com/oxideterm".into(),
            sync_auto_upload: false,
            sync_local_dirty: false,
            sync_last_sync: Some("2026-11-25 12:30"),
            sync_connections: true,
            sync_forwards: true,
            sync_quick_commands: true,
            sync_serial_profiles: true,
            sync_ble_profiles: true,
            sync_telnet_profiles: true,
            sync_mosh_profiles: true,
            sync_remote_desktop_profiles: true,
            sync_sensitive_credentials: false,
            sync_app_settings: true,
            sync_plugin_settings: true,
            sync_history: vec![CloudSyncHistoryEntry {
                action: "sync",
                timestamp: "2026-11-25 12:30",
                connections: 3,
                forwards: 1,
                quick_commands: 4,
                serial_profiles: 0,
                ble_profiles: 1,
                telnet_profiles: 0,
                sensitive_credentials: 0,
                plugin_settings_count: 2,
            }],
            sync_backups: vec![CloudSyncRollbackBackup {
                id: 1,
                created_at: "2026-11-25 12:30",
                connections: 3,
                forwards: 1,
                quick_commands: 4,
                serial_profiles: 0,
                ble_profiles: 1,
                telnet_profiles: 0,
                sensitive_credentials: 0,
                plugin_settings_count: 2,
                size: "14.2 KB",
            }],
            sync_notice: None,
        }
    }
}

fn plugin_marketplace_sort_label(sort: PluginMarketplaceSort) -> &'static str {
    match sort {
        PluginMarketplaceSort::Name => "plugin.marketplace_sort_name",
        PluginMarketplaceSort::RecentUpdates => "plugin.marketplace_sort_updated",
        PluginMarketplaceSort::NewestListings => "plugin.marketplace_sort_newest",
    }
}

fn glyph(icon: LucideIcon, size: f32, color: u32) -> AnyElement {
    svg()
        .path(icon.path())
        .size(px(size))
        .text_color(rgb(color))
        .into_any_element()
}

impl Workspace {
    fn page_card(&self) -> Div {
        semantic_surface(
            &self.tokens,
            SurfaceOptions::new(SurfaceKind::Inspector)
                .padding(SurfacePadding::None)
                .has_background_image(false),
        )
    }

    fn spacious_card(&self) -> Div {
        semantic_surface(
            &self.tokens,
            SurfaceOptions::new(SurfaceKind::Inspector)
                .padding(SurfacePadding::Spacious)
                .has_background_image(false),
        )
        .w_full()
        .min_w(px(0.0))
        .flex()
        .flex_col()
        .gap(px(14.0))
    }

    fn page_header_bar(
        &self,
        title: String,
        description: Option<String>,
        switcher: AnyElement,
    ) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        div()
            .flex_none()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(t.metrics.settings_page_gap))
            .px(px(t.metrics.settings_content_padding))
            .pt(px(t.metrics.settings_content_padding))
            .pb(px(t.metrics.settings_page_gap))
            .bg(rgb(theme.bg))
            .child(page_header(&t, title, description, Some(switcher)))
            .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
            .into_any_element()
    }

    // -------------------------------------------------------------------------
    // Runtime / Dashboard (TabKind::Runtime)
    // -------------------------------------------------------------------------
    pub(crate) fn render_runtime_page(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let active_section = self.pages.runtime_section;

        let overview_label = self.i18n.t("sidebar.panels.runtime_overview");
        let matrix_label = self.i18n.t("sidebar.panels.connection_matrix");
        let tab_width = oxideterm_gpui_ui::estimate_segmented_control_width(
            &t,
            320.0,
            520.0,
            true,
            [
                (overview_label.as_str(), None),
                (matrix_label.as_str(), None),
            ],
        );

        let make_tab = |section: RuntimeSection, label: String, icon: LucideIcon| {
            let active = active_section == section;
            let content = div()
                .w_full()
                .py(px(2.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(8.0))
                .child(glyph(
                    icon,
                    16.0,
                    if active {
                        theme.accent
                    } else {
                        theme.text_muted
                    },
                ))
                .child(label);
            segmented_control_item_content(&t, active, content.into_any_element())
                .font_weight(FontWeight::MEDIUM)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |h, _, _, cx| {
                        if h.pages.runtime_section != section {
                            let prev_idx =
                                usize::from(h.pages.runtime_section == RuntimeSection::Topology);
                            let next_idx = usize::from(section == RuntimeSection::Topology);
                            h.pages.runtime_previous_section = h.pages.runtime_section;
                            h.pages.runtime_section = section;
                            h.begin_segmented_transition(
                                "web-runtime-switcher",
                                prev_idx,
                                next_idx,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .into_any_element()
        };

        let active_idx = usize::from(active_section == RuntimeSection::Topology);
        let prev_idx = self
            .segmented_motion
            .previous_index_for("web-runtime-switcher", active_idx)
            .unwrap_or_else(|| {
                usize::from(self.pages.runtime_previous_section == RuntimeSection::Topology)
            });
        let switcher = segmented_control(
            &t,
            "web-runtime-switcher",
            SegmentedControlOptions::new(active_idx, prev_idx, 2)
                .user_transition_active(
                    self.segmented_motion
                        .is_active_for("web-runtime-switcher", active_idx),
                )
                .compact(tab_width),
            vec![
                make_tab(
                    RuntimeSection::Overview,
                    overview_label,
                    LucideIcon::LayoutList,
                ),
                make_tab(RuntimeSection::Topology, matrix_label, LucideIcon::Network),
            ],
        )
        .into_any_element();

        let header = self.page_header_bar(
            self.i18n.t("sidebar.panels.runtime"),
            Some(self.i18n.t("sidebar.panels.runtime_description")),
            switcher,
        );

        let (stats, summaries, _) = self
            .tools
            .monitor
            .as_ref()
            .map(|m| m.read(cx).pool_snapshot(cx))
            .unwrap_or((None, vec![], None));
        let mut stats = stats.unwrap_or_default();
        let ble_active = self.ble_links.len();
        stats.total_connections = stats.total_connections.max(self.sessions.len());
        stats.active_connections = stats
            .active_connections
            .max(ble_active + usize::from(self.ready));
        stats.total_terminals = self.sessions.len();

        let body = match active_section {
            RuntimeSection::Overview => {
                let metric = |label: String, value: usize, icon: LucideIcon, color: u32| {
                    div()
                        .min_w(px(104.0))
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_1()
                                .text_size(px(t.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(glyph(icon, 12.0, color))
                                .child(label),
                        )
                        .child(
                            monospace_datum(
                                &t,
                                value.to_string(),
                                Some("Lilex".into()),
                                MonospaceDatumOptions::new(MonospaceDatumTone::Primary)
                                    .text_size(20.0)
                                    .strong(),
                            )
                            .text_color(rgb(color)),
                        )
                };

                let pool_panel = self
                    .page_card()
                    .w(relative(0.64))
                    .min_w(px(360.0))
                    .flex_grow_1()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .text_size(px(t.metrics.ui_text_sm))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(glyph(LucideIcon::Gauge, 15.0, theme.text_muted))
                                    .child(self.i18n.t("connections.monitor.overview_pool_usage")),
                            )
                            .child(
                                div()
                                    .text_size(px(t.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("connections.monitor.idle_timeout_never")),
                            ),
                    )
                    .child(
                        monospace_datum(
                            &t,
                            stats.total_connections.to_string(),
                            Some("Lilex".into()),
                            MonospaceDatumOptions::new(MonospaceDatumTone::Primary)
                                .text_size(30.0)
                                .strong(),
                        )
                        .text_color(rgb(theme.text_heading)),
                    )
                    .child(
                        div()
                            .w_full()
                            .pt_3()
                            .border_t_1()
                            .border_color(rgba((theme.border << 8) | 0x80))
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .child(metric(
                                self.i18n.t("connections.monitor.active"),
                                stats.active_connections,
                                LucideIcon::Activity,
                                if stats.active_connections > 0 {
                                    MONITOR_EMERALD_DARK
                                } else {
                                    theme.text_muted
                                },
                            ))
                            .child(metric(
                                self.i18n.t("connections.monitor.idle"),
                                stats.idle_connections,
                                LucideIcon::Link2,
                                if stats.idle_connections > 0 {
                                    MONITOR_BLUE
                                } else {
                                    theme.text_muted
                                },
                            ))
                            .child(metric(
                                self.i18n.t("connections.monitor.reconnecting"),
                                stats.reconnecting_connections,
                                LucideIcon::RefreshCw,
                                if stats.reconnecting_connections > 0 {
                                    MONITOR_AMBER
                                } else {
                                    theme.text_muted
                                },
                            ))
                            .child(metric(
                                self.i18n.t("connections.monitor.link_down"),
                                stats.link_down_connections,
                                LucideIcon::AlertTriangle,
                                if stats.link_down_connections > 0 {
                                    MONITOR_RED
                                } else {
                                    theme.text_muted
                                },
                            )),
                    );

                let consumer_row = |icon: LucideIcon, label: String, count: usize, first: bool| {
                    div()
                        .w_full()
                        .py_2()
                        .when(!first, |r| {
                            r.border_t_1()
                                .border_color(rgba((theme.border << 8) | 0x45))
                        })
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_size(px(t.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_secondary))
                                .child(glyph(icon, 13.0, theme.text_muted))
                                .child(label),
                        )
                        .child(monospace_datum(
                            &t,
                            count.to_string(),
                            Some("Lilex".into()),
                            MonospaceDatumOptions::new(MonospaceDatumTone::Primary)
                                .text_size(t.metrics.ui_text_sm)
                                .strong(),
                        ))
                };

                let sftp_open = usize::from(
                    self.layout
                        .tabs()
                        .iter()
                        .any(|tab| tab.kind == TabKind::Sftp),
                );
                let ide_open = usize::from(
                    self.layout
                        .tabs()
                        .iter()
                        .any(|tab| tab.kind == TabKind::Ide),
                );
                let consumers_panel = self
                    .page_card()
                    .w(relative(0.32))
                    .min_w(px(240.0))
                    .flex_grow_1()
                    .p_4()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .mb_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(t.metrics.ui_text_sm))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(glyph(LucideIcon::AppWindow, 15.0, theme.text_muted))
                            .child(self.i18n.t("connections.monitor.overview_consumers")),
                    )
                    .child(consumer_row(
                        LucideIcon::Terminal,
                        self.i18n.t("connections.monitor.terminals"),
                        stats.total_terminals,
                        true,
                    ))
                    .child(consumer_row(
                        LucideIcon::FolderOpen,
                        self.i18n.t("sidebar.panels.sftp"),
                        sftp_open,
                        false,
                    ))
                    .child(consumer_row(
                        LucideIcon::Code2,
                        self.i18n.t("connections.monitor.ide"),
                        ide_open,
                        false,
                    ))
                    .child(consumer_row(
                        LucideIcon::Bluetooth,
                        self.i18n.t("modals.new_connection.transport_ble"),
                        ble_active,
                        false,
                    ));

                let mut summary_rows = div().w_full().flex().flex_col();
                for (i, entry) in summaries.iter().enumerate() {
                    summary_rows = summary_rows.child(
                        div()
                            .w_full()
                            .px_4()
                            .py_3()
                            .when(i > 0, |r| {
                                r.border_t_1()
                                    .border_color(rgba((theme.border << 8) | 0x45))
                            })
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_3()
                                    .child(
                                        div()
                                            .size(px(8.0))
                                            .rounded_full()
                                            .bg(rgb(MONITOR_EMERALD_DARK)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_size(px(t.metrics.ui_text_sm))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(theme.text))
                                                    .child(format!(
                                                        "{}@{}:{}",
                                                        entry.username, entry.host, entry.port
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(t.metrics.ui_text_xs))
                                                    .text_color(rgb(theme.text_muted))
                                                    .child(format!(
                                                        "{}: {} · {}: {}",
                                                        self.i18n
                                                            .t("connections.monitor.terminals"),
                                                        entry.terminal_count,
                                                        self.i18n.t("sidebar.panels.forwards"),
                                                        entry.forward_count
                                                    )),
                                            ),
                                    ),
                            )
                            .child(status_pill(
                                &t,
                                self.i18n.t("connections.monitor.active"),
                                StatusPillOptions::new(StatusTone::Success).compact(),
                            )),
                    );
                }

                div()
                    .id("web-runtime-overview")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .px(px(t.metrics.settings_content_padding))
                            .py(px(20.0))
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .flex_wrap()
                                    .items_stretch()
                                    .gap_3()
                                    .child(pool_panel)
                                    .child(consumers_panel),
                            )
                            .child(self.page_card().overflow_hidden().child(summary_rows)),
                    )
                    .into_any_element()
            }
            RuntimeSection::Topology => {
                let mut cards = div().w_full().flex().flex_wrap().gap_3();
                for (sid, session) in &self.sessions {
                    let is_ble = crate::ble::is_ble_node(&session.node);
                    let is_local = session.node == "local";
                    let icon = if is_ble {
                        LucideIcon::Bluetooth
                    } else if is_local {
                        LucideIcon::Terminal
                    } else {
                        LucideIcon::Server
                    };
                    let badge_color = if is_ble {
                        theme.warning
                    } else if is_local {
                        theme.accent
                    } else {
                        theme.info
                    };
                    let node_label = self
                        .ble_profiles
                        .get(&session.node)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| session.node.clone());
                    let node_id = session.node.clone();
                    cards = cards.child(
                        self.page_card()
                            .min_w(px(260.0))
                            .flex_1()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .size(px(28.0))
                                                    .rounded(px(t.radii.sm))
                                                    .bg(rgba((badge_color << 8) | 0x26))
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .child(glyph(icon, 14.0, badge_color)),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(t.metrics.ui_text_sm))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(theme.text))
                                                    .child(node_label),
                                            ),
                                    )
                                    .child(status_pill(
                                        &t,
                                        self.i18n.t("connections.monitor.active"),
                                        StatusPillOptions::new(StatusTone::Success).compact(),
                                    )),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .pt_2()
                                    .border_t_1()
                                    .border_color(rgba((theme.border << 8) | 0x45))
                                    .child(
                                        div()
                                            .font_family("Lilex")
                                            .text_size(px(t.metrics.ui_text_2xs))
                                            .text_color(rgb(theme.text_muted))
                                            .child(format!("session #{}", sid.0)),
                                    )
                                    .child(
                                        button_with(
                                            &t,
                                            self.i18n.t("sessions.tree.actions.disconnect"),
                                            ButtonOptions {
                                                variant: ButtonVariant::Ghost,
                                                size: ButtonSize::Sm,
                                                radius: ButtonRadius::Sm,
                                                disabled: false,
                                            },
                                        )
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |h, _, _, cx| {
                                                h.disconnect_node(&node_id, cx);
                                                cx.stop_propagation();
                                            }),
                                        ),
                                    ),
                            ),
                    );
                }
                div()
                    .id("web-runtime-topology")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(t.metrics.settings_content_padding))
                    .py(px(20.0))
                    .child(cards)
                    .into_any_element()
            }
        };

        div()
            .id("web-runtime-page")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(header)
            .child(fade_in(
                &t,
                ("web-runtime-body", active_idx),
                div().flex_1().min_h_0().flex().flex_col().child(body),
                MotionDuration::Micro,
            ))
            .into_any_element()
    }

    // -------------------------------------------------------------------------
    // Plugin Manager (TabKind::PluginManager) — Desktop 5-section parity
    // -------------------------------------------------------------------------
    fn plugin_action_button(
        &self,
        icon: LucideIcon,
        label: String,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        toolbar_button(
            &t,
            label,
            Some(glyph(icon, 14.0, theme.text_muted)),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(MouseButton::Left, listener)
        .into_any_element()
    }

    fn plugin_tag_pill(
        &self,
        label: String,
        active: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        div()
            .cursor_pointer()
            .px(px(10.0))
            .py(px(4.0))
            .rounded(px(t.radii.sm))
            .border_1()
            .border_color(if active {
                rgb(theme.accent)
            } else {
                rgba((theme.border << 8) | 0x80)
            })
            .bg(if active {
                rgba((theme.accent << 8) | 0x1a)
            } else {
                rgb(theme.bg_panel)
            })
            .text_size(px(t.metrics.ui_text_xs))
            .font_weight(if active {
                FontWeight::MEDIUM
            } else {
                FontWeight::NORMAL
            })
            .text_color(rgb(if active {
                theme.accent
            } else {
                theme.text_secondary
            }))
            .hover(|s| {
                s.bg(if active {
                    rgba((theme.accent << 8) | 0x26)
                } else {
                    rgb(theme.bg_hover)
                })
            })
            .child(label)
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_plugin_tag_filter(&self, tab: PluginManagerTab, cx: &Context<Self>) -> AnyElement {
        let active_tag = match tab {
            PluginManagerTab::Installed => self.pages.plugin_installed_tag,
            PluginManagerTab::Marketplace => self.pages.plugin_marketplace_tag,
        };
        let mut row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .child(self.plugin_tag_pill(
                self.i18n.t("plugin.marketplace_all"),
                active_tag.is_none(),
                cx.listener(move |h, _, _, cx| {
                    match tab {
                        PluginManagerTab::Installed => h.pages.plugin_installed_tag = None,
                        PluginManagerTab::Marketplace => h.pages.plugin_marketplace_tag = None,
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        for &(tag, label_key) in PLUGIN_TAGS {
            row = row.child(self.plugin_tag_pill(
                self.i18n.t(label_key),
                active_tag == Some(tag),
                cx.listener(move |h, _, _, cx| {
                    match tab {
                        PluginManagerTab::Installed => h.pages.plugin_installed_tag = Some(tag),
                        PluginManagerTab::Marketplace => h.pages.plugin_marketplace_tag = Some(tag),
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }
        row.into_any_element()
    }

    pub(crate) fn render_plugin_manager_page(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let active_tab = self.pages.plugin_tab;
        let installed_count = self.pages.plugins.iter().filter(|p| p.installed).count();
        let active_count = self
            .pages
            .plugins
            .iter()
            .filter(|p| p.installed && p.enabled)
            .count();
        let update_count = self
            .pages
            .plugins
            .iter()
            .filter(|p| p.installed && p.version != p.latest_version)
            .count();

        let installed_label = self.i18n.t("plugin.tab_installed");
        let installed_badge = installed_count.to_string();
        let marketplace_label = self.i18n.t("plugin.tab_marketplace");
        let marketplace_badge =
            (update_count > 0).then(|| format!("{update_count} {}", self.i18n.t("plugin.updates")));
        let tab_width = oxideterm_gpui_ui::estimate_segmented_control_width(
            &t,
            PLUGIN_MANAGER_TAB_BAR_WIDTH,
            540.0,
            true,
            [
                (installed_label.as_str(), Some(installed_badge.as_str())),
                (marketplace_label.as_str(), marketplace_badge.as_deref()),
            ],
        );

        let make_tab =
            |tab: PluginManagerTab, icon: LucideIcon, label: String, badge: Option<String>| {
                let active = active_tab == tab;
                let content = div()
                    .w_full()
                    .py(px(2.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(8.0))
                    .child(glyph(
                        icon,
                        16.0,
                        if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        },
                    ))
                    .child(label)
                    .when_some(badge, |c, badge| {
                        c.child(
                            div()
                                .ml(px(4.0))
                                .rounded(px(t.radii.sm))
                                .border_1()
                                .border_color(if active {
                                    rgb(theme.accent)
                                } else {
                                    rgba((theme.border << 8) | 0x80)
                                })
                                .bg(if active {
                                    rgba((theme.accent << 8) | 0x1a)
                                } else {
                                    rgb(theme.bg_panel)
                                })
                                .px(px(6.0))
                                .py(px(2.0))
                                .text_size(px(t.metrics.ui_text_xs))
                                .text_color(rgb(if active {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                }))
                                .child(badge),
                        )
                    });
                segmented_control_item_content(&t, active, content.into_any_element())
                    .font_weight(FontWeight::MEDIUM)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |h, _, _, cx| {
                            if h.pages.plugin_tab != tab {
                                let prev_idx = usize::from(
                                    h.pages.plugin_tab == PluginManagerTab::Marketplace,
                                );
                                let next_idx = usize::from(tab == PluginManagerTab::Marketplace);
                                h.pages.plugin_previous_tab = h.pages.plugin_tab;
                                h.pages.plugin_tab = tab;
                                h.begin_segmented_transition(
                                    "web-plugin-switcher",
                                    prev_idx,
                                    next_idx,
                                    cx,
                                );
                            }
                            if tab != PluginManagerTab::Marketplace {
                                h.pages.plugin_search_focused = false;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .into_any_element()
            };

        let active_idx = usize::from(active_tab == PluginManagerTab::Marketplace);
        let prev_idx = self
            .segmented_motion
            .previous_index_for("web-plugin-switcher", active_idx)
            .unwrap_or_else(|| {
                usize::from(self.pages.plugin_previous_tab == PluginManagerTab::Marketplace)
            });
        let switcher = segmented_control(
            &t,
            "web-plugin-switcher",
            SegmentedControlOptions::new(active_idx, prev_idx, 2)
                .user_transition_active(
                    self.segmented_motion
                        .is_active_for("web-plugin-switcher", active_idx),
                )
                .compact(tab_width),
            vec![
                make_tab(
                    PluginManagerTab::Installed,
                    LucideIcon::Puzzle,
                    installed_label,
                    Some(installed_badge),
                ),
                make_tab(
                    PluginManagerTab::Marketplace,
                    LucideIcon::Network,
                    marketplace_label,
                    marketplace_badge,
                ),
            ],
        )
        .into_any_element();

        let header = self.page_header_bar(
            self.i18n.t("plugin.manager_title"),
            Some(self.i18n.t(match active_tab {
                PluginManagerTab::Installed => "plugin.manager_description",
                PluginManagerTab::Marketplace => "plugin.marketplace_description",
            })),
            switcher,
        );

        // Actions bar (exact desktop plugin_manager.rs:670-829)
        let actions_bar = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(t.spacing.three))
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
                            .text_size(px(t.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(glyph(LucideIcon::Puzzle, 16.0, theme.accent))
                                    .child(
                                        self.i18n
                                            .t("plugin.footer")
                                            .replace("{{count}}", &installed_count.to_string()),
                                    ),
                            )
                            .child(div().child("·"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(glyph(LucideIcon::CheckCircle, 14.0, theme.success))
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
                            .child(self.plugin_action_button(
                                LucideIcon::FolderOpen,
                                self.i18n.t("plugin.open_plugins_dir"),
                                cx.listener(|h, _, _, cx| {
                                    h.pages.plugin_banner_generation =
                                        h.pages.plugin_banner_generation.wrapping_add(1);
                                    h.pages.plugin_status_banner =
                                        Some(h.i18n.t("plugin.open_plugins_dir"));
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .when(active_tab == PluginManagerTab::Installed, |bar| {
                                bar.child(self.plugin_action_button(
                                    LucideIcon::RefreshCw,
                                    self.i18n.t("plugin.refresh"),
                                    cx.listener(|h, _, _, cx| {
                                        h.pages.plugin_banner_generation =
                                            h.pages.plugin_banner_generation.wrapping_add(1);
                                        h.pages.plugin_status_banner =
                                            Some(h.i18n.t("plugin.refresh"));
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ))
                            })
                            .when(active_tab == PluginManagerTab::Marketplace, |bar| {
                                bar.child(self.plugin_action_button(
                                    LucideIcon::ExternalLink,
                                    self.i18n.t("plugin.marketplace_repository"),
                                    |_, _, cx| {
                                        cx.open_url(OFFICIAL_PLUGIN_MARKETPLACE_HOME);
                                        cx.stop_propagation();
                                    },
                                ))
                                .child(self.plugin_action_button(
                                    LucideIcon::RefreshCw,
                                    self.i18n.t("plugin.refresh"),
                                    cx.listener(|h, _, _, cx| {
                                        h.pages.plugin_banner_generation =
                                            h.pages.plugin_banner_generation.wrapping_add(1);
                                        h.pages.plugin_status_banner =
                                            Some(h.i18n.t("plugin.refresh"));
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ))
                            }),
                    ),
            )
            .when_some(self.pages.plugin_status_banner.as_ref(), |col, msg| {
                col.child(slide_fade_in_y(
                    &t,
                    ("web-plugin-banner", self.pages.plugin_banner_generation),
                    div()
                        .w_full()
                        .px(px(12.0))
                        .py(px(8.0))
                        .rounded(px(t.radii.sm))
                        .border_1()
                        .border_color(rgba((theme.success << 8) | 0x4d))
                        .bg(rgba((theme.success << 8) | 0x16))
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(t.metrics.ui_text_xs))
                                .text_color(rgb(theme.success))
                                .child(msg.clone()),
                        )
                        .child(
                            div()
                                .cursor_pointer()
                                .child(glyph(LucideIcon::X, 13.0, theme.text_muted))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|h, _, _, cx| {
                                        h.pages.plugin_status_banner = None;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        ),
                    -6.0,
                    MotionDuration::Micro,
                ))
            });

        let tabbed_content = fade_in(
            &t,
            ("web-plugin-body", active_idx),
            div().w_full().min_w(px(0.0)).child(match active_tab {
                PluginManagerTab::Installed => self.render_plugin_installed_section(cx),
                PluginManagerTab::Marketplace => self.render_plugin_marketplace_section(cx),
            }),
            MotionDuration::Micro,
        );

        // Bottom compatibility notice card (exact desktop plugin_manager.rs:625-656)
        let compat_card = semantic_surface(
            &t,
            SurfaceOptions::new(SurfaceKind::Inspector)
                .padding(SurfacePadding::Spacious)
                .has_background_image(false),
        )
        .w_full()
        .min_w(px(0.0))
        .flex()
        .items_start()
        .gap(px(12.0))
        .child(
            div()
                .flex_shrink_0()
                .pt(px(2.0))
                .child(glyph(LucideIcon::Info, 18.0, theme.accent)),
        )
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(px(t.metrics.ui_text_sm))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(rgb(theme.text_heading))
                        .child(self.i18n.t("plugin.compatibility_title")),
                )
                .child(
                    div()
                        .text_size(px(t.metrics.ui_text_xs))
                        .line_height(px(18.0))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("plugin.manager_compatibility_notice")),
                ),
        );

        div()
            .id("web-plugin-manager-page")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(header)
            .child(
                div()
                    .id("web-plugin-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(t.metrics.settings_content_padding))
                    .py(px(t.metrics.settings_page_gap))
                    .flex()
                    .flex_col()
                    .gap(px(t.metrics.settings_page_gap))
                    .child(actions_bar)
                    .child(tabbed_content)
                    .child(compat_card),
            )
            .into_any_element()
    }

    fn render_plugin_installed_section(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let tag_filter = self.pages.plugin_installed_tag;
        let matching: Vec<(usize, &WebPluginEntry)> = self
            .pages
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, p)| p.installed && tag_filter.is_none_or(|tag| p.category == tag))
            .collect();

        let mut section = div()
            .w_full()
            .min_w(px(0.0))
            .min_h(px(240.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(self.render_plugin_tag_filter(PluginManagerTab::Installed, cx));

        if matching.is_empty() {
            return section
                .child(
                    div()
                        .min_h(px(180.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(10.0))
                        .child(glyph(LucideIcon::Puzzle, 36.0, theme.text_muted))
                        .child(
                            div()
                                .text_size(px(t.metrics.ui_text_base))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(theme.text))
                                .child(self.i18n.t(if tag_filter.is_some() {
                                    "plugin.no_search_results"
                                } else {
                                    "plugin.empty_title"
                                })),
                        )
                        .when(tag_filter.is_none(), |empty| {
                            empty.child(
                                div()
                                    .max_w(px(560.0))
                                    .text_center()
                                    .text_size(px(t.metrics.ui_text_sm))
                                    .line_height(px(20.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("plugin.empty_description")),
                            )
                        }),
                )
                .into_any_element();
        }

        let mut list = div().w_full().min_w(px(0.0)).flex().flex_col();
        for (row_idx, (idx, plugin)) in matching.into_iter().enumerate() {
            let expanded = self.pages.plugin_expanded_ids.contains(plugin.id);
            let is_bg_pack = plugin.category == "backgrounds";
            let state_tone = if plugin.enabled {
                StatusTone::Success
            } else {
                StatusTone::Neutral
            };
            let state_label = self.i18n.t(if plugin.enabled {
                "plugin.status.active"
            } else {
                "plugin.status.disabled"
            });
            let pid = plugin.id;
            let pname = plugin.name;

            let mut row = div()
                .w_full()
                .min_w(px(0.0))
                .py(px(14.0))
                .when(row_idx > 0, |r| {
                    r.border_t_1()
                        .border_color(rgba((theme.border << 8) | 0x45))
                })
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_start()
                        .gap(px(t.spacing.three))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .overflow_hidden()
                                .flex()
                                .items_start()
                                .gap(px(t.spacing.three))
                                .child(div().flex_shrink_0().pt(px(2.0)).child(glyph(
                                    if is_bg_pack {
                                        LucideIcon::Image
                                    } else {
                                        LucideIcon::Puzzle
                                    },
                                    t.metrics.ui_menu_icon_size,
                                    theme.text_muted,
                                )))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .overflow_hidden()
                                        .flex()
                                        .flex_col()
                                        .gap(px(t.spacing.one))
                                        .child(
                                            div()
                                                .flex()
                                                .flex_wrap()
                                                .items_center()
                                                .gap(px(t.spacing.two))
                                                .child(
                                                    div()
                                                        .min_w(px(0.0))
                                                        .truncate()
                                                        .text_size(px(t.metrics.ui_text_sm))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(rgb(theme.text))
                                                        .child(plugin.name),
                                                )
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .text_size(px(t.metrics.ui_text_xs))
                                                        .text_color(rgb(theme.text_muted))
                                                        .child(format!("v{}", plugin.version)),
                                                )
                                                .child(
                                                    div()
                                                        .flex_none()
                                                        .text_size(px(t.metrics.ui_text_xs))
                                                        .text_color(
                                                            status_pill_colors(
                                                                &t, state_tone, false,
                                                            )
                                                            .text,
                                                        )
                                                        .child(state_label),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .min_w(px(0.0))
                                                .text_size(px(t.metrics.ui_text_xs))
                                                .line_height(px(18.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(plugin.summary),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .flex_shrink_0()
                                .flex()
                                .items_center()
                                .justify_end()
                                .gap(px(t.spacing.two))
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .size(px(28.0))
                                        .rounded(px(t.radii.sm))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(|s| s.bg(rgb(theme.bg_hover)))
                                        .child(animated_chevron(
                                            &t,
                                            ("web-plugin-chevron", idx),
                                            svg()
                                                .path(LucideIcon::ChevronRight.path())
                                                .size(px(t.metrics.ui_menu_icon_size))
                                                .text_color(rgb(theme.text_muted)),
                                            expanded,
                                        ))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |h, _, _, cx| {
                                                let now_expanded =
                                                    if h.pages.plugin_expanded_ids.insert(pid) {
                                                        true
                                                    } else {
                                                        h.pages.plugin_expanded_ids.remove(pid);
                                                        false
                                                    };
                                                h.begin_disclosure(
                                                    format!("plugin-details:{pid}"),
                                                    now_expanded,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                        ),
                                )
                                .when(!is_bg_pack && plugin.enabled, |actions| {
                                    actions.child(
                                        div()
                                            .cursor_pointer()
                                            .size(px(28.0))
                                            .rounded(px(t.radii.sm))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|s| s.bg(rgb(theme.bg_hover)))
                                            .child(glyph(
                                                LucideIcon::RefreshCw,
                                                14.0,
                                                theme.text_muted,
                                            ))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |h, _, _, cx| {
                                                    let msg = h
                                                        .i18n
                                                        .t("plugin.reload_success")
                                                        .replace("{{name}}", pname);
                                                    h.pages.plugin_banner_generation = h
                                                        .pages
                                                        .plugin_banner_generation
                                                        .wrapping_add(1);
                                                    h.pages.plugin_status_banner = Some(msg);
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }),
                                            ),
                                    )
                                })
                                .child(
                                    switch(&t, ("web-plugin-switch", idx), "", plugin.enabled)
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |h, _, _, cx| {
                                                if let Some(item) = h.pages.plugins.get_mut(idx) {
                                                    item.enabled = !item.enabled;
                                                    let key = if item.enabled {
                                                        "plugin.enable_success"
                                                    } else {
                                                        "plugin.disable_success"
                                                    };
                                                    h.pages.plugin_banner_generation = h
                                                        .pages
                                                        .plugin_banner_generation
                                                        .wrapping_add(1);
                                                    h.pages.plugin_status_banner = Some(
                                                        h.i18n
                                                            .t(key)
                                                            .replace("{{name}}", item.name),
                                                    );
                                                }
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .size(px(28.0))
                                        .rounded(px(t.radii.sm))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .hover(|s| s.bg(rgba((theme.error << 8) | 0x1a)))
                                        .child(glyph(LucideIcon::Trash2, 14.0, theme.text_muted))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |h, _, _, cx| {
                                                if let Some(item) = h.pages.plugins.get_mut(idx) {
                                                    item.installed = false;
                                                    item.enabled = false;
                                                }
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                        ),
                                ),
                        ),
                );

            let disc_key = format!("plugin-details:{pid}");
            if self.disclosure_motions.retained(&disc_key, expanded) {
                let details = div()
                    .ml(px(28.0))
                    .p(px(12.0))
                    .rounded(px(t.radii.sm))
                    .bg(rgb(theme.bg_panel))
                    .border_1()
                    .border_color(rgba((theme.border << 8) | 0x60))
                    .flex()
                    .flex_wrap()
                    .gap_x(px(24.0))
                    .gap_y(px(8.0))
                    .text_size(px(t.metrics.ui_text_xs))
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("plugin.create_plugin_id")),
                            )
                            .child(
                                div()
                                    .font_family("Lilex")
                                    .text_color(rgb(theme.text))
                                    .child(plugin.id),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("plugin.detail_author")),
                            )
                            .child(div().text_color(rgb(theme.text)).child(plugin.author)),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("plugin.detail_permissions")),
                            )
                            .child(
                                div()
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("plugin.permission_none")),
                            ),
                    );
                row = row.child(self.disclosure_motions.render(&disc_key, &t, details, None));
            }
            list = list.child(row);
        }

        section = section.child(list);
        section.into_any_element()
    }

    fn render_plugin_marketplace_section(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let query = self.pages.plugin_marketplace_search.trim().to_lowercase();
        let tag_filter = self.pages.plugin_marketplace_tag;
        let updates_only = self.pages.plugin_updates_only;
        let sort = self.pages.plugin_marketplace_sort;

        let mut visible: Vec<(usize, &WebPluginEntry)> = self
            .pages
            .plugins
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                if let Some(tag) = tag_filter
                    && p.category != tag
                {
                    return false;
                }
                if updates_only && !(p.installed && p.version != p.latest_version) {
                    return false;
                }
                if !query.is_empty() {
                    let hit = p.name.to_lowercase().contains(&query)
                        || p.summary.to_lowercase().contains(&query)
                        || p.id.to_lowercase().contains(&query)
                        || p.category.to_lowercase().contains(&query);
                    if !hit {
                        return false;
                    }
                }
                true
            })
            .collect();

        match sort {
            PluginMarketplaceSort::Name => visible.sort_by(|a, b| a.1.name.cmp(b.1.name)),
            PluginMarketplaceSort::RecentUpdates => {
                visible.sort_by(|a, b| b.1.updated_order.cmp(&a.1.updated_order))
            }
            PluginMarketplaceSort::NewestListings => {
                visible.sort_by(|a, b| b.1.listed_order.cmp(&a.1.listed_order))
            }
        }

        let sort_label = self.i18n.t("plugin.marketplace_sort").replace(
            "{{order}}",
            &self.i18n.t(plugin_marketplace_sort_label(sort)),
        );

        let search_focused = self.pages.plugin_search_focused;
        let search_box = div()
            .min_w(px(200.0))
            .flex_1()
            .h(px(34.0))
            .px(px(10.0))
            .rounded(px(t.radii.sm))
            .border_1()
            .border_color(if search_focused {
                rgb(theme.accent)
            } else {
                rgb(theme.border)
            })
            .bg(rgb(theme.bg_panel))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(glyph(LucideIcon::Search, 14.0, theme.text_muted))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(t.metrics.ui_text_sm))
                    .text_color(rgb(if self.pages.plugin_marketplace_search.is_empty() {
                        theme.text_muted
                    } else {
                        theme.text
                    }))
                    .child(if self.pages.plugin_marketplace_search.is_empty() {
                        self.i18n.t("plugin.search_placeholder")
                    } else {
                        self.pages.plugin_marketplace_search.clone()
                    }),
            )
            .when(!self.pages.plugin_marketplace_search.is_empty(), |box_el| {
                box_el.child(
                    div()
                        .cursor_pointer()
                        .child(glyph(LucideIcon::X, 13.0, theme.text_muted))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|h, _, _, cx| {
                                h.pages.plugin_marketplace_search.clear();
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|h, _, w, cx| {
                    h.pages.plugin_search_focused = true;
                    h.sidebar_search_focused = false;
                    h.knowledge.active_input = None;
                    w.focus(&h.focus, cx);
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        let sort_button = div()
            .cursor_pointer()
            .h(px(34.0))
            .px(px(12.0))
            .rounded(px(t.radii.sm))
            .border_1()
            .border_color(rgb(theme.border))
            .bg(rgb(theme.bg_panel))
            .hover(|s| s.bg(rgb(theme.bg_hover)))
            .flex()
            .items_center()
            .gap(px(8.0))
            .text_size(px(t.metrics.ui_text_xs))
            .text_color(rgb(theme.text))
            .child(sort_label)
            .child(glyph(LucideIcon::ChevronDown, 13.0, theme.text_muted))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|h, _, _, cx| {
                    h.pages.plugin_marketplace_sort = match h.pages.plugin_marketplace_sort {
                        PluginMarketplaceSort::Name => PluginMarketplaceSort::RecentUpdates,
                        PluginMarketplaceSort::RecentUpdates => {
                            PluginMarketplaceSort::NewestListings
                        }
                        PluginMarketplaceSort::NewestListings => PluginMarketplaceSort::Name,
                    };
                    cx.stop_propagation();
                    cx.notify();
                }),
            );

        let mut card = div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(t.spacing.two))
                    .child(search_box)
                    .child(sort_button),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(px(t.spacing.two))
                    .child(self.render_plugin_tag_filter(PluginManagerTab::Marketplace, cx))
                    .child(self.plugin_tag_pill(
                        self.i18n.t("plugin.marketplace_updates_only"),
                        updates_only,
                        cx.listener(|h, _, _, cx| {
                            h.pages.plugin_updates_only = !h.pages.plugin_updates_only;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )),
            );

        if visible.is_empty() {
            card = card.child(
                div()
                    .min_h(px(150.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(10.0))
                    .text_size(px(t.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(glyph(LucideIcon::Search, 24.0, theme.text_muted))
                    .child(self.i18n.t("plugin.no_search_results")),
            );
        } else {
            let mut list = div().w_full().min_w(px(0.0)).flex().flex_col();
            for (row_idx, (idx, plugin)) in visible.into_iter().enumerate() {
                let has_update = plugin.installed && plugin.version != plugin.latest_version;
                let expanded = self.pages.plugin_expanded_ids.contains(plugin.id);
                let pid = plugin.id;
                let action_btn = if has_update {
                    button_with(
                        &t,
                        self.i18n.t("plugin.update"),
                        ButtonOptions {
                            variant: ButtonVariant::Default,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Sm,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |h, _, _, cx| {
                            if let Some(item) = h.pages.plugins.get_mut(idx) {
                                item.version = item.latest_version;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .into_any_element()
                } else if plugin.installed {
                    status_pill(
                        &t,
                        self.i18n.t("plugin.installed"),
                        StatusPillOptions::new(StatusTone::Success).compact(),
                    )
                    .into_any_element()
                } else {
                    button_with(
                        &t,
                        self.i18n.t("plugin.install"),
                        ButtonOptions {
                            variant: ButtonVariant::Default,
                            size: ButtonSize::Sm,
                            radius: ButtonRadius::Sm,
                            disabled: false,
                        },
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |h, _, _, cx| {
                            if let Some(item) = h.pages.plugins.get_mut(idx) {
                                item.installed = true;
                                item.enabled = true;
                                item.version = item.latest_version;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                    .into_any_element()
                };

                let mut row = div()
                    .w_full()
                    .py(px(14.0))
                    .when(row_idx > 0, |r| {
                        r.border_t_1()
                            .border_color(rgba((theme.border << 8) | 0x45))
                    })
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .items_start()
                            .justify_between()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .items_start()
                                    .gap(px(12.0))
                                    .child(div().flex_shrink_0().pt(px(2.0)).child(glyph(
                                        LucideIcon::Puzzle,
                                        t.metrics.ui_menu_icon_size,
                                        theme.text_muted,
                                    )))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .flex()
                                            .flex_col()
                                            .gap(px(4.0))
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_wrap()
                                                    .items_center()
                                                    .gap(px(8.0))
                                                    .child(
                                                        div()
                                                            .text_size(px(t.metrics.ui_text_sm))
                                                            .font_weight(FontWeight::MEDIUM)
                                                            .text_color(rgb(theme.text))
                                                            .child(plugin.name),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(t.metrics.ui_text_xs))
                                                            .text_color(rgb(theme.text_muted))
                                                            .child(format!(
                                                                "v{}",
                                                                plugin.latest_version
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(t.metrics.ui_text_xs))
                                                            .text_color(rgb(theme.text_muted))
                                                            .child(
                                                                self.i18n
                                                                    .t("plugin.by_author")
                                                                    .replace(
                                                                        "{{author}}",
                                                                        plugin.author,
                                                                    ),
                                                            ),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(t.metrics.ui_text_xs))
                                                    .line_height(px(18.0))
                                                    .text_color(rgb(theme.text_muted))
                                                    .child(plugin.summary),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(
                                        div()
                                            .cursor_pointer()
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .rounded(px(t.radii.sm))
                                            .hover(|s| s.bg(rgb(theme.bg_hover)))
                                            .text_size(px(t.metrics.ui_text_xs))
                                            .text_color(rgb(theme.text_muted))
                                            .child(self.i18n.t(if expanded {
                                                "plugin.hide_details"
                                            } else {
                                                "plugin.show_details"
                                            }))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |h, _, _, cx| {
                                                    let now_expanded = if h
                                                        .pages
                                                        .plugin_expanded_ids
                                                        .insert(pid)
                                                    {
                                                        true
                                                    } else {
                                                        h.pages.plugin_expanded_ids.remove(pid);
                                                        false
                                                    };
                                                    h.begin_disclosure(
                                                        format!("plugin-mkt-details:{pid}"),
                                                        now_expanded,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                    cx.notify();
                                                }),
                                            ),
                                    )
                                    .child(action_btn),
                            ),
                    );
                let disc_key = format!("plugin-mkt-details:{pid}");
                if self.disclosure_motions.retained(&disc_key, expanded) {
                    let details = div()
                        .ml(px(28.0))
                        .p(px(10.0))
                        .rounded(px(t.radii.sm))
                        .bg(rgb(theme.bg_panel))
                        .border_1()
                        .border_color(rgba((theme.border << 8) | 0x60))
                        .flex()
                        .items_center()
                        .gap(px(16.0))
                        .text_size(px(t.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(format!(
                            "{}: {}",
                            self.i18n.t("plugin.create_plugin_id"),
                            plugin.id
                        ));
                    row = row.child(self.disclosure_motions.render(&disc_key, &t, details, None));
                }
                list = list.child(row);
            }
            card = card.child(list);
        }

        // Collapsible Install from URL section (exact desktop plugin_manager.rs:1144-1163)
        let url_expanded = self.pages.plugin_url_expanded;
        let url_toggle = toolbar_button(
            &t,
            self.i18n.t("plugin.url_install_title"),
            Some(animated_chevron(
                &t,
                "web-plugin-url-chevron",
                svg()
                    .path(LucideIcon::ChevronRight.path())
                    .size(px(14.0))
                    .text_color(rgb(theme.text_muted)),
                url_expanded,
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Outline,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled: false,
                },
                ..ToolbarButtonOptions::default()
            },
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|h, _, _, cx| {
                h.pages.plugin_url_expanded = !h.pages.plugin_url_expanded;
                h.begin_disclosure("plugin-url-install".into(), h.pages.plugin_url_expanded, cx);
                cx.stop_propagation();
                cx.notify();
            }),
        );
        div()
            .w_full()
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(card)
            .child(div().flex().items_center().child(url_toggle))
            .when(
                self.disclosure_motions
                    .retained("plugin-url-install", url_expanded),
                |section| {
                    section.child(
                        self.disclosure_motions.render(
                            "plugin-url-install",
                            &t,
                            self.spacious_card()
                                .child(
                                    div()
                                        .text_size(px(t.metrics.ui_text_xs))
                                        .line_height(px(18.0))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.i18n.t("plugin.url_install_desc")),
                                )
                                .child(
                                    div()
                                        .w_full()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(
                                            div()
                                                .flex_1()
                                                .h(px(34.0))
                                                .px(px(10.0))
                                                .rounded(px(t.radii.sm))
                                                .border_1()
                                                .border_color(rgb(theme.border))
                                                .bg(rgb(theme.bg_panel))
                                                .flex()
                                                .items_center()
                                                .text_size(px(t.metrics.ui_text_xs))
                                                .text_color(rgb(theme.text_muted))
                                                .child(self.i18n.t("plugin.url_placeholder")),
                                        )
                                        .child(button_with(
                                            &t,
                                            self.i18n.t("plugin.install"),
                                            ButtonOptions {
                                                variant: ButtonVariant::Default,
                                                size: ButtonSize::Sm,
                                                radius: ButtonRadius::Sm,
                                                disabled: true,
                                            },
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(t.metrics.ui_text_xs))
                                        .line_height(px(18.0))
                                        .text_color(rgb(theme.warning))
                                        .child(self.i18n.t("plugin.url_disclaimer")),
                                ),
                            None,
                        ),
                    )
                },
            )
            .into_any_element()
    }

    pub(crate) fn plugin_search_key(&mut self, e: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let on_marketplace = self
            .layout
            .active_tab()
            .is_some_and(|tab| tab.kind == TabKind::PluginManager)
            && self.pages.plugin_tab == PluginManagerTab::Marketplace;
        if !self.pages.plugin_search_focused || !on_marketplace {
            self.pages.plugin_search_focused = false;
            return false;
        }
        if e.keystroke.modifiers.platform || e.keystroke.modifiers.control {
            return false;
        }
        match e.keystroke.key.as_str() {
            "escape" => {
                self.pages.plugin_marketplace_search.clear();
                self.pages.plugin_search_focused = false;
                cx.notify();
                true
            }
            "enter" => {
                self.pages.plugin_search_focused = false;
                cx.notify();
                true
            }
            "backspace" => {
                self.pages.plugin_marketplace_search.pop();
                cx.notify();
                true
            }
            _ => {
                if let Some(ch) = &e.keystroke.key_char
                    && !ch.is_empty()
                    && !ch.chars().any(|c| c.is_control())
                {
                    self.pages.plugin_marketplace_search.push_str(ch);
                    cx.notify();
                    return true;
                }
                false
            }
        }
    }

    // -------------------------------------------------------------------------
    // Cloud Sync (TabKind::CloudSync) — Desktop card-by-card parity
    // -------------------------------------------------------------------------
    fn cloud_sync_section_title(&self, key: &str) -> AnyElement {
        div()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .font_weight(FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text))
            .child(self.i18n.t(key).to_uppercase())
            .into_any_element()
    }

    fn cloud_sync_toolbar_button(
        &self,
        icon: LucideIcon,
        label_key: &str,
        accent: bool,
        disabled: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let icon_color = if disabled {
            theme.text_muted
        } else if accent {
            theme.text_heading
        } else {
            theme.text_secondary
        };
        let btn = toolbar_button(
            &t,
            self.i18n.t(label_key),
            Some(glyph(icon, 14.0, icon_color)),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: if accent {
                        ButtonVariant::Default
                    } else {
                        ButtonVariant::Outline
                    },
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Sm,
                    disabled,
                },
                ..ToolbarButtonOptions::default()
            },
        );
        if disabled {
            btn.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .into_any_element()
        } else {
            btn.on_mouse_down(MouseButton::Left, listener)
                .into_any_element()
        }
    }

    fn cloud_sync_overview_fact(&self, icon: LucideIcon, label_key: &str, value: String) -> Div {
        let t = self.tokens;
        let theme = t.ui;
        div()
            .min_w(px(140.0))
            .flex_1()
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(t.radii.sm))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x55))
            .bg(rgb(theme.bg_panel))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(t.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(glyph(icon, 14.0, theme.accent))
                    .child(self.i18n.t(label_key)),
            )
            .child(
                div()
                    .text_size(px(t.metrics.ui_text_sm))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .truncate()
                    .child(value),
            )
    }

    fn format_history_summary(&self, entry: &CloudSyncHistoryEntry) -> String {
        self.i18n
            .t("plugin.cloud_sync.history.summary_line")
            .replace("{{connections}}", &entry.connections.to_string())
            .replace("{{forwards}}", &entry.forwards.to_string())
            .replace("{{quickCommands}}", &entry.quick_commands.to_string())
            .replace("{{serialProfiles}}", &entry.serial_profiles.to_string())
            .replace("{{bleProfiles}}", &entry.ble_profiles.to_string())
            .replace("{{telnetProfiles}}", &entry.telnet_profiles.to_string())
            .replace(
                "{{sensitiveCredentials}}",
                &entry.sensitive_credentials.to_string(),
            )
            .replace(
                "{{pluginSettingsCount}}",
                &entry.plugin_settings_count.to_string(),
            )
    }

    fn format_backup_summary(&self, backup: &CloudSyncRollbackBackup) -> String {
        self.i18n
            .t("plugin.cloud_sync.backup.summary_line")
            .replace("{{connections}}", &backup.connections.to_string())
            .replace("{{forwards}}", &backup.forwards.to_string())
            .replace("{{quickCommands}}", &backup.quick_commands.to_string())
            .replace("{{serialProfiles}}", &backup.serial_profiles.to_string())
            .replace("{{bleProfiles}}", &backup.ble_profiles.to_string())
            .replace("{{telnetProfiles}}", &backup.telnet_profiles.to_string())
            .replace(
                "{{sensitiveCredentials}}",
                &backup.sensitive_credentials.to_string(),
            )
            .replace(
                "{{pluginSettingsCount}}",
                &backup.plugin_settings_count.to_string(),
            )
            .replace("{{size}}", backup.size)
    }

    fn render_cloud_sync_history_entry(&self, entry: &CloudSyncHistoryEntry) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let action_key = match entry.action {
            "sync" => "plugin.cloud_sync.causal.sync",
            "restore" => "plugin.cloud_sync.history.action_restore",
            _ => "plugin.cloud_sync.history.action_upload",
        };
        div()
            .w_full()
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(t.radii.sm))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x55))
            .bg(rgb(theme.bg_panel))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .text_size(px(t.metrics.ui_text_sm))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text))
                    .child(self.i18n.t(action_key)),
            )
            .child(
                div()
                    .text_size(px(t.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(format!(
                        "{} · {}",
                        entry.timestamp,
                        self.format_history_summary(entry)
                    )),
            )
            .into_any_element()
    }

    fn render_cloud_sync_backup_row(
        &self,
        backup: &CloudSyncRollbackBackup,
        show_delete: bool,
        cx: &Context<Self>,
    ) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let backup_id = backup.id;
        div()
            .w_full()
            .px(px(12.0))
            .py(px(10.0))
            .rounded(px(t.radii.sm))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x55))
            .bg(rgb(theme.bg_panel))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(t.metrics.ui_text_sm))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(backup.created_at),
                    )
                    .child(
                        div()
                            .text_size(px(t.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.format_backup_summary(backup)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        button_with(
                            &t,
                            self.i18n.t("plugin.cloud_sync.actions.restore_backup"),
                            ButtonOptions {
                                variant: ButtonVariant::Outline,
                                size: ButtonSize::Sm,
                                radius: ButtonRadius::Sm,
                                disabled: false,
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |h, _, _, cx| {
                                h.pages.sync_history.push(CloudSyncHistoryEntry {
                                    action: "restore",
                                    timestamp: "just now",
                                    connections: h.sessions.len(),
                                    forwards: 1,
                                    quick_commands: 4,
                                    serial_profiles: 0,
                                    ble_profiles: h.ble_profiles.len(),
                                    telnet_profiles: 0,
                                    sensitive_credentials: 0,
                                    plugin_settings_count: 2,
                                });
                                h.pages.sync_local_dirty = false;
                                h.pages.sync_notice =
                                    Some(h.i18n.t("plugin.cloud_sync.history.action_restore"));
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                    )
                    .when(show_delete, |actions| {
                        actions.child(
                            button_with(
                                &t,
                                self.i18n.t("plugin.cloud_sync.actions.delete_backup"),
                                ButtonOptions {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Sm,
                                    disabled: false,
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |h, _, _, cx| {
                                    h.pages.sync_backups.retain(|b| b.id != backup_id);
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
            )
            .into_any_element()
    }

    pub(crate) fn render_cloud_sync_page(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let theme = t.ui;
        let active_tab = self.pages.sync_tab;

        let overview_label = self.i18n.t("plugin.cloud_sync.tabs.overview");
        let configure_label = self.i18n.t("plugin.cloud_sync.tabs.configure");
        let history_label = self.i18n.t("plugin.cloud_sync.tabs.history");
        let tab_width = oxideterm_gpui_ui::estimate_segmented_control_width(
            &t,
            CLOUD_SYNC_TAB_BAR_WIDTH,
            540.0,
            true,
            [
                (overview_label.as_str(), None),
                (configure_label.as_str(), None),
                (history_label.as_str(), None),
            ],
        );

        let make_tab = |tab: CloudSyncTab, icon: LucideIcon, label: String| {
            let active = active_tab == tab;
            let content = div()
                .w_full()
                .py(px(2.0))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(7.0))
                .child(glyph(
                    icon,
                    16.0,
                    if active {
                        theme.accent
                    } else {
                        theme.text_muted
                    },
                ))
                .child(label);
            segmented_control_item_content(&t, active, content.into_any_element())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |h, _, _, cx| {
                        if h.pages.sync_tab != tab {
                            let tab_idx = |s: CloudSyncTab| match s {
                                CloudSyncTab::Overview => 0,
                                CloudSyncTab::Configure => 1,
                                CloudSyncTab::History => 2,
                            };
                            let prev_idx = tab_idx(h.pages.sync_tab);
                            let next_idx = tab_idx(tab);
                            h.pages.sync_previous_tab = h.pages.sync_tab;
                            h.pages.sync_tab = tab;
                            h.begin_segmented_transition(
                                "web-cloud-sync-switcher",
                                prev_idx,
                                next_idx,
                                cx,
                            );
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .into_any_element()
        };

        let active_idx = match active_tab {
            CloudSyncTab::Overview => 0,
            CloudSyncTab::Configure => 1,
            CloudSyncTab::History => 2,
        };
        let prev_idx = self
            .segmented_motion
            .previous_index_for("web-cloud-sync-switcher", active_idx)
            .unwrap_or_else(|| match self.pages.sync_previous_tab {
                CloudSyncTab::Overview => 0,
                CloudSyncTab::Configure => 1,
                CloudSyncTab::History => 2,
            });
        let switcher = segmented_control(
            &t,
            "web-cloud-sync-switcher",
            SegmentedControlOptions::new(active_idx, prev_idx, 3)
                .user_transition_active(
                    self.segmented_motion
                        .is_active_for("web-cloud-sync-switcher", active_idx),
                )
                .compact(tab_width),
            vec![
                make_tab(CloudSyncTab::Overview, LucideIcon::Cloud, overview_label),
                make_tab(
                    CloudSyncTab::Configure,
                    LucideIcon::Settings,
                    configure_label,
                ),
                make_tab(CloudSyncTab::History, LucideIcon::Clock, history_label),
            ],
        )
        .into_any_element();

        let header = self.page_header_bar(
            self.i18n.t("plugin.cloud_sync.panel_title"),
            Some(self.i18n.t("plugin.cloud_sync.native_description")),
            switcher,
        );

        let content: Vec<AnyElement> = match active_tab {
            CloudSyncTab::Overview => self.render_cloud_sync_overview_cards(cx),
            CloudSyncTab::Configure => self.render_cloud_sync_configure_cards(cx),
            CloudSyncTab::History => self.render_cloud_sync_history_cards(cx),
        };

        div()
            .id("web-cloud-sync-page")
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(header)
            .child(
                div()
                    .id("web-cloud-sync-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px(px(t.metrics.settings_content_padding))
                    .py(px(t.metrics.settings_page_gap))
                    .child(fade_in(
                        &t,
                        ("web-cloud-sync-body", active_idx),
                        div()
                            .w_full()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(t.metrics.settings_page_gap))
                            .children(content),
                        MotionDuration::Micro,
                    )),
            )
            .into_any_element()
    }

    fn render_cloud_sync_overview_cards(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let t = self.tokens;
        let theme = t.ui;
        let local_file = self.pages.sync_backend.is_local_file();
        let backend_label = self.i18n.t(self.pages.sync_backend.label_key());
        let local_dirty = self.i18n.t(if self.pages.sync_local_dirty {
            "plugin.cloud_sync.common.yes"
        } else {
            "plugin.cloud_sync.common.no"
        });
        let last_sync = self.pages.sync_last_sync.unwrap_or("—").to_string();
        let has_backups = !self.pages.sync_backups.is_empty();

        // Card 1: Overview status + 4 facts + desktop toolbar buttons
        let overview_card = self
            .spacious_card()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.cloud_sync_section_title("plugin.cloud_sync.tabs.overview"))
                    .child(status_pill(
                        &t,
                        self.i18n.t("plugin.cloud_sync.status.ready"),
                        StatusPillOptions::new(StatusTone::Accent).compact(),
                    )),
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
                            .child(self.cloud_sync_overview_fact(
                                LucideIcon::Server,
                                "plugin.cloud_sync.fields.backend",
                                backend_label,
                            ))
                            .when(!local_file, |facts| {
                                facts
                                    .child(self.cloud_sync_overview_fact(
                                        LucideIcon::Hash,
                                        "plugin.cloud_sync.fields.namespace",
                                        self.pages.sync_namespace.clone(),
                                    ))
                                    .child(self.cloud_sync_overview_fact(
                                        LucideIcon::Activity,
                                        "plugin.cloud_sync.fields.local_dirty",
                                        local_dirty,
                                    ))
                                    .child(self.cloud_sync_overview_fact(
                                        LucideIcon::Clock,
                                        "plugin.cloud_sync.fields.last_sync",
                                        last_sync,
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
                            .child(self.cloud_sync_toolbar_button(
                                if local_file {
                                    LucideIcon::Upload
                                } else {
                                    LucideIcon::RefreshCw
                                },
                                if local_file {
                                    "plugin.cloud_sync.actions.export_local"
                                } else {
                                    "plugin.cloud_sync.causal.sync"
                                },
                                true,
                                false,
                                cx.listener(|h, _, _, cx| {
                                    h.pages.sync_history.push(CloudSyncHistoryEntry {
                                        action: "sync",
                                        timestamp: "just now",
                                        connections: h.sessions.len(),
                                        forwards: 1,
                                        quick_commands: 4,
                                        serial_profiles: 0,
                                        ble_profiles: h.ble_profiles.len(),
                                        telnet_profiles: 0,
                                        sensitive_credentials: 0,
                                        plugin_settings_count: 2,
                                    });
                                    let b_id = h.pages.sync_backups.len() as u32 + 1;
                                    h.pages.sync_backups.push(CloudSyncRollbackBackup {
                                        id: b_id,
                                        created_at: "just now",
                                        connections: h.sessions.len(),
                                        forwards: 1,
                                        quick_commands: 4,
                                        serial_profiles: 0,
                                        ble_profiles: h.ble_profiles.len(),
                                        telnet_profiles: 0,
                                        sensitive_credentials: 0,
                                        plugin_settings_count: 2,
                                        size: "14.6 KB",
                                    });
                                    h.pages.sync_last_sync = Some("just now");
                                    h.pages.sync_local_dirty = false;
                                    h.pages.sync_notice =
                                        Some(h.i18n.t("plugin.cloud_sync.status.ready"));
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.cloud_sync_toolbar_button(
                                LucideIcon::Download,
                                if local_file {
                                    "plugin.cloud_sync.actions.import_local"
                                } else {
                                    "plugin.cloud_sync.causal.view_remote"
                                },
                                false,
                                false,
                                cx.listener(|h, _, _, cx| {
                                    h.pages.sync_notice =
                                        Some(h.i18n.t("plugin.cloud_sync.status.ready"));
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ))
                            .child(self.cloud_sync_toolbar_button(
                                LucideIcon::RotateCcw,
                                "plugin.cloud_sync.actions.restore_backup",
                                false,
                                !has_backups,
                                cx.listener(|h, _, _, cx| {
                                    h.pages.sync_history.push(CloudSyncHistoryEntry {
                                        action: "restore",
                                        timestamp: "just now",
                                        connections: h.sessions.len(),
                                        forwards: 1,
                                        quick_commands: 4,
                                        serial_profiles: 0,
                                        ble_profiles: h.ble_profiles.len(),
                                        telnet_profiles: 0,
                                        sensitive_credentials: 0,
                                        plugin_settings_count: 2,
                                    });
                                    h.pages.sync_local_dirty = false;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .when_some(self.pages.sync_notice.as_ref(), |card, notice| {
                card.child(
                    div()
                        .text_size(px(t.metrics.ui_text_xs))
                        .text_color(rgb(theme.success))
                        .child(notice.clone()),
                )
            })
            .into_any_element();

        // Card 2: Recent History
        let recent_entries: Vec<&CloudSyncHistoryEntry> =
            self.pages.sync_history.iter().rev().take(3).collect();
        let recent_history_card = self
            .spacious_card()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        self.cloud_sync_section_title("plugin.cloud_sync.overview.recent_history"),
                    )
                    .when(!recent_entries.is_empty(), |hdr| {
                        hdr.child(
                            button_with(
                                &t,
                                self.i18n.t("plugin.cloud_sync.overview.view_all_history"),
                                ButtonOptions {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Sm,
                                    disabled: false,
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|h, _, _, cx| {
                                    h.pages.sync_tab = CloudSyncTab::History;
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
            )
            .child(if recent_entries.is_empty() {
                div()
                    .text_size(px(t.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("plugin.cloud_sync.history_empty"))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(
                        recent_entries
                            .into_iter()
                            .map(|entry| self.render_cloud_sync_history_entry(entry)),
                    )
                    .into_any_element()
            })
            .into_any_element();

        let mut cards = vec![overview_card, recent_history_card];

        // Card 3: Recent Rollback Backups (matches desktop: only rendered when non-empty)
        if !self.pages.sync_backups.is_empty() {
            let rollback_card = self
                .spacious_card()
                .child(self.cloud_sync_section_title("plugin.cloud_sync.sections.rollback_backups"))
                .children(
                    self.pages
                        .sync_backups
                        .iter()
                        .rev()
                        .take(3)
                        .map(|backup| self.render_cloud_sync_backup_row(backup, false, cx)),
                )
                .into_any_element();
            cards.push(rollback_card);
        }

        cards
    }

    fn render_cloud_sync_configure_cards(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let t = self.tokens;
        let theme = t.ui;
        let backend = self.pages.sync_backend;

        // Card 1: Quick Start Guide (exact desktop guide.rs structure)
        let (guide_title, guide_desc, guide_examples, guide_warning): (
            &str,
            &str,
            Vec<(&str, &str)>,
            Option<&str>,
        ) = match backend {
            CloudSyncBackend::Webdav => (
                "plugin.cloud_sync.guide.webdav_title",
                "plugin.cloud_sync.guide.webdav_description",
                vec![
                    (
                        "plugin.cloud_sync.settings.endpoint",
                        "plugin.cloud_sync.guide.webdav_example_endpoint",
                    ),
                    (
                        "plugin.cloud_sync.settings.namespace",
                        "plugin.cloud_sync.guide.webdav_example_namespace",
                    ),
                    (
                        "plugin.cloud_sync.settings.basic_username",
                        "plugin.cloud_sync.guide.webdav_example_username",
                    ),
                    (
                        "plugin.cloud_sync.settings.basic_password",
                        "plugin.cloud_sync.guide.webdav_example_password",
                    ),
                ],
                Some("plugin.cloud_sync.guide.webdav_duplicate_warning"),
            ),
            CloudSyncBackend::HttpJson => (
                "plugin.cloud_sync.guide.http_json_title",
                "plugin.cloud_sync.guide.http_json_description",
                vec![
                    (
                        "plugin.cloud_sync.settings.endpoint",
                        "plugin.cloud_sync.guide.http_json_example_endpoint",
                    ),
                    (
                        "plugin.cloud_sync.settings.namespace",
                        "plugin.cloud_sync.guide.http_json_example_namespace",
                    ),
                ],
                None,
            ),
            CloudSyncBackend::OneDrive => (
                "plugin.cloud_sync.guide.onedrive_title",
                "plugin.cloud_sync.guide.onedrive_description",
                vec![],
                Some("plugin.cloud_sync.guide.onedrive_permission_warning"),
            ),
            CloudSyncBackend::GoogleDrive => (
                "plugin.cloud_sync.guide.google_drive_title",
                "plugin.cloud_sync.guide.google_drive_description",
                vec![],
                Some("plugin.cloud_sync.guide.google_drive_permission_warning"),
            ),
            CloudSyncBackend::Dropbox => (
                "plugin.cloud_sync.backend.dropbox",
                "plugin.cloud_sync.notes.backend_dropbox",
                vec![],
                None,
            ),
            CloudSyncBackend::GithubGist => (
                "plugin.cloud_sync.backend.github_gist",
                "plugin.cloud_sync.notes.backend_github_gist",
                vec![],
                None,
            ),
            CloudSyncBackend::Git => (
                "plugin.cloud_sync.backend.git",
                "plugin.cloud_sync.notes.backend_git",
                vec![],
                None,
            ),
            CloudSyncBackend::S3 => (
                "plugin.cloud_sync.backend.s3",
                "plugin.cloud_sync.notes.backend_s3",
                vec![],
                None,
            ),
            CloudSyncBackend::LocalFile => (
                "plugin.cloud_sync.backend.local_file",
                "plugin.cloud_sync.native_description",
                vec![],
                None,
            ),
        };

        let mut steps = div()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .pl(px(16.0))
            .text_size(px(t.metrics.ui_text_sm))
            .line_height(px(20.0))
            .text_color(rgb(theme.text_muted));
        for (i, key) in CLOUD_SYNC_GUIDE_STEP_KEYS.iter().enumerate() {
            steps = steps.child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(format!("{}.", i + 1))
                    .child(self.i18n.t(key)),
            );
        }

        let guide_card = fade_in(
            &t,
            ("web-cloud-sync-guide", backend as usize),
            self.spacious_card()
                .child(self.cloud_sync_section_title("plugin.cloud_sync.sections.quick_start"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(t.metrics.ui_text_sm))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(theme.text_heading))
                                .child(self.i18n.t(guide_title)),
                        )
                        .child(
                            div()
                                .text_size(px(t.metrics.ui_text_sm))
                                .line_height(px(20.0))
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t(guide_desc)),
                        ),
                )
                .child(steps)
                .when(!guide_examples.is_empty(), |card| {
                    card.child(
                        div()
                            .p(px(12.0))
                            .rounded(px(t.radii.sm))
                            .bg(rgb(theme.bg_panel))
                            .border_1()
                            .border_color(rgba((theme.border << 8) | 0x55))
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .text_size(px(t.metrics.ui_text_xs))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.i18n.t("plugin.cloud_sync.guide.example_title")),
                            )
                            .children(guide_examples.into_iter().map(|(label_key, val_key)| {
                                div()
                                    .flex()
                                    .flex_wrap()
                                    .items_center()
                                    .gap(px(8.0))
                                    .text_size(px(t.metrics.ui_text_xs))
                                    .child(
                                        div()
                                            .text_color(rgb(theme.text_muted))
                                            .child(format!("{}:", self.i18n.t(label_key))),
                                    )
                                    .child(
                                        div()
                                            .font_family("Lilex")
                                            .text_color(rgb(theme.accent))
                                            .child(self.i18n.t(val_key)),
                                    )
                            })),
                    )
                })
                .when_some(guide_warning, |card, warn_key| {
                    card.child(
                        div()
                            .text_size(px(t.metrics.ui_text_xs))
                            .line_height(px(18.0))
                            .text_color(rgb(theme.accent))
                            .child(self.i18n.t(warn_key)),
                    )
                }),
            MotionDuration::Micro,
        );

        // Card 2: Connection Settings (all 8 real desktop backends + Local File + Save button)
        let mut backend_pills = div().flex().flex_wrap().gap(px(8.0));
        for candidate in CloudSyncBackend::ALL {
            let active = backend == candidate;
            backend_pills = backend_pills.child(self.plugin_tag_pill(
                self.i18n.t(candidate.label_key()),
                active,
                cx.listener(move |h, _, _, cx| {
                    h.pages.sync_backend = candidate;
                    h.pages.sync_local_dirty = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }

        let mut conflict_pills = div().flex().flex_wrap().gap(px(8.0));
        for strategy in CloudSyncConflictStrategy::ALL {
            let active = self.pages.sync_conflict == strategy;
            conflict_pills = conflict_pills.child(self.plugin_tag_pill(
                self.i18n.t(strategy.label_key()),
                active,
                cx.listener(move |h, _, _, cx| {
                    h.pages.sync_conflict = strategy;
                    h.pages.sync_local_dirty = true;
                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }

        let connection_card = self
            .spacious_card()
            .child(
                div()
                    .w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        self.cloud_sync_section_title(
                            "plugin.cloud_sync.sections.connection_settings",
                        ),
                    )
                    .child(self.cloud_sync_toolbar_button(
                        LucideIcon::Save,
                        "plugin.cloud_sync.actions.save_settings",
                        false,
                        false,
                        cx.listener(|h, _, _, cx| {
                            h.pages.sync_local_dirty = false;
                            h.pages.sync_notice =
                                Some(h.i18n.t("plugin.cloud_sync.actions.save_settings"));
                            cx.stop_propagation();
                            cx.notify();
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
                            .text_size(px(t.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("plugin.cloud_sync.fields.backend")),
                    )
                    .child(backend_pills),
            )
            .when(!backend.is_local_file(), |card| {
                card.child(
                    div()
                        .w_full()
                        .flex()
                        .flex_wrap()
                        .gap(px(12.0))
                        .child(
                            div()
                                .min_w(px(240.0))
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .text_size(px(t.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.i18n.t("plugin.cloud_sync.settings.endpoint")),
                                )
                                .child(
                                    div()
                                        .h(px(34.0))
                                        .px(px(10.0))
                                        .rounded(px(t.radii.sm))
                                        .border_1()
                                        .border_color(rgb(theme.border))
                                        .bg(rgb(theme.bg_panel))
                                        .flex()
                                        .items_center()
                                        .font_family("Lilex")
                                        .text_size(px(t.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text))
                                        .child(self.pages.sync_endpoint.clone()),
                                ),
                        )
                        .child(
                            div()
                                .min_w(px(200.0))
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .text_size(px(t.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.i18n.t("plugin.cloud_sync.settings.namespace")),
                                )
                                .child(
                                    div()
                                        .h(px(34.0))
                                        .px(px(10.0))
                                        .rounded(px(t.radii.sm))
                                        .border_1()
                                        .border_color(rgb(theme.border))
                                        .bg(rgb(theme.bg_panel))
                                        .flex()
                                        .items_center()
                                        .font_family("Lilex")
                                        .text_size(px(t.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text))
                                        .child(self.pages.sync_namespace.clone()),
                                ),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .pt(px(4.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(t.metrics.ui_text_sm))
                                .text_color(rgb(theme.text))
                                .child(
                                    self.i18n
                                        .t("plugin.cloud_sync.settings.auto_upload_enabled"),
                                ),
                        )
                        .child(
                            switch(&t, "web-sync-auto-upload", "", self.pages.sync_auto_upload)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|h, _, _, cx| {
                                        h.pages.sync_auto_upload = !h.pages.sync_auto_upload;
                                        h.pages.sync_local_dirty = true;
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
            })
            .child(conflict_pills)
            .into_any_element();

        // Card 3: Sync Scope (all 11 real desktop toggles in 2-column wrap grid)
        let scope_items: Vec<(&'static str, bool, fn(&mut PagesState))> = vec![
            (
                "plugin.cloud_sync.settings.sync_connections",
                self.pages.sync_connections,
                |p| p.sync_connections = !p.sync_connections,
            ),
            (
                "plugin.cloud_sync.settings.sync_forwards",
                self.pages.sync_forwards,
                |p| p.sync_forwards = !p.sync_forwards,
            ),
            (
                "plugin.cloud_sync.settings.sync_quick_commands",
                self.pages.sync_quick_commands,
                |p| p.sync_quick_commands = !p.sync_quick_commands,
            ),
            (
                "plugin.cloud_sync.settings.sync_serial_profiles",
                self.pages.sync_serial_profiles,
                |p| p.sync_serial_profiles = !p.sync_serial_profiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_ble_profiles",
                self.pages.sync_ble_profiles,
                |p| p.sync_ble_profiles = !p.sync_ble_profiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_telnet_profiles",
                self.pages.sync_telnet_profiles,
                |p| p.sync_telnet_profiles = !p.sync_telnet_profiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_mosh_profiles",
                self.pages.sync_mosh_profiles,
                |p| p.sync_mosh_profiles = !p.sync_mosh_profiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_remote_desktop_profiles",
                self.pages.sync_remote_desktop_profiles,
                |p| p.sync_remote_desktop_profiles = !p.sync_remote_desktop_profiles,
            ),
            (
                "plugin.cloud_sync.settings.sync_sensitive_credentials",
                self.pages.sync_sensitive_credentials,
                |p| p.sync_sensitive_credentials = !p.sync_sensitive_credentials,
            ),
            (
                "plugin.cloud_sync.settings.sync_app_settings",
                self.pages.sync_app_settings,
                |p| p.sync_app_settings = !p.sync_app_settings,
            ),
            (
                "plugin.cloud_sync.settings.sync_plugin_settings",
                self.pages.sync_plugin_settings,
                |p| p.sync_plugin_settings = !p.sync_plugin_settings,
            ),
        ];

        let scope_grid = div()
            .w_full()
            .flex()
            .flex_wrap()
            .gap_x(px(16.0))
            .gap_y(px(8.0))
            .children(scope_items.into_iter().map(|(label_key, checked, toggle)| {
                div()
                    .min_w(px(260.0))
                    .flex_1()
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(rgba((theme.border << 8) | 0x40))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(t.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t(label_key)),
                    )
                    .child(switch(&t, label_key, "", checked).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |h, _, _, cx| {
                            toggle(&mut h.pages);
                            h.pages.sync_local_dirty = true;
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    ))
            }));

        let scope_card = self
            .spacious_card()
            .child(self.cloud_sync_section_title("plugin.cloud_sync.sections.sync_scope"))
            .child(scope_grid)
            .into_any_element();

        // Card 4: Sync Health (5 desktop check rows inside outer inspector surface)
        let health_rows = [
            (
                "plugin.cloud_sync.health.backend_config",
                "plugin.cloud_sync.health.backend_config_ok",
                StatusTone::Success,
                "plugin.cloud_sync.health.status_pass",
            ),
            (
                "plugin.cloud_sync.health.sync_password",
                "plugin.cloud_sync.health.sync_password_missing",
                StatusTone::Warning,
                "plugin.cloud_sync.health.status_warning",
            ),
            (
                "plugin.cloud_sync.health.remote_check",
                if self.pages.sync_last_sync.is_some() {
                    "plugin.cloud_sync.health.remote_check_ok"
                } else {
                    "plugin.cloud_sync.health.remote_check_not_run"
                },
                if self.pages.sync_last_sync.is_some() {
                    StatusTone::Success
                } else {
                    StatusTone::Warning
                },
                if self.pages.sync_last_sync.is_some() {
                    "plugin.cloud_sync.health.status_pass"
                } else {
                    "plugin.cloud_sync.health.status_warning"
                },
            ),
            (
                "plugin.cloud_sync.health.conflict_state",
                "plugin.cloud_sync.health.conflict_state_ok",
                StatusTone::Success,
                "plugin.cloud_sync.health.status_pass",
            ),
            (
                "plugin.cloud_sync.health.local_changes",
                if self.pages.sync_local_dirty {
                    "plugin.cloud_sync.health.local_changes_dirty"
                } else {
                    "plugin.cloud_sync.health.local_changes_clean"
                },
                if self.pages.sync_local_dirty {
                    StatusTone::Warning
                } else {
                    StatusTone::Success
                },
                if self.pages.sync_local_dirty {
                    "plugin.cloud_sync.health.status_warning"
                } else {
                    "plugin.cloud_sync.health.status_pass"
                },
            ),
        ];

        let health_card = self
            .spacious_card()
            .child(self.cloud_sync_section_title("plugin.cloud_sync.sections.sync_health"))
            .child(
                div()
                    .text_size(px(t.metrics.ui_text_xs))
                    .line_height(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("plugin.cloud_sync.health.title")),
            )
            .child(
                div().w_full().flex().flex_wrap().gap_x(px(16.0)).children(
                    health_rows
                        .into_iter()
                        .map(|(label_key, detail_key, tone, status_key)| {
                            div()
                                .min_w(px(260.0))
                                .flex_1()
                                .border_b_1()
                                .border_color(rgba((theme.border << 8) | 0x40))
                                .py(px(12.0))
                                .flex()
                                .flex_col()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_between()
                                        .gap(px(10.0))
                                        .child(
                                            div()
                                                .text_size(px(t.metrics.ui_text_sm))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text))
                                                .child(self.i18n.t(label_key)),
                                        )
                                        .child(status_pill(
                                            &t,
                                            self.i18n.t(status_key),
                                            StatusPillOptions::new(tone).compact(),
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(px(t.metrics.ui_text_xs))
                                        .text_color(rgb(theme.text_muted))
                                        .child(self.i18n.t(detail_key)),
                                )
                        }),
                ),
            )
            .into_any_element();

        // Card 5: Notes
        let notes_card = self
            .spacious_card()
            .child(
                div()
                    .text_size(px(t.metrics.ui_text_sm))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(self.i18n.t("plugin.cloud_sync.sections.notes")),
            )
            .child(
                div()
                    .text_size(px(t.metrics.ui_text_sm))
                    .line_height(px(20.0))
                    .text_color(rgb(theme.text_muted))
                    .child(
                        self.i18n
                            .t("plugin.cloud_sync.native_scope_summary")
                            .replace("{{sections}}", "General · Appearance · Terminal · AI"),
                    ),
            )
            .into_any_element();

        vec![
            guide_card,
            connection_card,
            scope_card,
            health_card,
            notes_card,
        ]
    }

    fn render_cloud_sync_history_cards(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let t = self.tokens;
        let theme = t.ui;

        // Card 1: Full Sync History
        let history_card = self
            .spacious_card()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(self.cloud_sync_section_title("plugin.cloud_sync.sections.sync_history"))
                    .when(!self.pages.sync_history.is_empty(), |hdr| {
                        hdr.child(
                            button_with(
                                &t,
                                self.i18n.t("plugin.cloud_sync.actions.clear_history"),
                                ButtonOptions {
                                    variant: ButtonVariant::Ghost,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Sm,
                                    disabled: false,
                                },
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|h, _, _, cx| {
                                    h.pages.sync_history.clear();
                                    cx.stop_propagation();
                                    cx.notify();
                                }),
                            ),
                        )
                    }),
            )
            .child(if self.pages.sync_history.is_empty() {
                div()
                    .text_size(px(t.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("plugin.cloud_sync.history_empty"))
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .children(
                        self.pages
                            .sync_history
                            .iter()
                            .rev()
                            .map(|entry| self.render_cloud_sync_history_entry(entry)),
                    )
                    .into_any_element()
            })
            .into_any_element();

        // Card 2: Rollback Backups
        let backups_card =
            self.spacious_card()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.0))
                        .child(self.cloud_sync_section_title(
                            "plugin.cloud_sync.sections.rollback_backups",
                        ))
                        .when(!self.pages.sync_backups.is_empty(), |hdr| {
                            hdr.child(
                                button_with(
                                    &t,
                                    self.i18n.t("plugin.cloud_sync.actions.clear_backups"),
                                    ButtonOptions {
                                        variant: ButtonVariant::Ghost,
                                        size: ButtonSize::Sm,
                                        radius: ButtonRadius::Sm,
                                        disabled: false,
                                    },
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|h, _, _, cx| {
                                        h.pages.sync_backups.clear();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                            )
                        }),
                )
                .child(if self.pages.sync_backups.is_empty() {
                    div()
                        .text_size(px(t.metrics.ui_text_sm))
                        .text_color(rgb(theme.text_muted))
                        .child(self.i18n.t("plugin.cloud_sync.history_empty"))
                        .into_any_element()
                } else {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(8.0))
                        .children(
                            self.pages
                                .sync_backups
                                .iter()
                                .rev()
                                .map(|backup| self.render_cloud_sync_backup_row(backup, true, cx)),
                        )
                        .into_any_element()
                })
                .into_any_element();

        vec![history_card, backups_card]
    }
}
