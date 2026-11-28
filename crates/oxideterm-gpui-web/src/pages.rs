//! Desktop-parity main workspace pages for Runtime/Dashboard, Plugin Manager,
//! and Cloud Sync in OxideTerm Web.

mod catalog;
mod topology;
pub(crate) use catalog::Catalog;

use crate::host::Workspace;
use gpui::{prelude::*, *};
use oxideterm_gpui_ui::{
    SegmentedControlOptions, SurfaceKind, SurfaceOptions, SurfacePadding,
    badge::{StatusPillOptions, StatusTone, status_pill},
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, ToolbarButtonOptions,
        toolbar_button,
    },
    icons::LucideIcon,
    motion::{MotionDuration, fade_in},
    page_header, segmented_control, segmented_control_item_content, semantic_surface,
    switch::switch,
    typography::{MonospaceDatumOptions, MonospaceDatumTone, monospace_datum},
};
use oxideterm_settings::PluginMarketplaceSort;

const MONITOR_EMERALD_DARK: u32 = 0x10b981;
const MONITOR_RED: u32 = 0xef4444;

const PLUGIN_MANAGER_TAB_BAR_WIDTH: f32 = 340.0;
const CLOUD_SYNC_TAB_BAR_WIDTH: f32 = 396.0;
const OFFICIAL_PLUGIN_MARKETPLACE_HOME: &str =
    "https://github.com/AnalyseDeCircuit/oxideterm-plugins";

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

pub struct PagesState {
    pub(crate) graph_menu: Option<(topology::GraphTarget, Point<Pixels>)>,
    pub runtime_section: RuntimeSection,
    pub runtime_previous_section: RuntimeSection,

    // Plugin Manager state
    pub plugin_tab: PluginManagerTab,
    pub plugin_previous_tab: PluginManagerTab,
    pub(crate) catalog: Catalog,

    // Cloud Sync state
    pub sync_tab: CloudSyncTab,
    pub sync_previous_tab: CloudSyncTab,
    pub sync_backend: CloudSyncBackend,
    pub sync_conflict: CloudSyncConflictStrategy,
    pub sync_namespace: String,
    pub sync_endpoint: String,
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
}

impl Default for PagesState {
    fn default() -> Self {
        Self {
            graph_menu: None,
            runtime_section: RuntimeSection::Overview,
            runtime_previous_section: RuntimeSection::Overview,

            plugin_tab: PluginManagerTab::Installed,
            plugin_previous_tab: PluginManagerTab::Installed,
            catalog: Default::default(),

            sync_tab: CloudSyncTab::Overview,
            sync_previous_tab: CloudSyncTab::Overview,
            sync_backend: CloudSyncBackend::Webdav,
            sync_conflict: CloudSyncConflictStrategy::Merge,
            sync_namespace: "oxideterm-default".into(),
            sync_endpoint: "https://dav.example.com/oxideterm".into(),
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

        let (snapshot, targets) = self.workspace_topology(cx);
        let summaries = &snapshot.nodes;
        let active = summaries
            .iter()
            .filter(|n| n.status == oxideterm_topology::ConnectionTopologyStatus::Active)
            .count();
        let down = summaries
            .iter()
            .filter(|n| n.status == oxideterm_topology::ConnectionTopologyStatus::LinkDown)
            .count();
        let ble_active = self
            .sessions
            .values()
            .filter(|s| s.is_ble() && s.connected(self.ready))
            .count();

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
                                .child(self.i18n.t("topology.demo_workspace")),
                            ),
                    )
                    .child(
                        monospace_datum(
                            &t,
                            summaries.len().to_string(),
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
                                active,
                                LucideIcon::Activity,
                                if active > 0 {
                                    MONITOR_EMERALD_DARK
                                } else {
                                    theme.text_muted
                                },
                            ))
                            .child(metric(
                                self.i18n.t("connections.state.connecting"),
                                summaries.iter().filter(|n| n.status == oxideterm_topology::ConnectionTopologyStatus::Connecting).count(),
                                LucideIcon::RefreshCw, theme.text_muted,
                            ))
                            .child(metric(
                                self.i18n.t("connections.monitor.link_down"),
                                down,
                                LucideIcon::AlertTriangle,
                                if down > 0 {
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

                let sftp_open = self.files.as_ref().map_or(0, |f| f.sftp_pages.len());
                let ide_open = self.projects.project_count();
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
                        self.sessions.len(),
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
                    let (label, tone, color) = self.node_status(entry.status);
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
                                    .child(div().size(px(8.0)).rounded_full().bg(rgb(color)))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_size(px(t.metrics.ui_text_sm))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(theme.text))
                                                    .child(topology::address(entry)),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(t.metrics.ui_text_xs))
                                                    .text_color(rgb(theme.text_muted))
                                                    .child(format!(
                                                        "{}: {} · {}: {}",
                                                        self.i18n
                                                            .t("connections.monitor.terminals"),
                                                        entry.consumers.terminals,
                                                        self.i18n.t("sidebar.panels.sftp"),
                                                        entry.consumers.sftp
                                                    )),
                                            ),
                                    ),
                            )
                            .child(status_pill(
                                &t,
                                self.i18n.t(label),
                                StatusPillOptions::new(tone).compact(),
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
            RuntimeSection::Topology => self.topology_graph(&snapshot, &targets, cx),
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
    // Plugin catalog controls
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
            Some(self.i18n.t("plugin.cloud_sync.browser_preview")),
            switcher,
        );

        let content: Vec<AnyElement> = match active_tab {
            CloudSyncTab::Overview => self.render_cloud_sync_overview_cards(),
            CloudSyncTab::Configure => self.render_cloud_sync_configure_cards(cx),
            CloudSyncTab::History => vec![self.cloud_sync_empty_history()],
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

    fn cloud_sync_empty_history(&self) -> AnyElement {
        self.spacious_card()
            .child(self.cloud_sync_section_title("plugin.cloud_sync.sections.sync_history"))
            .child(
                div()
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.i18n.t("plugin.cloud_sync.history_empty")),
            )
            .into_any_element()
    }

    fn render_cloud_sync_overview_cards(&self) -> Vec<AnyElement> {
        let backend = self.pages.sync_backend;
        let facts = div()
            .flex()
            .flex_wrap()
            .gap(px(10.0))
            .child(self.cloud_sync_overview_fact(
                LucideIcon::Server,
                "plugin.cloud_sync.fields.backend",
                self.i18n.t(backend.label_key()),
            ))
            .when(!backend.is_local_file(), |row| {
                row.child(self.cloud_sync_overview_fact(
                    LucideIcon::Hash,
                    "plugin.cloud_sync.fields.namespace",
                    self.pages.sync_namespace.clone(),
                ))
            });
        vec![
            self.spacious_card()
                .child(self.cloud_sync_section_title("plugin.cloud_sync.tabs.overview"))
                .child(facts)
                .into_any_element(),
            self.cloud_sync_empty_history(),
        ]
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

                    cx.stop_propagation();
                    cx.notify();
                }),
            ));
        }

        let connection_card = self
            .spacious_card()
            .child(self.cloud_sync_section_title("plugin.cloud_sync.sections.connection_settings"))
            .child(backend_pills)
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

        vec![guide_card, connection_card, scope_card]
    }
}
