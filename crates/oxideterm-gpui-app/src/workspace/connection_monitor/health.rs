use oxideterm_gpui_ui::text_input::{TextInputView, text_input, text_input_anchor_probe};

const HOST_TOOLS_CONNECTION_ROW_HEIGHT: f32 = 32.0;
const SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT: f32 = 36.0;
const SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y: f32 = 8.0;
const SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS: usize = 4;
const SYSTEM_HEALTH_SELECTOR_GAP: f32 = 8.0;
const HOST_TOOLS_TAB_STRIP_HEIGHT: f32 = 48.0;
const HOST_TOOLS_TAB_SCROLLBAR_HEIGHT: f32 = 3.0;
const HOST_TOOLS_TAB_SCROLLBAR_BOTTOM_INSET: f32 = 5.0;
const HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET: f32 = 12.0;
const HOST_TOOLS_TAB_SCROLLBAR_MIN_THUMB_WIDTH: f32 = 32.0;
const HOST_TOOLS_TAB_SCROLLBAR_RADIUS: f32 = 2.0;
const HOST_TOOLS_TAB_SCROLLBAR_ALPHA: u32 = 0x66;

impl WorkspaceApp {
    pub(super) fn render_host_tools_context_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .id("host-tools-context-panel")
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_hidden()
            .bg(rgb(theme.bg))
            .text_color(rgb(theme.text))
            .child(self.render_host_tools_context_tabs(cx))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    // Only the secondary tab strip may own horizontal scroll.
                    // Keep tool bodies clipped to the companion-sidebar width.
                    .overflow_hidden()
                    .child(match self.active_context_sidebar_tool {
                        ContextSidebarTool::Monitor => self.render_host_tools_monitor_panel(cx),
                        ContextSidebarTool::Processes => self.render_host_processes_panel(cx),
                        ContextSidebarTool::Services => self.render_host_services_panel(cx),
                        ContextSidebarTool::Logs => self.render_host_tool_placeholder(
                            "sidebar.panels.logs",
                            LucideIcon::FileText,
                            cx,
                        ),
                        ContextSidebarTool::Tmux => self.render_host_tool_placeholder(
                            "sidebar.panels.tmux_management",
                            LucideIcon::Terminal,
                            cx,
                        ),
                        ContextSidebarTool::Docker => self.render_host_docker_panel(cx),
                    }),
            )
            .into_any_element()
    }

    fn render_host_tools_monitor_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("system-health-context-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .occlude()
            .child(
                div()
                    .size_full()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .px_3()
                    .py_3()
                    // Host Tools owns the secondary navigation; monitoring
                    // keeps the existing health panel behavior inside it.
                    .child(self.render_system_health_panel(true, cx)),
            )
            .into_any_element()
    }

    fn render_host_tools_context_tabs(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut tabs = div()
            .id("host-tools-tab-scroll-viewport")
            .flex_none()
            .w_full()
            .h(px(HOST_TOOLS_TAB_STRIP_HEIGHT))
            .min_w_0()
            .relative()
            .flex()
            .flex_row()
            .items_center()
            .gap_1()
            .px_3()
            .pt_2()
            .pb_3()
            // Match the main tabbar scroll model: the strip clips its own
            // children and maps wheel movement to horizontal offset, while the
            // thin visible thumb keeps hidden tab overflow discoverable.
            .occlude()
            .overflow_x_scroll()
            .track_scroll(&self.host_tools_tab_scroll_handle)
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                this.handle_host_tools_tab_scroll(event, window, cx);
            }))
            .border_b_1()
            .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA));

        tabs = tabs
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Monitor,
                LucideIcon::Activity,
                "sidebar.panels.host_monitor",
                true,
                cx,
            ))
            // These entries reserve the host-tools IA before their backends land.
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Processes,
                LucideIcon::ListChecks,
                "sidebar.panels.processes",
                true,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Services,
                LucideIcon::Wrench,
                "sidebar.panels.services",
                true,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Logs,
                LucideIcon::FileText,
                "sidebar.panels.logs",
                false,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Tmux,
                LucideIcon::Terminal,
                "sidebar.panels.tmux",
                false,
                cx,
            ))
            .child(self.render_host_tools_context_tab(
                ContextSidebarTool::Docker,
                LucideIcon::Layers,
                "sidebar.panels.docker",
                true,
                cx,
            ))
            .child(self.render_host_tools_tab_scrollbar());

        tabs.into_any_element()
    }

    fn render_host_tools_tab_scrollbar(&self) -> AnyElement {
        let viewport_width = f32::from(self.host_tools_tab_scroll_handle.bounds().size.width);
        let max_scroll = f32::from(self.host_tools_tab_scroll_handle.max_offset().width);
        let track_width = (viewport_width - HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET * 2.0)
            .max(0.0);
        if viewport_width <= 1.0 || max_scroll <= 1.0 || track_width <= 1.0 {
            return div().into_any_element();
        }

        let content_width = viewport_width + max_scroll;
        let min_thumb_width = HOST_TOOLS_TAB_SCROLLBAR_MIN_THUMB_WIDTH.min(track_width);
        let thumb_width = (viewport_width / content_width * track_width)
            .max(min_thumb_width)
            .min(track_width);
        let current_scroll_x =
            f32::from(-self.host_tools_tab_scroll_handle.offset().x).clamp(0.0, max_scroll);
        let thumb_left = HOST_TOOLS_TAB_SCROLLBAR_HORIZONTAL_INSET
            + (current_scroll_x / max_scroll * (track_width - thumb_width).max(0.0));
        // The thumb is painted inside the scrollable strip, so compensate for
        // the strip's content offset to keep the visible thumb aligned with the viewport track.
        let content_thumb_left = thumb_left + current_scroll_x;

        // Tauri's tab-strip scrollbar uses a 3px thin thumb; the GPUI component
        // `Always` mode paints a 16px hit area, so this surface draws only the
        // lightweight always-visible affordance while wheel scrolling remains native.
        div()
            .id("host-tools-tab-thin-scrollbar")
            .absolute()
            .left(px(0.0))
            .right(px(0.0))
            .bottom(px(HOST_TOOLS_TAB_SCROLLBAR_BOTTOM_INSET))
            .h(px(HOST_TOOLS_TAB_SCROLLBAR_HEIGHT))
            .child(
                div()
                    .absolute()
                    .left(px(content_thumb_left))
                    .w(px(thumb_width))
                    .h(px(HOST_TOOLS_TAB_SCROLLBAR_HEIGHT))
                    .rounded(px(HOST_TOOLS_TAB_SCROLLBAR_RADIUS))
                    .bg(rgba(
                        (self.tokens.ui.text_muted << 8) | HOST_TOOLS_TAB_SCROLLBAR_ALPHA,
                    )),
            )
            .into_any_element()
    }

    fn handle_host_tools_tab_scroll(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let max_scroll = f32::from(self.host_tools_tab_scroll_handle.max_offset().width);
        if max_scroll <= 1.0 {
            if self.host_tools_tab_scroll_handle.offset().x != px(0.0) {
                self.host_tools_tab_scroll_handle
                    .set_offset(Point::new(px(0.0), px(0.0)));
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        let delta = event.delta.pixel_delta(px(HOST_TOOLS_TAB_STRIP_HEIGHT));
        let delta_x = f32::from(delta.x);
        let delta_y = f32::from(delta.y);
        let scroll_delta = if delta_y != 0.0 { delta_y } else { delta_x };
        if scroll_delta == 0.0 {
            return;
        }

        let current_scroll_x =
            f32::from(-self.host_tools_tab_scroll_handle.offset().x).clamp(0.0, max_scroll);
        let next_scroll_x = (current_scroll_x - scroll_delta).clamp(0.0, max_scroll);
        if (next_scroll_x - current_scroll_x).abs() < 0.01 {
            cx.stop_propagation();
            return;
        }

        self.host_tools_tab_scroll_handle
            .set_offset(Point::new(px(-next_scroll_x), px(0.0)));
        cx.notify();
        cx.stop_propagation();
    }

    fn render_host_tools_context_tab(
        &self,
        tool: ContextSidebarTool,
        icon: LucideIcon,
        label_key: &'static str,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_context_sidebar_tool == tool;
        div()
            .h(px(28.0))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_1()
            .rounded(px(self.tokens.radii.md))
            .cursor(if enabled {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .opacity(if enabled { 1.0 } else { 0.45 })
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |tab| {
                if enabled {
                    tab.bg(rgb(theme.bg_hover))
                } else {
                    tab
                }
            })
            .child(Self::render_lucide_icon(
                icon,
                13.0,
                if active {
                    rgb(theme.accent)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(
                div()
                    .text_size(px(12.0))
                    .whitespace_nowrap()
                    .truncate()
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "host-tools-tab",
                        label_key,
                        self.i18n.t(label_key),
                        if active { theme.text } else { theme.text_muted },
                        cx,
                    )),
            )
            .when(enabled, |tab| {
                tab.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if this.active_context_sidebar_tool != tool {
                            this.active_context_sidebar_tool = tool;
                            if tool != ContextSidebarTool::Processes {
                                this.connection_monitor.host_process_search_focused = false;
                                this.clear_ime_selection();
                                this.ime_marked_text = None;
                            }
                            if tool != ContextSidebarTool::Docker {
                                this.connection_monitor.host_docker_search_focused = false;
                                this.clear_ime_selection();
                                this.ime_marked_text = None;
                            }
                            if tool != ContextSidebarTool::Services {
                                this.connection_monitor.host_service_search_focused = false;
                                this.clear_ime_selection();
                                this.ime_marked_text = None;
                            }
                            // Switching Host Tools pages should eagerly attach
                            // the selected connection profiler. Waiting for the
                            // heartbeat made data appear only after another
                            // layout event, such as entering fullscreen.
                            this.refresh_connection_monitor_pool_stats();
                            this.sync_connection_monitor_selection(cx);
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
            })
            .into_any_element()
    }

    fn render_host_tool_placeholder(
        &self,
        label_key: &'static str,
        icon: LucideIcon,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        monitor_center_state(
            self,
            icon,
            self.tokens.ui.text_muted,
            self.i18n.t(label_key),
            cx,
        )
    }

    fn render_host_processes_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::WifiOff,
                self.tokens.ui.text_muted,
                self.i18n.t("profiler.panel.no_connection"),
                cx,
            );
        }

        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let current = self
            .connection_monitor
            .profiler_registry
            .current(&active_connection.connection_id);
        let metrics = current.as_ref().and_then(|(metrics, _)| metrics.as_ref());
        let rows = metrics
            .map(|metrics| self.visible_host_process_rows(&metrics.top_processes))
            .unwrap_or_default();
        self.sync_host_process_list_state(&rows, selected_id);

        div()
            .id("host-processes-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(
                        self.render_connection_switcher_row(
                            &connections,
                            selected_id,
                            current.is_some(),
                            cx,
                        ),
                    )
                    .child(self.render_host_process_search(cx))
                    .child(self.render_host_process_filter_row(cx))
                    .child(self.render_host_process_sort_row(rows.len(), cx)),
            )
            .child(self.render_host_process_list(rows, current.is_some(), selected_id, cx))
            .into_any_element()
    }

    fn render_host_docker_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::WifiOff,
                self.tokens.ui.text_muted,
                self.i18n.t("profiler.panel.no_connection"),
                cx,
            );
        }

        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let current = self
            .connection_monitor
            .profiler_registry
            .current(&active_connection.connection_id);
        let metrics = current.as_ref().and_then(|(metrics, _)| metrics.as_ref());
        let rows = metrics
            .map(|metrics| {
                visible_docker_rows(
                    &metrics.docker.containers,
                    &self.connection_monitor.host_docker_search_query,
                )
            })
            .unwrap_or_default();
        let docker_status = metrics
            .map(|metrics| metrics.docker.status.clone())
            .unwrap_or_default();
        self.sync_host_docker_list_state(&rows, selected_id);

        div()
            .id("host-docker-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher_row(
                        &connections,
                        selected_id,
                        current.is_some(),
                        cx,
                    ))
                    .child(self.render_host_docker_search(cx))
                    .child(self.render_host_docker_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        cx,
                    )),
            )
            .child(self.render_host_docker_list(rows, current.is_some(), docker_status, selected_id, cx))
            .into_any_element()
    }

    fn render_host_services_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::WifiOff,
                self.tokens.ui.text_muted,
                self.i18n.t("profiler.panel.no_connection"),
                cx,
            );
        }

        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let current = self
            .connection_monitor
            .profiler_registry
            .current(&active_connection.connection_id);
        let metrics = current.as_ref().and_then(|(metrics, _)| metrics.as_ref());
        let rows = metrics
            .map(|metrics| {
                visible_service_rows(
                    &metrics.services.services,
                    &self.connection_monitor.host_service_search_query,
                )
            })
            .unwrap_or_default();
        let service_status = metrics
            .map(|metrics| metrics.services.status.clone())
            .unwrap_or_default();
        self.sync_host_service_list_state(&rows, selected_id);

        div()
            .id("host-services-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .flex_none()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pt_3()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher_row(
                        &connections,
                        selected_id,
                        current.is_some(),
                        cx,
                    ))
                    .child(self.render_host_service_search(cx))
                    .child(self.render_host_service_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        service_status.clone(),
                        cx,
                    )),
            )
            .child(self.render_host_service_list(rows, current.is_some(), service_status, selected_id, cx))
            .into_any_element()
    }

    fn render_host_docker_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostDockerSearch;
        let focused = self.connection_monitor.host_docker_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_docker_search_query,
                    placeholder: self.i18n.t("sidebar.host_docker.search_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.connection_monitor.host_docker_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
                    this.ime_marked_text = None;
                    this.new_connection_caret_visible = true;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_host_docker_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(div().flex_none().child(format!(
                "{} {}",
                visible_count,
                self.i18n.t("sidebar.host_docker.count_suffix")
            )))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::RefreshCw,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 24.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                },
                self.i18n.t("sidebar.host_docker.actions.refresh"),
                "host-docker-refresh",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.refresh_host_docker_snapshot(selected_id.clone(), cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    fn render_host_service_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostServiceSearch;
        let focused = self.connection_monitor.host_service_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_service_search_query,
                    placeholder: self.i18n.t("sidebar.host_services.search_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.connection_monitor.host_service_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.ime_marked_text = None;
                    this.new_connection_caret_visible = true;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_host_service_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceServiceStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let capability_label = match status {
            ResourceServiceStatus::Available {
                capability: ServiceCommandCapability::Full,
                ..
            } => self.i18n.t("sidebar.host_services.capability.full"),
            ResourceServiceStatus::Available {
                capability: ServiceCommandCapability::Partial,
                ..
            } => self.i18n.t("sidebar.host_services.capability.partial"),
            _ => self.i18n.t("sidebar.host_services.capability.unknown"),
        };
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .flex_none()
                    .child(format!(
                        "{} {} · {}",
                        visible_count,
                        self.i18n.t("sidebar.host_services.count_suffix"),
                        capability_label
                    )),
            )
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::RefreshCw,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 24.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                },
                self.i18n.t("sidebar.host_services.actions.refresh"),
                "host-service-refresh",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.refresh_host_service_snapshot(selected_id.clone(), cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    fn render_host_service_list(
        &self,
        rows: Vec<ResourceService>,
        has_metrics: bool,
        status: ResourceServiceStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_metrics {
            return monitor_center_state(
                self,
                LucideIcon::Wrench,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_services.sampling"),
                cx,
            );
        }
        match status {
            ResourceServiceStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Wrench,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_services.unavailable"),
                    cx,
                );
            }
            ResourceServiceStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_services.error", &[("error", message)]),
                    cx,
                );
            }
            ResourceServiceStatus::Unknown | ResourceServiceStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Wrench,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_services.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.connection_monitor.host_service_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_service_table_header())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(state, spec, move |index, _window, cx| {
                        let rows = rows.clone();
                        let selected_id = selected_id.clone();
                        workspace.update(cx, |this, cx| {
                            this.render_host_service_row(
                                selected_id.as_str(),
                                rows.get(index).cloned(),
                                cx,
                            )
                        })
                    })),
            )
            .into_any_element()
    }

    fn render_host_service_table_header(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_SERVICE_TABLE_HEADER_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg))
            .text_size(px(HOST_PROCESS_TABLE_HEADER_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(self.i18n.t("sidebar.host_services.columns.service")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SERVICE_STATE_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_services.columns.state")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SERVICE_ENABLED_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_services.columns.enabled")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_SERVICE_PID_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_services.columns.pid")),
            )
            .into_any_element()
    }

    fn render_host_service_row(
        &self,
        connection_id: &str,
        service: Option<ResourceService>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(service) = service else {
            return div().into_any_element();
        };
        let expanded = self
            .connection_monitor
            .host_service_expanded_id
            .as_deref()
            == Some(service.id.as_str());
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let state_label = self.i18n.t(service_state_label_key(&service.active_state));
        let enabled_label = self.i18n.t(service_enabled_label_key(&service.enabled_state));
        let main_pid = service.main_pid.clone().unwrap_or_else(|| "—".to_string());
        let state_color = service_state_color(&service.active_state, theme.text_muted);

        div()
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(HOST_SERVICE_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Keep the service identity as the first-level flex item.
                    // Nested name columns caused Docker names to collapse to `...`.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(service.id.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(state_color))
                            .font_family(mono_font.clone())
                            .child(state_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_ENABLED_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(enabled_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_SERVICE_PID_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(main_pid),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font)
                            .child(format!("{} · {}", service.sub_state, service.description)),
                    )
                    .child(self.render_host_service_inline_actions(connection_id, &service, cx)),
            )
            .when(expanded, |row| row.child(self.render_host_service_detail(&service)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = service.id.clone();
                    move |this, _event, _window, cx| {
                        if this
                            .connection_monitor
                            .host_service_expanded_id
                            .as_deref()
                            == Some(id.as_str())
                        {
                            this.connection_monitor.host_service_expanded_id = None;
                        } else {
                            this.connection_monitor.host_service_expanded_id = Some(id.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    fn render_host_service_inline_actions(
        &self,
        connection_id: &str,
        service: &ResourceService,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self
            .connection_monitor
            .host_service_action_running
            .as_ref()
            .is_some_and(|request| request.service_id == service.id);
        let active = service.active_state.trim().eq_ignore_ascii_case("active")
            || service.active_state.trim().eq_ignore_ascii_case("running");
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.render_host_service_logs_button(connection_id, service, is_running, cx))
            .child(self.render_host_service_follow_logs_button(
                connection_id,
                service,
                is_running,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Start,
                LucideIcon::Play,
                "sidebar.host_services.actions.start",
                false,
                is_running || active,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Stop,
                LucideIcon::Square,
                "sidebar.host_services.actions.stop",
                true,
                is_running || !active,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Restart,
                LucideIcon::RefreshCw,
                "sidebar.host_services.actions.restart",
                true,
                is_running,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Reload,
                LucideIcon::RefreshCcw,
                "sidebar.host_services.actions.reload",
                false,
                is_running,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Enable,
                LucideIcon::CheckCircle,
                "sidebar.host_services.actions.enable",
                false,
                is_running,
                cx,
            ))
            .child(self.render_host_service_action_button(
                connection_id,
                service,
                ServiceActionKind::Disable,
                LucideIcon::ShieldOff,
                "sidebar.host_services.actions.disable",
                true,
                is_running,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_service_action_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        action: ServiceActionKind,
        icon: LucideIcon,
        label_key: &'static str,
        danger: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key);
        let unsupported = self
            .host_service_action_command(connection_id, &service.id, action.clone())
            .is_err();
        let disabled = disabled || unsupported;
        let icon_color = if danger { MONITOR_RED } else { theme.text };
        self.workspace_tooltip_icon_button(
            icon,
            13.0,
            rgb(icon_color),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled,
                has_background: true,
                background: Some(if danger {
                    rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)
                } else {
                    rgb(theme.bg_hover)
                }),
                hover_background: Some(if danger {
                    rgba((MONITOR_RED << 8) | 0x30)
                } else {
                    rgb(theme.bg_panel)
                }),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            label,
            "host-service-action",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let service_id = service.id.clone();
                let description = service.description.clone();
                move |this, _event, _window, cx| {
                    this.request_host_service_action(
                        connection_id.clone(),
                        service_id.clone(),
                        description.clone(),
                        action.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    fn render_host_service_logs_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = self
            .host_service_logs_command(connection_id, &service.id)
            .is_err();
        self.workspace_tooltip_icon_button(
            LucideIcon::FileText,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            self.i18n.t("sidebar.host_services.actions.logs"),
            "host-service-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let service_id = service.id.clone();
                let description = service.description.clone();
                move |this, _event, _window, cx| {
                    this.request_host_service_logs(
                        connection_id.clone(),
                        service_id.clone(),
                        description.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    fn render_host_service_follow_logs_button(
        &self,
        connection_id: &str,
        service: &ResourceService,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = self
            .host_service_follow_logs_command(connection_id, &service.id)
            .is_err()
            || self.node_router.node_id_for_connection(connection_id).is_none();
        self.workspace_tooltip_icon_button(
            LucideIcon::Activity,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            self.i18n.t("sidebar.host_services.actions.follow_logs"),
            "host-service-follow-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let service_id = service.id.clone();
                let description = service.description.clone();
                move |this, _event, window, cx| {
                    this.open_host_service_follow_logs_terminal(
                        connection_id.clone(),
                        service_id.clone(),
                        description.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    fn render_host_service_detail(&self, service: &ResourceService) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
            .px_3()
            .pb_3()
            .pt_2()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_services.columns.description"),
                service.description.clone(),
            ))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_services.columns.load"),
                service.load_state.clone(),
            ))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_services.columns.sub_state"),
                service.sub_state.clone(),
            ))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font)
                    .text_color(rgb(theme.text))
                    .child(service.id.clone()),
            )
            .into_any_element()
    }

    fn render_host_docker_list(
        &self,
        rows: Vec<ResourceDockerContainer>,
        has_metrics: bool,
        status: ResourceDockerStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_metrics {
            return monitor_center_state(
                self,
                LucideIcon::Layers,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_docker.sampling"),
                cx,
            );
        }
        match status {
            ResourceDockerStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Layers,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_docker.unavailable"),
                    cx,
                );
            }
            ResourceDockerStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_docker.error", &[("error", message)]),
                    cx,
                );
            }
            ResourceDockerStatus::Unknown | ResourceDockerStatus::Available => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Layers,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_docker.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.connection_monitor.host_docker_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_docker_table_header())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(state, spec, move |index, _window, cx| {
                        let rows = rows.clone();
                        let selected_id = selected_id.clone();
                        workspace.update(cx, |this, cx| {
                            this.render_host_docker_row(
                                selected_id.as_str(),
                                rows.get(index).cloned(),
                                cx,
                            )
                        })
                    })),
            )
            .into_any_element()
    }

    fn render_host_docker_table_header(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_DOCKER_TABLE_HEADER_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg))
            .text_size(px(HOST_PROCESS_TABLE_HEADER_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(self.i18n.t("sidebar.host_docker.columns.container")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_DOCKER_STATE_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_docker.columns.state")),
            )
            .child(
                div()
                    .min_w(px(HOST_DOCKER_PORTS_COLUMN_MIN_WIDTH))
                    .flex_1()
                    .truncate()
                    .child(self.i18n.t("sidebar.host_docker.columns.ports")),
            )
            .into_any_element()
    }

    fn render_host_docker_row(
        &self,
        connection_id: &str,
        container: Option<ResourceDockerContainer>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(container) = container else {
            return div().into_any_element();
        };
        let expanded = self
            .connection_monitor
            .host_docker_expanded_id
            .as_deref()
            == Some(container.id.as_str());
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let state_label = self.i18n.t(docker_state_label_key(&container.state));
        let ports = container.ports.clone().unwrap_or_else(|| "—".to_string());
        let image_status = if container.image == "-" {
            container.status.clone()
        } else {
            format!("{} · {}", container.image, container.status)
        };

        div()
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(HOST_DOCKER_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(container.name.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_DOCKER_STATE_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(docker_state_color(&container.state, theme.text_muted)))
                            .font_family(mono_font.clone())
                            .child(state_label),
                    )
                    .child(
                        div()
                            .min_w(px(HOST_DOCKER_PORTS_COLUMN_MIN_WIDTH))
                            .flex_1()
                            .truncate()
                            .whitespace_nowrap()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(ports),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font)
                            .child(image_status),
                    )
                    .child(self.render_host_docker_inline_actions(connection_id, &container, cx)),
            )
            .when(expanded, |row| row.child(self.render_host_docker_detail(&container)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = container.id.clone();
                    move |this, _event, _window, cx| {
                        if this
                            .connection_monitor
                            .host_docker_expanded_id
                            .as_deref()
                            == Some(id.as_str())
                        {
                            this.connection_monitor.host_docker_expanded_id = None;
                        } else {
                            this.connection_monitor.host_docker_expanded_id = Some(id.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    fn render_host_docker_inline_actions(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self
            .connection_monitor
            .host_docker_action_running
            .as_ref()
            .is_some_and(|request| request.container_id == container.id);
        let state = container.state.trim().to_lowercase();
        let running_container = matches!(state.as_str(), "running" | "restarting" | "paused");
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.render_host_docker_logs_button(connection_id, container, is_running, cx))
            .child(self.render_host_docker_follow_logs_button(
                connection_id,
                container,
                is_running || !running_container,
                cx,
            ))
            .child(self.render_host_docker_exec_button(
                connection_id,
                container,
                is_running || !running_container,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Start,
                LucideIcon::Play,
                "sidebar.host_docker.actions.start",
                false,
                is_running || running_container,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Stop,
                LucideIcon::Square,
                "sidebar.host_docker.actions.stop",
                true,
                is_running || !running_container,
                cx,
            ))
            .child(self.render_host_docker_action_button(
                connection_id,
                container,
                DockerActionKind::Restart,
                LucideIcon::RefreshCw,
                "sidebar.host_docker.actions.restart",
                true,
                is_running || !running_container,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_docker_action_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        action: DockerActionKind,
        icon: LucideIcon,
        label_key: &'static str,
        danger: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key);
        let unsupported =
            self.host_docker_action_command(connection_id, &container.id, action.clone()).is_err();
        let disabled = disabled || unsupported;
        let icon_color = if danger { MONITOR_RED } else { theme.text };
        self.workspace_tooltip_icon_button(
            icon,
            13.0,
            rgb(icon_color),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled,
                has_background: true,
                background: Some(if danger {
                    rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)
                } else {
                    rgb(theme.bg_hover)
                }),
                hover_background: Some(if danger {
                    rgba((MONITOR_RED << 8) | 0x30)
                } else {
                    rgb(theme.bg_panel)
                }),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            label,
            "host-docker-action",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |this, _event, _window, cx| {
                    this.request_host_docker_action(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        action.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    fn render_host_docker_logs_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = self
            .host_docker_logs_command(connection_id, &container.id)
            .is_err();
        self.workspace_tooltip_icon_button(
            LucideIcon::FileText,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            self.i18n.t("sidebar.host_docker.actions.logs"),
            "host-docker-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |this, _event, _window, cx| {
                    this.request_host_docker_logs(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    fn render_host_docker_follow_logs_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = build_docker_follow_logs_command(&container.id).is_err()
            || self.node_router.node_id_for_connection(connection_id).is_none();
        self.workspace_tooltip_icon_button(
            LucideIcon::Activity,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            self.i18n.t("sidebar.host_docker.actions.follow_logs"),
            "host-docker-follow-logs",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |this, _event, window, cx| {
                    this.open_host_docker_follow_logs_terminal(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    fn render_host_docker_exec_button(
        &self,
        connection_id: &str,
        container: &ResourceDockerContainer,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let unsupported = build_docker_exec_shell_command(&container.id).is_err()
            || self.node_router.node_id_for_connection(connection_id).is_none();
        self.workspace_tooltip_icon_button(
            LucideIcon::Terminal,
            13.0,
            rgb(theme.text),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled: disabled || unsupported,
                has_background: true,
                background: Some(rgb(theme.bg_hover)),
                hover_background: Some(rgb(theme.bg_panel)),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            self.i18n.t("sidebar.host_docker.actions.exec"),
            "host-docker-exec",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let container_id = container.id.clone();
                let container_name = container.name.clone();
                move |this, _event, window, cx| {
                    this.open_host_docker_exec_terminal(
                        connection_id.clone(),
                        container_id.clone(),
                        container_name.clone(),
                        window,
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    fn render_host_docker_detail(&self, container: &ResourceDockerContainer) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
            .px_3()
            .pb_3()
            .pt_2()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(self.render_host_process_detail_line("ID", container.id.clone()))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_docker.columns.image"),
                container.image.clone(),
            ))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_docker.columns.ports"),
                container.ports.clone().unwrap_or_else(|| "—".to_string()),
            ))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font)
                    .text_color(rgb(theme.text))
                    .child(container.status.clone()),
            )
            .into_any_element()
    }

    fn render_host_process_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostProcessSearch;
        let focused = self.connection_monitor.host_process_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_process_search_query,
                    placeholder: self.i18n.t("sidebar.host_processes.search_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.connection_monitor.host_process_search_focused = true;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
                    this.ime_marked_text = None;
                    this.new_connection_caret_visible = true;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn render_host_process_filter_row(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .gap_1()
            .min_w_0()
            .child(self.render_host_process_filter_chip(
                ProcessFilter::All,
                "sidebar.host_processes.filters.all",
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::Running,
                "sidebar.host_processes.filters.running",
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::HighCpu,
                "sidebar.host_processes.filters.high_cpu",
                cx,
            ))
            .child(self.render_host_process_filter_chip(
                ProcessFilter::HighMemory,
                "sidebar.host_processes.filters.high_memory",
                cx,
            ))
            .into_any_element()
    }

    fn render_host_process_filter_chip(
        &self,
        filter: ProcessFilter,
        label_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.connection_monitor.host_process_filter == filter;
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .px_2()
            .h(px(24.0))
            .flex()
            .items_center()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(11.0))
            .cursor_pointer()
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "host-process-filter",
                label_key,
                self.i18n.t(label_key),
                if active { theme.text } else { theme.text_muted },
                cx,
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.connection_monitor.host_process_filter = filter;
                    this.connection_monitor.host_process_expanded_pid = None;
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_process_sort_row(&self, visible_count: usize, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .text_size(px(11.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .child(div().flex_none().child(format!(
                "{} {}",
                visible_count,
                self.i18n.t("sidebar.host_processes.count_suffix")
            )))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap_1()
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Cpu,
                        "sidebar.host_processes.sort.cpu",
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Memory,
                        "sidebar.host_processes.sort.memory",
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Pid,
                        "sidebar.host_processes.sort.pid",
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::Command,
                        "sidebar.host_processes.sort.command",
                        cx,
                    ))
                    .child(self.render_host_process_sort_button(
                        ProcessSort::User,
                        "sidebar.host_processes.sort.user",
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_host_process_sort_button(
        &self,
        sort: ProcessSort,
        label_key: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.connection_monitor.host_process_sort == sort;
        let theme = self.tokens.ui;
        let mut label = self.i18n.t(label_key);
        if active {
            label.push_str(if self.connection_monitor.host_process_sort_descending {
                " ↓"
            } else {
                " ↑"
            });
        }
        div()
            .flex_none()
            .px_1p5()
            .h(px(22.0))
            .flex()
            .items_center()
            .rounded(px(self.tokens.radii.sm))
            .cursor_pointer()
            .bg(if active {
                rgb(theme.bg_hover)
            } else {
                rgba(0x00000000)
            })
            .text_color(if active {
                rgb(theme.text)
            } else {
                rgb(theme.text_muted)
            })
            .hover(move |button| button.bg(rgb(theme.bg_hover)))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_process_sort == sort {
                        this.connection_monitor.host_process_sort_descending =
                            !this.connection_monitor.host_process_sort_descending;
                    } else {
                        this.connection_monitor.host_process_sort = sort;
                        this.connection_monitor.host_process_sort_descending =
                            !matches!(sort, ProcessSort::Command | ProcessSort::User);
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_process_list(
        &self,
        rows: Vec<ResourceTopProcess>,
        has_metrics: bool,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !has_metrics {
            return monitor_center_state(
                self,
                LucideIcon::Activity,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_processes.sampling"),
                cx,
            );
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::ListChecks,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_processes.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.connection_monitor.host_process_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        let separate_user_column = host_process_table_uses_separate_user_column(self.ai_sidebar_width);
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            // Processes are an operational table, not a card stack; keep the
            // header fixed while the GPUI List owns only the scrolling rows.
            .child(self.render_host_process_table_header(separate_user_column))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(state, spec, move |index, _window, cx| {
                        let rows = rows.clone();
                        let selected_id = selected_id.clone();
                        workspace.update(cx, |this, cx| {
                            this.render_host_process_row(
                                selected_id.as_str(),
                                rows.get(index).cloned(),
                                separate_user_column,
                                cx,
                            )
                        })
                    })),
            )
            .into_any_element()
    }

    fn render_host_process_table_header(&self, separate_user_column: bool) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_PROCESS_TABLE_HEADER_HEIGHT))
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg))
            .text_size(px(HOST_PROCESS_TABLE_HEADER_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .truncate()
                    .child(host_process_identity_header_label(
                        &self.i18n,
                        separate_user_column,
                    )),
            )
            .when(separate_user_column, |header| {
                header.child(
                    div()
                        .flex_none()
                        .w(px(HOST_PROCESS_USER_COLUMN_WIDTH))
                        .truncate()
                        .child(self.i18n.t("sidebar.host_processes.sort.user")),
                )
            })
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_PID_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_processes.sort.pid")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_CPU_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_processes.sort.cpu")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PROCESS_MEMORY_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_processes.sort.memory")),
            )
            .into_any_element()
    }

    fn render_host_process_row(
        &self,
        connection_id: &str,
        process: Option<ResourceTopProcess>,
        separate_user_column: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(process) = process else {
            return div().into_any_element();
        };
        let expanded = self
            .connection_monitor
            .host_process_expanded_pid
            .as_deref()
            == Some(process.pid.as_str());
        let theme = self.tokens.ui;
        let status = process
            .state
            .as_deref()
            .map(|state| self.i18n.t(process_state_label_key(state)))
            .unwrap_or_else(|| self.i18n.t("sidebar.host_processes.unknown"));
        let user = process
            .user
            .clone()
            .unwrap_or_else(|| self.i18n.t("sidebar.host_processes.unknown"));
        let cpu = process
            .cpu_percent
            .map(|value| format!("{value:.1}%"))
            .unwrap_or_else(|| "—".to_string());
        let memory = format!("{:.1}%", process.memory_percent);
        let cpu_color = threshold_color(process.cpu_percent);
        let memory_color = threshold_color(Some(process.memory_percent));
        let mono_font = settings_mono_font_family(self.settings_store.settings());

        div()
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .h(px(HOST_PROCESS_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .items_center()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(process_display_name(&process)),
                    )
                    .when(!separate_user_column, |main| {
                        main.child(
                            div()
                                .min_w(px(0.0))
                                .flex_1()
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(user.clone()),
                        )
                    })
                    .when(separate_user_column, |main| {
                        main.child(
                            div()
                                .flex_none()
                                .w(px(HOST_PROCESS_USER_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(user.clone()),
                        )
                    })
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PROCESS_PID_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(process.pid.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PROCESS_CPU_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(cpu_color))
                            .font_family(mono_font.clone())
                            .child(cpu),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PROCESS_MEMORY_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(memory_color))
                            .font_family(mono_font.clone())
                            .child(memory),
                    )
            )
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .px_3()
                    .pb_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Keep actions visible without stealing the btop-like
                    // Program/User/PID/CPU/Mem columns in the narrow sidebar.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(format!("{status} · {}", process_display_command(&process))),
                    )
                    .child(self.render_host_process_inline_actions(connection_id, &process, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_process_detail(connection_id, &process, cx))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let pid = process.pid.clone();
                    move |this, _event, _window, cx| {
                        if this
                            .connection_monitor
                            .host_process_expanded_pid
                            .as_deref()
                            == Some(pid.as_str())
                        {
                            this.connection_monitor.host_process_expanded_pid = None;
                        } else {
                            this.connection_monitor.host_process_expanded_pid = Some(pid.clone());
                        }
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .into_any_element()
    }

    fn render_host_process_inline_actions(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_running = self
            .connection_monitor
            .host_process_action_running
            .as_ref()
            .is_some_and(|request| request.pid == process.pid);
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.render_host_process_action_button(
                connection_id,
                process,
                ProcessActionKind::Term,
                LucideIcon::Power,
                "sidebar.host_processes.actions.term",
                false,
                is_running,
                cx,
            ))
            .child(self.render_host_process_action_button(
                connection_id,
                process,
                ProcessActionKind::Kill,
                LucideIcon::Zap,
                "sidebar.host_processes.actions.kill",
                true,
                is_running,
                cx,
            ))
            .child(self.render_host_process_action_button(
                connection_id,
                process,
                ProcessActionKind::Stop,
                LucideIcon::Pause,
                "sidebar.host_processes.actions.stop",
                false,
                is_running,
                cx,
            ))
            .child(self.render_host_process_action_button(
                connection_id,
                process,
                ProcessActionKind::Cont,
                LucideIcon::Play,
                "sidebar.host_processes.actions.cont",
                false,
                is_running,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_process_detail(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
            .px_3()
            .pb_3()
            .pt_2()
            .border_t_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .flex()
            .flex_col()
            .gap_1()
            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
            .text_color(rgb(theme.text_muted))
            .child(self.render_host_process_detail_line(
                "PPID",
                process.ppid.clone().unwrap_or_else(|| "—".to_string()),
            ))
            .child(self.render_host_process_detail_line(
                "RSS",
                process
                    .rss_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "—".to_string()),
            ))
            .child(self.render_host_process_detail_line(
                "VSZ",
                process
                    .vsz_bytes
                    .map(format_bytes)
                    .unwrap_or_else(|| "—".to_string()),
            ))
            .child(self.render_host_process_detail_line(
                self.i18n.t("sidebar.host_processes.elapsed"),
                process.elapsed.clone().unwrap_or_else(|| "—".to_string()),
            ))
            .child(self.render_host_process_action_bar(connection_id, process, cx))
            .child(
                div()
                    .mt_1()
                    .min_w_0()
                    .font_family(mono_font)
                    .text_color(rgb(theme.text))
                    .child(process_display_command(process)),
            )
            .into_any_element()
    }

    fn render_host_process_detail_line(&self, label: impl Into<String>, value: String) -> AnyElement {
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .min_w_0()
            .child(div().flex_none().child(label.into()))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_family(mono_font)
                    .child(value),
            )
            .into_any_element()
    }

    fn render_host_process_action_bar(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let is_running = self
            .connection_monitor
            .host_process_action_running
            .as_ref()
            .is_some_and(|request| request.pid == process.pid);
        div()
            .mt_2()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .min_w_0()
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(self.i18n.t("sidebar.host_processes.actions.renice")),
                    )
                    .child(self.render_host_process_renice_input(cx))
                    .child(self.render_host_process_action_button(
                        connection_id,
                        process,
                        ProcessActionKind::Renice {
                            nice: self.host_process_renice_value(),
                        },
                        LucideIcon::Gauge,
                        "sidebar.host_processes.actions.apply",
                        false,
                        is_running,
                        cx,
                    )),
            )
            .into_any_element()
    }

    fn render_host_process_action_button(
        &self,
        connection_id: &str,
        process: &ResourceTopProcess,
        action: ProcessActionKind,
        icon: LucideIcon,
        label_key: &'static str,
        danger: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let label = self.i18n.t(label_key);
        let unsupported = self.host_process_action_command(connection_id, &process.pid, action.clone()).is_err();
        let disabled = disabled || unsupported;
        let icon_color = if danger { MONITOR_RED } else { theme.text };
        self.workspace_tooltip_icon_button(
            icon,
            13.0,
            rgb(icon_color),
            oxideterm_gpui_ui::button::IconButtonOptions {
                size: 22.0,
                disabled,
                has_background: true,
                background: Some(if danger {
                    rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA)
                } else {
                    rgb(theme.bg_hover)
                }),
                hover_background: Some(if danger {
                    rgba((MONITOR_RED << 8) | 0x30)
                } else {
                    rgb(theme.bg_panel)
                }),
                idle_opacity: 1.0,
                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
            },
            label,
            "host-process-action",
            true,
            cx.listener({
                let connection_id = connection_id.to_string();
                let pid = process.pid.clone();
                let command = process_display_name(process);
                move |this, _event, _window, cx| {
                    this.request_host_process_action(
                        connection_id.clone(),
                        pid.clone(),
                        command.clone(),
                        action.clone(),
                        cx,
                    );
                    cx.stop_propagation();
                }
            }),
            cx.entity(),
        )
    }

    fn render_host_process_renice_input(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostProcessRenice;
        let focused = self.connection_monitor.host_process_renice_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_process_renice_value,
                    placeholder: self.i18n.t("sidebar.host_processes.actions.renice_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target),
                    marked_text: self.marked_text_for_target(target),
                },
            )
            .w(px(54.0))
            .h(px(26.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = true;
                    this.ime_marked_text = None;
                    this.new_connection_caret_visible = true;
                    window.focus(&this.focus_handle);
                    this.begin_ime_selection_from_mouse_down(target, event, window, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.update_ime_selection_drag_from_mouse_move(event, window, cx);
            })),
            move |anchor, _window, cx| {
                let _ = workspace.update(cx, |this, cx| {
                    this.update_text_input_anchor(anchor, cx);
                });
            },
        )
        .into_any_element()
    }

    fn host_process_action_command(
        &self,
        connection_id: &str,
        pid: &str,
        action: ProcessActionKind,
    ) -> Result<oxideterm_connection_monitor::ProcessActionCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Linux".to_string());
        build_process_action_command(&os_type, pid, action)
    }

    fn visible_host_process_rows(&self, processes: &[ResourceTopProcess]) -> Vec<ResourceTopProcess> {
        visible_process_rows(
            processes,
            &self.connection_monitor.host_process_search_query,
            self.connection_monitor.host_process_filter,
            self.connection_monitor.host_process_sort,
            self.connection_monitor.host_process_sort_descending,
        )
    }

    fn sync_host_process_list_state(&self, rows: &[ResourceTopProcess], selected_id: &str) {
        let signatures = rows
            .iter()
            .map(process_row_signature)
            .collect::<Vec<_>>();
        let identity = format!(
            "host-processes:{selected_id}:{}:{}:{}:{}",
            self.connection_monitor.host_process_search_query,
            self.connection_monitor.host_process_filter as u8,
            self.connection_monitor.host_process_sort as u8,
            self.connection_monitor.host_process_sort_descending
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_process_list_state,
            &mut self
                .connection_monitor
                .host_process_list_cache
                .borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_PROCESS_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    fn sync_host_docker_list_state(&self, rows: &[ResourceDockerContainer], selected_id: &str) {
        let signatures = rows.iter().map(docker_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-docker:{selected_id}:{}",
            self.connection_monitor.host_docker_search_query
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_docker_list_state,
            &mut self.connection_monitor.host_docker_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_DOCKER_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    fn sync_host_service_list_state(&self, rows: &[ResourceService], selected_id: &str) {
        let signatures = rows.iter().map(service_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-services:{selected_id}:{}",
            self.connection_monitor.host_service_search_query
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_service_list_state,
            &mut self
                .connection_monitor
                .host_service_list_cache
                .borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_SERVICE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    fn host_docker_action_command(
        &self,
        connection_id: &str,
        container_id: &str,
        action: DockerActionKind,
    ) -> Result<oxideterm_connection_monitor::DockerActionCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Linux".to_string());
        build_docker_action_command(&os_type, container_id, action)
    }

    fn host_docker_logs_command(
        &self,
        connection_id: &str,
        container_id: &str,
    ) -> Result<oxideterm_connection_monitor::DockerCaptureCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Linux".to_string());
        build_docker_logs_command(&os_type, container_id)
    }

    fn host_service_action_command(
        &self,
        connection_id: &str,
        service_id: &str,
        action: ServiceActionKind,
    ) -> Result<oxideterm_connection_monitor::ServiceActionCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Linux".to_string());
        build_service_action_command(&os_type, service_id, action)
    }

    fn host_service_logs_command(
        &self,
        connection_id: &str,
        service_id: &str,
    ) -> Result<oxideterm_connection_monitor::ServiceCaptureCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Linux".to_string());
        build_service_logs_command(&os_type, service_id)
    }

    fn host_service_follow_logs_command(
        &self,
        connection_id: &str,
        service_id: &str,
    ) -> Result<oxideterm_connection_monitor::ServiceCaptureCommand, String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Linux".to_string());
        build_service_follow_logs_command(&os_type, service_id)
    }

    fn refresh_host_docker_snapshot(&mut self, connection_id: String, cx: &mut Context<Self>) {
        self.connection_monitor
            .profiler_registry
            .stop(&connection_id);
        self.start_connection_monitor_profiler(connection_id, cx);
    }

    fn refresh_host_service_snapshot(&mut self, connection_id: String, cx: &mut Context<Self>) {
        self.connection_monitor
            .profiler_registry
            .stop(&connection_id);
        self.start_connection_monitor_profiler(connection_id, cx);
    }

    pub(super) fn handle_host_process_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_process_search_focused
            && !self.connection_monitor.host_process_renice_focused
        {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_process_search_focused = false;
            self.connection_monitor.host_process_renice_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn handle_host_docker_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_docker_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_docker_search_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn handle_host_service_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_service_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_service_search_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    fn host_process_renice_value(&self) -> i32 {
        self.connection_monitor
            .host_process_renice_value
            .trim()
            .parse::<i32>()
            .unwrap_or(0)
            .clamp(-20, 19)
    }

    fn request_host_process_action(
        &mut self,
        connection_id: String,
        pid: String,
        command: String,
        action: ProcessActionKind,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_process_action_running.is_some() {
            self.push_host_process_toast(
                self.i18n
                    .t("sidebar.host_processes.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        if let ProcessActionKind::Renice { nice } = action
            && !(-20..=19).contains(&nice)
        {
            self.push_host_process_toast(
                self.i18n.t("sidebar.host_processes.toast.invalid_nice"),
                TerminalNoticeVariant::Error,
            );
            return;
        }
        self.connection_monitor.host_process_pending_confirm =
            Some(HostProcessActionRequest {
                connection_id,
                pid,
                command,
                action,
            });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    fn request_host_docker_action(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        action: DockerActionKind,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_docker_action_running.is_some() {
            self.push_host_docker_toast(
                self.i18n
                    .t("sidebar.host_docker.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        self.connection_monitor.host_docker_pending_confirm = Some(HostDockerActionRequest {
            connection_id,
            container_id,
            container_name,
            action,
        });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    fn request_host_docker_logs(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_docker_logs_polling {
            self.push_host_docker_toast(
                self.i18n.t("sidebar.host_docker.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            self.push_host_docker_toast(
                self.i18n.t("sidebar.host_docker.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Linux".to_string());
        let command = match build_docker_logs_command(&os_type, &container_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_docker_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        let request = HostDockerLogsRequest {
            connection_id,
            container_id,
            container_name,
        };
        self.connection_monitor.host_docker_logs_dialog = Some(HostDockerLogsDialog {
            request: request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_docker_logs_rx = Some(rx);
        self.connection_monitor.host_docker_logs_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_DOCKER_LOGS_TIMEOUT,
                    HOST_DOCKER_LOGS_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostDockerLogsDelivery { request, result });
        });
        cx.notify();
    }

    fn request_host_service_action(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        action: ServiceActionKind,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_service_action_running.is_some() {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.action_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        self.connection_monitor.host_service_pending_confirm = Some(HostServiceActionRequest {
            connection_id,
            service_id,
            description,
            action,
        });
        self.reset_standard_confirm_focus();
        cx.notify();
    }

    fn request_host_service_logs(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_service_logs_polling {
            self.push_host_service_toast(
                self.i18n.t("sidebar.host_services.toast.logs_already_running"),
                TerminalNoticeVariant::Warning,
            );
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            self.push_host_service_toast(
                self.i18n.t("sidebar.host_services.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Linux".to_string());
        let command = match build_service_logs_command(&os_type, &service_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_service_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == ServiceCommandCapability::Partial {
            self.push_host_service_toast(
                self.i18n_replace(
                    "sidebar.host_services.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }
        let request = HostServiceLogsRequest {
            connection_id,
            service_id,
            description,
        };
        self.connection_monitor.host_service_logs_dialog = Some(HostServiceLogsDialog {
            request: request.clone(),
            output: None,
            error: None,
            loading: true,
        });
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_service_logs_rx = Some(rx);
        self.connection_monitor.host_service_logs_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_SERVICE_LOGS_TIMEOUT,
                    HOST_SERVICE_LOGS_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostServiceLogsDelivery { request, result });
        });
        cx.notify();
    }

    fn open_host_docker_exec_terminal(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match build_docker_exec_shell_command(&container_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_docker_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        let title = self.i18n_replace(
            "sidebar.host_docker.exec_title",
            &[("name", container_name.clone())],
        );
        self.open_host_docker_terminal_command(
            connection_id,
            container_name,
            command,
            title,
            "sidebar.host_docker.toast.exec_opened",
            window,
            cx,
        );
    }

    fn open_host_docker_follow_logs_terminal(
        &mut self,
        connection_id: String,
        container_id: String,
        container_name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match build_docker_follow_logs_command(&container_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_docker_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        let title = self.i18n_replace(
            "sidebar.host_docker.follow_title",
            &[("name", container_name.clone())],
        );
        // Follow mode belongs in a visible terminal so Ctrl-C and tab lifecycle stop the stream.
        self.open_host_docker_terminal_command(
            connection_id,
            container_name,
            command,
            title,
            "sidebar.host_docker.toast.follow_opened",
            window,
            cx,
        );
    }

    fn open_host_docker_terminal_command(
        &mut self,
        connection_id: String,
        container_name: String,
        command: String,
        title: String,
        opened_toast_key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_docker_toast(
                self.i18n
                    .t("sidebar.host_docker.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_docker_toast(
                self.i18n
                    .t("sidebar.host_docker.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        match self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id,
            Some(command),
            node.config,
            title,
            node.saved_connection_id,
            None,
            None,
            window,
            cx,
        ) {
            Ok(()) => self.push_host_docker_toast(
                self.i18n_replace(opened_toast_key, &[("name", container_name)]),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_docker_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }

    fn open_host_service_follow_logs_terminal(
        &mut self,
        connection_id: String,
        service_id: String,
        description: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            self.push_host_service_toast(
                self.i18n.t("sidebar.host_services.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Linux".to_string());
        let command = match build_service_follow_logs_command(&os_type, &service_id) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_service_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == ServiceCommandCapability::Partial {
            self.push_host_service_toast(
                self.i18n_replace(
                    "sidebar.host_services.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }
        let title = self.i18n_replace(
            "sidebar.host_services.follow_title",
            &[("name", service_id.clone())],
        );
        // Follow mode belongs in a visible terminal so Ctrl-C and tab lifecycle stop the stream.
        self.open_host_service_terminal_command(
            connection_id,
            description,
            command.command,
            title,
            "sidebar.host_services.toast.follow_opened",
            window,
            cx,
        );
    }

    fn open_host_service_terminal_command(
        &mut self,
        connection_id: String,
        description: String,
        command: String,
        title: String,
        opened_toast_key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_service_toast(
                self.i18n
                    .t("sidebar.host_services.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        match self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id,
            Some(command),
            node.config,
            title,
            node.saved_connection_id,
            None,
            None,
            window,
            cx,
        ) {
            Ok(()) => self.push_host_service_toast(
                self.i18n_replace(opened_toast_key, &[("name", description)]),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_service_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }


    pub(super) fn handle_host_process_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.connection_monitor.host_process_pending_confirm.is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.connection_monitor.host_process_pending_confirm = None;
                self.clear_standard_confirm_focus();
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_process_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_host_docker_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.connection_monitor.host_docker_pending_confirm.is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.connection_monitor.host_docker_pending_confirm = None;
                self.clear_standard_confirm_focus();
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_docker_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    pub(super) fn handle_host_service_confirm_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.connection_monitor.host_service_pending_confirm.is_none() {
            return false;
        }
        match self.handle_standard_confirm_key(event, cx) {
            Some(ConfirmKeyboardAction::Cancel) => {
                self.connection_monitor.host_service_pending_confirm = None;
                self.clear_standard_confirm_focus();
                cx.notify();
                true
            }
            Some(ConfirmKeyboardAction::Confirm) => {
                self.confirm_host_service_action(cx);
                true
            }
            Some(ConfirmKeyboardAction::Handled) => true,
            None => false,
        }
    }

    fn confirm_host_process_action(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.connection_monitor.host_process_pending_confirm.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        self.start_host_process_action(request, cx);
    }

    fn confirm_host_docker_action(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.connection_monitor.host_docker_pending_confirm.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        self.start_host_docker_action(request, cx);
    }

    fn confirm_host_service_action(&mut self, cx: &mut Context<Self>) {
        let Some(request) = self.connection_monitor.host_service_pending_confirm.take() else {
            return;
        };
        self.clear_standard_confirm_focus();
        self.start_host_service_action(request, cx);
    }

    fn start_host_process_action(
        &mut self,
        request: HostProcessActionRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            self.push_host_process_toast(
                self.i18n.t("sidebar.host_processes.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Linux".to_string());
        let command = match build_process_action_command(
            &os_type,
            &request.pid,
            request.action.clone(),
        ) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_process_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == ProcessCommandCapability::Partial {
            self.push_host_process_toast(
                self.i18n_replace(
                    "sidebar.host_processes.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let delivery_request = request.clone();
        self.connection_monitor.host_process_action_running = Some(request);
        self.connection_monitor.host_process_action_rx = Some(rx);
        self.connection_monitor.host_process_action_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_PROCESS_ACTION_TIMEOUT,
                    HOST_PROCESS_ACTION_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostProcessActionDelivery {
                request: delivery_request,
                result,
            });
        });
        cx.notify();
    }

    fn start_host_docker_action(
        &mut self,
        request: HostDockerActionRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            self.push_host_docker_toast(
                self.i18n.t("sidebar.host_docker.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Linux".to_string());
        let command =
            match build_docker_action_command(&os_type, &request.container_id, request.action.clone()) {
                Ok(command) => command,
                Err(error) => {
                    self.push_host_docker_toast(error, TerminalNoticeVariant::Error);
                    cx.notify();
                    return;
                }
            };

        let (tx, rx) = std::sync::mpsc::channel();
        let delivery_request = request.clone();
        self.connection_monitor.host_docker_action_running = Some(request);
        self.connection_monitor.host_docker_action_rx = Some(rx);
        self.connection_monitor.host_docker_action_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_DOCKER_ACTION_TIMEOUT,
                    HOST_DOCKER_ACTION_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostDockerActionDelivery {
                request: delivery_request,
                result,
            });
        });
        cx.notify();
    }

    fn start_host_service_action(
        &mut self,
        request: HostServiceActionRequest,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.ssh_registry.get(&request.connection_id) else {
            self.push_host_service_toast(
                self.i18n.t("sidebar.host_services.toast.connection_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let os_type = handle
            .remote_env()
            .map(|env| env.os_type)
            .unwrap_or_else(|| "Linux".to_string());
        let command = match build_service_action_command(
            &os_type,
            &request.service_id,
            request.action.clone(),
        ) {
            Ok(command) => command,
            Err(error) => {
                self.push_host_service_toast(error, TerminalNoticeVariant::Error);
                cx.notify();
                return;
            }
        };
        if command.capability == ServiceCommandCapability::Partial {
            self.push_host_service_toast(
                self.i18n_replace(
                    "sidebar.host_services.toast.partial_support",
                    &[("os", os_type.clone())],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let (tx, rx) = std::sync::mpsc::channel();
        let delivery_request = request.clone();
        self.connection_monitor.host_service_action_running = Some(request);
        self.connection_monitor.host_service_action_rx = Some(rx);
        self.connection_monitor.host_service_action_polling = true;
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_SERVICE_ACTION_TIMEOUT,
                    HOST_SERVICE_ACTION_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostServiceActionDelivery {
                request: delivery_request,
                result,
            });
        });
        cx.notify();
    }

    pub(super) fn poll_host_process_action_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_process_action_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_process_action_rx.take() else {
            self.connection_monitor.host_process_action_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_process_action_polling = false;
                self.connection_monitor.host_process_action_running = None;
                self.finish_host_process_action(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_process_action_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_process_action_polling = false;
                self.connection_monitor.host_process_action_running = None;
                self.push_host_process_toast(
                    self.i18n.t("sidebar.host_processes.toast.action_failed"),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
            }
        }
    }

    pub(super) fn poll_host_docker_action_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_docker_action_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_docker_action_rx.take() else {
            self.connection_monitor.host_docker_action_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_docker_action_polling = false;
                self.connection_monitor.host_docker_action_running = None;
                self.finish_host_docker_action(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_docker_action_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_docker_action_polling = false;
                self.connection_monitor.host_docker_action_running = None;
                self.push_host_docker_toast(
                    self.i18n.t("sidebar.host_docker.toast.action_failed"),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
            }
        }
    }

    pub(super) fn poll_host_docker_logs_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_docker_logs_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_docker_logs_rx.take() else {
            self.connection_monitor.host_docker_logs_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_docker_logs_polling = false;
                self.finish_host_docker_logs(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_docker_logs_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_docker_logs_polling = false;
                if let Some(dialog) = self
                    .connection_monitor
                    .host_docker_logs_dialog
                    .as_mut()
                {
                    dialog.loading = false;
                    dialog.error = Some(self.i18n.t("sidebar.host_docker.toast.logs_failed"));
                }
                cx.notify();
            }
        }
    }

    pub(super) fn poll_host_service_action_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_service_action_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_service_action_rx.take() else {
            self.connection_monitor.host_service_action_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_service_action_polling = false;
                self.connection_monitor.host_service_action_running = None;
                self.finish_host_service_action(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_service_action_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_service_action_polling = false;
                self.connection_monitor.host_service_action_running = None;
                self.push_host_service_toast(
                    self.i18n.t("sidebar.host_services.toast.action_failed"),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
            }
        }
    }

    pub(super) fn poll_host_service_logs_results(&mut self, cx: &mut Context<Self>) {
        if !self.connection_monitor.host_service_logs_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_service_logs_rx.take() else {
            self.connection_monitor.host_service_logs_polling = false;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.connection_monitor.host_service_logs_polling = false;
                self.finish_host_service_logs(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_service_logs_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.connection_monitor.host_service_logs_polling = false;
                if let Some(dialog) = self
                    .connection_monitor
                    .host_service_logs_dialog
                    .as_mut()
                {
                    dialog.loading = false;
                    dialog.error = Some(self.i18n.t("sidebar.host_services.toast.logs_failed"));
                }
                cx.notify();
            }
        }
    }

    fn finish_host_process_action(
        &mut self,
        delivery: HostProcessActionDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery.result {
            Ok(output) if process_action_succeeded(output.exit_code) => {
                self.push_host_process_toast(
                    process_action_success_message(&output.stdout, &output.stderr),
                    TerminalNoticeVariant::Success,
                );
            }
            Ok(output) => {
                self.push_host_process_toast(
                    process_action_failure_message(&output.stdout, &output.stderr, output.exit_code),
                    TerminalNoticeVariant::Error,
                );
            }
            Err(error) => {
                self.push_host_process_toast(error, TerminalNoticeVariant::Error);
            }
        }
        self.connection_monitor
            .profiler_registry
            .stop(&delivery.request.connection_id);
        self.start_connection_monitor_profiler(delivery.request.connection_id, cx);
    }

    fn finish_host_docker_action(
        &mut self,
        delivery: HostDockerActionDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery.result {
            Ok(output) if docker_action_succeeded(output.exit_code) => {
                self.push_host_docker_toast(
                    docker_action_success_message(&output.stdout, &output.stderr),
                    TerminalNoticeVariant::Success,
                );
            }
            Ok(output) => {
                self.push_host_docker_toast(
                    docker_action_failure_message(&output.stdout, &output.stderr, output.exit_code),
                    TerminalNoticeVariant::Error,
                );
            }
            Err(error) => {
                self.push_host_docker_toast(error, TerminalNoticeVariant::Error);
            }
        }
        self.refresh_host_docker_snapshot(delivery.request.connection_id, cx);
    }

    fn finish_host_docker_logs(
        &mut self,
        delivery: HostDockerLogsDelivery,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .connection_monitor
            .host_docker_logs_dialog
            .as_mut()
            .filter(|dialog| dialog.request == delivery.request)
        else {
            cx.notify();
            return;
        };
        dialog.loading = false;
        match delivery.result {
            Ok(output) if docker_action_succeeded(output.exit_code) => {
                let logs = if output.stdout.trim().is_empty() {
                    self.i18n.t("sidebar.host_docker.logs.empty")
                } else {
                    output.stdout
                };
                dialog.output = Some(logs);
                dialog.error = None;
            }
            Ok(output) => {
                dialog.output = None;
                dialog.error = Some(docker_action_failure_message(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                ));
            }
            Err(error) => {
                dialog.output = None;
                dialog.error = Some(error);
            }
        }
        cx.notify();
    }

    fn finish_host_service_action(
        &mut self,
        delivery: HostServiceActionDelivery,
        cx: &mut Context<Self>,
    ) {
        match delivery.result {
            Ok(output) if service_action_succeeded(output.exit_code) => {
                self.push_host_service_toast(
                    service_action_success_message(&output.stdout, &output.stderr),
                    TerminalNoticeVariant::Success,
                );
            }
            Ok(output) => {
                self.push_host_service_toast(
                    service_action_failure_message(
                        &output.stdout,
                        &output.stderr,
                        output.exit_code,
                    ),
                    TerminalNoticeVariant::Error,
                );
            }
            Err(error) => {
                self.push_host_service_toast(error, TerminalNoticeVariant::Error);
            }
        }
        self.refresh_host_service_snapshot(delivery.request.connection_id, cx);
    }

    fn finish_host_service_logs(
        &mut self,
        delivery: HostServiceLogsDelivery,
        cx: &mut Context<Self>,
    ) {
        let Some(dialog) = self
            .connection_monitor
            .host_service_logs_dialog
            .as_mut()
            .filter(|dialog| dialog.request == delivery.request)
        else {
            cx.notify();
            return;
        };
        dialog.loading = false;
        match delivery.result {
            Ok(output) if service_action_succeeded(output.exit_code) => {
                let logs = if output.stdout.trim().is_empty() {
                    self.i18n.t("sidebar.host_services.logs.empty")
                } else {
                    output.stdout
                };
                dialog.output = Some(logs);
                dialog.error = None;
            }
            Ok(output) => {
                dialog.output = None;
                dialog.error = Some(service_action_failure_message(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                ));
            }
            Err(error) => {
                dialog.output = None;
                dialog.error = Some(error);
            }
        }
        cx.notify();
    }

    fn push_host_process_toast(&mut self, message: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    fn push_host_docker_toast(&mut self, message: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    fn push_host_service_toast(&mut self, message: String, variant: TerminalNoticeVariant) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }

    pub(super) fn render_host_process_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let request = self
            .connection_monitor
            .host_process_pending_confirm
            .as_ref()?;
        let title = self.i18n.t("sidebar.host_processes.confirm.title");
        let description = self.i18n_replace(
            host_process_confirm_description_key(&request.action),
            &[
                ("pid", request.pid.clone()),
                ("command", request.command.clone()),
            ],
        );
        Some(confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: if matches!(request.action, ProcessActionKind::Kill) {
                    ConfirmDialogVariant::Danger
                } else {
                    ConfirmDialogVariant::Default
                },
                title: div().child(title).into_any_element(),
                description: Some(div().child(description).into_any_element()),
                cancel_label: div()
                    .child(self.i18n.t("sidebar.host_processes.confirm.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t(host_process_confirm_label_key(&request.action)))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.connection_monitor.host_process_pending_confirm = None;
                this.clear_standard_confirm_focus();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.confirm_host_process_action(cx);
            }),
        )
        .into_any_element())
    }

    pub(super) fn render_host_docker_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let request = self
            .connection_monitor
            .host_docker_pending_confirm
            .as_ref()?;
        let title = self.i18n.t("sidebar.host_docker.confirm.title");
        let description = self.i18n_replace(
            host_docker_confirm_description_key(&request.action),
            &[
                ("id", request.container_id.clone()),
                ("name", request.container_name.clone()),
            ],
        );
        Some(confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: if matches!(
                    request.action,
                    DockerActionKind::Stop | DockerActionKind::Restart
                ) {
                    ConfirmDialogVariant::Danger
                } else {
                    ConfirmDialogVariant::Default
                },
                title: div().child(title).into_any_element(),
                description: Some(div().child(description).into_any_element()),
                cancel_label: div()
                    .child(self.i18n.t("sidebar.host_docker.confirm.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t(host_docker_confirm_label_key(&request.action)))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.connection_monitor.host_docker_pending_confirm = None;
                this.clear_standard_confirm_focus();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.confirm_host_docker_action(cx);
            }),
        )
        .into_any_element())
    }

    pub(super) fn render_host_docker_logs_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.connection_monitor.host_docker_logs_dialog.as_ref()?;
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let follow_connection_id = dialog.request.connection_id.clone();
        let follow_container_id = dialog.request.container_id.clone();
        let follow_container_name = dialog.request.container_name.clone();
        let follow_logs_disabled =
            build_docker_follow_logs_command(&follow_container_id).is_err()
                || self
                    .node_router
                    .node_id_for_connection(&follow_connection_id)
                    .is_none();
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("sidebar.host_docker.logs.loading"))
                .into_any_element()
        } else if let Some(error) = dialog.error.as_ref() {
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element()
        } else {
            let output = dialog.output.clone().unwrap_or_default();
            // Docker logs keep their original line shape, so horizontal
            // overflow must belong to the dialog body rather than the row.
            let mut lines = div()
                .p_3()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .font_family(mono_font)
                .text_size(px(11.0))
                .text_color(rgb(theme.text));
            for (index, line) in output.lines().enumerate() {
                let line = if line.is_empty() {
                    " ".to_string()
                } else {
                    line.to_string()
                };
                lines = lines.child(
                    div()
                        .id(("host-docker-log-line", index))
                        .flex_none()
                        .whitespace_nowrap()
                        .child(line),
                );
            }
            lines.into_any_element()
        };

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.connection_monitor.host_docker_logs_dialog = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .w(px(HOST_DOCKER_LOGS_DIALOG_WIDTH))
                        .max_h(px(HOST_DOCKER_LOGS_DIALOG_MAX_HEIGHT))
                        .child(
                            div()
                                .flex_none()
                                .px_4()
                                .py_3()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text))
                                                .child(self.i18n_replace(
                                                    "sidebar.host_docker.logs.title",
                                                    &[(
                                                        "name",
                                                        dialog.request.container_name.clone(),
                                                    )],
                                                )),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(dialog.request.container_id.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(self.workspace_tooltip_icon_button(
                                            LucideIcon::Activity,
                                            14.0,
                                            rgb(theme.text),
                                            oxideterm_gpui_ui::button::IconButtonOptions {
                                                size: 24.0,
                                                disabled: follow_logs_disabled,
                                                has_background: true,
                                                background: Some(rgb(theme.bg_hover)),
                                                hover_background: Some(rgb(theme.bg_panel)),
                                                idle_opacity: 1.0,
                                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                                    24.0,
                                                )
                                            },
                                            self.i18n.t("sidebar.host_docker.actions.follow_logs"),
                                            "host-docker-logs-follow",
                                            true,
                                            cx.listener({
                                                let connection_id = follow_connection_id.clone();
                                                let container_id = follow_container_id.clone();
                                                let container_name = follow_container_name.clone();
                                                move |this, _event, window, cx| {
                                                    this.connection_monitor.host_docker_logs_dialog =
                                                        None;
                                                    this.open_host_docker_follow_logs_terminal(
                                                        connection_id.clone(),
                                                        container_id.clone(),
                                                        container_name.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }
                                            }),
                                            cx.entity(),
                                        ))
                                        .child(self.workspace_tooltip_icon_button(
                                            LucideIcon::X,
                                            14.0,
                                            rgb(theme.text_muted),
                                            oxideterm_gpui_ui::button::IconButtonOptions {
                                                size: 24.0,
                                                has_background: true,
                                                background: Some(rgb(theme.bg_hover)),
                                                hover_background: Some(rgb(theme.bg_panel)),
                                                idle_opacity: 1.0,
                                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                                    24.0,
                                                )
                                            },
                                            self.i18n.t("sidebar.host_docker.logs.close"),
                                            "host-docker-logs-close",
                                            true,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.connection_monitor.host_docker_logs_dialog =
                                                    None;
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                            cx.entity(),
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .id("host-docker-logs-scroll")
                                .flex_1()
                                .min_h_0()
                                .max_h(px(HOST_DOCKER_LOGS_DIALOG_MAX_HEIGHT - 84.0))
                                .overflow_y_scroll()
                                // Long log lines should scroll sideways instead
                                // of being clipped by the modal boundary.
                                .overflow_x_scrollbar()
                                .child(content),
                        ),
                ))
                .into_any_element(),
        )
    }

    pub(super) fn render_host_service_confirm_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let request = self
            .connection_monitor
            .host_service_pending_confirm
            .as_ref()?;
        let title = self.i18n.t("sidebar.host_services.confirm.title");
        let description = self.i18n_replace(
            host_service_confirm_description_key(&request.action),
            &[
                ("name", request.description.clone()),
                ("id", request.service_id.clone()),
            ],
        );
        Some(confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: if matches!(
                    request.action,
                    ServiceActionKind::Stop
                        | ServiceActionKind::Restart
                        | ServiceActionKind::Disable
                ) {
                    ConfirmDialogVariant::Danger
                } else {
                    ConfirmDialogVariant::Default
                },
                title: div().child(title).into_any_element(),
                description: Some(div().child(description).into_any_element()),
                cancel_label: div()
                    .child(self.i18n.t("sidebar.host_services.confirm.cancel"))
                    .into_any_element(),
                confirm_label: div()
                    .child(self.i18n.t(host_service_confirm_label_key(&request.action)))
                    .into_any_element(),
            },
            self.standard_confirm_focus(),
            cx.listener(|this, _event, _window, cx| {
                this.connection_monitor.host_service_pending_confirm = None;
                this.clear_standard_confirm_focus();
                cx.notify();
            }),
            cx.listener(|this, _event, _window, cx| {
                this.confirm_host_service_action(cx);
            }),
        )
        .into_any_element())
    }

    pub(super) fn render_host_service_logs_dialog(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let dialog = self.connection_monitor.host_service_logs_dialog.as_ref()?;
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let follow_connection_id = dialog.request.connection_id.clone();
        let follow_service_id = dialog.request.service_id.clone();
        let follow_description = dialog.request.description.clone();
        let follow_logs_disabled = self
            .host_service_follow_logs_command(&follow_connection_id, &follow_service_id)
            .is_err()
            || self
                .node_router
                .node_id_for_connection(&follow_connection_id)
                .is_none();
        let content = if dialog.loading {
            div()
                .p_4()
                .text_color(rgb(theme.text_muted))
                .child(self.i18n.t("sidebar.host_services.logs.loading"))
                .into_any_element()
        } else if let Some(error) = dialog.error.as_ref() {
            div()
                .p_4()
                .text_color(rgb(MONITOR_RED))
                .child(error.clone())
                .into_any_element()
        } else {
            let output = dialog.output.clone().unwrap_or_default();
            // Service logs keep their original line shape, so the dialog body
            // owns horizontal overflow just like Docker logs.
            let mut lines = div()
                .p_3()
                .flex()
                .flex_col()
                .gap(px(1.0))
                .font_family(mono_font)
                .text_size(px(11.0))
                .text_color(rgb(theme.text));
            for (index, line) in output.lines().enumerate() {
                let line = if line.is_empty() {
                    " ".to_string()
                } else {
                    line.to_string()
                };
                lines = lines.child(
                    div()
                        .id(("host-service-log-line", index))
                        .flex_none()
                        .whitespace_nowrap()
                        .child(line),
                );
            }
            lines.into_any_element()
        };

        Some(
            oxideterm_gpui_ui::modal::dismissible_dialog_backdrop()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event, _window, cx| {
                        this.connection_monitor.host_service_logs_dialog = None;
                        cx.stop_propagation();
                        cx.notify();
                    }),
                )
                .child(oxideterm_gpui_ui::modal::overlay_content_boundary(
                    oxideterm_gpui_ui::modal::dialog_content(&self.tokens)
                        .w(px(HOST_SERVICE_LOGS_DIALOG_WIDTH))
                        .max_h(px(HOST_SERVICE_LOGS_DIALOG_MAX_HEIGHT))
                        .child(
                            div()
                                .flex_none()
                                .px_4()
                                .py_3()
                                .border_b_1()
                                .border_color(rgb(theme.border))
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap_3()
                                .child(
                                    div()
                                        .min_w_0()
                                        .flex()
                                        .flex_col()
                                        .gap_1()
                                        .child(
                                            div()
                                                .text_size(px(14.0))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .text_color(rgb(theme.text))
                                                .child(self.i18n_replace(
                                                    "sidebar.host_services.logs.title",
                                                    &[("name", dialog.request.service_id.clone())],
                                                )),
                                        )
                                        .child(
                                            div()
                                                .truncate()
                                                .text_size(px(11.0))
                                                .text_color(rgb(theme.text_muted))
                                                .child(dialog.request.description.clone()),
                                        ),
                                )
                                .child(
                                    div()
                                        .flex_none()
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .child(self.workspace_tooltip_icon_button(
                                            LucideIcon::Activity,
                                            14.0,
                                            rgb(theme.text),
                                            oxideterm_gpui_ui::button::IconButtonOptions {
                                                size: 24.0,
                                                disabled: follow_logs_disabled,
                                                has_background: true,
                                                background: Some(rgb(theme.bg_hover)),
                                                hover_background: Some(rgb(theme.bg_panel)),
                                                idle_opacity: 1.0,
                                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                                    24.0,
                                                )
                                            },
                                            self.i18n
                                                .t("sidebar.host_services.actions.follow_logs"),
                                            "host-service-logs-follow",
                                            true,
                                            cx.listener({
                                                let connection_id = follow_connection_id.clone();
                                                let service_id = follow_service_id.clone();
                                                let description = follow_description.clone();
                                                move |this, _event, window, cx| {
                                                    this.connection_monitor.host_service_logs_dialog =
                                                        None;
                                                    this.open_host_service_follow_logs_terminal(
                                                        connection_id.clone(),
                                                        service_id.clone(),
                                                        description.clone(),
                                                        window,
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }
                                            }),
                                            cx.entity(),
                                        ))
                                        .child(self.workspace_tooltip_icon_button(
                                            LucideIcon::X,
                                            14.0,
                                            rgb(theme.text_muted),
                                            oxideterm_gpui_ui::button::IconButtonOptions {
                                                size: 24.0,
                                                has_background: true,
                                                background: Some(rgb(theme.bg_hover)),
                                                hover_background: Some(rgb(theme.bg_panel)),
                                                idle_opacity: 1.0,
                                                ..oxideterm_gpui_ui::button::IconButtonOptions::compact(
                                                    24.0,
                                                )
                                            },
                                            self.i18n.t("sidebar.host_services.logs.close"),
                                            "host-service-logs-close",
                                            true,
                                            cx.listener(|this, _event, _window, cx| {
                                                this.connection_monitor.host_service_logs_dialog =
                                                    None;
                                                cx.stop_propagation();
                                                cx.notify();
                                            }),
                                            cx.entity(),
                                        )),
                                ),
                        )
                        .child(
                            div()
                                .id("host-service-logs-scroll")
                                .flex_1()
                                .min_h_0()
                                .max_h(px(HOST_SERVICE_LOGS_DIALOG_MAX_HEIGHT - 84.0))
                                .overflow_y_scroll()
                                .overflow_x_scrollbar()
                                .child(content),
                        ),
                ))
                .into_any_element(),
        )
    }

    fn render_system_health_panel(&self, compact: bool, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return div()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .py_8()
                .px_4()
                .text_align(gpui::TextAlign::Center)
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(div().mb_2().opacity(0.3).child(Self::render_lucide_icon(
                    LucideIcon::WifiOff,
                    32.0,
                    rgb(self.tokens.ui.text_muted),
                )))
                .child(
                    div()
                        .text_size(px(14.0))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "system-health-empty",
                            "no-connection",
                            self.i18n.t("profiler.panel.no_connection"),
                            self.tokens.ui.text_muted,
                            cx,
                        )),
                )
                .into_any_element();
        }

        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let active_connection = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .unwrap_or(&connections[0]);
        let snapshot = (!compact).then(|| {
            self.connection_monitor
                .profiler_registry
                .snapshot(&active_connection.connection_id)
        }).flatten();
        let current = compact.then(|| {
            self.connection_monitor
                .profiler_registry
                .current(&active_connection.connection_id)
        }).flatten();
        let disabled = self
            .connection_monitor
            .disabled_profiler_connections
            .contains(&active_connection.connection_id);
        let profiler_state = if compact {
            current.as_ref().map(|(_, state)| *state)
        } else {
            snapshot.as_ref().map(|snapshot| snapshot.state)
        };
        let is_running = matches!(profiler_state, Some(ProfilerState::Running));
        let metrics = if compact {
            current.as_ref().and_then(|(metrics, _)| metrics.as_ref())
        } else {
            snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.metrics.as_ref())
        };
        let show_history = !compact;
        let history = if show_history {
            snapshot
                .as_ref()
                .map(|snapshot| {
                    snapshot
                        .history
                        .iter()
                        .rev()
                        .take(MONITOR_SPARKLINE_POINTS)
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let panel = div()
            .relative()
            .flex()
            .flex_col()
            .gap_2()
            .when(compact, |panel| panel.flex_1().min_h_0())
            .child(self.render_monitor_panel_header(
                &connections,
                active_connection,
                selected_id,
                is_running,
                !disabled,
                cx,
            ));

        if disabled || (!is_running && metrics.is_none()) {
            return panel
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py_8()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(div().mb_3().opacity(0.2).child(Self::render_lucide_icon(
                            LucideIcon::Power,
                            32.0,
                            rgb(self.tokens.ui.text_muted),
                        )))
                        .child(
                            div()
                                .mb_3()
                                .text_size(px(14.0))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "system-health-profiler",
                                    "disabled",
                                    self.i18n.t("profiler.panel.disabled"),
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
                        )
                        .child(
                            div()
                                .px_3()
                                .py_1()
                                .rounded(px(self.tokens.radii.md))
                                .border_1()
                                .border_color(rgba(
                                    (self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA,
                                ))
                                .text_size(px(12.0))
                                .cursor_pointer()
                                .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
                                // Profiler enable is a button label; keep it outside selection ownership.
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::NonSelectable,
                                    "system-health-profiler",
                                    "enable",
                                    self.i18n.t("profiler.panel.enable"),
                                    self.tokens.ui.text_muted,
                                    cx,
                                ))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener({
                                        let connection_id = active_connection.connection_id.clone();
                                        move |this, _event, _window, cx| {
                                            this.start_connection_monitor_profiler(
                                                connection_id.clone(),
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }
                                    }),
                                ),
                        ),
                )
                .into_any_element();
        }

        if metrics.is_none() && is_running {
            return panel
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py_6()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(div().mb_2().opacity(0.5).child(Self::render_lucide_icon(
                            LucideIcon::Activity,
                            20.0,
                            rgb(self.tokens.ui.text_muted),
                        )))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "system-health-profiler",
                                    "sampling",
                                    self.i18n.t("profiler.panel.sampling"),
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
                        ),
                )
                .into_any_element();
        }

        let Some(metrics) = metrics else {
            return panel
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .py_6()
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(
                            div()
                                .opacity(0.6)
                                .text_size(px(12.0))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "system-health-profiler",
                                    "no-data",
                                    self.i18n.t("profiler.panel.no_data"),
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
                        ),
                )
                .into_any_element();
        };

        let is_rtt_only = resource_metrics_is_rtt_only(metrics);
        let can_retry_sampling = !disabled
            && (matches!(profiler_state, Some(ProfilerState::Degraded))
                || matches!(metrics.source, MetricsSource::Unsupported));
        if compact {
            return panel
                .child(
                    div()
                        .id("host-tools-monitor-metrics-scroll")
                        .flex_1()
                        .min_h_0()
                        .child(self.render_compact_system_health_metrics(
                            metrics,
                            can_retry_sampling,
                            active_connection.connection_id.clone(),
                            cx,
                        )),
                )
                .into_any_element();
        }

        let mut metric_body = div().flex().flex_col().gap_2();
        if !is_rtt_only && let Some(cpu) = metrics.cpu_percent {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.cpu"),
                format!("{cpu:.1}%"),
                LucideIcon::Cpu,
                threshold_color(Some(cpu)),
                Some(cpu as f32),
                Self::metric_history(show_history, &history, |metric| metric.cpu_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only && metrics.memory_used.is_some() && metrics.memory_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.memory"),
                format!(
                    "{} / {}",
                    format_bytes(metrics.memory_used.unwrap_or_default()),
                    format_bytes(metrics.memory_total.unwrap_or_default())
                ),
                LucideIcon::MemoryStick,
                threshold_color(metrics.memory_percent),
                metrics.memory_percent.map(|value| value as f32),
                Self::metric_history(show_history, &history, |metric| metric.memory_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only && metrics.swap_used.is_some() && metrics.swap_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.swap"),
                format!(
                    "{} / {}",
                    format_bytes(metrics.swap_used.unwrap_or_default()),
                    format_bytes(metrics.swap_total.unwrap_or_default())
                ),
                LucideIcon::MemoryStick,
                threshold_color(metrics.swap_percent),
                metrics.swap_percent.map(|value| value as f32),
                Self::metric_history(show_history, &history, |metric| metric.swap_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only && metrics.disk_used.is_some() && metrics.disk_total.is_some() {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.disk"),
                format!(
                    "{} / {}",
                    format_bytes(metrics.disk_used.unwrap_or_default()),
                    format_bytes(metrics.disk_total.unwrap_or_default())
                ),
                LucideIcon::HardDrive,
                threshold_color(metrics.disk_percent),
                metrics.disk_percent.map(|value| value as f32),
                Self::metric_history(show_history, &history, |metric| metric.disk_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only && let Some(gpu_utilization) = gpu_utilization_percent(metrics) {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.gpu"),
                format!("{gpu_utilization:.1}%"),
                LucideIcon::Cpu,
                threshold_color(Some(gpu_utilization)),
                Some(gpu_utilization as f32),
                Self::metric_history(show_history, &history, gpu_utilization_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only && let Some(gpu_memory) = gpu_memory_summary(metrics) {
            metric_body = metric_body.child(self.render_metric_card(
                self.i18n.t("profiler.panel.gpu_memory"),
                format!(
                    "{} / {}",
                    format_bytes(gpu_memory.used),
                    format_bytes(gpu_memory.total)
                ),
                LucideIcon::MemoryStick,
                threshold_color(gpu_memory.percent),
                gpu_memory.percent.map(|value| value as f32),
                Self::metric_history(show_history, &history, gpu_memory_percent),
                !compact,
                cx,
            ));
        }
        if !is_rtt_only
            && (metrics.net_rx_bytes_per_sec.is_some() || metrics.net_tx_bytes_per_sec.is_some())
        {
            metric_body = metric_body.child(self.render_network_metric_card(metrics, !compact, cx));
        }
        if !is_rtt_only && !metrics.gpus.is_empty() {
            metric_body = metric_body.child(self.render_gpu_list_card(metrics, !compact, cx));
        }
        if !is_rtt_only && !metrics.disks.is_empty() {
            metric_body = metric_body.child(self.render_disk_list_card(metrics, !compact, cx));
        }
        if !is_rtt_only && !metrics.net_interfaces.is_empty() {
            metric_body = metric_body.child(self.render_interface_list_card(metrics, !compact, cx));
        }
        if !is_rtt_only && !metrics.top_processes.is_empty() {
            metric_body =
                metric_body.child(self.render_top_process_list_card(metrics, !compact, cx));
        }

        let metric_body = metric_body
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap_2()
                    .when(!is_rtt_only && metrics.load_avg_1.is_some(), |row| {
                        row.child(self.render_compact_metric_box(
                            LucideIcon::Gauge,
                            self.i18n.t("profiler.panel.load_avg"),
                            format!(
                                "{:.2} / {:.2} / {:.2}",
                                metrics.load_avg_1.unwrap_or_default(),
                                metrics.load_avg_5.unwrap_or_default(),
                                metrics.load_avg_15.unwrap_or_default()
                            ),
                            self.tokens.ui.text,
                            !compact,
                            cx,
                        ))
                    })
                    .child(
                        self.render_compact_metric_box(
                            LucideIcon::Activity,
                            self.i18n.t("profiler.panel.rtt"),
                            metrics
                                .ssh_rtt_ms
                                .map(|rtt| format!("{rtt} ms"))
                            .unwrap_or_else(|| "—".to_string()),
                            rtt_color(metrics.ssh_rtt_ms),
                            !compact,
                            cx,
                        ),
                    ),
            )
            .when(can_retry_sampling, |panel| {
                panel.child(self.render_retry_sampling_button(
                    active_connection.connection_id.clone(),
                    cx,
                ))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_1()
                    .pt_1()
                    .text_size(px(10.0))
                    .text_color(rgba(
                        (self.tokens.ui.text_muted << 8) | MONITOR_SOURCE_ALPHA,
                    ))
                    .child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .child(self.render_monitor_text(
                                !compact,
                                "monitor-metric-source-label",
                                "profiler.panel.source",
                                self.i18n.t("profiler.panel.source"),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .font_family("monospace")
                            .child(self.render_monitor_text(
                                !compact,
                                "monitor-metric-source",
                                (),
                                self.i18n.t(metrics_source_label_key(metrics.source)),
                                self.tokens.ui.text_muted,
                                cx,
                            )),
                    ),
            );

        panel.child(metric_body).into_any_element()
    }

    fn render_connection_switcher_row(
        &self,
        connections: &[MonitorConnectionOption],
        selected_id: &str,
        is_running: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(connection) = connections
            .iter()
            .find(|connection| connection.connection_id == selected_id)
            .or_else(|| connections.first())
        else {
            return div().into_any_element();
        };

        let theme = self.tokens.ui;
        let selected_index = monitor_connection_selected_index(connections, selected_id);
        let can_switch = monitor_connection_can_switch(connections);
        let focus_visible = browser_behavior::browser_focus_visible(
            self.connection_monitor.selector_focus_origin.is_some(),
            self.connection_monitor.selector_focus_origin,
        );
        // This is a host identity row first and a selector only when multiple
        // live hosts exist. Keeping it visually inline avoids the old form-field
        // dropdown sitting between the tabs and each Host Tools page.
        let selector_bottom_margin = if can_switch && self.connection_monitor.selector_open {
            let visible_options = connections
                .len()
                .max(1)
                .min(SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS) as f32;
            SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y
                + (visible_options * SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT)
                + (SYSTEM_HEALTH_SELECTOR_GAP * 2.0)
        } else {
            0.0
        };
        let mut trigger = div()
            .h(px(HOST_TOOLS_CONNECTION_ROW_HEIGHT))
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .px_1()
            .rounded(px(self.tokens.radii.md))
            .when(can_switch, |row| row.cursor_pointer())
            .when(can_switch && (self.connection_monitor.selector_open || focus_visible), |row| {
                row.bg(rgba((theme.bg_panel << 8) | MONITOR_TINT_ALPHA))
            })
            .when(can_switch, |row| {
                row.hover(|hovered| hovered.bg(rgba((theme.bg_panel << 8) | MONITOR_TINT_ALPHA)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _event, _window, cx| {
                            this.connection_monitor.selector_focus_origin =
                                Some(browser_behavior::BrowserFocusOrigin::Pointer);
                            if this.connection_monitor.selector_open {
                                this.connection_monitor.selector_open = false;
                                this.connection_monitor.selector_highlighted_index = None;
                            } else {
                                let connections = this.monitor_connections();
                                let selected_id = this
                                    .connection_monitor
                                    .selected_connection_id
                                    .as_deref()
                                    .unwrap_or_else(|| {
                                        connections
                                            .first()
                                            .map(|connection| connection.connection_id.as_str())
                                            .unwrap_or_default()
                                    });
                                this.connection_monitor.selector_highlighted_index = Some(
                                    monitor_connection_selected_index(&connections, selected_id),
                                );
                                this.connection_monitor.selector_open = true;
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }),
                    )
            })
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                14.0,
                if is_running {
                    rgb(MONITOR_EMERALD)
                } else {
                    rgb(theme.text_muted)
                },
            ))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .whitespace_nowrap()
                    .text_size(px(13.0))
                    .font_family("monospace")
                    .text_color(rgb(theme.text))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "host-tools-connection-endpoint",
                        connection.connection_id.as_str(),
                        monitor_connection_label(connection),
                        theme.text,
                        cx,
                    )),
            );
        if can_switch {
            trigger = trigger.child(
                div()
                    .flex_none()
                    .opacity(0.75)
                    .child(Self::render_lucide_icon(
                        LucideIcon::ChevronDown,
                        14.0,
                        rgb(theme.text_muted),
                    )),
            );
        }

        let mut wrapper = div()
            .relative()
            .mb(px(selector_bottom_margin))
            .child(trigger);
        if can_switch && self.connection_monitor.selector_open {
            let highlighted = self
                .connection_monitor
                .selector_highlighted_index
                .unwrap_or(selected_index);
            let mut popup = select_event_boundary(
                div()
                    .absolute()
                    .top(px(HOST_TOOLS_CONNECTION_ROW_HEIGHT + SYSTEM_HEALTH_SELECTOR_GAP))
                    .left_0()
                    .right_0()
                    .overflow_hidden()
                    .max_h(px(
                        SYSTEM_HEALTH_SELECTOR_MENU_PADDING_Y
                            + (SYSTEM_HEALTH_SELECTOR_VISIBLE_OPTIONS as f32
                                * SYSTEM_HEALTH_SELECTOR_OPTION_HEIGHT),
                    ))
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgb(self.tokens.ui.border))
                    .bg(rgb(self.tokens.ui.bg_panel))
                    .p_1()
                    .shadow_lg(),
            );
            for (index, connection) in connections.iter().enumerate() {
                let connection_id = connection.connection_id.clone();
                let selected = connection.connection_id == selected_id;
                let highlighted = highlighted == index;
                popup = popup.child(
                    select_option_action(
                        select_option_highlighted(
                            &self.tokens,
                            monitor_connection_label(connection),
                            selected,
                            highlighted,
                        )
                            .font_family("monospace")
                            .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                                if this.connection_monitor.selector_highlighted_index
                                    != Some(index)
                                {
                                    this.connection_monitor.selector_highlighted_index =
                                        Some(index);
                                    cx.notify();
                                }
                            }))
                            .child(div().mr_2().child(Self::render_lucide_icon(
                                LucideIcon::Server,
                                14.0,
                                rgb(self.tokens.ui.text_muted),
                            ))),
                        false,
                        false,
                        cx.listener(move |this, _event, _window, cx| {
                            this.connection_monitor.selected_connection_id =
                                Some(connection_id.clone());
                            this.connection_monitor.selector_open = false;
                            this.connection_monitor.selector_highlighted_index = None;
                            this.connection_monitor.selector_focus_origin = None;
                            this.sync_connection_monitor_selection(cx);
                            cx.stop_propagation();
                        }),
                    ),
                );
            }
            wrapper = wrapper.child(popup);
        }
        wrapper.into_any_element()
    }

    pub(super) fn handle_connection_monitor_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let connections = self.monitor_connections();
        if !monitor_connection_can_switch(&connections) {
            self.connection_monitor.selector_open = false;
            self.connection_monitor.selector_highlighted_index = None;
            self.connection_monitor.selector_focus_origin = None;
            return false;
        }
        let selected_id = self
            .connection_monitor
            .selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let selected_index = monitor_connection_selected_index(&connections, selected_id);
        let current = self
            .connection_monitor
            .selector_highlighted_index
            .unwrap_or(selected_index);

        if self.connection_monitor.selector_open {
            return self.handle_open_connection_monitor_select_key(event, &connections, current, cx);
        }

        match event.keystroke.key.as_str() {
            "tab" => {
                // Tauri/Radix exposes the select trigger as a keyboard tab stop.
                // Native has no DOM focus chain, so the monitor page owns that
                // first trigger focus explicitly.
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "enter" | "space" | " " | "arrowdown" | "down"
                if self.connection_monitor.selector_focus_origin.is_some() =>
            {
                self.connection_monitor.selector_open = true;
                self.connection_monitor.selector_highlighted_index = Some(selected_index);
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "escape" if self.connection_monitor.selector_focus_origin.is_some() => {
                self.connection_monitor.selector_focus_origin = None;
                self.connection_monitor.selector_highlighted_index = None;
                cx.notify();
                true
            }
            _ => false,
        }
    }

    fn handle_open_connection_monitor_select_key(
        &mut self,
        event: &KeyDownEvent,
        connections: &[MonitorConnectionOption],
        current: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                self.connection_monitor.selector_open = false;
                self.connection_monitor.selector_highlighted_index = None;
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "tab" => {
                self.connection_monitor.selector_open = false;
                self.connection_monitor.selector_highlighted_index = None;
                self.connection_monitor.selector_focus_origin = None;
                cx.notify();
                true
            }
            "arrowdown" | "down" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(browser_behavior::browser_select_next_index(
                        current,
                        connections.len(),
                        browser_behavior::BrowserSelectKeyDirection::Next,
                    ));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "arrowup" | "up" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(browser_behavior::browser_select_next_index(
                        current,
                        connections.len(),
                        browser_behavior::BrowserSelectKeyDirection::Previous,
                    ));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "home" => {
                self.connection_monitor.selector_highlighted_index = Some(0);
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "end" => {
                self.connection_monitor.selector_highlighted_index =
                    Some(connections.len().saturating_sub(1));
                self.connection_monitor.selector_focus_origin =
                    Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                cx.notify();
                true
            }
            "enter" | "space" | " " => {
                if let Some(connection) = connections.get(current.min(connections.len() - 1)) {
                    self.connection_monitor.selected_connection_id =
                        Some(connection.connection_id.clone());
                    self.connection_monitor.selector_open = false;
                    self.connection_monitor.selector_highlighted_index = None;
                    self.connection_monitor.selector_focus_origin =
                        Some(browser_behavior::BrowserFocusOrigin::Keyboard);
                    self.sync_connection_monitor_selection(cx);
                }
                true
            }
            _ => false,
        }
    }

    fn render_monitor_panel_header(
        &self,
        connections: &[MonitorConnectionOption],
        connection: &MonitorConnectionOption,
        selected_id: &str,
        is_running: bool,
        is_enabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .min_h(px(HOST_TOOLS_CONNECTION_ROW_HEIGHT))
            .w_full()
            .min_w_0()
            .flex()
            .items_start()
            .gap_2()
            .px_1()
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .child(self.render_connection_switcher_row(
                        connections,
                        selected_id,
                        is_running,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex_none()
                    .p_1()
                    .rounded(px(self.tokens.radii.md))
                    .cursor_pointer()
                    .text_color(if is_enabled {
                        rgb(MONITOR_EMERALD)
                    } else {
                        rgb(theme.text_muted)
                    })
                    .hover(|button| {
                        if is_enabled {
                            button
                                .text_color(rgb(MONITOR_RED))
                                .bg(rgba((MONITOR_RED << 8) | MONITOR_TINT_ALPHA))
                        } else {
                            button
                                .text_color(rgb(MONITOR_EMERALD))
                                .bg(rgba((MONITOR_EMERALD_DARK << 8) | MONITOR_TINT_ALPHA))
                        }
                    })
                    .child(Self::render_lucide_icon(
                        LucideIcon::Power,
                        14.0,
                        if is_enabled {
                            rgb(MONITOR_EMERALD)
                        } else {
                            rgb(theme.text_muted)
                        },
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener({
                            let connection_id = connection.connection_id.clone();
                            move |this, _event, _window, cx| {
                                let profiler_state = this
                                    .connection_monitor
                                    .profiler_registry
                                    .state(&connection_id);
                                if this
                                    .connection_monitor
                                    .disabled_profiler_connections
                                    .contains(&connection_id)
                                    || !matches!(profiler_state, Some(ProfilerState::Running))
                                {
                                    this.start_connection_monitor_profiler(
                                        connection_id.clone(),
                                        cx,
                                    );
                                } else {
                                    this.stop_connection_monitor_profiler(
                                        connection_id.clone(),
                                        cx,
                                    );
                                }
                                cx.stop_propagation();
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .w_2()
                    .h_2()
                    .rounded_full()
                    .bg(rgb(if is_running {
                        MONITOR_EMERALD_DARK
                    } else {
                        theme.text_muted
                    }))
                    .opacity(if is_running { 1.0 } else { 0.5 }),
            )
            .into_any_element()
    }

    fn render_retry_sampling_button(
        &self,
        connection_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .px_3()
            .py_1()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
            .text_size(px(12.0))
            .text_color(rgb(self.tokens.ui.text_muted))
            .cursor_pointer()
            .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "system-health-profiler",
                "retry",
                self.i18n.t("profiler.panel.retry"),
                self.tokens.ui.text_muted,
                cx,
            ))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.start_connection_monitor_profiler(connection_id.clone(), cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_compact_system_health_metrics(
        &self,
        metrics: &ResourceMetrics,
        can_retry_sampling: bool,
        connection_id: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = Arc::new(compact_monitor_rows(
            metrics,
            can_retry_sampling.then_some(connection_id),
        ));
        self.sync_compact_monitor_list_state(&rows);
        let state = self.connection_monitor.compact_monitor_list_state.clone();
        let spec = self.compact_monitor_list_spec();
        let workspace = cx.entity();

        div()
            .size_full()
            .child(tauri_virtual_list(state, spec, move |index, _window, cx| {
                let rows = rows.clone();
                workspace.update(cx, |this, cx| {
                    this.render_compact_monitor_virtual_row(rows.get(index).cloned(), cx)
                })
            }))
            .into_any_element()
    }

    fn sync_compact_monitor_list_state(&self, rows: &[CompactMonitorRow]) {
        let signatures = rows
            .iter()
            .map(compact_monitor_row_signature)
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.compact_monitor_list_state,
            &mut self
                .connection_monitor
                .compact_monitor_list_cache
                .borrow_mut(),
            "host-tools-monitor-compact",
            &signatures,
            self.compact_monitor_list_spec(),
        );
    }

    fn compact_monitor_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(COMPACT_MONITOR_LIST_ESTIMATED_ROW_HEIGHT),
            COMPACT_MONITOR_LIST_OVERSCAN,
        )
    }

    fn render_compact_monitor_virtual_row(
        &self,
        row: Option<CompactMonitorRow>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(row) = row else {
            return div().into_any_element();
        };
        match row {
            CompactMonitorRow::Metric {
                kind,
                value,
                level,
            } => {
                let value = if kind == MonitorMetricKind::Source {
                    self.i18n.t(&value)
                } else {
                    value
                };
                self.render_compact_monitor_metric_row(
                    monitor_metric_icon(kind),
                    self.compact_monitor_metric_label(kind),
                    value,
                    self.monitor_level_color(level),
                )
            }
            CompactMonitorRow::Network { rx, tx } => {
                self.render_compact_monitor_network_row(rx, tx)
            }
            CompactMonitorRow::Section { kind } => self.render_compact_monitor_section_row(
                monitor_section_icon(kind),
                self.i18n.t(monitor_section_label_key(kind)),
            ),
            CompactMonitorRow::Detail {
                name,
                value,
                level,
            } => self.render_compact_monitor_detail_row(
                name,
                value,
                self.monitor_level_color(level),
            ),
            CompactMonitorRow::Retry { connection_id } => div()
                .w_full()
                .h(px(COMPACT_MONITOR_RETRY_ROW_HEIGHT))
                .flex()
                .items_center()
                .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
                .child(self.render_retry_sampling_button(connection_id, cx))
                .into_any_element(),
        }
    }

    fn render_compact_monitor_metric_row(
        &self,
        icon: LucideIcon,
        label: String,
        value: String,
        value_color: u32,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Compact metric rows stay flat so labels keep room in the narrow
        // companion panel while the GPUI List owns the hot scroll surface.
        div()
            .w_full()
            .h(px(COMPACT_MONITOR_METRIC_ROW_HEIGHT))
            .min_w_0()
            .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .text_size(px(12.0))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 13.0, rgb(theme.text_muted)))
                    .child(div().min_w_0().truncate().child(label)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(relative(COMPACT_MONITOR_VALUE_MAX_WIDTH_RATIO))
                    .truncate()
                    .font_family("monospace")
                    .text_align(gpui::TextAlign::Right)
                    .text_color(rgb(value_color))
                    .child(value),
            )
            .into_any_element()
    }

    fn compact_monitor_metric_label(&self, kind: MonitorMetricKind) -> String {
        match kind {
            MonitorMetricKind::Source => self.i18n.t("profiler.panel.source"),
            _ => self.i18n.t(monitor_metric_label_key(kind)),
        }
    }

    fn monitor_level_color(&self, level: MonitorValueLevel) -> u32 {
        monitor_value_level_color(level, self.tokens.ui.text_muted)
    }

    fn render_compact_monitor_network_row(&self, rx: String, tx: String) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h(px(COMPACT_MONITOR_METRIC_ROW_HEIGHT))
            .min_w_0()
            .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .text_size(px(12.0))
            .child(
                div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Wifi,
                        13.0,
                        rgb(theme.text_muted),
                    ))
                    .child(div().min_w_0().truncate().child(self.i18n.t("profiler.panel.network"))),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(relative(COMPACT_MONITOR_VALUE_MAX_WIDTH_RATIO))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(8.0))
                    .font_family("monospace")
                    .child(
                        div()
                            .flex_none()
                            .truncate()
                            .text_color(rgb(MONITOR_EMERALD))
                            .child(format!("↓ {rx}")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .truncate()
                            .text_color(rgb(MONITOR_AMBER))
                            .child(format!("↑ {tx}")),
                    ),
            )
            .into_any_element()
    }

    fn render_compact_monitor_section_row(&self, icon: LucideIcon, label: String) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .w_full()
            .h(px(COMPACT_MONITOR_SECTION_ROW_HEIGHT))
            .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
            .flex()
            .items_center()
            .gap(px(6.0))
            .min_w_0()
            .text_size(px(12.0))
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(icon, 13.0, rgb(theme.text_muted)))
            .child(div().min_w_0().truncate().child(label))
            .into_any_element()
    }

    fn render_compact_monitor_detail_row(
        &self,
        name: String,
        value: String,
        value_color: u32,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        // Detail rows are plain measured list items, not selectable dashboard
        // widgets, so scroll stays owned by the GPUI List surface.
        div()
            .w_full()
            .h(px(COMPACT_MONITOR_DETAIL_ROW_HEIGHT))
            .flex()
            .items_center()
            .min_w_0()
            .px(px(COMPACT_MONITOR_ROW_SIDE_PADDING))
            .text_size(px(11.0))
            .font_family("monospace")
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .pl(px(COMPACT_MONITOR_DETAIL_INDENT))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_color(rgb(theme.text))
                            .child(name),
                    )
                    .child(
                        div()
                            .flex_none()
                            .max_w(relative(COMPACT_MONITOR_DETAIL_VALUE_MAX_WIDTH_RATIO))
                            .truncate()
                            .text_align(gpui::TextAlign::Right)
                            .text_color(rgb(value_color))
                            .child(value),
                    ),
            )
            .into_any_element()
    }

    fn render_metric_card(
        &self,
        label: String,
        value: String,
        icon: LucideIcon,
        color: u32,
        progress_value: Option<f32>,
        history: Vec<Option<f64>>,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_size(px(12.0))
                            .text_color(rgb(theme.text_muted))
                            .child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text_muted)))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-metric-label",
                                &label,
                                label.clone(),
                                theme.text_muted,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .text_size(px(12.0))
                            .text_color(rgb(color))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-metric-value",
                                &label,
                                value,
                                color,
                                cx,
                            )),
                    ),
            )
            .child(progress(&self.tokens, progress_value, false).h(px(6.0)))
            .when(
                history.iter().filter_map(|value| *value).count() >= 2,
                |card| card.child(render_sparkline(history, color)),
            )
            .into_any_element()
    }

    fn metric_history(
        show_history: bool,
        history: &[ResourceMetrics],
        value: impl Fn(&ResourceMetrics) -> Option<f64>,
    ) -> Vec<Option<f64>> {
        // Compact sidebars avoid sparkline canvas work; full pages keep history.
        if show_history {
            history.iter().map(value).collect()
        } else {
            Vec::new()
        }
    }

    fn render_monitor_text(
        &self,
        selectable: bool,
        scope: &str,
        key: impl Hash,
        text: impl Into<String>,
        color: u32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = text.into();
        if selectable {
            self.render_selectable_text_scoped(scope, key, text, color, cx)
        } else {
            self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                scope,
                key,
                text,
                color,
                cx,
            )
        }
    }

    fn render_network_metric_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let rx_rate = format_rate(metrics.net_rx_bytes_per_sec.unwrap_or_default());
        let tx_rate = format_rate(metrics.net_tx_bytes_per_sec.unwrap_or_default());
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .p_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .mb_2()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(
                        LucideIcon::Wifi,
                        14.0,
                        rgb(theme.text_muted),
                    ))
                    .child(self.render_monitor_text(
                        selectable,
                        "system-health-section-label",
                        "network",
                        self.i18n.t("profiler.panel.network"),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .font_family("monospace")
                    .text_size(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowDown,
                                12.0,
                                rgb(MONITOR_EMERALD),
                            ))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-network-rx",
                                (),
                                rx_rate,
                                self.tokens.ui.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(Self::render_lucide_icon(
                                LucideIcon::ArrowUp,
                                12.0,
                                rgb(MONITOR_AMBER),
                            ))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-network-tx",
                                (),
                                tx_rate,
                                self.tokens.ui.text,
                                cx,
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_disk_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::HardDrive,
            self.i18n.t("profiler.panel.mounts"),
            disk_list_rows(metrics, 4),
            selectable,
            cx,
        )
    }

    fn render_interface_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::Wifi,
            self.i18n.t("profiler.panel.interfaces"),
            interface_list_rows(metrics, 4),
            selectable,
            cx,
        )
    }

    fn render_gpu_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::Cpu,
            self.i18n.t("profiler.panel.gpus"),
            gpu_list_rows(metrics, 4),
            selectable,
            cx,
        )
    }

    fn render_top_process_list_card(
        &self,
        metrics: &ResourceMetrics,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render_monitor_list_card(
            LucideIcon::Activity,
            self.i18n.t("profiler.panel.top_processes"),
            top_process_list_rows(metrics, 5),
            selectable,
            cx,
        )
    }

    fn render_monitor_list_card(
        &self,
        icon: LucideIcon,
        label: String,
        rows: Vec<MonitorListRow>,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut card = div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .p_3()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .min_w(px(0.0))
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text_muted)))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .truncate()
                            .whitespace_nowrap()
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-list-label",
                                &label,
                                label.clone(),
                                theme.text_muted,
                                cx,
                            )),
                    ),
            );
        for (index, row) in rows.into_iter().enumerate() {
            let value_color = self.monitor_level_color(row.level);
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .min_w(px(0.0))
                    .text_size(px(11.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .truncate()
                            .whitespace_nowrap()
                            .font_family("monospace")
                            .text_color(rgb(theme.text))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-list-name",
                                (&label, index),
                                row.name,
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(180.0))
                            .truncate()
                            .whitespace_nowrap()
                            .font_family("monospace")
                            .text_color(rgb(value_color))
                            .child(self.render_monitor_text(
                                selectable,
                                "monitor-list-value",
                                (&label, index),
                                row.value,
                                value_color,
                                cx,
                            )),
                    ),
            );
        }
        card.into_any_element()
    }

    fn render_compact_metric_box(
        &self,
        icon: LucideIcon,
        label: String,
        value: String,
        value_color: u32,
        selectable: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .p_3()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .mb_1()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text_muted))
                    .child(Self::render_lucide_icon(icon, 14.0, rgb(theme.text_muted)))
                    .child(self.render_monitor_text(
                        selectable,
                        "monitor-compact-metric-label",
                        &label,
                        label.clone(),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .font_family("monospace")
                    .text_size(px(12.0))
                    .text_color(rgb(value_color))
                    .child(self.render_monitor_text(
                        selectable,
                        "monitor-compact-metric-value",
                        &label,
                        value,
                        value_color,
                        cx,
                    )),
            )
            .into_any_element()
    }
}

