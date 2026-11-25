// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::form_state::ConnectionFormDrawer;
use oxideterm_gpui_ui::{Scrollbar, ScrollbarAxis, modal::rounded_shell_child_radius};
use oxideterm_settings_model::parse_rgb24_hex;

pub(super) const CONNECTION_STUDIO_WIDTH: f32 = 920.0;
pub(super) const CONNECTION_STUDIO_MAX_WIDTH: f32 = 1120.0;
pub(super) const CONNECTION_STUDIO_EDIT_WIDTH: f32 = 760.0;
pub(super) const CONNECTION_STUDIO_EDIT_MAX_WIDTH: f32 = 920.0;
pub(super) const CONNECTION_STUDIO_MIN_HEIGHT: f32 = 560.0;
pub(super) const CONNECTION_STUDIO_MAX_HEIGHT: f32 = 760.0;
pub(super) const CONNECTION_STUDIO_VIEWPORT_MARGIN: f32 = 16.0;
pub(super) const CONNECTION_STUDIO_COMPACT_VIEWPORT_MARGIN: f32 = 8.0;
pub(super) const CONNECTION_STUDIO_SIDEBAR_WIDTH: f32 = 180.0;
pub(super) const CONNECTION_STUDIO_SIDEBAR_SLIM_WIDTH: f32 = 144.0;
pub(super) const CONNECTION_STUDIO_SIDEBAR_ICON_WIDTH: f32 = 54.0;

/// Responsive breakpoints derived from the live modal width so small split
/// windows, standard laptops, and 4K displays all avoid clipping or crowding.
#[derive(Clone, Copy, Debug)]
pub(super) struct StudioLayoutMetrics {
    pub(super) sidebar_width: f32,
    pub(super) icon_only_sidebar: bool,
    pub(super) content_padding_x: f32,
    pub(super) content_width: f32,
    pub(super) compact_content: bool,
    pub(super) narrow_content: bool,
}

impl StudioLayoutMetrics {
    pub(super) fn compute(
        modal_width: f32,
        tokens: &ThemeTokens,
        shows_transport_selector: bool,
    ) -> Self {
        let font_scale = studio_font_scale(tokens);
        let (sidebar_width, icon_only_sidebar) = if !shows_transport_selector {
            (0.0, false)
        } else if modal_width < 620.0 {
            (CONNECTION_STUDIO_SIDEBAR_ICON_WIDTH, true)
        } else if modal_width < 760.0 {
            (
                (CONNECTION_STUDIO_SIDEBAR_SLIM_WIDTH * font_scale.max(1.0)).round(),
                false,
            )
        } else {
            (
                (CONNECTION_STUDIO_SIDEBAR_WIDTH * font_scale.max(1.0)).round(),
                false,
            )
        };
        let pane_width = (modal_width - sidebar_width).max(240.0);
        let content_padding_x = if pane_width < 460.0 {
            14.0
        } else if pane_width < 560.0 {
            18.0
        } else {
            22.0
        };
        let content_width = (pane_width - content_padding_x * 2.0).max(200.0);
        Self {
            sidebar_width,
            icon_only_sidebar,
            content_padding_x,
            content_width,
            compact_content: content_width < 520.0,
            narrow_content: content_width < 440.0,
        }
    }
}

pub(super) fn studio_viewport_margin(window: &Window) -> f32 {
    let viewport = window.viewport_size();
    let viewport_w = f32::from(viewport.width);
    let viewport_h = f32::from(viewport.height);
    if viewport_w < 600.0 || viewport_h < 520.0 {
        CONNECTION_STUDIO_COMPACT_VIEWPORT_MARGIN
    } else {
        CONNECTION_STUDIO_VIEWPORT_MARGIN
    }
}

fn studio_font_scale(tokens: &ThemeTokens) -> f32 {
    // Baseline ui_text_sm is 14px in ThemeTokens; track user font scaling directly.
    (tokens.metrics.ui_text_sm / 14.0).clamp(0.85, 1.6)
}

