use super::*;
use oxideterm_gpui_ui::{
    SurfaceKind, SurfaceOptions, SurfacePadding, segmented_control, segmented_control_item_content,
    semantic_surface,
};

const NOTIFICATION_CENTER_TAB_BAR_WIDTH: f32 = 300.0; // Two localized view labels share the compact page-header switcher.

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
        let has_background = self.background_surface_active("notification_center");
        let page_padding = self.tokens.metrics.settings_content_padding;
        let page_gap = self.tokens.metrics.settings_page_gap;
        div()
            .size_full()
            .overflow_hidden()
            .flex()
            .flex_col()
            .gap(px(page_gap))
            .p(px(page_padding))
            .bg(if has_background {
                rgba(0x00000000)
            } else {
                rgb(theme.bg)
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_start()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .min_w(px(280.0))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .text_size(px(self.tokens.metrics.ui_text_2xl))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .text_color(rgb(theme.text_heading))
                                    .child(self.i18n.t("sidebar.panels.notifications")),
                            )
                            .child(
                                div()
                                    .max_w(px(680.0))
                                    .text_size(px(self.tokens.metrics.ui_text_base))
                                    .line_height(px(22.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(self.i18n.t("event_log.center_description")),
                            ),
                    )
                    .child(self.render_notification_center_tab_bar(has_background, cx)),
            )
            .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
            .child(
                semantic_surface(
                    &self.tokens,
                    SurfaceOptions::new(SurfaceKind::Inspector)
                        .padding(SurfacePadding::None)
                        .has_background_image(has_background),
                )
                .w_full()
                .flex_1()
                .min_h(px(0.0))
                .flex()
                .flex_col()
                .overflow_hidden()
                .child(match self.notification_center.active_view {
                    WorkspaceActivityView::Notifications => {
                        self.render_notifications_center_content(cx)
                    }
                    WorkspaceActivityView::EventLog => self.render_event_log_center_content(cx),
                }),
            )
            .into_any_element()
    }

    fn render_notification_center_tab_bar(
        &self,
        has_background: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let notification_count = if self.notification_center.notifications.dnd_enabled {
            0
        } else {
            self.notification_center.notifications.unread_count
        };
        let event_count = if self.notification_center.event_log.dnd_enabled {
            0
        } else {
            self.notification_center.event_log.unread_count
        };
        let items = vec![
            self.render_notification_center_tab(
                WorkspaceActivityView::Notifications,
                LucideIcon::Bell,
                self.i18n.t("sidebar.panels.notifications"),
                notification_count,
                !self.notification_center.notifications.dnd_enabled
                    && self.notification_center.notifications.unread_critical_count > 0,
                cx,
            ),
            self.render_notification_center_tab(
                WorkspaceActivityView::EventLog,
                LucideIcon::History,
                self.i18n.t("sidebar.panels.event_log"),
                event_count,
                !self.notification_center.event_log.dnd_enabled
                    && self.notification_center.event_log.unread_errors > 0,
                cx,
            ),
        ];
        let active_index = notification_center_view_index(self.notification_center.active_view);
        let user_transition_active = self.segmented_control_user_transition_active(
            selection_motion::NOTIFICATION_CENTER_SWITCHER_ID,
            active_index,
        );
        // The opposite view is valid transition history only while a user
        // click token is active; restored state and remounts render settled.
        let previous_index = if user_transition_active {
            1usize.saturating_sub(active_index)
        } else {
            active_index
        };
        segmented_control(
            &self.tokens,
            selection_motion::NOTIFICATION_CENTER_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(active_index, previous_index, 2)
                .user_transition_active(user_transition_active)
                .has_background_image(has_background)
                .compact(NOTIFICATION_CENTER_TAB_BAR_WIDTH),
            items,
        )
        .into_any_element()
    }

    fn render_notification_center_tab(
        &self,
        view: WorkspaceActivityView,
        icon: LucideIcon,
        label: String,
        badge_count: u32,
        badge_error: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.notification_center.active_view == view;
        let content = div()
            .w_full()
            .py(px(2.0))
            .flex()
            .items_center()
            .justify_center()
            .gap(px(7.0))
            .child(Self::render_lucide_icon(
                icon,
                16.0,
                rgb(if active {
                    theme.accent
                } else {
                    theme.text_muted
                }),
            ))
            .child(label)
            .when(badge_count > 0, |content| {
                content.child(
                    div()
                        .min_w(px(18.0))
                        .h(px(18.0))
                        .px(px(5.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .bg(rgba(
                            (if badge_error {
                                theme.error
                            } else {
                                theme.accent
                            } << 8)
                                | 0x26,
                        ))
                        .text_size(px(self.tokens.metrics.ui_text_xs))
                        .text_color(rgb(if badge_error {
                            theme.error
                        } else {
                            theme.accent
                        }))
                        .child(if badge_count > 99 {
                            "99+".to_string()
                        } else {
                            badge_count.to_string()
                        }),
                )
            });
        segmented_control_item_content(&self.tokens, active, content.into_any_element())
            .font_weight(gpui::FontWeight::MEDIUM)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.notification_center.active_view != view {
                        this.notification_center.active_view = view;
                        this.mark_active_notification_center_view_read();
                        this.begin_user_segmented_control_transition(
                            selection_motion::NOTIFICATION_CENTER_SWITCHER_ID,
                            notification_center_view_index(view),
                            cx,
                        );
                    }
                    cx.notify();
                }),
            )
            .into_any_element()
    }
}

fn notification_center_view_index(view: WorkspaceActivityView) -> usize {
    match view {
        WorkspaceActivityView::Notifications => 0,
        WorkspaceActivityView::EventLog => 1,
    }
}
