#[derive(Clone)]
enum SessionManagerDisplayItem {
    Connection(ConnectionInfo),
    Serial(SerialProfile),
    Telnet(TelnetProfile),
}

impl SessionManagerDisplayItem {
    fn id(&self) -> &str {
        match self {
            Self::Connection(connection) => &connection.id,
            Self::Serial(profile) => &profile.id,
            Self::Telnet(profile) => &profile.id,
        }
    }

    fn name(&self) -> &str {
        match self {
            Self::Connection(connection) => &connection.name,
            Self::Serial(profile) => &profile.name,
            Self::Telnet(profile) => &profile.name,
        }
    }

    fn group(&self) -> Option<&str> {
        match self {
            Self::Connection(connection) => connection.group.as_deref(),
            Self::Serial(profile) => profile.group.as_deref(),
            Self::Telnet(profile) => profile.group.as_deref(),
        }
    }

    fn last_used(&self) -> Option<String> {
        match self {
            Self::Connection(connection) => connection.last_used_at.clone(),
            Self::Serial(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::Telnet(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
        }
    }

    fn host(&self) -> &str {
        match self {
            Self::Connection(connection) => &connection.host,
            Self::Serial(profile) => &profile.port_path,
            Self::Telnet(profile) => &profile.host,
        }
    }

    fn port_sort_key(&self) -> u32 {
        match self {
            Self::Connection(connection) => u32::from(connection.port),
            Self::Serial(profile) => profile.baud_rate,
            Self::Telnet(profile) => u32::from(profile.port),
        }
    }

    fn username(&self) -> &str {
        match self {
            Self::Connection(connection) => &connection.username,
            Self::Serial(_) | Self::Telnet(_) => "",
        }
    }

    fn auth_sort_key(&self) -> String {
        match self {
            Self::Connection(connection) => auth_label(connection.auth_type).to_lowercase(),
            Self::Serial(_) => "serial".to_string(),
            Self::Telnet(_) => "telnet".to_string(),
        }
    }

    fn subtitle(&self) -> String {
        match self {
            Self::Connection(connection) => {
                format!("{}@{}:{}", connection.username, connection.host, connection.port)
            }
            Self::Serial(profile) => format!("{} · {}", profile.port_path, profile.baud_rate),
            Self::Telnet(profile) => format!("{}:{}", profile.host, profile.port),
        }
    }

    fn search_text(&self) -> String {
        match self {
            Self::Connection(connection) => format!(
                "{}\n{}\n{}\n{}\n{}\n{}",
                connection.name,
                connection.host,
                connection.port,
                connection.username,
                connection.group.as_deref().unwrap_or_default(),
                connection.tags.join(" ")
            ),
            Self::Serial(profile) => format!(
                "{}\n{}\n{}\n{}",
                profile.name,
                profile.port_path,
                profile.baud_rate,
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::Telnet(profile) => format!(
                "{}\n{}\n{}\n{}",
                profile.name,
                profile.host,
                profile.port,
                profile.group.as_deref().unwrap_or_default()
            ),
        }
    }

    fn icon(&self) -> LucideIcon {
        match self {
            Self::Connection(_) => LucideIcon::Server,
            Self::Serial(_) => LucideIcon::Radio,
            Self::Telnet(_) => LucideIcon::Terminal,
        }
    }
}

impl WorkspaceApp {
    fn session_manager_display_items(&self) -> Vec<SessionManagerDisplayItem> {
        let query = self.session_manager.search_query.trim().to_lowercase();
        let mut items = self
            .connection_store
            .connection_infos()
            .into_iter()
            .map(SessionManagerDisplayItem::Connection)
            .chain(
                self.connection_store
                    .serial_profiles()
                    .iter()
                    .cloned()
                    .map(SessionManagerDisplayItem::Serial),
            )
            .chain(
                self.connection_store
                    .telnet_profiles()
                    .iter()
                    .cloned()
                    .map(SessionManagerDisplayItem::Telnet),
            )
            .filter(|item| {
                query.is_empty() || item.search_text().to_lowercase().contains(query.as_str())
            })
            .collect::<Vec<_>>();
        self.sort_session_manager_display_items(&mut items);
        items
    }

    fn sort_session_manager_display_items(&self, items: &mut [SessionManagerDisplayItem]) {
        let field = self.session_manager.sort_field;
        let direction = self.session_manager.sort_direction;
        // Sort once at the display-model boundary so grid/list/tree cannot
        // drift apart and reintroduce view-specific ordering bugs.
        items.sort_by(|left, right| {
            let ordering = match field {
                SessionSortField::Name => compare_lower(left.name(), right.name()),
                SessionSortField::Host => compare_lower(left.host(), right.host()),
                SessionSortField::Port => left.port_sort_key().cmp(&right.port_sort_key()),
                SessionSortField::Username => compare_lower(left.username(), right.username()),
                SessionSortField::AuthType => left.auth_sort_key().cmp(&right.auth_sort_key()),
                SessionSortField::Group => compare_option_lower(left.group(), right.group()),
                SessionSortField::LastUsed => left.last_used().cmp(&right.last_used()),
            }
            .then_with(|| compare_lower(left.name(), right.name()))
            .then_with(|| left.id().cmp(right.id()));

            match direction {
                SortDirection::Asc => ordering,
                SortDirection::Desc => ordering.reverse(),
            }
        });
    }

    fn render_session_manager_view_content(
        &mut self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = self.session_manager_display_items();
        if items.is_empty() {
            return self.render_session_manager_empty_view(has_background).into_any_element();
        }
        match self.session_manager.view_mode {
            SessionManagerViewMode::Grid => {
                self.render_session_manager_grid_view(items, has_background, cx)
            }
            SessionManagerViewMode::List => {
                self.render_session_manager_list_view(items, has_background, cx)
            }
            SessionManagerViewMode::Tree => {
                self.render_session_manager_tree_view(items, has_background, cx)
            }
        }
    }

    fn render_session_manager_empty_view(&self, has_background: bool) -> Div {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(self.tokens.spacing.three))
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .text_color(rgb(theme.text_muted))
            .child(Self::render_lucide_icon(
                LucideIcon::Server,
                48.0,
                rgba((theme.text_muted << 8) | 0x66),
            ))
            .child(
                div()
                    .text_size(px(MANAGER_ROW_TEXT_SIZE))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(if self.session_manager.search_query.trim().is_empty() {
                        self.i18n.t("sessionManager.table.no_connections")
                    } else {
                        self.i18n.t("sessionManager.table.no_search_results")
                    }),
            )
    }

    fn render_session_manager_grid_view(
        &self,
        items: Vec<SessionManagerDisplayItem>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let recent = recent_session_items(&items);
        let (roots, _) = self.session_group_tree();
        let has_groups = !roots.is_empty();
        let mut sections = div()
            .p(px(self.tokens.spacing.three))
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three));
        let content = div()
            .size_full()
            .overflow_y_scrollbar()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_session_manager_view_actions(false, has_background, cx));

