//! Owns the packages Host Tool UI and request lifecycle.

use super::*;

impl WorkspaceApp {
    pub(super) fn render_host_packages_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let connections = self.monitor_connections();
        if connections.is_empty() {
            return monitor_center_state(
                self,
                LucideIcon::Archive,
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
            .host_package_snapshot
            .as_ref()
            .filter(|_| {
                self.connection_monitor
                    .host_package_snapshot_connection_id
                    .as_deref()
                    == Some(selected_id)
            });
        let rows = snapshot
            .map(|snapshot| {
                visible_package_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_package_search_query,
                    self.connection_monitor.host_package_filter,
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_package_list_state(&rows, selected_id);

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
                        !self.connection_monitor.host_package_snapshot_polling,
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
                self.connection_monitor.host_package_snapshot_polling,
                status,
                selected_id,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_host_package_search(&self, cx: &mut Context<Self>) -> AnyElement {
        let target = WorkspaceImeTarget::HostPackageSearch;
        let focused = self.connection_monitor.host_package_search_focused;
        let workspace = cx.entity();
        text_input_anchor_probe(
            target.anchor_id(),
            text_input(
                &self.tokens,
                TextInputView {
                    value: &self.connection_monitor.host_package_search_query,
                    placeholder: self.i18n.t("sidebar.host_packages.search_placeholder"),
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
                    this.connection_monitor.host_package_search_focused = true;
                    this.connection_monitor.host_process_search_focused = false;
                    this.connection_monitor.host_process_renice_focused = false;
                    this.connection_monitor.host_docker_search_focused = false;
                    this.connection_monitor.host_service_search_focused = false;
                    this.connection_monitor.host_log_search_focused = false;
                    this.connection_monitor.host_tmux_search_focused = false;
                    this.connection_monitor.host_port_search_focused = false;
                    this.connection_monitor.host_schedule_search_focused = false;
                    this.connection_monitor.host_filesystem_search_focused = false;
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
        let active = self.connection_monitor.host_package_filter == filter;
        self.host_tools_filter_chip(active)
            .child(self.i18n.t(package_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.connection_monitor.host_package_filter != filter {
                        this.connection_monitor.host_package_filter = filter;
                        this.connection_monitor.host_package_expanded_index = None;
                    }
                    cx.notify();
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
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::RefreshCw,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 24.0,
                    disabled: self.connection_monitor.host_package_snapshot_polling,
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
                cx.entity(),
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
        let state = self.connection_monitor.host_package_list_state.clone();
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
        let expanded = self.connection_monitor.host_package_expanded_index == Some(index);
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
                    if this.connection_monitor.host_package_expanded_index == Some(index) {
                        this.connection_monitor.host_package_expanded_index = None;
                    } else {
                        this.connection_monitor.host_package_expanded_index = Some(index);
                    }
                    cx.notify();
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
                self.i18n.t("sidebar.host_packages.actions.copy_name"),
                "host-package-copy-name",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.copy_host_package_name(package_name.clone(), cx);
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
                cx.entity(),
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
    ) {
        let signatures = rows.iter().map(package_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-packages:{selected_id}:{}:{}:{}",
            self.connection_monitor.host_package_search_query,
            self.connection_monitor.host_package_filter as u8,
            self.connection_monitor
                .host_package_expanded_index
                .unwrap_or(usize::MAX)
        );
        sync_tauri_variable_list_state_by_signatures(
            &self.connection_monitor.host_package_list_state,
            &mut self.connection_monitor.host_package_list_cache.borrow_mut(),
            &identity,
            &signatures,
            TauriVirtualListSpec::new(px(HOST_PACKAGE_LIST_ESTIMATED_ROW_HEIGHT), 8),
        );
    }

    pub(super) fn host_package_snapshot_command(
        &self,
        connection_id: &str,
    ) -> (oxideterm_connection_monitor::PackageCaptureCommand, String) {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        (build_package_snapshot_command(&os_type), os_type)
    }

    pub(super) fn host_package_inspect_command(
        &self,
        connection_id: &str,
        manager: &str,
        package_name: &str,
    ) -> Result<(oxideterm_connection_monitor::PackageInspectCommand, String), String> {
        let os_type = self
            .ssh_registry
            .get(connection_id)
            .and_then(|handle| handle.remote_env().map(|env| env.os_type))
            .unwrap_or_else(|| "Unknown".to_string());
        build_package_inspect_command(&os_type, manager, package_name)
            .map(|command| (command, os_type))
    }

    pub(in crate::workspace) fn handle_host_package_search_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connection_monitor.host_package_search_focused {
            return false;
        }
        if event.keystroke.key.as_str() == "escape" && !event.keystroke.modifiers.platform {
            self.connection_monitor.host_package_search_focused = false;
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
        self.request_host_packages_snapshot(connection_id, HostSnapshotFeedback::Silent, cx);
    }

    pub(super) fn request_host_packages_snapshot(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        if self.connection_monitor.host_package_snapshot_polling {
            if feedback.should_toast() {
                self.push_host_package_toast(
                    self.i18n
                        .t("sidebar.host_packages.toast.snapshot_already_running"),
                    TerminalNoticeVariant::Warning,
                );
            }
            return;
        }
        let Some(handle) = self.ssh_registry.get(&connection_id) else {
            if feedback.should_toast() {
                self.push_host_package_toast(
                    self.i18n
                        .t("sidebar.host_packages.toast.connection_missing"),
                    TerminalNoticeVariant::Error,
                );
            }
            cx.notify();
            return;
        };
        let (command, _os_type) = self.host_package_snapshot_command(&connection_id);

        let request = HostPackageSnapshotRequest {
            connection_id: connection_id.clone(),
            feedback,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.connection_monitor.host_package_snapshot_connection_id = Some(connection_id);
        self.connection_monitor.host_package_snapshot_running = Some(request.clone());
        self.connection_monitor.host_package_snapshot_rx = Some(rx);
        self.connection_monitor.host_package_snapshot_polling = true;
        self.connection_monitor.host_package_last_error = None;
        // Package inventory is snapshot-driven and read-only. Keep it outside
        // the metric profiler so package managers are not queried on every tick.
        self.forwarding_runtime.handle().spawn(async move {
            let result = handle
                .run_command_capture(
                    &command.command,
                    HOST_PACKAGE_SNAPSHOT_TIMEOUT,
                    HOST_PACKAGE_SNAPSHOT_MAX_OUTPUT_SIZE,
                )
                .await
                .map_err(|error| error.to_string());
            let _ = tx.send(HostPackageSnapshotDelivery { request, result });
        });
        cx.notify();
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
        let (command, _os_type) =
            match self.host_package_inspect_command(&connection_id, &entry.manager, &entry.name) {
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
        let Some(node) = self.ssh_nodes.get(&node_id).cloned() else {
            self.push_host_package_toast(
                self.i18n
                    .t("sidebar.host_packages.toast.exec_terminal_missing"),
                TerminalNoticeVariant::Error,
            );
            cx.notify();
            return;
        };
        match self.queue_ssh_terminal_tab_for_node_with_mark_used(
            node_id,
            Some(command.command),
            node.config,
            title,
            node.saved_connection_id,
            None,
            None,
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

    pub(in crate::workspace) fn poll_host_packages_snapshot_results(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if !self.connection_monitor.host_package_snapshot_polling {
            return;
        }
        let Some(rx) = self.connection_monitor.host_package_snapshot_rx.take() else {
            self.connection_monitor.host_package_snapshot_polling = false;
            self.connection_monitor.host_package_snapshot_running = None;
            return;
        };
        match rx.try_recv() {
            Ok(delivery) => {
                self.finish_host_packages_snapshot(delivery, cx);
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                self.connection_monitor.host_package_snapshot_rx = Some(rx);
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let feedback = self
                    .connection_monitor
                    .host_package_snapshot_running
                    .as_ref()
                    .map(|request| request.feedback)
                    .unwrap_or(HostSnapshotFeedback::Silent);
                self.connection_monitor.host_package_snapshot_polling = false;
                self.connection_monitor.host_package_snapshot_running = None;
                let reason = self.i18n.t("sidebar.host_packages.toast.unknown_error");
                self.connection_monitor.host_package_last_error = Some(reason.clone());
                if feedback.should_toast() {
                    self.push_host_package_toast(
                        self.i18n_replace(
                            "sidebar.host_packages.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
                cx.notify();
            }
        }
    }

    pub(super) fn finish_host_packages_snapshot(
        &mut self,
        delivery: HostPackageSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self
            .connection_monitor
            .host_package_snapshot_running
            .as_ref()
            .is_some_and(|running| running != &delivery.request)
        {
            cx.notify();
            return;
        }
        let feedback = delivery.request.feedback;
        self.connection_monitor.host_package_snapshot_polling = false;
        self.connection_monitor.host_package_snapshot_running = None;
        self.connection_monitor.host_package_snapshot_rx = None;
        match delivery.result {
            Ok(output) if output.exit_code.unwrap_or(0) == 0 => {
                let snapshot = parse_package_snapshot(&output.stdout);
                let visible_count = visible_package_rows(
                    &snapshot.entries,
                    &self.connection_monitor.host_package_search_query,
                    self.connection_monitor.host_package_filter,
                )
                .len();
                match &snapshot.status {
                    ResourcePackageStatus::Available { .. } => {
                        self.connection_monitor.host_package_last_error = None;
                        if feedback.should_toast() {
                            self.push_host_package_toast(
                                self.i18n_replace(
                                    "sidebar.host_packages.toast.snapshot_loaded",
                                    &[("count", visible_count.to_string())],
                                ),
                                TerminalNoticeVariant::Success,
                            );
                        }
                    }
                    ResourcePackageStatus::Unavailable => {
                        self.connection_monitor.host_package_last_error =
                            Some(self.i18n.t("sidebar.host_packages.unavailable"));
                        if feedback.should_toast() {
                            self.push_host_package_toast(
                                self.i18n.t("sidebar.host_packages.toast.unavailable"),
                                TerminalNoticeVariant::Warning,
                            );
                        }
                    }
                    ResourcePackageStatus::Error { message } => {
                        self.connection_monitor.host_package_last_error = Some(message.clone());
                        if feedback.should_toast() {
                            self.push_host_package_toast(
                                self.i18n_replace(
                                    "sidebar.host_packages.toast.snapshot_failed",
                                    &[("reason", message.clone())],
                                ),
                                TerminalNoticeVariant::Error,
                            );
                        }
                    }
                    ResourcePackageStatus::Unknown => {}
                }
                self.connection_monitor.host_package_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_package_snapshot = Some(snapshot);
            }
            Ok(output) => {
                let reason = host_package_capture_failure_message(
                    &output.stdout,
                    &output.stderr,
                    output.exit_code,
                    self.i18n.t("sidebar.host_packages.toast.unknown_error"),
                );
                self.connection_monitor.host_package_last_error = Some(reason.clone());
                self.connection_monitor.host_package_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_package_snapshot = Some(ResourcePackageSnapshot {
                    status: ResourcePackageStatus::Error {
                        message: reason.clone(),
                    },
                    managers: Vec::new(),
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    self.push_host_package_toast(
                        self.i18n_replace(
                            "sidebar.host_packages.toast.snapshot_failed",
                            &[("reason", reason)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
            }
            Err(error) => {
                self.connection_monitor.host_package_last_error = Some(error.clone());
                self.connection_monitor.host_package_snapshot_connection_id =
                    Some(delivery.request.connection_id);
                self.connection_monitor.host_package_snapshot = Some(ResourcePackageSnapshot {
                    status: ResourcePackageStatus::Error {
                        message: error.clone(),
                    },
                    managers: Vec::new(),
                    entries: Vec::new(),
                });
                if feedback.should_toast() {
                    self.push_host_package_toast(
                        self.i18n_replace(
                            "sidebar.host_packages.toast.snapshot_failed",
                            &[("reason", error)],
                        ),
                        TerminalNoticeVariant::Error,
                    );
                }
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

fn host_package_capture_failure_message(
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    fallback: String,
) -> String {
    let reason = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(fallback.as_str());
    match exit_code {
        Some(code) => format!("{reason} (exit {code})"),
        None => reason.to_string(),
    }
}