fn studio_density_scale(tokens: &ThemeTokens) -> f32 {
    // Baseline ui_control_height is 32px in ThemeTokens; track user density directly.
    (tokens.metrics.ui_control_height / 32.0).clamp(0.82, 1.25)
}

fn studio_ui_scale(tokens: &ThemeTokens) -> f32 {
    studio_font_scale(tokens).max(studio_density_scale(tokens))
}

/// Computes a viewport-locked outer frame for every multi-page Connection Studio
/// shell (create, edit, standalone SFTP, SSH tunnel) so switching protocols,
/// section tabs, auth tabs, or algorithm categories never shifts modal bounds,
/// scales up proportionally on 4K/HiDPI viewports or enlarged UI font/density,
/// and stays bounded inside small windows.
pub(super) fn studio_modal_size(
    window: &Window,
    tokens: &ThemeTokens,
    shows_transport_selector: bool,
    modal_max_height: f32,
) -> (f32, f32) {
    let viewport = window.viewport_size();
    let viewport_w = f32::from(viewport.width);
    let viewport_h = f32::from(viewport.height);
    let margin = studio_viewport_margin(window);
    let ui_scale = studio_ui_scale(tokens);
    let (base_w, cap_w) = if shows_transport_selector {
        (CONNECTION_STUDIO_WIDTH, CONNECTION_STUDIO_MAX_WIDTH)
    } else {
        (
            CONNECTION_STUDIO_EDIT_WIDTH,
            CONNECTION_STUDIO_EDIT_MAX_WIDTH,
        )
    };
    let desired_w = (base_w * ui_scale)
        .max(viewport_w * 0.54)
        .min(cap_w * ui_scale.max(1.0));
    let max_w = (viewport_w - margin * 2.0).max(0.0);
    let width = desired_w.min(max_w);
    let max_h = modal_max_height.min((viewport_h - margin * 2.0).max(0.0));
    let min_h = (CONNECTION_STUDIO_MIN_HEIGHT * ui_scale.min(1.15)).min(max_h);
    let max_studio_h = CONNECTION_STUDIO_MAX_HEIGHT * ui_scale.max(1.0);
    let height = (viewport_h * 0.80).clamp(min_h, max_studio_h).min(max_h);
    (width, height)
}

const STUDIO_PROTOCOL_SELECTOR_ID: &str = "studio-protocol-selector";
const STUDIO_CATEGORY_SELECTOR_ID: &str = "studio-category-selector";
const STUDIO_PROTOCOL_ROW_GAP: f32 = 2.0;
const STUDIO_PROTOCOL_DIVIDER_HEIGHT: f32 = 1.0;
const STUDIO_PROTOCOL_DIVIDER_MARGIN_Y: f32 = 4.0;
const STUDIO_CATEGORY_TAB_BAR_HEIGHT: f32 = 38.0;

pub(super) struct CategoryNavItem {
    /// `None` represents the primary General/Session page.
    pub(super) drawer: Option<ConnectionFormDrawer>,
    pub(super) label: String,
    pub(super) summary: Option<String>,
}

impl ConnectionFormView {
    pub(super) fn transport_display_meta(
        &self,
        transport: NewConnectionTransport,
    ) -> (LucideIcon, String) {
        match transport {
            NewConnectionTransport::Ssh => (
                LucideIcon::Server,
                self.i18n.t("modals.new_connection.transport_ssh"),
            ),
            NewConnectionTransport::Mosh => (
                LucideIcon::Wifi,
                self.i18n.t("modals.new_connection.transport_mosh"),
            ),
            NewConnectionTransport::Telnet => (
                LucideIcon::Network,
                self.i18n.t("modals.new_connection.transport_telnet"),
            ),
            NewConnectionTransport::Serial => (
                LucideIcon::Radio,
                self.i18n.t("modals.new_connection.transport_serial"),
            ),
            NewConnectionTransport::Ble => (
                LucideIcon::Bluetooth,
                self.i18n.t("modals.new_connection.transport_ble"),
            ),
            NewConnectionTransport::Rdp => (
                LucideIcon::Monitor,
                self.i18n.t("modals.new_connection.transport_rdp"),
            ),
            NewConnectionTransport::Vnc => (
                LucideIcon::Monitor,
                self.i18n.t("modals.new_connection.transport_vnc"),
            ),
            NewConnectionTransport::WslGraphics => (
                LucideIcon::AppWindow,
                self.i18n.t("modals.new_connection.transport_wsl_graphics"),
            ),
            NewConnectionTransport::LocalTerminal => (
                LucideIcon::Terminal,
                self.i18n
                    .t("modals.new_connection.transport_local_terminal"),
            ),
            NewConnectionTransport::StandaloneSftp => (
                LucideIcon::FolderSync,
                self.i18n
                    .t("modals.new_connection.transport_standalone_sftp"),
            ),
            NewConnectionTransport::Ftp => (
                LucideIcon::FolderSync,
                self.i18n.t("modals.new_connection.transport_ftp"),
            ),
        }
    }

