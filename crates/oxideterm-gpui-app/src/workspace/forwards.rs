use std::{
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use gpui::{
    AnyElement, Context, IntoElement, MouseButton, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::*, px, rgb, rgba,
};
use oxideterm_forwarding::{
    DetectedPort, ForwardEvent, ForwardRule, ForwardStats, ForwardStatus, ForwardType,
    ForwardUpdate, ForwardingManager, PortDetectionSnapshot,
};
use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};
use oxideterm_ssh::NodeId;
use oxideterm_workspace::{Tab, TabId, TabKind, TabTitleSource, TerminalSessionId};

use super::ime::WorkspaceImeTarget;
use super::*;

const FORWARDS_MAX_WIDTH: f32 = 896.0; // Tauri max-w-4xl
const FORWARDS_PAGE_PADDING: f32 = 16.0; // Tauri p-4
const FORWARDS_SECTION_GAP: f32 = 24.0; // Tauri space-y-6
const FORWARDS_CARD_RADIUS: f32 = 8.0; // Tauri rounded-lg
const FORWARDS_FORM_RADIUS: f32 = 2.0; // Tauri rounded-sm
const FORWARDS_TABLE_HEADER_H: f32 = 34.0; // Tauri px-4 py-2 text-sm
const FORWARDS_TABLE_ROW_H: f32 = 42.0;
const FORWARDS_TEXT_SM: f32 = 13.0;
const FORWARDS_TEXT_XS: f32 = 12.0;
const FORWARDS_TYPE_BADGE_H: f32 = 20.0;
const FORWARDS_PORT_SCAN_INTERVAL: Duration = Duration::from_secs(12);
const FORWARDS_BG_ACTIVE_THEME_ALPHA: u32 = 0x66; // Tauri [data-bg-active] theme bg/panel/card 40%
const FORWARDS_BG_ACTIVE_HOVER_ALPHA: u32 = 0x80; // Tauri [data-bg-active] bg-hover 50%
const FORWARDS_BG_ACTIVE_SUNKEN_ALPHA: u32 = 0x59; // Tauri [data-bg-active] bg-sunken 35%
const FORWARDS_BG_ACTIVE_BORDER_ALPHA: u32 = 0xbf; // Tauri [data-bg-active] border 75%
const FORWARDS_BG_ACTIVE_BORDER_HALF_ALPHA: u32 = 0x60; // Tauri border/50 after active border mix
const FORWARDS_TW_ALPHA_05: u32 = 0x0d; // Tauri /5
const FORWARDS_TW_ALPHA_30: u32 = 0x4d; // Tauri /30
const FORWARDS_TW_ALPHA_40: u32 = 0x66; // Tauri /40
const FORWARDS_TW_ALPHA_50: u32 = 0x80; // Tauri /50
const FORWARDS_ALPHA_TRANSPARENT: u32 = 0x00; // Tauri transparent root when tab background is active
const FORWARDS_DEFAULT_BIND_ADDRESS: &str = "localhost"; // Tauri create form default bindAddress
const FORWARDS_DEFAULT_TARGET_HOST: &str = "localhost"; // Tauri create form default targetHost

// Tailwind palette literals used by the Tauri ForwardsView source.
const TW_BLACK: u32 = 0x000000;
const TW_BLUE_400: u32 = 0x60a5fa;
const TW_BLUE_500: u32 = 0x3b82f6;
const TW_BLUE_900: u32 = 0x1e3a8a;
const TW_CYAN_500: u32 = 0x06b6d4;
const TW_EMERALD_400: u32 = 0x34d399;
const TW_EMERALD_800: u32 = 0x065f46;
const TW_EMERALD_900: u32 = 0x064e3b;
const TW_GREEN_400: u32 = 0x4ade80;
const TW_GREEN_500: u32 = 0x22c55e;
const TW_ORANGE_400: u32 = 0xfb923c;
const TW_ORANGE_500: u32 = 0xf97316;
const TW_PURPLE_400: u32 = 0xc084fc;
const TW_PURPLE_900: u32 = 0x581c87;
const TW_RED_400: u32 = 0xf87171;
const TW_RED_500: u32 = 0xef4444;
const TW_RED_900: u32 = 0x7f1d1d;
const TW_RED_950: u32 = 0x450a0a;
const TW_YELLOW_400: u32 = 0xfacc15;
const TW_YELLOW_900: u32 = 0x713f12;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(super) enum ForwardInput {
    CreateBindAddress,
    CreateBindPort,
    CreateTargetHost,
    CreateTargetPort,
    EditBindAddress,
    EditBindPort,
    EditTargetHost,
    EditTargetPort,
}

