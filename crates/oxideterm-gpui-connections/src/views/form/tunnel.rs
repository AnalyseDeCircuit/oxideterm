// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;
use gpui::{AnimationExt, StatefulInteractiveElement};
use oxideterm_gpui_ui::{
    Scrollbar, ScrollbarAxis,
    button::button_with,
    modal::{TauriBackdropRole, staged_overlay_root, staged_overlay_scrim},
    tabular_nums_features,
};

const TUNNEL_FLOW_WIRE_WIDTH: f32 = 136.0;
const TUNNEL_FLOW_PULSE_WIDTH: f32 = 24.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TunnelKind {
    Local,
    Remote,
    Dynamic,
}

pub struct TunnelFormEnvironment {
    pub host_id: Option<String>,
    pub editing: bool,
    pub kind: TunnelKind,
    pub startup: bool,
}

fn format_tunnel_endpoint(host: &str, port: &str) -> String {
    let host = host.trim();
    let host = if host.is_empty() { "127.0.0.1" } else { host };
    let port = port.trim();
    if port.is_empty() {
        format!("{host}:—")
    } else {
        format!("{host}:{port}")
    }
}

impl ConnectionFormView {
    fn tunnel_input(
        &self,
        label: &str,
        value: &str,
        placeholder: impl Into<String>,
        field: NewConnectionField,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        form_field(
            &self.tokens,
            self.i18n.t(label),
            self.render_connection_input(value, placeholder.into(), field, false, cx),
        )
    }

    fn render_tunnel_host_select(
        &self,
        selected_id: Option<&str>,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = selected_id
            .and_then(|id| {
                self.environment
                    .connections
                    .iter()
                    .find(|host| host.id == id)
            })
            .map(|host| {
                format!(
                    "{} · {}@{}:{}",
                    host.name, host.username, host.host, host.port
                )
            })
            .unwrap_or_else(|| self.i18n.t("ssh_tunnel.choose_host"));
        super::super::select::labelled_select(
            &self.flow,
            &self.dispatch,
            &self.tokens,
            NewConnectionSelect::TunnelHost,
            self.i18n.t("ssh_tunnel.host"),
            label,
            selected_id.is_none(),
            disabled,
            cx,
        )
    }

