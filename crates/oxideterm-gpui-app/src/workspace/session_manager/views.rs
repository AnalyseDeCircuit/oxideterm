use super::*;
use gpui::StatefulInteractiveElement;

impl Render for SessionManagerDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // The preview stays compact so the destination folder remains visible while dragging.
        div().pl(self.position.x).pt(self.position.y).child(
            div()
                .max_w(px(MANAGER_DRAG_PREVIEW_MAX_WIDTH))
                .px_3()
                .py_2()
                .rounded(px(MANAGER_DRAG_PREVIEW_RADIUS))
                .border_1()
                .border_color(self.border)
                .bg(self.background)
                .text_color(self.text)
                .text_size(px(MANAGER_ROW_TEXT_SIZE))
                .shadow_lg()
                .child(self.label.clone()),
        )
    }
}

#[derive(Clone)]
pub(super) enum SessionManagerDisplayItem {
    Connection(ConnectionInfo),
    SshConfig(SessionManagerSshConfigDisplayItem),
    Serial(SerialProfile),
    Telnet(TelnetProfile),
    Mosh(MoshProfile),
    StandaloneSftp(oxideterm_connections::StandaloneSftpProfile),
    RemoteDesktop(RemoteDesktopProfile),
}

#[derive(Clone)]
pub(super) struct SessionManagerSshConfigDisplayItem {
    alias: String,
    hostname: Option<String>,
    user: Option<String>,
    port: Option<u16>,
}

impl From<&SshConfigHost> for SessionManagerSshConfigDisplayItem {
    fn from(host: &SshConfigHost) -> Self {
        // Session Manager only needs non-secret SSH config metadata. In
        // particular, never copy proxy commands or their SecretString values
        // into the long-lived virtual-list projection.
        Self {
            alias: host.alias.clone(),
            hostname: host.hostname.clone(),
            user: host.user.clone(),
            port: host.port,
        }
    }
}

