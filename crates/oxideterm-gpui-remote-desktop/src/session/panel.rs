// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::rc::Rc;

use gpui::{
    AnchoredPositionMode, AnyElement, Corner, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, ScrollEvent, WeakEntity, anchored, deferred, svg,
};
use oxideterm_gpui_ui::{
    button::{
        ButtonOptions, ButtonRadius, ButtonSize, ButtonVariant, IconButtonOptions,
        ToolbarButtonOptions, icon_button, toolbar_button,
    },
    context_menu::{
        ContextMenuActionableStyle, context_menu_action, context_menu_actionable_row,
        context_menu_backdrop, context_menu_event_boundary, context_menu_item_height_estimate,
        context_menu_separator_height_estimate,
    },
    dropdown_menu::{
        DropdownMenuItemKind, dropdown_menu_content, dropdown_menu_item, dropdown_menu_label,
        dropdown_menu_separator,
    },
    modal::overlay_content_boundary,
    select::{SelectAnchorId, select_anchor_probe},
};
use oxideterm_i18n::I18n;

use super::*;
use crate::remote_desktop_surface_with_geometry;

#[derive(Clone)]
pub enum DesktopWindowAction {
    FocusKeyboard(TabId),
    Reconnect(TabId),
    Disconnect(TabId),
    DismissOverlays,
    ShowCapabilityTooltip {
        text: String,
        position: Point<Pixels>,
    },
    ClearCapabilityTooltip,
}

type WindowActionHandler = Rc<dyn Fn(DesktopWindowAction, &mut Window, &mut App)>;

pub(super) struct DesktopChrome<'a> {
    pub(super) tokens: &'a ThemeTokens,
    pub(super) i18n: &'a I18n,
    desktops: Entity<RemoteDesktopWorkspaceEntity>,
    window_action: WindowActionHandler,
}

type DesktopAppearance = Box<dyn Fn(&App) -> Option<(ThemeTokens, I18n)>>;
type DesktopCertificate =
    Box<dyn Fn(TabId, u64, RemoteDesktopCertificateChallengeState, &mut App) -> Option<AnyElement>>;

pub struct RemoteDesktopPanelView {
    session: WeakEntity<RemoteDesktopSessionEntity>,
    desktops: WeakEntity<RemoteDesktopWorkspaceEntity>,
    appearance: DesktopAppearance,
    certificate: DesktopCertificate,
    window_action: WindowActionHandler,
    _session_changed: Subscription,
}

impl RemoteDesktopPanelView {
    pub fn mount(
        session: &Entity<RemoteDesktopSessionEntity>,
        desktops: &Entity<RemoteDesktopWorkspaceEntity>,
        appearance: impl Fn(&App) -> Option<(ThemeTokens, I18n)> + 'static,
        certificate: impl Fn(
            TabId,
            u64,
            RemoteDesktopCertificateChallengeState,
            &mut App,
        ) -> Option<AnyElement>
        + 'static,
        window_action: impl Fn(DesktopWindowAction, &mut Window, &mut App) + 'static,
        cx: &mut App,
    ) {
        let panel = cx.new(|cx: &mut Context<Self>| {
            let session_changed = cx.observe(session, |_, _, cx| cx.notify());
            Self {
                session: session.downgrade(),
                desktops: desktops.downgrade(),
                appearance: Box::new(appearance),
                certificate: Box::new(certificate),
                window_action: Rc::new(window_action),
                _session_changed: session_changed,
            }
        });
        session.update(cx, |session, _| session.panel = Some(panel));
    }
}

