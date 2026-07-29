//! Owns the filesystems Host Tool UI and request lifecycle.

use super::*;

use oxideterm_connection_monitor::{filesystem_percent_severity, parse_filesystem_snapshot};

impl HostToolsEntity {
    #[allow(clippy::too_many_arguments)]
    fn render_host_filesystems_panel(
        &self,
        search_ime: HostToolsPlainTextImeFrame,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return host_tools_center_state(
                LucideIcon::WifiOff,
                tokens.ui.text_muted,
                i18n.t("profiler.panel.no_connection"),
                selectable_text,
                cx,
            );
        }

        let selected_connection_id = self.selected_connection_id_owned();
        let selected_id = selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let snapshot = self.filesystem_snapshot_for(selected_id);
        let filter = self.filesystem_filter();
        let filesystem_search_query = self.ui.host_filesystem_search_query.clone();
        let rows = snapshot
            .map(|snapshot| {
                visible_filesystem_rows(&snapshot.entries, &filesystem_search_query, filter)
            })
            .unwrap_or_default();
        let status = snapshot
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_filesystem_render_rows(&rows, selected_id);

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
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &connections,
                        selected_id,
                        !self.filesystem_snapshot_polling(),
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_filesystem_search(&search_ime, tokens, i18n, cx))
                    .child(self.render_host_filesystem_filter_row(tokens, i18n, cx))
                    .child(self.render_host_filesystem_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(self.render_host_filesystem_list(
                rows,
                self.filesystem_snapshot_polling(),
                status,
                selected_id,
                sidebar_width,
                tokens,
                i18n,
                mono_font_family,
                selectable_text,
                cx,
            ))
            .into_any_element()
    }

    fn render_host_filesystem_search(
        &self,
        ime: &HostToolsPlainTextImeFrame,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = ime.input();
        let anchor_frame = ime.clone();
        text_input_anchor_probe(
            ime.anchor_id(),
            text_input(
                tokens,
                TextInputView {
                    value: &self.ui.host_filesystem_search_query,
                    placeholder: i18n.t("sidebar.host_filesystems.search_placeholder"),
                    focused: self.ui.input_is_focused(input),
                    caret_visible: ime.caret_visible(),
                    secret: false,
                    selected_all: false,
                    selected_range: ime.selected_range(),
                    marked_text: ime.marked_text(),
                },
            )
            .h(px(34.0))
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, event: &MouseDownEvent, window, cx| {
                    host_tools.ui.focus_input(input);
                    // Window-owned IME selection crosses the Entity boundary once.
                    window.dispatch_action(
                        Box::new(HostToolsWindowRequest::new(
                            HostToolsWindowIntent::BeginPlainTextImeSelection {
                                input,
                                event: event.clone(),
                            },
                        )),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ),
            move |anchor, _window, _cx| {
                anchor_frame.update_anchor(anchor);
            },
        )
        .into_any_element()
    }
}