    pub(super) fn render_identity_avatar_preview(
        &self,
        size_px: f32,
        icon_px: f32,
        cx: &App,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let Some((icon_value, color_value, bg_value, transport)) =
            self.connection_form_state(cx).form.as_ref().map(|form| {
                (
                    form.icon.clone(),
                    form.color.clone(),
                    form.icon_background_color.clone(),
                    form.transport,
                )
            })
        else {
            return div().into_any_element();
        };
        let preview_color = parse_rgb24_hex(&color_value).unwrap_or(theme.accent);
        let preview_bg = parse_rgb24_hex(&bg_value)
            .map(rgb)
            .unwrap_or_else(|| rgba((preview_color << 8) | 0x20));
        let (default_icon, _) = self.transport_display_meta(transport);
        let active_icon = session_icon_from_id(Some(&icon_value)).unwrap_or(default_icon.into());

        div()
            .flex_none()
            .size(px(size_px))
            .rounded(px(self.tokens.radii.md))
            .bg(preview_bg)
            .border_1()
            .border_color(rgba((preview_color << 8) | 0x42))
            .flex()
            .items_center()
            .justify_center()
            .child(active_icon.render(icon_px, rgb(preview_color), true))
            .into_any_element()
    }

    pub(super) fn reset_studio_form_scroll(&self) {
        let origin = point(px(0.0), px(0.0));
        for id in [
            "new-connection-modal-form-scroll",
            "standalone-sftp-form-scroll",
            "ssh-tunnel-form-scroll",
        ] {
            self.selectable_text_scroll_handle(id).set_offset(origin);
        }
    }

    /// Selects a transport from the top protocol bar and runs all transport-switch cleanup.
    fn select_composer_transport(
        &mut self,
        transport: NewConnectionTransport,
        focus_field: NewConnectionField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.environment.tunnel.is_some() {
            self.request(FormAction::LeaveTunnel, Some(window), cx);
        }
        self.reset_studio_form_scroll();
        let mut should_refresh_ports = false;
        #[cfg(not(target_family = "wasm"))]
        let mut should_refresh_ble_adapters = false;
        self.update_connection_form_state(cx, |state| {
            if let Some(form) = state.form.as_mut() {
                let previous_transport = form.transport;
                apply_transport_default_port(form, previous_transport, transport);
                apply_transport_default_username(form, previous_transport, transport);
                if matches!(
                    transport,
                    NewConnectionTransport::Rdp | NewConnectionTransport::Telnet
                ) && previous_transport != transport
                {
                    form.upstream_proxy_policy = NewConnectionUpstreamProxyPolicy::Direct;
                    form.upstream_proxy_protocol = SavedUpstreamProxyProtocol::Socks5;
                }
                if form.transport != transport {
                    form.standalone_connection_id = None;
                    form.active_drawer = None;
                }
                #[cfg(not(target_family = "wasm"))]
                if transport != NewConnectionTransport::Ble {
                    form.ble.scan_generation += 1;
                    form.ble.scanning = false;
                    form.ble.scan_started = false;
                    form.ble.inspecting = false;
                    form.ble.devices.clear();
                    form.ble.adapter_guard = None;
                    form.ble.selected_device_id = None;
                    form.ble.device_details = None;
                    form.ble.inspect_error = None;
                    form.ble.selected_service_uuid = None;
                }
                form.transport = transport;
                form.transport_picker_open = false;
                form.appearance_popover_open = false;
                form.focused_field = focus_field;
                form.field_focused = false;
                form.error = None;
                clear_connection_selection(form);
                should_refresh_ports = transport == NewConnectionTransport::Serial
                    && form.serial_ports.is_empty()
                    && !form.serial_ports_loading;
                #[cfg(not(target_family = "wasm"))]
                {
                    should_refresh_ble_adapters = transport == NewConnectionTransport::Ble
                        && form.ble.adapters.is_empty()
                        && !form.ble.adapters_loading;
                }
            }
        });
        self.user_motion.clear(STUDIO_CATEGORY_SELECTOR_ID);
        self.selectable_text_scroll_handle("studio-category-tabs-scroll")
            .set_offset(point(px(0.0), px(0.0)));
        self.close_new_connection_select(cx);
        if should_refresh_ports {
            self.refresh_serial_ports(cx);
        }
        #[cfg(not(target_family = "wasm"))]
        if should_refresh_ble_adapters {
            self.refresh_ble_adapters(cx);
        }
        cx.notify();
    }

