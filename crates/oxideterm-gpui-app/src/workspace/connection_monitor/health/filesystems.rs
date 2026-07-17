//! Owns the filesystems Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::{filesystem_capture_snapshot, filesystem_percent_severity};

impl WorkspaceApp {
    pub(super) fn render_host_filesystems_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::HardDrive,
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
            .host_filesystem_snapshot
            .as_ref()
            .filter(|_| {
                self.connection_monitor
                    .host_filesystem_snapshot_connection_id
                    .as_deref()
                    == Some(selected_id)
            });
        let rows = snapshot
            .map(|snapshot| {
                visible_filesystem_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_filesystem_search_query,
                    self.connection_monitor.host_filesystem_filter,
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_filesystem_list_state(&rows, selected_id);

        div()
            .id("host-filesystems-panel")
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
                        !self.connection_monitor.host_filesystem_snapshot_polling,
                        cx,
                    ))
                    .child(self.render_host_filesystem_search(cx))
                    .child(self.render_host_filesystem_filter_row(cx))
                    .child(self.render_host_filesystem_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        cx,
                    )),
            )
            .child(self.render_host_filesystem_list(
                rows,
                self.connection_monitor.host_filesystem_snapshot_polling,
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_filesystem_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostFilesystemSearch;
        let focused = self.connection_monitor.host_filesystem_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_filesystem_search_query,
                    placeholder: self.i18n.t("sidebar.host_filesystems.search_placeholder"),
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
                    this.connection_monitor.host_filesystem_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
                    this.connection_monitor.host_log_search_focused = false;
                    this.connection_monitor.host_tmux_search_focused = false;
                    this.connection_monitor.host_port_search_focused = false;
                    this.connection_monitor.host_schedule_search_focused = false;
                    this.connection_monitor.host_package_search_focused = false;
                    this.ime_marked_text = None;
                    this.new_connection_caret_visible = true;
                    window.focus(&this.focus_handle, cx);
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

    pub(super) fn render_host_filesystem_filter_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .id("host-filesystem-filter-scroll")
            .flex()
            .items_center()
            .gap_1()
            .overflow_x_scroll();
        for filter in [
            FilesystemFilter::All,
            FilesystemFilter::Attention,
            FilesystemFilter::Mounts,
            FilesystemFilter::ReadOnly,
            FilesystemFilter::HighUsage,
            FilesystemFilter::InodePressure,
            FilesystemFilter::InodeHotspots,
            FilesystemFilter::LargeItems,
            FilesystemFilter::Blocks,
        ] {
            row = row.child(self.render_host_filesystem_filter_chip(filter, cx));
        }
        row.into_any_element()
    }

    pub(super) fn render_host_filesystem_filter_chip(
        &self,
        filter: FilesystemFilter,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.connection_monitor.host_filesystem_filter == filter;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(filesystem_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_filesystem_filter != filter {
                        this.connection_monitor.host_filesystem_filter = filter;
                        this.connection_monitor.host_filesystem_expanded_index = None;
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_filesystem_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceFilesystemStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let capability_label = match status {
            ResourceFilesystemStatus::Available {
                capability: FilesystemCommandCapability::Full,
                ..
            } => self.i18n.t("sidebar.host_filesystems.capability.full"),
            ResourceFilesystemStatus::Available {
                capability: FilesystemCommandCapability::Partial,
                ..
            } => self.i18n.t("sidebar.host_filesystems.capability.partial"),
            _ => self.i18n.t("sidebar.host_filesystems.capability.unknown"),
        };
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
                visible_count,
                self.i18n.t("sidebar.host_filesystems.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::Terminal,
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
                        self.i18n.t("sidebar.host_filesystems.actions.diagnostic"),
                        "host-filesystem-diagnostic",
                        true,
                        cx.listener({
                            let selected_id = selected_id.clone();
                            move |this, _event, window, cx| {
                                this.open_host_filesystem_diagnostic_terminal(
                                    selected_id.clone(),
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                        cx.entity(),
                    ))
                    .child(self.workspace_tooltip_icon_button(
                        LucideIcon::RefreshCw,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.connection_monitor.host_filesystem_snapshot_polling,
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        self.i18n.t("sidebar.host_filesystems.actions.refresh"),
                        "host-filesystem-refresh",
                        true,
                        cx.listener(move |this, _event, _window, cx| {
                            this.request_host_filesystems_snapshot(
                                selected_id.clone(),
                                HostSnapshotFeedback::Toast,
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                        cx.entity(),
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_host_filesystem_list(
        &self,
        rows: Vec<ResourceFilesystemEntry>,
        loading: bool,
        status: ResourceFilesystemStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::HardDrive,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_filesystems.loading"),
                cx,
            );
        }
        match status {
            ResourceFilesystemStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::HardDrive,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_filesystems.unavailable"),
                    cx,
                );
            }
            ResourceFilesystemStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_filesystems.error", &[("error", message)]),
                    cx,
                );
            }
            ResourceFilesystemStatus::Unknown | ResourceFilesystemStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::HardDrive,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_filesystems.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.connection_monitor.host_filesystem_list_state.clone();
        let spec = TauriVirtualListSpec::new(px(HOST_FILESYSTEM_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        let show_context_columns =
            self.ai.chat.sidebar_width >= HOST_FILESYSTEM_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_filesystem_table_header(show_context_columns))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            let rows = rows.clone();
                            let selected_id = selected_id.clone();
                            workspace.update(cx, |this, cx| {
                                this.render_host_filesystem_row(
                                    selected_id.as_str(),
                                    index,
                                    rows.get(index).cloned(),
                                    show_context_columns,
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    pub(super) fn render_host_filesystem_table_header(
        &self,
        show_context_columns: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_FILESYSTEM_TABLE_HEADER_HEIGHT))
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
                    .child(self.i18n.t("sidebar.host_filesystems.columns.path")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_KIND_COLUMN_WIDTH))
                    .truncate()
                    .child(self.i18n.t("sidebar.host_filesystems.columns.kind")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_USAGE_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_filesystems.columns.usage")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_INODE_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(self.i18n.t("sidebar.host_filesystems.columns.inode")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_FS_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_filesystems.columns.fs")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_SIZE_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .child(self.i18n.t("sidebar.host_filesystems.columns.size")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_RO_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_filesystems.columns.read_only")),
                    )
            })
            .into_any_element()
    }

    pub(super) fn render_host_filesystem_row(
        &self,
        connection_id: &str,
        index: usize,
        entry: Option<ResourceFilesystemEntry>,
        show_context_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.connection_monitor.host_filesystem_expanded_index == Some(index);
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let kind = host_filesystem_kind_display(&self.i18n, &entry.kind);
        let usage = host_filesystem_usage_label(&self.i18n, &entry);
        let inode = host_filesystem_percent_dash(&entry.inode_percent);
        let size = host_filesystem_size_label(&entry.size_bytes);
        let read_only = host_filesystem_read_only_display(&self.i18n, entry.read_only);

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
                    .h(px(HOST_FILESYSTEM_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Path is the identity column. Keep it first-level flex so
                    // fixed filesystem metadata cannot collapse it during sidebar resize.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(host_filesystem_path_color(&entry, theme.text)))
                            .font_family(mono_font.clone())
                            .child(host_filesystem_blank_dash(&entry.path)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_KIND_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(kind),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_USAGE_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_filesystem_percent_color(
                                &entry.used_percent,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(usage),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_INODE_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_filesystem_percent_color(
                                &entry.inode_percent,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(inode),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_FILESYSTEM_FS_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(host_filesystem_blank_dash(&entry.fs_type)),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_FILESYSTEM_SIZE_COLUMN_WIDTH))
                                .flex()
                                .justify_end()
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(size.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_FILESYSTEM_RO_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(if entry.read_only {
                                    MONITOR_AMBER
                                } else {
                                    theme.text_muted
                                }))
                                .font_family(mono_font.clone())
                                .child(read_only.clone()),
                        )
                    }),
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
                            .child(host_filesystem_meta_label(
                                &self.i18n,
                                &entry,
                                show_context_columns,
                            )),
                    )
                    .child(self.render_host_filesystem_attention_badges(&entry))
                    .child(self.render_host_filesystem_inline_actions(connection_id, &entry, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_filesystem_detail(&entry))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_filesystem_expanded_index == Some(index) {
                        this.connection_monitor.host_filesystem_expanded_index = None;
                    } else {
                        this.connection_monitor.host_filesystem_expanded_index = Some(index);
                    }
                    cx.notify();
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_filesystem_inline_actions(
        &self,
        connection_id: &str,
        entry: &ResourceFilesystemEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let path = entry.path.clone();
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::Copy,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                self.i18n.t("sidebar.host_filesystems.actions.copy_path"),
                "host-filesystem-copy-path",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.copy_host_filesystem_path(path.clone(), cx);
                    cx.stop_propagation();
                }),
                cx.entity(),
            ))
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::Terminal,
                12.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 22.0,
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(22.0)
                },
                self.i18n.t("sidebar.host_filesystems.actions.diagnostic"),
                "host-filesystem-row-diagnostic",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |this, _event, window, cx| {
                        this.open_host_filesystem_diagnostic_terminal(
                            connection_id.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
                cx.entity(),
            ))
            .into_any_element()
    }

    pub(super) fn render_host_filesystem_attention_badges(
        &self,
        entry: &ResourceFilesystemEntry,
    ) -> AnyElement {
        let keys = filesystem_attention_label_keys(entry);
        if keys.is_empty() {
            return div().into_any_element();
        }
        let severity = filesystem_entry_severity(entry);
        let color = match severity {
            FilesystemEntrySeverity::Critical => MONITOR_RED,
            FilesystemEntrySeverity::Warning => MONITOR_AMBER,
            FilesystemEntrySeverity::Normal => self.tokens.ui.text_muted,
        };
        let mut row = div()
            .flex_none()
            .flex()
            .items_center()
            .gap_1()
            .overflow_hidden();
        for key in keys.into_iter().take(2) {
            row = row.child(
                div()
                    .flex_none()
                    .h(px(20.0))
                    .px_1p5()
                    .flex()
                    .items_center()
                    .rounded(px(10.0))
                    .bg(rgba((color << 8) | MONITOR_TINT_ALPHA))
                    .text_size(px(10.0))
                    .text_color(rgb(color))
                    .child(self.i18n.t(key)),
            );
        }
        row.into_any_element()
    }

    pub(super) fn render_host_filesystem_detail(
        &self,
        entry: &ResourceFilesystemEntry,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        div()
            .mx_3()
            .mb_2()
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(rgba((theme.border << 8) | MONITOR_BORDER_ALPHA))
            .bg(rgb(theme.bg_panel))
            .overflow_x_scrollbar()
            .child(
                div()
                    .p_3()
                    .min_w(px(700.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .font_family(mono_font)
                    .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.path"),
                        host_filesystem_blank_dash(&entry.path)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.kind"),
                        host_filesystem_kind_display(&self.i18n, &entry.kind)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.device"),
                        host_filesystem_blank_dash(&entry.device)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.fs"),
                        host_filesystem_blank_dash(&entry.fs_type)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.size"),
                        host_filesystem_size_label(&entry.size_bytes)
                    ))
                    .child(format!(
                        "{}: {} / {}",
                        self.i18n
                            .t("sidebar.host_filesystems.columns.used_available"),
                        host_filesystem_size_label(&entry.used_bytes),
                        host_filesystem_size_label(&entry.available_bytes)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.usage"),
                        host_filesystem_percent_dash(&entry.used_percent)
                    ))
                    .child(format!(
                        "{}: {} / {} / {}",
                        self.i18n.t("sidebar.host_filesystems.columns.inode"),
                        host_filesystem_blank_dash(&entry.inode_used),
                        host_filesystem_blank_dash(&entry.inode_available),
                        host_filesystem_percent_dash(&entry.inode_percent)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.read_only"),
                        host_filesystem_read_only_display(&self.i18n, entry.read_only)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.attention"),
                        host_filesystem_attention_summary(&self.i18n, entry)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.source"),
                        host_filesystem_blank_dash(&entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.detail"),
                        host_filesystem_blank_dash(&entry.detail)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_filesystems.columns.options"),
                        host_filesystem_blank_dash(&entry.options)
                    ))),
            )
            .into_any_element()
    }

    pub(super) fn sync_host_filesystem_list_state(
        &self,
        rows: &[ResourceFilesystemEntry],
        selected_id: &str,
    ) {
        let signatures = rows
            .iter()
            .map(filesystem_row_signature)
            .collect::<Vec<_>>();
        let identity = format!(
            "host-filesystems:{selected_id}:{}:{}:{}",
            self.connection_monitor.host_filesystem_search_query,
            self.connection_monitor.host_filesystem_filter as u8,
            self.connection_monitor
                .host_filesystem_expanded_index
                .unwrap_or(usize::MAX)
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_filesystem_list_state,
            &mut self
                .connection_monitor
                .host_filesystem_list_cache
                .borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_FILESYSTEM_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn host_filesystem_snapshot_command(
        &self,
        connection_id: &str,
    ) -> (
        oxideterm_connection_monitor::FilesystemCaptureCommand,
        String,
    ) {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        (build_filesystem_snapshot_command(&os_type), os_type)
    }

    pub(super) fn host_filesystem_diagnostic_command(
        &self,
        connection_id: &str,
    ) -> (String, String) {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        (build_filesystem_diagnostic_command(&os_type), os_type)
    }

    pub(in crate::workspace) fn handle_host_filesystem_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_filesystem_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_filesystem_search_focused = false;
            self.ime_marked_text = None;
            self.clear_ime_selection();
            cx.notify();
            return true;
        }
        false
    }

    pub(super) fn request_host_filesystems_snapshot_for_selected_connection(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let connections = self.monitor_connections();
        let Some(connection_id) = self
            .connection_monitor
            .selected_connection_id
            .clone()
            .or_else(|| {
                connections
                    .first()
                    .map(|connection| connection.connection_id.clone())
            })
        else {
            return;
        };
        self.request_host_filesystems_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_host_filesystems_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_filesystem_snapshot_polling {
            if feedback.should_toast() {
                self.push_host_filesystem_toast(
                    self.i18n
                        .t("sidebar.host_filesystems.toast.snapshot_already_running"),
                    TerminalNoticeVariant::Warning,
                );
            }
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            if feedback.should_toast() {
                self.push_host_filesystem_toast(
                    self.i18n
                        .t("sidebar.host_filesystems.toast.connection_missing"),
                    TerminalNoticeVariant::Error,
                );
            }
            cx.notify();
            return;
        };
        let (command, os_type) = self.host_filesystem_snapshot_command(&connection_id);
        if feedback.should_toast() && command.capability == FilesystemCommandCapability::Partial {
            self.push_host_filesystem_toast(
                self.i18n_replace(
                    "sidebar.host_filesystems.toast.partial_support",
                    &[("os", os_type)],
                ),
                TerminalNoticeVariant::Warning,
            );
        }

        let request = HostFilesystemSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor
            .host_filesystem_snapshot_connection_id = Some(connection_id);
        self.connection_monitor.host_filesystem_snapshot_running = Some(request.clone());
        self.connection_monitor.host_filesystem_snapshot_rx = Some(rx);
        self.connection_monitor.host_filesystem_snapshot_polling = true;
        self.connection_monitor.host_filesystem_last_error = None;
        // Filesystem scans can touch du/find, so they stay manual snapshot work
        // instead of joining the high-frequency resource profiler loop.
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_FILESYSTEM_SNAPSHOT_TIMEOUT,
                    HOST_FILESYSTEM_SNAPSHOT_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostFilesystemSnapshotDelivery { request, result });
        });
        cx.notify();
    }

    pub(super) fn copy_host_filesystem_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
        self.push_host_filesystem_toast(
            self.i18n_replace(
                "sidebar.host_filesystems.toast.copied_path",
                &[("path", path)],
            ),
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(super) fn open_host_filesystem_diagnostic_terminal(
        &mut self,
        connection_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (command, _os_type) = self.host_filesystem_diagnostic_command(&connection_id);
        let title = self.i18n.t("sidebar.host_filesystems.diagnostic_title");
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_filesystem_toast(
                self.i18n
                    .t("sidebar.host_filesystems.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_filesystem_toast(
                self.i18n
                    .t("sidebar.host_filesystems.toast.exec_terminal_missing"),
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
            Ok(()) => self.push_host_filesystem_toast(
                self.i18n
                    .t("sidebar.host_filesystems.toast.diagnostic_opened"),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_filesystem_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }

    pub(in crate::workspace) fn poll_host_filesystems_snapshot_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_monitor.host_filesystem_snapshot_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_filesystem_snapshot_rx.take() else {
            self.connection_monitor.host_filesystem_snapshot_polling = false;
            self.connection_monitor.host_filesystem_snapshot_running = None;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.finish_host_filesystems_snapshot(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_filesystem_snapshot_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let feedback = self
                    .connection_monitor
                    .host_filesystem_snapshot_running
                    .as_ref()
                    .map(|request| request.feedback)
                    .unwrap_or(HostSnapshotFeedback::Silent);
                self.connection_monitor.host_filesystem_snapshot_polling = false;
                self.connection_monitor.host_filesystem_snapshot_running = None;
                let reason = self.i18n.t("sidebar.host_filesystems.toast.unknown_error");
                self.connection_monitor.host_filesystem_last_error = Some(reason.clone());
                if feedback.should_toast() {
                    self.push_host_filesystem_toast(
                        self.i18n_replace(
                            "sidebar.host_filesystems.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
                cx.notify();
            }
        }
    }

    pub(super) fn finish_host_filesystems_snapshot(
        &mut self,
        delivery: HostFilesystemSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_filesystem_snapshot_running
            .as_ref()
            .is_some_and(|running| running != &delivery.request)
        {
            cx.notify();
            return;
        }
        let feedback = delivery.request.feedback;
        self.connection_monitor.host_filesystem_snapshot_polling = false;
        self.connection_monitor.host_filesystem_snapshot_running = None;
        self.connection_monitor.host_filesystem_snapshot_rx = None;
        match delivery.result {
            Ok(output) => {
                let snapshot =
                    filesystem_capture_snapshot(&output.stdout, &output.stderr, output.exit_code);
                let visible_count = visible_filesystem_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_filesystem_search_query,
                    self.connection_monitor.host_filesystem_filter,
                )
                .len();
                match &snapshot.status {
                    ResourceFilesystemStatus::Available { .. } => {
                        self.connection_monitor.host_filesystem_last_error = None;
                        if feedback.should_toast() {
                            self.push_host_filesystem_toast(
                                self.i18n_replace(
                                    "sidebar.host_filesystems.toast.snapshot_loaded",
                                    &[("count", visible_count.to_string())],
                                ),
                                TerminalNoticeVariant::Success,
                            );
                        }
                    }
                    ResourceFilesystemStatus::Unavailable => {
                        self.connection_monitor.host_filesystem_last_error =
                            Some(self.i18n.t("sidebar.host_filesystems.unavailable"));
                        if feedback.should_toast() {
                            self.push_host_filesystem_toast(
                                self.i18n.t("sidebar.host_filesystems.toast.unavailable"),
                                TerminalNoticeVariant::Warning,
                            );
                        }
                    }
                    ResourceFilesystemStatus::Error { message } => {
                        self.connection_monitor.host_filesystem_last_error = Some(message.clone());
                        if feedback.should_toast() {
                            self.push_host_filesystem_toast(
                                self.i18n_replace(
                                    "sidebar.host_filesystems.toast.snapshot_failed",
                                    &[("reason", message.clone())],
                                ),
                                TerminalNoticeVariant::Error,
                            );
                        }
                    }
                    ResourceFilesystemStatus::Unknown => {}
                }
                self.connection_monitor
                    .host_filesystem_snapshot_connection_id = Some(delivery.request.connection_id);
                self.connection_monitor.host_filesystem_snapshot = Some(snapshot);
            }
            Err(error) => {
                self.connection_monitor.host_filesystem_last_error = Some(error.clone());
                self.connection_monitor
                    .host_filesystem_snapshot_connection_id = Some(delivery.request.connection_id);
                self.connection_monitor.host_filesystem_snapshot =
                    Some(ResourceFilesystemSnapshot {
                        status: ResourceFilesystemStatus::Error {
                            message: error.clone(),
                        },
                        entries: Vec::new(),
                    });
                if feedback.should_toast() {
                    self.push_host_filesystem_toast(
                        self.i18n_replace(
                            "sidebar.host_filesystems.toast.snapshot_failed",
                            &[("reason", error)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
            }
        }
        cx.notify();
    }

    pub(super) fn push_host_filesystem_toast(
        &mut self,
        message: String,
        variant: TerminalNoticeVariant,
    ) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title: message,
            description: None,
            status_text: None,
            progress: None,
            variant,
        });
    }
}

fn host_filesystem_blank_dash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        "—".to_string()
    } else {
        trimmed.to_string()
    }
}

fn host_filesystem_kind_display(i18n: &I18n, kind: &str) -> String {
    let key = filesystem_kind_label_key(kind);
    if key == "sidebar.host_filesystems.kinds.unknown" && !kind.trim().is_empty() {
        kind.trim().to_string()
    } else {
        i18n.t(key)
    }
}

fn host_filesystem_read_only_display(i18n: &I18n, read_only: bool) -> String {
    i18n.t(filesystem_read_only_label_key(read_only))
}

fn host_filesystem_usage_label(i18n: &I18n, entry: &ResourceFilesystemEntry) -> String {
    if entry.kind == "mount" {
        return host_filesystem_percent_dash(&entry.used_percent);
    }
    if entry.kind == "inode_dir" {
        return host_filesystem_i18n_replace(
            i18n,
            "sidebar.host_filesystems.values.inode_count",
            &[("count", host_filesystem_blank_dash(&entry.inode_used))],
        );
    }
    if entry.kind == "count_dir" {
        return host_filesystem_i18n_replace(
            i18n,
            "sidebar.host_filesystems.values.file_count",
            &[("count", host_filesystem_blank_dash(&entry.inode_used))],
        );
    }
    host_filesystem_size_label(&entry.size_bytes)
}

fn host_filesystem_i18n_replace(i18n: &I18n, key: &str, replacements: &[(&str, String)]) -> String {
    let mut text = i18n.t(key);
    for (name, value) in replacements {
        text = text.replace(&format!("{{{{{name}}}}}"), value);
    }
    text
}

fn host_filesystem_percent_dash(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('%');
    if trimmed.is_empty() {
        "—".to_string()
    } else {
        format!("{trimmed}%")
    }
}

fn host_filesystem_size_label(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return "—".to_string();
    }
    match trimmed.parse::<u64>() {
        Ok(bytes) => format_bytes(bytes),
        Err(_) => trimmed.to_string(),
    }
}

fn host_filesystem_path_color(entry: &ResourceFilesystemEntry, default_color: u32) -> u32 {
    match filesystem_entry_severity(entry) {
        FilesystemEntrySeverity::Critical => MONITOR_RED,
        FilesystemEntrySeverity::Warning => MONITOR_AMBER,
        FilesystemEntrySeverity::Normal => default_color,
    }
}

fn host_filesystem_percent_color(value: &str, muted_color: u32) -> u32 {
    match filesystem_percent_severity(value) {
        FilesystemEntrySeverity::Critical => MONITOR_RED,
        FilesystemEntrySeverity::Warning => MONITOR_AMBER,
        FilesystemEntrySeverity::Normal if host_filesystem_percent_value(value) > 0 => {
            MONITOR_EMERALD
        }
        FilesystemEntrySeverity::Normal => muted_color,
    }
}

fn host_filesystem_percent_value(value: &str) -> u32 {
    value
        .trim()
        .trim_end_matches('%')
        .split('.')
        .next()
        .unwrap_or_default()
        .parse::<u32>()
        .unwrap_or(0)
}

fn host_filesystem_meta_label(
    i18n: &I18n,
    entry: &ResourceFilesystemEntry,
    show_context_columns: bool,
) -> String {
    if show_context_columns {
        return format!(
            "{} · {}",
            i18n.t("sidebar.host_filesystems.columns.source"),
            host_filesystem_blank_dash(&entry.source)
        );
    }
    let device_or_detail = if !entry.device.trim().is_empty() {
        entry.device.as_str()
    } else if !entry.detail.trim().is_empty() {
        entry.detail.as_str()
    } else {
        entry.source.as_str()
    };
    format!(
        "{} · {}",
        host_filesystem_blank_dash(device_or_detail),
        host_filesystem_blank_dash(&entry.options)
    )
}

fn host_filesystem_attention_summary(i18n: &I18n, entry: &ResourceFilesystemEntry) -> String {
    let labels = filesystem_attention_label_keys(entry)
        .into_iter()
        .map(|key| i18n.t(key))
        .collect::<Vec<_>>();
    if labels.is_empty() {
        "—".to_string()
    } else {
        labels.join(" · ")
    }
}
