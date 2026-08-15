use super::*;

#[derive(Clone)]
pub(super) struct OxideExportConnectionRow {
    id: String,
    name: String,
    meta: String,
    created_at_millis: i64,
}

impl From<&SavedConnection> for OxideExportConnectionRow {
    fn from(connection: &SavedConnection) -> Self {
        let group = connection
            .group
            .as_ref()
            .map(|group| format!(" [{group}]"))
            .unwrap_or_default();
        Self {
            id: connection.id.clone(),
            name: connection.name.clone(),
            meta: format!(
                "{}@{}:{}{group}",
                connection.username, connection.host, connection.port
            ),
            created_at_millis: connection.created_at.timestamp_millis(),
        }
    }
}

#[derive(Clone)]
struct OxideExportForwardRow {
    id: String,
    description: String,
    summary: String,
}

#[derive(Clone)]
pub(super) struct OxideExportForwardGroupRow {
    owner: String,
    forwards: Arc<[OxideExportForwardRow]>,
}

pub(super) fn oxide_export_forward_group_rows(
    connections: &[SavedConnection],
    forwards: &[PersistedForward],
) -> Arc<[OxideExportForwardGroupRow]> {
    let connection_names = connections
        .iter()
        .map(|connection| (connection.id.as_str(), connection.name.as_str()))
        .collect::<HashMap<_, _>>();
    let mut groups: HashMap<String, Vec<OxideExportForwardRow>> = HashMap::new();
    for forward in forwards {
        let owner = forward
            .owner_connection_id
            .as_deref()
            .map(|id| connection_names.get(id).copied().unwrap_or(id))
            .unwrap_or("-");
        groups
            .entry(owner.to_string())
            .or_default()
            .push(OxideExportForwardRow {
                id: forward.id.clone(),
                description: oxide_forward_description_or_summary(forward),
                summary: oxide_forward_summary(forward),
            });
    }
    let mut rows = groups
        .into_iter()
        .map(|(owner, forwards)| OxideExportForwardGroupRow {
            owner,
            forwards: forwards.into(),
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| left.owner.cmp(&right.owner));
    rows.into()
}

#[derive(Clone)]
struct OxideExportConnectionRowRenderer {
    session_manager: Entity<SessionManagerState>,
    tokens: ThemeTokens,
    new_label: String,
}

impl OxideExportConnectionRowRenderer {
    fn render(&self, row: OxideExportConnectionRow, index: usize, cx: &mut App) -> AnyElement {
        let (checked, is_new_since_last_export) = {
            let manager = self.session_manager.read(cx);
            let dialog = manager.oxide_export_dialog.as_ref();
            (
                dialog.is_some_and(|dialog| dialog.selected_ids.contains(&row.id)),
                dialog
                    .and_then(|dialog| dialog.last_export_timestamp)
                    .is_some_and(|timestamp| row.created_at_millis > timestamp),
            )
        };
        let row_id = row.id.clone();
        let checkbox_id = row.id.clone();
        let row_manager = self.session_manager.clone();
        let checkbox_manager = self.session_manager.clone();
        let theme = self.tokens.ui;
        div()
            .px(px(8.0))
            .when(index == 0, |item| item.pt(px(8.0)))
            .pb(px(4.0))
            .child(
                div()
                    .p(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(self.tokens.radii.sm))
                    .hover(move |item| item.bg(rgb(theme.bg_hover)))
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                        toggle_oxide_export_connection(&row_manager, &row_id, cx);
                        cx.stop_propagation();
                    })
                    .child(
                        checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                            MouseButton::Left,
                            move |_event, _window, cx| {
                                toggle_oxide_export_connection(&checkbox_manager, &checkbox_id, cx);
                                cx.stop_propagation();
                            },
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_sm))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text))
                                    .child(row.name)
                                    .when(is_new_since_last_export, |item| {
                                        item.child(
                                            div()
                                                .px(px(6.0))
                                                .py(px(2.0))
                                                .rounded_full()
                                                .bg(rgba(
                                                    (OXIDE_GREEN_500 << 8)
                                                        | OXIDE_NEW_BADGE_BG_ALPHA,
                                                ))
                                                .flex()
                                                .items_center()
                                                .gap(px(2.0))
                                                .text_size(px(10.0))
                                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                                .text_color(rgb(OXIDE_GREEN_500))
                                                .child(WorkspaceApp::render_lucide_icon(
                                                    LucideIcon::Sparkles,
                                                    10.0,
                                                    rgb(OXIDE_GREEN_500),
                                                ))
                                                .child(self.new_label.clone()),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(self.tokens.metrics.ui_text_xs))
                                    .text_color(rgb(theme.text_muted))
                                    .child(row.meta),
                            ),
                    ),
            )
            .into_any_element()
    }
}

