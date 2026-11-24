use super::{chrome::glyph, *};
use oxideterm_gpui_connections::session_manager::SessionManagerOpenTarget;
use oxideterm_gpui_shell::chrome::activity_button;
use oxideterm_gpui_ui::motion::{MotionDuration, exit_settle_duration};

impl Workspace {
    pub(super) fn collapse(&mut self, side: RailSide, close: bool, cx: &mut Context<Self>) {
        let rail = self.rails.rail_mut(side);
        let generation = rail.set_collapsed(close, &self.tokens);
        if close && rail.mounted() {
            let delay = exit_settle_duration(&self.tokens, MotionDuration::Control);
            at_deadline(Instant::now() + delay, cx, move |h, _, cx| {
                if h.rails.rail_mut(side).finish_collapse(generation) {
                    cx.notify();
                }
            })
            .detach();
        }
    }

    fn separator(&self) -> Div {
        let m = self.tokens.metrics;
        div()
            .flex_none()
            .w(px(m.divider_width))
            .h(px(m.divider_height))
            .mt(px(2.))
            .mb(px(m.activity_icon_gap + 2.))
            .bg(self.chrome_divider())
    }

    fn rail_button(
        &self,
        icon: LucideIcon,
        key: &'static str,
        active: bool,
        count: u32,
        cx: &Context<Self>,
        activate: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + Clone + 'static,
    ) -> AnyElement {
        let t = &self.tokens;
        let critical = icon == LucideIcon::Bell && self.notifications.unread_critical_count > 0;
        let dimmed = matches!(
            icon,
            LucideIcon::Activity | LucideIcon::Docker | LucideIcon::ServerCog
        ) && !self.layout.active_tab().is_some_and(|tab| {
            matches!(
                tab.kind,
                TabKind::LocalTerminal
                    | TabKind::SshTerminal
                    | TabKind::Forwards
                    | TabKind::Sftp
                    | TabKind::Ide
            )
        }) && (self.rails.right.collapsed()
            || self.rails.context_panel != ContextSidebarPanel::HostTools);
        let button = activity_button(
            t,
            icon,
            active,
            dimmed,
            rgb(t.ui.shell_active_bg()),
            count,
            if critical { t.ui.error } else { t.ui.accent },
        );
        self.bind_button(button, key, self.i18n.t(key), None, cx, activate)
    }

    fn page_button(
        &self,
        action: Action,
        icon: LucideIcon,
        kind: TabKind,
        key: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        let active = self.layout.active_tab().is_some_and(|t| t.kind == kind);
        let count = if action == Action::Notifications && !self.notifications.dnd_enabled {
            self.notifications.unread_count
        } else {
            0
        };
        self.rail_button(icon, key, active, count, cx, move |h, w, cx| {
            h.action(action, w, cx)
        })
    }

    fn drawer_button(
        &self,
        section: SidebarSection,
        icon: LucideIcon,
        key: &'static str,
        cx: &Context<Self>,
    ) -> AnyElement {
        self.rail_button(
            icon,
            key,
            self.rails.section == section && !self.rails.left.collapsed(),
            0,
            cx,
            move |h, w, cx| h.request(ShellRequest::Select(section), w, cx),
        )
    }

    pub(super) fn left_activity_rail(
        &self,
        env: &ShellEnvironment,
        cx: &Context<Self>,
    ) -> AnyElement {
        use SidebarSection::*;
        let mut primary = div()
            .id("activity-primary-items")
            .flex_1()
            .min_h_0()
            .w_full()
            .pt(px(4.))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center();
        for (section, icon) in SidebarSection::PRIMARY_ITEMS {
            primary = match section {
                Sessions => {
                    primary.child(self.drawer_button(section, icon, "sidebar.panels.sessions", cx))
                }
                Connections if self.demo => primary.child(self.page_button(
                    Action::Catalog,
                    icon,
                    TabKind::SessionManager,
                    "sidebar.panels.session_manager",
                    cx,
                )),
                _ => primary,
            };
        }
        if self.demo {
            primary = primary
                .child(self.drawer_button(
                    Forwards,
                    LucideIcon::ArrowLeftRight,
                    "forwards.table.title",
                    cx,
                ))
                .child(self.separator())
                .child(self.page_button(
                    Action::Sftp,
                    LucideIcon::FolderInput,
                    TabKind::Sftp,
                    Action::Sftp.key(),
                    cx,
                ))
                .child(self.page_button(
                    Action::Ide,
                    LucideIcon::Code2,
                    TabKind::Ide,
                    Action::Ide.key(),
                    cx,
                ));
        }
        let mut footer = div()
            .w_full()
            .flex()
            .flex_col()
            .items_center()
            .child(self.separator().mt_0());
        for (section, icon) in SidebarSection::FOOTER_ITEMS {
            footer = match section {
                Workspace if self.ready => footer.child(self.rail_button(
                    icon,
                    "sidebar.actions.new_local_terminal",
                    false,
                    self.sessions.values().filter(|s| s.node == "local").count() as u32,
                    cx,
                    |h, w, cx| h.action(Action::Terminal, w, cx),
                )),
                Files if self.demo => footer.child(self.page_button(
                    Action::Files,
                    icon,
                    TabKind::FileManager,
                    "sidebar.panels.files",
                    cx,
                )),
                Notifications => footer.child(self.page_button(
                    Action::Notifications,
                    icon,
                    TabKind::NotificationCenter,
                    "sidebar.panels.notifications",
                    cx,
                )),
                Settings => footer.child(self.page_button(
                    Action::Settings,
                    icon,
                    TabKind::Settings,
                    "sidebar.tooltips.settings",
                    cx,
                )),
                _ => footer,
            };
        }
        let body = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .pb_2()
            .flex()
            .flex_col()
            .items_center()
            .bg(env.chrome_background)
            .child(primary)
            .child(footer);
        activity_rail(RailSide::Left, !self.rails.left.collapsed(), body, env, cx)
    }

