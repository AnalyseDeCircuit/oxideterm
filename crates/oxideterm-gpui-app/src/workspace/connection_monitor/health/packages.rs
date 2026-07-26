//! Owns the packages Host Tool UI and request lifecycle.

use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_packages_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections(cx);
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Archive,
                self.tokens.ui.text_muted,
                self.i18n.t("profiler.panel.no_connection"),
                cx,
            );
        }

        let selected_connection_id = self.host_tools.read(cx).selected_connection_id_owned();
        let selected_id = selected_connection_id
            .as_deref()
            .unwrap_or(connections[0].connection_id.as_str());
        let snapshot = self.host_tools.read(cx).package_snapshot_for(selected_id);
        let filter = self.host_tools.read(cx).package_filter();
        let package_search_query = self
            .host_tools
            .read(cx)
            .ui
            .host_package_search_query
            .clone();
        let rows = snapshot
            .as_ref()
            .map(|snapshot| visible_package_rows(&snapshot.entries, &package_search_query, filter))
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_package_list_state(&rows, selected_id, cx);

        div()
            .id("host-packages-panel")
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
                        !self.host_tools.read(cx).package_snapshot_polling(),
                        cx,
                    ))
                    .child(self.render_host_package_search(cx))
                    .child(self.render_host_package_filter_row(cx))
                    .child(self.render_host_package_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        cx,
                    )),
            )
            .child(self.render_host_package_list(
                rows,
                self.host_tools.read(cx).package_snapshot_polling(),
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_package_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostPackageSearch;
        let (focused, value) = {
            let ui = &self.host_tools.read(cx).ui;
            (
                ui.input_is_focused(HostToolsTextInput::PackageSearch),
                ui.host_package_search_query.clone(),
            )
        };
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &value,
                    placeholder: self.i18n.t("sidebar.host_packages.search_placeholder"),
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
                        host_tools.ui.focus_input(HostToolsTextInput::PackageSearch);
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

    pub(super) fn render_host_package_filter_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut row = div()
            .id("host-package-filter-scroll")
            .flex()
            .items_center()
            .gap_1()
            .overflow_x_scroll();
        for filter in [
            PackageFilter::All,
            PackageFilter::Upgradable,
            PackageFilter::Installed,
            PackageFilter::Services,
            PackageFilter::Apt,
            PackageFilter::Dnf,
            PackageFilter::Yum,
            PackageFilter::Pacman,
            PackageFilter::Brew,
        ] {
            row = row.child(self.render_host_package_filter_chip(filter, cx));
        }
        row.into_any_element()
    }

    pub(super) fn render_host_package_filter_chip(
        &self,
        filter: PackageFilter,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.host_tools.read(cx).package_filter() == filter;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(package_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.select_package_filter(filter, cx);
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_package_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourcePackageStatus,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let capability_label = match status {
            ResourcePackageStatus::Available {
                capability: PackageCommandCapability::Full,
                ..
            } => self.i18n.t("sidebar.host_packages.capability.full"),
            ResourcePackageStatus::Available {
                capability: PackageCommandCapability::Partial,
                ..
            } => self.i18n.t("sidebar.host_packages.capability.partial"),
            _ => self.i18n.t("sidebar.host_packages.capability.unknown"),
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
                self.i18n.t("sidebar.host_packages.count_suffix"),
                capability_label
            )))
            .child(host_tools_tooltip_icon_button(
                &self.tokens,
                LucideIcon::RefreshCw,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 24.0,
                    disabled: self.host_tools.read(cx).package_snapshot_polling(),
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                },
                self.i18n.t("sidebar.host_packages.actions.refresh"),
                "host-package-refresh",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.request_host_packages_snapshot(
                        selected_id.clone(),
                        HostSnapshotFeedback::Toast,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    pub(super) fn render_host_package_list(
        &self,
        rows: Vec<ResourcePackageEntry>,
        loading: bool,
        status: ResourcePackageStatus,
        selected_id: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if loading && rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Archive,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_packages.loading"),
                cx,
            );
        }
        match status {
            ResourcePackageStatus::Unavailable => {
                return monitor_center_state(
                    self,
                    LucideIcon::Archive,
                    self.tokens.ui.text_muted,
                    self.i18n.t("sidebar.host_packages.unavailable"),
                    cx,
                );
            }
            ResourcePackageStatus::Error { message } => {
                return monitor_center_state(
                    self,
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    self.i18n_replace("sidebar.host_packages.error", &[("error", message)]),
                    cx,
                );
            }
            ResourcePackageStatus::Unknown | ResourcePackageStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Archive,
                self.tokens.ui.text_muted,
                self.i18n.t("sidebar.host_packages.empty"),
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.host_tools.read(cx).package_list_state();
        let spec = TauriVirtualListSpec::new(px(HOST_PACKAGE_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let workspace = cx.entity();
        let show_context_columns =
            self.ai.chat.sidebar_width >= HOST_PACKAGE_CONTEXT_COLUMNS_MIN_WIDTH;
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(self.render_host_package_table_header(show_context_columns))
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
                                this.render_host_package_row(
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

    pub(super) fn render_host_package_table_header(
        &self,
        show_context_columns: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .h(px(HOST_PACKAGE_TABLE_HEADER_HEIGHT))
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
                    .child(self.i18n.t("sidebar.host_packages.columns.package")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PACKAGE_STATUS_COLUMN_WIDTH))
                    .truncate()
                    .child(self.i18n.t("sidebar.host_packages.columns.status")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PACKAGE_VERSION_COLUMN_WIDTH))
                    .truncate()
                    .child(self.i18n.t("sidebar.host_packages.columns.installed")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PACKAGE_MANAGER_COLUMN_WIDTH))
                    .truncate()
                    .child(self.i18n.t("sidebar.host_packages.columns.manager")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_VERSION_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_packages.columns.candidate")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_SERVICE_COLUMN_WIDTH))
                            .truncate()
                            .child(self.i18n.t("sidebar.host_packages.columns.service")),
                    )
            })
            .into_any_element()
    }

    pub(super) fn render_host_package_row(
        &self,
        connection_id: &str,
        index: usize,
        entry: Option<ResourcePackageEntry>,
        show_context_columns: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.host_tools.read(cx).package_expanded_index() == Some(index);
        let theme = self.tokens.ui;
        let mono_font = settings_mono_font_family(self.settings_store.settings());
        let status = host_package_status_display(&self.i18n, &entry.status);
        let installed = host_package_blank_dash(&entry.installed_version);
        let candidate = host_package_blank_dash(&entry.candidate_version);
        let manager = host_package_blank_dash(&entry.manager);
        let service = host_package_service_label(&entry);

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
                    .h(px(HOST_PACKAGE_TABLE_MAIN_ROW_HEIGHT))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    // Package name is the identity column. Keep it as a
                    // first-level flex child; metadata/actions must not be
                    // able to collapse this into the classic `...` regression.
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_COMMAND_TEXT_SIZE))
                            .text_color(rgb(theme.text))
                            .font_family(mono_font.clone())
                            .child(host_package_blank_dash(&entry.name)),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_STATUS_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(host_package_status_color(
                                &entry.status,
                                theme.text_muted,
                            )))
                            .font_family(mono_font.clone())
                            .child(status),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_VERSION_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(installed),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_MANAGER_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font.clone())
                            .child(manager),
                    )
                    .when(show_context_columns, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .w(px(HOST_PACKAGE_VERSION_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(candidate.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_PACKAGE_SERVICE_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font.clone())
                                .child(service.clone()),
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
                            .child(host_package_meta_label(
                                &self.i18n,
                                &entry,
                                show_context_columns,
                            )),
                    )
                    .child(self.render_host_package_inline_actions(connection_id, &entry, cx)),
            )
            .when(expanded, |row| {
                row.child(self.render_host_package_detail(&entry))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    this.host_tools.update(cx, |host_tools, cx| {
                        host_tools.toggle_package_expanded(index, cx);
                    });
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_host_package_inline_actions(
        &self,
        connection_id: &str,
        entry: &ResourcePackageEntry,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let package_name = entry.name.clone();
        let inspect_entry = entry.clone();
        div()
            .flex_none()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(4.0))
            .child(host_tools_tooltip_icon_button(
                &self.tokens,
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
                self.i18n.t("sidebar.host_packages.actions.copy_name"),
                "host-package-copy-name",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.copy_host_package_name(package_name.clone(), cx);
                    cx.stop_propagation();
                }),
            ))
            .child(host_tools_tooltip_icon_button(
                &self.tokens,
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
                self.i18n.t("sidebar.host_packages.actions.inspect"),
                "host-package-row-inspect",
                true,
                cx.listener({
                    let connection_id = connection_id.to_string();
                    move |this, _event, window, cx| {
                        this.open_host_package_inspect_terminal(
                            connection_id.clone(),
                            inspect_entry.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
            ))
            .into_any_element()
    }

    pub(super) fn render_host_package_detail(&self, entry: &ResourcePackageEntry) -> AnyElement {
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
                        self.i18n.t("sidebar.host_packages.columns.package"),
                        host_package_blank_dash(&entry.name)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.status"),
                        host_package_status_display(&self.i18n, &entry.status)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.manager"),
                        host_package_blank_dash(&entry.manager)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.installed"),
                        host_package_blank_dash(&entry.installed_version)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.candidate"),
                        host_package_blank_dash(&entry.candidate_version)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.arch"),
                        host_package_blank_dash(&entry.arch)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.repository"),
                        host_package_blank_dash(&entry.repository)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.service"),
                        host_package_service_label(entry)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.owner_paths"),
                        host_package_owner_paths_label(entry)
                    ))
                    .child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.source"),
                        host_package_blank_dash(&entry.source)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        self.i18n.t("sidebar.host_packages.columns.summary"),
                        host_package_blank_dash(&entry.summary)
                    ))),
            )
            .into_any_element()
    }

    pub(super) fn sync_host_package_list_state(
        &self,
        rows: &[ResourcePackageEntry],
        selected_id: &str,
        cx: &mut Context<Self>,
    ) {
        let signatures = rows.iter().map(package_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-packages:{selected_id}:{}:{}:{}",
            self.host_tools.read(cx).ui.host_package_search_query,
            self.host_tools.read(cx).package_filter() as u8,
            self.host_tools
                .read(cx)
                .package_expanded_index()
                .unwrap_or(usize::MAX)
        );
        self.host_tools
            .read(cx)
            .sync_package_list_signatures(&identity, &signatures);
    }

    pub(in crate::workspace) fn handle_host_package_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .host_tools
            .read(cx)
            .ui
            .input_is_focused(HostToolsTextInput::PackageSearch)
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

    pub(super) fn request_host_packages_snapshot_for_selected_connection(
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
        self.request_host_packages_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_host_packages_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        let monitoring_enabled = self.host_tool_monitoring_enabled(ContextSidebarTool::Packages);
        let runtime = self.forwarding_runtime.handle().clone();
        let failure_fallback = self.i18n.t("sidebar.host_packages.toast.unknown_error");
        let notices = self.host_tools.update(cx, |host_tools, cx| {
            host_tools.request_package_snapshot(
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

    pub(super) fn copy_host_package_name(&mut self, package_name: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(package_name.clone()));
        self.push_host_package_toast(
            self.i18n_replace(
                "sidebar.host_packages.toast.copied_name",
                &[("name", package_name)],
            ),
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(super) fn open_host_package_inspect_terminal(
        &mut self,
        connection_id: String,
        entry: ResourcePackageEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self.host_tools.read(cx).package_inspect_command(
            &connection_id,
            &entry.manager,
            &entry.name,
        ) {
            Ok(command) => command,
            Err(_error) => {
                self.push_host_package_toast(
                    self.i18n_replace(
                        "sidebar.host_packages.toast.inspect_unsupported",
                        &[("manager", host_package_blank_dash(&entry.manager))],
                    ),
                    TerminalNoticeVariant::Error,
                );
                cx.notify();
                return;
            }
        };
        let title = format!(
            "{}: {}",
            self.i18n.t("sidebar.host_packages.inspect_title"),
            entry.name
        );
        let Some(node_id) = self.node_router.node_id_for_connection(&connection_id) else {
            self.push_host_package_toast(
                self.i18n
                    .t("sidebar.host_packages.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        if !self.ssh_nodes.contains_key(&node_id) {
            self.push_host_package_toast(
                self.i18n
                    .t("sidebar.host_packages.toast.exec_terminal_missing"),
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
            Ok(()) => self.push_host_package_toast(
                self.i18n_replace(
                    "sidebar.host_packages.toast.inspect_opened",
                    &[("name", entry.name)],
                ),
                TerminalNoticeVariant::Success,
            ),
            Err(error) => {
                self.push_host_package_toast(error.to_string(), TerminalNoticeVariant::Error)
            }
        }
        cx.notify();
    }

    pub(super) fn push_host_package_toast(
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
    pub(super) fn package_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<ResourcePackageSnapshot> {
        self.host_packages
            .snapshot
            .as_ref()
            .filter(|_| self.host_packages.snapshot_connection_id.as_deref() == Some(connection_id))
            .cloned()
    }

    pub(super) fn package_snapshot_polling(&self) -> bool {
        self.host_packages.polling
    }

    pub(in crate::workspace::connection_monitor) fn package_filter(&self) -> PackageFilter {
        self.host_packages.filter
    }

    pub(super) fn package_list_state(&self) -> ListState {
        self.host_packages.list_state.clone()
    }

    pub(in crate::workspace::connection_monitor) fn package_expanded_index(&self) -> Option<usize> {
        self.host_packages.expanded_index
    }

    pub(in crate::workspace::connection_monitor) fn select_package_filter(
        &mut self,
        filter: PackageFilter,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.host_packages.filter == filter {
            return false;
        }
        self.host_packages.filter = filter;
        self.host_packages.expanded_index = None;
        cx.notify();
        true
    }

    pub(in crate::workspace::connection_monitor) fn toggle_package_expanded(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.host_packages.expanded_index =
            (self.host_packages.expanded_index != Some(index)).then_some(index);
        cx.notify();
    }

    pub(super) fn sync_package_list_signatures(&self, identity: &str, signatures: &[u64]) {
        sync_tauri_variable_list_state_by_signatures(
            &self.host_packages.list_state,
            &mut self.host_packages.list_cache.borrow_mut(),
            identity,
            signatures,
            TauriVirtualListSpec::new(px(HOST_PACKAGE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn package_inspect_command(
        &self,
        connection_id: &str,
        manager: &str,
        package_name: &str,
    ) -> Result<String, String> {
        let os_type = self
            .connection_os_type(connection_id)
            .unwrap_or_else(|| "Unknown".to_string());
        build_package_inspect_command(&os_type, manager, package_name)
            .map(|command| command.command)
    }

    pub(in crate::workspace::connection_monitor) fn request_package_snapshot(
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
        if self.host_packages.polling {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PackageSnapshotAlreadyRunning)
                .into_iter()
                .collect();
        }
        let Some(os_type) = self.connection_os_type(&connection_id) else {
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PackageConnectionMissing)
                .into_iter()
                .collect();
        };
        let command = build_package_snapshot_command(&os_type);
        let request = HostPackageSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
            failure_fallback,
        };
        self.host_packages.snapshot_connection_id = Some(connection_id);
        self.host_packages.running = Some(request.clone());
        self.host_packages.polling = true;
        // Package inventory is read-only manual work, not a periodic sampler.
        let spawned = self.spawn_package_snapshot_capture(
            command.command,
            request,
            HOST_PACKAGE_SNAPSHOT_TIMEOUT,
            HOST_PACKAGE_SNAPSHOT_MAX_OUTPUT_SIZE,
            runtime,
        );
        if !spawned {
            self.host_packages.polling = false;
            self.host_packages.running = None;
            return feedback
                .should_toast()
                .then_some(HostToolsNotice::PackageConnectionMissing)
                .into_iter()
                .collect();
        }
        cx.notify();
        Vec::new()
    }

    pub(in crate::workspace::connection_monitor) fn finish_host_packages_snapshot(
        &mut self,
        delivery: HostPackageSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_packages.running.as_ref() != Some(&delivery.request) {
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_packages.polling = false;
        self.host_packages.running = None;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_package_snapshot(&output.stdout);
                if feedback.should_toast() {
                    match &snapshot.status {
                        ResourcePackageStatus::Available { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::PackageSnapshotLoaded {
                                    count: snapshot.entries.len(),
                                },
                            ));
                        }
                        ResourcePackageStatus::Unavailable => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::PackageUnavailable,
                            ));
                        }
                        ResourcePackageStatus::Error { .. } => {
                            cx.emit(HostToolsEvent::ShowNotice(
                                HostToolsNotice::PackageSnapshotFailed,
                            ));
                        }
                        ResourcePackageStatus::Unknown => {}
                    }
                }
                self.host_packages.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_packages.snapshot = Some(snapshot);
            }
            Ok(_) | Err(()) => {
                self.host_packages.snapshot_connection_id = Some(delivery.request.connection_id);
                self.host_packages.snapshot = Some(ResourcePackageSnapshot {
                    status: ResourcePackageStatus::Error {
                        message: failure_fallback,
                    },
                    managers: Vec::new(),
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    cx.emit(HostToolsEvent::ShowNotice(
                        HostToolsNotice::PackageSnapshotFailed,
                    ));
                }
            }
        }
        cx.notify();
    }
}

fn host_package_blank_dash(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "-" {
        "—".to_string()
    } else {
        trimmed.to_string()
    }
}

fn host_package_status_display(i18n: &I18n, status: &str) -> String {
    let key = package_status_label_key(status);
    if key == "sidebar.host_packages.status.unknown" && !status.trim().is_empty() {
        status.trim().to_string()
    } else {
        i18n.t(key)
    }
}

fn host_package_status_color(status: &str, muted_color: u32) -> u32 {
    match status.trim().to_lowercase().as_str() {
        "upgradable" | "outdated" => MONITOR_AMBER,
        "installed" => MONITOR_EMERALD,
        _ => muted_color,
    }
}

fn host_package_service_label(entry: &ResourcePackageEntry) -> String {
    if entry.service_units.is_empty() {
        "—".to_string()
    } else {
        entry.service_units.join(" · ")
    }
}

fn host_package_owner_paths_label(entry: &ResourcePackageEntry) -> String {
    if entry.owner_paths.is_empty() {
        "—".to_string()
    } else {
        entry.owner_paths.join(" · ")
    }
}

fn host_package_meta_label(
    i18n: &I18n,
    entry: &ResourcePackageEntry,
    show_context_columns: bool,
) -> String {
    if show_context_columns {
        return format!(
            "{} · {}",
            i18n.t("sidebar.host_packages.columns.source"),
            host_package_blank_dash(&entry.source)
        );
    }
    if !entry.summary.trim().is_empty() {
        return entry.summary.clone();
    }
    let repo_or_arch = if !entry.repository.trim().is_empty() {
        entry.repository.as_str()
    } else {
        entry.arch.as_str()
    };
    format!(
        "{} · {}",
        host_package_blank_dash(repo_or_arch),
        host_package_service_label(entry)
    )
}
