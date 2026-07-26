//! Owns the filesystems Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::{filesystem_percent_severity, parse_filesystem_snapshot};

impl WorkspaceApp {
    pub(super) fn render_host_filesystems_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections(cx);
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::HardDrive,
                self.tokens.ui.text_muted,
                self.i18n.t("profiler.panel.no_connection"),
                cx,
            );
        }

        let selected_connection_id = self.host_tools.read(cx).selected_connection_id_owned();
        let selected_id = selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let snapshot = self
            .host_tools
            .read(cx)
            .filesystem_snapshot_for(selected_id);
        let filter = self.host_tools.read(cx).filesystem_filter();
        let filesystem_search_query = self
            .host_tools
            .read(cx)
            .ui
            .host_filesystem_search_query
            .clone();
        let rows = snapshot
            .as_ref()
            .map(|snapshot| {
                visible_filesystem_rows(&snapshot.entries, &filesystem_search_query, filter)
            })
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_filesystem_list_state(&rows, selected_id, cx);

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
                        !self.host_tools.read(cx).filesystem_snapshot_polling(),
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
                self.host_tools.read(cx).filesystem_snapshot_polling(),
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_filesystem_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostFilesystemSearch;
        let (focused, value) = {
            let ui = &self.host_tools.read(cx).ui;
            (
                ui.input_is_focused(HostToolsTextInput::FilesystemSearch),
                ui.host_filesystem_search_query.clone(),
            )
        };
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder: self.i18n.t("sidebar.host_filesystems.search_placeholder"),
                    focused,
                    caret_visible: self.new_connection_caret_visible,
                    secret: false,
                    selected_all: false,
                    selected_range: self.ime_selected_range_for_target(target, cx),
                    marked_text: self.marked_text_for_target(target, cx),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.host_tools.update(cx, |host_tools, _cx| {
                        host_tools
                            .ui
                            .focus_input(HostToolsTextInput::FilesystemSearch);
                    });
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
        let active = self.host_tools.read(cx).filesystem_filter() == filter;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(filesystem_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.select_filesystem_filter(filter, cx);
                    });
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
                            disabled: self.host_tools.read(cx).filesystem_snapshot_polling(),
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
        let state = self.host_tools.read(cx).filesystem_list_state();
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
        let expanded = self.host_tools.read(cx).filesystem_expanded_index() == Some(index);
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
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.toggle_filesystem_expanded(index, cx);
                    });
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
        cx: &mut Context<Self>,
    ) {
        let signatures = rows
            .iter()
            .map(filesystem_row_signature)
            .collect::<Vec<_>>();
        let identity = format!(
            "host-filesystems:{selected_id}:{}:{}:{}",
            self.host_tools.read(cx).ui.host_filesystem_search_query,
            self.host_tools.read(cx).filesystem_filter() as u8,
            self.host_tools
                .read(cx)
                .filesystem_expanded_index()
                .unwrap_or(usize::MAX)
        );
        self.host_tools
            .read(cx)
            .sync_filesystem_list_signatures(&identity, &signatures);
    }

    pub(in crate::workspace) fn handle_host_filesystem_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::FilesystemSearch)
        {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.host_tools.update(cx, |host_tools, _cx| {
                host_tools.ui.clear_input_focus();
            });
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
        let connections = self.monitor_connections(cx);
        let Some(connection_id) = self
            .host_tools
            .read(cx)
            .selected_connection_id_owned()
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
        let monitoring_enabled = self.host_tool_monitoring_enabled(ContextSidebarTool::Filesystems);
        let runtime = self.forwarding_runtime.handle().clone();
        let failure_fallback = self.i18n.t("sidebar.host_filesystems.toast.unknown_error");
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_filesystem_snapshot(
                connection_id,
                feedback,
                monitoring_enabled,
                runtime,
                failure_fallback,
                cx,
            )
        });
        for notice in notices {
            self.push_host_tools_notice(notice);
        }
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
        let command = self
            .host_tools
            .read(cx)
            .filesystem_diagnostic_command(&connection_id);
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
        if !self.ssh_nodes.contains_key(&node_id) {
            self.push_host_filesystem_toast(
                self.i18n
                    .t("sidebar.host_filesystems.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        }
        match self.queue_ssh_terminal_tab_for_existing_node(
            node_id,
            Some(command),
            title,
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

impl HostToolsEntity {
    pub(super) fn filesystem_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<ResourceFilesystemSnapshot> {
        self.host_filesystems
            .snapshot
            .as_ref()
            .filter(|_| {
                self.host_filesystems.snapshot_connection_id.as_deref() == Some(connection_id)
            })
            .cloned()
    }

    pub(super) fn filesystem_snapshot_polling(&self) -> bool {
        self.host_filesystems.polling
    }

    pub(in crate::workspace::connection_monitor) fn filesystem_filter(&self) -> FilesystemFilter {
        self.host_filesystems.filter
    }

    pub(super) fn filesystem_list_state(&self) -> ListState {
        self.host_filesystems.list_state.clone()
    }

    pub(in crate::workspace::connection_monitor) fn filesystem_expanded_index(
        &self,
    ) -> Option<usize> {
        self.host_filesystems.expanded_index
    }

    pub(in crate::workspace::connection_monitor) fn select_filesystem_filter(
        &mut self,
        filter: FilesystemFilter,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_filesystems.filter == filter {
            return false;
        }
        self.host_filesystems.filter = filter;
        self.host_filesystems.expanded_index = None;
        cx.notify();
        true
    }

    pub(in crate::workspace::connection_monitor) fn toggle_filesystem_expanded(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.host_filesystems.expanded_index =
            (self.host_filesystems.expanded_index != Some(index)).then_some(index);
        cx.notify();
    }

    pub(super) fn sync_filesystem_list_signatures(&self, identity: &str, signatures: &[u64]) {
        sync_tauri_variable_list_state_by_signatures(
            &self.host_filesystems.list_state,
            &mut self.host_filesystems.list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(px(HOST_FILESYSTEM_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn filesystem_diagnostic_command(&self, connection_id: &str) -> String {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_filesystem_diagnostic_command(&os_type)
    }

    pub(super) fn request_filesystem_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        monitoring_enabled: bool,
        runtime: tokio::runtime::Handle,
        failure_fallback: String,
        cx: &mut Context<Self>,
    ) -> Vec<HostToolsNotice> {
        if !monitoring_enabled {
            return Vec::new();
        }
        if self.host_filesystems.polling {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::FilesystemSnapshotAlreadyRunning)
                .into_iter()
                .collect();
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::FilesystemConnectionMissing)
                .into_iter()
                .collect();
        };
        let command = build_filesystem_snapshot_command(&os_type);
        let mut notices = Vec::new();
        if feedback.should_toast() && command.capability == FilesystemCommandCapability::Partial {
            notices.push(HostToolsNotice::FilesystemPartialSupport { os_type });
        }

        let request = HostFilesystemSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            failure_fallback,
        };
        self.host_filesystems.snapshot_connection_id = Some(connection_id);
        self.host_filesystems.running = Some(request.clone());
        self.host_filesystems.polling = true;
        // Filesystem scans may touch du/find and remain manual user work.
        let spawned = self.spawn_filesystem_snapshot_capture(
            command.command,
            request,
            HOST_FILESYSTEM_SNAPSHOT_TIMEOUT,
            HOST_FILESYSTEM_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_filesystems.polling = false;
            self.host_filesystems.running = None;
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::FilesystemConnectionMissing)
                .into_iter()
                .collect();
        }
        cx.notify();
        notices
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_filesystems_snapshot(
        &mut self,
        delivery: HostFilesystemSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_filesystems.running.as_ref() != Some(&delivery.request) {
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_filesystems.polling = false;
        self.host_filesystems.running = None;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_filesystem_snapshot(&output.stdout);
                if feedback.should_toast() {
                    match &snapshot.status {
                        ResourceFilesystemStatus::Available { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::FilesystemSnapshotLoaded {
                                    count: snapshot.entries.len(),
                                },
                            ));
                        }
                        ResourceFilesystemStatus::Unavailable => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::FilesystemUnavailable,
                            ));
                        }
                        ResourceFilesystemStatus::Error { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::FilesystemSnapshotFailed,
                            ));
                        }
                        ResourceFilesystemStatus::Unknown => {}
                    }
                }
                self.host_filesystems.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_filesystems.snapshot = Some(snapshot);
            }
            Ok(_) | Err(()) => {
                self.host_filesystems.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_filesystems.snapshot = Some(ResourceFilesystemSnapshot {
                    status: ResourceFilesystemStatus::Error {
                        message: failure_fallback,
                    },
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::FilesystemSnapshotFailed,
                    ));
                }
            }
        }
        cx.notify();
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