    pub(super) fn right_activity_rail(
        &self,
        env: &ShellEnvironment,
        cx: &Context<Self>,
    ) -> AnyElement {
        let visible = !self.rails.right.collapsed();
        let selected = self
            .tools
            .monitor
            .as_ref()
            .map(|m| m.read(cx).active_tool(cx));
        let mut body = div()
            .id("context-activity-items")
            .overflow_y_scroll()
            .flex_1()
            .min_h_0()
            .w_full()
            .pt(px(4.))
            .flex()
            .flex_col()
            .items_center()
            .bg(env.chrome_background)
            .child(self.rail_button(
                LucideIcon::Sparkles,
                Action::Assistant.key(),
                visible && self.rails.context_panel == ContextSidebarPanel::Assistant,
                0,
                cx,
                |h, w, cx| h.action(Action::Assistant, w, cx),
            ))
            .child(self.separator());
        for (tool, action) in [
            (ContextSidebarTool::Monitor, Action::Monitor),
            (ContextSidebarTool::Docker, Action::Docker),
            (ContextSidebarTool::Services, Action::Services),
        ] {
            if tool == ContextSidebarTool::Services {
                body = body.child(self.separator());
            }
            body = body.child(self.rail_button(
                tool.icon(),
                tool.label_key(),
                visible
                    && self.rails.context_panel == ContextSidebarPanel::HostTools
                    && selected == Some(tool),
                0,
                cx,
                move |h, w, cx| h.action(action, w, cx),
            ));
        }
        activity_rail(RailSide::Right, visible, body, env, cx)
    }

    fn drawer(&self, header: Div, content: AnyElement) -> AnyElement {
        let t = self.tokens;
        div()
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                header
                    .w_full()
                    .min_w_0()
                    .flex_none()
                    .h(px(t.metrics.tabbar_height))
                    .flex()
                    .items_center()
                    .bg(rgb(t.ui.shell_bg()))
                    .occlude()
                    .border_b_1()
                    .border_color(self.chrome_divider()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .bg(rgb(t.ui.shell_bg()))
                    .child(content),
            )
            .into_any_element()
    }