fn toggle_oxide_export_connection(
    session_manager: &Entity<SessionManagerState>,
    connection_id: &str,
    cx: &mut App,
) {
    session_manager.update(cx, |manager, cx| {
        if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
            if !dialog.selected_ids.remove(connection_id) {
                dialog.selected_ids.insert(connection_id.to_string());
            }
            cx.emit(SessionManagerWorkspaceEvent::RefreshOxideExportPreflight);
            cx.notify();
        }
    });
}

#[derive(Clone)]
struct OxideExportForwardGroupRenderer {
    session_manager: Entity<SessionManagerState>,
    tokens: ThemeTokens,
}

impl OxideExportForwardGroupRenderer {
    fn render(&self, row: OxideExportForwardGroupRow, cx: &mut App) -> AnyElement {
        let mut group = div().flex().flex_col().gap(px(4.0)).child(
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(self.tokens.ui.text))
                .child(row.owner),
        );
        for forward in row.forwards.iter() {
            let checked = self
                .session_manager
                .read(cx)
                .oxide_export_dialog
                .as_ref()
                .is_some_and(|dialog| dialog.selected_forward_ids.contains(&forward.id));
            let forward_id = forward.id.clone();
            let session_manager = self.session_manager.clone();
            group = group.child(
                div()
                    .px_1()
                    .py(px(4.0))
                    .rounded(px(self.tokens.radii.sm))
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .hover({
                        let hover = self.tokens.ui.bg_hover;
                        move |item| item.bg(rgb(hover))
                    })
                    .cursor_pointer()
                    .child(
                        checkbox(&self.tokens, String::new(), checked).on_mouse_down(
                            MouseButton::Left,
                            move |_event, _window, cx| {
                                session_manager.update(cx, |manager, cx| {
                                    if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                        if !dialog.selected_forward_ids.remove(&forward_id) {
                                            dialog.selected_forward_ids.insert(forward_id.clone());
                                        }
                                        cx.emit(
                                            SessionManagerWorkspaceEvent::RefreshOxideExportPreflight,
                                        );
                                        cx.notify();
                                    }
                                });
                                cx.stop_propagation();
                            },
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .text_size(px(self.tokens.metrics.ui_text_xs))
                            .child(
                                div()
                                    .text_color(rgb(self.tokens.ui.text))
                                    .child(forward.description.clone()),
                            )
                            .child(
                                div()
                                    .text_color(rgb(self.tokens.ui.text_muted))
                                    .child(forward.summary.clone()),
                            ),
                    ),
            );
        }
        div().pb(px(12.0)).child(group).into_any_element()
    }
}

pub(super) fn oxide_export_connection_signature(connection: &SavedConnection) -> u64 {
    let mut hasher = DefaultHasher::new();
    // Export rows are keyed by saved connection id. Other visible fields affect
    // labels/badges and should remeasure the dialog row after edits/imports.
    connection.id.hash(&mut hasher);
    connection.name.hash(&mut hasher);
    connection.username.hash(&mut hasher);
    connection.host.hash(&mut hasher);
    connection.port.hash(&mut hasher);
    connection.group.hash(&mut hasher);
    connection.created_at.timestamp_millis().hash(&mut hasher);
    hasher.finish()
}

fn oxide_export_forward_group_row_signature(row: &OxideExportForwardGroupRow) -> u64 {
    let mut hasher = DefaultHasher::new();
    row.owner.hash(&mut hasher);
    for forward in row.forwards.iter() {
        forward.id.hash(&mut hasher);
        forward.description.hash(&mut hasher);
        forward.summary.hash(&mut hasher);
    }
    hasher.finish()
}