    fn render_tunnel_flow_node(
        &self,
        icon: LucideIcon,
        role: String,
        endpoint: String,
        is_source: bool,
        mono_font: &SharedString,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(4.0))
            .child(
                div()
                    .flex_none()
                    .size(px(32.0))
                    .rounded(px(self.tokens.radii.md))
                    .bg(if is_source {
                        rgba((theme.accent << 8) | 0x1e)
                    } else {
                        rgb(theme.bg_card)
                    })
                    .border_1()
                    .border_color(if is_source {
                        rgba((theme.accent << 8) | 0x55)
                    } else {
                        rgb(theme.border)
                    })
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(Self::render_lucide_icon(
                        icon,
                        16.0,
                        rgb(if is_source {
                            theme.accent
                        } else {
                            theme.text_muted
                        }),
                    )),
            )
            .child(
                div()
                    .max_w_full()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(role),
            )
            .child(
                div()
                    .max_w_full()
                    .truncate()
                    .font_family(mono_font.clone())
                    .font_features(tabular_nums_features())
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.text_heading))
                    .child(endpoint),
            )
            .into_any_element()
    }

    fn render_tunnel_flow_pulse(
        &self,
        id: (&'static str, u64),
        remote: bool,
        phase_offset: f32,
        wire_width: f32,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(TUNNEL_FLOW_PULSE_WIDTH))
            .rounded_full()
            .bg(rgb(theme.accent))
            .with_animation(
                id,
                gpui::Animation::new(oxideterm_gpui_ui::motion::scaled_duration(
                    &self.tokens,
                    2800,
                ))
                .repeat()
                .with_easing(|progress| progress),
                move |pulse, progress| {
                    let phase = (progress + phase_offset) % 1.0;
                    let travel = if remote { 1.0 - phase } else { phase };
                    let left =
                        (wire_width + TUNNEL_FLOW_PULSE_WIDTH) * travel - TUNNEL_FLOW_PULSE_WIDTH;
                    let opacity = (if phase < 0.5 {
                        phase * 5.0
                    } else {
                        (1.0 - phase) * 5.0
                    })
                    .clamp(0.0, 1.0);
                    pulse.left(px(left)).opacity(opacity)
                },
            )
            .into_any_element()
    }

    /// Live traffic-direction preview adapted from the mobile tunnel flow diagram:
    /// maps the local machine, encrypted SSH pipe, and selected remote host.
    #[allow(clippy::too_many_arguments)]
    fn render_tunnel_flow_card(
        &self,
        kind: TunnelKind,
        host_name: Option<&str>,
        bind_host: &str,
        bind_port: &str,
        target_host: &str,
        target_port: &str,
        content_width: f32,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = SharedString::from(self.tokens.metrics.markdown_code_font_family);
        let wire_width = (content_width * 0.24)
            .clamp(76.0, TUNNEL_FLOW_WIRE_WIDTH)
            .round();
        let remote = kind == TunnelKind::Remote;
        let dynamic = kind == TunnelKind::Dynamic;
        let listener = format_tunnel_endpoint(bind_host, bind_port);
        let target = format_tunnel_endpoint(target_host, target_port);
        let local_endpoint = if remote {
            target.clone()
        } else {
            listener.clone()
        };
        let remote_endpoint = if remote {
            listener
        } else if dynamic {
            "SOCKS5".to_string()
        } else {
            target
        };
        let remote_role = host_name
            .map(str::to_owned)
            .unwrap_or_else(|| self.i18n.t("forwards.form.remote_server"));
        let animate = self.tokens.motion.enabled && self.tokens.motion.spatial_enabled;
        let direction_key = u64::from(remote);

        let mut wire_track = div()
            .absolute()
            .left_0()
            .right_0()
            .h(px(2.0))
            .rounded_full()
            .bg(rgb(theme.border))
            .overflow_hidden();
        if animate {
            wire_track = wire_track
                .child(self.render_tunnel_flow_pulse(
                    ("ssh-tunnel-flow-pulse-a", direction_key),
                    remote,
                    0.0,
                    wire_width,
                ))
                .child(self.render_tunnel_flow_pulse(
                    ("ssh-tunnel-flow-pulse-b", direction_key),
                    remote,
                    0.5,
                    wire_width,
                ));
        }

        div()
            .w_full()
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_sunken << 8) | 0x90))
            .border_1()
            .border_color(rgba((theme.border << 8) | 0x66))
            .px(px(14.0))
            .py(px(12.0))
            .flex()
            .items_start()
            .gap(px(10.0))
            .child(self.render_tunnel_flow_node(
                LucideIcon::Monitor,
                self.i18n.t("forwards.form.local_client"),
                local_endpoint,
                !remote,
                &mono_font,
            ))
            .child(
                div()
                    .flex_none()
                    .w(px(wire_width))
                    .h(px(32.0))
                    .relative()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(wire_track)
                    .child(
                        div()
                            .relative()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded_full()
                            .bg(rgb(theme.bg_card))
                            .border_1()
                            .border_color(rgba((theme.accent << 8) | 0x48))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Lock,
                                10.5,
                                rgb(theme.accent),
                            ))
                            .child(
                                div()
                                    .font_family(mono_font.clone())
                                    .text_size(px(10.5))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(rgb(theme.accent))
                                    .child(if remote { "◀ SSH" } else { "SSH ▶" }),
                            ),
                    ),
            )
            .child(self.render_tunnel_flow_node(
                LucideIcon::Server,
                remote_role,
                remote_endpoint,
                remote,
                &mono_font,
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_tunnel_endpoint_section(
        &self,
        icon: LucideIcon,
        side_label: String,
        badge: Option<String>,
        accent_header: bool,
        host_label: &str,
        host_value: &str,
        host_field: NewConnectionField,
        port_value: &str,
        port_field: NewConnectionField,
        section_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(Self::render_lucide_icon(
                                icon,
                                13.5,
                                rgb(if accent_header {
                                    theme.accent
                                } else {
                                    theme.text_muted
                                }),
                            ))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text_heading))
                                    .child(side_label),
                            ),
                    )
                    .when_some(badge, |row, text| {
                        row.child(
                            div()
                                .flex_none()
                                .px(px(6.0))
                                .py(px(1.5))
                                .rounded(px(self.tokens.radii.xs))
                                .bg(rgba((theme.accent << 8) | 0x1c))
                                .text_size(px(10.5))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(rgb(theme.accent))
                                .child(text),
                        )
                    }),
            )
            .child(self.render_unified_host_port_bar(
                self.i18n.t(host_label),
                host_field,
                host_value,
                "127.0.0.1".to_string(),
                self.i18n.t("forwards.form.port_placeholder"),
                port_field,
                port_value,
                "1–65535".to_string(),
                section_width,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_ssh_tunnel_form(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let form = self.environment.tunnel.as_ref()?;
        let theme = self.tokens.ui;
        let modal_max_height = f32::from(window.viewport_size().height)
            * self.tokens.metrics.modal_max_viewport_height_ratio;
        let shows_transport_selector = !form.editing;
        let (modal_width, studio_height) = composer::studio_modal_size(
            window,
            &self.tokens,
            shows_transport_selector,
            modal_max_height,
        );
        let studio_layout = composer::StudioLayoutMetrics::compute(
            modal_width,
            &self.tokens,
            shows_transport_selector,
        );
        let (name, group, bind_host, bind_port, target_host, target_port, error) = {
            let draft = self.connection_form_state(cx).form.as_ref()?;
            (
                draft.name.clone(),
                draft.group.clone(),
                draft.host.clone(),
                draft.port.clone(),
                draft.tunnel_target_host.clone(),
                draft.tunnel_target_port.clone(),
                draft.error.clone(),
            )
        };
        let selected_host_name = form
            .host_id
            .as_deref()
            .and_then(|id| {
                self.environment
                    .connections
                    .iter()
                    .find(|host| host.id == id)
            })
            .map(|host| host.name.as_str());
        let valid_port = |value: &str| value.trim().parse::<u16>().is_ok_and(|port| port > 0);
        let can_save = !name.trim().is_empty()
            && selected_host_name.is_some()
            && (form.editing
                || (!bind_host.trim().is_empty()
                    && valid_port(&bind_port)
                    && (form.kind == TunnelKind::Dynamic
                        || (!target_host.trim().is_empty() && valid_port(&target_port)))));

        let name_input =
            self.tunnel_input("ssh_tunnel.name", &name, "", NewConnectionField::Name, cx);
        let group_input = super::super::select::group_select(
            &self.flow,
            &self.dispatch,
            &self.tokens,
            &self.i18n,
            self.i18n.t("ssh_tunnel.group"),
            &group,
            cx,
        );
        let identity_row = if studio_layout.narrow_content {
            div()
                .w_full()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .child(name_input)
                .child(group_input)
        } else {
            let group_width = (studio_layout.content_width * 0.30)
                .clamp(148.0, 220.0)
                .round();
            div()
                .flex()
                .flex_row()
                .gap(px(12.0))
                .child(div().flex_1().min_w_0().child(name_input))
                .child(div().w(px(group_width)).flex_none().child(group_input))
        };

        let mut body = div()
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(identity_row)
            .child(self.render_tunnel_host_select(form.host_id.as_deref(), form.editing, cx))
            .child(self.render_tunnel_flow_card(
                form.kind,
                selected_host_name,
                &bind_host,
                &bind_port,
                &target_host,
                &target_port,
                studio_layout.content_width,
            ));

        if !form.editing {
            let remote = form.kind == TunnelKind::Remote;
            let dynamic = form.kind == TunnelKind::Dynamic;
            let types = self.render_sunken_segmented_track().children(
                [
                    (TunnelKind::Local, "forwards.form.type_local"),
                    (TunnelKind::Remote, "forwards.form.type_remote"),
                    (TunnelKind::Dynamic, "forwards.form.type_dynamic"),
                ]
                .into_iter()
                .enumerate()
                .map(|(index, (kind, label))| {
                    self.render_sunken_segmented_tab(
                        SharedString::from(format!("ssh-tunnel-kind-{index}")),
                        self.i18n.t(label),
                        form.kind == kind,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            this.close_new_connection_select(cx);
                            this.request(ConnectionsWindowRequest::SetTunnelKind(kind), None, cx);
                            this.update_connection_form_state(cx, |state| {
                                if let Some(form) = state.form.as_mut() {
                                    form.field_focused = false;
                                    form.selected_field = None;
                                    form.focused_field = NewConnectionField::Host;
                                    form.error = None;
                                }
                            });
                            this.request(ConnectionsWindowRequest::ClearComposition, None, cx);
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
                }),
            );

            let stacked_endpoints = studio_layout.narrow_content || dynamic;
            let endpoint_width = if stacked_endpoints {
                studio_layout.content_width
            } else {
                (studio_layout.content_width - 14.0) * 0.5
            };
            let listener_section = self.render_tunnel_endpoint_section(
                if remote {
                    LucideIcon::Server
                } else {
                    LucideIcon::Monitor
                },
                self.i18n.t(if remote {
                    "forwards.form.remote_server"
                } else {
                    "forwards.form.local_client"
                }),
                dynamic.then(|| self.i18n.t("forwards.form.socks5_mode")),
                true,
                "forwards.form.bind_address",
                &bind_host,
                NewConnectionField::Host,
                &bind_port,
                NewConnectionField::Port,
                endpoint_width,
                cx,
            );

            let endpoints = if dynamic {
                div().w_full().child(listener_section)
            } else {
                let target_section = self.render_tunnel_endpoint_section(
                    if remote {
                        LucideIcon::Monitor
                    } else {
                        LucideIcon::Server
                    },
                    self.i18n.t(if remote {
                        "forwards.form.local_client"
                    } else {
                        "forwards.form.remote_server"
                    }),
                    None,
                    false,
                    "forwards.form.target_address",
                    &target_host,
                    NewConnectionField::TunnelTargetHost,
                    &target_port,
                    NewConnectionField::TunnelTargetPort,
                    endpoint_width,
                    cx,
                );
                if studio_layout.narrow_content {
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .gap(px(10.0))
                        .child(listener_section)
                        .child(self.form_hairline())
                        .child(target_section)
                } else {
                    div()
                        .w_full()
                        .flex()
                        .flex_row()
                        .gap(px(14.0))
                        .child(listener_section)
                        .child(target_section)
                }
            };

            body = body
                .child(form_field(
                    &self.tokens,
                    self.i18n.t("forwards.form.type"),
                    types,
                ))
                .child(self.form_hairline())
                .child(endpoints);
        }

        let startup_enabled = form.startup;
        body = body.child(self.form_hairline()).child(
            div()
                .id("ssh-tunnel-startup")
                .w_full()
                .min_h(px(34.0))
                .py(px(4.0))
                .px(px(2.0))
                .rounded(px(self.tokens.radii.sm))
                .cursor_pointer()
                .hover(|el| el.bg(rgba((theme.bg_hover << 8) | 0x45)))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t("ssh_tunnel.startup")),
                )
                .child(switch_with(
                    &self.tokens,
                    "track-ssh-tunnel-startup",
                    "",
                    startup_enabled,
                    SwitchOptions::default(),
                ))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.request(ConnectionsWindowRequest::ToggleTunnelStartup, None, cx);
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ),
        );

        if let Some(error) = error {
            body = body.child(self.render_prompt_feedback_box(error, false));
        }

        let scroll = self.selectable_text_scroll_handle("ssh-tunnel-form-scroll");
        let dialog = modal_container(&self.tokens)
            .bg(rgb(theme.bg_elevated))
            .overflow_hidden()
            .w(px(modal_width))
            .h(px(studio_height))
            .flex()
            .flex_col()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(self.render_studio_top_chrome(
                self.i18n.t("ssh_tunnel.title"),
                Some(self.i18n.t("ssh_tunnel.description")),
                shows_transport_selector,
                false,
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .when(shows_transport_selector, |content| {
                        content.child(self.render_studio_protocol_sidebar(studio_layout, cx))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .relative()
                            .child(
                                div()
                                    .id("ssh-tunnel-form-scroll")
                                    .size_full()
                                    .px(px(studio_layout.content_padding_x))
                                    .py(px(16.0))
                                    .overflow_y_scroll()
                                    .track_scroll(&scroll)
                                    .on_scroll(cx.listener(|this, _, _, cx| {
                                        if this.connection_form_state(cx).open_select.is_some() {
                                            this.close_new_connection_select(cx);
                                            this.clear_new_connection_select_anchor(cx);
                                            cx.notify();
                                        }
                                    }))
                                    .child(body),
                            )
                            .child(Scrollbar::new(&scroll).axis(ScrollbarAxis::Vertical)),
                    ),
            )
            .child(
                modal_footer(&self.tokens)
                    .flex_none()
                    .h_auto()
                    .min_h(px(self.tokens.metrics.modal_footer_height))
                    .flex_wrap()
                    .bg(rgb(theme.bg_panel))
                    .px(px(studio_layout.content_padding_x))
                    .py(px(13.0))
                    .gap(px(8.0))
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("common.cancel"),
                            ButtonOptions::default(),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.request(
                                    ConnectionsWindowRequest::CloseTunnel,
                                    Some(window),
                                    cx,
                                )
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("ssh.form.save"),
                            ButtonOptions {
                                disabled: !can_save,
                                ..Default::default()
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                if can_save {
                                    this.request(
                                        ConnectionsWindowRequest::SaveTunnel { start: false },
                                        Some(window),
                                        cx,
                                    );
                                }
                            }),
                        ),
                    )
                    .child(
                        button_with(
                            &self.tokens,
                            self.i18n.t("forwards.form.save_and_start"),
                            ButtonOptions {
                                variant: ButtonVariant::Default,
                                disabled: !can_save,
                                ..Default::default()
                            },
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _, window, cx| {
                                if can_save {
                                    this.request(
                                        ConnectionsWindowRequest::SaveTunnel { start: true },
                                        Some(window),
                                        cx,
                                    );
                                }
                            }),
                        ),
                    ),
            );
        Some(
            staged_overlay_root()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        this.request(ConnectionsWindowRequest::CloseTunnel, Some(window), cx);
                        cx.stop_propagation();
                    }),
                )
                .child(staged_overlay_scrim(
                    &self.tokens,
                    "ssh-tunnel-form-scrim",
                    TauriBackdropRole::Dialog,
                    true,
                ))
                .child(dialog)
                .into_any_element(),
        )
    }
}
