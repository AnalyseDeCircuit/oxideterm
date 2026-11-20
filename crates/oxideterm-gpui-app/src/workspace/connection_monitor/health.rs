use super::*;

#[path = "health/container.rs"]
mod container;
#[path = "health/docker.rs"]
mod docker;
#[path = "health/filesystems.rs"]
mod filesystems;
#[path = "health/logs.rs"]
mod logs;
#[path = "health/monitor.rs"]
mod monitor;
#[path = "health/packages.rs"]
mod packages;
#[path = "health/ports.rs"]
mod ports;
#[path = "health/process.rs"]
mod process;
#[path = "health/redis.rs"]
mod redis;
#[path = "health/scheduled_tasks.rs"]
mod scheduled_tasks;
#[path = "health/services.rs"]
mod services;
#[path = "health/tmux.rs"]
mod tmux;

impl WorkspaceApp {
    /// Host Tools attach actions need a terminal target: an SSH node consumer
    /// or, for the local machine, a local terminal tab.
    pub(super) fn host_tools_terminal_available(&self, connection_id: &str) -> bool {
        if connection_id == LOCAL_HOST_CONNECTION_ID {
            return true;
        }
        self.node_router
            .node_id_for_connection(connection_id)
            .is_some_and(|node_id| self.ssh_nodes.contains_key(&node_id))
    }

    fn host_tool_monitoring_enabled(&self, tool: ContextSidebarTool) -> bool {
        tool.monitoring_enabled(&self.settings_store.settings().host_tools)
    }

    pub(in crate::workspace) fn set_host_tool_monitoring_enabled(
        &mut self,
        tool: ContextSidebarTool,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        self.edit_settings(
            move |settings| tool.set_monitoring_enabled(&mut settings.host_tools, enabled),
            cx,
        );
        if !enabled && tool == ContextSidebarTool::Services {
            self.host_tools
                .update(cx, |host_tools, _cx| host_tools.pause_service_refreshes());
        }
        if !enabled && tool == ContextSidebarTool::Redis {
            // A disabled Redis tool owns no probe. Dropping the queued and
            // in-flight reads makes their deliveries stale, and clearing the
            // credential focus stops Enter from starting a new one.
            self.host_tools.update(cx, |host_tools, _cx| {
                host_tools.ui.clear_input_focus();
                host_tools.host_redis.snapshot_pending = None;
                host_tools.host_redis.keys_pending = None;
                host_tools.host_redis.snapshot_running = None;
                host_tools.host_redis.snapshot_in_flight = false;
                host_tools.host_redis.keys_running = None;
                host_tools.host_redis.keys_in_flight = false;
            });
        }
        if enabled && self.host_tools.read(cx).active_tool() == tool {
            self.request_host_tool_snapshot_if_needed(tool, cx);
        }
    }