impl ForwardInput {
    pub(super) fn anchor_key(self) -> u64 {
        match self {
            Self::CreateBindAddress => 1,
            Self::CreateBindPort => 2,
            Self::CreateTargetHost => 3,
            Self::CreateTargetPort => 4,
            Self::EditBindAddress => 5,
            Self::EditBindPort => 6,
            Self::EditTargetHost => 7,
            Self::EditTargetPort => 8,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct ForwardsViewState {
    show_new_form: bool,
    editing_forward: Option<ForwardRule>,
    pending_delete_forward: Option<ForwardRule>,
    copied_forward_id: Option<String>,
    forward_type: ForwardType,
    bind_address: String,
    bind_port: String,
    target_host: String,
    target_port: String,
    skip_health_check: bool,
    edit_bind_address: String,
    edit_bind_port: String,
    edit_target_host: String,
    edit_target_port: String,
    pub(super) focused_input: Option<ForwardInput>,
    pub(super) error: Option<String>,
    pending: bool,
    detected_ports: Vec<DetectedPort>,
    new_ports: Vec<DetectedPort>,
    has_scanned_ports: bool,
    port_scan_pending: bool,
    port_scan_error: Option<String>,
    last_port_scan_started: Option<Instant>,
}

impl Default for ForwardsViewState {
    fn default() -> Self {
        Self {
            show_new_form: false,
            editing_forward: None,
            pending_delete_forward: None,
            copied_forward_id: None,
            forward_type: ForwardType::Local,
            bind_address: FORWARDS_DEFAULT_BIND_ADDRESS.to_string(),
            bind_port: String::new(),
            target_host: FORWARDS_DEFAULT_TARGET_HOST.to_string(),
            target_port: String::new(),
            skip_health_check: false,
            edit_bind_address: FORWARDS_DEFAULT_BIND_ADDRESS.to_string(),
            edit_bind_port: String::new(),
            edit_target_host: FORWARDS_DEFAULT_TARGET_HOST.to_string(),
            edit_target_port: String::new(),
            focused_input: None,
            error: None,
            pending: false,
            detected_ports: Vec::new(),
            new_ports: Vec::new(),
            has_scanned_ports: false,
            port_scan_pending: false,
            port_scan_error: None,
            last_port_scan_started: None,
        }
    }
}

pub(super) enum ForwardingWorkerResult {
    Operation {
        tab_id: TabId,
        message_key: &'static str,
        result: Result<(), String>,
    },
    PortScan {
        tab_id: TabId,
        result: Result<PortDetectionSnapshot, String>,
    },
}

impl WorkspaceApp {
    pub(super) fn open_forwards_tab(
        &mut self,
        node_id: NodeId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let node_title = self
            .ssh_nodes
            .get(&node_id)
            .map(|node| node.title.clone())
            .unwrap_or_else(|| node_id.0.clone());
        let title = format!("{} · {}", self.i18n.t("forwards.table.title"), node_title);
        let tab_id = if let Some((tab_id, _)) = self
            .forward_tab_nodes
            .iter()
            .find(|(_, existing_node_id)| *existing_node_id == &node_id)
        {
            *tab_id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::Forwards,
                title,
                title_source: TabTitleSource::Static,
                root_pane: None,
                active_pane_id: None,
            });
            self.forward_tab_nodes.insert(tab_id, node_id.clone());
            tab_id
        };

        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_sidebar_section = SidebarSection::Sessions;
        self.active_ssh_node_id = Some(node_id);
        self.forwarding_view.error = None;
        self.start_port_scan_for_forwards_tab(tab_id, cx);
        self.persist_sidebar_settings();
        cx.notify();
    }

    pub(super) fn render_forwards_surface(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(tab_id) = self.active_tab_id else {
            return self.render_empty_workspace(cx);
        };
        let Some(node_id) = self.forward_tab_nodes.get(&tab_id).cloned() else {
            return self.render_empty_workspace(cx);
        };
        let node_ready = self
            .ssh_nodes
            .get(&node_id)
            .is_some_and(|node| node.readiness == NodeReadiness::Ready);
        let manager = self.forwarding_manager_for_node_readonly(&node_id);
        let forwards = manager
            .as_ref()
            .map(|manager| manager.list_forwards())
            .unwrap_or_default();
        let forwards_for_remote_ports = forwards.clone();
        let has_background = self.terminal_background_preferences("forwards").is_some();

        let mut surface = div()
            .id("forwards-view-scroll")
            .size_full()
            .overflow_y_scroll()
            .p(px(FORWARDS_PAGE_PADDING))
            .bg(if has_background {
                forwards_transparent()
            } else {
                rgb(theme.bg)
            })
            .child(
                div()
                    .w_full()
                    .max_w(px(FORWARDS_MAX_WIDTH))
                    .mx_auto()
                    .flex()
                    .flex_col()
                    .gap(px(FORWARDS_SECTION_GAP))
                    .when(!self.forwarding_view.new_ports.is_empty(), |page| {
                        page.child(self.render_port_detection_banner(
                            node_id.clone(),
                            tab_id,
                            self.forwarding_view.new_ports.clone(),
                            has_background,
                            cx,
                        ))
                    })
                    .child(self.render_forwards_quick_actions(
                        node_id.clone(),
                        node_ready,
                        tab_id,
                        has_background,
                        cx,
                    ))
                    .child(self.render_forwards_separator(has_background))
                    .child(self.render_forwards_table(
                        node_id.clone(),
                        tab_id,
                        forwards,
                        manager,
                        has_background,
                        cx,
                    ))
                    .when(self.forwarding_view.show_new_form, |page| {
                        page.child(self.render_forward_create_form(
                            node_id.clone(),
                            tab_id,
                            has_background,
                            cx,
                        ))
                    })
                    .when_some(self.forwarding_view.error.as_ref(), |page, error| {
                        page.child(self.render_forwards_error(error))
                    })
                    .child(self.render_forwards_separator(has_background))
                    .child(self.render_remote_ports_section(
                        node_id.clone(),
                        tab_id,
                        &forwards_for_remote_ports,
                        has_background,
                        cx,
                    )),
            );
        if self.forwarding_view.editing_forward.is_some() {
            surface = surface.child(self.render_forward_edit_modal(
                node_id.clone(),
                tab_id,
                has_background,
                cx,
            ));
        }
        if self.forwarding_view.pending_delete_forward.is_some() {
            surface = surface.child(self.render_forward_delete_confirm(
                node_id,
                tab_id,
                has_background,
                cx,
            ));
        }
        surface.into_any_element()
    }

    fn render_forwards_quick_actions(
        &self,
        node_id: NodeId,
        node_ready: bool,
        tab_id: TabId,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.render_forwards_section_title(self.i18n.t("forwards.quick.title")))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap(px(8.0))
                    .child(self.render_forwards_quick_button(
                        "forwards.quick.jupyter",
                        TW_ORANGE_500,
                        node_id.clone(),
                        tab_id,
                        8888,
                        node_ready,
                        has_background,
                        cx,
                    ))
                    .child(self.render_forwards_quick_button(
                        "forwards.quick.tensorboard",
                        TW_BLUE_500,
                        node_id.clone(),
                        tab_id,
                        6006,
                        node_ready,
                        has_background,
                        cx,
                    ))
                    .child(self.render_forwards_quick_button(
                        "forwards.quick.vscode",
                        TW_CYAN_500,
                        node_id,
                        tab_id,
                        8080,
                        node_ready,
                        has_background,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_forwards_quick_button(
        &self,
        label_key: &'static str,
        dot_color: u32,
        node_id: NodeId,
        tab_id: TabId,
        port: u16,
        enabled: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_forward_button(
            self.i18n.t(label_key),
            None,
            ForwardButtonVariant::Secondary,
            enabled,
            has_background,
            cx.listener(move |this, _event, _window, cx| {
                let persist = this.forward_persist_context_for_node(&node_id);
                let registry = this.forwarding_registry.clone();
                this.start_forward_operation(
                    tab_id,
                    node_id.clone(),
                    "forwards.messages.created",
                    move |manager| {
                        Box::pin(async move {
                            let created = match label_key {
                                "forwards.quick.jupyter" => {
                                    manager.forward_jupyter(port, port).await?
                                }
                                "forwards.quick.tensorboard" => {
                                    manager.forward_tensorboard(port, port).await?
                                }
                                "forwards.quick.vscode" => {
                                    manager.forward_vscode(port, port).await?
                                }
                                _ => unreachable!("unknown forward quick action"),
                            };
                            if let Some((session_id, owner_connection_id)) = persist {
                                let forward_id = created.id.clone();
                                let _ = registry.sync_persisted_forward_rule(
                                    &forward_id,
                                    &session_id,
                                    owner_connection_id,
                                    created,
                                );
                            }
                            Ok(())
                        })
                    },
                    cx,
                );
                cx.stop_propagation();
            }),
        )
        .map(|button| {
            button.child(
                div()
                    .size(px(8.0))
                    .rounded_full()
                    .bg(forwards_palette_color(dot_color)),
            )
        })
        .into_any_element()
    }

    fn render_forwards_table(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        forwards: Vec<ForwardRule>,
        manager: Option<Arc<ForwardingManager>>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let forward_count = forwards.len();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(self.render_forwards_section_title(self.i18n.t("forwards.table.title")))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .child(self.render_forward_icon_button(
                                LucideIcon::RefreshCcw,
                                theme.text_muted,
                                has_background,
                                cx.listener(|_this, _event, _window, cx| {
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.render_forward_button(
                                self.i18n.t("forwards.actions.new_forward"),
                                Some(LucideIcon::Plus),
                                if self.forwarding_view.show_new_form {
                                    ForwardButtonVariant::Secondary
                                } else {
                                    ForwardButtonVariant::Primary
                                },
                                true,
                                has_background,
                                cx.listener(|this, _event, _window, cx| {
                                    this.forwarding_view.show_new_form =
                                        !this.forwarding_view.show_new_form;
                                    this.forwarding_view.error = None;
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .min_h(px(100.0))
                    .w_full()
                    .overflow_hidden()
                    .rounded(px(FORWARDS_CARD_RADIUS))
                    .border_1()
                    .border_color(forwards_theme_border(theme.border, has_background))
                    .bg(forwards_theme_card_bg(theme.bg_card, has_background))
                    .child(self.render_forward_table_header(has_background))
                    .when(forwards.is_empty(), |table| {
                        table.child(
                            div()
                                .h(px(120.0))
                                .flex()
                                .flex_col()
                                .items_center()
                                .justify_center()
                                .gap(px(12.0))
                                .rounded_b(px(FORWARDS_CARD_RADIUS))
                                .text_size(px(FORWARDS_TEXT_SM))
                                .text_color(rgb(theme.text_muted))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::ArrowUpDown,
                                    40.0,
                                    forwards_theme_with_alpha(
                                        theme.text_muted,
                                        FORWARDS_TW_ALPHA_30,
                                    ),
                                ))
                                .child(self.i18n.t("forwards.table.no_forwards")),
                        )
                    })
                    .children(forwards.into_iter().enumerate().map(|(index, rule)| {
                        let stats = matches!(rule.status, ForwardStatus::Active)
                            .then(|| {
                                manager
                                    .as_ref()
                                    .and_then(|manager| manager.get_stats(&rule.id).ok())
                            })
                            .flatten();
                        self.render_forward_row(
                            node_id.clone(),
                            tab_id,
                            rule,
                            stats,
                            index + 1 == forward_count,
                            has_background,
                            cx,
                        )
                    })),
            )
            .into_any_element()
    }

    fn render_forward_table_header(&self, has_background: bool) -> AnyElement {
        let theme = self.tokens.ui;
        self.forward_row_base(
            FORWARDS_TABLE_HEADER_H,
            forwards_theme_panel_bg(theme.bg_panel, has_background),
            ForwardRowCorners::Top,
        )
        .border_b_1()
        .border_color(forwards_theme_border(theme.border, has_background))
        .text_size(px(FORWARDS_TEXT_XS))
        .text_color(rgb(theme.text_muted))
        .child(self.forward_cell(0.9, self.i18n.t("forwards.table.type")))
        .child(self.forward_cell(1.35, self.i18n.t("forwards.table.local_address")))
        .child(self.forward_cell(1.35, self.i18n.t("forwards.table.remote_address")))
        .child(self.forward_cell(1.6, self.i18n.t("forwards.table.status")))
        .child(
            div()
                .w(px(128.0))
                .pr(px(16.0))
                .text_align(gpui::TextAlign::Right)
                .child(self.i18n.t("forwards.table.actions")),
        )
        .into_any_element()
    }

    fn render_forward_row(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        rule: ForwardRule,
        stats: Option<ForwardStats>,
        rounded_bottom: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (local, remote) = forward_addresses(&rule);
        let active = matches!(rule.status, ForwardStatus::Active);
        let stopped = matches!(rule.status, ForwardStatus::Stopped);
        let rule_for_stop = rule.clone();
        let rule_for_restart = rule.clone();
        let rule_for_delete = rule.clone();
        let rule_for_edit = rule.clone();

        self.forward_row_base(
            FORWARDS_TABLE_ROW_H,
            forwards_theme_sunken_bg(theme.bg_sunken, has_background),
            if rounded_bottom {
                ForwardRowCorners::Bottom
            } else {
                ForwardRowCorners::None
            },
        )
        .border_b_1()
        .border_color(forwards_theme_border_half(theme.border, has_background))
        .hover(move |row| row.bg(forwards_theme_hover_bg(theme.bg_hover, has_background)))
        .text_size(px(FORWARDS_TEXT_SM))
        .child(self.forward_cell_element(0.9, self.render_forward_type_badge(rule.forward_type)))
        .child(self.render_forward_address_cell(&rule, local, tab_id, cx))
        .child(self.forward_cell(1.35, remote))
        .child(self.forward_cell_element(1.6, self.render_forward_status(&rule.status, stats)))
        .child(
            div()
                .w(px(128.0))
                .pr(px(10.0))
                .flex()
                .justify_end()
                .gap(px(4.0))
                .when(active, |actions| {
                    actions.child(self.render_forward_icon_button(
                        LucideIcon::Square,
                        TW_YELLOW_400,
                        has_background,
                        cx.listener({
                            let node_id = node_id.clone();
                            move |this, _event, _window, cx| {
                                let forward_id = rule_for_stop.id.clone();
                                this.start_forward_operation(
                                    tab_id,
                                    node_id.clone(),
                                    "forwards.messages.stopped",
                                    move |manager| {
                                        Box::pin(async move {
                                            manager.stop_forward(&forward_id).await.map(|_| ())
                                        })
                                    },
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                    ))
                })
                .when(stopped, |actions| {
                    actions
                        .child(self.render_forward_icon_button(
                            LucideIcon::Play,
                            TW_GREEN_400,
                            has_background,
                            cx.listener({
                                let node_id = node_id.clone();
                                move |this, _event, _window, cx| {
                                    let forward_id = rule_for_restart.id.clone();
                                    let persist = this.forward_persist_context_for_node(&node_id);
                                    let registry = this.forwarding_registry.clone();
                                    this.start_forward_operation(
                                        tab_id,
                                        node_id.clone(),
                                        "forwards.messages.restarted",
                                        move |manager| {
                                            Box::pin(async move {
                                                let restarted =
                                                    manager.restart_forward(&forward_id).await?;
                                                if let Some((session_id, owner_connection_id)) =
                                                    persist
                                                {
                                                    let forward_id = restarted.id.clone();
                                                    let _ = registry.sync_persisted_forward_rule(
                                                        &forward_id,
                                                        &session_id,
                                                        owner_connection_id,
                                                        restarted,
                                                    );
                                                }
                                                Ok(())
                                            })
                                        },
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }
                            }),
                        ))
                        .child(self.render_forward_icon_button(
                            LucideIcon::Pencil,
                            TW_BLUE_400,
                            has_background,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_forward_edit_form(rule_for_edit.clone(), cx);
                                cx.stop_propagation();
                            }),
                        ))
                })
                .when(matches!(rule.status, ForwardStatus::Suspended), |actions| {
                    actions.child(
                        div()
                            .px_2()
                            .text_size(px(FORWARDS_TEXT_XS))
                            .text_color(forwards_palette_alpha(TW_ORANGE_400, FORWARDS_TW_ALPHA_50))
                            .child(self.i18n.t("forwards.actions.will_recover")),
                    )
                })
                .child(self.render_forward_icon_button(
                    LucideIcon::Trash2,
                    TW_RED_400,
                    has_background,
                    cx.listener(move |this, _event, _window, cx| {
                        this.forwarding_view.pending_delete_forward = Some(rule_for_delete.clone());
                        this.forwarding_view.error = None;
                        cx.notify();
                        cx.stop_propagation();
                    }),
                )),
        )
        .into_any_element()
    }

    fn render_forward_address_cell(
        &self,
        rule: &ForwardRule,
        address: String,
        tab_id: TabId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let should_copy = rule.forward_type != ForwardType::Remote
            && matches!(rule.status, ForwardStatus::Active);
        if !should_copy {
            return self.forward_cell(1.35, address);
        }

        let forward_id = rule.id.clone();
        let copied = self.forwarding_view.copied_forward_id.as_deref() == Some(&forward_id);
        self.forward_cell_element(
            1.35,
            div()
                .flex()
                .items_center()
                .gap(px(4.0))
                .truncate()
                .font_family(SharedString::from("monospace"))
                .text_color(rgb(self.tokens.ui.text))
                .hover({
                    let accent = self.tokens.ui.accent;
                    move |cell| cell.text_color(rgb(accent))
                })
                .cursor_pointer()
                .child(address.clone())
                .child(Self::render_lucide_icon(
                    if copied {
                        LucideIcon::Check
                    } else {
                        LucideIcon::Copy
                    },
                    12.0,
                    if copied {
                        forwards_palette_color(TW_GREEN_400)
                    } else {
                        rgb(self.tokens.ui.text_muted)
                    },
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(address.clone()));
                        this.forwarding_view.copied_forward_id = Some(forward_id.clone());
                        cx.notify();

                        let copied_forward_id = forward_id.clone();
                        cx.spawn(async move |weak, cx| {
                            Timer::after(Duration::from_secs(2)).await;
                            let _ = weak.update(cx, |this, cx| {
                                if this.forwarding_view.copied_forward_id.as_deref()
                                    == Some(copied_forward_id.as_str())
                                {
                                    this.forwarding_view.copied_forward_id = None;
                                    cx.notify();
                                }
                            });
                        })
                        .detach();
                        let _ = tab_id;
                        cx.stop_propagation();
                    }),
                )
                .into_any_element(),
        )
    }

    fn render_forward_create_form(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(FORWARDS_FORM_RADIUS))
            .border_1()
            .border_color(forwards_theme_border(theme.border, has_background))
            .bg(forwards_theme_with_alpha(
                theme.bg_panel,
                FORWARDS_TW_ALPHA_30,
            ))
            .p_4()
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(FORWARDS_TEXT_SM))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("forwards.form.new_title")),
                    )
                    .child(self.render_forward_button(
                        self.i18n.t("forwards.form.cancel"),
                        None,
                        ForwardButtonVariant::Ghost,
                        true,
                        has_background,
                        cx.listener(|this, _event, _window, cx| {
                            this.forwarding_view.show_new_form = false;
                            this.forwarding_view.error = None;
                            this.forwarding_view.focused_input = None;
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    )),
            )
            .child(self.render_forward_type_picker(has_background, cx))
            .child(self.render_forward_address_form(false, has_background, cx))
            .when(
                self.forwarding_view.forward_type != ForwardType::Dynamic,
                |form| form.child(self.render_forward_skip_health_check(has_background, cx)),
            )
            .when_some(self.forwarding_view.error.as_ref(), |form, error| {
                form.child(self.render_forwards_error(error))
            })
            .child(div().flex().justify_end().child(self.render_forward_button(
                if self.forwarding_view.pending {
                    if self.forwarding_view.skip_health_check {
                        self.i18n.t("forwards.form.creating")
                    } else {
                        self.i18n.t("forwards.form.checking_port")
                    }
                } else {
                    self.i18n.t("forwards.form.create_forward")
                },
                None,
                ForwardButtonVariant::Primary,
                !self.forwarding_view.pending,
                has_background,
                cx.listener(move |this, _event, _window, cx| {
                    this.submit_forward_create(tab_id, node_id.clone(), cx);
                    cx.stop_propagation();
                }),
            )))
            .into_any_element()
    }

    fn render_forward_edit_modal(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(editing) = self.forwarding_view.editing_forward.as_ref() else {
            return div().into_any_element();
        };

        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(forwards_palette_alpha(TW_BLACK, FORWARDS_TW_ALPHA_50))
            .child(
                div()
                    .w(px(500.0))
                    .rounded(px(FORWARDS_CARD_RADIUS))
                    .border_1()
                    .border_color(forwards_theme_border(theme.border, has_background))
                    .bg(forwards_theme_panel_bg(theme.bg_panel, has_background))
                    .p(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_size(px(FORWARDS_TEXT_SM))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("forwards.form.edit_title")),
                            )
                            .child(self.render_forward_icon_button(
                                LucideIcon::X,
                                theme.text_muted,
                                has_background,
                                cx.listener(|this, _event, _window, cx| {
                                    this.forwarding_view.editing_forward = None;
                                    this.forwarding_view.focused_input = None;
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(FORWARDS_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(format!(
                                "{}: {} | ID: {}...",
                                self.i18n.t("forwards.form.type"),
                                forward_type_label(editing.clone(), &self.i18n),
                                editing.id.chars().take(8).collect::<String>()
                            )),
                    )
                    .child(self.render_forward_address_form(true, has_background, cx))
                    .when_some(self.forwarding_view.error.as_ref(), |modal, error| {
                        modal.child(self.render_forwards_error(error))
                    })
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.render_forward_button(
                                self.i18n.t("forwards.form.cancel"),
                                None,
                                ForwardButtonVariant::Ghost,
                                true,
                                has_background,
                                cx.listener(|this, _event, _window, cx| {
                                    this.forwarding_view.editing_forward = None;
                                    this.forwarding_view.focused_input = None;
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.render_forward_button(
                                self.i18n.t("forwards.form.save_changes"),
                                None,
                                ForwardButtonVariant::Primary,
                                !self.forwarding_view.pending,
                                has_background,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.submit_forward_edit(tab_id, node_id.clone(), cx);
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_forward_delete_confirm(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some(rule) = self.forwarding_view.pending_delete_forward.as_ref() else {
            return div().into_any_element();
        };
        let forward_id = rule.id.clone();
        let confirm_id = forward_id.clone();
        div()
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .bottom_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(forwards_palette_alpha(TW_BLACK, FORWARDS_TW_ALPHA_50))
            .child(
                div()
                    .w(px(420.0))
                    .rounded(px(FORWARDS_CARD_RADIUS))
                    .border_1()
                    .border_color(forwards_theme_border(theme.border, has_background))
                    .bg(forwards_theme_panel_bg(theme.bg_panel, has_background))
                    .p(px(20.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(
                        div()
                            .text_size(px(FORWARDS_TEXT_SM))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("forwards.actions.confirm_delete_title")),
                    )
                    .child(
                        div()
                            .text_size(px(FORWARDS_TEXT_XS))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("forwards.actions.confirm_delete_desc")),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap(px(8.0))
                            .child(self.render_forward_button(
                                self.i18n.t("forwards.form.cancel"),
                                None,
                                ForwardButtonVariant::Ghost,
                                true,
                                has_background,
                                cx.listener(|this, _event, _window, cx| {
                                    this.forwarding_view.pending_delete_forward = None;
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.render_forward_button(
                                self.i18n.t("forwards.actions.delete"),
                                Some(LucideIcon::Trash2),
                                ForwardButtonVariant::Danger,
                                true,
                                has_background,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.forwarding_view.pending_delete_forward = None;
                                    let registry = this.forwarding_registry.clone();
                                    let delete_id = confirm_id.clone();
                                    this.start_forward_operation(
                                        tab_id,
                                        node_id.clone(),
                                        "forwards.messages.deleted",
                                        move |manager| {
                                            Box::pin(async move {
                                                manager.delete_forward(&delete_id).await?;
                                                let _ =
                                                    registry.delete_persisted_forward(&delete_id);
                                                Ok(())
                                            })
                                        },
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_forward_type_picker(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .gap(px(16.0))
            .child(self.render_forward_type_choice(
                ForwardType::Local,
                "forwards.form.type_local",
                has_background,
                cx,
            ))
            .child(self.render_forward_type_choice(
                ForwardType::Remote,
                "forwards.form.type_remote",
                has_background,
                cx,
            ))
            .child(self.render_forward_type_choice(
                ForwardType::Dynamic,
                "forwards.form.type_dynamic",
                has_background,
                cx,
            ))
            .into_any_element()
    }

    fn render_forward_type_choice(
        &self,
        forward_type: ForwardType,
        label_key: &'static str,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let selected = self.forwarding_view.forward_type == forward_type;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .text_size(px(FORWARDS_TEXT_SM))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .size(px(14.0))
                    .rounded_full()
                    .border_1()
                    .border_color(if selected {
                        rgb(theme.accent)
                    } else {
                        forwards_theme_border(theme.border, has_background)
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(selected, |radio| {
                        radio.child(div().size(px(8.0)).rounded_full().bg(rgb(theme.accent)))
                    }),
            )
            .child(self.i18n.t(label_key))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.forwarding_view.forward_type = forward_type;
                    this.forwarding_view.error = None;
                    if forward_type == ForwardType::Dynamic {
                        this.forwarding_view.skip_health_check = false;
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_forward_skip_health_check(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let checked = self.forwarding_view.skip_health_check;
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px_2()
            .cursor_pointer()
            .text_size(px(FORWARDS_TEXT_XS))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .size(px(14.0))
                    .rounded(px(FORWARDS_FORM_RADIUS))
                    .border_1()
                    .border_color(if checked {
                        rgb(theme.accent)
                    } else {
                        forwards_theme_border(theme.border, has_background)
                    })
                    .bg(if checked {
                        rgb(theme.accent)
                    } else {
                        forwards_transparent()
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(checked, |checkbox| {
                        checkbox.child(Self::render_lucide_icon(
                            LucideIcon::Check,
                            11.0,
                            rgb(theme.accent_text),
                        ))
                    }),
            )
            .child(self.i18n.t("forwards.form.skip_check"))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.forwarding_view.skip_health_check =
                        !this.forwarding_view.skip_health_check;
                    this.forwarding_view.error = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_forward_address_form(
        &self,
        editing: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let forward_type = if editing {
            self.forwarding_view
                .editing_forward
                .as_ref()
                .map(|rule| rule.forward_type)
                .unwrap_or(ForwardType::Local)
        } else {
            self.forwarding_view.forward_type
        };

        div()
            .flex()
            .items_center()
            .gap(px(16.0))
            .p_4()
            .rounded(px(FORWARDS_FORM_RADIUS))
            .border_1()
            .border_color(forwards_theme_border_half(theme.border, has_background))
            .bg(forwards_theme_sunken_bg(theme.bg_sunken, has_background))
            .child(self.render_forward_address_side(
                if forward_type == ForwardType::Remote {
                    self.i18n.t("forwards.form.remote_server")
                } else {
                    self.i18n.t("forwards.form.local_client")
                },
                if editing {
                    ForwardInput::EditBindAddress
                } else {
                    ForwardInput::CreateBindAddress
                },
                if editing {
                    ForwardInput::EditBindPort
                } else {
                    ForwardInput::CreateBindPort
                },
                cx,
            ))
            .child(
                div()
                    .pt(px(22.0))
                    .text_size(px(18.0))
                    .text_color(rgb(theme.text_muted))
                    .child("→"),
            )
            .child(if forward_type == ForwardType::Dynamic {
                div()
                    .flex_1()
                    .pt(px(22.0))
                    .text_center()
                    .italic()
                    .text_size(px(FORWARDS_TEXT_SM))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("forwards.form.socks5_mode"))
                    .into_any_element()
            } else {
                self.render_forward_address_side(
                    if forward_type == ForwardType::Remote {
                        self.i18n.t("forwards.form.local_client")
                    } else {
                        self.i18n.t("forwards.form.remote_server")
                    },
                    if editing {
                        ForwardInput::EditTargetHost
                    } else {
                        ForwardInput::CreateTargetHost
                    },
                    if editing {
                        ForwardInput::EditTargetPort
                    } else {
                        ForwardInput::CreateTargetPort
                    },
                    cx,
                )
            })
            .into_any_element()
    }

    fn render_forward_address_side(
        &self,
        label: String,
        host_input: ForwardInput,
        port_input: ForwardInput,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(FORWARDS_TEXT_XS))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(label),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(self.render_forward_text_input(
                        host_input,
                        self.i18n.t("forwards.form.host_placeholder"),
                        true,
                        cx,
                    ))
                    .child(div().w(px(96.0)).child(self.render_forward_text_input(
                        port_input,
                        self.i18n.t("forwards.form.port_placeholder"),
                        true,
                        cx,
                    ))),
            )
            .into_any_element()
    }

    fn render_forward_text_input(
        &self,
        input: ForwardInput,
        placeholder: String,
        fill: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace = cx.entity();
        let focused = self.forwarding_view.focused_input == Some(input);
        let value = self.forward_input_value(input);
        let target = WorkspaceImeTarget::Forwards(input);
        text_input_anchor_probe(
            target.anchor_id(),
            div()
                .when(fill, |wrapper| wrapper.w_full())
                .child(text_input(
                    &self.tokens,
                    TextInputView {
                        value,
                        placeholder,
                        focused,
                        caret_visible: self.new_connection_caret_visible,
                        secret: false,
                        selected_all: false,
                        marked_text: self.marked_text_for_target(target),
                    },
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        this.forwarding_view.focused_input = Some(input);
                        this.ime_marked_text = None;
                        this.needs_active_pane_focus = false;
                        window.focus(&this.focus_handle);
                        cx.notify();
                        cx.stop_propagation();
                    }),
                ),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_forward_type_badge(&self, forward_type: ForwardType) -> AnyElement {
        let (bg, text) = match forward_type {
            ForwardType::Local => (TW_BLUE_900, TW_BLUE_400),
            ForwardType::Remote => (TW_PURPLE_900, TW_PURPLE_400),
            ForwardType::Dynamic => (TW_YELLOW_900, TW_YELLOW_400),
        };
        div()
            .h(px(FORWARDS_TYPE_BADGE_H))
            .px_2()
            .flex()
            .items_center()
            .rounded(px(4.0))
            .bg(forwards_palette_alpha(bg, FORWARDS_TW_ALPHA_30))
            .text_size(px(FORWARDS_TEXT_XS))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(forwards_palette_color(text))
            .child(forward_type_key(forward_type, &self.i18n))
            .into_any_element()
    }

    fn render_forward_status(
        &self,
        status: &ForwardStatus,
        stats: Option<ForwardStats>,
    ) -> AnyElement {
        let (dot, text_color) = match status {
            ForwardStatus::Active => (TW_GREEN_500, self.tokens.ui.text_muted),
            ForwardStatus::Stopped => (self.tokens.ui.text_muted, self.tokens.ui.text_muted),
            ForwardStatus::Suspended => (TW_ORANGE_500, TW_ORANGE_400),
            ForwardStatus::Starting => (TW_BLUE_500, self.tokens.ui.text_muted),
            ForwardStatus::Error(_) => (TW_RED_500, self.tokens.ui.text_muted),
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(FORWARDS_TEXT_XS))
            .text_color(rgb(text_color))
            .child(
                div()
                    .size(px(8.0))
                    .rounded_full()
                    .bg(forwards_palette_color(dot)),
            )
            .child(self.i18n.t(forward_status_key(status)))
            .when_some(stats, |row, stats| {
                row.child(
                    div()
                        .ml_2()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Activity,
                            12.0,
                            rgb(self.tokens.ui.text_muted),
                        ))
                        .child(format!(
                            "{}/{} | ↑{} ↓{}",
                            stats.active_connections,
                            stats.connection_count,
                            format_bytes(stats.bytes_sent),
                            format_bytes(stats.bytes_received)
                        )),
                )
            })
            .into_any_element()
    }

    fn render_forward_button(
        &self,
        label: String,
        icon: Option<LucideIcon>,
        variant: ForwardButtonVariant,
        enabled: bool,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        let theme = self.tokens.ui;
        let (bg, border, text, hover_bg) = match variant {
            ForwardButtonVariant::Primary => (
                rgb(theme.accent),
                rgb(theme.accent),
                theme.accent_text,
                rgb(theme.accent_hover),
            ),
            ForwardButtonVariant::Secondary => (
                forwards_theme_hover_bg(theme.bg_hover, has_background),
                forwards_theme_border(theme.border, has_background),
                theme.text,
                forwards_theme_bg(theme.bg_active, has_background),
            ),
            ForwardButtonVariant::Ghost => (
                forwards_theme_panel_bg(theme.bg_panel, has_background),
                forwards_theme_panel_bg(theme.bg_panel, has_background),
                theme.text_muted,
                forwards_theme_hover_bg(theme.bg_hover, has_background),
            ),
            ForwardButtonVariant::Danger => (
                forwards_palette_color(TW_RED_500),
                forwards_palette_color(TW_RED_500),
                theme.accent_text,
                forwards_palette_color(TW_RED_400),
            ),
        };
        div()
            .h(px(32.0))
            .px_3()
            .flex()
            .items_center()
            .gap(px(6.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(px(FORWARDS_TEXT_SM))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(text))
            .opacity(if enabled { 1.0 } else { 0.5 })
            .when(enabled, |button| {
                button
                    .cursor_pointer()
                    .hover(move |button| button.bg(hover_bg))
                    .on_mouse_down(MouseButton::Left, listener)
            })
            .when_some(icon, |button, icon| {
                button.child(Self::render_lucide_icon(icon, 14.0, rgb(text)))
            })
            .child(label)
    }

    fn render_forward_icon_button(
        &self,
        icon: LucideIcon,
        color: u32,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        div()
            .size(px(28.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .hover(move |button| {
                button.bg(forwards_theme_hover_bg(
                    self.tokens.ui.bg_hover,
                    has_background,
                ))
            })
            .child(Self::render_lucide_icon(
                icon,
                13.0,
                forwards_palette_color(color),
            ))
            .on_mouse_down(MouseButton::Left, listener)
            .into_any_element()
    }

    fn render_forwards_section_title(&self, label: String) -> AnyElement {
        div()
            .text_size(px(FORWARDS_TEXT_XS))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(label.to_uppercase())
            .into_any_element()
    }

    fn render_forwards_separator(&self, has_background: bool) -> AnyElement {
        div()
            .h(px(1.0))
            .w_full()
            .bg(forwards_theme_border(self.tokens.ui.border, has_background))
            .into_any_element()
    }

    fn render_forwards_error(&self, error: &str) -> AnyElement {
        div()
            .rounded(px(FORWARDS_FORM_RADIUS))
            .border_1()
            .border_color(forwards_palette_alpha(TW_RED_900, FORWARDS_TW_ALPHA_50))
            .bg(forwards_palette_alpha(TW_RED_950, FORWARDS_TW_ALPHA_30))
            .p_3()
            .text_size(px(FORWARDS_TEXT_XS))
            .text_color(forwards_palette_color(TW_RED_400))
            .child(error.to_string())
            .into_any_element()
    }

    fn render_port_detection_banner(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        new_ports: Vec<DetectedPort>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .children(new_ports.into_iter().map(|port| {
                let dismiss_port = port.port;
                let forward_port = port.clone();
                let forward_node_id = node_id.clone();
                div()
                    .min_h(px(36.0))
                    .w_full()
                    .px_3()
                    .py_2()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(forwards_palette_alpha(TW_BLUE_500, FORWARDS_TW_ALPHA_30))
                    .bg(forwards_palette_alpha(TW_BLUE_500, FORWARDS_TW_ALPHA_05))
                    .text_size(px(FORWARDS_TEXT_SM))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Radio,
                                14.0,
                                forwards_palette_color(TW_BLUE_400),
                            ))
                            .child(div().truncate().child(format!(
                                "{} :{}{}",
                                self.i18n.t("forwards.detection.detected"),
                                port.port,
                                port.process_name
                                    .as_ref()
                                    .map(|process| format!(
                                        " ({process}{})",
                                        port.pid
                                            .map(|pid| format!(" #{pid}"))
                                            .unwrap_or_default()
                                    ))
                                    .unwrap_or_default()
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(self.render_forward_button(
                                self.i18n.t("forwards.detection.forward"),
                                Some(LucideIcon::ArrowRight),
                                ForwardButtonVariant::Ghost,
                                true,
                                has_background,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.create_local_forward_for_detected_port(
                                        tab_id,
                                        forward_node_id.clone(),
                                        forward_port.clone(),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            ))
                            .child(self.render_forward_icon_button(
                                LucideIcon::X,
                                self.tokens.ui.text_muted,
                                has_background,
                                cx.listener(move |this, _event, _window, cx| {
                                    this.dismiss_detected_port(dismiss_port);
                                    cx.notify();
                                    cx.stop_propagation();
                                }),
                            )),
                    )
            }))
            .into_any_element()
    }

    fn render_remote_ports_section(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        forwards: &[ForwardRule],
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let visible_ports: Vec<DetectedPort> = self
            .forwarding_view
            .detected_ports
            .iter()
            .filter(|port| port.port != 22)
            .cloned()
            .collect();
        let visible_port_count = visible_ports.len();
        let forwarded_ports: std::collections::HashSet<u16> = forwards
            .iter()
            .filter(|rule| {
                rule.forward_type == ForwardType::Local
                    && matches!(rule.status, ForwardStatus::Active | ForwardStatus::Starting)
            })
            .map(|rule| rule.target_port)
            .collect();
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Radio,
                        16.0,
                        forwards_palette_color(TW_EMERALD_400),
                    ))
                    .child(self.render_forwards_section_title(
                        self.i18n.t("forwards.detection.remotePorts"),
                    ))
                    .when(!visible_ports.is_empty(), |header| {
                        header.child(
                            div()
                                .text_size(px(FORWARDS_TEXT_XS))
                                .text_color(rgb(theme.text_muted))
                                .child(format!("({})", visible_ports.len())),
                        )
                    }),
            )
            .child(
                div()
                    .rounded(px(FORWARDS_CARD_RADIUS))
                    .border_1()
                    .border_color(forwards_theme_border(theme.border, has_background))
                    .overflow_hidden()
                    .bg(forwards_theme_card_bg(theme.bg_card, has_background))
                    .child(
                        self.forward_row_base(
                            FORWARDS_TABLE_HEADER_H,
                            forwards_theme_panel_bg(theme.bg_panel, has_background),
                            ForwardRowCorners::Top,
                        )
                        .border_b_1()
                        .border_color(forwards_theme_border(theme.border, has_background))
                        .text_size(px(FORWARDS_TEXT_XS))
                        .text_color(rgb(theme.text_muted))
                        .child(self.forward_cell(1.0, self.i18n.t("forwards.detection.port")))
                        .child(self.forward_cell(1.4, self.i18n.t("forwards.detection.bindAddr")))
                        .child(self.forward_cell(1.4, self.i18n.t("forwards.detection.process")))
                        .child(
                            div()
                                .w(px(128.0))
                                .pr(px(16.0))
                                .text_align(gpui::TextAlign::Right)
                                .child(self.i18n.t("forwards.detection.action")),
                        ),
                    )
                    .when(visible_ports.is_empty(), |table| {
                        table.child(
                            div()
                                .h(px(72.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded_b(px(FORWARDS_CARD_RADIUS))
                                .text_size(px(FORWARDS_TEXT_XS))
                                .text_color(rgb(theme.text_muted))
                                .child(if self.forwarding_view.port_scan_pending {
                                    self.i18n.t("forwards.detection.scanning")
                                } else if let Some(error) =
                                    self.forwarding_view.port_scan_error.as_ref()
                                {
                                    error.clone()
                                } else if self.forwarding_view.has_scanned_ports {
                                    self.i18n.t("forwards.detection.noPorts")
                                } else {
                                    self.i18n.t("forwards.detection.scanning")
                                }),
                        )
                    })
                    .children(visible_ports.into_iter().enumerate().map(|(index, port)| {
                        let already_forwarded = forwarded_ports.contains(&port.port);
                        self.render_detected_port_row(
                            node_id.clone(),
                            tab_id,
                            port,
                            already_forwarded,
                            index + 1 == visible_port_count,
                            has_background,
                            cx,
                        )
                    })),
            )
            .into_any_element()
    }

    fn render_detected_port_row(
        &self,
        node_id: NodeId,
        tab_id: TabId,
        port: DetectedPort,
        already_forwarded: bool,
        rounded_bottom: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let forward_port = port.clone();
        self.forward_row_base(
            FORWARDS_TABLE_ROW_H,
            forwards_theme_sunken_bg(theme.bg_sunken, has_background),
            if rounded_bottom {
                ForwardRowCorners::Bottom
            } else {
                ForwardRowCorners::None
            },
        )
        .border_b_1()
        .border_color(forwards_theme_border_half(theme.border, has_background))
        .hover(move |row| row.bg(forwards_theme_hover_bg(theme.bg_hover, has_background)))
        .text_size(px(FORWARDS_TEXT_XS))
        .child(
            self.forward_cell_element(
                1.0,
                div()
                    .font_family(SharedString::from("monospace"))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(forwards_palette_color(TW_EMERALD_400))
                    .child(port.port.to_string())
                    .into_any_element(),
            ),
        )
        .child(
            self.forward_cell_element(
                1.4,
                div()
                    .truncate()
                    .font_family(SharedString::from("monospace"))
                    .text_color(rgb(theme.text_muted))
                    .child(if port.bind_addr.is_empty() {
                        "0.0.0.0".to_string()
                    } else {
                        port.bind_addr.clone()
                    })
                    .into_any_element(),
            ),
        )
        .child(
            self.forward_cell_element(
                1.4,
                div()
                    .truncate()
                    .text_color(rgb(theme.text_muted))
                    .child(match (port.process_name.clone(), port.pid) {
                        (Some(process), Some(pid)) => format!("{process} ({pid})"),
                        (Some(process), None) => process,
                        (None, _) => "—".to_string(),
                    })
                    .into_any_element(),
            ),
        )
        .child(
            div()
                .w(px(128.0))
                .pr(px(16.0))
                .flex()
                .justify_end()
                .child(if already_forwarded {
                    div()
                        .h(px(22.0))
                        .px_2()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .rounded(px(4.0))
                        .border_1()
                        .border_color(forwards_palette_alpha(TW_EMERALD_800, FORWARDS_TW_ALPHA_40))
                        .bg(forwards_palette_alpha(TW_EMERALD_900, FORWARDS_TW_ALPHA_30))
                        .text_size(px(FORWARDS_TEXT_XS))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(forwards_palette_color(TW_EMERALD_400))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Activity,
                            12.0,
                            forwards_palette_color(TW_EMERALD_400),
                        ))
                        .child(self.i18n.t("forwards.detection.alreadyForwarded"))
                        .into_any_element()
                } else {
                    self.render_forward_button(
                        self.i18n.t("forwards.detection.forward"),
                        Some(LucideIcon::Play),
                        ForwardButtonVariant::Ghost,
                        true,
                        has_background,
                        cx.listener(move |this, _event, _window, cx| {
                            this.create_local_forward_for_detected_port(
                                tab_id,
                                node_id.clone(),
                                forward_port.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .h(px(24.0))
                    .text_size(px(FORWARDS_TEXT_XS))
                    .into_any_element()
                }),
        )
        .into_any_element()
    }

    fn forward_row_base(
        &self,
        height: f32,
        bg: gpui::Rgba,
        corners: ForwardRowCorners,
    ) -> gpui::Div {
        div()
            .h(px(height))
            .w_full()
            .flex()
            .items_center()
            .bg(bg)
            .when(matches!(corners, ForwardRowCorners::Top), |row| {
                row.rounded_t(px(FORWARDS_CARD_RADIUS))
            })
            .when(matches!(corners, ForwardRowCorners::Bottom), |row| {
                row.rounded_b(px(FORWARDS_CARD_RADIUS))
            })
    }

    fn forward_cell(&self, flex: f32, text: String) -> AnyElement {
        self.forward_cell_element(
            flex,
            div()
                .truncate()
                .font_family(SharedString::from("monospace"))
                .child(text)
                .into_any_element(),
        )
    }

    fn forward_cell_element(&self, flex: f32, child: AnyElement) -> AnyElement {
        div()
            .flex_grow()
            .flex_basis(px(0.0))
            .min_w(px(0.0))
            .px_4()
            .when(flex > 1.0, |cell| cell.flex_grow())
            .child(child)
            .into_any_element()
    }

    fn submit_forward_create(&mut self, tab_id: TabId, node_id: NodeId, cx: &mut Context<Self>) {
        let forward_type = self.forwarding_view.forward_type;
        let bind_port_value = self.forwarding_view.bind_port.clone();
        let target_port_value = self.forwarding_view.target_port.clone();
        let Some((bind_port, target_port)) =
            self.validate_forward_form(forward_type, &bind_port_value, &target_port_value)
        else {
            cx.notify();
            return;
        };
        let rule = match forward_type {
            ForwardType::Local => ForwardRule::local(
                self.forwarding_view.bind_address.clone(),
                bind_port,
                self.forwarding_view.target_host.clone(),
                target_port.unwrap_or(0),
            ),
            ForwardType::Remote => ForwardRule::remote(
                self.forwarding_view.bind_address.clone(),
                bind_port,
                self.forwarding_view.target_host.clone(),
                target_port.unwrap_or(0),
            ),
            ForwardType::Dynamic => {
                ForwardRule::dynamic(self.forwarding_view.bind_address.clone(), bind_port)
            }
        };
        let check_health = !self.forwarding_view.skip_health_check;
        let persist = self.forward_persist_context_for_node(&node_id);
        let registry = self.forwarding_registry.clone();
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
            move |manager| {
                Box::pin(async move {
                    let created = manager
                        .create_forward_with_health_check(rule, check_health)
                        .await?;
                    if let Some((session_id, owner_connection_id)) = persist {
                        let forward_id = created.id.clone();
                        let _ = registry.sync_persisted_forward_rule(
                            &forward_id,
                            &session_id,
                            owner_connection_id,
                            created,
                        );
                    }
                    Ok(())
                })
            },
            cx,
        );
    }

    fn create_local_forward_for_detected_port(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        port: DetectedPort,
        cx: &mut Context<Self>,
    ) {
        let mut rule = ForwardRule::local(
            FORWARDS_DEFAULT_BIND_ADDRESS,
            port.port,
            FORWARDS_DEFAULT_TARGET_HOST,
            port.port,
        );
        rule.description = port
            .process_name
            .as_ref()
            .map(|process| format!("{process} ({})", self.i18n.t("forwards.detection.auto")))
            .unwrap_or_else(|| {
                format!(
                    "{} {} ({})",
                    self.i18n.t("forwards.detection.port"),
                    port.port,
                    self.i18n.t("forwards.detection.auto")
                )
            });
        self.dismiss_detected_port(port.port);
        let persist = self.forward_persist_context_for_node(&node_id);
        let registry = self.forwarding_registry.clone();
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.created",
            move |manager| {
                Box::pin(async move {
                    let created = manager.create_forward_with_health_check(rule, true).await?;
                    if let Some((session_id, owner_connection_id)) = persist {
                        let forward_id = created.id.clone();
                        let _ = registry.sync_persisted_forward_rule(
                            &forward_id,
                            &session_id,
                            owner_connection_id,
                            created,
                        );
                    }
                    Ok(())
                })
            },
            cx,
        );
    }

    fn dismiss_detected_port(&mut self, port: u16) {
        self.forwarding_view
            .new_ports
            .retain(|detected| detected.port != port);
        if let Some(tab_id) = self.active_tab_id
            && let Some(node_id) = self.forward_tab_nodes.get(&tab_id)
            && let Some(manager) = self.forwarding_manager_for_node_readonly(node_id)
        {
            manager.ignore_detected_port(port);
        }
    }

    fn submit_forward_edit(&mut self, tab_id: TabId, node_id: NodeId, cx: &mut Context<Self>) {
        let Some(editing) = self.forwarding_view.editing_forward.clone() else {
            return;
        };
        let edit_bind_port = self.forwarding_view.edit_bind_port.clone();
        let edit_target_port = self.forwarding_view.edit_target_port.clone();
        let Some((bind_port, target_port)) =
            self.validate_forward_form(editing.forward_type, &edit_bind_port, &edit_target_port)
        else {
            cx.notify();
            return;
        };
        let update = ForwardUpdate {
            bind_address: Some(self.forwarding_view.edit_bind_address.clone()),
            bind_port: Some(bind_port),
            target_host: (editing.forward_type != ForwardType::Dynamic)
                .then(|| self.forwarding_view.edit_target_host.clone()),
            target_port,
            ..ForwardUpdate::default()
        };
        let forward_id = editing.id;
        let persist = self.forward_persist_context_for_node(&node_id);
        let registry = self.forwarding_registry.clone();
        self.start_forward_operation(
            tab_id,
            node_id,
            "forwards.messages.updated",
            move |manager| {
                Box::pin(async move {
                    let updated = manager.update_stopped_forward(&forward_id, update)?;
                    if let Some((session_id, owner_connection_id)) = persist {
                        let forward_id = updated.id.clone();
                        let _ = registry.sync_persisted_forward_rule(
                            &forward_id,
                            &session_id,
                            owner_connection_id,
                            updated,
                        );
                    }
                    Ok(())
                })
            },
            cx,
        );
    }

    fn validate_forward_form(
        &mut self,
        forward_type: ForwardType,
        bind_port: &str,
        target_port: &str,
    ) -> Option<(u16, Option<u16>)> {
        let Some(bind_port) = parse_port(bind_port) else {
            self.forwarding_view.error = Some(self.i18n.t(if bind_port.trim().is_empty() {
                "forwards.form.port_required"
            } else {
                "forwards.form.port_invalid"
            }));
            return None;
        };
        if forward_type == ForwardType::Dynamic {
            self.forwarding_view.error = None;
            return Some((bind_port, None));
        }
        let Some(target_port) = parse_port(target_port) else {
            self.forwarding_view.error = Some(self.i18n.t(if target_port.trim().is_empty() {
                "forwards.form.port_required"
            } else {
                "forwards.form.port_invalid"
            }));
            return None;
        };
        self.forwarding_view.error = None;
        Some((bind_port, Some(target_port)))
    }

    fn start_forward_operation<F>(
        &mut self,
        tab_id: TabId,
        node_id: NodeId,
        message_key: &'static str,
        operation: F,
        cx: &mut Context<Self>,
    ) where
        F: FnOnce(
                Arc<ForwardingManager>,
            ) -> std::pin::Pin<
                Box<
                    dyn std::future::Future<
                            Output = Result<(), oxideterm_forwarding::ForwardingError>,
                        > + Send,
                >,
            > + Send
            + 'static,
    {
        let manager = match self.forwarding_manager_for_node(&node_id, cx) {
            Ok(manager) => manager,
            Err(error) => {
                self.forwarding_view.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.forwarding_view.pending = true;
        self.forwarding_view.error = None;
        let tx = self.forwarding_worker_tx.clone();
        thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime
                    .block_on(operation(manager))
                    .map_err(|error| error.to_string()),
                Err(error) => Err(format!("failed to initialize forwarding runtime: {error}")),
            };
            let _ = tx.send(ForwardingWorkerResult::Operation {
                tab_id,
                message_key,
                result,
            });
        });
        cx.notify();
    }

    fn start_port_scan_for_forwards_tab(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(node_id) = self.forward_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        self.start_port_scan(tab_id, node_id, cx);
    }

    pub(super) fn maybe_start_forwards_port_scan(&mut self, cx: &mut Context<Self>) {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        if self
            .tabs
            .iter()
            .find(|tab| tab.id == tab_id)
            .is_none_or(|tab| tab.kind != TabKind::Forwards)
        {
            return;
        }
        if self.forwarding_view.port_scan_pending {
            return;
        }
        let due = self
            .forwarding_view
            .last_port_scan_started
            .is_none_or(|last| last.elapsed() >= FORWARDS_PORT_SCAN_INTERVAL);
        if due {
            self.start_port_scan_for_forwards_tab(tab_id, cx);
        }
    }

    fn start_port_scan(&mut self, tab_id: TabId, node_id: NodeId, cx: &mut Context<Self>) {
        if self.forwarding_view.port_scan_pending {
            return;
        }
        let manager = match self.forwarding_manager_for_node(&node_id, cx) {
            Ok(manager) => manager,
            Err(error) => {
                self.forwarding_view.port_scan_error = Some(error);
                self.forwarding_view.has_scanned_ports = true;
                cx.notify();
                return;
            }
        };

        self.forwarding_view.port_scan_pending = true;
        self.forwarding_view.port_scan_error = None;
        self.forwarding_view.last_port_scan_started = Some(Instant::now());
        let tx = self.forwarding_worker_tx.clone();
        thread::spawn(move || {
            let result = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime
                    .block_on(manager.scan_remote_ports())
                    .map_err(|error| error.to_string()),
                Err(error) => Err(format!("failed to initialize forwarding runtime: {error}")),
            };
            let _ = tx.send(ForwardingWorkerResult::PortScan { tab_id, result });
        });
        cx.notify();
    }

    pub(super) fn poll_forwarding_worker_results(&mut self, cx: &mut Context<Self>) {
        let mut results = Vec::new();
        while let Ok(result) = self.forwarding_worker_rx.try_recv() {
            results.push(result);
        }
        for result in results {
            match result {
                ForwardingWorkerResult::Operation {
                    tab_id,
                    message_key,
                    result,
                } => {
                    if Some(tab_id) == self.active_tab_id {
                        self.forwarding_view.pending = false;
                        match result {
                            Ok(()) => {
                                let _ = message_key;
                                self.forwarding_view.error = None;
                                self.forwarding_view.show_new_form = false;
                                self.forwarding_view.skip_health_check = false;
                                self.forwarding_view.editing_forward = None;
                                self.forwarding_view.focused_input = None;
                            }
                            Err(error) => self.forwarding_view.error = Some(error),
                        }
                        cx.notify();
                    }
                }
                ForwardingWorkerResult::PortScan { tab_id, result } => {
                    if Some(tab_id) == self.active_tab_id {
                        self.forwarding_view.port_scan_pending = false;
                        match result {
                            Ok(snapshot) => {
                                self.forwarding_view.has_scanned_ports = snapshot.has_scanned;
                                self.forwarding_view.detected_ports = snapshot.all_ports;
                                if !snapshot.new_ports.is_empty() {
                                    let existing: std::collections::HashSet<u16> = self
                                        .forwarding_view
                                        .new_ports
                                        .iter()
                                        .map(|port| port.port)
                                        .collect();
                                    self.forwarding_view.new_ports.extend(
                                        snapshot
                                            .new_ports
                                            .into_iter()
                                            .filter(|port| !existing.contains(&port.port)),
                                    );
                                }
                                if !snapshot.closed_ports.is_empty() {
                                    let closed: std::collections::HashSet<u16> = snapshot
                                        .closed_ports
                                        .iter()
                                        .map(|port| port.port)
                                        .collect();
                                    self.forwarding_view
                                        .new_ports
                                        .retain(|port| !closed.contains(&port.port));
                                }
                                self.forwarding_view.port_scan_error = None;
                            }
                            Err(error) => {
                                self.forwarding_view.has_scanned_ports = true;
                                self.forwarding_view.port_scan_error = Some(error);
                            }
                        }
                        cx.notify();
                    }
                }
            }
        }
    }

    pub(super) fn poll_forwarding_events(&mut self, cx: &mut Context<Self>) {
        let mut events = Vec::new();
        while let Ok(event) = self.forwarding_event_rx.try_recv() {
            events.push(event);
        }

        for event in events {
            match event {
                ForwardEvent::StatusChanged {
                    session_id,
                    status,
                    error,
                    ..
                } => {
                    if !self.active_forwards_tab_matches_session(&session_id) {
                        continue;
                    }
                    match status {
                        ForwardStatus::Suspended => {
                            self.forwarding_view.error =
                                Some(self.i18n.t("forwards.toast.suspended_desc"));
                        }
                        ForwardStatus::Error(message) => {
                            self.forwarding_view.error = Some(error.unwrap_or(message));
                        }
                        _ => {}
                    }
                    cx.notify();
                }
                ForwardEvent::StatsUpdated { session_id, .. } => {
                    if self.active_forwards_tab_matches_session(&session_id) {
                        cx.notify();
                    }
                }
                ForwardEvent::SessionSuspended {
                    session_id,
                    forward_ids,
                } => {
                    if !self.active_forwards_tab_matches_session(&session_id) {
                        continue;
                    }
                    self.forwarding_view.error = Some(
                        self.i18n
                            .t("forwards.toast.session_suspended_desc")
                            .replace("{{count}}", &forward_ids.len().to_string()),
                    );
                    cx.notify();
                }
            }
        }
    }

    fn active_forwards_tab_matches_session(&self, session_id: &str) -> bool {
        let Some(tab_id) = self.active_tab_id else {
            return false;
        };
        let Some(node_id) = self.forward_tab_nodes.get(&tab_id) else {
            return false;
        };
        self.ssh_nodes
            .get(node_id)
            .and_then(|node| node.terminal_ids.first())
            .is_some_and(|active_session_id| active_session_id.0.to_string() == session_id)
    }

    fn forwarding_manager_for_node_readonly(
        &self,
        node_id: &NodeId,
    ) -> Option<Arc<ForwardingManager>> {
        let session_id = self.ssh_nodes.get(node_id)?.terminal_ids.first()?;
        self.forwarding_registry.get(&session_id.0.to_string())
    }

    fn forwarding_manager_for_node(
        &mut self,
        node_id: &NodeId,
        cx: &mut Context<Self>,
    ) -> Result<Arc<ForwardingManager>, String> {
        let session_id = *self
            .ssh_nodes
            .get(node_id)
            .and_then(|node| node.terminal_ids.first())
            .ok_or_else(|| self.i18n.t("forwards.messages.node_not_ready"))?;
        if let Some(manager) = self.forwarding_registry.get(&session_id.0.to_string()) {
            return Ok(manager);
        }
        let pane_id = self
            .pane_id_for_terminal_session(session_id)
            .ok_or_else(|| self.i18n.t("forwards.messages.connection_not_ready"))?;
        let handle = self
            .panes
            .get(&pane_id)
            .and_then(|pane| pane.read(cx).ssh_connection_handle())
            .ok_or_else(|| self.i18n.t("forwards.messages.connection_not_ready"))?;
        let manager = self
            .forwarding_registry
            .register(session_id.0.to_string(), handle);
        self.start_saved_forwards_for_node(node_id, session_id, manager.clone());
        Ok(manager)
    }

    fn forward_persist_context_for_node(
        &self,
        node_id: &NodeId,
    ) -> Option<(String, Option<String>)> {
        let node = self.ssh_nodes.get(node_id)?;
        let session_id = node.terminal_ids.first()?.0.to_string();
        Some((session_id, node.saved_connection_id.clone()))
    }

    fn start_saved_forwards_for_node(
        &self,
        node_id: &NodeId,
        session_id: TerminalSessionId,
        manager: Arc<ForwardingManager>,
    ) {
        let Some(node) = self.ssh_nodes.get(node_id) else {
            return;
        };
        let session_id_string = session_id.0.to_string();
        if let Some(owner_connection_id) = node.saved_connection_id.as_ref() {
            let _ = self.forwarding_registry.saved_store().map(|store| {
                store.bind_owned_forwards_to_session(owner_connection_id, &session_id_string)
            });
        }
        let saved_forwards = if let Some(owner_connection_id) = node.saved_connection_id.as_ref() {
            self.forwarding_registry
                .load_owned_forwards(owner_connection_id)
        } else {
            self.forwarding_registry
                .load_persisted_forwards(&session_id_string)
        };
        let auto_start_rules: Vec<ForwardRule> = saved_forwards
            .into_iter()
            .filter(|forward| forward.auto_start)
            .map(|forward| forward.rule)
            .collect();
        if auto_start_rules.is_empty() {
            return;
        }
        thread::spawn(move || {
            let Ok(runtime) = tokio::runtime::Runtime::new() else {
                return;
            };
            runtime.block_on(async move {
                for mut rule in auto_start_rules {
                    rule.status = ForwardStatus::Starting;
                    let _ = manager.create_forward(rule).await;
                }
            });
        });
    }

    fn pane_id_for_terminal_session(&self, session_id: TerminalSessionId) -> Option<PaneId> {
        self.tabs
            .iter()
            .filter_map(|tab| tab.root_pane.as_ref())
            .find_map(|root| root.pane_id_for_session(session_id))
    }

    fn open_forward_edit_form(&mut self, rule: ForwardRule, cx: &mut Context<Self>) {
        self.forwarding_view.edit_bind_address = rule.bind_address.clone();
        self.forwarding_view.edit_bind_port = rule.bind_port.to_string();
        self.forwarding_view.edit_target_host = rule.target_host.clone();
        self.forwarding_view.edit_target_port = rule.target_port.to_string();
        self.forwarding_view.editing_forward = Some(rule);
        self.forwarding_view.error = None;
        self.forwarding_view.focused_input = None;
        cx.notify();
    }

    pub(super) fn handle_forwards_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(input) = self.forwarding_view.focused_input else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        match key {
            "escape" => {
                self.forwarding_view.focused_input = None;
                self.ime_marked_text = None;
                cx.notify();
                true
            }
            "backspace" => {
                self.forward_input_value_mut(input).pop();
                self.forwarding_view.error = None;
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(super) fn forward_input_value(&self, input: ForwardInput) -> &str {
        match input {
            ForwardInput::CreateBindAddress => &self.forwarding_view.bind_address,
            ForwardInput::CreateBindPort => &self.forwarding_view.bind_port,
            ForwardInput::CreateTargetHost => &self.forwarding_view.target_host,
            ForwardInput::CreateTargetPort => &self.forwarding_view.target_port,
            ForwardInput::EditBindAddress => &self.forwarding_view.edit_bind_address,
            ForwardInput::EditBindPort => &self.forwarding_view.edit_bind_port,
            ForwardInput::EditTargetHost => &self.forwarding_view.edit_target_host,
            ForwardInput::EditTargetPort => &self.forwarding_view.edit_target_port,
        }
    }

    pub(super) fn forward_input_value_mut(&mut self, input: ForwardInput) -> &mut String {
        match input {
            ForwardInput::CreateBindAddress => &mut self.forwarding_view.bind_address,
            ForwardInput::CreateBindPort => &mut self.forwarding_view.bind_port,
            ForwardInput::CreateTargetHost => &mut self.forwarding_view.target_host,
            ForwardInput::CreateTargetPort => &mut self.forwarding_view.target_port,
            ForwardInput::EditBindAddress => &mut self.forwarding_view.edit_bind_address,
            ForwardInput::EditBindPort => &mut self.forwarding_view.edit_bind_port,
            ForwardInput::EditTargetHost => &mut self.forwarding_view.edit_target_host,
            ForwardInput::EditTargetPort => &mut self.forwarding_view.edit_target_port,
        }
    }
}

#[derive(Clone, Copy)]
enum ForwardButtonVariant {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

#[derive(Clone, Copy)]
enum ForwardRowCorners {
    None,
    Top,
    Bottom,
}

fn parse_port(value: &str) -> Option<u16> {
    let trimmed = value.trim();
    if trimmed.is_empty() || !trimmed.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    trimmed.parse::<u16>().ok().filter(|port| *port > 0)
}

fn forwards_theme_bg(color: u32, has_background: bool) -> gpui::Rgba {
    if has_background {
        rgba((color << 8) | FORWARDS_BG_ACTIVE_THEME_ALPHA)
    } else {
        rgb(color)
    }
}

fn forwards_theme_panel_bg(color: u32, has_background: bool) -> gpui::Rgba {
    forwards_theme_bg(color, has_background)
}

fn forwards_theme_card_bg(color: u32, has_background: bool) -> gpui::Rgba {
    forwards_theme_bg(color, has_background)
}

fn forwards_theme_sunken_bg(color: u32, has_background: bool) -> gpui::Rgba {
    if has_background {
        rgba((color << 8) | FORWARDS_BG_ACTIVE_SUNKEN_ALPHA)
    } else {
        rgb(color)
    }
}

fn forwards_theme_hover_bg(color: u32, has_background: bool) -> gpui::Rgba {
    if has_background {
        rgba((color << 8) | FORWARDS_BG_ACTIVE_HOVER_ALPHA)
    } else {
        rgb(color)
    }
}

fn forwards_theme_border(color: u32, has_background: bool) -> gpui::Rgba {
    if has_background {
        rgba((color << 8) | FORWARDS_BG_ACTIVE_BORDER_ALPHA)
    } else {
        rgb(color)
    }
}

fn forwards_theme_border_half(color: u32, has_background: bool) -> gpui::Rgba {
    if has_background {
        rgba((color << 8) | FORWARDS_BG_ACTIVE_BORDER_HALF_ALPHA)
    } else {
        rgba((color << 8) | FORWARDS_TW_ALPHA_50)
    }
}

fn forwards_theme_with_alpha(color: u32, alpha: u32) -> gpui::Rgba {
    rgba((color << 8) | alpha)
}

fn forwards_palette_color(color: u32) -> gpui::Rgba {
    rgb(color)
}

fn forwards_palette_alpha(color: u32, alpha: u32) -> gpui::Rgba {
    rgba((color << 8) | alpha)
}

fn forwards_transparent() -> gpui::Rgba {
    forwards_palette_alpha(TW_BLACK, FORWARDS_ALPHA_TRANSPARENT)
}

fn forward_addresses(rule: &ForwardRule) -> (String, String) {
    match rule.forward_type {
        ForwardType::Remote => (
            format!("{}:{}", rule.target_host, rule.target_port),
            format!("{}:{}", rule.bind_address, rule.bind_port),
        ),
        ForwardType::Local | ForwardType::Dynamic => (
            format!("{}:{}", rule.bind_address, rule.bind_port),
            format!("{}:{}", rule.target_host, rule.target_port),
        ),
    }
}

fn forward_type_key(forward_type: ForwardType, i18n: &I18n) -> String {
    match forward_type {
        ForwardType::Local => i18n.t("forwards.type.local"),
        ForwardType::Remote => i18n.t("forwards.type.remote"),
        ForwardType::Dynamic => i18n.t("forwards.type.dynamic"),
    }
}

fn forward_type_label(rule: ForwardRule, i18n: &I18n) -> String {
    forward_type_key(rule.forward_type, i18n)
}

fn forward_status_key(status: &ForwardStatus) -> &'static str {
    match status {
        ForwardStatus::Starting => "forwards.status.starting",
        ForwardStatus::Active => "forwards.status.active",
        ForwardStatus::Stopped => "forwards.status.stopped",
        ForwardStatus::Error(_) => "forwards.status.error",
        ForwardStatus::Suspended => "forwards.status.suspended",
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let units = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index + 1 < units.len() {
        value /= 1024.0;
        index += 1;
    }
    format!("{value:.1} {}", units[index])
}