#[derive(Clone)]
pub(super) enum SessionManagerOpenTarget {
    Connection(String),
    SshConfig(String),
    Serial(String),
    Telnet(String),
    Mosh(String),
    StandaloneSftp(String),
    RemoteDesktop(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SessionManagerGridRow {
    SectionHeader {
        title: String,
        item_count: usize,
    },
    RecentItems {
        item_indices: Vec<usize>,
        is_last_in_section: bool,
    },
    Cards {
        item_indices: Vec<usize>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SessionManagerTreeRow {
    Group {
        path: String,
        depth: usize,
        expanded: bool,
        has_children: bool,
    },
    Item {
        item_index: usize,
        depth: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionManagerItemPointerAction {
    None,
    Select,
    Open,
}

/// Keeps card and list-row pointer behavior aligned across Session Manager layouts.
pub(super) fn session_manager_item_pointer_action(
    click_count: usize,
    selectable: bool,
) -> SessionManagerItemPointerAction {
    match click_count {
        2 => SessionManagerItemPointerAction::Open,
        1 if selectable => SessionManagerItemPointerAction::Select,
        _ => SessionManagerItemPointerAction::None,
    }
}

impl SessionManagerDisplayItem {
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Connection(connection) => &connection.id,
            Self::SshConfig(host) => &host.alias,
            Self::Serial(profile) => &profile.id,
            Self::Telnet(profile) => &profile.id,
            Self::Mosh(profile) => &profile.id,
            Self::StandaloneSftp(profile) => &profile.id,
            Self::RemoteDesktop(profile) => &profile.id,
        }
    }

    pub(super) fn selection_target(&self) -> Option<SessionManagerSelectionTarget> {
        // Only SSH config discoveries are transient; every saved profile can use batch actions.
        match self {
            Self::Connection(connection) => Some(SessionManagerSelectionTarget::Connection(
                connection.id.clone(),
            )),
            Self::Serial(profile) => {
                Some(SessionManagerSelectionTarget::Serial(profile.id.clone()))
            }
            Self::Telnet(profile) => {
                Some(SessionManagerSelectionTarget::Telnet(profile.id.clone()))
            }
            Self::Mosh(profile) => Some(SessionManagerSelectionTarget::Mosh(profile.id.clone())),
            Self::StandaloneSftp(profile) => Some(SessionManagerSelectionTarget::StandaloneSftp(
                profile.id.clone(),
            )),
            Self::RemoteDesktop(profile) => Some(SessionManagerSelectionTarget::RemoteDesktop(
                profile.id.clone(),
            )),
            Self::SshConfig(_) => None,
        }
    }

    pub(super) fn row_action_target(&self) -> Option<SessionManagerRowActionTarget> {
        // SSH config discoveries are not persisted rows and therefore have no delete menu.
        match self {
            Self::Connection(connection) => Some(SessionManagerRowActionTarget::Connection(
                connection.id.clone(),
            )),
            Self::Serial(profile) => {
                Some(SessionManagerRowActionTarget::Serial(profile.id.clone()))
            }
            Self::Telnet(profile) => {
                Some(SessionManagerRowActionTarget::Telnet(profile.id.clone()))
            }
            Self::Mosh(profile) => Some(SessionManagerRowActionTarget::Mosh(profile.id.clone())),
            Self::StandaloneSftp(profile) => Some(SessionManagerRowActionTarget::StandaloneSftp(
                profile.id.clone(),
            )),
            Self::RemoteDesktop(profile) => Some(SessionManagerRowActionTarget::RemoteDesktop(
                profile.id.clone(),
            )),
            Self::SshConfig(_) => None,
        }
    }

    pub(super) fn open_target(&self) -> SessionManagerOpenTarget {
        match self {
            Self::Connection(connection) => {
                SessionManagerOpenTarget::Connection(connection.id.clone())
            }
            Self::SshConfig(host) => SessionManagerOpenTarget::SshConfig(host.alias.clone()),
            Self::Serial(profile) => SessionManagerOpenTarget::Serial(profile.id.clone()),
            Self::Telnet(profile) => SessionManagerOpenTarget::Telnet(profile.id.clone()),
            Self::Mosh(profile) => SessionManagerOpenTarget::Mosh(profile.id.clone()),
            Self::StandaloneSftp(profile) => {
                SessionManagerOpenTarget::StandaloneSftp(profile.id.clone())
            }
            Self::RemoteDesktop(profile) => {
                SessionManagerOpenTarget::RemoteDesktop(profile.id.clone())
            }
        }
    }

    pub(super) fn name(&self) -> &str {
        match self {
            Self::Connection(connection) => &connection.name,
            Self::SshConfig(host) => &host.alias,
            Self::Serial(profile) => &profile.name,
            Self::Telnet(profile) => &profile.name,
            Self::Mosh(profile) => &profile.name,
            Self::StandaloneSftp(profile) => &profile.name,
            Self::RemoteDesktop(profile) => &profile.name,
        }
    }

    pub(super) fn group(&self) -> Option<&str> {
        match self {
            Self::Connection(connection) => connection.group.as_deref(),
            Self::SshConfig(_) => None,
            Self::Serial(profile) => profile.group.as_deref(),
            Self::Telnet(profile) => profile.group.as_deref(),
            Self::Mosh(profile) => profile.group.as_deref(),
            Self::StandaloneSftp(profile) => profile.group.as_deref(),
            Self::RemoteDesktop(profile) => profile.group.as_deref(),
        }
    }

    pub(super) fn last_used(&self) -> Option<String> {
        match self {
            Self::Connection(connection) => connection.last_used_at.clone(),
            Self::SshConfig(_) => None,
            Self::Serial(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::Telnet(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::Mosh(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::StandaloneSftp(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
            Self::RemoteDesktop(profile) => profile.last_used_at.map(|time| time.to_rfc3339()),
        }
    }

    pub(super) fn host(&self) -> &str {
        match self {
            Self::Connection(connection) => &connection.host,
            Self::SshConfig(host) => host.hostname.as_deref().unwrap_or(&host.alias),
            Self::Serial(profile) => &profile.port_path,
            Self::Telnet(profile) => &profile.host,
            Self::Mosh(profile) => &profile.host,
            Self::StandaloneSftp(profile) => &profile.host,
            Self::RemoteDesktop(profile) => &profile.host,
        }
    }

    pub(super) fn port_sort_key(&self) -> u32 {
        match self {
            Self::Connection(connection) => u32::from(connection.port),
            Self::SshConfig(host) => u32::from(host.port.unwrap_or(22)),
            Self::Serial(profile) => profile.baud_rate,
            Self::Telnet(profile) => u32::from(profile.port),
            Self::Mosh(profile) => u32::from(profile.ssh_port),
            Self::StandaloneSftp(profile) => u32::from(profile.port),
            Self::RemoteDesktop(profile) => u32::from(profile.port),
        }
    }

    pub(super) fn username(&self) -> &str {
        match self {
            Self::Connection(connection) => &connection.username,
            Self::SshConfig(host) => host.user.as_deref().unwrap_or_default(),
            Self::Serial(_) | Self::Telnet(_) => "",
            Self::Mosh(profile) => &profile.username,
            Self::StandaloneSftp(profile) => &profile.username,
            Self::RemoteDesktop(profile) => profile.username.as_deref().unwrap_or_default(),
        }
    }

    pub(super) fn auth_sort_key(&self) -> String {
        match self {
            Self::Connection(connection) => auth_label(connection.auth_type).to_lowercase(),
            Self::SshConfig(_) => "ssh config".to_string(),
            Self::Serial(_) => "serial".to_string(),
            Self::Telnet(_) => "telnet".to_string(),
            Self::Mosh(_) => "mosh".to_string(),
            Self::StandaloneSftp(profile) => auth_label(profile.auth.auth_type()).to_lowercase(),
            Self::RemoteDesktop(profile) => profile.protocol.provider_id().to_string(),
        }
    }

    pub(super) fn subtitle(&self) -> String {
        match self {
            Self::Connection(connection) => {
                format!(
                    "{}@{}:{}",
                    connection.username, connection.host, connection.port
                )
            }
            Self::SshConfig(host) => match host.user.as_deref() {
                Some(user) if !user.is_empty() => {
                    format!(
                        "{}@{}:{}",
                        user,
                        host.hostname.as_deref().unwrap_or(&host.alias),
                        host.port.unwrap_or(22)
                    )
                }
                _ => format!(
                    "{}:{}",
                    host.hostname.as_deref().unwrap_or(&host.alias),
                    host.port.unwrap_or(22)
                ),
            },
            Self::Serial(profile) => format!("{} · {}", profile.port_path, profile.baud_rate),
            Self::Telnet(profile) => format!("{}:{}", profile.host, profile.port),
            Self::Mosh(profile) => {
                format!("{}@{}:{}", profile.username, profile.host, profile.ssh_port)
            }
            Self::StandaloneSftp(profile) => {
                format!("{}@{}:{}", profile.username, profile.host, profile.port)
            }
            Self::RemoteDesktop(profile) => match profile.username.as_deref() {
                Some(username) if !username.is_empty() => {
                    format!("{username}@{}:{}", profile.host, profile.port)
                }
                _ => format!("{}:{}", profile.host, profile.port),
            },
        }
    }

    pub(super) fn search_text(&self) -> String {
        match self {
            Self::Connection(connection) => connection.search_text(),
            Self::SshConfig(host) => format!(
                "{}\n{}\n{}\n{}\nssh config",
                host.alias,
                host.hostname.as_deref().unwrap_or(&host.alias),
                host.port.unwrap_or(22),
                host.user.as_deref().unwrap_or_default()
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
            Self::Mosh(profile) => format!(
                "{}\n{}\n{}\n{}\nmosh",
                profile.name,
                profile.host,
                profile.username,
                profile.group.as_deref().unwrap_or_default()
            ),
            Self::StandaloneSftp(profile) => format!(
                "{}\n{}\n{}\n{}\n{}\nstandalone sftp",
                profile.name,
                profile.host,
                profile.username,
                profile.group.as_deref().unwrap_or_default(),
                profile.initial_remote_path.as_deref().unwrap_or_default()
            ),
            Self::RemoteDesktop(profile) => format!(
                "{}\n{}\n{}\n{}\n{}",
                profile.name,
                profile.protocol.provider_id(),
                profile.host,
                profile.port,
                profile.group.as_deref().unwrap_or_default()
            ),
        }
    }

    pub(super) fn icon(&self) -> LucideIcon {
        match self {
            Self::Connection(connection) => {
                session_icons::session_icon_from_id(connection.icon.as_deref())
                    .unwrap_or(LucideIcon::Server)
            }
            Self::SshConfig(_) => LucideIcon::FileTerminal,
            Self::Serial(profile) => session_icons::session_icon_from_id(profile.icon.as_deref())
                .unwrap_or(LucideIcon::Radio),
            Self::Telnet(profile) => session_icons::session_icon_from_id(profile.icon.as_deref())
                .unwrap_or(LucideIcon::Terminal),
            Self::Mosh(profile) => session_icons::session_icon_from_id(profile.icon.as_deref())
                .unwrap_or(LucideIcon::Wifi),
            Self::StandaloneSftp(profile) => {
                session_icons::session_icon_from_id(profile.icon.as_deref())
                    .unwrap_or(LucideIcon::FolderSync)
            }
            Self::RemoteDesktop(profile) => {
                session_icons::session_icon_from_id(profile.icon.as_deref())
                    .unwrap_or(LucideIcon::Monitor)
            }
        }
    }

    pub(super) fn icon_color(&self) -> Option<&str> {
        match self {
            Self::Connection(connection) => connection.color.as_deref(),
            Self::Serial(profile) => profile.color.as_deref(),
            Self::Telnet(profile) => profile.color.as_deref(),
            Self::Mosh(profile) => profile.color.as_deref(),
            Self::StandaloneSftp(profile) => profile.color.as_deref(),
            Self::RemoteDesktop(profile) => profile.color.as_deref(),
            Self::SshConfig(_) => None,
        }
    }

    pub(super) fn icon_background_color(&self) -> Option<&str> {
        match self {
            Self::Connection(connection) => connection.icon_background_color.as_deref(),
            Self::Serial(profile) => profile.icon_background_color.as_deref(),
            Self::Telnet(profile) => profile.icon_background_color.as_deref(),
            Self::Mosh(profile) => profile.icon_background_color.as_deref(),
            Self::StandaloneSftp(profile) => profile.icon_background_color.as_deref(),
            Self::RemoteDesktop(profile) => profile.icon_background_color.as_deref(),
            Self::SshConfig(_) => None,
        }
    }
}

impl WorkspaceApp {
    fn session_manager_card_surface(&self, radius: f32, has_background: bool) -> Div {
        let surface = oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::Inspector)
                .padding(oxideterm_gpui_ui::SurfacePadding::None)
                .has_background_image(has_background),
        );
        // Compact shortcuts and full session cards share project chrome while
        // retaining the radius that communicates their different hierarchy.
        surface.rounded(px(radius))
    }

    pub(super) fn session_manager_display_items(&self, cx: &App) -> Vec<SessionManagerDisplayItem> {
        let query = self
            .session_manager
            .read(cx)
            .search_query
            .trim()
            .to_lowercase();
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
            .chain(
                self.connection_store
                    .mosh_profiles()
                    .iter()
                    .cloned()
                    .map(SessionManagerDisplayItem::Mosh),
            )
            .chain(
                self.connection_store
                    .standalone_sftp_profiles()
                    .iter()
                    .cloned()
                    .map(SessionManagerDisplayItem::StandaloneSftp),
            )
            .chain(
                self.connection_store
                    .remote_desktop_profiles()
                    .iter()
                    .cloned()
                    .map(SessionManagerDisplayItem::RemoteDesktop),
            )
            .chain(
                self.session_manager
                    .read(cx)
                    .ssh_config_hosts
                    .iter()
                    .filter(|host| !host.already_imported)
                    .map(SessionManagerSshConfigDisplayItem::from)
                    .map(SessionManagerDisplayItem::SshConfig),
            )
            .filter(|item| {
                query.is_empty() || item.search_text().to_lowercase().contains(query.as_str())
            })
            .collect::<Vec<_>>();
        self.sort_session_manager_display_items(&mut items, cx);
        items
    }

    pub(super) fn sort_session_manager_display_items(
        &self,
        items: &mut [SessionManagerDisplayItem],
        cx: &App,
    ) {
        let manager = self.session_manager.read(cx);
        let field = manager.sort_field;
        let direction = manager.sort_direction;
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

    fn session_manager_grid_columns(&self, window: &Window, cx: &App) -> (usize, usize) {
        let settings = self.settings_store.settings();
        let mut available_width = f32::from(window.viewport_size().width);
        if !settings.sidebar_ui.zen_mode {
            available_width -= self.tokens.metrics.activity_bar_width;
            if !self.sidebar_collapsed {
                available_width -= self.sidebar_panel_width();
            }
        }
        if self.context_sidebar_visible() {
            available_width -= self.ai_entity.read(cx).chat_ui().sidebar_width;
        }
        let grid_width =
            (available_width - self.tokens.spacing.three * 2.0).max(MANAGER_GRID_CARD_MIN_WIDTH);
        let gap = self.tokens.spacing.three;
        let card_columns = ((grid_width + gap) / (MANAGER_GRID_CARD_BASIS + gap))
            .floor()
            .max(1.0) as usize;
        let recent_columns = ((grid_width + self.tokens.spacing.two)
            / (MANAGER_RECENT_ITEM_BASIS + self.tokens.spacing.two))
            .floor()
            .max(1.0) as usize;
        (card_columns, recent_columns)
    }

    fn session_manager_grid_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(MANAGER_GRID_ESTIMATED_ROW_HEIGHT),
            MANAGER_MAIN_VIEW_OVERSCAN,
        )
    }

    fn session_manager_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(MANAGER_LIST_ESTIMATED_ROW_HEIGHT),
            MANAGER_MAIN_VIEW_OVERSCAN,
        )
    }

    fn session_manager_tree_list_spec() -> TauriVirtualListSpec {
        TauriVirtualListSpec::new(
            px(MANAGER_TREE_ESTIMATED_ROW_HEIGHT),
            MANAGER_MAIN_VIEW_OVERSCAN,
        )
    }

    fn sync_session_manager_grid_list_state(
        &self,
        rows: &[SessionManagerGridRow],
        items: &[SessionManagerDisplayItem],
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(|row| session_manager_grid_row_signature(row, items))
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.main_grid_list_state,
            &mut manager.main_grid_list_cache.borrow_mut(),
            "session-manager-grid",
            &signatures,
            Self::session_manager_grid_list_spec(),
        );
    }

    fn sync_session_manager_main_list_state(&self, items: &[SessionManagerDisplayItem], cx: &App) {
        let signatures = items
            .iter()
            .map(session_manager_display_item_signature)
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.main_list_state,
            &mut manager.main_list_cache.borrow_mut(),
            "session-manager-list",
            &signatures,
            Self::session_manager_list_spec(),
        );
    }

    fn sync_session_manager_tree_list_state(
        &self,
        rows: &[SessionManagerTreeRow],
        items: &[SessionManagerDisplayItem],
        cx: &App,
    ) {
        let signatures = rows
            .iter()
            .map(|row| session_manager_tree_row_signature(row, items))
            .collect::<Vec<_>>();
        let manager = self.session_manager.read(cx);
        sync_tauri_variable_list_state_by_signatures(
            &manager.main_tree_list_state,
            &mut manager.main_tree_list_cache.borrow_mut(),
            "session-manager-tree",
            &signatures,
            Self::session_manager_tree_list_spec(),
        );
    }

    pub(super) fn render_session_manager_view_content(
        &mut self,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let items = self.session_manager_display_items(cx);
        let view_mode = self.session_manager.read(cx).view_mode;
        if items.is_empty() && view_mode != SessionManagerViewMode::Tree {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.render_session_manager_view_actions(
                    view_mode == SessionManagerViewMode::Tree,
                    has_background,
                    cx,
                ))
                .child(
                    div()
                        .flex_1()
                        .min_h(px(0.0))
                        .child(self.render_session_manager_empty_view(has_background, cx)),
                )
                .into_any_element();
        }
        match view_mode {
            SessionManagerViewMode::Grid => {
                self.render_session_manager_grid_view(items, window, has_background, cx)
            }
            SessionManagerViewMode::List => {
                self.render_session_manager_list_view(items, has_background, cx)
            }
            SessionManagerViewMode::Tree => {
                self.render_session_manager_tree_view(items, has_background, cx)
            }
        }
    }

    pub(super) fn render_session_manager_empty_view(&self, has_background: bool, cx: &App) -> Div {
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
                    .child(
                        if self.session_manager.read(cx).search_query.trim().is_empty() {
                            self.i18n.t("sessionManager.table.no_connections")
                        } else {
                            self.i18n.t("sessionManager.table.no_search_results")
                        },
                    ),
            )
    }