    /// Left vertical rail listing all connection protocols grouped by domain so every built-in
    /// transport stays visible at once without horizontal scrolling. Collapses cleanly to a
    /// compact icon-only vertical rail on narrow viewports.
    pub(super) fn render_studio_protocol_sidebar(
        &self,
        layout: StudioLayoutMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let icon_only = layout.icon_only_sidebar;
        let tunnel_active = self.environment.tunnel.is_some();
        let active_transport = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| form.transport)
            .unwrap_or(NewConnectionTransport::Ssh);
        let plugin_types = self.environment.plugin_types.clone();

        enum ProtocolEntry {
            BuiltIn(NewConnectionTransport, NewConnectionField),
            Tunnel,
        }

        let mut groups: Vec<Vec<ProtocolEntry>> = vec![
            vec![
                ProtocolEntry::BuiltIn(NewConnectionTransport::Ssh, NewConnectionField::Name),
                ProtocolEntry::BuiltIn(NewConnectionTransport::Mosh, NewConnectionField::Name),
                ProtocolEntry::BuiltIn(NewConnectionTransport::Telnet, NewConnectionField::Host),
                ProtocolEntry::BuiltIn(
                    NewConnectionTransport::LocalTerminal,
                    NewConnectionField::Name,
                ),
            ],
            {
                let mut remote_desktop = vec![
                    ProtocolEntry::BuiltIn(NewConnectionTransport::Rdp, NewConnectionField::Host),
                    ProtocolEntry::BuiltIn(NewConnectionTransport::Vnc, NewConnectionField::Host),
                ];
                if cfg!(target_os = "windows") {
                    remote_desktop.push(ProtocolEntry::BuiltIn(
                        NewConnectionTransport::WslGraphics,
                        NewConnectionField::Name,
                    ));
                }
                remote_desktop
            },
            vec![
                ProtocolEntry::BuiltIn(
                    NewConnectionTransport::Serial,
                    NewConnectionField::SerialPortPath,
                ),
                ProtocolEntry::BuiltIn(
                    NewConnectionTransport::Ble,
                    NewConnectionField::BleProfileName,
                ),
            ],
            vec![
                ProtocolEntry::BuiltIn(
                    NewConnectionTransport::StandaloneSftp,
                    NewConnectionField::Name,
                ),
                ProtocolEntry::BuiltIn(NewConnectionTransport::Ftp, NewConnectionField::Name),
                ProtocolEntry::Tunnel,
            ],
        ];

