// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use crate::form_state::ConnectionFormDrawer;
use gpui::AnimationExt;
use oxideterm_gpui_ui::{Scrollbar, ScrollbarAxis, modal::rounded_shell_child_radius};
use oxideterm_settings_model::parse_rgb24_hex;

pub(super) const CONNECTION_STUDIO_WIDTH: f32 = 880.0;
pub(super) const CONNECTION_STUDIO_EDIT_WIDTH: f32 = 760.0;
pub(super) const CONNECTION_STUDIO_MIN_HEIGHT: f32 = 560.0;
pub(super) const CONNECTION_STUDIO_MAX_HEIGHT: f32 = 680.0;
pub(super) const CONNECTION_STUDIO_VIEWPORT_MARGIN: f32 = 16.0;
pub(super) const CONNECTION_STUDIO_SIDEBAR_WIDTH: f32 = 188.0;

/// Computes a viewport-locked outer frame for every multi-page Connection Studio
/// shell (create, edit, standalone SFTP, SSH tunnel) so switching protocols,
/// sidebar pages, auth tabs, or algorithm categories never shifts modal bounds
/// and never overflows small windows.
pub(super) fn studio_modal_size(
    window: &Window,
    shows_transport_selector: bool,
    modal_max_height: f32,
) -> (f32, f32) {
    let viewport = window.viewport_size();
    let viewport_w = f32::from(viewport.width);
    let viewport_h = f32::from(viewport.height);
    let desired_w = if shows_transport_selector {
        CONNECTION_STUDIO_WIDTH
    } else {
        CONNECTION_STUDIO_EDIT_WIDTH
    };
    let max_w = (viewport_w - CONNECTION_STUDIO_VIEWPORT_MARGIN * 2.0).max(0.0);
    let width = desired_w.min(max_w);
    let max_h =
        modal_max_height.min((viewport_h - CONNECTION_STUDIO_VIEWPORT_MARGIN * 2.0).max(0.0));
    let height = (viewport_h * 0.82)
        .clamp(CONNECTION_STUDIO_MIN_HEIGHT, CONNECTION_STUDIO_MAX_HEIGHT)
        .min(max_h);
    (width, height)
}

const STUDIO_PROTOCOL_SELECTOR_ID: &str = "studio-protocol-selector";
const STUDIO_CATEGORY_SELECTOR_ID: &str = "studio-category-selector";
const STUDIO_PROTOCOL_TILE_WIDTH: f32 = 72.0;
const STUDIO_PROTOCOL_TILE_GAP: f32 = 3.0;
const STUDIO_CATEGORY_ROW_HEIGHT: f32 = 34.0;
const STUDIO_CATEGORY_ROW_GAP: f32 = 3.0;

pub(super) struct CategoryNavItem {
    /// `None` represents the primary General/Session page.
    pub(super) drawer: Option<ConnectionFormDrawer>,
    pub(super) icon: LucideIcon,
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