pub(super) fn oxide_export_logical_scroll_changed(
    before_item_ix: usize,
    before_offset: f32,
    after_item_ix: usize,
    after_offset: f32,
) -> bool {
    before_item_ix != after_item_ix || (after_offset - before_offset).abs() >= 0.01
}

pub(super) fn oxide_export_selection_count_label(
    template: String,
    selected: usize,
    total: usize,
) -> String {
    template
        .replace("{{selected}}", &selected.to_string())
        .replace("{{total}}", &total.to_string())
}

pub(super) fn oxide_export_count_label(template: String, count: usize) -> String {
    template.replace("{{count}}", &count.to_string())
}

impl WorkspaceApp {
    pub(super) fn render_oxide_connection_selection(
        &self,
        connections: &[SavedConnection],
        selected_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let total = connections.len();
        let all_selected = total > 0 && selected_count == total;
        let select_connections_label = oxide_export_selection_count_label(
            self.i18n.t("export.select_connections"),
            selected_count,
            total,
        );
        let select_all_label = if all_selected {
            self.i18n.t("export.deselect_all")
        } else {
            self.i18n.t("export.select_all")
        };
        let new_connection_count = self
            .session_manager
            .read(cx)
            .oxide_export_dialog
            .as_ref()
            .and_then(|dialog| dialog.last_export_timestamp)
            .map(|timestamp| {
                connections
                    .iter()
                    .filter(|connection| connection.created_at.timestamp_millis() > timestamp)
                    .count()
            })
            .unwrap_or(0);
        let list = if connections.is_empty() {
            div()
                .id("oxide-export-connections-selection")
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.bg))
                .p(px(8.0))
                .child(
                    div()
                        .py(px(16.0))
                        .text_align(gpui::TextAlign::Center)
                        .text_size(px(self.tokens.metrics.ui_text_sm))
                        .text_color(rgb(theme.text_muted))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "oxide-export-connections",
                            "empty",
                            self.i18n.t("export.no_connections"),
                            theme.text_muted,
                            cx,
                        )),
                )
                .into_any_element()
        } else {
            self.sync_oxide_export_connection_list_state(connections, cx);
            let state = self
                .session_manager
                .read(cx)
                .oxide_export_connection_list_state
                .clone();
            let spec = self.oxide_export_connection_list_spec();
            let renderer = OxideExportConnectionRowRenderer {
                session_manager: self.session_manager.clone(),
                tokens: self.tokens,
                new_label: self.i18n.t("export.badge_new"),
            };
            let rows = self
                .session_manager
                .read(cx)
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| Arc::clone(&dialog.connection_rows))
                .unwrap_or_else(|| Arc::from([]));
            let list_height = (connections.len() as f32
                * OXIDE_EXPORT_CONNECTION_LIST_ESTIMATED_HEIGHT)
                .min(OXIDE_MODAL_LIST_MAX_H);
            div()
                .id("oxide-export-connections-selection")
                .relative()
                .h(px(list_height))
                .rounded(px(self.tokens.radii.md))
                .border_1()
                .border_color(rgb(theme.border))
                .bg(rgb(theme.bg))
                .child(tauri_virtual_list(
                    state,
                    spec,
                    move |index, _window, cx| {
                        rows.get(index)
                            .cloned()
                            .map(|row| renderer.render(row, index, cx))
                            .unwrap_or_else(|| div().into_any_element())
                    },
                ))
                .child(div().absolute().inset_0().on_scroll_wheel({
                    let list_state = self
                        .session_manager
                        .read(cx)
                        .oxide_export_connection_list_state
                        .clone();
                    let session_manager = self.session_manager.clone();
                    move |event: &ScrollWheelEvent, _window, cx| {
                        let delta = event.delta.pixel_delta(px(20.0));
                        let scroll_distance = -f32::from(delta.y);
                        if scroll_distance.abs() < 0.01 {
                            return;
                        }
                        let before = list_state.logical_scroll_top();
                        list_state.scroll_by(px(scroll_distance));
                        let after = list_state.logical_scroll_top();
                        if oxide_export_logical_scroll_changed(
                            before.item_ix,
                            f32::from(before.offset_in_item),
                            after.item_ix,
                            f32::from(after.offset_in_item),
                        ) {
                            session_manager.update(cx, |_manager, cx| cx.notify());
                            cx.stop_propagation();
                        }
                    }
                }))
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(theme.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "oxide-export-selection",
                                "connection-count",
                                select_connections_label,
                                theme.text,
                                cx,
                            )),
                    )
                    .child(
                        // Tauri OxideExportModal renders select-all as an
                        // outline h-7 text-xs Button. Route through the shared
                        // toolbar primitive so disabled/focus behavior matches
                        // the rest of the dialog actions.
                        self.workspace_toolbar_action_button(
                            select_all_label,
                            None,
                            ToolbarButtonOptions {
                                button: ButtonOptions {
                                    variant: ButtonVariant::Outline,
                                    size: ButtonSize::Sm,
                                    radius: ButtonRadius::Md,
                                    disabled: total == 0,
                                },
                                height: Some(OXIDE_SELECT_ALL_BUTTON_HEIGHT),
                                font_size: Some(self.tokens.metrics.ui_text_xs),
                                ..ToolbarButtonOptions::default()
                            },
                            cx.listener(move |this, _event, _window, cx| {
                                let all_ids = this
                                    .connection_store
                                    .connections()
                                    .iter()
                                    .map(|connection| connection.id.clone())
                                    .collect::<HashSet<_>>();
                                this.session_manager.update(cx, |manager, cx| {
                                    if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                        if dialog.selected_ids.len() == all_ids.len() {
                                            dialog.selected_ids.clear();
                                        } else {
                                            dialog.selected_ids = all_ids;
                                        }
                                    }
                                    cx.notify();
                                });
                                this.refresh_oxide_export_preflight(cx);
                                cx.stop_propagation();
                            }),
                        ),
                    ),
            )
            .when(new_connection_count > 0, |section| {
                section.child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(OXIDE_GREEN_500))
                        .child(Self::render_lucide_icon(
                            LucideIcon::Sparkles,
                            12.0,
                            rgb(OXIDE_GREEN_500),
                        ))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "oxide-export-selection",
                            "new-connections",
                            oxide_export_count_label(
                                self.i18n.t("export.new_since_last_export"),
                                new_connection_count,
                            ),
                            OXIDE_GREEN_500,
                            cx,
                        )),
                )
            })
            .child(list)
            .into_any_element()
    }

    pub(super) fn sync_oxide_export_connection_list_state(
        &self,
        connections: &[SavedConnection],
        cx: &App,
    ) {
        let signatures = connections
            .iter()
            .map(oxide_export_connection_signature)
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.oxide_export_connection_list_state,
            &mut manager.oxide_export_connection_list_cache.borrow_mut(),
            "oxide-export-connections",
            &signatures,
            self.oxide_export_connection_list_spec(),
        );
    }

    pub(super) fn oxide_export_connection_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(OXIDE_EXPORT_CONNECTION_LIST_ESTIMATED_HEIGHT),
            OXIDE_EXPORT_CONNECTION_LIST_OVERSCAN,
        )
    }

    pub(super) fn render_oxide_export_options(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some((
            include_app_settings,
            selected_app_settings_sections,
            include_local_terminal_env_vars,
            include_quick_commands,
            include_serial_profiles,
            include_telnet_profiles,
            include_mosh_profiles,
            include_remote_desktop_profiles,
            include_plugin_settings,
            plugin_groups,
            selected_plugin_ids,
            include_portable_secrets,
        )) = ({
            self.session_manager
                .read(cx)
                .oxide_export_dialog
                .as_ref()
                .map(|dialog| {
                    (
                        dialog.include_app_settings,
                        dialog.selected_app_settings_sections.clone(),
                        dialog.include_local_terminal_env_vars,
                        dialog.include_quick_commands,
                        dialog.include_serial_profiles,
                        dialog.include_telnet_profiles,
                        dialog.include_mosh_profiles,
                        dialog.include_remote_desktop_profiles,
                        dialog.include_plugin_settings,
                        dialog.plugin_groups.clone(),
                        dialog.selected_plugin_ids.clone(),
                        dialog.include_portable_secrets,
                    )
                })
        })
        else {
            return div().into_any_element();
        };
        div()
            .flex()
            .flex_col()
            .gap(px(OXIDE_MODAL_SECTION_GAP))
            .child(self.render_oxide_forward_card(cx))
            .child(self.render_oxide_option_row(
                "包含全局设置".to_string(),
                "导出终端外观、操作习惯和其他 OxideTerm 应用设置。".to_string(),
                include_app_settings,
                cx.listener(|this, _event, _window, cx| {
                    this.session_manager.update(cx, |manager, cx| {
                        if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                            dialog.include_app_settings = !dialog.include_app_settings;
                        }
                        cx.notify();
                    });
                    this.refresh_oxide_export_preflight(cx);
                    cx.stop_propagation();
                }),
                cx,
            ))
            .when(include_app_settings, |options| {
                let mut children = vec![
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                .font_weight(gpui::FontWeight::SEMIBOLD)
                                .text_color(rgb(self.tokens.ui.text))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "oxide-export-app-settings",
                                    "title",
                                    "应用设置分组",
                                    self.tokens.ui.text,
                                    cx,
                                )),
                        )
                        .child(
                            div()
                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                .text_color(rgb(self.tokens.ui.text_muted))
                                .child(self.render_display_text_with_role(
                                    SelectableTextRole::PlainDocument,
                                    "oxide-export-app-settings",
                                    "description",
                                    "选择要包含到 .oxide 文件中的应用设置分组。",
                                    self.tokens.ui.text_muted,
                                    cx,
                                )),
                        )
                        .into_any_element(),
                    self.render_oxide_settings_section_grid(
                        &selected_app_settings_sections,
                        false,
                        cx,
                    ),
                ];
                if selected_app_settings_sections.is_empty() {
                    children.push(self.render_oxide_section_empty_warning(
                        "尚未选择任何应用设置分组".to_string(),
                        cx,
                    ));
                }
                options.child(self.render_oxide_card(None, children, cx))
            })
            .when(
                include_app_settings && selected_app_settings_sections.contains("localTerminal"),
                |options| {
                    options.child(self.render_oxide_card(
                        None,
                        vec![self.render_oxide_option_row(
                            "包含本地终端环境变量".to_string(),
                            "可能包含机器相关或敏感值。".to_string(),
                            include_local_terminal_env_vars,
                            cx.listener(|this, _event, _window, cx| {
                                this.session_manager.update(cx, |manager, cx| {
                                    if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                        dialog.include_local_terminal_env_vars =
                                            !dialog.include_local_terminal_env_vars;
                                    }
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }),
                            cx,
                        )],
                        cx,
                    ))
                },
            )
            .child(self.render_oxide_option_row(
                "包含快捷命令".to_string(),
                "快捷命令可能包含主机名、路径或命令中的敏感信息。".to_string(),
                include_quick_commands,
                cx.listener(|this, _event, _window, cx| {
                    this.session_manager.update(cx, |manager, cx| {
                        if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                            dialog.include_quick_commands = !dialog.include_quick_commands;
                        }
                        cx.notify();
                    });
                    this.refresh_oxide_export_preflight(cx);
                    cx.stop_propagation();
                }),
                cx,
            ))
            .child(
                self.render_oxide_option_row(
                    self.i18n.t("export.include_serial_profiles"),
                    self.i18n
                        .t("export.include_serial_profiles_description")
                        .replace(
                            "{{count}}",
                            &self.connection_store.serial_profiles().len().to_string(),
                        ),
                    include_serial_profiles,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                dialog.include_serial_profiles = !dialog.include_serial_profiles;
                            }
                            cx.notify();
                        });
                        this.refresh_oxide_export_preflight(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ),
            )
            .child(
                self.render_oxide_option_row(
                    self.i18n.t("export.include_telnet_profiles"),
                    self.i18n
                        .t("export.include_telnet_profiles_description")
                        .replace(
                            "{{count}}",
                            &self.connection_store.telnet_profiles().len().to_string(),
                        ),
                    include_telnet_profiles,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                dialog.include_telnet_profiles = !dialog.include_telnet_profiles;
                            }
                            cx.notify();
                        });
                        this.refresh_oxide_export_preflight(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ),
            )
            .child(
                self.render_oxide_option_row(
                    self.i18n.t("export.include_mosh_profiles"),
                    self.i18n
                        .t("export.include_mosh_profiles_description")
                        .replace(
                            "{{count}}",
                            &self.connection_store.mosh_profiles().len().to_string(),
                        ),
                    include_mosh_profiles,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                dialog.include_mosh_profiles = !dialog.include_mosh_profiles;
                            }
                            cx.notify();
                        });
                        this.refresh_oxide_export_preflight(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ),
            )
            .child(
                self.render_oxide_option_row(
                    self.i18n.t("export.include_remote_desktop_profiles"),
                    self.i18n
                        .t("export.include_remote_desktop_profiles_description")
                        .replace(
                            "{{count}}",
                            &self
                                .connection_store
                                .remote_desktop_profiles()
                                .len()
                                .to_string(),
                        ),
                    include_remote_desktop_profiles,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                dialog.include_remote_desktop_profiles =
                                    !dialog.include_remote_desktop_profiles;
                            }
                            cx.notify();
                        });
                        this.refresh_oxide_export_preflight(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ),
            )
            .child(self.render_oxide_option_row(
                "包含插件偏好设置".to_string(),
                "导出存放在 OxideTerm 本地存储中的声明式插件 settings。".to_string(),
                include_plugin_settings,
                cx.listener(|this, _event, _window, cx| {
                    this.session_manager.update(cx, |manager, cx| {
                        if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                            dialog.include_plugin_settings = !dialog.include_plugin_settings;
                        }
                        cx.notify();
                    });
                    this.refresh_oxide_export_preflight(cx);
                    cx.stop_propagation();
                }),
                cx,
            ))
            .child(self.render_oxide_export_plugin_settings(
                plugin_groups,
                selected_plugin_ids,
                include_plugin_settings,
                cx,
            ))
            .child(self.render_oxide_option_row(
                "包含便携秘密项".to_string(),
                "导出可在导入时恢复的便携安全秘密项，例如 AI 提供商密钥。".to_string(),
                include_portable_secrets,
                cx.listener(|this, _event, _window, cx| {
                    this.session_manager.update(cx, |manager, cx| {
                        if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                            dialog.include_portable_secrets = !dialog.include_portable_secrets;
                        }
                        cx.notify();
                    });
                    this.refresh_oxide_export_preflight(cx);
                    cx.stop_propagation();
                }),
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_oxide_forward_card(&self, cx: &mut Context<Self>) -> AnyElement {
        let available_forward_count = self
            .session_manager
            .read(cx)
            .oxide_export_dialog
            .as_ref()
            .map(|dialog| dialog.available_forwards.len())
            .unwrap_or_default();
        let mut children = vec![
            div()
                .text_size(px(self.tokens.metrics.ui_text_xs))
                .line_height(px(16.0))
                .text_color(rgb(self.tokens.ui.text_muted))
                .child(self.render_display_text_with_role(
                    SelectableTextRole::PlainDocument,
                    "oxide-export-forwards",
                    "description",
                    "所选的已保存端口转发会连同其所属的连接配置一起导出。",
                    self.tokens.ui.text_muted,
                    cx,
                ))
                .into_any_element(),
        ];
        if available_forward_count == 0 {
            children.push(
                div()
                    .text_size(px(self.tokens.metrics.ui_text_xs))
                    .text_color(rgb(self.tokens.ui.text_muted))
                    .child(self.render_display_text_with_role(
                        SelectableTextRole::PlainDocument,
                        "oxide-export-forwards",
                        "empty",
                        "没有已保存的端口转发",
                        self.tokens.ui.text_muted,
                        cx,
                    ))
                    .into_any_element(),
            );
        } else {
            children.push(self.render_oxide_forward_selection(cx));
        }
        self.render_oxide_card(
            Some((
                LucideIcon::Shield,
                format!("已保存的端口转发（{available_forward_count}）"),
            )),
            children,
            cx,
        )
    }

    pub(super) fn render_oxide_forward_selection(&self, cx: &mut Context<Self>) -> AnyElement {
        let rows = self
            .session_manager
            .read(cx)
            .oxide_export_dialog
            .as_ref()
            .map(|dialog| Arc::clone(&dialog.forward_group_rows))
            .unwrap_or_else(|| Arc::from([]));
        self.sync_oxide_export_forward_group_list_state(&rows, cx);
        let state = self
            .session_manager
            .read(cx)
            .oxide_export_forward_group_list_state
            .clone();
        let spec = self.oxide_export_forward_group_list_spec();
        let renderer = OxideExportForwardGroupRenderer {
            session_manager: self.session_manager.clone(),
            tokens: self.tokens,
        };
        let list_height = (rows.len() as f32 * OXIDE_EXPORT_FORWARD_GROUP_LIST_ESTIMATED_HEIGHT)
            .min(OXIDE_MODAL_FORWARDS_MAX_H);
        div()
            .id("oxide-export-forwards-selection")
            .h(px(list_height))
            .child(tauri_virtual_list(
                state,
                spec,
                move |index, _window, cx| {
                    rows.get(index)
                        .cloned()
                        .map(|row| renderer.render(row, cx))
                        .unwrap_or_else(|| div().into_any_element())
                },
            ))
            .into_any_element()
    }

    pub(super) fn sync_oxide_export_forward_group_list_state(
        &self,
        rows: &[OxideExportForwardGroupRow],
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(oxide_export_forward_group_row_signature)
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.oxide_export_forward_group_list_state,
            &mut manager.oxide_export_forward_group_list_cache.borrow_mut(),
            "oxide-export-forward-groups",
            &signatures,
            self.oxide_export_forward_group_list_spec(),
        );
    }

    pub(super) fn oxide_export_forward_group_list_spec(&self) -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(OXIDE_EXPORT_FORWARD_GROUP_LIST_ESTIMATED_HEIGHT),
            OXIDE_EXPORT_FORWARD_GROUP_LIST_OVERSCAN,
        )
    }

    pub(super) fn render_oxide_export_plugin_settings(
        &self,
        plugin_groups: HashMap<String, usize>,
        selected_plugin_ids: HashSet<String>,
        include_plugin_settings: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut entries = plugin_groups
            .iter()
            .map(|(plugin_id, count)| (plugin_id.clone(), *count))
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| left.0.cmp(&right.0));
        if entries.is_empty() {
            return self.render_oxide_card(
                None,
                vec![
                    div()
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(self.tokens.ui.text_muted))
                        .child(self.render_display_text_with_role(
                            SelectableTextRole::PlainDocument,
                            "oxide-export-plugin-settings",
                            "empty",
                            "没有可导出的插件偏好设置",
                            self.tokens.ui.text_muted,
                            cx,
                        ))
                        .into_any_element(),
                ],
                cx,
            );
        }

        let mut children = Vec::new();
        for (plugin_id, count) in entries {
            let selected = selected_plugin_ids.contains(&plugin_id);
            let enabled = include_plugin_settings;
            let row_plugin_id = plugin_id.clone();
            children.push(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .opacity(if enabled { 1.0 } else { 0.6 })
                    .cursor_pointer()
                    .child(self.render_oxide_checkbox(
                        String::new(),
                        selected,
                        cx.listener(move |this, _event, _window, cx| {
                            this.session_manager.update(cx, |manager, cx| {
                                if let Some(dialog) = manager.oxide_export_dialog.as_mut() {
                                    if dialog.selected_plugin_ids.contains(&row_plugin_id) {
                                        dialog.selected_plugin_ids.remove(&row_plugin_id);
                                    } else {
                                        dialog.selected_plugin_ids.insert(row_plugin_id.clone());
                                    }
                                }
                                cx.notify();
                            });
                            cx.stop_propagation();
                        }),
                    ))
                    .child(
                        div()
                            .text_size(px(self.tokens.metrics.ui_text_sm))
                            .text_color(rgb(self.tokens.ui.text))
                            .child(self.render_display_text_with_role(
                                SelectableTextRole::PlainDocument,
                                "oxide-export-plugin-settings",
                                plugin_id.as_str(),
                                format!("{}（{} 项设置）", plugin_id, count),
                                self.tokens.ui.text,
                                cx,
                            )),
                    )
                    .into_any_element(),
            );
        }
        self.render_oxide_card(None, children, cx)
    }
}