impl Render for RemoteDesktopPanelView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (Some(session), Some(desktops), Some((tokens, i18n))) = (
            self.session.upgrade(),
            self.desktops.upgrade(),
            (self.appearance)(cx),
        ) else {
            return div().into_any_element();
        };
        let tab_id = session.read(cx).tab_id;
        let resize_menu_open = desktops.read(cx).resize_menu_open(tab_id);
        let certificate_dialog =
            session
                .read(cx)
                .certificate_prompt()
                .and_then(|(generation, challenge)| {
                    (self.certificate)(tab_id, generation, challenge, cx)
                });
        let chrome = DesktopChrome {
            tokens: &tokens,
            i18n: &i18n,
            desktops,
            window_action: self.window_action.clone(),
        };
        session.update(cx, |session, cx| {
            session.bind_file_clipboard(window, cx);
            chrome.render(session, window, cx, resize_menu_open, certificate_dialog)
        })
    }
}

impl DesktopChrome<'_> {
    fn request(
        &self,
        action: DesktopWindowAction,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let action_handler = self.window_action.clone();
        move |_, window, cx| action_handler(action.clone(), window, cx)
    }

    fn render(
        &self,
        session: &RemoteDesktopSessionEntity,
        window: &mut Window,
        cx: &mut Context<RemoteDesktopSessionEntity>,
        resize_menu_open: bool,
        certificate_dialog: Option<AnyElement>,
    ) -> AnyElement {
        let tab_id = session.tab_id;
        let resize_session = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let scale_factor = Some(remote_desktop_scale_factor_percent(window.scale_factor()));
            let _ = resize_session.update(cx, |session, cx| {
                // Canvas geometry is final only after layout.
                if session.schedule_viewport_resize(scale_factor, cx) {
                    cx.notify();
                }
            });
        });
        let mut desktop = div()
            .id(("remote-desktop-surface", tab_id.0))
            .min_h(px(0.0))
            .flex_1()
            .relative()
            .on_hover(cx.listener(|session, hovered: &bool, window, cx| {
                if *hovered && window.is_window_active() {
                    session.sync_local_file_clipboard(cx);
                }
            }))
            .child(remote_desktop_surface_with_geometry(
                self.tokens,
                &session.state,
                Some(session.geometry.clone()),
            ))
            .on_mouse_move(cx.listener(|session, event: &MouseMoveEvent, _window, cx| {
                if session.handle_mouse_move(event.position) {
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .on_scroll(cx.listener(|session, event: &ScrollEvent, _window, cx| {
                if session.handle_wheel(event.position, &event.delta) {
                    cx.notify();
                }
                cx.stop_propagation();
            }));
        for button in [
            MouseButton::Left,
            MouseButton::Right,
            MouseButton::Middle,
            MouseButton::Navigate(gpui::NavigationDirection::Back),
            MouseButton::Navigate(gpui::NavigationDirection::Forward),
        ] {
            let Some(remote_button) = remote_desktop_mouse_button_from_gpui(button) else {
                continue;
            };
            let action = self.window_action.clone();
            let owner = cx.entity().downgrade();
            desktop = desktop
                .on_mouse_down(button, move |event: &MouseDownEvent, window, cx| {
                    if owner
                        .update(cx, |session, cx| {
                            session.sync_local_file_clipboard(cx);
                            if session.handle_mouse_button(
                                event.position,
                                remote_button,
                                RemoteDesktopMouseButtonState::Pressed,
                            ) {
                                cx.notify();
                            }
                        })
                        .is_ok()
                    {
                        action(DesktopWindowAction::FocusKeyboard(tab_id), window, cx);
                    }
                    cx.stop_propagation();
                })
                .on_mouse_up(
                    button,
                    cx.listener(move |session, event: &MouseUpEvent, _window, cx| {
                        if session.handle_mouse_button(
                            event.position,
                            remote_button,
                            RemoteDesktopMouseButtonState::Released,
                        ) {
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up_out(
                    button,
                    cx.listener(move |session, _, _, cx| {
                        if session.release_mouse_button_out(remote_button) {
                            cx.notify();
                        }
                    }),
                );
        }
        div()
            .size_full()
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .child(desktop)
            .child(self.render_remote_desktop_footer(session, resize_menu_open, cx))
            .when(resize_menu_open, |surface| {
                surface.child(
                    context_menu_backdrop()
                        .on_mouse_down(
                            MouseButton::Left,
                            self.request(DesktopWindowAction::DismissOverlays),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            self.request(DesktopWindowAction::DismissOverlays),
                        )
                        .child(self.render_remote_desktop_resize_menu(session, window, cx)),
                )
            })
            .children(certificate_dialog)
            .when(session.vnc_files.open, |surface| {
                surface.child(self.render_vnc_file_browser(session.vnc_files.clone(), cx))
            })
            .into_any_element()
    }

    pub(super) fn render_lucide_icon(icon: LucideIcon, size: f32, color: Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }

    pub(super) fn workspace_toolbar_action_button(
        &self,
        label: String,
        icon: Option<AnyElement>,
        options: ToolbarButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        let actionable = !(options.button.disabled || options.loading);
        toolbar_button(self.tokens, label, icon, options)
            .when(actionable, |button| {
                button.on_mouse_down(MouseButton::Left, listener)
            })
            .when(!actionable, |button| {
                button.on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            })
    }

    fn workspace_icon_action_button(
        &self,
        icon: LucideIcon,
        size: f32,
        color: Rgba,
        options: IconButtonOptions,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Div {
        icon_button(
            self.tokens,
            Self::render_lucide_icon(icon, size, color),
            options,
        )
        .on_mouse_down(MouseButton::Left, listener)
    }

    fn remote_desktop_footer_action_button(
        &self,
        group_id: impl Into<SharedString>,
        icon: LucideIcon,
        label: String,
        disabled: bool,
        danger_hover: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.tokens.ui;
        let group_id: SharedString = group_id.into();
        let idle_color = if disabled {
            theme.text_muted
        } else {
            theme.text_secondary
        };
        let hover_color = if danger_hover {
            theme.error
        } else {
            theme.text
        };
        // Match sidebar session actions: keep the persistent footer calm at rest and
        // reveal the error tint for icon + label together only on hover.
        let hover_background = danger_hover.then(|| rgba((theme.error << 8) | 0x1a).into());
        let mut icon_element = svg()
            .path(icon.path())
            .size(px(12.0))
            .text_color(rgb(idle_color));
        if !disabled {
            icon_element = icon_element.group_hover(group_id.clone(), move |icon| {
                icon.text_color(rgb(hover_color))
            });
        }
        self.workspace_toolbar_action_button(
            label,
            Some(icon_element.into_any_element()),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled,
                },
                height: Some(24.0),
                padding_x: Some(8.0),
                font_size: Some(self.tokens.metrics.ui_text_xs),
                text_color: Some(rgb(idle_color).into()),
                hover_text_color: (!disabled).then(|| rgb(hover_color).into()),
                hover_background,
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
        .id(group_id.clone())
        .group(group_id)
    }

    fn render_remote_desktop_resize_control(
        &self,
        tab_id: TabId,
        menu_open: bool,
        disabled: bool,
        cx: &mut Context<RemoteDesktopSessionEntity>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let session = cx.entity().downgrade();
        let desktops = self.desktops.clone();
        let anchor_desktops = self.desktops.clone();
        let fit_button = self.remote_desktop_footer_action_button(
            format!("remote-desktop-fit-{}", tab_id.0),
            LucideIcon::Monitor,
            self.i18n.t("remote_desktop.fit_current_window"),
            disabled,
            false,
            cx.listener(move |this, _event, window, cx| {
                this.fit_current_viewport_immediately(
                    remote_desktop_scale_factor_percent(window.scale_factor()),
                    cx,
                );
                cx.stop_propagation();
            }),
        );
        let menu_button = self.workspace_icon_action_button(
            LucideIcon::ChevronDown,
            12.0,
            rgb(if disabled {
                theme.text_muted
            } else {
                theme.text_secondary
            }),
            IconButtonOptions {
                radius: ButtonRadius::Md,
                has_background: menu_open,
                ..IconButtonOptions::compact(24.0)
            },
            cx.listener(move |_session, _event, _window, cx| {
                desktops.update(cx, |desktops, _| desktops.toggle_resize_menu(tab_id));
                cx.stop_propagation();
                cx.notify();
            }),
        );
        let trigger = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.one))
            .child(fit_button)
            .child(menu_button);

        // Keep the split-button bounds warm so the first menu open can be
        // placed above the footer without relying on pointer coordinates.
        select_anchor_probe(
            SelectAnchorId::RemoteDesktopResizeMenu(tab_id.0),
            trigger,
            move |anchor, _window, cx| {
                let open = anchor_desktops.read(cx).resize_menu_open(tab_id);
                let _ = session.update(cx, |session, cx| {
                    if session.resize_anchor.replace(anchor) != Some(anchor) && open {
                        cx.notify();
                    }
                });
            },
        )
        .into_any_element()
    }

    fn render_remote_desktop_resize_menu(
        &self,
        session: &RemoteDesktopSessionEntity,
        window: &Window,
        cx: &mut Context<RemoteDesktopSessionEntity>,
    ) -> AnyElement {
        let Some(anchor) = session.resize_anchor else {
            return div().into_any_element();
        };
        let desktops = self.desktops.clone();
        let snapshot = session.state.snapshot();
        let disabled = session.uses_monitor_layout();
        let follow_window_size = session.follow_window_size && !disabled;
        let current_size = snapshot.size;
        let viewport = window.viewport_size();
        let actionable_or_value_count = 2 + REMOTE_DESKTOP_COMMON_RESOLUTIONS.len();
        let menu_label_count = 2;
        let menu_separator_count = 2.0;
        let menu_height = context_menu_item_height_estimate(self.tokens)
            * (actionable_or_value_count + menu_label_count) as f32
            + context_menu_separator_height_estimate(self.tokens) * menu_separator_count
            + self.tokens.metrics.ui_menu_padding * 2.0;
        let placement = clamp_context_menu_position(
            f32::from(anchor.bounds.right()) - REMOTE_DESKTOP_RESIZE_MENU_WIDTH,
            f32::from(anchor.bounds.top()) - menu_height - REMOTE_DESKTOP_RESIZE_MENU_GAP,
            f32::from(viewport.width),
            f32::from(viewport.height),
            REMOTE_DESKTOP_RESIZE_MENU_WIDTH,
            menu_height,
            REMOTE_DESKTOP_RESIZE_MENU_VIEWPORT_PADDING,
        );
        let follow_item = dropdown_menu_item(
            self.tokens,
            self.i18n.t("remote_desktop.follow_window_size"),
            DropdownMenuItemKind::Checkbox(follow_window_size),
            false,
            disabled,
        );
        let mut menu = context_menu_event_boundary(
            dropdown_menu_content(self.tokens).w(px(REMOTE_DESKTOP_RESIZE_MENU_WIDTH)),
        )
        .child(menu_action(
            follow_item,
            disabled,
            false,
            oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle::default(),
            cx.listener(move |this, _event, window, cx| {
                if this.set_follow_window_size(
                    !follow_window_size,
                    remote_desktop_scale_factor_percent(window.scale_factor()),
                    cx,
                ) {
                    cx.notify();
                }
            }),
        ))
        .child(dropdown_menu_separator(self.tokens))
        .child(dropdown_menu_label(
            self.tokens,
            self.i18n.t("remote_desktop.current_remote_size"),
            false,
        ));
        let current_size_label = current_size
            .map(remote_desktop_resolution_label)
            .unwrap_or_else(|| self.i18n.t("remote_desktop.size_unavailable"));
        menu = menu
            .child(dropdown_menu_item(
                self.tokens,
                current_size_label,
                DropdownMenuItemKind::Plain,
                false,
                true,
            ))
            .child(dropdown_menu_separator(self.tokens))
            .child(dropdown_menu_label(
                self.tokens,
                self.i18n.t("remote_desktop.common_resolutions"),
                false,
            ));
        for size in REMOTE_DESKTOP_COMMON_RESOLUTIONS {
            let selected = !follow_window_size && current_size == Some(size);
            let item = dropdown_menu_item(
                self.tokens,
                remote_desktop_resolution_label(size),
                DropdownMenuItemKind::Radio(selected),
                false,
                disabled,
            );
            menu = menu.child(menu_action(
                item,
                disabled,
                false,
                oxideterm_gpui_ui::context_menu::ContextMenuActionableStyle::default(),
                {
                    let desktops = desktops.clone();
                    cx.listener(move |this, _event, window, cx| {
                        desktops.update(cx, |desktops, _| {
                            desktops.close_resize_menu();
                        });
                        this.request_preset_size(
                            size,
                            remote_desktop_scale_factor_percent(window.scale_factor()),
                            cx,
                        );
                        cx.notify();
                    })
                },
            ));
        }

        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(oxideterm_gpui_ui::motion::popover_zoom_enter(
                    self.tokens,
                    "remote-desktop-resize-menu",
                    div().child(overlay_content_boundary(menu)),
                    -4.0,
                    gpui::point(0.5, 0.0),
                )),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    fn render_remote_desktop_footer(
        &self,
        session: &RemoteDesktopSessionEntity,
        resize_menu_open: bool,
        cx: &mut Context<RemoteDesktopSessionEntity>,
    ) -> AnyElement {
        let tab_id = session.tab_id;
        let theme = self.tokens.ui;
        let snapshot = session.state.snapshot();
        let status = snapshot.status;
        let status_color = remote_desktop_status_color(self.tokens, status);
        let reconnect_disabled = !remote_desktop_reconnect_enabled(status);
        let resize_capability = snapshot
            .negotiated_capabilities
            .as_ref()
            .map(|capabilities| capabilities.resize)
            .unwrap_or_else(|| {
                if session.provider.capabilities.resize {
                    NegotiatedCapabilityStatus::Unknown
                } else {
                    NegotiatedCapabilityStatus::Unsupported
                }
            });
        let resize_capability_label = match resize_capability {
            NegotiatedCapabilityStatus::Supported => {
                self.i18n.t("remote_desktop.fit_current_window")
            }
            NegotiatedCapabilityStatus::Unsupported => self.i18n.t("remote_desktop.resize_fixed"),
            NegotiatedCapabilityStatus::Unknown => self.i18n.t("remote_desktop.resize_unknown"),
        };
        let vnc_file_download_available = session.vnc_file_download_available();
        let vnc_capability_presentation =
            (snapshot.protocol == RemoteDesktopProtocol::Vnc).then(|| {
                self.remote_desktop_vnc_capability_presentation(
                    snapshot.negotiated_capabilities.as_ref(),
                )
            });
        let label = format!(
            "{} · {}:{}",
            session.provider.name, session.profile.endpoint.host, session.profile.endpoint.port
        );

        div()
            .flex_none()
            .h(px(oxideterm_gpui_ui::WORKSPACE_STATUS_BAR_HEIGHT))
            .px(px(14.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.one))
            .bg(rgb(theme.bg_panel))
            .border_t_1()
            .border_color(rgba((theme.border << 8) | 0x80))
            .child(remote_desktop_protocol_chip(self.tokens, snapshot.protocol))
            .child(
                div()
                    .size(px(7.0))
                    .rounded_full()
                    .bg(rgb(status_color))
                    .flex_none(),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(label),
            )
            .when(resize_capability.is_supported(), |footer| {
                footer.child(self.render_remote_desktop_resize_control(
                    tab_id,
                    resize_menu_open,
                    session.uses_monitor_layout(),
                    cx,
                ))
            })
            .when(!resize_capability.is_supported(), |footer| {
                footer.child(remote_desktop_footer_status_label(
                    self.tokens,
                    resize_capability_label,
                ))
            })
            .when_some(
                vnc_capability_presentation,
                |footer, (capability_label, capability_tooltip)| {
                    let tooltip_for_move = capability_tooltip;
                    footer
                        .when(!resize_capability.is_supported(), |footer| {
                            footer.child(
                                div()
                                    .flex_none()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgba((theme.text_muted << 8) | 0x80))
                                    .child("·"),
                            )
                        })
                        .child(
                            remote_desktop_footer_status_label(self.tokens, capability_label)
                                .id(SharedString::from(format!(
                                    "remote-desktop-vnc-capabilities-{}",
                                    tab_id.0
                                )))
                                .hover(move |style| style.text_color(rgb(theme.text_secondary)))
                                .on_mouse_move({
                                    let action = self.window_action.clone();
                                    move |event: &MouseMoveEvent, window, cx| {
                                        action(
                                            DesktopWindowAction::ShowCapabilityTooltip {
                                                text: tooltip_for_move.clone(),
                                                position: event.position,
                                            },
                                            window,
                                            cx,
                                        );
                                    }
                                })
                                .on_hover({
                                    let action = self.window_action.clone();
                                    move |hovered: &bool, window, cx| {
                                        if !*hovered {
                                            action(
                                                DesktopWindowAction::ClearCapabilityTooltip,
                                                window,
                                                cx,
                                            );
                                        }
                                    }
                                }),
                        )
                },
            )
            .child(
                div()
                    .flex_none()
                    .w(px(1.0))
                    .h(px(12.0))
                    .mx(px(self.tokens.spacing.one))
                    .bg(rgba((theme.border << 8) | 0x66)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.one))
                    .when(vnc_file_download_available, |actions| {
                        actions.child(self.remote_desktop_footer_action_button(
                            format!("remote-desktop-files-{}", tab_id.0),
                            LucideIcon::FolderOpen,
                            self.i18n.t("remote_desktop.file_browser"),
                            false,
                            false,
                            cx.listener(move |this, _event, _window, cx| {
                                this.open_vnc_file_browser(cx);
                            }),
                        ))
                    })
                    .child(self.remote_desktop_footer_action_button(
                        format!("remote-desktop-reconnect-{}", tab_id.0),
                        LucideIcon::RefreshCw,
                        self.i18n.t("remote_desktop.reconnect"),
                        reconnect_disabled,
                        false,
                        self.request(DesktopWindowAction::Reconnect(tab_id)),
                    ))
                    .child(self.remote_desktop_footer_action_button(
                        format!("remote-desktop-disconnect-{}", tab_id.0),
                        LucideIcon::Power,
                        self.i18n.t("remote_desktop.disconnect"),
                        false,
                        true,
                        self.request(DesktopWindowAction::Disconnect(tab_id)),
                    )),
            )
            .into_any_element()
    }

    fn remote_desktop_vnc_capability_presentation(
        &self,
        capabilities: Option<&NegotiatedCapabilities>,
    ) -> (String, String) {
        let unknown_capabilities = NegotiatedCapabilities::default();
        let is_pending = capabilities.is_none();
        let capabilities = capabilities.unwrap_or(&unknown_capabilities);
        let supported = self.i18n.t("remote_desktop.capability_supported");
        let unsupported = self.i18n.t("remote_desktop.capability_unsupported");
        let unknown = self.i18n.t("remote_desktop.capability_unknown");
        let status = |value: NegotiatedCapabilityStatus| match value {
            NegotiatedCapabilityStatus::Supported => supported.as_str(),
            NegotiatedCapabilityStatus::Unsupported => unsupported.as_str(),
            NegotiatedCapabilityStatus::Unknown => unknown.as_str(),
        };
        let feature_statuses = [
            capabilities.resize,
            capabilities.multi_monitor,
            capabilities.extended_clipboard,
            capabilities.tight,
            capabilities.jpeg,
            capabilities.continuous_updates,
            capabilities.fence,
            capabilities.last_rect,
            capabilities.h264,
            capabilities.qemu_audio,
            capabilities.vendor_files,
            capabilities.extended_key_events,
            capabilities.extended_mouse_buttons,
            capabilities.lock_key_sync,
        ];
        let supported_count = feature_statuses
            .iter()
            .filter(|value| value.is_supported())
            .count();
        let label = if is_pending {
            self.i18n.t("remote_desktop.capabilities_pending")
        } else {
            self.i18n
                .t("remote_desktop.capabilities_summary")
                .replace("{{supported}}", &supported_count.to_string())
                .replace("{{total}}", &feature_statuses.len().to_string())
        };
        let security_method = capabilities
            .selected_security_method
            .as_deref()
            .or_else(|| capabilities.security_methods.first().map(String::as_str))
            .unwrap_or(unknown.as_str());
        let clipboard_formats = match capabilities.extended_clipboard {
            NegotiatedCapabilityStatus::Supported
                if !capabilities.extended_clipboard_formats.is_empty() =>
            {
                capabilities.extended_clipboard_formats.join(", ")
            }
            value => status(value).to_string(),
        };
        let lines = [
            format!(
                "{}: {security_method}",
                self.i18n.t("remote_desktop.capability_security_method")
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_encryption"),
                status(capabilities.encrypted)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_identity_verified"),
                status(capabilities.peer_identity_verified)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_resize"),
                status(capabilities.resize)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_multi_monitor"),
                status(capabilities.multi_monitor)
            ),
            format!(
                "{}: {clipboard_formats}",
                self.i18n.t("remote_desktop.capability_extended_clipboard")
            ),
            format!("Tight: {}", status(capabilities.tight)),
            format!("JPEG: {}", status(capabilities.jpeg)),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_continuous_updates"),
                status(capabilities.continuous_updates)
            ),
            format!("Fence: {}", status(capabilities.fence)),
            format!("LastRect: {}", status(capabilities.last_rect)),
            format!("H.264: {}", status(capabilities.h264)),
            format!("QEMU Audio: {}", status(capabilities.qemu_audio)),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_vendor_files"),
                status(capabilities.vendor_files)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_extended_keys"),
                status(capabilities.extended_key_events)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_extended_mouse"),
                status(capabilities.extended_mouse_buttons)
            ),
            format!(
                "{}: {}",
                self.i18n.t("remote_desktop.capability_lock_sync"),
                status(capabilities.lock_key_sync)
            ),
        ];

        (label, lines.join("\n"))
    }
}

fn menu_action(
    item: gpui::Div,
    disabled: bool,
    loading: bool,
    style: ContextMenuActionableStyle,
    listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> gpui::Div {
    context_menu_action(
        context_menu_actionable_row(item, disabled, loading, style),
        disabled,
        loading,
        move |event, window, cx| {
            listener(event, window, cx);
            cx.stop_propagation();
        },
    )
}

fn clamp_context_menu_position(
    x: f32,
    y: f32,
    viewport_width: f32,
    viewport_height: f32,
    width: f32,
    height: f32,
    margin: f32,
) -> Point<f32> {
    gpui::point(
        x.min(viewport_width - width - margin).max(margin),
        y.min(viewport_height - height - margin).max(margin),
    )
}

#[derive(Clone, Copy)]
pub(super) enum LucideIcon {
    Monitor,
    ChevronDown,
    FolderOpen,
    RefreshCw,
    Power,
    Folder,
    Square,
    CheckSquare,
    ArrowUp,
    Download,
}
impl LucideIcon {
    fn path(self) -> &'static str {
        match self {
            Self::Monitor => "lucide/monitor.svg",
            Self::ChevronDown => "lucide/chevron-down.svg",
            Self::FolderOpen => "lucide/folder-open.svg",
            Self::RefreshCw => "lucide/refresh-cw.svg",
            Self::Power => "lucide/power.svg",
            Self::Folder => "lucide/folder.svg",
            Self::Square => "lucide/square.svg",
            Self::CheckSquare => "lucide/check-square.svg",
            Self::ArrowUp => "lucide/arrow-up.svg",
            Self::Download => "lucide/download.svg",
        }
    }
}
