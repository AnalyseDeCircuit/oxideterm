use super::*;

impl WorkspaceApp {
    pub(super) fn open_notification_center_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::NotificationCenter)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::NotificationCenter,
                title: self.i18n.t("sidebar.panels.notifications"),
                title_source: TabTitleSource::I18nKey("sidebar.panels.notifications"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        self.mark_active_notification_center_view_read();
        self.set_active_tab(tab_id, window, cx);
    }

    fn mark_active_notification_center_view_read(&mut self) {
        match self.notification_center.active_view {
            WorkspaceActivityView::Notifications => {
                self.notification_center.notifications.unread_count = 0;
                self.notification_center.notifications.unread_critical_count = 0;
            }
            WorkspaceActivityView::EventLog => {
                self.notification_center.event_log.mark_read();
            }
        }
    }

    pub(super) fn render_notification_center_surface(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        div()
            .size_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .bg(rgb(theme.bg))
            .child(
                div()
                    .px_8()
                    .pt_7()
                    .pb_4()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(Self::render_lucide_icon(
                                LucideIcon::Bell,
                                24.0,
                                rgb(theme.accent),
                            ))
                            .child(
                                div()
                                    .text_size(px(26.0))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.i18n.t("sidebar.panels.notifications")),
                            ),
                    ),
            )
            .child(
                div()
                    .px_8()
                    .py_3()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(self.render_activity_tab_button(
                        WorkspaceActivityView::Notifications,
                        LucideIcon::Bell,
                        self.i18n.t("sidebar.panels.notifications"),
                        if self.notification_center.notifications.dnd_enabled {
                            0
                        } else {
                            self.notification_center.notifications.unread_count
                        },
                        !self.notification_center.notifications.dnd_enabled
                            && self.notification_center.notifications.unread_critical_count > 0,
                        cx,
                    ))
                    .child(self.render_activity_tab_button(
                        WorkspaceActivityView::EventLog,
                        LucideIcon::History,
                        self.i18n.t("sidebar.panels.event_log"),
                        if self.notification_center.event_log.dnd_enabled {
                            0
                        } else {
                            self.notification_center.event_log.unread_count
                        },
                        !self.notification_center.event_log.dnd_enabled
                            && self.notification_center.event_log.unread_errors > 0,
                        cx,
                    )),
            )
            .child(
                div().flex_1().min_h(px(0.0)).px_8().py_5().child(
                    div()
                        .size_full()
                        .rounded(px(self.tokens.radii.md))
                        .border_1()
                        .border_color(rgb(theme.border))
                        .bg(rgb(theme.bg_panel))
                        .overflow_hidden()
                        .child(match self.notification_center.active_view {
                            WorkspaceActivityView::Notifications => {
                                self.render_notifications_sidebar_content(cx)
                            }
                            WorkspaceActivityView::EventLog => {
                                self.render_event_log_sidebar_content(cx)
                            }
                        }),
                ),
            )
            .into_any_element()
    }
}