    pub(super) fn render_session_manager_grid_view(
        &self,
        items: Vec<SessionManagerDisplayItem>,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (card_columns, recent_columns) = self.session_manager_grid_columns(window, cx);
        let (roots, _) = self.session_group_tree();
        let rows = Arc::<[SessionManagerGridRow]>::from(session_manager_grid_rows(
            &items,
            &roots,
            self.i18n.t("sessionManager.views.recent"),
            self.i18n.t("sessionManager.views.hosts"),
            card_columns,
            recent_columns,
        ));
        self.sync_session_manager_grid_list_state(&rows, &items, cx);
        let state = self.session_manager.read(cx).main_grid_list_state.clone();
        let workspace = cx.entity();
        let items = Arc::<[SessionManagerDisplayItem]>::from(items);
        let list = tauri_virtual_list(
            state,
            Self::session_manager_grid_list_spec(),
            move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    this.render_session_manager_grid_row(
                        rows.get(index),
                        &items,
                        has_background,
                        cx,
                    )
                })
            },
        )
        // Virtual rows own their horizontal gutters so full-width rows stay symmetric.
        .pt(px(self.tokens.spacing.three));
        let content = div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_session_manager_view_actions(false, has_background, cx));

        content
            .child(div().flex_1().min_h(px(0.0)).child(list))
            .into_any_element()
    }

    pub(super) fn render_session_manager_grid_row(
        &self,
        row: Option<&SessionManagerGridRow>,
        items: &[SessionManagerDisplayItem],
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            Some(SessionManagerGridRow::SectionHeader { title, item_count }) => self
                .render_session_manager_section_header(title.clone(), *item_count)
                .pb(px(self.tokens.spacing.three))
                .into_any_element(),
            Some(SessionManagerGridRow::RecentItems {
                item_indices,
                is_last_in_section,
            }) => div()
                .w_full()
                .min_w(px(0.0))
                .px(px(self.tokens.spacing.three))
                .flex()
                .flex_wrap()
                .gap(px(self.tokens.spacing.two))
                .pb(px(if *is_last_in_section {
                    self.tokens.spacing.three
                } else {
                    self.tokens.spacing.two
                }))
                .children(item_indices.iter().filter_map(|index| {
                    items.get(*index).map(|item| {
                        self.render_session_manager_recent_item(item, has_background, cx)
                    })
                }))
                .into_any_element(),
            Some(SessionManagerGridRow::Cards { item_indices }) => div()
                .w_full()
                .min_w(px(0.0))
                .px(px(self.tokens.spacing.three))
                .flex()
                .flex_wrap()
                .gap(px(self.tokens.spacing.three))
                .pb(px(self.tokens.spacing.three))
                .children(item_indices.iter().filter_map(|index| {
                    items
                        .get(*index)
                        .map(|item| self.render_session_manager_item_card(item, has_background, cx))
                }))
                .into_any_element(),
            None => div().into_any_element(),
        }
    }

    pub(super) fn render_session_manager_recent_item(
        &self,
        item: &SessionManagerDisplayItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let open_target = item.open_target();
        let selection_target = item.selection_target();
        let open_button_target = item.open_target();
        let last_used = format_last_used(item.last_used().as_deref(), &self.i18n);
        let is_selected = selection_target.as_ref().is_some_and(|target| {
            self.session_manager
                .read(cx)
                .selected_items
                .contains(target)
        });
        self.session_manager_card_surface(self.tokens.radii.md, has_background)
            .min_w(px(MANAGER_RECENT_ITEM_MIN_WIDTH))
            .flex_basis(px(MANAGER_RECENT_ITEM_BASIS))
            .px_2()
            .py_1()
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .hover(|shortcut| shortcut.bg(theme_row_hover_bg(theme.bg_hover, has_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    match session_manager_item_pointer_action(
                        event.click_count,
                        selection_target.is_some(),
                    ) {
                        SessionManagerItemPointerAction::Select => {
                            if let Some(target) = selection_target.clone() {
                                this.toggle_session_selection(target, cx);
                                cx.notify();
                            }
                        }
                        SessionManagerItemPointerAction::Open => {
                            this.open_session_manager_target(open_target.clone(), window, cx);
                        }
                        SessionManagerItemPointerAction::None => {}
                    }
                }),
            )
            .child(
                div()
                    .size(px(MANAGER_RECENT_ICON_SIZE))
                    .flex_none()
                    .rounded(px(self.tokens.radii.md))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba((theme.accent << 8) | MANAGER_RECENT_ACCENT_BG_ALPHA))
                    .child(Self::render_lucide_icon(
                        item.icon(),
                        MANAGER_RECENT_ICON_GLYPH_SIZE,
                        rgb(theme.accent),
                    )),
            )
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(if is_selected {
                                theme.accent
                            } else {
                                theme.text
                            }))
                            .child(item.name().to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(last_used),
                    ),
            )
            .child(self.render_row_icon_button(
                LucideIcon::Play,
                MANAGER_ROW_ACTION_BUTTON,
                MANAGER_ROW_ACTION_ICON_SIZE,
                rgb(theme.accent),
                has_background,
                move |this, _event, window, cx| {
                    this.open_session_manager_target(open_button_target.clone(), window, cx);
                    cx.stop_propagation();
                },
                cx,
            ))
    }

    pub(super) fn render_session_manager_section_header(&self, title: String, count: usize) -> Div {
        div()
            .w_full()
            .min_w(px(0.0))
            .px(px(self.tokens.spacing.three))
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

    pub(super) fn render_session_manager_item_card(
        &self,
        item: &SessionManagerDisplayItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let open_target = item.open_target();
        let selection_target = item.selection_target();
        let checkbox_target = selection_target.clone();
        let subtitle = if matches!(item, SessionManagerDisplayItem::SshConfig(_)) {
            format!(
                "{} · {}",
                item.subtitle(),
                self.i18n.t("command_palette.ssh_config_source")
            )
        } else {
            item.subtitle()
        };
        // Keep the selected connection name aligned with the checkbox's accent treatment.
        let is_selected = selection_target.as_ref().is_some_and(|target| {
            self.session_manager
                .read(cx)
                .selected_items
                .contains(target)
        });
        self.session_manager_card_surface(self.tokens.radii.lg, has_background)
            .min_w(px(260.0))
            .flex_grow_1()
            .flex_basis(px(320.0))
            .px(px(self.tokens.spacing.three))
            .py(px(self.tokens.spacing.three))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.three))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    match session_manager_item_pointer_action(
                        event.click_count,
                        selection_target.is_some(),
                    ) {
                        SessionManagerItemPointerAction::Select => {
                            if let Some(target) = selection_target.clone() {
                                this.toggle_session_selection(target, cx);
                                cx.notify();
                            }
                        }
                        SessionManagerItemPointerAction::Open => {
                            this.open_session_manager_target(open_target.clone(), window, cx);
                        }
                        SessionManagerItemPointerAction::None => {}
                    }
                }),
            )
            .when_some(checkbox_target, |card, target| {
                card.child(
                    checkbox(&self.tokens, String::new(), is_selected).on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| {
                            this.toggle_session_selection(target.clone(), cx);
                            cx.notify();
                            cx.stop_propagation();
                        }),
                    ),
                )
            })
            .child(self.render_session_manager_item_icon(item, theme.text))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(if is_selected {
                                theme.accent
                            } else {
                                theme.text
                            }))
                            .child(item.name().to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(subtitle),
                    ),
            )
            .child(self.render_session_manager_display_item_actions(item, has_background, cx))
    }

    pub(super) fn render_session_manager_list_view(
        &self,
        items: Vec<SessionManagerDisplayItem>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        self.sync_session_manager_main_list_state(&items, cx);
        let state = self.session_manager.read(cx).main_list_state.clone();
        let workspace = cx.entity();
        let items = Arc::<[SessionManagerDisplayItem]>::from(items);
        let list = tauri_virtual_list(
            state,
            Self::session_manager_list_spec(),
            move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    let Some(item) = items.get(index) else {
                        return div().into_any_element();
                    };
                    this.render_session_manager_display_item_row(item, 0, has_background, false, cx)
                        .into_any_element()
                })
            },
        );
        div()
            .size_full()
            .flex()
            .flex_col()
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
                    .py_1()
                    .flex()
                    .items_center()
                    .gap(px(self.tokens.spacing.three))
                    .text_size(px(MANAGER_TABLE_HEADER_TEXT_SIZE))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text_muted))
                    .child(div().w(px(MANAGER_SELECTION_COLUMN_WIDTH)).flex_none())
                    .child(div().w(px(MANAGER_ROW_ICON_SIZE)).flex_none())
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(self.i18n.t("sessionManager.table.name")),
                    )
                    .child(
                        div()
                            .w(px(MANAGER_LIST_LAST_USED_WIDTH))
                            .flex_none()
                            .child(self.i18n.t("sessionManager.table.last_used")),
                    )
                    .child(
                        div()
                            .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                            .flex_none()
                            .flex()
                            .justify_end()
                            .child(self.i18n.t("sessionManager.table.actions")),
                    ),
            )
            .child(div().flex_1().min_h(px(0.0)).child(list))
            .into_any_element()
    }

    pub(super) fn render_session_manager_tree_view(
        &mut self,
        items: Vec<SessionManagerDisplayItem>,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let (roots, children) = self.session_group_tree();
        let expanded_groups = self.session_manager.read(cx).expanded_groups.clone();
        let rows = Arc::<[SessionManagerTreeRow]>::from(session_manager_tree_rows(
            &items,
            &roots,
            &children,
            &expanded_groups,
        ));
        let has_rows = !rows.is_empty();
        self.sync_session_manager_tree_list_state(&rows, &items, cx);
        let state = self.session_manager.read(cx).main_tree_list_state.clone();
        let workspace = cx.entity();
        let items = Arc::<[SessionManagerDisplayItem]>::from(items);
        let list = tauri_virtual_list(
            state,
            Self::session_manager_tree_list_spec(),
            move |index, _window, cx| {
                workspace.update(cx, |this, cx| {
                    this.render_session_manager_tree_row(
                        rows.get(index),
                        &items,
                        has_background,
                        cx,
                    )
                })
            },
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(self.render_session_manager_view_actions(true, has_background, cx))
            .child(
                div()
                    .id("session-manager-tree-root-drop-target")
                    .flex_1()
                    .min_h(px(0.0))
                    .drag_over::<SessionManagerDrag>(move |target, _drag, _window, _cx| {
                        target
                            .border_1()
                            .border_dashed()
                            .border_color(rgb(theme.accent))
                            .bg(rgba((theme.accent << 8) | MANAGER_DRAG_ROOT_BG_ALPHA))
                    })
                    .can_drop(|drag, _window, _cx| drag.is::<SessionManagerDrag>())
                    .on_drop(cx.listener(|this, drag: &SessionManagerDrag, _window, cx| {
                        // Empty tree space and ungrouped rows both represent the root group.
                        this.move_dragged_sessions_to_group(&drag.targets, None, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            // Empty tree space represents the root group level.
                            this.open_session_manager_context_menu(
                                SessionManagerRowActionTarget::GroupRoot,
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_scroll_wheel(cx.listener(|this, _event, _window, cx| {
                        // Close pointer-positioned menus before the virtual rows move.
                        this.close_session_row_menus(cx);
                    }))
                    .child(if has_rows {
                        list.into_any_element()
                    } else {
                        self.render_session_manager_empty_view(has_background, cx)
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    pub(super) fn render_session_manager_view_actions(
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
            .bg(theme_bg(theme.bg, has_background))
            .px_3()
            .py_1();
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
                        this.session_manager.update(cx, |manager, cx| {
                            manager.expanded_groups = groups;
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
                    cx,
                ))
                .child(self.render_tree_mode_action_button(
                    LucideIcon::ChevronRight,
                    self.i18n.t("sessionManager.views.collapse_all"),
                    has_background,
                    cx.listener(|this, _event, _window, cx| {
                        this.session_manager.update(cx, |manager, cx| {
                            manager.expanded_groups.clear();
                            cx.notify();
                        });
                        cx.stop_propagation();
                    }),
                    cx,
                ))
                .child(self.render_tree_mode_action_button(
                    LucideIcon::FolderPlus,
                    self.i18n.t("sessionManager.folder_tree.new_group"),
                    has_background,
                    cx.listener(|this, _event, _window, cx| {
                        // Root-level creation is visible without requiring a context menu.
                        this.open_session_group_creation(cx);
                        cx.stop_propagation();
                    }),
                    cx,
                ));
        }
        // Group maintenance is a manager-level action shared by every view.
        row = row.child(self.render_tree_mode_action_button(
            LucideIcon::FolderOpen,
            self.i18n.t("sessionManager.folder_tree.manage_groups"),
            has_background,
            cx.listener(|this, _event, _window, cx| {
                this.open_session_group_manager(cx);
                cx.stop_propagation();
            }),
            cx,
        ));
        row.child(self.render_tree_mode_action_button(
            LucideIcon::FolderInput,
            self.i18n.t("settings_view.connections.ssh_config.title"),
            has_background,
            cx.listener(|this, _event, _window, cx| {
                this.close_session_row_menus(cx);
                this.open_settings_ssh_config_import_dialog(cx);
                cx.stop_propagation();
            }),
            cx,
        ))
        .child(self.render_tree_mode_action_button(
            LucideIcon::Download,
            self.i18n.t("settings_view.connections.importers.title"),
            has_background,
            cx.listener(|this, _event, window, cx| {
                this.close_session_row_menus(cx);
                this.open_connection_importers_settings(window, cx);
                cx.stop_propagation();
            }),
            cx,
        ))
    }

    pub(super) fn render_tree_mode_action_button(
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

    pub(super) fn render_session_manager_tree_row(
        &self,
        row: Option<&SessionManagerTreeRow>,
        items: &[SessionManagerDisplayItem],
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match row {
            Some(SessionManagerTreeRow::Group {
                path,
                depth,
                expanded,
                has_children,
            }) => self
                .render_session_manager_tree_group_row(
                    path,
                    *depth,
                    *expanded,
                    *has_children,
                    has_background,
                    cx,
                )
                .into_any_element(),
            Some(SessionManagerTreeRow::Item { item_index, depth }) => items
                .get(*item_index)
                .map(|item| {
                    let context_target = item.row_action_target();
                    let drop_group = item.group().map(ToOwned::to_owned);
                    let theme = self.tokens.ui;
                    self.render_session_manager_display_item_row(
                        item,
                        *depth,
                        has_background,
                        true,
                        cx,
                    )
                    .drag_over::<SessionManagerDrag>(move |row, _drag, _window, _cx| {
                        // Every visible child row participates in its nearest folder's drop area.
                        row.border_1()
                            .border_color(rgb(theme.accent))
                            .bg(rgba((theme.accent << 8) | MANAGER_DRAG_GROUP_BG_ALPHA))
                    })
                    .can_drop(|drag, _window, _cx| drag.is::<SessionManagerDrag>())
                    .on_drop(
                        cx.listener(move |this, drag: &SessionManagerDrag, _window, cx| {
                            this.move_dragged_sessions_to_group(
                                &drag.targets,
                                drop_group.as_deref(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                            if let Some(target) = context_target.clone() {
                                this.open_session_manager_context_menu(
                                    target,
                                    f32::from(event.position.x),
                                    f32::from(event.position.y),
                                    cx,
                                );
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .into_any_element()
                })
                .unwrap_or_else(|| div().into_any_element()),
            None => div().into_any_element(),
        }
    }

    pub(super) fn render_session_manager_tree_group_row(
        &self,
        group: &str,
        depth: usize,
        expanded: bool,
        has_children: bool,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let group_name = group.rsplit('/').next().unwrap_or(group).to_string();
        let group_id = group.to_string();
        let context_group = group.to_string();
        let menu_group = group.to_string();
        let drop_group = group.to_string();
        let subgroup_parent = group.to_string();
        let subgroup_tooltip = format!(
            "{} — {group}",
            self.i18n.t("sessionManager.folder_tree.new_subgroup")
        );
        let workspace = cx.entity();
        div()
            .w_full()
            .min_w(px(0.0))
            .border_b_1()
            .border_color(theme_border_half(theme.border, has_background))
            .px_3()
            .py_2()
            .pl(px(depth as f32 * 24.0 + 12.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.two))
            .hover(|row| row.bg(theme_row_hover_bg(theme.bg_hover, has_background)))
            .drag_over::<SessionManagerDrag>(move |row, _drag, _window, _cx| {
                row.border_1()
                    .border_color(rgb(theme.accent))
                    .bg(rgba((theme.accent << 8) | MANAGER_DRAG_GROUP_BG_ALPHA))
            })
            .can_drop(|drag, _window, _cx| drag.is::<SessionManagerDrag>())
            .on_drop(
                cx.listener(move |this, drag: &SessionManagerDrag, _window, cx| {
                    this.move_dragged_sessions_to_group(&drag.targets, Some(&drop_group), cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if has_children {
                        this.toggle_session_group_expanded(&group_id, cx);
                        cx.notify();
                    }
                    cx.stop_propagation();
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.open_session_manager_context_menu(
                        SessionManagerRowActionTarget::Group(context_group.clone()),
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        cx,
                    );
                    cx.stop_propagation();
                }),
            )
            .child(self.render_animated_chevron(
                (
                    gpui::SharedString::from(format!("session-group-chevron-{group}")),
                    expanded as usize,
                ),
                expanded,
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
                rgb(theme.warning),
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
                    .rounded(px(self.tokens.radii.sm))
                    .bg(theme_input_bg(theme.bg, has_background))
                    .px_2()
                    .py(px(1.0))
                    .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                    .text_color(rgb(theme.text_muted))
                    .child(self.connection_count_for_group(group).to_string()),
            )
            .child(self.workspace_tooltip_icon_button(
                LucideIcon::FolderPlus,
                MANAGER_ROW_ACTION_ICON_SIZE,
                rgb(theme.text),
                IconButtonOptions {
                    has_background,
                    ..IconButtonOptions::opaque_toolbar(MANAGER_ROW_ACTION_BUTTON, ButtonRadius::Sm)
                },
                subgroup_tooltip,
                "session-manager-new-subgroup",
                true,
                cx.listener(move |this, _event, _window, cx| {
                    this.open_session_subgroup_creation(&subgroup_parent, cx);
                    cx.stop_propagation();
                }),
                workspace,
            ))
            .child(self.render_row_icon_button(
                LucideIcon::MoreVertical,
                MANAGER_ROW_ACTION_BUTTON,
                MANAGER_ROW_ACTION_ICON_SIZE,
                rgb(theme.text),
                has_background,
                move |this, event, _window, cx| {
                    this.open_session_manager_row_action_menu(
                        SessionManagerRowActionTarget::Group(menu_group.clone()),
                        f32::from(event.position.x),
                        f32::from(event.position.y),
                        cx,
                    );
                    cx.stop_propagation();
                },
                cx,
            ))
    }

    pub(super) fn render_session_manager_display_item_row(
        &self,
        item: &SessionManagerDisplayItem,
        depth: usize,
        has_background: bool,
        allow_drag: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = self.tokens.ui;
        let open_target = item.open_target();
        let selection_target = item.selection_target();
        let checkbox_target = selection_target.clone();
        let last_used = item.last_used();
        let subtitle = if matches!(item, SessionManagerDisplayItem::SshConfig(_)) {
            format!(
                "{} · {}",
                item.subtitle(),
                self.i18n.t("command_palette.ssh_config_source")
            )
        } else {
            item.subtitle()
        };
        // List rows mirror the card view so selection feedback is consistent.
        let is_selected = selection_target.as_ref().is_some_and(|target| {
            self.session_manager
                .read(cx)
                .selected_items
                .contains(target)
        });
        let drag_handle = if allow_drag
            && self.session_manager.read(cx).search_query.trim().is_empty()
        {
            selection_target.clone().map(|target| {
                let selected_targets = &self.session_manager.read(cx).selected_items;
                let targets = if is_selected {
                    selected_targets.iter().cloned().collect::<Vec<_>>()
                } else {
                    vec![target.clone()]
                };
                let label = if targets.len() > 1 {
                    selected_count_label(&self.i18n, targets.len())
                } else {
                    item.name().to_string()
                };
                let drag = SessionManagerDrag {
                    targets,
                    label,
                    position: Point::default(),
                    background: rgb(theme.bg_panel),
                    border: rgb(theme.accent),
                    text: rgb(theme.text),
                };
                let drag_workspace = cx.entity();
                let grip_color = rgba((theme.text_muted << 8) | MANAGER_DRAG_GRIP_ALPHA);
                div()
                    .id(format!(
                        "session-manager-item-drag-{}",
                        session_manager_display_item_signature(item)
                    ))
                    .size(px(MANAGER_ROW_DRAG_HANDLE_SIZE))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(self.tokens.radii.sm))
                    .cursor(CursorStyle::OpenHand)
                    .hover(move |handle| {
                        handle.bg(theme_row_hover_bg(theme.bg_hover, has_background))
                    })
                    .child(
                        // A six-dot grip distinguishes moving from opening the connection row.
                        div()
                            .w(px(MANAGER_DRAG_GRIP_WIDTH))
                            .flex()
                            .flex_wrap()
                            .gap(px(MANAGER_DRAG_GRIP_DOT_SIZE))
                            .children((0..MANAGER_DRAG_GRIP_DOT_COUNT).map(move |_| {
                                div()
                                    .size(px(MANAGER_DRAG_GRIP_DOT_SIZE))
                                    .rounded(px(MANAGER_DRAG_GRIP_DOT_SIZE / 2.0))
                                    .bg(grip_color)
                            })),
                    )
                    .on_drag(drag, move |drag, position, _window, cx| {
                        let dragged_targets = drag.targets.iter().cloned().collect::<HashSet<_>>();
                        let _ = drag_workspace.update(cx, |this, cx| {
                            this.session_manager.update(cx, |session_manager, cx| {
                                // Row mouse-down selection is restored to the exact drag payload.
                                if session_manager.selected_items != dragged_targets {
                                    session_manager.selected_items = dragged_targets;
                                    cx.notify();
                                }
                            });
                        });
                        cx.new(|_| drag.with_position(position))
                    })
                    .into_any_element()
            })
        } else {
            None
        };
        let selection = if let Some(target) = checkbox_target {
            checkbox(&self.tokens, String::new(), is_selected)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event, _window, cx| {
                        this.toggle_session_selection(target.clone(), cx);
                        cx.notify();
                        cx.stop_propagation();
                    }),
                )
                .into_any_element()
        } else {
            // Non-SSH profiles keep the selection column reserved so all
            // identity and metadata columns remain aligned.
            div()
                .size(px(MANAGER_SELECTION_COLUMN_WIDTH))
                .flex_none()
                .into_any_element()
        };
        div()
            .w_full()
            .min_w(px(0.0))
            .border_b_1()
            .border_color(theme_border_half(theme.border, has_background))
            .px_3()
            .py_2()
            .pl(px(depth as f32 * 24.0 + 12.0))
            .flex()
            .items_center()
            .gap(px(self.tokens.spacing.three))
            .hover(|row| row.bg(theme_row_hover_bg(theme.bg_hover, has_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    match session_manager_item_pointer_action(
                        event.click_count,
                        selection_target.is_some(),
                    ) {
                        SessionManagerItemPointerAction::Select => {
                            if let Some(target) = selection_target.clone() {
                                this.toggle_session_selection(target, cx);
                                cx.notify();
                            }
                        }
                        SessionManagerItemPointerAction::Open => {
                            this.open_session_manager_target(open_target.clone(), window, cx);
                        }
                        SessionManagerItemPointerAction::None => {}
                    }
                }),
            )
            .when_some(drag_handle, |row, handle| row.child(handle))
            .child(selection)
            .child(self.render_session_manager_item_icon(item, theme.text))
            .child(
                div()
                    .min_w(px(0.0))
                    .flex_1()
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_TEXT_SIZE))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(rgb(if is_selected {
                                theme.accent
                            } else {
                                theme.text
                            }))
                            .child(item.name().to_string()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                            .text_color(rgb(theme.text_muted))
                            .child(subtitle),
                    ),
            )
            .child(
                div()
                    .w(px(MANAGER_LIST_LAST_USED_WIDTH))
                    .flex_none()
                    .text_size(px(MANAGER_ROW_META_TEXT_SIZE))
                    .text_color(rgb(theme.text_muted))
                    .child(format_last_used(last_used.as_deref(), &self.i18n)),
            )
            .child(self.render_session_manager_display_item_actions(item, has_background, cx))
    }

    pub(super) fn render_session_manager_item_icon(
        &self,
        item: &SessionManagerDisplayItem,
        text: u32,
    ) -> Div {
        let (default_background, default_foreground) = match item {
            SessionManagerDisplayItem::Connection(_)
            | SessionManagerDisplayItem::RemoteDesktop(_) => (0x0ea5e933, 0x7dd3fc),
            SessionManagerDisplayItem::SshConfig(_) => (0x8b5cf633, 0xc4b5fd),
            SessionManagerDisplayItem::Serial(_) => (0xf59e0b33, 0xfcd34d),
            SessionManagerDisplayItem::Telnet(_) => (0x22c55e33, 0x86efac),
            SessionManagerDisplayItem::Mosh(_) => (0x3b82f633, 0x93c5fd),
            SessionManagerDisplayItem::StandaloneSftp(_) => (0x14b8a633, 0x5eead4),
        };
        let configured_foreground = item.icon_color().and_then(parse_hex_color);
        // Older assets used one accent for both layers; keep that appearance
        // until an explicit background is selected.
        let bg = item
            .icon_background_color()
            .and_then(parse_hex_color)
            .map(rgb)
            .or_else(|| configured_foreground.map(|color| rgba((color << 8) | 0x33)))
            .unwrap_or_else(|| rgba(default_background));
        let fg = configured_foreground
            .map(rgb)
            .unwrap_or_else(|| rgb(default_foreground));
        div()
            .w(px(MANAGER_ROW_ICON_SIZE))
            .h(px(MANAGER_ROW_ICON_SIZE))
            .flex_none()
            .rounded(px(self.tokens.radii.lg))
            .flex()
            .items_center()
            .justify_center()
            .bg(bg)
            .child(Self::render_lucide_icon(item.icon(), 20.0, fg))
            .when(
                matches!(item, SessionManagerDisplayItem::Connection(_)),
                |icon| icon.border_1().border_color(rgba((text << 8) | 0x1a)),
            )
    }

    pub(super) fn render_session_manager_display_item_actions(
        &self,
        item: &SessionManagerDisplayItem,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        match item {
            SessionManagerDisplayItem::Connection(connection) => {
                let open_id = connection.id.clone();
                let edit_id = connection.id.clone();
                let menu_id = connection.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
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
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_connection_editor(&edit_id, None, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Connection(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::SshConfig(host) => {
                let open_alias = host.alias.clone();
                let import_alias = host.alias.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_ssh_config_alias_from_palette(open_alias.clone(), window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Download,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, _window, cx| {
                            this.import_session_manager_ssh_config_host(import_alias.clone(), cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Serial(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_serial_profile(&open_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_serial_profile_editor(&edit_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Serial(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Telnet(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_telnet_profile(&open_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_telnet_profile_editor(&edit_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Telnet(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::Mosh(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_mosh_profile(&open_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_mosh_profile_editor(&edit_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::Mosh(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::StandaloneSftp(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_standalone_sftp_profile(&open_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_standalone_sftp_profile_editor(&edit_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::StandaloneSftp(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
            SessionManagerDisplayItem::RemoteDesktop(profile) => {
                let open_id = profile.id.clone();
                let edit_id = profile.id.clone();
                let menu_id = profile.id.clone();
                div()
                    .w(px(MANAGER_ROW_ACTIONS_WIDTH))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(MANAGER_ROW_ACTION_GAP))
                    .child(self.render_row_icon_button(
                        LucideIcon::Play,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.accent),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_remote_desktop_profile(&open_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::Pencil,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, _event, window, cx| {
                            this.open_saved_remote_desktop_profile_editor(&edit_id, window, cx);
                            cx.stop_propagation();
                        },
                        cx,
                    ))
                    .child(self.render_row_icon_button(
                        LucideIcon::MoreVertical,
                        MANAGER_ROW_ACTION_BUTTON,
                        MANAGER_ROW_ACTION_ICON_SIZE,
                        rgb(self.tokens.ui.text),
                        has_background,
                        move |this, event, _window, cx| {
                            this.open_session_manager_row_action_menu(
                                SessionManagerRowActionTarget::RemoteDesktop(menu_id.clone()),
                                f32::from(event.position.x),
                                f32::from(event.position.y),
                                cx,
                            );
                            cx.stop_propagation();
                        },
                        cx,
                    ))
            }
        }
    }

    pub(super) fn render_session_manager_row_action_menu(
        &self,
        menu: SessionManagerRowActionMenu,
        window: &Window,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let viewport = window.viewport_size();
        let menu_height = match &menu.target {
            SessionManagerRowActionTarget::Connection(_) => {
                MANAGER_ROW_ACTION_MENU_CONNECTION_HEIGHT
            }
            SessionManagerRowActionTarget::Serial(_)
            | SessionManagerRowActionTarget::Telnet(_)
            | SessionManagerRowActionTarget::Mosh(_)
            | SessionManagerRowActionTarget::StandaloneSftp(_)
            | SessionManagerRowActionTarget::RemoteDesktop(_) => {
                MANAGER_ROW_ACTION_MENU_EDITABLE_PROFILE_HEIGHT
            }
            SessionManagerRowActionTarget::GroupRoot => MANAGER_ROW_ACTION_MENU_PROFILE_HEIGHT,
            SessionManagerRowActionTarget::Group(_) => MANAGER_ROW_ACTION_MENU_GROUP_HEIGHT,
        };
        let (requested_x, requested_y) = match menu.origin {
            SessionManagerRowActionMenuOrigin::ActionButton => (
                menu.x - MANAGER_ROW_ACTION_MENU_WIDTH + MANAGER_ROW_ACTION_BUTTON / 2.0,
                menu.y + MANAGER_ROW_ACTION_BUTTON / 2.0,
            ),
            SessionManagerRowActionMenuOrigin::Pointer => (menu.x, menu.y),
        };
        let placement = browser_behavior::clamp_context_menu_position(
            requested_x,
            requested_y,
            f32::from(viewport.width),
            f32::from(viewport.height),
            MANAGER_ROW_ACTION_MENU_WIDTH,
            menu_height,
            self.tokens.spacing.two,
        );
        let mut popup = context_menu_event_boundary(
            dropdown_menu_content(&self.tokens).w(px(MANAGER_ROW_ACTION_MENU_WIDTH)),
        );

        if matches!(&menu.target, SessionManagerRowActionTarget::GroupRoot) {
            popup = popup.child(self.render_session_manager_menu_action(
                dropdown_menu_item(
                    &self.tokens,
                    self.i18n.t("sessionManager.folder_tree.new_group"),
                    DropdownMenuItemKind::Plain,
                    false,
                    false,
                ),
                false,
                false,
                has_background,
                move |this, _event, _window, cx| {
                    this.open_session_group_creation(cx);
                    cx.stop_propagation();
                },
                cx,
            ));
        }

        if let SessionManagerRowActionTarget::Group(group) = &menu.target {
            let subgroup_parent = group.clone();
            let rename_group = group.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t("sessionManager.folder_tree.new_subgroup"),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, _window, cx| {
                        this.open_session_subgroup_creation(&subgroup_parent, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t("sessionManager.folder_tree.rename_group"),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, _window, cx| {
                        this.open_session_group_rename(&rename_group, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Connection(id) = &menu.target {
            let test_id = id.clone();
            popup = popup.child(self.render_session_manager_menu_action(
                dropdown_menu_item(
                    &self.tokens,
                    self.i18n.t("sessionManager.actions.test_connection"),
                    DropdownMenuItemKind::Plain,
                    false,
                    false,
                ),
                false,
                false,
                has_background,
                move |this, _event, window, cx| {
                    this.test_connection(&test_id, window, cx);
                    cx.stop_propagation();
                },
                cx,
            ));

            let duplicate_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t("sessionManager.actions.duplicate"),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.duplicate_connection(&duplicate_id, window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::RemoteDesktop(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t("sessionManager.actions.edit"),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.open_saved_remote_desktop_profile_editor(&edit_id, window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Mosh(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t("sessionManager.actions.edit"),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.open_saved_mosh_profile_editor(&edit_id, window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::StandaloneSftp(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t("sessionManager.actions.edit"),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.open_saved_standalone_sftp_profile_editor(&edit_id, window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Serial(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t("sessionManager.actions.edit"),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.open_saved_serial_profile_editor(&edit_id, window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        if let SessionManagerRowActionTarget::Telnet(id) = &menu.target {
            let edit_id = id.clone();
            popup = popup
                .child(self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        self.i18n.t("sessionManager.actions.edit"),
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    ),
                    false,
                    false,
                    has_background,
                    move |this, _event, window, cx| {
                        this.open_saved_telnet_profile_editor(&edit_id, window, cx);
                        cx.stop_propagation();
                    },
                    cx,
                ))
                .child(dropdown_menu_separator(&self.tokens));
        }

        let delete_action = match &menu.target {
            SessionManagerRowActionTarget::Connection(id) => {
                Some((id.clone(), self.i18n.t("sessionManager.actions.delete")))
            }
            SessionManagerRowActionTarget::Serial(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.serial_profiles.delete"),
            )),
            SessionManagerRowActionTarget::Telnet(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.telnet_profiles.delete"),
            )),
            SessionManagerRowActionTarget::Mosh(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.mosh_profiles.delete"),
            )),
            SessionManagerRowActionTarget::StandaloneSftp(id) => Some((
                id.clone(),
                self.i18n
                    .t("sessionManager.standalone_sftp_profiles.delete"),
            )),
            SessionManagerRowActionTarget::RemoteDesktop(id) => Some((
                id.clone(),
                self.i18n.t("sessionManager.remote_desktop_profiles.delete"),
            )),
            SessionManagerRowActionTarget::Group(group) => Some((
                group.clone(),
                self.i18n.t("sessionManager.folder_tree.delete_group"),
            )),
            SessionManagerRowActionTarget::GroupRoot => None,
        };
        if let Some((delete_id, delete_label)) = delete_action {
            let delete_target = menu.target.clone();
            popup = popup.child(
                self.render_session_manager_menu_action(
                    dropdown_menu_item(
                        &self.tokens,
                        delete_label,
                        DropdownMenuItemKind::Plain,
                        false,
                        false,
                    )
                    .text_color(rgb(self.tokens.ui.error)),
                    false,
                    false,
                    has_background,
                    move |this, _event, _window, cx| {
                        match &delete_target {
                            SessionManagerRowActionTarget::Connection(_) => {
                                this.request_delete_connection(&delete_id, cx)
                            }
                            SessionManagerRowActionTarget::Serial(_) => {
                                this.request_delete_serial_profile(&delete_id, cx)
                            }
                            SessionManagerRowActionTarget::Telnet(_) => {
                                this.request_delete_telnet_profile(&delete_id, cx)
                            }
                            SessionManagerRowActionTarget::Mosh(_) => {
                                this.request_delete_mosh_profile(&delete_id, cx)
                            }
                            SessionManagerRowActionTarget::StandaloneSftp(_) => {
                                this.request_delete_standalone_sftp_profile(&delete_id, cx)
                            }
                            SessionManagerRowActionTarget::RemoteDesktop(_) => {
                                this.request_delete_remote_desktop_profile(&delete_id, cx)
                            }
                            SessionManagerRowActionTarget::Group(_) => {
                                this.request_delete_session_group(&delete_id, cx)
                            }
                            SessionManagerRowActionTarget::GroupRoot => {}
                        }
                        cx.stop_propagation();
                    },
                    cx,
                ),
            );
        }

        // Pointer menus use the click position while ellipsis menus preserve
        // their established button alignment; both mount at the window root.
        deferred(
            anchored()
                .anchor(Corner::TopLeft)
                .position(gpui::point(px(placement.x), px(placement.y)))
                .position_mode(AnchoredPositionMode::Window)
                .child(overlay_content_boundary(popup)),
        )
        .with_priority(oxideterm_gpui_ui::modal::TAURI_POPOVER_LAYER_PRIORITY)
        .into_any_element()
    }

    pub(super) fn render_session_manager_menu_action(
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
            |_| {},
            move |this, event, window, cx| {
                this.close_session_row_menus(cx);
                listener(this, event, window, cx);
            },
            cx,
        )
    }

    pub(super) fn render_row_icon_button(
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

    pub(super) fn open_session_manager_target(
        &mut self,
        target: SessionManagerOpenTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match target {
            SessionManagerOpenTarget::Connection(connection_id) => {
                self.open_saved_connection(&connection_id, window, cx)
            }
            SessionManagerOpenTarget::SshConfig(alias) => {
                self.open_ssh_config_alias_from_palette(alias, window, cx)
            }
            SessionManagerOpenTarget::Serial(profile_id) => {
                self.open_saved_serial_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::Telnet(profile_id) => {
                self.open_saved_telnet_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::Mosh(profile_id) => {
                self.open_saved_mosh_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::StandaloneSftp(profile_id) => {
                self.open_saved_standalone_sftp_profile(&profile_id, window, cx)
            }
            SessionManagerOpenTarget::RemoteDesktop(profile_id) => {
                self.open_saved_remote_desktop_profile(&profile_id, window, cx)
            }
        }
    }
}

pub(super) fn compare_lower(left: &str, right: &str) -> std::cmp::Ordering {
    left.to_lowercase().cmp(&right.to_lowercase())
}

pub(super) fn compare_option_lower(left: Option<&str>, right: Option<&str>) -> std::cmp::Ordering {
    compare_lower(left.unwrap_or_default(), right.unwrap_or_default())
}

pub(super) fn session_manager_grid_rows(
    items: &[SessionManagerDisplayItem],
    roots: &[String],
    recent_title: String,
    hosts_title: String,
    card_columns: usize,
    recent_columns: usize,
) -> Vec<SessionManagerGridRow> {
    let mut rows = Vec::new();
    let recent_indices = recent_session_item_indices(items);
    push_session_manager_grid_section(
        &mut rows,
        recent_title,
        &recent_indices,
        recent_columns,
        true,
    );

    // Grid mode keeps each root group as one section containing its subtree.
    for group in roots {
        let group_indices = session_item_indices_for_group_subtree(items, group);
        push_session_manager_grid_section(
            &mut rows,
            group_display_name(group),
            &group_indices,
            card_columns,
            false,
        );
    }

    let host_indices = if roots.is_empty() {
        (0..items.len()).collect::<Vec<_>>()
    } else {
        direct_session_item_indices_for_group(items, None)
    };
    push_session_manager_grid_section(&mut rows, hosts_title, &host_indices, card_columns, false);
    rows
}

fn push_session_manager_grid_section(
    rows: &mut Vec<SessionManagerGridRow>,
    title: String,
    item_indices: &[usize],
    columns: usize,
    recent: bool,
) {
    if item_indices.is_empty() {
        return;
    }
    rows.push(SessionManagerGridRow::SectionHeader {
        title,
        item_count: item_indices.len(),
    });
    let chunk_count = item_indices.len().div_ceil(columns.max(1));
    for (row_index, chunk) in item_indices.chunks(columns.max(1)).enumerate() {
        if recent {
            rows.push(SessionManagerGridRow::RecentItems {
                item_indices: chunk.to_vec(),
                is_last_in_section: row_index + 1 == chunk_count,
            });
        } else {
            rows.push(SessionManagerGridRow::Cards {
                item_indices: chunk.to_vec(),
            });
        }
    }
}

pub(super) fn session_manager_tree_rows(
    items: &[SessionManagerDisplayItem],
    roots: &[String],
    children: &HashMap<String, Vec<String>>,
    expanded_groups: &HashSet<String>,
) -> Vec<SessionManagerTreeRow> {
    let mut rows = Vec::new();
    for root in roots {
        push_session_manager_tree_group_rows(&mut rows, root, 0, items, children, expanded_groups);
    }
    rows.extend(
        direct_session_item_indices_for_group(items, None)
            .into_iter()
            .map(|item_index| SessionManagerTreeRow::Item {
                item_index,
                depth: 0,
            }),
    );
    rows
}

fn push_session_manager_tree_group_rows(
    rows: &mut Vec<SessionManagerTreeRow>,
    group: &str,
    depth: usize,
    items: &[SessionManagerDisplayItem],
    children: &HashMap<String, Vec<String>>,
    expanded_groups: &HashSet<String>,
) {
    let group_item_indices = direct_session_item_indices_for_group(items, Some(group));
    let child_groups = children.get(group).map(Vec::as_slice).unwrap_or_default();
    let expanded = expanded_groups.contains(group);
    rows.push(SessionManagerTreeRow::Group {
        path: group.to_string(),
        depth,
        expanded,
        has_children: !child_groups.is_empty() || !group_item_indices.is_empty(),
    });
    if !expanded {
        return;
    }

    for child_group in child_groups {
        push_session_manager_tree_group_rows(
            rows,
            child_group,
            depth + 1,
            items,
            children,
            expanded_groups,
        );
    }
    rows.extend(
        group_item_indices
            .into_iter()
            .map(|item_index| SessionManagerTreeRow::Item {
                item_index,
                depth: depth + 1,
            }),
    );
}

fn recent_session_item_indices(items: &[SessionManagerDisplayItem]) -> Vec<usize> {
    let mut indices = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| item.last_used().is_some().then_some(index))
        .collect::<Vec<_>>();
    indices.sort_by(|left, right| items[*right].last_used().cmp(&items[*left].last_used()));
    indices.truncate(8);
    indices
}

fn direct_session_item_indices_for_group(
    items: &[SessionManagerDisplayItem],
    group: Option<&str>,
) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match (group, item.group()) {
            (None, None) => Some(index),
            (Some(group), Some(item_group)) if item_group == group => Some(index),
            _ => None,
        })
        .collect()
}

fn session_item_indices_for_group_subtree(
    items: &[SessionManagerDisplayItem],
    group: &str,
) -> Vec<usize> {
    let child_prefix = format!("{group}/");
    items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            item.group()
                .is_some_and(|item_group| {
                    item_group == group || item_group.starts_with(&child_prefix)
                })
                .then_some(index)
        })
        .collect()
}

fn session_manager_display_item_signature(item: &SessionManagerDisplayItem) -> u64 {
    let mut hasher = DefaultHasher::new();
    std::mem::discriminant(item).hash(&mut hasher);
    item.id().hash(&mut hasher);
    item.name().hash(&mut hasher);
    item.group().hash(&mut hasher);
    item.subtitle().hash(&mut hasher);
    item.last_used().hash(&mut hasher);
    item.icon_color().hash(&mut hasher);
    item.icon_background_color().hash(&mut hasher);
    hasher.finish()
}

fn session_manager_grid_row_signature(
    row: &SessionManagerGridRow,
    items: &[SessionManagerDisplayItem],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        SessionManagerGridRow::SectionHeader { title, item_count } => {
            0_u8.hash(&mut hasher);
            title.hash(&mut hasher);
            item_count.hash(&mut hasher);
        }
        SessionManagerGridRow::RecentItems {
            item_indices,
            is_last_in_section,
        } => {
            1_u8.hash(&mut hasher);
            is_last_in_section.hash(&mut hasher);
            hash_session_manager_row_items(item_indices, items, &mut hasher);
        }
        SessionManagerGridRow::Cards { item_indices } => {
            2_u8.hash(&mut hasher);
            hash_session_manager_row_items(item_indices, items, &mut hasher);
        }
    }
    hasher.finish()
}

fn session_manager_tree_row_signature(
    row: &SessionManagerTreeRow,
    items: &[SessionManagerDisplayItem],
) -> u64 {
    let mut hasher = DefaultHasher::new();
    match row {
        SessionManagerTreeRow::Group {
            path,
            depth,
            expanded,
            has_children,
        } => {
            0_u8.hash(&mut hasher);
            path.hash(&mut hasher);
            depth.hash(&mut hasher);
            expanded.hash(&mut hasher);
            has_children.hash(&mut hasher);
        }
        SessionManagerTreeRow::Item { item_index, depth } => {
            1_u8.hash(&mut hasher);
            depth.hash(&mut hasher);
            if let Some(item) = items.get(*item_index) {
                session_manager_display_item_signature(item).hash(&mut hasher);
            }
        }
    }
    hasher.finish()
}

fn hash_session_manager_row_items(
    item_indices: &[usize],
    items: &[SessionManagerDisplayItem],
    hasher: &mut DefaultHasher,
) {
    for item_index in item_indices {
        if let Some(item) = items.get(*item_index) {
            session_manager_display_item_signature(item).hash(hasher);
        }
    }
}

pub(super) fn group_display_name(group: &str) -> String {
    group.rsplit('/').next().unwrap_or(group).to_string()
}

pub(super) fn collect_session_group_paths(
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

#[cfg(test)]
mod session_manager_pointer_tests {
    use super::*;

    #[test]
    fn saved_connection_click_selects_and_double_click_opens() {
        assert_eq!(
            session_manager_item_pointer_action(1, true),
            SessionManagerItemPointerAction::Select
        );
        assert_eq!(
            session_manager_item_pointer_action(2, true),
            SessionManagerItemPointerAction::Open
        );
        assert_eq!(
            session_manager_item_pointer_action(1, false),
            SessionManagerItemPointerAction::None
        );
        assert_eq!(
            session_manager_item_pointer_action(2, false),
            SessionManagerItemPointerAction::Open
        );
    }
}
