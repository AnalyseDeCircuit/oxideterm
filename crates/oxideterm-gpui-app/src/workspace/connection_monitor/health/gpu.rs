//! Owns the GPU / NPU Host Tool UI and its page-scoped sampling bridge.

use super::*;

use oxideterm_connection_monitor::ResourceSampler;

impl WorkspaceApp {
    pub(super) fn render_host_gpu_panel(&self, cx: &mut Context<Self>) -> AnyElement {
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
        let snapshot = self
            .connection_monitor
            .host_gpu
            .snapshot
            .as_ref()
            .filter(|_| {
                self.connection_monitor
                    .host_gpu
                    .snapshot_connection_id
                    .as_deref()
                    == Some(selected_id)
            });
        let devices = snapshot
            .map(|snapshot| snapshot.devices.clone())
            .unwrap_or_default();
        self.sync_host_gpu_list_state(&devices, snapshot, selected_id);
        let is_running = self
            .connection_monitor
            .host_gpu
            .sampling_task
            .as_ref()
            .is_some_and(|task| task.connection_id() == selected_id && !task.is_finished());

        div()
            .id("host-gpu-panel")
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
                        is_running,
                        cx,
                    ))
                    .when_some(snapshot, |header, snapshot| {
                        header.child(self.render_host_gpu_summary(snapshot, cx))
                    })
                    .child(self.render_host_gpu_status_row(
                        devices.len(),
                        selected_id.to_string(),
                        cx,
                    )),
            )
            .child(self.render_host_gpu_list(devices, snapshot.cloned(), selected_id, cx))
            .into_any_element()
    }

    fn render_host_gpu_summary(
        &self,
        snapshot: &GpuSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let summary = snapshot.summary();
        let utilization = summary
            .average_utilization_percent
            .map(|value| format!("{value:.0}%"))
            .unwrap_or_else(|| "—".to_string());
        let memory = if summary.memory_total > 0 {
            format!(
                "{} / {}",
                format_bytes(summary.memory_used),
                format_bytes(summary.memory_total)
            )
        } else {
            "—".to_string()
        };
        let temperature = summary
            .maximum_temperature_celsius
            .map(|value| format!("{value:.0} °C"))
            .unwrap_or_else(|| "—".to_string());
        let power = summary
            .power_draw_watts
            .map(|value| format!("{value:.0} W"))
            .unwrap_or_else(|| "—".to_string());

        div()
            .w_full()
            .min_w_0()
            .grid()
            .grid_cols(2)
            .gap_1()
            .child(self.render_host_gpu_summary_item(
                "sidebar.host_gpu.summary.utilization",
                utilization,
                cx,
            ))
            .child(self.render_host_gpu_summary_item("sidebar.host_gpu.summary.memory", memory, cx))
            .child(self.render_host_gpu_summary_item(
                "sidebar.host_gpu.summary.temperature",
                temperature,
                cx,
            ))
            .child(self.render_host_gpu_summary_item("sidebar.host_gpu.summary.power", power, cx))
            .into_any_element()
    }

    fn render_host_gpu_summary_item(
        &self,
        label_key: &'static str,
        value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .min_w_0()
            .px_2()
            .py_1()
            .rounded(px(self.tokens.radii.md))
            .bg(rgba((theme.bg_panel << 8) | MONITOR_TINT_ALPHA))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(rgb(theme.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::NonSelectable,
                        "host-gpu-summary-label",
                        label_key,
                        self.i18n.t(label_key),
                        theme.text_muted,
                        cx,
                    )),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.0))
                    .text_color(rgb(theme.text))
                    .child(value),
            )
            .into_any_element()
    }

    fn render_host_gpu_status_row(
        &self,
        count: usize,
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
            .child(div().min_w_0().flex_1().truncate().child(format!(
                "{} {} · {}",
                count,
                self.i18n.t("sidebar.host_gpu.count_suffix"),
                self.i18n.t("sidebar.host_gpu.refresh_interval")
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
                self.i18n.t("sidebar.host_gpu.actions.refresh"),
                "host-gpu-refresh",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.restart_host_gpu_sampling(selected_id.clone(), cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    fn render_host_gpu_list(
        &self,
        devices: Vec<GpuDevice>,
        snapshot: Option<GpuSnapshot>,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(snapshot) = snapshot else {
            return monitor_center_state(
                self,
                LucideIcon::Cpu,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_gpu.sampling"),
                cx,
            );
        };
        match &snapshot.status {
            GpuSnapshotStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Cpu,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_gpu.unavailable"),
                    cx,
                );
            }
            GpuSnapshotStatus::Unsupported => {
                return monitor_center_state(
                    self,
                    LucideIcon::Cpu,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_gpu.unsupported"),
                    cx,
                );
            }
            GpuSnapshotStatus::NoDevices => {
                return monitor_center_state(
                    self,
                    LucideIcon::Cpu,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_gpu.no_devices"),
                    cx,
                );
            }
            GpuSnapshotStatus::Error(message) => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_gpu.error", &[("error", message.clone())]),
                    cx,
                );
            }
            GpuSnapshotStatus::Unknown if devices.is_empty() => {
                return monitor_center_state(
                    self,
                    LucideIcon::Cpu,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_gpu.sampling"),
                    cx,
                );
            }
            GpuSnapshotStatus::Available | GpuSnapshotStatus::Unknown => {}
        }

        let devices = Arc::new(devices);
        let snapshot = Arc::new(snapshot);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.connection_monitor.host_gpu.list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_GPU_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_gpu_table_header())
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            let devices = devices.clone();
                            let snapshot = snapshot.clone();
                            let selected_id = selected_id.clone();
                            workspace.update(cx, |this, cx| {
                                this.render_host_gpu_row(
                                    selected_id.as_str(),
                                    devices.get(index).cloned(),
                                    snapshot.as_ref(),
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_gpu_table_header(&self) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_GPU_TABLE_HEADER_HEIGHT))
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
                    .child(self.i18n.t("sidebar.host_gpu.columns.device")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_GPU_UTILIZATION_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_gpu.columns.utilization")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_GPU_MEMORY_COLUMN_WIDTH))
                    .child(self.i18n.t("sidebar.host_gpu.columns.memory")),
            )
            .into_any_element()
    }

    fn render_host_gpu_row(
        &self,
        _connection_id: &str,
        device: Option<GpuDevice>,
        snapshot: &GpuSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(device) = device else {
            return div().into_any_element();
        };
        let expanded =
            self.connection_monitor.host_gpu.expanded_uuid.as_deref() == Some(device.uuid.as_str());
        let theme = self.tokens.ui;
        let device_uuid = device.uuid.clone();
        let device_kind = match device.provider {
            GpuProvider::Ascend | GpuProvider::Cambricon => "NPU",
            GpuProvider::Nvidia
            | GpuProvider::Amd
            | GpuProvider::Hygon
            | GpuProvider::Intel
            | GpuProvider::Mthreads => "GPU",
        };
        let utilization = percent_text(device.utilization_percent);
        let memory = match (device.memory_used, device.memory_total) {
            (Some(used), Some(total)) => {
                format!("{} / {}", format_bytes(used), format_bytes(total))
            }
            _ => percent_text(device.memory_utilization_percent),
        };
        let process_rows = snapshot.processes_for(&device).cloned().collect::<Vec<_>>();

        div()
            .w_full()
            .min_w_0()
            .border_b_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .cursor_pointer()
            .hover(|row| row.bg(rgb(theme.bg_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_gpu.expanded_uuid.as_deref()
                        == Some(device_uuid.as_str())
                    {
                        this.connection_monitor.host_gpu.expanded_uuid = None;
                    } else {
                        this.connection_monitor.host_gpu.expanded_uuid = Some(device_uuid.clone());
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(
                div()
                    .h(px(40.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_none().child(Self::render_lucide_icon(
                        if expanded {
                            LucideIcon::ChevronDown
                        } else {
                            LucideIcon::ChevronRight
                        },
                        13.0,
                        rgb(theme.text_muted),
                    )))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(12.0))
                                    .text_color(rgb(theme.text))
                                    .child(format!(
                                        "{device_kind} {} · {}",
                                        device.index, device.name
                                    )),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(device.pci_bus_id.clone()),
                            ),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_GPU_UTILIZATION_COLUMN_WIDTH))
                            .text_size(px(11.0))
                            .text_color(rgb(theme.text))
                            .child(utilization),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_GPU_MEMORY_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(rgb(theme.text))
                            .child(memory),
                    ),
            )
            .when(expanded, |row| {
                row.child(self.render_host_gpu_details(&device, &process_rows))
            })
            .into_any_element()
    }

    fn render_host_gpu_details(
        &self,
        device: &GpuDevice,
        processes: &[oxideterm_connection_monitor::GpuProcess],
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut details =
            div()
                .px_3()
                .pb_3()
                .pl(px(34.0))
                .flex()
                .flex_col()
                .gap_2()
                .text_size(px(10.0))
                .text_color(rgb(theme.text_muted))
                .child(self.render_host_gpu_detail_line(
                    "sidebar.host_gpu.details.uuid",
                    device.uuid.clone(),
                ))
                .child(self.render_host_gpu_detail_line(
                    "sidebar.host_gpu.details.driver",
                    device.driver_version.clone().unwrap_or_else(|| "—".into()),
                ))
                .child(
                    self.render_host_gpu_detail_line(
                        "sidebar.host_gpu.details.performance_state",
                        device
                            .performance_state
                            .clone()
                            .unwrap_or_else(|| "—".into()),
                    ),
                )
                .child(self.render_host_gpu_detail_line(
                    "sidebar.host_gpu.details.health",
                    device.health_status.clone().unwrap_or_else(|| "—".into()),
                ))
                .child(
                    self.render_host_gpu_detail_line(
                        "sidebar.host_gpu.details.temperature",
                        device
                            .temperature_celsius
                            .map(|value| format!("{value:.0} °C"))
                            .unwrap_or_else(|| "—".into()),
                    ),
                )
                .child(self.render_host_gpu_detail_line(
                    "sidebar.host_gpu.details.power",
                    match (device.power_draw_watts, device.power_limit_watts) {
                        (Some(draw), Some(limit)) => format!("{draw:.0} / {limit:.0} W"),
                        (Some(draw), None) => format!("{draw:.0} W"),
                        _ => "—".into(),
                    },
                ))
                .child(self.render_host_gpu_detail_line(
                    "sidebar.host_gpu.details.fan",
                    percent_text(device.fan_speed_percent),
                ))
                .child(
                    div()
                        .mt_1()
                        .text_size(px(11.0))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(self.i18n.t("sidebar.host_gpu.processes.title")),
                );
        if processes.is_empty() {
            details = details.child(self.i18n.t("sidebar.host_gpu.processes.empty"));
        } else {
            for process in processes {
                let memory = process
                    .used_memory
                    .map(format_bytes)
                    .unwrap_or_else(|| "—".into());
                details = details.child(
                    div()
                        .min_w_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_none()
                                .text_color(rgb(theme.text_muted))
                                .child(process.pid.to_string()),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .truncate()
                                .text_color(rgb(theme.text))
                                .child(process.process_name.clone()),
                        )
                        .child(div().flex_none().child(memory)),
                );
            }
        }
        details.into_any_element()
    }

    fn render_host_gpu_detail_line(&self, label_key: &'static str, value: String) -> AnyElement {
        div()
            .min_w_0()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex_none().w(px(82.0)).child(self.i18n.t(label_key)))
            .child(div().min_w_0().flex_1().truncate().child(value))
            .into_any_element()
    }

    fn sync_host_gpu_list_state(
        &self,
        devices: &[GpuDevice],
        snapshot: Option<&GpuSnapshot>,
        selected_id: &str,
    ) {
        let signatures = devices
            .iter()
            .map(|device| {
                let process_count = snapshot
                    .map(|snapshot| snapshot.processes_for(device).count())
                    .unwrap_or_default();
                let expanded = self.connection_monitor.host_gpu.expanded_uuid.as_deref()
                    == Some(device.uuid.as_str());
                gpu_device_row_signature(device, process_count, expanded)
            })
            .collect::<Vec<_>>();
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_gpu.list_state,
            &mut self.connection_monitor.host_gpu.list_cache.borrow_mut(),
            &format!("host-gpu:{selected_id}"),
            &signatures,
            TauriVirtualListSpec::new(px(HOST_GPU_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(in crate::workspace) fn sync_host_gpu_sampling(&mut self, cx: &mut Context<Self>) {
        let visible = self.context_sidebar_visible()
            && self.active_context_sidebar_panel == ContextSidebarPanel::HostTools
            && self.active_context_sidebar_tool == ContextSidebarTool::Gpu;
        if !visible {
            if let Some(task) = self.connection_monitor.host_gpu.sampling_task.take() {
                task.stop();
            }
            return;
        }

        let Some(connection_id) = self.connection_monitor.selected_connection_id.clone() else {
            return;
        };
        if self
            .connection_monitor
            .host_gpu
            .sampling_task
            .as_ref()
            .is_some_and(|task| task.connection_id() == connection_id)
        {
            return;
        }
        if let Some(task) = self.connection_monitor.host_gpu.sampling_task.take() {
            task.stop();
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            return;
        };
        let Some(os_type) = handle.remote_env().map(|environment| environment.os_type) else {
            return;
        };
        let sampler: Arc<dyn ResourceSampler> = Arc::new(handle);
        self.connection_monitor.host_gpu.snapshot_connection_id = Some(connection_id.clone());
        self.connection_monitor.host_gpu.snapshot = None;
        self.connection_monitor.host_gpu.expanded_uuid = None;
        // The task is owned by the visible GPU page. It borrows the registry
        // connection through ResourceSampler and owns only its shell channel.
        self.connection_monitor.host_gpu.sampling_task = Some(start_gpu_sampling_on(
            connection_id,
            sampler,
            os_type,
            self.connection_monitor.host_gpu.update_tx.clone(),
            self.forwarding_runtime.handle().clone(),
        ));
        cx.notify();
    }

    fn restart_host_gpu_sampling(&mut self, connection_id: String, cx: &mut Context<Self>) {
        if self
            .connection_monitor
            .host_gpu
            .sampling_task
            .as_ref()
            .is_some_and(|task| task.connection_id() == connection_id)
            && let Some(task) = self.connection_monitor.host_gpu.sampling_task.take()
        {
            task.stop();
        }
        self.connection_monitor.host_gpu.snapshot = None;
        self.sync_host_gpu_sampling(cx);
    }

    pub(in crate::workspace) fn poll_host_gpu_updates(
        &mut self,
        request_repaint: bool,
        cx: &mut Context<Self>,
    ) {
        let active_connection_id = self
            .connection_monitor
            .host_gpu
            .sampling_task
            .as_ref()
            .map(|task| task.connection_id().to_string());
        let mut received_update = false;
        while let Ok(update) = self.connection_monitor.host_gpu.update_rx.try_recv() {
            if active_connection_id.as_deref() != Some(update.connection_id.as_str()) {
                continue;
            }
            self.connection_monitor.host_gpu.snapshot_connection_id = Some(update.connection_id);
            self.connection_monitor.host_gpu.snapshot = Some(update.snapshot);
            received_update = true;
        }
        if received_update && request_repaint {
            cx.notify();
        }
    }
}

fn percent_text(value: Option<f64>) -> String {
    value
        .map(|value| format!("{value:.0}%"))
        .unwrap_or_else(|| "—".to_string())
}