fn monitor_metric_icon(kind: MonitorMetricKind) -> LucideIcon {
    match kind {
        MonitorMetricKind::Cpu | MonitorMetricKind::Gpu => LucideIcon::Cpu,
        MonitorMetricKind::Memory | MonitorMetricKind::Swap | MonitorMetricKind::GpuMemory => {
            LucideIcon::MemoryStick
        }
        MonitorMetricKind::Disk => LucideIcon::HardDrive,
        MonitorMetricKind::LoadAverage => LucideIcon::Gauge,
        MonitorMetricKind::Rtt => LucideIcon::Activity,
        MonitorMetricKind::Source => LucideIcon::Info,
    }
}

fn monitor_metric_label_key(kind: MonitorMetricKind) -> &'static str {
    match kind {
        MonitorMetricKind::Cpu => "profiler.panel.cpu",
        MonitorMetricKind::Memory => "profiler.panel.memory",
        MonitorMetricKind::Swap => "profiler.panel.swap",
        MonitorMetricKind::Disk => "profiler.panel.disk",
        MonitorMetricKind::Gpu => "profiler.panel.gpu",
        MonitorMetricKind::GpuMemory => "profiler.panel.gpu_memory",
        MonitorMetricKind::LoadAverage => "profiler.panel.load_avg",
        MonitorMetricKind::Rtt => "profiler.panel.rtt",
        MonitorMetricKind::Source => "profiler.panel.source",
    }
}

