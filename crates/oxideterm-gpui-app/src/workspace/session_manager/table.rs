impl WorkspaceApp {
    fn render_session_manager_table(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let rows = self.filtered_session_connections();
        let all_selected = !rows.is_empty()
            && rows
                .iter()
                .all(|row| self.session_manager.selected_ids.contains(&row.id));
        let empty_message = if self.session_manager.search_query.trim().is_empty() {
            self.i18n.t("sessionManager.table.no_connections")
        } else {
            self.i18n.t("sessionManager.table.no_search_results")
        };
        let row_count = rows.len();
        let virtual_rows = Arc::new(rows.clone());
        let workspace = cx.entity();
        let table_scroll = self.session_manager.table_scroll_handle.clone();
        let virtual_spec = TauriVirtualListSpec::new(
            px(MANAGER_TABLE_VIRTUAL_ROW_HEIGHT),
            MANAGER_TABLE_VIRTUAL_OVERSCAN,
        );
        div()
            .flex_1()
            .min_w(px(0.0))
            .h_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_connection_table_header(has_background, all_selected, cx))
            .child(
                div()
                    .id("session-manager-table-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .when(rows.is_empty(), |body| {
                        body.flex().items_center().justify_center().child(
                            div()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap(px(16.0))
                                .py(px(64.0))
                                .text_color(rgb(theme.text_muted))
                                .child(Self::render_lucide_icon(
                                    LucideIcon::Server,
                                    48.0,
                                    rgba((theme.text_muted << 8) | 0x4d),
                                ))
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .child(
                                            div()
                                                .text_size(px(self.tokens.metrics.ui_text_sm))
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .child(self.render_selectable_display_text(
                                                    "session-manager-table-empty",
                                                    &self.session_manager.search_query,
                                                    empty_message,
                                                    theme.text_muted,
                                                    cx,
                                                )),
                                        )
                                        .child(
                                            div()
                                                .mt_1()
                                                .text_size(px(self.tokens.metrics.ui_text_xs))
                                                .child(self.render_selectable_display_text(
                                                    "session-manager-table-empty-hint",
                                                    "no-connections-hint",
                                                    self.i18n.t(
                                                        "sessionManager.table.no_connections_hint",
                                                    ),
                                                    theme.text_muted,
                                                    cx,
                                                ),
                                                ),
                                        ),
                                )
                                .child(
                                    self.render_toolbar_button(
                                        LucideIcon::Plus,
                                        self.i18n.t("sessionManager.toolbar.new_connection"),
                                        ButtonVariant::Default,
                                        has_background,
                                        true,
                                        cx.listener(|this, _event, window, cx| {
                                            this.open_new_connection_form(window, cx);
                                            cx.stop_propagation();
                                        }),
                                    ),
                                ),
                        )
                    })
                    .when(!rows.is_empty(), |body| {
                        body.child(tauri_virtual_uniform_list(
                            "session-manager-table-virtual",
                            row_count,
                            table_scroll,
                            virtual_spec,
                            move |range, _window, app| {
                                let mut rendered = Vec::new();
                                let rows = virtual_rows.clone();
                                let _ = workspace.update(app, |this, cx| {
                                    for index in range {
                                        let Some(conn) = rows.get(index).cloned() else {
                                            continue;
                                        };
                                        rendered.push(this.render_connection_table_row(
                                            conn,
                                            has_background,
                                            cx,
                                        ));
                                    }
                                });
                                rendered
                            },
                        ))
                    })
            )
            .into_any_element()
    }

    fn render_connection_table_header(
        &self,
        has_background: bool,
        all_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        tauri_table_header(
            &self.tokens,
            self.manager_table_colors(has_background),
            self.manager_table_metrics(),
        )
        .child(tauri_table_checkbox_cell(
            MANAGER_COL_CHECKBOX,
            checkbox(&self.tokens, String::new(), all_selected).on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _event, _window, cx| {
                    this.toggle_all_visible_connections(cx);
                    cx.stop_propagation();
                }),
            ),
        ))
        .child(self.render_sort_header(
            "sessionManager.table.name",
            SessionSortField::Name,
            MANAGER_COL_NAME_BASIS,
            true,
            cx,
        ))
        .child(self.render_sort_header(
            "sessionManager.table.host",
            SessionSortField::Host,
            MANAGER_COL_HOST,
            false,
            cx,
        ))
        .child(self.render_sort_header(
            "sessionManager.table.port",
            SessionSortField::Port,
            MANAGER_COL_PORT,
            false,
            cx,
        ))
        .child(self.render_sort_header(
            "sessionManager.table.username",
            SessionSortField::Username,
            MANAGER_COL_USERNAME,
            false,
            cx,
        ))
        .child(self.render_sort_header(
            "sessionManager.table.auth_type",
            SessionSortField::AuthType,
            MANAGER_COL_AUTH,
            false,
            cx,
        ))
        .child(self.render_sort_header(
            "sessionManager.table.group",
            SessionSortField::Group,
            MANAGER_COL_GROUP,
            false,
            cx,
        ))
        .child(self.render_sort_header(
            "sessionManager.table.last_used",
            SessionSortField::LastUsed,
            MANAGER_COL_LAST_USED,
            false,
            cx,
        ))
        .child(tauri_table_spacer_cell(MANAGER_COL_ACTIONS))
        .into_any_element()
    }

    fn render_sort_header(
        &self,
        label_key: &'static str,
        field: SessionSortField,
        width: f32,
        flexible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.session_manager.sort_field == field;
        let (icon, icon_color) = if active {
            let icon = match self.session_manager.sort_direction {
                SortDirection::Asc => LucideIcon::ArrowUp,
                SortDirection::Desc => LucideIcon::ArrowDown,
            };
            (icon, rgb(0x60a5fa))
        } else {
            (
                LucideIcon::ArrowUpDown,
                rgba((theme.text_muted << 8) | 0x66),
            )
        };
        let options = self.manager_table_cell_options(width, flexible);
        let field_key = match field {
            SessionSortField::Name => "name",
            SessionSortField::Host => "host",
            SessionSortField::Port => "port",
            SessionSortField::Username => "username",
            SessionSortField::AuthType => "auth-type",
            SessionSortField::Group => "group",
            SessionSortField::LastUsed => "last-used",
        };
        let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
            "session-manager-sort-header",
            field_key,
        );
        div()
            .when(options.flexible, |cell| {
                cell.flex_1().min_w(px(options.min_width))
            })
            .when(!options.flexible, |cell| {
                cell.w(px(options.width)).flex_none()
            })
            .pl(px(options.padding_left))
            .flex()
            .items_center()
            .gap(px(4.0))
            .cursor_pointer()
            .hover(move |cell| cell.text_color(rgb(theme.text)))
            .child(div().truncate().child(
                self.render_row_safe_selectable_display_text_in_group(
                    selection_group_id,
                    "session-manager-sort-header-cell",
                    field_key,
                    0,
                    self.i18n.t(label_key),
                    theme.text_muted,
                    None,
                    cx,
                ),
            ))
            .child(Self::render_lucide_icon(icon, 14.0, icon_color))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event, _window, cx| {
                if this.session_manager.sort_field == field {
                    this.session_manager.sort_direction =
                        this.session_manager.sort_direction.toggled();
                } else {
                    this.session_manager.sort_field = field;
                    this.session_manager.sort_direction = SortDirection::Asc;
                }
                cx.notify();
            }),
        )
        .into_any_element()
    }

    fn render_connection_table_row(
        &self,
        conn: ConnectionInfo,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.session_manager.selected_ids.contains(&conn.id);
        let hovered =
            self.session_manager.hovered_connection_id.as_deref() == Some(conn.id.as_str());
        let id = conn.id.clone();
        let selection_group_id = crate::workspace::selectable_text::selectable_text_id(
            "session-manager-table-row",
            &conn.id,
        );
        let color = conn.color.as_deref().and_then(parse_hex_color);
        tauri_table_row(
            self.manager_table_colors(has_background),
            self.manager_table_metrics(),
            selected,
        )
        .id((
            gpui::ElementId::from("session-manager-table-row"),
            conn.id.clone(),
        ))
        .on_hover(cx.listener({
            let id = conn.id.clone();
            move |this, is_hovered: &bool, _window, cx| {
                if *is_hovered {
                    this.session_manager.hovered_connection_id = Some(id.clone());
                } else if this.session_manager.hovered_connection_id.as_deref() == Some(id.as_str())
                {
                    this.session_manager.hovered_connection_id = None;
                }
                cx.notify();
            }
        }))
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                this.close_session_row_menus();
                if event.click_count == 2 {
                    this.open_saved_connection(&id, window, cx);
                } else {
                    this.begin_selectable_text_group_from_mouse_down(
                        selection_group_id,
                        event,
                        window,
                        cx,
                    );
                }
                cx.stop_propagation();
            }),
        )
        .on_mouse_down(
            MouseButton::Right,
            cx.listener({
                let id = conn.id.clone();
                move |this, event: &MouseDownEvent, _window, cx| {
                    this.open_session_row_context_menu(
                        &id,
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                    );
                    cx.notify();
                    cx.stop_propagation();
                }
            }),
        )
        .when_some(color, |row, color| {
            row.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(MANAGER_COLOR_INDICATOR_WIDTH))
                    .rounded_l(px(self.tokens.radii.sm))
                    .bg(rgb(color)),
            )
        })
        .child(tauri_table_checkbox_cell(
            MANAGER_COL_CHECKBOX,
            checkbox(&self.tokens, String::new(), selected).on_mouse_down(
                MouseButton::Left,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, _window, cx| {
                        this.toggle_connection_selection(&id);
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            ),
        ))
        .child(self.render_table_cell(
            conn.name.clone(),
            selection_group_id,
            ("name", conn.id.as_str()),
            0,
            MANAGER_COL_NAME_BASIS,
            TauriTableCellStyle::Primary,
            true,
            cx,
        ))
        .child(self.render_table_cell(
            conn.host.clone(),
            selection_group_id,
            ("host", conn.id.as_str()),
            1,
            MANAGER_COL_HOST,
            TauriTableCellStyle::MetaMono,
            false,
            cx,
        ))
        .child(self.render_table_cell(
            conn.port.to_string(),
            selection_group_id,
            ("port", conn.id.as_str()),
            2,
            MANAGER_COL_PORT,
            TauriTableCellStyle::MetaMono,
            false,
            cx,
        ))
        .child(self.render_table_cell(
            conn.username.clone(),
            selection_group_id,
            ("username", conn.id.as_str()),
            3,
            MANAGER_COL_USERNAME,
            TauriTableCellStyle::Meta,
            false,
            cx,
        ))
        .child(self.render_auth_badge_cell(conn.auth_type))
        .child(self.render_table_cell(
            conn.group.clone().unwrap_or_else(|| "—".to_string()),
            selection_group_id,
            ("group", conn.id.as_str()),
            4,
            MANAGER_COL_GROUP,
            TauriTableCellStyle::Meta,
            false,
            cx,
        ))
        .child(self.render_table_cell(
            format_last_used(conn.last_used_at.as_deref(), &self.i18n),
            selection_group_id,
            ("last-used", conn.id.as_str()),
            5,
            MANAGER_COL_LAST_USED,
            TauriTableCellStyle::Meta,
            false,
            cx,
        ))
        .child(tauri_table_spacer_cell(MANAGER_COL_ACTIONS))
        .child(self.render_inline_row_actions(conn, hovered, has_background, cx))
        .into_any_element()
    }

    fn render_table_cell(
        &self,
        text: String,
        selection_group_id: u64,
        selection_key: impl std::hash::Hash,
        selection_order: usize,
        width: f32,
        style: TauriTableCellStyle,
        flexible: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let options = self.manager_table_cell_options(width, flexible);
        let strong = style == TauriTableCellStyle::Primary;
        let color = if strong {
            self.tokens.ui.text
        } else {
            self.tokens.ui.text_muted
        };
        div()
            .when(options.flexible, |cell| {
                cell.flex_1().min_w(px(options.min_width))
            })
            .when(!options.flexible, |cell| {
                cell.w(px(options.width)).flex_none()
            })
            .pl(px(options.padding_left))
            .truncate()
            .text_size(px(match style {
                TauriTableCellStyle::Primary => options.primary_text_size,
                TauriTableCellStyle::Meta | TauriTableCellStyle::MetaMono => {
                    options.meta_text_size
                }
            }))
            .font_weight(if strong {
                gpui::FontWeight::MEDIUM
            } else {
                gpui::FontWeight::NORMAL
            })
            .text_color(rgb(color))
            .when(style == TauriTableCellStyle::MetaMono, |cell| {
                if let Some(font) = options.mono_font.clone() {
                    cell.font_family(font)
                } else {
                    cell
                }
            })
            .child(self.render_row_safe_selectable_display_text_in_group(
                selection_group_id,
                "session-manager-table-cell",
                selection_key,
                selection_order,
                text,
                color,
                (style == TauriTableCellStyle::MetaMono)
                    .then(|| options.mono_font.clone())
                    .flatten(),
                cx,
            ))
            .into_any_element()
    }

    fn manager_table_colors(&self, has_background: bool) -> TauriTableColors {
        let theme = self.tokens.ui;
        TauriTableColors {
            header_border: theme_border(theme.border, has_background),
            header_bg: theme_secondary_bg(theme.bg_secondary, has_background),
            row_border: theme_border_half(theme.border, has_background),
            row_hover_bg: theme_hover_bg(theme.bg_hover, has_background),
            row_selected_bg: rgba((theme.info << 8) | BG_ACTIVE_ROW_SELECTED_ALPHA),
        }
    }

    fn manager_table_metrics(&self) -> TauriTableMetrics {
        TauriTableMetrics {
            header_text_size: MANAGER_TABLE_HEADER_TEXT_SIZE,
            ..TauriTableMetrics::default()
        }
    }

    fn manager_table_cell_options(&self, width: f32, flexible: bool) -> TauriTableCellOptions {
        TauriTableCellOptions {
            width,
            min_width: MANAGER_COL_NAME_MIN,
            flexible,
            padding_left: if flexible { 4.0 } else { 0.0 },
            primary_text_size: MANAGER_ROW_TEXT_SIZE,
            meta_text_size: MANAGER_ROW_META_TEXT_SIZE,
            mono_font: Some(settings_mono_font_family(self.settings_store.settings())),
        }
    }

    fn render_auth_badge_cell(&self, auth_type: AuthType) -> AnyElement {
        let theme = self.tokens.ui;
        let (icon, label, bg, fg) = auth_badge_style(auth_type, theme.text_muted, theme.text);
        div()
            .w(px(MANAGER_COL_AUTH))
            .flex_none()
            .flex()
            .items_center()
            .child(icon_badge(
                IconBadgeMetrics {
                    width: auth_badge_width(label),
                    gap: MANAGER_AUTH_BADGE_GAP,
                    padding_x: MANAGER_AUTH_BADGE_PADDING_X,
                    padding_y: 2.0,
                    text_size: MANAGER_AUTH_BADGE_TEXT_SIZE,
                    radius: self.tokens.radii.md,
                },
                label,
                Self::render_lucide_icon(icon, MANAGER_AUTH_BADGE_ICON_SIZE, fg),
                bg,
                fg,
            ))
            .into_any_element()
    }

    fn render_inline_row_actions(
        &self,
        conn: ConnectionInfo,
        visible: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let menu_open =
            self.session_manager.row_menu_connection_id.as_deref() == Some(conn.id.as_str());
        let actions_visible = visible || menu_open;
        div()
            .absolute()
            .right_0()
            .top_0()
            .bottom_0()
            .w(px(MANAGER_COL_ACTIONS))
            .flex()
            .items_center()
            .justify_end()
            .gap(px(2.0))
            .opacity(if actions_visible { 1.0 } else { 0.0 })
            .bg(if actions_visible {
                theme_hover_bg(theme.bg_hover, has_background)
            } else {
                theme_bg(theme.bg, has_background)
            })
            .child(self.render_row_icon_button(
                LucideIcon::Play,
                MANAGER_ROW_ACTION_BUTTON,
                12.0,
                rgb(0x4ade80),
                has_background,
                {
                    let id = conn.id.clone();
                    move |this, _event, window, cx| {
                        this.close_session_row_menus();
                        this.open_saved_connection(&id, window, cx);
                        cx.stop_propagation();
                    }
                },
                cx,
            ))
            .child(self.render_row_icon_button(
                LucideIcon::Pencil,
                MANAGER_ROW_ACTION_BUTTON,
                12.0,
                rgb(theme.text),
                has_background,
                {
                    let id = conn.id.clone();
                    move |this, _event, window, cx| {
                        this.close_session_row_menus();
                        this.open_saved_connection_editor(&id, None, window, cx);
                        cx.stop_propagation();
                    }
                },
                cx,
            ))
            .child(self.render_row_icon_button(
                LucideIcon::MoreHorizontal,
                MANAGER_ROW_MORE_BUTTON,
                14.0,
                rgb(theme.text),
                has_background,
                {
                    let id = conn.id.clone();
                    move |this, event: &MouseDownEvent, _window, cx| {
                        let trigger_x = f32::from(event.position.x);
                        let trigger_y = f32::from(event.position.y);
                        this.toggle_session_row_more_menu(
                            &id,
                            trigger_x - MANAGER_ROW_MENU_WIDTH + MANAGER_ROW_MORE_BUTTON,
                            trigger_y,
                        );
                        cx.notify();
                        cx.stop_propagation();
                    }
                },
                cx,
            ))
            .into_any_element()
    }

    fn render_row_more_menu(
        &self,
        conn: ConnectionInfo,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            self.session_manager.row_menu_x,
            self.session_manager.row_menu_y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_ROW_MENU_WIDTH,
            MANAGER_ROW_MENU_HEIGHT,
            8.0,
        );
        let theme = self.tokens.ui;
        // Row "more" menus used to be row-local absolute children. Rendering
        // them as a context-menu surface gives them the same outside-click,
        // wheel island, and Esc dismissal path as right-click row menus.
        context_menu_event_boundary(div()
            .absolute()
            .left(px(placement.x))
            .top(px(placement.y))
            .w(px(MANAGER_ROW_MENU_WIDTH))
            .p(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_panel_bg(theme.bg_panel, has_background))
            .shadow_lg())
            .child(
                self.render_session_manager_menu_action(
                    self.render_row_menu_item(
                        LucideIcon::Zap,
                        self.i18n.t("sessionManager.actions.test_connection"),
                        rgb(theme.text),
                        false,
                        false,
                        has_background,
                        cx,
                ),
                false,
                false,
                has_background,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, window, cx| {
                        this.test_connection(&id, window, cx);
                    }
                }),
                cx,
                ),
            )
            .child(
                self.render_session_manager_menu_action(
                    self.render_row_menu_item(
                        LucideIcon::Copy,
                        self.i18n.t("sessionManager.actions.duplicate"),
                        rgb(theme.text),
                        false,
                        false,
                        has_background,
                        cx,
                ),
                false,
                false,
                has_background,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, _window, cx| {
                        this.duplicate_connection(&id, cx);
                    }
                }),
                cx,
                ),
            )
            .child(
                div()
                    .h(px(1.0))
                    .my(px(4.0))
                    .bg(theme_border_half(theme.border, has_background)),
            )
            .child(
                self.render_session_manager_menu_action(
                    self.render_row_menu_item(
                        LucideIcon::Trash2,
                        self.i18n.t("sessionManager.actions.delete"),
                        rgb(0xf87171),
                        false,
                        false,
                        has_background,
                        cx,
                ),
                false,
                false,
                has_background,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, _window, cx| {
                        this.delete_connection(&id, cx);
                    }
                }),
                cx,
                ),
            )
            .into_any_element()
    }

    fn render_row_context_menu(
        &self,
        conn: ConnectionInfo,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let placement = browser_behavior::clamp_context_menu_position(
            self.session_manager.row_context_menu_x,
            self.session_manager.row_context_menu_y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_ROW_MENU_WIDTH,
            MANAGER_ROW_CONTEXT_MENU_HEIGHT,
            8.0,
        );
        let theme = self.tokens.ui;
        context_menu_event_boundary(div()
            .absolute()
            .left(px(placement.x))
            .top(px(placement.y))
            .w(px(MANAGER_ROW_MENU_WIDTH))
            .p(px(4.0))
            .rounded(px(self.tokens.radii.md))
            .border_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_panel_bg(theme.bg_panel, has_background))
            .shadow_lg())
            .child(
                self.render_session_manager_menu_action(
                    self.render_row_menu_item_with_icon_color(
                        LucideIcon::Play,
                        self.i18n.t("sessionManager.actions.connect"),
                        rgb(theme.text),
                        rgb(0x4ade80),
                        false,
                        false,
                        has_background,
                        cx,
                ),
                false,
                false,
                has_background,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, window, cx| {
                        this.open_saved_connection(&id, window, cx);
                    }
                }),
                cx,
                ),
            )
            .child(
                self.render_session_manager_menu_action(
                    self.render_row_menu_item_with_icon_color(
                        LucideIcon::Zap,
                        self.i18n.t("sessionManager.actions.test_connection"),
                        rgb(theme.text),
                        rgb(theme.text),
                        false,
                        false,
                        has_background,
                        cx,
                ),
                false,
                false,
                has_background,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, window, cx| {
                        this.test_connection(&id, window, cx);
                    }
                }),
                cx,
                ),
            )
            .child(
                self.render_session_manager_menu_action(
                    self.render_row_menu_item_with_icon_color(
                        LucideIcon::Pencil,
                        self.i18n.t("sessionManager.actions.edit"),
                        rgb(theme.text),
                        rgb(theme.text),
                        false,
                        false,
                        has_background,
                        cx,
                ),
                false,
                false,
                has_background,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, window, cx| {
                        this.open_saved_connection_editor(&id, None, window, cx);
                    }
                }),
                cx,
                ),
            )
            .child(
                self.render_session_manager_menu_action(
                    self.render_row_menu_item_with_icon_color(
                        LucideIcon::Copy,
                        self.i18n.t("sessionManager.actions.duplicate"),
                        rgb(theme.text),
                        rgb(theme.text),
                        false,
                        false,
                        has_background,
                        cx,
                ),
                false,
                false,
                has_background,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, _window, cx| {
                        this.duplicate_connection(&id, cx);
                    }
                }),
                cx,
                ),
            )
            .child(
                div()
                    .h(px(1.0))
                    .my(px(4.0))
                    .bg(theme_border_half(theme.border, has_background)),
            )
            .child(
                self.render_session_manager_menu_action(
                    self.render_row_menu_item_with_icon_color(
                        LucideIcon::Trash2,
                        self.i18n.t("sessionManager.actions.delete"),
                        rgb(0xf87171),
                        rgb(0xf87171),
                        false,
                        false,
                        has_background,
                        cx,
                ),
                false,
                false,
                has_background,
                cx.listener({
                    let id = conn.id.clone();
                    move |this, _event, _window, cx| {
                        this.delete_connection(&id, cx);
                    }
                }),
                cx,
                ),
            )
            .into_any_element()
    }

    fn render_row_menu_item(
        &self,
        icon: LucideIcon,
        label: String,
        color: Rgba,
        _disabled: bool,
        _loading: bool,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let item = div()
            .h(px(30.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px_2()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(color)
            .child(Self::render_lucide_icon(icon, 16.0, color))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "session-manager-row-menu-item",
                label.clone(),
                label,
                color.into(),
                cx,
            ));
        item
    }

    fn render_row_menu_item_with_icon_color(
        &self,
        icon: LucideIcon,
        label: String,
        text_color: Rgba,
        icon_color: Rgba,
        _disabled: bool,
        _loading: bool,
        _has_background: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let item = div()
            .h(px(30.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .px_2()
            .rounded(px(self.tokens.radii.sm))
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(text_color)
            .child(Self::render_lucide_icon(icon, 16.0, icon_color))
            .child(self.render_display_text_with_role(
                SelectableTextRole::NonSelectable,
                "session-manager-row-menu-item",
                label.clone(),
                label,
                text_color.into(),
                cx,
            ));
        item
    }

    fn render_session_manager_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // SessionManager has both inline "..." menus and row context menus.
        // Tauri routes both through Radix ContextMenuItem semantics, so native
        // keeps invocation, close, and disabled/loading guards in one path.
        self.workspace_context_menu_styled_action(
            item,
            disabled,
            loading,
            ContextMenuActionableStyle {
                hover_background: Some(theme_hover_bg(self.tokens.ui.bg_hover, has_background)),
                hover_text_color: None,
            },
            |this| {
                this.close_session_row_menus();
            },
            move |_this, event, window, cx| listener(event, window, cx),
            cx,
        )
    }

    fn render_row_icon_button(
        &self,
        icon: LucideIcon,
        size: f32,
        icon_size: f32,
        icon_color: Rgba,
        has_background: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        self.workspace_icon_action_button(
            icon,
            icon_size,
            icon_color,
            IconButtonOptions {
                has_background,
                ..IconButtonOptions::opaque_toolbar(size, ButtonRadius::Sm)
            },
            listener,
            cx,
        )
    }

}