        if !recent.is_empty() {
            sections = sections.child(self.render_session_manager_grid_section(
                self.i18n.t("sessionManager.views.recent"),
                recent,
                has_background,
                cx,
            ));
        }

        // Grid mode treats groups as containers for hosts, not as standalone
        // cards, so the visual relationship stays obvious without switching
        // to the explicit tree view.
        for group in &roots {
            let group_items = session_items_for_group_subtree(&items, group);
            if group_items.is_empty() {
                continue;
            }
            sections = sections.child(self.render_session_manager_grid_section(
                group_display_name(group),
                group_items,
                has_background,
                cx,
            ));
        }

        let ungrouped_items = direct_session_items_for_group(&items, None);
        let host_items = if has_groups {
            ungrouped_items
        } else {
            items
        };
        if host_items.is_empty() {
            return content.child(sections).into_any_element();
        }

        content
            .child(sections.child(self.render_session_manager_grid_section(
                self.i18n.t("sessionManager.views.hosts"),
                host_items,
                has_background,
                cx,
            )))
            .into_any_element()
    }

    fn render_session_manager_grid_section(
        &self,
        title: String,
        items: Vec<SessionManagerDisplayItem>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let count = items.len();
        let mut cards = div().flex().flex_wrap().gap(px(self.tokens.spacing.three));
        for item in items {
            cards = cards.child(self.render_session_manager_item_card(item, has_background, cx));
        }
        self.render_session_manager_section_header(title, count)
            .child(cards)
    }

    fn render_session_manager_section_header(&self, title: String, count: usize) -> Div {
        div()
            .flex()
            .flex_col()
            .gap(px(self.tokens.spacing.three))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(self.tokens.ui.text_muted))
                            .child(count.to_string()),
                    ),
            )
    }

    fn render_session_manager_item_card(
        &self,
        item: SessionManagerDisplayItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let open_item = item.clone();
        div()
            .min_w(px(260.0))
            .flex_grow()
            .flex_basis(px(320.0))
            .rounded(px(self.tokens.radii.lg))
            .border_1()
            .border_color(theme_border_half(theme.border, has_background))
            .bg(theme_secondary_bg(theme.bg_secondary, has_background))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.three))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.three))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.click_count == 2 {
                        this.open_session_manager_display_item(open_item.clone(), window, cx);
                    }
                }),
            )
            .when(
                matches!(item, SessionManagerDisplayItem::Connection(_)),
                |card| {
                    let id = item.id().to_string();
                    card.child(
                        checkbox(
                            &self.tokens,
                            String::new(),
                            self.session_manager.selected_ids.contains(item.id()),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.toggle_connection_selection(&id);
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        ),
                    )
                },
            )
            .child(self.render_session_manager_item_icon(&item, theme.text))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(theme.text))
                            .child(item.name().to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(item.subtitle()),
                    ),
            )
            .child(self.render_session_manager_display_item_actions(item, has_background, cx))
    }

    fn render_session_manager_list_view(
        &self,
        items: Vec<SessionManagerDisplayItem>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let mut rows = div().flex().flex_col();
        for item in items {
            rows = rows.child(self.render_session_manager_display_item_row(
                item,
                0,
                has_background,
                cx,
            ));
        }
        div()
            .size_full()
            .overflow_y_scrollbar()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_session_manager_view_actions(false, has_background, cx))
            .child(
                div()
                    .border_b_1()
                    .border_color(theme_border(theme.border, has_background))
                    .bg(theme_secondary_bg(theme.bg_secondary, has_background))
                    .px_3()
                    .py_2()
                    .text_size(px(MANAGER_TABLE_HEADER_TEXT_SIZE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(self.i18n.t("sessionManager.views.list_header")),
            )
            .child(rows)
            .into_any_element()
    }

    fn render_session_manager_tree_view(
        &mut self,
        items: Vec<SessionManagerDisplayItem>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (roots, _children) = self.session_group_tree();
        let mut body = div().flex().flex_col();
        for group in roots {
            body = body.child(self.render_session_manager_tree_group(
                &group,
                0,
                &items,
                has_background,
                cx,
            ));
        }
        for item in direct_session_items_for_group(&items, None) {
            body = body.child(self.render_session_manager_display_item_row(
                item,
                0,
                has_background,
                cx,
            ));
        }

        div()
            .size_full()
            .overflow_y_scrollbar()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_session_manager_view_actions(true, has_background, cx))
            .child(body)
            .into_any_element()
    }

    fn render_session_manager_view_actions(
        &self,
        include_tree_controls: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let mut row = div()
            // The SSH config importer is a discovery action for every
            // session-manager layout, not a tree-only folder operation.
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .border_b_1()
            .border_color(theme_border(theme.border, has_background))
            .bg(theme_secondary_bg(theme.bg_secondary, has_background))
            .px_3()
            .py_2();
        if include_tree_controls {
            row = row
                .child(self.render_tree_mode_action_button(
                    LucideIcon::ChevronDown,
                    self.i18n.t("sessionManager.views.expand_all"),
                    has_background,
                    cx.listener(|this, _event, _window, cx| {
                        let (roots, children) = this.session_group_tree();
                        let mut groups = HashSet::new();
                        collect_session_group_paths(&roots, &children, &mut groups);
                        this.session_manager.expanded_groups = groups;
                        cx.notify();
                        cx.stop_propagation();
                    }),
                    cx,
                ))
                .child(self.render_tree_mode_action_button(
                    LucideIcon::ChevronRight,
                    self.i18n.t("sessionManager.views.collapse_all"),
                    has_background,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.expanded_groups.clear();
                        cx.notify();
                        cx.stop_propagation();
                    }),
                    cx,
                ))
                .child(self.render_tree_mode_action_button(
                    LucideIcon::Plus,
                    self.i18n.t("sessionManager.folder_tree.new_group"),
                    has_background,
                    cx.listener(|this, _event, _window, cx| {
                        this.close_session_row_menus();
                        this.session_manager.show_new_group = true;
                        this.session_manager.new_group_name.clear();
                        this.session_manager.focused_input = Some(SessionManagerInput::NewGroup);
                        cx.notify();
                        cx.stop_propagation();
                    }),
                    cx,
                ));
        }
        row.child(self.render_tree_mode_action_button(
            LucideIcon::FolderInput,
            self.i18n.t("settings_view.connections.ssh_config.title"),
            has_background,
            cx.listener(|this, _event, _window, cx| {
                this.close_session_row_menus();
                this.open_ssh_config_import(cx);
                cx.stop_propagation();
            }),
            cx,
        ))
    }

    fn render_tree_mode_action_button(
        &self,
        icon: LucideIcon,
        label: String,
        has_background: bool,
        listener: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        _cx: &mut Context<Self>,
    ) -> Div {
        self.workspace_toolbar_action_button(
            label,
            Some(Self::render_lucide_icon(
                icon,
                14.0,
                rgb(self.tokens.ui.text),
            )),
            ToolbarButtonOptions {
                button: ButtonOptions {
                    variant: ButtonVariant::Ghost,
                    size: ButtonSize::Sm,
                    radius: ButtonRadius::Md,
                    disabled: false,
                },
                has_background,
                show_label: true,
                ..ToolbarButtonOptions::default()
            },
            listener,
        )
    }

    fn render_session_manager_tree_group(
        &mut self,
        group: &str,
        depth: usize,
        items: &[SessionManagerDisplayItem],
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let (_roots, children) = self.session_group_tree();
        let group_items = direct_session_items_for_group(items, Some(group));
        let child_groups = children.get(group).cloned().unwrap_or_default();
        let expanded = self.session_manager.expanded_groups.contains(group);
        let has_children = !child_groups.is_empty() || !group_items.is_empty();
        let group_name = group.rsplit('/').next().unwrap_or(group).to_string();
        let group_id = group.to_string();
        let mut group_container = div().flex().flex_col().child(
            div()
                .border_b_1()
                .border_color(theme_border_half(theme.border, has_background))
                .px_3()
                .py_2()
                .pl(px(depth as f32 * 24.0 + 12.0))
                .flex()
                .items_center()
                .gap(px(self.tokens.spacing.two))
                .hover(|row| row.bg(theme_hover_bg(theme.bg_hover, has_background)))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        if has_children {
                            this.toggle_session_group_expanded(&group_id);
                            cx.notify();
                        }
                        cx.stop_propagation();
                    }),
                )
                .child(Self::render_lucide_icon(
                    if expanded {
                        LucideIcon::ChevronDown
                    } else {
                        LucideIcon::ChevronRight
                    },
                    16.0,
                    rgb(theme.text_muted),
                ))
                .child(Self::render_lucide_icon(
                    if expanded {
                        LucideIcon::FolderOpen
                    } else {
                        LucideIcon::Folder
                    },
                    16.0,
                    rgb(0xeab308),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .truncate()
                        .text_size(px(MANAGER_ROW_TEXT_SIZE))
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .text_color(rgb(theme.text))
                        .child(group_name),
                )
                .child(
                    div()
                        .rounded_full()
                        .bg(theme_input_bg(theme.bg, has_background))
                        .px_2()
                        .py(px(1.0))
                        .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                        .text_color(rgb(theme.text_muted))
                        .child(self.connection_count_for_group(group).to_string()),
                ),
        );
        if expanded {
            for child in child_groups {
                group_container = group_container.child(self.render_session_manager_tree_group(
                    &child,
                    depth + 1,
                    items,
                    has_background,
                    cx,
                ));
            }
            for item in group_items {
                group_container = group_container.child(self.render_session_manager_display_item_row(
                    item,
                    depth + 1,
                    has_background,
                    cx,
                ));
            }
        }
        group_container
    }

    fn render_session_manager_display_item_row(
        &self,
        item: SessionManagerDisplayItem,
        depth: usize,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let open_item = item.clone();
        let last_used = item.last_used();
        div()
            .border_b_1()
            .border_color(theme_border_half(theme.border, has_background))
            .px_3()
            .py_2()
            .pl(px(depth as f32 * 24.0 + 12.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.three))
            .hover(|row| row.bg(theme_hover_bg(theme.bg_hover, has_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if event.click_count == 2 {
                        this.open_session_manager_display_item(open_item.clone(), window, cx);
                    }
                }),
            )
            .when(
                matches!(item, SessionManagerDisplayItem::Connection(_)),
                |row| {
                    let id = item.id().to_string();
                    row.child(
                        checkbox(
                            &self.tokens,
                            String::new(),
                            self.session_manager.selected_ids.contains(item.id()),
                        )
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event, _window, cx| {
                                this.toggle_connection_selection(&id);
                                cx.notify();
                                cx.stop_propagation();
                            }),
                        ),
                    )
                },
            )
            .child(self.render_session_manager_item_icon(&item, theme.text))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(theme.text))
                            .child(item.name().to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(item.subtitle()),
                    ),
            )
            .child(
                div()
                    .min_w(px(96.0))
                    .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                    .text_color(rgb(theme.text_muted))
                    .child(format_last_used(last_used.as_deref(), &self.i18n)),
            )
            .child(self.render_session_manager_display_item_actions(item, has_background, cx))
    }

    fn render_session_manager_item_icon(&self, item: &SessionManagerDisplayItem, text: u32) -> Div {
        let bg = match item {
            SessionManagerDisplayItem::Connection(connection) => connection
                .color
                .as_deref()
                .and_then(parse_hex_color)
                .map(|color| rgba((color << 8) | 0x33))
                .unwrap_or_else(|| rgba(0x0ea5e933)),
            SessionManagerDisplayItem::Serial(_) => rgba(0xf59e0b33),
            SessionManagerDisplayItem::Telnet(_) => rgba(0x22c55e33),
        };
        let fg = match item {
            SessionManagerDisplayItem::Connection(connection) => connection
                .color
                .as_deref()
                .and_then(parse_hex_color)
                .map(rgb)
                .unwrap_or_else(|| rgb(0x7dd3fc)),
            SessionManagerDisplayItem::Serial(_) => rgb(0xfcd34d),
            SessionManagerDisplayItem::Telnet(_) => rgb(0x86efac),
        };
        div()
            .w(px(40.0))
            .h(px(40.0))
            .flex_none()
            .rounded(px(self.tokens.radii.lg))
            .flex()
            .items_center()
            .justify_center()
            .bg(bg)
            .child(Self::render_lucide_icon(item.icon(), 20.0, fg))
            .when(matches!(item, SessionManagerDisplayItem::Connection(_)), |icon| {
                icon.border_1().border_color(rgba((text << 8) | 0x1a))
            })
    }

    fn render_session_manager_display_item_actions(
        &self,
        item: SessionManagerDisplayItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        match item {
            SessionManagerDisplayItem::Connection(connection) => {
                let open_id = connection.id.clone();
                let edit_id = connection.id.clone();
                let test_id = connection.id.clone();
                let duplicate_id = connection.id.clone();
                let delete_id = connection.id.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(0x4ade80),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_connection(&open_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_connection_editor(&edit_id, None, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Zap,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.test_connection(&test_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Copy,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.duplicate_connection(&duplicate_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Trash2,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(0xf87171),
                        has_background,
                        move |this, _event, _window, cx| {
                            this.request_delete_connection(&delete_id, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Serial(profile) => {
                let open_id = profile.id.clone();
                let delete_id = profile.id.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(0x4ade80),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_serial_profile(&open_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Trash2,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(0xf87171),
                        has_background,
                        move |this, _event, _window, cx| {
                            this.request_delete_serial_profile(&delete_id, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Telnet(profile) => {
                let open_id = profile.id.clone();
                let delete_id = profile.id.clone();
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(0x4ade80),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_telnet_profile(&open_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Trash2,
                        MANAGER_ROW_ACTION_BUTTON,
                        12.0,
                        rgb(0xf87171),
                        has_background,
                        move |this, _event, _window, cx| {
                            this.request_delete_telnet_profile(&delete_id, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
        }
    }

    fn render_session_manager_menu_action(
        &self,
        item: gpui::Div,
        disabled: bool,
        loading: bool,
        has_background: bool,
        listener: impl Fn(&mut Self, &MouseDownEvent, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        // Session Manager menus share Workspace's guarded context-menu action
        // styling so dropdown and batch popovers dismiss consistently.
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
            listener,
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

    fn open_session_manager_display_item(
        &mut self,
        item: SessionManagerDisplayItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match item {
            SessionManagerDisplayItem::Connection(connection) => {
                self.open_saved_connection(&connection.id, window, cx)
            }
            SessionManagerDisplayItem::Serial(profile) => {
                self.open_saved_serial_profile(&profile.id, window, cx)
            }
            SessionManagerDisplayItem::Telnet(profile) => {
                self.open_saved_telnet_profile(&profile.id, window, cx)
            }
        }
    }
}

fn compare_lower(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase().cmp(&right.to_lowercase())
}

fn compare_option_lower(left: Option<&str>, right: Option<&str>) -> std::cmp::Ordering {
    compare_lower(left.unwrap_or_default(), right.unwrap_or_default())
}

fn recent_session_items(items: &[SessionManagerDisplayItem]) -> Vec<SessionManagerDisplayItem> {
    let mut recent = items
        .iter()
        .filter(|item| item.last_used().is_some())
        .cloned()
        .collect::<Vec<_>>();
    recent.sort_by(|left, right| right.last_used().cmp(&left.last_used()));
    recent.truncate(8);
    recent
}

fn direct_session_items_for_group(
    items: &[SessionManagerDisplayItem],
    group: Option<&str>,
) -> Vec<SessionManagerDisplayItem> {
    items
        .iter()
        .filter(|item| match (group, item.group()) {
            (None, None) => true,
            (Some(group), Some(item_group)) => item_group == group,
            _ => false,
        })
        .cloned()
        .collect()
}

fn session_items_for_group_subtree(
    items: &[SessionManagerDisplayItem],
    group: &str,
) -> Vec<SessionManagerDisplayItem> {
    let child_prefix = format!("{group}/");
    items
        .iter()
        .filter(|item| {
            item.group()
                .is_some_and(|item_group| item_group == group || item_group.starts_with(&child_prefix))
        })
        .cloned()
        .collect()
}

fn group_display_name(group: &str) -> String {
    group.rsplit('/').next().unwrap_or(group).to_string()
}

fn collect_session_group_paths(
    roots: &[String],
    children: &HashMap<String, Vec<String>>,
    output: &mut HashSet<String>,
) {
    for root in roots {
        output.insert(root.clone());
        if let Some(child_groups) = children.get(root) {
            collect_session_group_paths(child_groups, children, output);
        }
    }
}