fn monitor_section_icon(kind: MonitorSectionKind) -> LucideIcon {
    match kind {
        MonitorSectionKind::Mounts => LucideIcon::HardDrive,
        MonitorSectionKind::Gpus => LucideIcon::Cpu,
        MonitorSectionKind::Interfaces => LucideIcon::Wifi,
        MonitorSectionKind::TopProcesses => LucideIcon::Activity,
    }
}

fn monitor_section_label_key(kind: MonitorSectionKind) -> &'static str {
    match kind {
        MonitorSectionKind::Mounts => "profiler.panel.mounts",
        MonitorSectionKind::Gpus => "profiler.panel.gpus",
        MonitorSectionKind::Interfaces => "profiler.panel.interfaces",
        MonitorSectionKind::TopProcesses => "profiler.panel.top_processes",
    }
}

fn host_process_confirm_description_key(action: &ProcessActionKind) -> &'static str {
    match action {
        ProcessActionKind::Term => "sidebar.host_processes.confirm.term_desc",
        ProcessActionKind::Kill => "sidebar.host_processes.confirm.kill_desc",
        ProcessActionKind::Stop => "sidebar.host_processes.confirm.stop_desc",
        ProcessActionKind::Cont => "sidebar.host_processes.confirm.cont_desc",
        ProcessActionKind::Renice { .. } => "sidebar.host_processes.confirm.renice_desc",
    }
}