    pub(super) fn right_drawer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let t = self.tokens;
        let (icon, key, content) = match self.rails.context_panel {
            ContextSidebarPanel::Assistant => (
                LucideIcon::Sparkles,
                Action::Assistant.key(),
                self.assistant.as_ref()?.clone().into_any_element(),
            ),
            ContextSidebarPanel::HostTools => {
                let monitor = self.tools.monitor.as_ref()?;
                let width = self.rails.right.width();
                if monitor.read(cx).width != width {
                    monitor.update(cx, |m, cx| {
                        m.width = width;
                        cx.notify();
                    });
                }
                let tool = monitor.read(cx).active_tool(cx);
                (
                    tool.icon(),
                    tool.label_key(),
                    monitor.clone().into_any_element(),
                )
            }
        };
        let header = div()
            .px_3()
            .gap(px(t.spacing.two))
            .child(glyph(icon, t.metrics.tab_icon_size, t.ui.text_secondary))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(t.metrics.titlebar_label_font_size))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.i18n.t(key)),
            );
        Some(self.drawer(header, content))
    }

    pub(super) fn left_drawer(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let forwards = self.rails.section == SidebarSection::Forwards;
        let title = self.i18n.t(if forwards {
            "forwards.table.title"
        } else {
            "sidebar.panels.sessions_header"
        });
        let mut header = div().px(px(10.)).gap(px(4.)).child(
            div()
                .flex_1()
                .min_w_0()
                .truncate()
                .text_size(px(t.metrics.sidebar_title_font_size))
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(t.ui.text_muted))
                .child(title),
        );
        let size = t.metrics.sidebar_action_size;
        if self.demo {
            header = header.child(self.toolbar_button(
                if forwards {
                    Action::Forwards
                } else {
                    Action::Catalog
                },
                LucideIcon::ExternalLink,
                "sidebar.tooltips.open_in_tab",
                size,
                cx,
            ));
        }
        if !forwards && self.demo {
            header = header.child(self.toolbar_button(
                Action::NewConnection,
                LucideIcon::Plus,
                "sidebar.tooltips.new_connection",
                size,
                cx,
            ));
        }
        let content = if forwards {
            self.tools.forwards_sidebar(&t, &self.i18n)
        } else {
            self.sessions_sidebar(cx)
        };
        self.drawer(header, content)
    }

    fn sessions_sidebar(&self, cx: &Context<Self>) -> AnyElement {
        let t = self.tokens;
        let mut rows = Vec::new();
        for (_, tab) in self.layout.main_tabs() {
            let Some(root) = &tab.root_pane else { continue };
            let mut panes = Vec::new();
            root.collect_pane_ids(&mut panes);
            for pane in panes {
                let Some(id) = root.session_id_for_pane(pane) else {
                    continue;
                };
                let Some(session) = self.sessions.get(&id) else {
                    continue;
                };
                let tab_id = tab.id;
                let active = self.active_session() == Some(id);
                let profile = self.catalog_data.items.iter().find(|row| matches!(&row.target, SessionManagerOpenTarget::Connection(node) if node == &session.node));
                let label = profile.map_or_else(|| tab.title.clone(), |row| row.name.clone());
                let detail = profile.map(|row| row.subtitle.as_str());
                let button = self.session_row(
                    &label,
                    detail,
                    if session.node == "local" {
                        LucideIcon::Square
                    } else {
                        LucideIcon::Terminal
                    },
                    self.ready,
                    active,
                );
                rows.push(self.bind_button(
                    button,
                    ("session-row", id.0),
                    label,
                    None,
                    cx,
                    move |h, w, cx| {
                        h.layout.select_main_tab(Some(tab_id));
                        h.pane_request(
                            ShellPaneRequest::Focus(tab_id, pane, PaneContent::Terminal(id)),
                            w,
                            cx,
                        );
                    },
                ));
            }
        }
        for row in &self.catalog_data.items {
            let SessionManagerOpenTarget::Connection(node) = &row.target else {
                continue;
            };
            if self.sessions.values().any(|s| &s.node == node) {
                continue;
            }
            let node = node.clone();
            let button = self.session_row(
                &row.name,
                Some(&row.subtitle),
                LucideIcon::Server,
                false,
                false,
            );
            rows.push(self.bind_button(
                button,
                SharedString::from(format!("profile-{node}")),
                row.subtitle.clone(),
                None,
                cx,
                move |h, w, cx| h.new_terminal(node.clone(), None, w, cx),
            ));
        }
        div()
            .id("sessions-sidebar-list")
            .size_full()
            .overflow_y_scroll()
            .pt(px(4.))
            .px_1()
            .when(rows.is_empty(), |d| {
                d.child(
                    div()
                        .h(px(t.metrics.empty_sidebar_height))
                        .flex()
                        .flex_col()
                        .justify_center()
                        .items_center()
                        .gap_2()
                        .child(glyph(
                            LucideIcon::Server,
                            t.metrics.empty_sidebar_icon_size,
                            t.ui.text_muted,
                        ))
                        .text_size(px(t.metrics.empty_sidebar_title_font_size))
                        .text_color(rgb(t.ui.text_muted))
                        .child(self.i18n.t("sessions.tree.no_sessions")),
                )
            })
            .children(rows)
            .into_any_element()
    }

    fn session_row(
        &self,
        label: &str,
        detail: Option<&str>,
        icon: LucideIcon,
        connected: bool,
        active: bool,
    ) -> Div {
        let t = self.tokens;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.))
            .px_2()
            .py_1()
            .rounded(px(t.radii.md))
            .cursor_pointer()
            .when(active, |d| d.bg(rgba((t.ui.accent << 8) | 0x1a)))
            .hover(move |d| d.bg(rgb(t.ui.bg_hover)))
            .child(glyph(icon, 14., t.ui.text_secondary))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(t.metrics.ui_text_sm))
                    .child(div().truncate().child(label.to_owned()))
                    .when_some(detail, |d, s| {
                        d.child(
                            div()
                                .truncate()
                                .text_size(px(t.metrics.ui_text_xs))
                                .text_color(rgb(t.ui.text_muted))
                                .child(s.to_owned()),
                        )
                    }),
            )
            .child(
                div()
                    .flex_none()
                    .size(px(6.))
                    .rounded_full()
                    .bg(rgb(if connected {
                        t.ui.success
                    } else {
                        t.ui.text_muted
                    })),
            )
    }
}