        if matches!(&self.flow, FormOwner::Portable(..)) {
            for group in &mut groups {
                group.retain(|entry| {
                    !matches!(
                        entry,
                        ProtocolEntry::BuiltIn(NewConnectionTransport::WslGraphics, _)
                    )
                });
            }
        }

        // Scale row hitbox, icon, and rail width with user density and UI font size so
        // 4K/HiDPI or enlarged-font setups stay legible and comfortable to click.
        let font_scale = studio_font_scale(&self.tokens);
        let protocol_row_height = self
            .tokens
            .metrics
            .ui_control_height
            .max((self.tokens.metrics.ui_text_sm * 2.25).round());
        let protocol_icon_size = (14.0 * font_scale).round();
        let sidebar_width = layout.sidebar_width;

        // Precompute exact vertical offsets (including inter-group hairlines) so the vertical
        // active indicator slides cleanly across group boundaries.
        let divider_stride = STUDIO_PROTOCOL_DIVIDER_HEIGHT
            + STUDIO_PROTOCOL_DIVIDER_MARGIN_Y * 2.0
            + STUDIO_PROTOCOL_ROW_GAP;
        let row_stride = protocol_row_height + STUDIO_PROTOCOL_ROW_GAP;
        let mut item_offsets_y: Vec<f32> = Vec::new();
        let mut active_index = 0usize;
        let mut cursor_y = 0.0_f32;
        for (group_index, group) in groups.iter().enumerate() {
            if group_index > 0 {
                cursor_y += divider_stride;
            }
            for entry in group {
                let index = item_offsets_y.len();
                let is_active = match entry {
                    ProtocolEntry::BuiltIn(transport, _) => {
                        !tunnel_active && *transport == active_transport
                    }
                    ProtocolEntry::Tunnel => tunnel_active,
                };
                if is_active {
                    active_index = index;
                }
                item_offsets_y.push(cursor_y);
                cursor_y += row_stride;
            }
        }
        let active_y = item_offsets_y.get(active_index).copied().unwrap_or(0.0);

        let divider = || {
            div()
                .flex_none()
                .h(px(STUDIO_PROTOCOL_DIVIDER_HEIGHT))
                .mx(px(8.0))
                .my(px(STUDIO_PROTOCOL_DIVIDER_MARGIN_Y))
                .bg(rgba((theme.border << 8) | 0x80))
        };