fn host_process_confirm_label_key(action: &ProcessActionKind) -> &'static str {
    match action {
        ProcessActionKind::Term => "sidebar.host_processes.actions.term",
        ProcessActionKind::Kill => "sidebar.host_processes.actions.kill",
        ProcessActionKind::Stop => "sidebar.host_processes.actions.stop",
        ProcessActionKind::Cont => "sidebar.host_processes.actions.cont",
        ProcessActionKind::Renice { .. } => "sidebar.host_processes.actions.apply",
    }
}

fn host_docker_confirm_description_key(action: &DockerActionKind) -> &'static str {
    match action {
        DockerActionKind::Start => "sidebar.host_docker.confirm.start_desc",
        DockerActionKind::Stop => "sidebar.host_docker.confirm.stop_desc",
        DockerActionKind::Restart => "sidebar.host_docker.confirm.restart_desc",
    }
}

fn host_docker_confirm_label_key(action: &DockerActionKind) -> &'static str {
    match action {
        DockerActionKind::Start => "sidebar.host_docker.actions.start",
        DockerActionKind::Stop => "sidebar.host_docker.actions.stop",
        DockerActionKind::Restart => "sidebar.host_docker.actions.restart",
    }
}

fn host_service_confirm_description_key(action: &ServiceActionKind) -> &'static str {
    match action {
        ServiceActionKind::Start => "sidebar.host_services.confirm.start_desc",
        ServiceActionKind::Stop => "sidebar.host_services.confirm.stop_desc",
        ServiceActionKind::Restart => "sidebar.host_services.confirm.restart_desc",
        ServiceActionKind::Reload => "sidebar.host_services.confirm.reload_desc",
        ServiceActionKind::Enable => "sidebar.host_services.confirm.enable_desc",
        ServiceActionKind::Disable => "sidebar.host_services.confirm.disable_desc",
    }
}

fn host_service_confirm_label_key(action: &ServiceActionKind) -> &'static str {
    match action {
        ServiceActionKind::Start => "sidebar.host_services.actions.start",
        ServiceActionKind::Stop => "sidebar.host_services.actions.stop",
        ServiceActionKind::Restart => "sidebar.host_services.actions.restart",
        ServiceActionKind::Reload => "sidebar.host_services.actions.reload",
        ServiceActionKind::Enable => "sidebar.host_services.actions.enable",
        ServiceActionKind::Disable => "sidebar.host_services.actions.disable",
    }
}

fn docker_state_color(state: &str, muted_color: u32) -> u32 {
    match state.trim().to_lowercase().as_str() {
        "running" => MONITOR_EMERALD,
        "created" | "paused" | "restarting" => MONITOR_AMBER,
        "dead" | "removing" => MONITOR_RED,
        "exited" => muted_color,
        _ => muted_color,
    }
}

fn service_state_color(state: &str, muted_color: u32) -> u32 {
    match state.trim().to_lowercase().as_str() {
        "active" | "running" => MONITOR_EMERALD,
        "activating" | "deactivating" | "reloading" => MONITOR_AMBER,
        "failed" => MONITOR_RED,
        _ => muted_color,
    }
}