    fn render_identity_avatar_preview(&self, size_px: f32, icon_px: f32, cx: &App) -> AnyElement {
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
            .child(active_icon.render(icon_px, rgb(preview_color)))
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
            self.request(ConnectionsWindowRequest::LeaveTunnel, Some(window), cx);
        }
        self.reset_studio_form_scroll();
        let mut should_refresh_ports = false;
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
                should_refresh_ble_adapters = transport == NewConnectionTransport::Ble
                    && form.ble.adapters.is_empty()
                    && !form.ble.adapters_loading;
            }
        });
        self.user_motion.clear(STUDIO_CATEGORY_SELECTOR_ID);
        self.close_new_connection_select(cx);
        if should_refresh_ports {
            self.refresh_serial_ports(cx);
        }
        if should_refresh_ble_adapters {
            self.refresh_ble_adapters(cx);
        }
        cx.notify();
    }

    /// Renders the horizontally scrollable icon-over-label protocol strip with sliding motion
    /// and smart overflow arrows.
    fn render_top_protocol_strip(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let tunnel_active = self.environment.tunnel.is_some();
        let active_transport = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| form.transport)
            .unwrap_or(NewConnectionTransport::Ssh);
        let plugin_types = self.environment.plugin_types.clone();

        let mut built_in_transports = vec![
            (NewConnectionTransport::Ssh, NewConnectionField::Name),
            (NewConnectionTransport::Mosh, NewConnectionField::Name),
            (NewConnectionTransport::Telnet, NewConnectionField::Host),
            (
                NewConnectionTransport::LocalTerminal,
                NewConnectionField::Name,
            ),
            (NewConnectionTransport::Rdp, NewConnectionField::Host),
            (NewConnectionTransport::Vnc, NewConnectionField::Host),
        ];
        if cfg!(target_os = "windows") {
            built_in_transports.push((
                NewConnectionTransport::WslGraphics,
                NewConnectionField::Name,
            ));
        }
        built_in_transports.extend([
            (
                NewConnectionTransport::Serial,
                NewConnectionField::SerialPortPath,
            ),
            (
                NewConnectionTransport::Ble,
                NewConnectionField::BleProfileName,
            ),
            (
                NewConnectionTransport::StandaloneSftp,
                NewConnectionField::Name,
            ),
            (NewConnectionTransport::Ftp, NewConnectionField::Name),
        ]);
        let tunnel_index = built_in_transports.len();
        let active_index = if tunnel_active {
            tunnel_index
        } else {
            built_in_transports
                .iter()
                .position(|(transport, _)| *transport == active_transport)
                .unwrap_or(0)
        };
        let protocol_stride = STUDIO_PROTOCOL_TILE_WIDTH + STUDIO_PROTOCOL_TILE_GAP;
        let render_active_protocol_surface = |tile_index: usize| -> AnyElement {
            let surface = div()
                .absolute()
                .inset_0()
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgba((theme.accent << 8) | 0x66))
                .bg(rgb(theme.bg_card));
            let Some((generation, _)) =
                self.segmented_control_user_transition(STUDIO_PROTOCOL_SELECTOR_ID, tile_index)
            else {
                return surface.into_any_element();
            };
            let Some(previous_index) =
                self.segmented_control_user_previous_index(STUDIO_PROTOCOL_SELECTOR_ID, tile_index)
            else {
                return surface.into_any_element();
            };
            if previous_index == tile_index {
                return surface.into_any_element();
            }
            let Some(motion) = oxideterm_gpui_ui::segmented_control_motion(&self.tokens) else {
                return surface.into_any_element();
            };
            let start_dx = (previous_index as f32 - tile_index as f32) * protocol_stride;
            surface
                .with_animation(
                    (STUDIO_PROTOCOL_SELECTOR_ID, generation),
                    gpui::Animation::new(motion.duration),
                    move |el, delta| {
                        let eased = oxideterm_gpui_ui::motion::ease_out_cubic(delta);
                        if motion.spatial {
                            el.left(px(oxideterm_gpui_ui::motion::lerp(start_dx, 0.0, eased)))
                        } else {
                            el.opacity(eased)
                        }
                    },
                )
                .into_any_element()
        };

        let mut strip = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(px(STUDIO_PROTOCOL_TILE_GAP))
            .px(px(16.0))
            .pb(px(10.0));

        for (tile_index, (transport, focus_field)) in built_in_transports.into_iter().enumerate() {
            let (icon, label) = self.transport_display_meta(transport);
            let active = tile_index == active_index;
            let tile = div()
                .id(SharedString::from(format!(
                    "studio-transport-{transport:?}"
                )))
                .debug_selector(|| format!("new-connection-transport-{transport:?}"))
                .relative()
                .flex_none()
                .w(px(STUDIO_PROTOCOL_TILE_WIDTH))
                .px(px(6.0))
                .py(px(6.0))
                .rounded(px(self.tokens.radii.md))
                .cursor_pointer()
                .when(!active, |el| {
                    el.hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                })
                .when(active, |el| {
                    el.child(render_active_protocol_surface(tile_index))
                })
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .child(Self::render_lucide_icon(
                    icon,
                    16.0,
                    rgb(if active {
                        theme.accent
                    } else {
                        theme.text_muted
                    }),
                ))
                .child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(px(11.0))
                        .line_height(px(14.0))
                        .font_weight(if active {
                            gpui::FontWeight::MEDIUM
                        } else {
                            gpui::FontWeight::NORMAL
                        })
                        .text_color(rgb(if active {
                            theme.text_heading
                        } else {
                            theme.text_secondary
                        }))
                        .child(label),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, window, cx| {
                        if tile_index != active_index {
                            this.begin_user_segmented_control_transition_from(
                                STUDIO_PROTOCOL_SELECTOR_ID,
                                active_index,
                                tile_index,
                                cx,
                            );
                        }
                        this.select_composer_transport(transport, focus_field, window, cx);
                        cx.stop_propagation();
                    }),
                );
            strip = strip.child(tile);
        }

        strip = strip.child(
            div()
                .id("studio-transport-tunnel")
                .relative()
                .flex_none()
                .w(px(STUDIO_PROTOCOL_TILE_WIDTH))
                .px(px(6.0))
                .py(px(6.0))
                .rounded(px(self.tokens.radii.md))
                .cursor_pointer()
                .when(!tunnel_active, |el| {
                    el.hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                })
                .when(tunnel_active, |el| {
                    el.child(render_active_protocol_surface(tunnel_index))
                })
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(4.0))
                .child(Self::render_lucide_icon(
                    LucideIcon::Network,
                    16.0,
                    rgb(if tunnel_active {
                        theme.accent
                    } else {
                        theme.text_muted
                    }),
                ))
                .child(
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(px(11.0))
                        .line_height(px(14.0))
                        .font_weight(if tunnel_active {
                            gpui::FontWeight::MEDIUM
                        } else {
                            gpui::FontWeight::NORMAL
                        })
                        .text_color(rgb(if tunnel_active {
                            theme.text_heading
                        } else {
                            theme.text_secondary
                        }))
                        .child(self.i18n.t("ssh_tunnel.title")),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        if tunnel_index != active_index {
                            this.begin_user_segmented_control_transition_from(
                                STUDIO_PROTOCOL_SELECTOR_ID,
                                active_index,
                                tunnel_index,
                                cx,
                            );
                        }
                        if this.environment.tunnel.is_none() {
                            this.request(ConnectionsWindowRequest::OpenTunnel, Some(window), cx);
                        }
                        cx.stop_propagation();
                    }),
                ),
        );

        for (plugin, definition) in plugin_types {
            strip = strip.child(
                div()
                    .flex_none()
                    .w(px(STUDIO_PROTOCOL_TILE_WIDTH))
                    .px(px(6.0))
                    .py(px(6.0))
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(4.0))
                    .child(Self::render_lucide_icon(
                        LucideIcon::from_plugin_name(&definition.icon),
                        16.0,
                        rgb(theme.text_muted),
                    ))
                    .child(
                        div()
                            .max_w_full()
                            .truncate()
                            .text_size(px(11.0))
                            .line_height(px(14.0))
                            .text_color(rgb(theme.text_secondary))
                            .child(definition.title),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            this.close_new_connection_select(cx);
                            this.request(
                                ConnectionsWindowRequest::OpenPluginForm {
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

        let scroll = self.selectable_text_scroll_handle("new-connection-transport-scroll");
        let max_x = f32::from(scroll.max_offset().x);
        let cur_x = f32::from(scroll.offset().x);
        let can_scroll_left = max_x > 1.0 && cur_x < -1.0;
        let can_scroll_right = max_x > 1.0 && cur_x > -max_x + 1.0;
        let scroll_left_handle = scroll.clone();
        let scroll_right_handle = scroll.clone();

        div()
            .w_full()
            .relative()
            .child(
                div()
                    .id("new-connection-transport-scroll")
                    .w_full()
                    .overflow_x_scroll()
                    .track_scroll(&scroll)
                    .on_scroll(cx.listener(|_, _, _, cx| {
                        // Refresh overflow chevrons as the user scrolls horizontally.
                        cx.notify();
                    }))
                    .child(strip),
            )
            .child(Scrollbar::new(&scroll).axis(ScrollbarAxis::Horizontal))
            .when(can_scroll_left, |bar| {
                bar.child(
                    div()
                        .id("studio-transport-scroll-left")
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom(px(10.0))
                        .w(px(28.0))
                        .bg(rgba((theme.bg_panel << 8) | 0xe8))
                        .border_r_1()
                        .border_color(rgba((theme.border << 8) | 0x80))
                        .cursor_pointer()
                        .hover(|el| el.bg(rgb(theme.bg_hover)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::render_lucide_icon(
                            LucideIcon::ChevronLeft,
                            14.0,
                            rgb(theme.text),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, _, _, cx| {
                                let current = f32::from(scroll_left_handle.offset().x);
                                let next = (current + 180.0).min(0.0);
                                scroll_left_handle.set_offset(point(px(next), px(0.0)));
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
            })
            .when(can_scroll_right, |bar| {
                bar.child(
                    div()
                        .id("studio-transport-scroll-right")
                        .absolute()
                        .right_0()
                        .top_0()
                        .bottom(px(10.0))
                        .w(px(28.0))
                        .bg(rgba((theme.bg_panel << 8) | 0xe8))
                        .border_l_1()
                        .border_color(rgba((theme.border << 8) | 0x80))
                        .cursor_pointer()
                        .hover(|el| el.bg(rgb(theme.bg_hover)))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Self::render_lucide_icon(
                            LucideIcon::ChevronRight,
                            14.0,
                            rgb(theme.text),
                        ))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |_, _, _, cx| {
                                let current = f32::from(scroll_right_handle.offset().x);
                                let max_offset = f32::from(scroll_right_handle.max_offset().x);
                                let next = (current - 180.0).max(-max_offset);
                                scroll_right_handle.set_offset(point(px(next), px(0.0)));
                                cx.stop_propagation();
                                cx.notify();
                            }),
                        ),
                )
            })
            .into_any_element()
    }

    /// Unified top chrome with proper GPUI inner top radius (fixes rectangular overflow clipping)
    /// and the full-row horizontally scrollable protocol bar.
    pub(super) fn render_studio_top_chrome(
        &self,
        title: String,
        subtitle: Option<String>,
        shows_transport_strip: bool,
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
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .px(px(20.0))
                    .pt(px(13.0))
                    .pb(px(if shows_transport_strip { 8.0 } else { 13.0 }))
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
                                            .text_size(px(15.0))
                                            .line_height(px(20.0))
                                            .font_weight(gpui::FontWeight::SEMIBOLD)
                                            .text_color(rgb(theme.text_heading))
                                            .child(title),
                                    )
                                    .when_some(
                                        subtitle
                                            .filter(|s| !s.is_empty() && !shows_transport_strip),
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
                    ),
            )
            .when(shows_transport_strip, |chrome| {
                chrome.child(self.render_top_protocol_strip(cx))
            })
            .into_any_element()
    }

    /// Left vertical category sidebar for the active protocol (WindTerm-style orthogonal navigation).
    pub(super) fn render_studio_category_sidebar(
        &self,
        items: Vec<CategoryNavItem>,
        active_drawer: Option<ConnectionFormDrawer>,
        shows_appearance_preview: bool,
        appearance_drawer_target: Option<ConnectionFormDrawer>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (preview_name, preview_detail) = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .map(|form| {
                let (_, transport_label) = self.transport_display_meta(form.transport);
                let name = if !form.name.trim().is_empty() {
                    form.name.trim().to_owned()
                } else if !form.host.trim().is_empty() {
                    form.host.trim().to_owned()
                } else {
                    transport_label
                };
                let detail = if !form.host.trim().is_empty() {
                    if !form.username.trim().is_empty() {
                        format!(
                            "{}@{}:{}",
                            form.username.trim(),
                            form.host.trim(),
                            form.port.trim()
                        )
                    } else {
                        format!("{}:{}", form.host.trim(), form.port.trim())
                    }
                } else {
                    self.i18n.t("ssh.form.appearance")
                };
                (name, detail)
            })
            .unwrap_or_default();

        let active_index = items
            .iter()
            .position(|item| item.drawer == active_drawer)
            .unwrap_or(0);
        let appearance_index = appearance_drawer_target
            .and_then(|drawer| items.iter().position(|item| item.drawer == Some(drawer)));
        let row_stride = STUDIO_CATEGORY_ROW_HEIGHT + STUDIO_CATEGORY_ROW_GAP;

        let mut list = div().flex().flex_col().gap(px(STUDIO_CATEGORY_ROW_GAP));
        for (item_index, item) in items.into_iter().enumerate() {
            let target = item.drawer;
            let active = item_index == active_index;
            let configured = item.summary.is_some();
            let selection_surface = active.then(|| {
                let surface = div()
                    .absolute()
                    .inset_0()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_card))
                    .flex()
                    .items_center()
                    .pl(px(10.0))
                    .child(
                        div()
                            .w(px(2.5))
                            .h(px(14.0))
                            .rounded_full()
                            .bg(rgb(theme.accent)),
                    );
                self.user_motion.render_vertical(
                    STUDIO_CATEGORY_SELECTOR_ID,
                    active_index,
                    &self.tokens,
                    surface,
                )
            });
            let row = div()
                .id(SharedString::from(format!("studio-category-{target:?}")))
                .debug_selector(move || format!("studio-category-{target:?}"))
                .relative()
                .w_full()
                .h(px(STUDIO_CATEGORY_ROW_HEIGHT))
                .px(px(10.0))
                .rounded(px(self.tokens.radii.md))
                .cursor_pointer()
                .when(!active, |el| {
                    el.hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x99)))
                })
                .when_some(selection_surface, |el, surface| el.child(surface))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().w(px(2.5)))
                        .child(Self::render_lucide_icon(
                            item.icon,
                            14.0,
                            rgb(if active || configured {
                                theme.accent
                            } else {
                                theme.text_muted
                            }),
                        ))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
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
                                .child(item.label),
                        ),
                )
                .when_some(item.summary, |row, summary| {
                    if summary == "•" {
                        row.child(div().size(px(6.0)).rounded_full().bg(rgb(theme.accent)))
                    } else {
                        row.child(
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.close_new_connection_select(cx);
                        if item_index != active_index {
                            this.reset_studio_form_scroll();
                            let offset_y = (active_index as f32 - item_index as f32) * row_stride;
                            this.begin_user_segmented_control_transition_with_history(
                                STUDIO_CATEGORY_SELECTOR_ID,
                                Some(active_index),
                                item_index,
                                Some(offset_y),
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
            list = list.child(row);
        }

        div()
            .w(px(CONNECTION_STUDIO_SIDEBAR_WIDTH))
            .flex_none()
            .min_h_0()
            .bg(rgb(theme.bg_panel))
            .border_r_1()
            .border_color(rgb(theme.border))
            .flex()
            .flex_col()
            .justify_between()
            .child(div().px(px(10.0)).py(px(14.0)).child(list))
            .when(shows_appearance_preview, |sidebar| {
                sidebar.child(
                    div()
                        .flex_none()
                        .p(px(10.0))
                        .border_t_1()
                        .border_color(rgb(theme.border))
                        .child(
                            div()
                                .id("studio-appearance-card")
                                .w_full()
                                .p(px(8.0))
                                .rounded(px(self.tokens.radii.md))
                                .bg(rgb(theme.bg_card))
                                .border_1()
                                .border_color(rgb(theme.border))
                                .cursor_pointer()
                                .hover(|el| el.border_color(rgb(theme.accent)))
                                .flex()
                                .items_center()
                                .gap(px(9.0))
                                .child(self.render_identity_avatar_preview(32.0, 16.0, cx))
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
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text_heading))
                                                .child(preview_name),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(10.5))
                                                .text_color(rgb(theme.text_muted))
                                                .child(preview_detail),
                                        ),
                                )
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.close_new_connection_select(cx);
                                        if let Some(target_idx) = appearance_index
                                            && target_idx != active_index
                                        {
                                            this.reset_studio_form_scroll();
                                            let offset_y =
                                                (active_index as f32 - target_idx as f32)
                                                    * row_stride;
                                            this.begin_user_segmented_control_transition_with_history(
                                                STUDIO_CATEGORY_SELECTOR_ID,
                                                Some(active_index),
                                                target_idx,
                                                Some(offset_y),
                                                cx,
                                            );
                                        }
                                        this.update_connection_form_state(cx, |state| {
                                            if let Some(form) = state.form.as_mut() {
                                                if let Some(drawer) = appearance_drawer_target {
                                                    form.active_drawer = Some(drawer);
                                                    form.icon_picker_expanded = true;
                                                    form.ssh_algorithm_menu = None;
                                                } else {
                                                    form.appearance_popover_open =
                                                        !form.appearance_popover_open;
                                                    form.icon_picker_expanded = true;
                                                }
                                                form.field_focused = false;
                                                clear_connection_selection(form);
                                            }
                                        });
                                        cx.stop_propagation();
                                        cx.notify();
                                    }),
                                ),
                        ),
                )
            })
            .into_any_element()
    }

    /// Floating overlays owned by the studio (Appearance Popover for non-sidebar modes).
    pub(super) fn render_composer_overlays(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let appearance_open = self
            .connection_form_state(cx)
            .form
            .as_ref()
            .is_some_and(|form| form.appearance_popover_open);
        if appearance_open {
            return Some(self.render_appearance_popover(cx));
        }
        None
    }

    fn render_appearance_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let card = div()
            .w(px(500.0))
            .max_h(px(480.0))
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
