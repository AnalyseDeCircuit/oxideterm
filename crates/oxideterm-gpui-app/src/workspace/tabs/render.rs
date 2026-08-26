use super::helpers::{
    WelcomeRecentConnection, WelcomeRecentKind, WelcomeRecentTarget, effective_shortcut_label,
    welcome_layout_is_stacked, welcome_recent_connections,
};
use super::*;

use gpui::StatefulInteractiveElement;

#[derive(Clone, Copy)]
enum WelcomeToolAction {
    NewConnection,
    LocalTerminal,
    ImportConnections,
    SessionManager,
    CloudSync,
}

fn tab_kind_icon(
    workspace: &WorkspaceApp,
    kind: &TabKind,
    cx: &Context<WorkspaceApp>,
) -> LucideIcon {
    match kind {
        TabKind::LocalTerminal => LucideIcon::Square,
        TabKind::SshTerminal => LucideIcon::Terminal,
        TabKind::MoshTerminal => LucideIcon::Terminal,
        TabKind::FileManager => LucideIcon::FolderOpen,
        TabKind::Graphics | TabKind::RemoteDesktop => LucideIcon::Monitor,
        TabKind::Runtime | TabKind::ConnectionPool => LucideIcon::Gauge,
        TabKind::Topology => LucideIcon::Network,
        TabKind::NotificationCenter => LucideIcon::Bell,
        TabKind::Sftp => LucideIcon::FolderInput,
        TabKind::Ide => LucideIcon::Code2,
        TabKind::Forwards => LucideIcon::ArrowLeftRight,
        TabKind::SessionManager => LucideIcon::LayoutList,
        TabKind::PluginManager => LucideIcon::Puzzle,
        TabKind::Plugin { plugin_id, tab_id } => workspace
            .plugin_entity
            .read(cx)
            .registry()
            .contributions()
            .tab_contribution(plugin_id, tab_id)
            .map(|contribution| LucideIcon::from_plugin_name(&contribution.definition.icon))
            .unwrap_or(LucideIcon::Puzzle),
        TabKind::CloudSync => LucideIcon::Cloud,
        TabKind::Settings => LucideIcon::Settings,
    }
}