    fn request_host_tool_snapshot_if_needed(
        &mut self,
        tool: ContextSidebarTool,
        cx: &mut Context<Self>,
    ) {
        if !self.host_tool_monitoring_enabled(tool) {
            // A disabled tool never probes: the toggle only starts sampling
            // for tools that are actually enabled.
            return;
        }
        if tool == ContextSidebarTool::Monitor {
            self.host_tools.update(cx, |host_tools, cx| {
                if let Some(connection_id) = host_tools.selected_connection_id_owned() {
                    host_tools.request_profiler_refresh(connection_id, cx);
                }
            });
            return;
        }
        if tool == ContextSidebarTool::Container {
            self.host_tools.update(cx, |host_tools, cx| {
                host_tools.request_container_discovery_if_needed(cx)
            });
            return;
        }
        if matches!(
            tool,
            ContextSidebarTool::Gpu | ContextSidebarTool::Processes | ContextSidebarTool::Docker
        ) {
            return;
        }
        // Snapshot ownership stays in the Entity; the root only persists the
        // monitoring toggle that led to this request.
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_active_tool_snapshot(HostSnapshotFeedback::Silent, cx);
        });
    }

    pub(in crate::workspace) fn render_host_tools_context_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active_tool = self.host_tools.read(cx).active_tool();
        let enabled = self.host_tool_monitoring_enabled(active_tool);
        let content = if enabled {
            match active_tool {
                ContextSidebarTool::Monitor => self.render_host_tools_monitor_panel(cx),
                ContextSidebarTool::Gpu => {
                    let tokens = self.tokens;
                    let i18n = &self.i18n;
                    let mono_font_family =
                        settings_mono_font_family(self.settings_store.settings());
                    let selectable_text = self.host_tools_selectable_text(cx);
                    self.host_tools.update(cx, |host_tools, cx| {
                        host_tools.render_host_gpu_panel(
                            &tokens,
                            i18n,
                            mono_font_family,
                            &selectable_text,
                            cx,
                        )
                    })
                }
                ContextSidebarTool::Processes => self.render_host_processes_panel(cx),
                ContextSidebarTool::Services => self.render_host_services_panel(cx),
                ContextSidebarTool::Logs => self.render_host_logs_panel(cx),
                ContextSidebarTool::Tmux => self.render_host_tmux_panel(cx),
                ContextSidebarTool::Docker => self.render_host_docker_panel(cx),
                ContextSidebarTool::Container => self.render_host_container_panel(cx),
                ContextSidebarTool::Ports => self.render_host_ports_panel(cx),
                ContextSidebarTool::Schedules => self.render_host_schedules_panel(cx),
                ContextSidebarTool::Filesystems => self.render_host_filesystems_panel(cx),
                ContextSidebarTool::Packages => self.render_host_packages_panel(cx),
                ContextSidebarTool::Redis => self.render_host_redis_panel(cx),
            }
        } else {
            self.render_host_tool_monitoring_disabled(active_tool, cx)
        };
        let content = oxideterm_gpui_ui::motion::fade_in(
            &self.tokens,
            SharedString::from(format!("host-tools-page-{active_tool:?}")),
            div()
                .w_full()
                .min_w_0()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(content),
            oxideterm_gpui_ui::motion::MotionDuration::Micro,
        );
        div()
            .id("host-tools-context-panel")
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_hidden()
            .bg(self.context_sidebar_content_background(theme.shell_bg()))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    // Keep tool bodies clipped to the companion-sidebar width.
                    .overflow_hidden()
                    .child(content),
            )
            .into_any_element()
    }

    fn render_host_tools_monitor_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .id("host-tools-monitor-context-panel")
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            // Host selection and sampling stay in the shared monitor panel.
            .child(self.render_host_monitor_panel(cx))
            .into_any_element()
    }

    pub(in crate::workspace) fn render_host_tool_monitoring_toggle(
        &self,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tool = self.host_tools.read(cx).active_tool();
        let enabled = self.host_tool_monitoring_enabled(tool);
        let color = if enabled {
            MONITOR_EMERALD
        } else {
            self.tokens.ui.text_muted
        };
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(LucideIcon::Power, 14.0, rgb(color)),
            oxideterm_gpui_ui::button::IconButtonOptions::opaque_toolbar(
                28.0,
                oxideterm_gpui_ui::button::ButtonRadius::Md,
            ),
        )
        .id("host-tool-monitoring-toggle")
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| {
                this.set_host_tool_monitoring_enabled(tool, !enabled, cx);
                cx.stop_propagation();
            }),
        )
        .into_any_element()
    }

    fn render_host_tool_monitoring_disabled(
        &self,
        tool: ContextSidebarTool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let color = self.tokens.ui.text_muted;
        div()
            .size_full()
            .p_4()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .text_align(gpui::TextAlign::Center)
            .text_color(rgb(color))
            .child(div().mb_2().opacity(0.3).child(Self::render_lucide_icon(
                LucideIcon::Power,
                24.0,
                rgb(color),
            )))
            .child(
                div()
                    .mb_3()
                    .text_size(px(14.0))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "host-tool-monitoring-disabled",
                        format!("{tool:?}"),
                        self.i18n.t("profiler.panel.disabled"),
                        color,
                        cx,
                    )),
            )
            .child(
                div()
                    .px_3()
                    .py_1()
                    .rounded(px(self.tokens.radii.md))
                    .border_1()
                    .border_color(rgba((self.tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .text_size(px(12.0))
                    .cursor_pointer()
                    .hover(|button| button.bg(rgb(self.tokens.ui.bg_hover)))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "host-tool-monitoring-enable",
                        format!("{tool:?}"),
                        self.i18n.t("profiler.panel.enable"),
                        color,
                        cx,
                    ))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.set_host_tool_monitoring_enabled(tool, true, cx);
                            cx.stop_propagation();
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(in crate::workspace) fn handle_connection_monitor_select_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.handle_connection_selector_key(&event.keystroke, cx)
        })
    }
}