        let render_active_surface = || {
            let surface = div()
                .absolute()
                .inset_0()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.bg_card))
                .flex()
                .items_center()
                .pl(px(if icon_only { 3.0 } else { 8.0 }))
                .child(
                    div()
                        .w(px(2.5))
                        .h(px(14.0))
                        .rounded_full()
                        .bg(rgb(theme.accent)),
                );
            self.user_motion.render_vertical(
                STUDIO_PROTOCOL_SELECTOR_ID,
                active_index,
                &self.tokens,
                surface,
            )
        };

        let protocol_font_size =
            (self.tokens.metrics.ui_text_xs + self.tokens.metrics.ui_text_sm) * 0.5;
        let mut list = div().flex().flex_col().gap(px(STUDIO_PROTOCOL_ROW_GAP));
        let mut item_index = 0usize;
        for (group_index, group) in groups.drain(..).enumerate() {
            if group_index > 0 {
                list = list.child(divider());
            }
            for entry in group {
                let index = item_index;
                let item_y = item_offsets_y[index];
                item_index += 1;
                let active = index == active_index;
                let (id, debug_name, icon, label) = match &entry {
                    ProtocolEntry::BuiltIn(transport, _) => {
                        let (icon, label) = self.transport_display_meta(*transport);
                        (
                            SharedString::from(format!("studio-transport-{transport:?}")),
                            format!("new-connection-transport-{transport:?}"),
                            icon,
                            label,
                        )
                    }
                    ProtocolEntry::Tunnel => (
                        SharedString::from("studio-transport-tunnel"),
                        "new-connection-transport-Tunnel".to_string(),
                        LucideIcon::Network,
                        self.i18n.t("ssh_tunnel.title"),
                    ),
                };

                let row = div()
                    .id(id)
                    .debug_selector(move || debug_name.clone())
                    .relative()
                    .flex_none()
                    .w_full()
                    .h(px(protocol_row_height))
                    .px(px(if icon_only { 0.0 } else { 8.0 }))
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .when(!active, |el| {
                        el.hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                    })
                    .when(active, |el| el.child(render_active_surface()))
                    .flex()
                    .items_center()
                    .when(icon_only, |el| el.justify_center())
                    .when(!icon_only, |el| el.gap(px(8.0)).child(div().w(px(2.5))))
                    .child(Self::render_lucide_icon(
                        icon,
                        protocol_icon_size,
                        rgb(if active {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                    ))
                    .when(!icon_only, |el| {
                        el.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(px(protocol_font_size))
                                .font_weight(if active {
                                    gpui::FontWeight::MEDIUM
                                } else {
                                    gpui::FontWeight::NORMAL
                                })
                                .text_color(rgb(if active {
                                    theme.text_heading
                                } else {
                                    theme.text
                                }))
                                .child(label),
                        )
                    })
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, window, cx| {
                            if index != active_index {
                                let offset_y = active_y - item_y;
                                this.begin_user_segmented_control_transition_with_history(
                                    STUDIO_PROTOCOL_SELECTOR_ID,
                                    Some(active_index),
                                    index,
                                    Some(offset_y),
                                    cx,
                                );
                            }
                            match entry {
                                ProtocolEntry::BuiltIn(transport, focus_field) => {
                                    this.select_composer_transport(
                                        transport,
                                        focus_field,
                                        window,
                                        cx,
                                    );
                                }
                                ProtocolEntry::Tunnel => {
                                    if this.environment.tunnel.is_none() {
                                        this.request(FormAction::OpenTunnel, Some(window), cx);
                                    }
                                }
                            }
                            cx.stop_propagation();
                        }),
                    );
                list = list.child(row);
            }
        }

        if !plugin_types.is_empty() {
            list = list.child(divider());
            for (plugin, definition) in plugin_types {
                list = list.child(
                    div()
                        .flex_none()
                        .w_full()
                        .h(px(protocol_row_height))
                        .px(px(if icon_only { 0.0 } else { 8.0 }))
                        .rounded(px(self.tokens.radii.md))
                        .cursor_pointer()
                        .hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                        .flex()
                        .items_center()
                        .when(icon_only, |el| el.justify_center())
                        .when(!icon_only, |el| el.gap(px(8.0)).child(div().w(px(2.5))))
                        .child(Self::render_lucide_icon(
                            LucideIcon::from_plugin_name(&definition.icon),
                            protocol_icon_size,
                            rgb(theme.text_muted),
                        ))
                        .when(!icon_only, |el| {
                            el.child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(protocol_font_size))
                                    .text_color(rgb(theme.text))
                                    .child(definition.title),
                            )
                        })
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                this.close_new_connection_select(cx);
                                this.request(
                                    FormAction::OpenPluginForm {
                                        plugin: plugin.clone(),
                                        type_id: definition.id.clone(),
                                    },
                                    Some(window),
                                    cx,
                                );
                                cx.stop_propagation();
                            }),
                        ),
                );
            }
        }

        let scroll = self.selectable_text_scroll_handle("new-connection-transport-scroll");
        div()
            .w(px(sidebar_width))
            .flex_none()
            .min_h_0()
            .bg(rgb(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border))
            .relative()
            .child(
                div()
                    .id("new-connection-transport-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .px(px(if icon_only { 6.0 } else { 10.0 }))
                    .py(px(8.0))
                    .child(list),
            )
            .child(Scrollbar::new(&scroll).axis(ScrollbarAxis::Vertical))
            .into_any_element()
    }

    /// Unified top chrome with proper GPUI inner top radius (fixes rectangular overflow clipping).
    pub(super) fn render_studio_top_chrome(
        &self,
        title: String,
        subtitle: Option<String>,
        shows_transport_rail: bool,
        shows_header_appearance_button: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let top_radius = rounded_shell_child_radius(self.tokens.radii.lg);

        div()
            .flex_none()
            .w_full()
            // GPUI's overflow_hidden masks to a rectangle; match the modal shell's inner top radius.
            .rounded_t(px(top_radius))
            .bg(rgb(theme.bg_panel))
            .border_b_1()
            .border_color(rgb(theme.border))
            .px(px(20.0))
            .py(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .when(shows_header_appearance_button, |row| {
                        row.child(
                            div()
                                .id("studio-header-appearance-trigger")
                                .cursor_pointer()
                                .rounded(px(self.tokens.radii.md))
                                .child(self.render_identity_avatar_preview(32.0, 16.0, cx))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.close_new_connection_select(cx);
                                        this.update_connection_form_state(cx, |state| {
                                            if let Some(form) = state.form.as_mut() {
                                                form.appearance_popover_open =
                                                    !form.appearance_popover_open;
                                                form.icon_picker_expanded = true;
                                                form.field_focused = false;
                                                clear_connection_selection(form);
                                            }
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.modal_title_font_size + 1.0))
                                    .line_height(px((self.tokens.metrics.modal_title_font_size
                                        * 1.45)
                                        .round()))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(title),
                            )
                            .when_some(
                                subtitle.filter(|s| !s.is_empty() && !shows_transport_rail),
                                |col, desc| {
                                    col.child(
                                        div()
                                            .truncate()
                                            .text_size(px(self.tokens.metrics.ui_text_xs))
                                            .text_color(rgb(theme.text_muted))
                                            .child(desc),
                                    )
                                },
                            ),
                    ),
            )
            .into_any_element()
    }

    /// Horizontal underline section tab bar at the top of the right content pane.
    pub(super) fn render_studio_category_tabs(
        &self,
        items: Vec<CategoryNavItem>,
        active_drawer: Option<ConnectionFormDrawer>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active_index = items
            .iter()
            .position(|item| item.drawer == active_drawer)
            .unwrap_or(0);
        let tab_font_size = self.tokens.metrics.ui_text_xs;
        let tabs_scroll = self.selectable_text_scroll_handle("studio-category-tabs-scroll");

        // Align the first tab's text (16px bar padding + 6px tab padding = 22px)
        // with the form content margin below.
        let mut strip = div()
            .id("studio-category-tabs-scroll")
            .size_full()
            .overflow_x_scroll()
            .track_scroll(&tabs_scroll)
            .flex()
            .flex_row()
            .items_stretch()
            .px(px(16.0))
            .gap(px(2.0));

        for (item_index, item) in items.into_iter().enumerate() {
            let target = item.drawer;
            let active = item_index == active_index;
            let underline = active.then(|| {
                let bar = div()
                    .absolute()
                    .left(px(6.0))
                    .right(px(6.0))
                    .bottom_0()
                    .h(px(2.0))
                    .rounded_t_full()
                    .bg(rgb(theme.accent));
                if let Some((generation, _)) =
                    self.segmented_control_user_transition(STUDIO_CATEGORY_SELECTOR_ID, item_index)
                {
                    oxideterm_gpui_ui::motion::fade_in(
                        &self.tokens,
                        (STUDIO_CATEGORY_SELECTOR_ID, generation),
                        bar,
                        oxideterm_gpui_ui::motion::MotionDuration::Control,
                    )
                } else {
                    bar.into_any_element()
                }
            });

            let tab = div()
                .id(SharedString::from(format!("studio-category-{target:?}")))
                .debug_selector(move || format!("studio-category-{target:?}"))
                .relative()
                .flex_none()
                .h_full()
                .px(px(6.0))
                .cursor_pointer()
                .flex()
                .items_center()
                .gap(px(5.0))
                .text_color(rgb(if active {
                    theme.text_heading
                } else {
                    theme.text_muted
                }))
                .when(!active, |el| el.hover(|el| el.text_color(rgb(theme.text))))
                .child(
                    div()
                        .whitespace_nowrap()
                        .text_size(px(tab_font_size))
                        .font_weight(if active {
                            gpui::FontWeight::MEDIUM
                        } else {
                            gpui::FontWeight::NORMAL
                        })
                        .child(item.label),
                )
                .when_some(item.summary, |tab, summary| {
                    if summary == "•" {
                        tab.child(div().size(px(5.0)).rounded_full().bg(rgb(theme.accent)))
                    } else {
                        tab.child(
                            div()
                                .px(px(5.0))
                                .py(px(1.0))
                                .rounded(px(self.tokens.radii.xs))
                                .bg(rgba((theme.accent << 8) | 0x20))
                                .text_size(px(10.0))
                                .text_color(rgb(theme.accent))
                                .child(summary),
                        )
                    }
                })
                .when_some(underline, |tab, bar| tab.child(bar))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        if item_index != active_index {
                            this.reset_studio_form_scroll();
                            this.begin_user_segmented_control_transition_from(
                                STUDIO_CATEGORY_SELECTOR_ID,
                                active_index,
                                item_index,
                                cx,
                            );
                        }
                        this.update_connection_form_state(cx, |state| {
                            if let Some(form) = state.form.as_mut() {
                                form.active_drawer = target;
                                form.ssh_algorithm_menu = None;
                                form.field_focused = false;
                                clear_connection_selection(form);
                            }
                        });
                        cx.stop_propagation();
                        cx.notify();
                    }),
                );
            strip = strip.child(tab);
        }

        let tab_bar_height =
            (STUDIO_CATEGORY_TAB_BAR_HEIGHT * studio_ui_scale(&self.tokens)).round();
        div()
            .flex_none()
            .w_full()
            .h(px(tab_bar_height))
            .bg(rgb(theme.bg_elevated))
            .border_b_1()
            .border_color(rgba((theme.border << 8) | 0xb3))
            .child(strip)
            .into_any_element()
    }

    /// Floating overlays owned by the studio (Appearance Popover for non-sidebar modes).
    pub(super) fn render_composer_overlays(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let appearance_open = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| form.appearance_popover_open);
        if appearance_open {
            return Some(self.render_appearance_popover(window, cx));
        }
        None
    }

    fn render_appearance_popover(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let viewport = window.viewport_size();
        let margin = studio_viewport_margin(window);
        let popover_width = (f32::from(viewport.width) - margin * 2.0).clamp(260.0, 520.0);
        let popover_max_height = (f32::from(viewport.height) - margin * 2.0).clamp(220.0, 500.0);
        let card = div()
            .w(px(popover_width))
            .max_h(px(popover_max_height))
            .rounded(px(self.tokens.radii.lg))
            .bg(rgb(theme.bg_elevated))
            .border_1()
            .border_color(rgb(theme.border))
            .shadow(vec![gpui::BoxShadow {
                color: rgba(0x00000050).into(),
                offset: point(px(0.0), px(14.0)),
                blur_radius: px(32.0),
                spread_radius: px(-4.0),
                inset: false,
            }])
            .p(px(18.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text_heading))
                            .child(self.i18n.t("ssh.form.appearance")),
                    )
                    .child(self.workspace_icon_action_button(
                        LucideIcon::X,
                        14.0,
                        rgb(theme.text_muted),
                        IconButtonOptions::compact(24.0),
                        |this, _, _, cx| {
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.appearance_popover_open = false;
                                }
                            });
                            cx.stop_propagation();
                            cx.notify();
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .id("composer-appearance-popover-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(
                        &self.selectable_text_scroll_handle("composer-appearance-popover-scroll"),
                    )
                    .child(self.render_connection_appearance_options(cx)),
            );

        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgba(0x00000033))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    this.update_connection_form_state(cx, |state| {
                        if let Some(form) = state.form.as_mut() {
                            form.appearance_popover_open = false;
                        }
                    });
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(oxideterm_gpui_ui::motion::fade_in(
                &self.tokens,
                "composer-appearance-popover",
                card,
                oxideterm_gpui_ui::motion::MotionDuration::Control,
            ))
            .into_any_element()
    }
}