impl WorkspaceApp {
    fn render_skip_future_ssh_close_confirmations(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(
                checkbox(
                    &self.tokens,
                    self.i18n.t("common.actions.do_not_ask_again"),
                    self.skip_future_ssh_close_confirmations,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.toggle_skip_future_ssh_close_confirmations(cx);
                        cx.stop_propagation();
                    }),
                ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn render_tab_bar(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let bar = div()
            .h(px(self.tokens.metrics.tabbar_height))
            .flex()
            .flex_row()
            .items_center()
            .relative()
            .overflow_hidden()
            .border_b_1()
            .border_color(rgb(theme.border))
            .bg(self.workspace_chrome_background(theme.bg));

        // Tauri's scroll container measures the full inline-flex tab row as
        // scrollWidth. GPUI's ScrollHandle computes max_offset from direct
        // child bounds, so tab items must stay direct children of this viewport.
        // Keep overflow hidden instead of overflow_x_scroll: the custom wheel
        // adapter owns browser-style scrollLeft clamping, while track_scroll
        // still measures and applies the offset without GPUI boundary overscroll.
        let mut scroll_viewport = div()
            .id("workspace-tab-scroll-viewport")
            .h_full()
            .flex_1()
            .min_w(px(0.0))
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .pl(px(self.tokens.metrics.tabbar_leading_offset))
            .overflow_hidden()
            .track_scroll(&self.main_window_tabs.scroll_handle)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                this.handle_tabbar_scroll(event, window, cx);
            }));

        let returning_placeholder = self.detached_tab_return_placeholder(cx);
        let returning_tab = returning_placeholder.and_then(|placeholder| {
            self.tab_by_id(placeholder.tab_id, cx).map(|tab| {
                (
                    tab.id,
                    tab.kind.clone(),
                    self.tab_display_title(tab),
                    self.tab_visual_width(tab),
                )
            })
        });
        let outside_main_tabs = self.tab_host.read(cx).outside_main_tab_ids();
        let tabs_with_unread_terminal_output =
            self.tab_host.read(cx).unread_terminal_output_tab_ids();
        let active_tab_id = self.active_tab_id(cx);
        let mut live_tabs = self
            .tabs(cx)
            .iter()
            .enumerate()
            .filter(|(_, tab)| !outside_main_tabs.contains(&tab.id))
            .map(|(tab_index, tab)| {
                let tab_id = tab.id;
                let tab_width = self.tab_visual_width(tab);
                let reconnect_node_id = self.reconnect_node_id_for_tab(tab, cx);
                let reconnect_job = reconnect_node_id.as_ref().and_then(|node_id| {
                    self.workspace_runtime
                        .read(cx)
                        .reconnect_active_progress(node_id)
                });
                let icon = tab_kind_icon(self, &tab.kind, cx);
                let tab_text = self.tab_display_title(tab);
                (
                    tab_index,
                    tab_id,
                    tab_width,
                    reconnect_node_id,
                    reconnect_job,
                    icon,
                    tab_text,
                )
            })
            .collect::<Vec<_>>()
            .into_iter();
        let live_tab_count = live_tabs.len();
        let visual_tab_count = live_tab_count
            + self.main_window_tabs.exiting_tabs.len()
            + usize::from(returning_tab.is_some());
        let mut visible_tab_index = 0;
        let mut placeholder_rendered = false;
        for visual_index in 0..visual_tab_count {
            if let Some(exiting) = self
                .main_window_tabs
                .exiting_tabs
                .iter()
                .find(|exiting| exiting.visual_index == visual_index)
            {
                scroll_viewport =
                    scroll_viewport.child(self.render_exiting_tab_visual(exiting, cx));
                continue;
            }
            if !placeholder_rendered
                && returning_placeholder
                    .is_some_and(|placeholder| placeholder.visible_index == visible_tab_index)
                && let Some((tab_id, tab_kind, tab_title, tab_width)) = returning_tab.as_ref()
            {
                scroll_viewport =
                    scroll_viewport.child(self.render_detached_return_tab_placeholder(
                        *tab_id, tab_kind, tab_title, *tab_width, cx,
                    ));
                placeholder_rendered = true;
                continue;
            }
            let Some((
                tab_index,
                tab_id,
                tab_width,
                reconnect_node_id,
                reconnect_job,
                icon,
                tab_text,
            )) = live_tabs.next()
            else {
                continue;
            };
            let current_visible_index = visible_tab_index;
            visible_tab_index += 1;
            let active = Some(tab_id) == active_tab_id;
            let has_unread_terminal_output = tabs_with_unread_terminal_output.contains(&tab_id);
            let drag_state = self.main_window_tabs.drag.as_ref();
            let drag_active = drag_state.is_some_and(|drag| drag.active);
            let is_being_dragged = drag_state.is_some_and(|drag| drag.tab_id == tab_id);
            let show_drop_indicator = drag_state.is_some_and(|drag| {
                drag.active
                    && drag.mode == TabDragMode::Reorder
                    && drag.drop_target_index == current_visible_index
                    && super::navigation::tab_reorder_target_visible_index(
                        drag.from_index,
                        drag.drop_target_index,
                    ) != drag.from_index
            });
            let show_reconnect_progress = reconnect_job.is_some();
            let tab_tooltip_label = tab_text.clone();
            let tab_tooltip_id = format!("workspace-tab-title-{}", tab_id.0);
            let middle_click_tooltip_id = tab_tooltip_id.clone();
            let close_button_tooltip_id = tab_tooltip_id.clone();
            let tab_text_color = if active {
                rgb(theme.text)
            } else if has_unread_terminal_output {
                rgb(theme.success)
            } else {
                rgb(theme.text_muted)
            };
            let tab_element = div()
                .id(("workspace-tab", tab_id.0))
                .h_full()
                .flex_none()
                .w(px(tab_width))
                .min_w(px(self.tokens.metrics.tab_min_width))
                .max_w(px(self.tokens.metrics.tab_max_width))
                .px(px(self.tokens.metrics.tab_padding_x))
                .relative()
                .flex()
                .flex_row()
                .items_center()
                .gap(px(self.tokens.metrics.tab_gap))
                .border_r_1()
                .border_color(if show_reconnect_progress {
                    rgb(theme.warning)
                } else {
                    rgb(theme.border)
                })
                .bg(self.workspace_chrome_background(if active {
                    theme.bg_panel
                } else {
                    theme.bg
                }))
                .text_color(tab_text_color)
                .opacity(if is_being_dragged && drag_active {
                    0.5
                } else {
                    1.0
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.start_tab_drag_candidate(tab_id, tab_index, event, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                        this.open_tab_context_menu(tab_id, event, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |this, _event, window, cx| {
                        this.clear_workspace_tooltip(&middle_click_tooltip_id, cx);
                        this.request_close_tab_by_id(tab_id, window, cx);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_move(cx.listener({
                    let tooltip_id = tab_tooltip_id.clone();
                    move |this, event: &MouseMoveEvent, _window, cx| {
                        this.queue_workspace_tooltip(
                            tooltip_id.clone(),
                            tab_tooltip_label.clone(),
                            f32::from(event.position.x) + 12.0,
                            f32::from(event.position.y) + 16.0,
                            cx,
                        );
                    }
                }))
                .on_hover(cx.listener(move |this, hovered: &bool, _window, cx| {
                    if !*hovered {
                        this.clear_workspace_tooltip(&tab_tooltip_id, cx);
                    }
                }))
                .when(show_drop_indicator, |tab| {
                    tab.child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .bottom_0()
                            .w(px(2.0))
                            .bg(rgb(theme.accent)),
                    )
                })
                .when(active || show_reconnect_progress, |tab| {
                    tab.child(
                        div()
                            .absolute()
                            .top_0()
                            .left_0()
                            .right_0()
                            .h(px(self.tokens.metrics.tab_active_accent_height))
                            .bg(rgb(if show_reconnect_progress {
                                theme.warning
                            } else {
                                theme.accent
                            })),
                    )
                })
                .child(Self::render_lucide_icon(
                    icon,
                    self.tokens.metrics.tab_icon_size,
                    tab_text_color,
                ))
                .child(
                    div()
                        .flex_1()
                        .truncate()
                        .text_size(px(self.tokens.metrics.tab_font_size))
                        .child(tab_text),
                )
                .when_some(
                    reconnect_job.zip(reconnect_node_id),
                    |tab, (job, node_id)| {
                        tab.child(self.render_tab_reconnect_indicator(&job, node_id, cx))
                    },
                )
                .when(!show_reconnect_progress, |tab| {
                    tab.child(
                        div()
                            .size(px(self.tokens.metrics.tab_close_button_size))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .cursor_pointer()
                            .text_color(rgb(theme.text_muted))
                            .child(Self::render_lucide_icon(
                                LucideIcon::X,
                                self.tokens.metrics.tab_close_icon_size,
                                rgb(theme.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, _event, window, cx| {
                                    this.clear_workspace_tooltip(&close_button_tooltip_id, cx);
                                    this.set_active_tab(tab_id, window, cx);
                                    this.request_close_active_tab(window, cx);
                                    cx.stop_propagation();
                                }),
                            ),
                    )
                });
            scroll_viewport = scroll_viewport.child(oxideterm_gpui_ui::motion::horizontal_reveal(
                &self.tokens,
                ("workspace-tab-open", tab_id.0),
                // Keep the content at full width while the clipping viewport
                // animates as the scroll area's direct flex child.
                div()
                    .w(px(tab_width))
                    .h_full()
                    .flex_none()
                    .child(tab_element),
                tab_width,
                true,
            ));
        }

        let show_trailing_drop_indicator =
            self.main_window_tabs.drag.as_ref().is_some_and(|drag| {
                drag.active
                    && drag.mode == TabDragMode::Reorder
                    && drag.drop_target_index == live_tab_count
                    && super::navigation::tab_reorder_target_visible_index(
                        drag.from_index,
                        drag.drop_target_index,
                    ) != drag.from_index
            });
        if show_trailing_drop_indicator {
            scroll_viewport =
                scroll_viewport.child(div().w(px(2.0)).h_full().flex_none().bg(rgb(theme.accent)));
        }

        // Only the trailing filler is draggable. Do not mark the tabbar parent
        // or scroll viewport as WindowControlArea::Drag; those elements contain
        // interactive tabs and close buttons that must keep receiving normal
        // GPUI mouse events.
        scroll_viewport = scroll_viewport
            .child(self.render_window_drag_region("workspace-tabbar-drag-region", cx));

        bar.child(scroll_viewport)
            .child(self.render_tabbar_scrollbar(window, cx))
            .into_any_element()
    }

    fn render_detached_return_tab_placeholder(
        &self,
        tab_id: TabId,
        tab_kind: &TabKind,
        tab_title: &str,
        tab_width: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let accent = theme.accent;
        let placeholder = div()
            .w_full()
            .h(px((self.tokens.metrics.tabbar_height - 8.0).max(24.0)))
            .px(px(self.tokens.metrics.tab_padding_x))
            .flex()
            .items_center()
            .gap(px(self.tokens.metrics.tab_gap))
            .rounded(px(self.tokens.radii.sm))
            .border_1()
            .border_color(rgba((accent << 8) | 0x88))
            .bg(rgba((accent << 8) | 0x18))
            .text_color(rgba((theme.text << 8) | 0xcc))
            .child(Self::render_lucide_icon(
                tab_kind_icon(self, tab_kind, cx),
                self.tokens.metrics.tab_icon_size,
                rgba((accent << 8) | 0xcc),
            ))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.tab_font_size))
                    .child(tab_title.to_string()),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::PanelLeft,
                self.tokens.metrics.tab_close_icon_size,
                rgba((accent << 8) | 0xcc),
            ));

        // The placeholder participates in tab layout, so later tabs move out
        // of the future slot before the detached window is actually closed.
        div()
            .id(("detached-tab-return-placeholder", tab_id.0))
            .w(px(tab_width))
            .h_full()
            .flex_none()
            .flex()
            .items_center()
            .child(placeholder)
            .into_any_element()
    }

    fn render_tabbar_scrollbar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(geometry) = self.tabbar_scrollbar_geometry(window, cx) else {
            return div().into_any_element();
        };
        let scrollbar_dragging = self.main_window_tabs.scrollbar_drag.is_some();
        let thumb_color = if scrollbar_dragging {
            rgb(self.tokens.ui.accent)
        } else if self.main_window_tabs.scrollbar_hovered {
            rgba((self.tokens.ui.accent << 8) | TABBAR_SCROLLBAR_HOVER_ALPHA)
        } else {
            rgba((self.tokens.ui.text_muted << 8) | TABBAR_SCROLLBAR_ALPHA)
        };

        // The visible thumb stays subtle, while the transparent hit target remains easy to drag.
        div()
            .id("workspace-tabbar-thin-scrollbar")
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom_0()
            .h(px(TABBAR_SCROLLBAR_DRAG_HEIGHT))
            .cursor(if scrollbar_dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                if this.main_window_tabs.scrollbar_hovered != *hovered {
                    this.main_window_tabs.scrollbar_hovered = *hovered;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.start_tabbar_scrollbar_drag(event, window, cx);
                }),
            )
            .child(
                div()
                    .absolute()
                    .left(px(geometry.thumb_left))
                    .bottom_0()
                    .w(px(geometry.thumb_width))
                    .h(px(TABBAR_SCROLLBAR_HEIGHT))
                    .rounded(px(TABBAR_SCROLLBAR_RADIUS))
                    .bg(thumb_color),
            )
            .into_any_element()
    }

    fn tabbar_scrollbar_geometry(
        &self,
        window: &Window,
        cx: &App,
    ) -> Option<TabbarScrollbarGeometry> {
        let viewport_bounds = self.main_window_tabs.scroll_handle.bounds();
        let measured_width = f32::from(viewport_bounds.size.width);
        let viewport_width = if measured_width > 1.0 {
            measured_width
        } else {
            self.tabbar_scroll_viewport_width(window, cx)
        };
        let viewport_left = if measured_width > 1.0 {
            f32::from(viewport_bounds.origin.x)
        } else {
            self.tabbar_left_x()
        };
        calculate_tabbar_scrollbar_geometry(
            viewport_left,
            viewport_width,
            self.tabbar_max_scroll(window, cx),
            self.tabbar_effective_scroll_x(window, cx),
        )
    }

    fn start_tabbar_scrollbar_drag(
        &mut self,
        event: &MouseDownEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(geometry) = self.tabbar_scrollbar_geometry(window, cx) else {
            return;
        };
        let pointer_x = f32::from(event.position.x) - geometry.viewport_left;
        let track_left = TABBAR_SCROLLBAR_HORIZONTAL_INSET;
        let track_right = track_left + geometry.track_width;
        let thumb_right = geometry.thumb_left + geometry.thumb_width;
        let grab_offset_x = if pointer_x >= geometry.thumb_left && pointer_x <= thumb_right {
            pointer_x - geometry.thumb_left
        } else {
            geometry.thumb_width / 2.0
        };
        self.main_window_tabs.scrollbar_drag = Some(TabbarScrollbarDragState { grab_offset_x });
        let thumb_left =
            (pointer_x - grab_offset_x).clamp(track_left, track_right - geometry.thumb_width);
        self.set_tabbar_scroll_x(
            tabbar_scroll_x_for_thumb_left(thumb_left, geometry),
            window,
            cx,
        );
        cx.notify();
        cx.stop_propagation();
    }

    pub(in crate::workspace) fn update_tabbar_scrollbar_drag(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.main_window_tabs.scrollbar_drag else {
            return;
        };
        if !event.dragging() {
            self.finish_tabbar_scrollbar_drag(cx);
            return;
        }
        let Some(geometry) = self.tabbar_scrollbar_geometry(window, cx) else {
            self.finish_tabbar_scrollbar_drag(cx);
            return;
        };
        let pointer_x = f32::from(event.position.x) - geometry.viewport_left;
        let track_left = TABBAR_SCROLLBAR_HORIZONTAL_INSET;
        let max_thumb_left = track_left + geometry.track_width - geometry.thumb_width;
        let thumb_left = (pointer_x - drag.grab_offset_x).clamp(track_left, max_thumb_left);
        self.set_tabbar_scroll_x(
            tabbar_scroll_x_for_thumb_left(thumb_left, geometry),
            window,
            cx,
        );
        cx.notify();
        cx.stop_propagation();
    }

    pub(in crate::workspace) fn finish_tabbar_scrollbar_drag(&mut self, cx: &mut Context<Self>) {
        if self.main_window_tabs.scrollbar_drag.take().is_some() {
            cx.notify();
        }
    }

    fn render_exiting_tab_visual(
        &self,
        exiting: &ExitingTabVisual,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let tab = div()
            .h_full()
            .flex_none()
            .w(px(exiting.width))
            .relative()
            .px(px(self.tokens.metrics.tab_padding_x))
            .flex()
            .items_center()
            .gap(px(self.tokens.metrics.tab_gap))
            .border_r_1()
            .border_color(rgb(theme.border))
            .bg(self.workspace_chrome_background(if exiting.was_active {
                theme.bg_panel
            } else {
                theme.bg
            }))
            .text_color(rgb(if exiting.was_active {
                theme.text
            } else {
                theme.text_muted
            }))
            .when(exiting.was_active, |tab| {
                tab.child(
                    div()
                        .absolute()
                        .top_0()
                        .left_0()
                        .right_0()
                        .h(px(self.tokens.metrics.tab_active_accent_height))
                        .bg(rgb(theme.accent)),
                )
            })
            .child(Self::render_lucide_icon(
                tab_kind_icon(self, &exiting.kind, cx),
                self.tokens.metrics.tab_icon_size,
                rgb(if exiting.was_active {
                    theme.text
                } else {
                    theme.text_muted
                }),
            ))
            .child(
                div()
                    .flex_1()
                    .truncate()
                    .text_size(px(self.tokens.metrics.tab_font_size))
                    .child(exiting.title.clone()),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::X,
                self.tokens.metrics.tab_close_icon_size,
                rgb(theme.text_muted),
            ));
        oxideterm_gpui_ui::motion::horizontal_reveal(
            &self.tokens,
            ("workspace-tab-exit", exiting.tab_id.0),
            tab,
            exiting.width,
            false,
        )
    }

    pub(in crate::workspace) fn render_node_disconnect_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(snapshot) = self.overlay.read(cx).confirm_snapshot() else {
            return div().into_any_element();
        };
        let WorkspaceOverlayConfirmKind::NodeDisconnect { display_name, .. } = &snapshot.kind
        else {
            return div().into_any_element();
        };
        let title = self
            .i18n
            .t("common.confirm.disconnect_node")
            .replace("{{name}}", display_name);
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "node-disconnect-confirm-motion",
            snapshot.phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(title).into_any_element(),
                description: Some(self.render_skip_future_ssh_close_confirmations(cx)),
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            snapshot.focused_action,
            cx.listener(|this, _event, _window, cx| {
                this.cancel_node_disconnect_confirm(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, window, cx| {
                this.confirm_node_disconnect_confirm(window, cx);
                cx.stop_propagation();
            }),
        )
    }

    pub(in crate::workspace) fn render_tab_close_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(snapshot) = self.tab_host.read(cx).close_confirm_snapshot() else {
            return div().into_any_element();
        };
        let (title_key, description_key, other_tab_count) = match &snapshot.confirm {
            TabCloseConfirm::Single { .. } => (
                "tabbar.confirm_close_terminal_title",
                Some("tabbar.confirm_close_terminal_desc"),
                None,
            ),
            TabCloseConfirm::LocalChildProcess { .. }
            | TabCloseConfirm::LocalChildProcessBatch { .. } => {
                ("tabbar.child_process_warning", None, None)
            }
            TabCloseConfirm::Other { tab_ids } => (
                "tabbar.confirm_close_other_title",
                Some("tabbar.confirm_close_other_desc"),
                Some(tab_ids.len()),
            ),
        };
        let description = match (description_key, other_tab_count) {
            (Some(key), Some(count)) => self.i18n.t(key).replace("{{count}}", &count.to_string()),
            (Some(key), None) => self.i18n.t(key),
            (None, _) => String::new(),
        };
        let is_ssh_confirmation = matches!(
            &snapshot.confirm,
            TabCloseConfirm::Single { .. } | TabCloseConfirm::Other { .. }
        );
        let description = if is_ssh_confirmation {
            Some(
                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .when(!description.is_empty(), |body| body.child(description))
                    .child(self.render_skip_future_ssh_close_confirmations(cx))
                    .into_any_element(),
            )
        } else {
            (!description.is_empty()).then(|| div().child(description).into_any_element())
        };
        oxideterm_gpui_ui::confirm::confirm_dialog_with_focus_motion(
            &self.tokens,
            "tab-close-confirm-motion",
            snapshot.phase,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: div().child(self.i18n.t(title_key)).into_any_element(),
                description,
                cancel_label: div()
                    .child(self.i18n.t("common.actions.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t("common.actions.confirm"))
                    .into_any_element(),
            },
            snapshot.focused_action,
            cx.listener(|this, _event, _window, cx| {
                this.cancel_tab_close_confirm(cx);
                cx.stop_propagation();
            }),
            cx.listener(|this, _event, window, cx| {
                this.confirm_tab_close_confirm(window, cx);
                cx.stop_propagation();
            }),
        )
    }

    fn reconnect_node_id_for_tab(&self, tab: &Tab, cx: &App) -> Option<NodeId> {
        match tab.kind {
            TabKind::SshTerminal => {
                if let Some(active_pane_id) = tab.active_pane_id
                    && let Some(session_id) = tab
                        .root_pane
                        .as_ref()
                        .and_then(|root| root.session_id_for_pane(active_pane_id))
                    && let Some(node_id) = self
                        .workspace_runtime
                        .read(cx)
                        .ssh_terminal_node_id(session_id)
                {
                    return Some(node_id);
                }
                let mut session_ids = Vec::new();
                tab.root_pane
                    .as_ref()
                    .map(|root| root.collect_session_ids(&mut session_ids));
                session_ids
                    .into_iter()
                    .filter_map(|session_id| {
                        self.workspace_runtime
                            .read(cx)
                            .ssh_terminal_node_id(session_id)
                    })
                    .find(|node_id| self.has_active_reconnect_job(node_id, cx))
                    .or_else(|| {
                        tab.root_pane.as_ref().and_then(|root| {
                            let mut session_ids = Vec::new();
                            root.collect_session_ids(&mut session_ids);
                            session_ids.first().and_then(|session_id| {
                                self.workspace_runtime
                                    .read(cx)
                                    .ssh_terminal_node_id(*session_id)
                            })
                        })
                    })
            }
            TabKind::Sftp => self
                .sftp_tab_nodes
                .get(&tab.id)
                .cloned()
                .filter(|node_id| self.has_active_reconnect_job(node_id, cx)),
            TabKind::Forwards => self
                .forwarding
                .read(cx)
                .node_for_tab(tab.id)
                .filter(|node_id| self.has_active_reconnect_job(node_id, cx)),
            TabKind::Ide => self
                .ide_workspace
                .read(cx)
                .node_for_tab(tab.id)
                .cloned()
                .filter(|node_id| self.has_active_reconnect_job(node_id, cx)),
            _ => None,
        }
    }

    fn render_tab_reconnect_indicator(
        &self,
        job: &ReconnectProgress,
        node_id: NodeId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let phase_label = reconnect_phase_label(&job.status);
        let phase_text = if job.attempt > 1 {
            format!("{phase_label} {}/{}", job.attempt, job.max_attempts)
        } else {
            phase_label.to_string()
        };
        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .text_size(px(10.0))
            .text_color(rgb(theme.warning))
            .child(Self::render_lucide_icon(
                LucideIcon::RefreshCw,
                12.0,
                rgb(theme.warning),
            ))
            .child(self.render_reconnect_phase_strip(job))
            .child(div().max_w(px(72.0)).truncate().child(phase_text))
            .child(
                div()
                    .id((
                        gpui::ElementId::from("tabbar-cancel-reconnect"),
                        node_id.0.clone(),
                    ))
                    .size(px(self.tokens.metrics.tab_close_button_size))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.sm))
                    .cursor_pointer()
                    .hover(move |button| button.bg(rgb(theme.bg_hover)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::X,
                        self.tokens.metrics.tab_close_icon_size,
                        rgb(theme.warning),
                    ))
                    .on_mouse_move(cx.listener({
                        let label = self.i18n.t("sessions.tree.actions.cancel_reconnect");
                        move |this, event: &MouseMoveEvent, _window, cx| {
                            this.queue_workspace_tooltip(
                                "tabbar-cancel-reconnect",
                                label.clone(),
                                f32::from(event.position.x) + 12.0,
                                f32::from(event.position.y) + 16.0,
                                cx,
                            );
                        }
                    }))
                    .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                        if !*hovered {
                            // The reconnect tooltip is mounted at the workspace
                            // root, so the tab button owns the explicit leave
                            // clear just like a browser TooltipTrigger.
                            this.clear_workspace_tooltip("tabbar-cancel-reconnect", cx);
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.cancel_reconnect_for_node(&node_id, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_reconnect_phase_strip(&self, job: &ReconnectProgress) -> AnyElement {
        let phases = [
            ReconnectPhase::Snapshot,
            ReconnectPhase::GracePeriod,
            ReconnectPhase::SshConnect,
            ReconnectPhase::AwaitTerminal,
            ReconnectPhase::RestoreForwards,
            ReconnectPhase::ResumeTransfers,
            ReconnectPhase::RestoreIde,
            ReconnectPhase::Verify,
        ];
        div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .children(phases.into_iter().map(|phase| {
                let result = job
                    .phase_history
                    .iter()
                    .rev()
                    .find(|event| event.phase == phase)
                    .map(|event| event.result);
                let color = match result {
                    Some(PhaseResult::Ok) => self.tokens.ui.success,
                    Some(PhaseResult::Failed) => self.tokens.ui.error,
                    Some(PhaseResult::Skipped) => self.tokens.ui.text_muted,
                    Some(PhaseResult::Running) => self.tokens.ui.warning,
                    None => self.tokens.ui.border,
                };
                div()
                    .size(px(4.0))
                    .rounded_full()
                    .bg(rgb(color))
                    .into_any_element()
            }))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_empty_workspace(
        &self,
        available_width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const WELCOME_CONTENT_MAX_WIDTH: f32 = 920.0;

        let theme = self.tokens.ui;
        let stacked = welcome_layout_is_stacked(available_width);
        div()
            .size_full()
            // Match both sidebars at the top-level content layer. Fixed tab
            // chrome keeps its stronger tint independently for readability.
            .bg(self.workspace_sidebar_background(theme.bg))
            .text_color(rgb(theme.text_muted))
            .font_family(settings_ui_font_family(
                &self.settings_store.settings().appearance.ui_font_family,
            ))
            .child(
                div()
                    .id("welcome-workspace-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .w_full()
                            .min_h_full()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .px(px(24.0))
                            .py(px(24.0))
                            .child(
                                div()
                                    .w_full()
                                    // Leave enough room for all four localized shortcut hints
                                    // while retaining wrapping in genuinely narrow windows.
                                    .max_w(px(WELCOME_CONTENT_MAX_WIDTH))
                                    .flex()
                                    .flex_col()
                                    .items_center()
                                    .gap(px(16.0))
                                    .child(self.render_welcome_header())
                                    .child(self.render_welcome_workbench(stacked, cx))
                                    .child(self.render_welcome_shortcuts()),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn welcome_main_content_width(
        &self,
        window: &Window,
        cx: &App,
    ) -> f32 {
        let settings = self.settings_store.settings();
        let mut available_width = f32::from(window.viewport_size().width);
        if !settings.sidebar_ui.zen_mode {
            available_width -= self.tokens.metrics.activity_bar_width;
            if !self.sidebar_collapsed {
                available_width -= self.sidebar_panel_width();
            }
            if self.context_sidebar_visible() {
                available_width -= self.ai_entity.read(cx).chat_ui().sidebar_width;
            }
        }
        available_width.max(self.tokens.metrics.min_main_width)
    }

    fn render_welcome_header(&self) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            // Separate the brand statement from the operational surfaces below.
            .pb(px(12.0))
            .child(self.render_welcome_brand())
            .into_any_element()
    }

    fn render_welcome_brand(&self) -> AnyElement {
        const BRAND_ICON_SIZE: f32 = 84.0;
        const BRAND_GLOW_BG_ALPHA: u32 = 0x02;
        const BRAND_GLOW_INNER_ALPHA: u32 = 0x12;
        const BRAND_GLOW_OUTER_ALPHA: u32 = 0x08;
        const BRAND_CARET_GLOW_ALPHA: u32 = 0x4d;

        let theme = self.tokens.ui;
        let icon_path = crate::app_icon::app_icon_variant_resource_path(
            self.settings_store.settings().appearance.app_icon,
        );
        let brand_glow = vec![
            gpui::BoxShadow {
                inset: false,
                color: rgba((theme.accent << 8) | BRAND_GLOW_INNER_ALPHA).into_color(),
                offset: gpui::point(px(0.0), px(0.0)),
                blur_radius: px(40.0),
                spread_radius: px(0.0),
            },
            gpui::BoxShadow {
                inset: false,
                color: rgba((theme.accent << 8) | BRAND_GLOW_OUTER_ALPHA).into_color(),
                offset: gpui::point(px(0.0), px(0.0)),
                blur_radius: px(80.0),
                spread_radius: px(0.0),
            },
        ];
        let caret_glow = vec![gpui::BoxShadow {
            inset: false,
            color: rgba((theme.accent << 8) | BRAND_CARET_GLOW_ALPHA).into_color(),
            offset: gpui::point(px(0.0), px(0.0)),
            blur_radius: px(14.0),
            spread_radius: px(0.0),
        }];

        div()
            .flex()
            .items_center()
            .gap(px(28.0))
            // Reuse the active runtime icon variant selected in Appearance settings.
            .child(
                gpui::img(icon_path)
                    .size(px(BRAND_ICON_SIZE))
                    .object_fit(ObjectFit::Contain),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(px(8.0))
                    .child(
                        div()
                            .relative()
                            .flex()
                            .items_center()
                            .py(px(2.0))
                            // GPUI does not expose CSS text-shadow; a static shadowed
                            // backing glow keeps the Tauri brand halo without drawing
                            // a visible capsule behind the wordmark.
                            .child(
                                div()
                                    .absolute()
                                    .top(px(10.0))
                                    .left(px(8.0))
                                    .right(px(8.0))
                                    .bottom(px(10.0))
                                    .rounded(px(self.tokens.radii.lg))
                                    .bg(rgba((theme.accent << 8) | BRAND_GLOW_BG_ALPHA))
                                    .shadow(brand_glow),
                            )
                            .child(
                                div()
                                    .relative()
                                    .flex()
                                    .items_center()
                                    // The brand must remain the strongest type in the start-page hierarchy.
                                    .text_size(px(38.0))
                                    .line_height(px(42.0))
                                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                                    .text_color(rgb(theme.text))
                                    .child(self.i18n.t("layout.empty.title"))
                                    .child(
                                        div()
                                            .w(px(2.0))
                                            .h(px(27.0))
                                            .ml(px(5.0))
                                            .rounded(px(self.tokens.radii.active_indicator))
                                            .bg(rgb(theme.accent))
                                            .shadow(caret_glow),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_base))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t("layout.empty.subtitle")),
                    ),
            )
            .into_any_element()
    }

    fn render_welcome_workbench(&self, stacked: bool, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_row()
            .items_stretch()
            .justify_center()
            .gap(px(16.0))
            .when(stacked, |workbench| workbench.flex_col())
            .when(!stacked, |workbench| workbench.flex_wrap())
            .child(self.render_welcome_recent_connections(stacked, cx))
            .child(self.render_welcome_guidance(stacked, cx))
            .into_any_element()
    }

    fn render_welcome_recent_connections(
        &self,
        stacked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const RECENT_CONNECTION_LIMIT: usize = 4;

        let theme = self.tokens.ui;
        let recent = welcome_recent_connections(&self.connection_store, RECENT_CONNECTION_LIMIT);
        let saved_count = self.connection_store.connections().len()
            + self.connection_store.serial_profiles().len()
            + self.connection_store.telnet_profiles().len()
            + self.connection_store.mosh_profiles().len()
            + self.connection_store.remote_desktop_profiles().len();
        let count_label = self
            .i18n
            .t("layout.empty.saved_count")
            .replace("{{count}}", &saved_count.to_string());
        let has_background = self.window_background_preferences().is_some();
        let mut surface = oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::Inspector)
                .padding(oxideterm_gpui_ui::SurfacePadding::Normal)
                .has_background_image(has_background),
        )
        // The start page uses borders and fill for grouping; extra elevation
        // makes the three peer surfaces feel heavier than the brand above.
        .shadow_none()
        .min_w(px(360.0))
        .flex_1()
        .flex_basis(px(540.0))
        .when(stacked, |surface| {
            surface
                .w_full()
                .max_w_full()
                .min_w(px(0.0))
                .flex_basis(gpui::auto())
        })
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .w_full()
                .flex()
                .items_start()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_base))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(theme.text))
                                .child(self.i18n.t("layout.empty.recent_connections")),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(theme.text_muted))
                                .child(self.i18n.t("layout.empty.recent_connections_hint")),
                        ),
                )
                .child(
                    div()
                        .flex_none()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded_full()
                        .bg(rgb(theme.bg_sunken))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(theme.text_muted))
                        .child(count_label),
                ),
        );

        if recent.is_empty() {
            surface = surface.child(self.render_welcome_recent_empty(cx));
        } else {
            surface = surface.child(
                div().w_full().flex().flex_col().gap(px(3.0)).children(
                    recent
                        .into_iter()
                        .map(|connection| self.render_welcome_recent_row(connection, cx)),
                ),
            );
        }

        surface.into_any_element()
    }

    fn render_welcome_recent_empty(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_h(px(156.0))
            .flex()
            .flex_col()
            .items_start()
            .justify_center()
            .gap(px(10.0))
            .border_t_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("layout.empty.recent_connections_empty")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_size(px(self.tokens.metrics.ui_text_sm))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(rgb(theme.accent))
                    .cursor_pointer()
                    .hover(move |row| row.text_color(rgb(theme.accent_hover)))
                    .child(Self::render_lucide_icon(
                        LucideIcon::LayoutList,
                        15.0,
                        rgb(theme.accent),
                    ))
                    .child(self.i18n.t("layout.empty.open_session_manager"))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, window, cx| {
                            this.open_session_manager_tab(window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    fn render_welcome_recent_row(
        &self,
        connection: WelcomeRecentConnection,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let target = connection.target.clone();
        oxideterm_gpui_ui::entity_list_row(
            &self.tokens,
            oxideterm_gpui_ui::EntityListRowOptions::new().compact(),
            Some(
                div()
                    .size(px(30.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.md))
                    .bg(rgb(theme.bg_sunken))
                    .child(Self::render_lucide_icon(
                        self.welcome_recent_icon(connection.kind),
                        15.0,
                        rgb(theme.accent),
                    ))
                    .into_any_element(),
            ),
            div()
                .min_w_0()
                .truncate()
                .text_size(px(self.tokens.metrics.ui_text_sm))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(rgb(theme.text))
                .child(connection.name)
                .into_any_element(),
            Some(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(theme.text_muted))
                    .child(connection.subtitle)
                    .into_any_element(),
            ),
            vec![
                div()
                    .text_size(px(11.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t(self.welcome_recent_kind_label(connection.kind)))
                    .into_any_element(),
            ],
            vec![Self::render_lucide_icon(
                LucideIcon::ChevronRight,
                14.0,
                rgb(theme.text_muted),
            )],
        )
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, window, cx| {
                this.open_welcome_recent_target(target.clone(), window, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_welcome_guidance(&self, stacked: bool, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.window_background_preferences().is_some();
        oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::Inspector)
                .padding(oxideterm_gpui_ui::SurfacePadding::Normal)
                .has_background_image(has_background),
        )
        .shadow_none()
        .min_w(px(260.0))
        .max_w(px(344.0))
        .flex_1()
        .flex_basis(px(300.0))
        .when(stacked, |surface| {
            surface
                .w_full()
                .max_w_full()
                .min_w(px(0.0))
                .flex_basis(gpui::auto())
        })
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_base))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(theme.text))
                .child(self.i18n.t("layout.empty.get_started")),
        )
        // Every destination is a flat peer option inside one semantic surface.
        .child(self.render_welcome_tool_row(
            LucideIcon::Plus,
            "layout.empty.new_connection",
            "layout.empty.new_connection_hint",
            WelcomeToolAction::NewConnection,
            cx,
        ))
        .child(self.render_welcome_tool_row(
            LucideIcon::Terminal,
            "layout.empty.new_local_terminal",
            "layout.empty.new_local_terminal_hint",
            WelcomeToolAction::LocalTerminal,
            cx,
        ))
        .child(self.render_welcome_tool_row(
            LucideIcon::Download,
            "layout.empty.import_connections",
            "layout.empty.import_connections_hint",
            WelcomeToolAction::ImportConnections,
            cx,
        ))
        .child(self.render_welcome_tool_row(
            LucideIcon::LayoutList,
            "layout.empty.open_session_manager",
            "layout.empty.open_session_manager_hint",
            WelcomeToolAction::SessionManager,
            cx,
        ))
        .child(self.render_welcome_tool_row(
            LucideIcon::Cloud,
            "plugin.cloud_sync.panel_title",
            "layout.empty.cloud_sync_hint",
            WelcomeToolAction::CloudSync,
            cx,
        ))
        .into_any_element()
    }

    fn render_welcome_tool_row(
        &self,
        icon: LucideIcon,
        title_key: &str,
        description_key: &str,
        action: WelcomeToolAction,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(10.0))
            .px(px(6.0))
            .py(px(9.0))
            .rounded(px(self.tokens.radii.md))
            .cursor_pointer()
            .hover(move |row| row.bg(rgb(theme.bg_hover)))
            .child(Self::render_lucide_icon(icon, 17.0, rgb(theme.accent)))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(self.i18n.t(title_key)),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t(description_key)),
                    ),
            )
            .child(Self::render_lucide_icon(
                LucideIcon::ChevronRight,
                14.0,
                rgb(theme.text_muted),
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, window, cx| {
                    match action {
                        WelcomeToolAction::NewConnection => {
                            this.open_new_connection_form(window, cx)
                        }
                        WelcomeToolAction::LocalTerminal => {
                            let _ = this.create_local_terminal_tab(window, cx);
                        }
                        WelcomeToolAction::ImportConnections => {
                            this.open_connection_importers_settings(window, cx)
                        }
                        WelcomeToolAction::SessionManager => {
                            this.open_session_manager_tab(window, cx)
                        }
                        WelcomeToolAction::CloudSync => this.open_cloud_sync_tab(window, cx),
                    }
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn welcome_recent_icon(&self, kind: WelcomeRecentKind) -> LucideIcon {
        match kind {
            WelcomeRecentKind::Ssh => LucideIcon::Server,
            WelcomeRecentKind::Serial => LucideIcon::Radio,
            WelcomeRecentKind::Telnet => LucideIcon::Terminal,
            WelcomeRecentKind::Mosh => LucideIcon::Wifi,
            WelcomeRecentKind::Rdp | WelcomeRecentKind::Vnc => LucideIcon::Monitor,
        }
    }

    fn welcome_recent_kind_label(&self, kind: WelcomeRecentKind) -> &'static str {
        match kind {
            WelcomeRecentKind::Ssh => "terminal.typeSsh",
            WelcomeRecentKind::Serial => "modals.new_connection.transport_serial",
            WelcomeRecentKind::Telnet => "modals.new_connection.transport_telnet",
            WelcomeRecentKind::Mosh => "terminal.typeMosh",
            WelcomeRecentKind::Rdp => "modals.new_connection.transport_rdp",
            WelcomeRecentKind::Vnc => "modals.new_connection.transport_vnc",
        }
    }

    fn open_welcome_recent_target(
        &mut self,
        target: WelcomeRecentTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            WelcomeRecentTarget::Ssh(id) => self.open_saved_connection(&id, window, cx),
            WelcomeRecentTarget::Serial(id) => self.open_saved_serial_profile(&id, window, cx),
            WelcomeRecentTarget::Telnet(id) => self.open_saved_telnet_profile(&id, window, cx),
            WelcomeRecentTarget::Mosh(id) => self.open_saved_mosh_profile(&id, window, cx),
            WelcomeRecentTarget::RemoteDesktop(id) => {
                self.open_saved_remote_desktop_profile(&id, window, cx)
            }
        }
    }

    fn render_welcome_shortcuts(&self) -> AnyElement {
        const WELCOME_SHORTCUTS: [(&str, &str); 4] = [
            ("app.commandPalette", "command_palette.title"),
            ("app.newConnection", "layout.empty.new_connection"),
            ("app.newTerminal", "layout.empty.new_local_terminal"),
            ("app.showShortcuts", "layout.empty.keyboard_shortcuts"),
        ];

        let overrides = &self.settings_store.settings().keybindings.overrides;
        let has_background = self.window_background_preferences().is_some();
        oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::Inspector)
                .padding(oxideterm_gpui_ui::SurfacePadding::Compact)
                .has_background_image(has_background),
        )
        .shadow_none()
        .w_full()
        .flex()
        .flex_row()
        .flex_wrap()
        .items_center()
        .justify_center()
        .gap_x(px(20.0))
        .gap_y(px(8.0))
        // Invalid registry entries are omitted instead of showing a shortcut that cannot fire.
        .children(
            WELCOME_SHORTCUTS
                .into_iter()
                .filter_map(|(action_id, label_key)| {
                    effective_shortcut_label(action_id, overrides)
                        .map(|key| self.render_welcome_shortcut(key, label_key))
                }),
        )
        .into_any_element()
    }

    fn render_welcome_shortcut(&self, key: String, label_key: &str) -> AnyElement {
        let theme = self.tokens.ui;
        // Window imagery needs the stronger semantic text color to remain legible.
        let label_color = if self.window_background_preferences().is_some() {
            theme.text
        } else {
            theme.text_muted
        };
        div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(self.tokens.metrics.ui_text_xs))
            .text_color(rgb(label_color))
            .child(
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(theme.border))
                    .bg(rgb(theme.bg_panel))
                    .font_family(settings_mono_font_family(self.settings_store.settings()))
                    .text_size(px(11.0))
                    .line_height(px(14.0))
                    .text_color(rgb(theme.text))
                    .child(key),
            )
            .child(self.i18n.t(label_key))
            .into_any_element()
    }
}