impl HostToolsEntity {
    fn render_host_filesystem_filter_row(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            row = row.child(self.render_host_filesystem_filter_chip(filter, tokens, i18n, cx));
        }
        row.into_any_element()
    }

    fn render_host_filesystem_filter_chip(
        &self,
        filter: FilesystemFilter,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.filesystem_filter() == filter;
        host_filesystem_filter_chip(active, tokens)
            .child(i18n.t(filesystem_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.select_filesystem_filter(filter, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }
}

impl HostToolsEntity {
    fn render_host_filesystem_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourceFilesystemStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match status {
            ResourceFilesystemStatus::Available {
                capability: FilesystemCommandCapability::Full,
                ..
            } => i18n.t("sidebar.host_filesystems.capability.full"),
            ResourceFilesystemStatus::Available {
                capability: FilesystemCommandCapability::Partial,
                ..
            } => i18n.t("sidebar.host_filesystems.capability.partial"),
            _ => i18n.t("sidebar.host_filesystems.capability.unknown"),
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
                i18n.t("sidebar.host_filesystems.count_suffix"),
                capability_label
            )))
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(host_tools_tooltip_icon_button(
                        tokens,
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
                        i18n.t("sidebar.host_filesystems.actions.diagnostic"),
                        "host-filesystem-diagnostic",
                        true,
                        cx.listener({
                            let selected_id = selected_id.clone();
                            let i18n = i18n.clone();
                            move |host_tools, _event, window, cx| {
                                host_tools.dispatch_host_filesystem_diagnostic(
                                    selected_id.clone(),
                                    &i18n,
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }
                        }),
                    ))
                    .child(host_tools_tooltip_icon_button(
                        tokens,
                        LucideIcon::RefreshCw,
                        13.0,
                        rgb(theme.text),
                        oxideterm_gpui_ui::button::IconButtonOptions {
                            size: 24.0,
                            disabled: self.filesystem_snapshot_polling(),
                            has_background: true,
                            background: Some(rgb(theme.bg_hover)),
                            hover_background: Some(rgb(theme.bg_panel)),
                            idle_opacity: 1.0,
                            ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                        },
                        i18n.t("sidebar.host_filesystems.actions.refresh"),
                        "host-filesystem-refresh",
                        true,
                        cx.listener(move |host_tools, _event, _window, cx| {
                            host_tools
                                .request_active_tool_snapshot(HostSnapshotFeedback::Toast, cx);
                            cx.stop_propagation();
                        }),
                    )),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_filesystem_list(
        &self,
        rows: Vec<ResourceFilesystemEntry>,
        loading: bool,
        status: ResourceFilesystemStatus,
        selected_id: &str,
        sidebar_width: f32,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        selectable_text: &SelectableTextRenderState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::HardDrive,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_filesystems.loading"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourceFilesystemStatus::Unavailable => {
                return host_tools_center_state(
                    LucideIcon::HardDrive,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_filesystems.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourceFilesystemStatus::Error { message } => {
                return host_tools_center_state(
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    host_filesystem_i18n_replace(
                        i18n,
                        "sidebar.host_filesystems.error",
                        &[("error", message)],
                    ),
                    selectable_text,
                    cx,
                );
            }
            ResourceFilesystemStatus::Unknown | ResourceFilesystemStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::HardDrive,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_filesystems.empty"),
                selectable_text,
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.filesystem_list_state();
        let spec = TauriVirtualListSpec::new(px(HOST_FILESYSTEM_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let tokens = *tokens;
        let i18n = i18n.clone();
        let show_context_columns = sidebar_width >= HOST_FILESYSTEM_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_filesystem_table_header(show_context_columns, &tokens, &i18n))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            let mono_font_family = mono_font_family.clone();
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_filesystem_row(
                                    selected_id.as_str(),
                                    index,
                                    rows.get(index).cloned(),
                                    show_context_columns,
                                    &tokens,
                                    &i18n,
                                    mono_font_family,
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_filesystem_table_header(
        &self,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
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
                    .child(i18n.t("sidebar.host_filesystems.columns.path")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_KIND_COLUMN_WIDTH))
                    .truncate()
                    .child(i18n.t("sidebar.host_filesystems.columns.kind")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_USAGE_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_filesystems.columns.usage")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_FILESYSTEM_INODE_COLUMN_WIDTH))
                    .flex()
                    .justify_end()
                    .child(i18n.t("sidebar.host_filesystems.columns.inode")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_FS_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_filesystems.columns.fs")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_SIZE_COLUMN_WIDTH))
                            .flex()
                            .justify_end()
                            .child(i18n.t("sidebar.host_filesystems.columns.size")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_RO_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_filesystems.columns.read_only")),
                    )
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_filesystem_row(
        &self,
        connection_id: &str,
        index: usize,
        entry: Option<ResourceFilesystemEntry>,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.filesystem_expanded_index() == Some(index);
        let theme = tokens.ui;
        let kind = host_filesystem_kind_display(i18n, &entry.kind);
        let usage = host_filesystem_usage_label(i18n, &entry);
        let inode = host_filesystem_percent_dash(&entry.inode_percent);
        let size = host_filesystem_size_label(&entry.size_bytes);
        let read_only = host_filesystem_read_only_display(i18n, entry.read_only);

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
                            .font_family(mono_font_family.clone())
                            .child(host_filesystem_blank_dash(&entry.path)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_FILESYSTEM_KIND_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
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
                                .font_family(mono_font_family.clone())
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
                                .font_family(mono_font_family.clone())
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
                                .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
                            .child(host_filesystem_meta_label(
                                i18n,
                                &entry,
                                show_context_columns,
                            )),
                    )
                    .child(self.render_host_filesystem_attention_badges(&entry, tokens, i18n))
                    .child(self.render_host_filesystem_inline_actions(
                        connection_id,
                        &entry,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .when(expanded, |row| {
                row.child(self.render_host_filesystem_detail(
                    &entry,
                    tokens,
                    i18n,
                    mono_font_family,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Expansion is local view state and must not trigger a remote scan.
                    host_tools.toggle_filesystem_expanded(index, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_filesystem_inline_actions(
        &self,
        connection_id: &str,
        entry: &ResourceFilesystemEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let path = entry.path.clone();
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(host_tools_tooltip_icon_button(
                tokens,
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
                i18n.t("sidebar.host_filesystems.actions.copy_path"),
                "host-filesystem-copy-path",
                true,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.copy_host_filesystem_path(path.clone(), cx);
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                tokens,
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
                i18n.t("sidebar.host_filesystems.actions.diagnostic"),
                "host-filesystem-row-diagnostic",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    let i18n = i18n.clone();
                    move |host_tools, _event, window, cx| {
                        host_tools.dispatch_host_filesystem_diagnostic(
                            connection_id.clone(),
                            &i18n,
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
            ))
            .into_any_element()
    }

    fn render_host_filesystem_attention_badges(
        &self,
        entry: &ResourceFilesystemEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let keys = filesystem_attention_label_keys(entry);
        if keys.is_empty() {
            return div().into_any_element();
        }
        let severity = filesystem_entry_severity(entry);
        let color = match severity {
            FilesystemEntrySeverity::Critical => MONITOR_RED,
            FilesystemEntrySeverity::Warning => MONITOR_AMBER,
            FilesystemEntrySeverity::Normal => tokens.ui.text_muted,
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
                    .child(i18n.t(key)),
            );
        }
        row.into_any_element()
    }

    fn render_host_filesystem_detail(
        &self,
        entry: &ResourceFilesystemEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
    ) -> AnyElement {
        let theme = tokens.ui;
        div()
            .mx_3()
            .mb_2()
            .rounded(px(tokens.radii.md))
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
                    .font_family(mono_font_family)
                    .text_size(px(HOST_PROCESS_DETAIL_TEXT_SIZE))
                    .text_color(rgb(theme.text))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.path"),
                        host_filesystem_blank_dash(&entry.path)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.kind"),
                        host_filesystem_kind_display(i18n, &entry.kind)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.device"),
                        host_filesystem_blank_dash(&entry.device)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.fs"),
                        host_filesystem_blank_dash(&entry.fs_type)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.size"),
                        host_filesystem_size_label(&entry.size_bytes)
                    ))
                    .child(format!(
                        "{}: {} / {}",
                        i18n.t("sidebar.host_filesystems.columns.used_available"),
                        host_filesystem_size_label(&entry.used_bytes),
                        host_filesystem_size_label(&entry.available_bytes)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.usage"),
                        host_filesystem_percent_dash(&entry.used_percent)
                    ))
                    .child(format!(
                        "{}: {} / {} / {}",
                        i18n.t("sidebar.host_filesystems.columns.inode"),
                        host_filesystem_blank_dash(&entry.inode_used),
                        host_filesystem_blank_dash(&entry.inode_available),
                        host_filesystem_percent_dash(&entry.inode_percent)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.read_only"),
                        host_filesystem_read_only_display(i18n, entry.read_only)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.attention"),
                        host_filesystem_attention_summary(i18n, entry)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.source"),
                        host_filesystem_blank_dash(&entry.source)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.detail"),
                        host_filesystem_blank_dash(&entry.detail)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_filesystems.columns.options"),
                        host_filesystem_blank_dash(&entry.options)
                    ))),
            )
            .into_any_element()
    }

    fn copy_host_filesystem_path(&mut self, path: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::FilesystemPathCopied { path },
        ));
        cx.notify();
    }

    fn dispatch_host_filesystem_diagnostic(
        &self,
        connection_id: String,
        i18n: &I18n,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Only the fixed OS-specific builder output may become a terminal command.
        let command = self.filesystem_diagnostic_command(&connection_id);
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title: i18n.t("sidebar.host_filesystems.diagnostic_title"),
                    opened_notice: i18n.t("sidebar.host_filesystems.toast.diagnostic_opened"),
                    missing_notice: i18n.t("sidebar.host_filesystems.toast.exec_terminal_missing"),
                },
            )),
            cx,
        );
    }
}

impl WorkspaceApp {
    pub(super) fn render_host_filesystems_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.selectable_text_render_state(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::FilesystemSearch, cx)
            .expect("filesystem search is a non-secret Host Tools input");
        let sidebar_width = self.ai_entity.read(cx).chat_ui().sidebar_width;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_filesystems_panel(
                search_ime,
                sidebar_width,
                &tokens,
                i18n,
                mono_font_family,
                &selectable_text,
                cx,
            )
        })
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
}

impl HostToolsEntity {
    fn sync_filesystem_render_rows(&self, rows: &[ResourceFilesystemEntry], selected_id: &str) {
        let signatures = rows
            .iter()
            .map(filesystem_row_signature)
            .collect::<Vec<_>>();
        let identity = format!(
            "host-filesystems:{selected_id}:{}:{}:{}",
            self.ui.host_filesystem_search_query,
            self.filesystem_filter() as u8,
            self.filesystem_expanded_index().unwrap_or(usize::MAX)
        );
        self.sync_filesystem_list_signatures(&identity, &signatures);
    }

    pub(super) fn filesystem_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<&ResourceFilesystemSnapshot> {
        self.host_filesystems.snapshot.as_ref().filter(|_| {
            self.host_filesystems.snapshot_connection_id.as_deref() == Some(connection_id)
        })
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

    pub(in crate::workspace::connection_monitor) fn request_filesystem_snapshot(
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
        mut delivery: HostFilesystemSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_filesystems.running.as_ref() != Some(&delivery.request) {
            if let Ok(output) = delivery.result.as_mut() {
                zeroize_host_snapshot_output(output);
            }
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_filesystems.polling = false;
        self.host_filesystems.running = None;
        match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                let mut snapshot = parse_filesystem_snapshot(&output.stdout);
                if matches!(&snapshot.status, ResourceFilesystemStatus::Error { .. }) {
                    snapshot.status = ResourceFilesystemStatus::Error {
                        message: failure_fallback,
                    };
                }
                zeroize_host_snapshot_output(&mut output);
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
            Ok(mut output) => {
                zeroize_host_snapshot_output(&mut output);
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
            Err(()) => {
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

fn host_filesystem_filter_chip(active: bool, tokens: &ThemeTokens) -> Div {
    let theme = tokens.ui;
    // Keep the filter styling independent from WorkspaceApp so Entity listeners
    // can update filesystem state without a reverse workspace dependency.
    div()
        .flex_none()
        .h(px(tokens.metrics.ui_button_sm_height * 0.75))
        .px(px(tokens.spacing.two))
        .flex()
        .items_center()
        .rounded(px(tokens.radii.md))
        .cursor_pointer()
        .bg(if active {
            rgb(theme.bg_hover)
        } else {
            rgba(0x00000000)
        })
        .text_size(px(tokens.metrics.ui_text_xs))
        .text_color(if active {
            rgb(theme.text)
        } else {
            rgb(theme.text_muted)
        })
        .hover(move |chip| chip.bg(rgb(theme.bg_hover)))
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
