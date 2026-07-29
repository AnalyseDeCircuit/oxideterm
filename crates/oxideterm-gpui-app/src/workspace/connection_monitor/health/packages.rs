//! Owns the packages Host Tool UI and request lifecycle.

use super::*;

impl HostToolsEntity {
    #[allow(clippy::too_many_arguments)]
    fn render_host_packages_panel(
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
                LucideIcon::Archive,
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
        let snapshot = self.package_snapshot_for(selected_id);
        let rows = snapshot
            .as_ref()
            .map(|snapshot| {
                visible_package_rows(
                    &snapshot.entries,
                    &self.ui.host_package_search_query,
                    self.package_filter(),
                )
            })
            .unwrap_or_default();
        let status = snapshot
            .as_ref()
            .map(|snapshot| snapshot.status.clone())
            .unwrap_or_default();
        self.sync_host_package_list_state(&rows, selected_id);
        let snapshot_polling = self.package_snapshot_polling();

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
                    .border_color(rgba((tokens.ui.border << 8) | MONITOR_BORDER_ALPHA))
                    .child(self.render_connection_switcher(
                        &connections,
                        selected_id,
                        !snapshot_polling,
                        tokens,
                        mono_font_family.clone(),
                        selectable_text,
                        cx,
                    ))
                    .child(self.render_host_package_search(&search_ime, tokens, i18n, cx))
                    .child(self.render_host_package_filter_row(tokens, i18n, cx))
                    .child(self.render_host_package_status_row(
                        rows.len(),
                        selected_id.to_string(),
                        status.clone(),
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .child(self.render_host_package_list(
                rows,
                snapshot_polling,
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

    fn render_host_package_search(
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
                    value: &self.ui.host_package_search_query,
                    placeholder: i18n.t("sidebar.host_packages.search_placeholder"),
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
                    // Shared window IME selection crosses as a data-only one-shot intent.
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

    fn render_host_package_filter_row(
        &self,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            row = row.child(self.render_host_package_filter_chip(filter, tokens, i18n, cx));
        }
        row.into_any_element()
    }

    fn render_host_package_filter_chip(
        &self,
        filter: PackageFilter,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.package_filter() == filter;
        let theme = tokens.ui;
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
            .child(i18n.t(package_filter_label_key(filter)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Filtering changes only the local package projection.
                    host_tools.select_package_filter(filter, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_package_status_row(
        &self,
        visible_count: usize,
        selected_id: String,
        status: ResourcePackageStatus,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let capability_label = match status {
            ResourcePackageStatus::Available {
                capability: PackageCommandCapability::Full,
                ..
            } => i18n.t("sidebar.host_packages.capability.full"),
            ResourcePackageStatus::Available {
                capability: PackageCommandCapability::Partial,
                ..
            } => i18n.t("sidebar.host_packages.capability.partial"),
            _ => i18n.t("sidebar.host_packages.capability.unknown"),
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
                i18n.t("sidebar.host_packages.count_suffix"),
                capability_label
            )))
            .child(host_tools_tooltip_icon_button(
                tokens,
                LucideIcon::RefreshCw,
                13.0,
                rgb(theme.text),
                oxideterm_gpui_ui::button::IconButtonOptions {
                    size: 24.0,
                    disabled: self.package_snapshot_polling(),
                    has_background: true,
                    background: Some(rgb(theme.bg_hover)),
                    hover_background: Some(rgb(theme.bg_panel)),
                    idle_opacity: 1.0,
                    ..oxideterm_gpui_ui::button::IconButtonOptions::compact(24.0)
                },
                i18n.t("sidebar.host_packages.actions.refresh"),
                "host-package-refresh",
                true,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.request_package_snapshot_from_ui(
                        selected_id.clone(),
                        HostSnapshotFeedback::Toast,
                        cx,
                    );
                    cx.stop_propagation();
                }),
            ))
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_package_list(
        &self,
        rows: Vec<ResourcePackageEntry>,
        loading: bool,
        status: ResourcePackageStatus,
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
                LucideIcon::Archive,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_packages.loading"),
                selectable_text,
                cx,
            );
        }
        match status {
            ResourcePackageStatus::Unavailable => {
                return host_tools_center_state(
                    LucideIcon::Archive,
                    tokens.ui.text_muted,
                    i18n.t("sidebar.host_packages.unavailable"),
                    selectable_text,
                    cx,
                );
            }
            ResourcePackageStatus::Error { message } => {
                return host_tools_center_state(
                    LucideIcon::AlertTriangle,
                    MONITOR_RED,
                    i18n.t("sidebar.host_packages.error")
                        .replace("{{error}}", &message),
                    selectable_text,
                    cx,
                );
            }
            ResourcePackageStatus::Unknown | ResourcePackageStatus::Available { .. } => {}
        }
        if rows.is_empty() {
            return host_tools_center_state(
                LucideIcon::Archive,
                tokens.ui.text_muted,
                i18n.t("sidebar.host_packages.empty"),
                selectable_text,
                cx,
            );
        }

        let rows = Arc::new(rows);
        let selected_id = Arc::new(selected_id.to_string());
        let state = self.package_list_state();
        let spec = TauriVirtualListSpec::new(px(HOST_PACKAGE_LIST_ESTIMATED_ROW_HEIGHT), 8);
        let host_tools = cx.entity();
        let show_context_columns = sidebar_width >= HOST_PACKAGE_CONTEXT_COLUMNS_MIN_WIDTH;
        let row_tokens = *tokens;
        let row_i18n = i18n.clone();
        let row_mono_font_family = mono_font_family.clone();
        div()
            .w_full()
            .min_w_0()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(Self::render_host_package_table_header(
                show_context_columns,
                tokens,
                i18n,
            ))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .child(tauri_virtual_list(
                        state,
                        spec,
                        move |index, _window, cx| {
                            host_tools.update(cx, |host_tools, cx| {
                                host_tools.render_host_package_row(
                                    selected_id.as_str(),
                                    index,
                                    rows.get(index).cloned(),
                                    show_context_columns,
                                    &row_tokens,
                                    &row_i18n,
                                    row_mono_font_family.clone(),
                                    cx,
                                )
                            })
                        },
                    )),
            )
            .into_any_element()
    }

    fn render_host_package_table_header(
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
    ) -> AnyElement {
        let theme = tokens.ui;
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
                    .child(i18n.t("sidebar.host_packages.columns.package")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PACKAGE_STATUS_COLUMN_WIDTH))
                    .truncate()
                    .child(i18n.t("sidebar.host_packages.columns.status")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PACKAGE_VERSION_COLUMN_WIDTH))
                    .truncate()
                    .child(i18n.t("sidebar.host_packages.columns.installed")),
            )
            .child(
                div()
                    .flex_none()
                    .w(px(HOST_PACKAGE_MANAGER_COLUMN_WIDTH))
                    .truncate()
                    .child(i18n.t("sidebar.host_packages.columns.manager")),
            )
            .when(show_context_columns, |header| {
                header
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_VERSION_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_packages.columns.candidate")),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_SERVICE_COLUMN_WIDTH))
                            .truncate()
                            .child(i18n.t("sidebar.host_packages.columns.service")),
                    )
            })
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn render_host_package_row(
        &self,
        connection_id: &str,
        index: usize,
        entry: Option<ResourcePackageEntry>,
        show_context_columns: bool,
        tokens: &ThemeTokens,
        i18n: &I18n,
        mono_font_family: SharedString,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(entry) = entry else {
            return div().into_any_element();
        };
        let expanded = self.package_expanded_index() == Some(index);
        let theme = tokens.ui;
        let status = host_package_status_display(i18n, &entry.status);
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
                            .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
                            .child(status),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_VERSION_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font_family.clone())
                            .child(installed),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(HOST_PACKAGE_MANAGER_COLUMN_WIDTH))
                            .truncate()
                            .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .font_family(mono_font_family.clone())
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
                                .font_family(mono_font_family.clone())
                                .child(candidate.clone()),
                        )
                        .child(
                            div()
                                .flex_none()
                                .w(px(HOST_PACKAGE_SERVICE_COLUMN_WIDTH))
                                .truncate()
                                .text_size(px(HOST_PROCESS_TABLE_VALUE_TEXT_SIZE))
                                .text_color(rgb(theme.text_muted))
                                .font_family(mono_font_family.clone())
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
                            .font_family(mono_font_family.clone())
                            .child(host_package_meta_label(i18n, &entry, show_context_columns)),
                    )
                    .child(self.render_host_package_inline_actions(
                        connection_id,
                        &entry,
                        tokens,
                        i18n,
                        cx,
                    )),
            )
            .when(expanded, |row| {
                row.child(Self::render_host_package_detail(
                    &entry,
                    tokens,
                    i18n,
                    mono_font_family,
                ))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |host_tools, _event, _window, cx| {
                    // Expansion changes only Entity-owned list presentation.
                    host_tools.toggle_package_expanded(index, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    fn render_host_package_inline_actions(
        &self,
        connection_id: &str,
        entry: &ResourcePackageEntry,
        tokens: &ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = tokens.ui;
        let package_name = entry.name.clone();
        let inspect_connection_id = connection_id.to_string();
        let inspect_manager = entry.manager.clone();
        let inspect_package_name = entry.name.clone();
        let inspect_title = format!(
            "{}: {}",
            i18n.t("sidebar.host_packages.inspect_title"),
            entry.name
        );
        let inspect_opened_notice = i18n
            .t("sidebar.host_packages.toast.inspect_opened")
            .replace("{{name}}", &entry.name);
        let inspect_missing_notice = i18n.t("sidebar.host_packages.toast.exec_terminal_missing");
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
                i18n.t("sidebar.host_packages.actions.copy_name"),
                "host-package-copy-name",
                true,
                cx.listener(move |host_tools, _event, _window, cx| {
                    host_tools.copy_host_package_name(package_name.clone(), cx);
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
                i18n.t("sidebar.host_packages.actions.inspect"),
                "host-package-row-inspect",
                true,
                cx.listener({
                    move |host_tools, _event, window, cx| {
                        host_tools.dispatch_host_package_inspect_terminal(
                            inspect_connection_id.clone(),
                            inspect_manager.clone(),
                            inspect_package_name.clone(),
                            inspect_title.clone(),
                            inspect_opened_notice.clone(),
                            inspect_missing_notice.clone(),
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                }),
            ))
            .into_any_element()
    }

    fn render_host_package_detail(
        entry: &ResourcePackageEntry,
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
                        i18n.t("sidebar.host_packages.columns.package"),
                        host_package_blank_dash(&entry.name)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.status"),
                        host_package_status_display(i18n, &entry.status)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.manager"),
                        host_package_blank_dash(&entry.manager)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.installed"),
                        host_package_blank_dash(&entry.installed_version)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.candidate"),
                        host_package_blank_dash(&entry.candidate_version)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.arch"),
                        host_package_blank_dash(&entry.arch)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.repository"),
                        host_package_blank_dash(&entry.repository)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.service"),
                        host_package_service_label(entry)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.owner_paths"),
                        host_package_owner_paths_label(entry)
                    ))
                    .child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.source"),
                        host_package_blank_dash(&entry.source)
                    ))
                    .child(div().pt_2().whitespace_nowrap().child(format!(
                        "{}: {}",
                        i18n.t("sidebar.host_packages.columns.summary"),
                        host_package_blank_dash(&entry.summary)
                    ))),
            )
            .into_any_element()
    }

    fn sync_host_package_list_state(&self, rows: &[ResourcePackageEntry], selected_id: &str) {
        let signatures = rows.iter().map(package_row_signature).collect::<Vec<_>>();
        let identity = format!(
            "host-packages:{selected_id}:{}:{}:{}",
            self.ui.host_package_search_query,
            self.package_filter() as u8,
            self.package_expanded_index().unwrap_or(usize::MAX)
        );
        self.sync_package_list_signatures(&identity, &signatures);
    }

    fn request_package_snapshot_from_ui(
        &mut self,
        connection_id: String,
        feedback: HostSnapshotFeedback,
        cx: &mut Context<Self>,
    ) {
        let (Some(runtime), Some(messages)) =
            (self.lifecycle_runtime.clone(), self.messages.as_ref())
        else {
            return;
        };
        for notice in self.request_package_snapshot(
            connection_id,
            feedback,
            self.monitoring.packages_enabled,
            runtime,
            messages.package_unknown_error.clone(),
            cx,
        ) {
            cx.emit(HostToolsEvent::ShowNotice(notice));
        }
    }

    fn copy_host_package_name(&mut self, package_name: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(package_name.clone()));
        cx.emit(HostToolsEvent::ShowNotice(
            HostToolsNotice::PackageNameCopied { package_name },
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_host_package_inspect_terminal(
        &mut self,
        connection_id: String,
        manager: String,
        package_name: String,
        title: String,
        opened_notice: String,
        missing_notice: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let command = match self.package_inspect_command(&connection_id, &manager, &package_name) {
            Ok(command) => command,
            Err(_) => {
                cx.emit(HostToolsEvent::ShowNotice(
                    HostToolsNotice::PackageInspectUnsupported {
                        manager: host_package_blank_dash(&manager),
                    },
                ));
                return;
            }
        };
        // The fixed inspect command moves into the shared one-shot terminal boundary.
        window.dispatch_action(
            Box::new(HostToolsWindowRequest::new(
                HostToolsWindowIntent::OpenExistingNodeTerminal {
                    connection_id,
                    command,
                    title,
                    opened_notice,
                    missing_notice,
                },
            )),
            cx,
        );
    }
}

impl WorkspaceApp {
    pub(super) fn render_host_packages_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = self.tokens;
        let i18n = &self.i18n;
        let mono_font_family = settings_mono_font_family(self.settings_store.settings());
        let selectable_text = self.selectable_text_render_state(cx);
        let search_ime = self
            .host_tools_plain_text_ime_frame(HostToolsTextInput::PackageSearch, cx)
            .expect("package search is a non-secret Host Tools input");
        let sidebar_width = self.ai_entity.read(cx).chat_ui().sidebar_width;
        self.host_tools.update(cx, |host_tools, cx| {
            host_tools.render_host_packages_panel(
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
}

impl HostToolsEntity {
    pub(super) fn package_snapshot_for(
        &self,
        connection_id: &str,
    ) -> Option<&ResourcePackageSnapshot> {
        self.host_packages
            .snapshot
            .as_ref()
            .filter(|_| self.host_packages.snapshot_connection_id.as_deref() == Some(connection_id))
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
        mut delivery: HostPackageSnapshotDelivery,
        cx: &mut Context<Self>,
    ) {
        if self.host_packages.running.as_ref() != Some(&delivery.request) {
            if let Ok(output) = delivery.result.as_mut() {
                zeroize_host_snapshot_output(output);
            }
            return;
        }
        let feedback = delivery.request.feedback;
        let failure_fallback = delivery.request.failure_fallback.clone();
        self.host_packages.polling = false;
        self.host_packages.running = None;
        match delivery.result {
            Ok(mut output) if output.exit_code.unwrap_or(0) == 0 => {
                let mut snapshot = parse_package_snapshot(&output.stdout);
                if matches!(&snapshot.status, ResourcePackageStatus::Error { .. }) {
                    snapshot.status = ResourcePackageStatus::Error {
                        message: failure_fallback,
                    };
                }
                zeroize_host_snapshot_output(&mut output);
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
            Ok(mut output) => {
                zeroize_host_snapshot_output(&mut output);
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
            Err(()) => {
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

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn package_copy_stays_in_entity_and_emits_structured_notice(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                SshConnectionRegistry::default(),
                cx,
            )
        });
        let mut events = cx.events(&entity);

        entity.update(cx, |host_tools, cx| {
            host_tools.copy_host_package_name("oxideterm-package".to_string(), cx);
        });

        assert_eq!(
            cx.read_from_clipboard().and_then(|item| item.text()),
            Some("oxideterm-package".to_string())
        );
        assert_eq!(
            events.try_recv().unwrap(),
            HostToolsEvent::ShowNotice(HostToolsNotice::PackageNameCopied {
                package_name: "oxideterm-package".to_string(),
            })
        );
    }

    #[gpui::test]
    fn package_filter_and_expansion_do_not_start_remote_capture(cx: &mut TestAppContext) {
        let (profiler_update_tx, profiler_update_rx) = tokio::sync::mpsc::unbounded_channel();
        let entity = cx.new(|cx| {
            HostToolsEntity::new(
                profiler_update_tx,
                profiler_update_rx,
                SshConnectionRegistry::default(),
                cx,
            )
        });

        entity.update(cx, |host_tools, cx| {
            assert!(host_tools.select_package_filter(PackageFilter::Upgradable, cx));
            host_tools.toggle_package_expanded(2, cx);
            assert!(!host_tools.package_snapshot_polling());
            assert!(host_tools.host_packages.running.is_none());
        });
    }
}
