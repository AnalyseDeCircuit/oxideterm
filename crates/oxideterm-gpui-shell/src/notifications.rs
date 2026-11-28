//! Notification presentation; the host owns delivery, storage and action targets.
use gpui::{
    AnyElement, App, Context, MouseButton, MouseDownEvent, Window, div, prelude::*, px, rgb, rgba,
    svg,
};
use oxideterm_gpui_ui::{
    SurfaceKind, SurfaceOptions, SurfacePadding, icons::LucideIcon, segmented_control,
    segmented_control_item_content, semantic_surface, virtual_list::*,
};
use oxideterm_i18n::I18n;
use oxideterm_notification_center::*;
use oxideterm_theme::ThemeTokens;
use std::{rc::Rc, sync::Arc};
pub const NOTIFICATION_CENTER_SWITCHER_ID: &str = "notification-center-tab-bar";
const ACTIVITY_TOOLBAR_BUTTON_SIZE: f32 = 28.0;
const ACTIVITY_TOOLBAR_ICON_SIZE: f32 = 15.0;
const ACTIVITY_TOOLBAR_GROUP_PADDING: f32 = 2.0;
const ACTIVITY_EMPTY_STATE_ICON_SIZE: f32 = 20.0;
const ACTIVITY_TOOLBAR_ACTIVE_BACKGROUND_ALPHA: u32 = 0x1f;
const ACTIVITY_TOOLBAR_ACTIVE_BORDER_ALPHA: u32 = 0x52;
const NOTIFICATION_CENTER_TAB_BAR_WIDTH: f32 = 300.0; // Two localized view labels share the compact page-header switcher.
const NOTIFICATION_SIDEBAR_ROW_HEIGHT_ESTIMATE: f32 = 72.;
const NOTIFICATION_SIDEBAR_VIRTUAL_OVERSCAN: usize = 10;
#[derive(Clone, Copy)]
pub enum NotificationRequest {
    SelectView(ActivityView),
    MarkRead(u64),
    MarkAllRead,
    ToggleDnd,
    CycleStatus,
    CycleSeverity,
    CycleKind,
    Clear,
    Dismiss(u64),
    InvokeAction(u64),
}
#[derive(Clone)]
pub enum NotificationActions {
    Audit,
    Objects(std::collections::HashMap<u64, &'static str>),
}

impl NotificationActions {
    fn label(&self, entry: &NotificationEntry) -> Option<&'static str> {
        match self {
            Self::Audit
                if entry.kind == NotificationKind::Security
                    || matches!(
                        entry.severity,
                        NotificationSeverity::Error | NotificationSeverity::Critical
                    ) =>
            {
                Some("event_log.actions.audit_view")
            }
            Self::Audit => None,
            Self::Objects(labels) => labels.get(&entry.id).copied(),
        }
    }
}

#[derive(Clone)]
pub struct NotificationFrame {
    pub tokens: ThemeTokens,
    pub i18n: I18n,
    pub notifications: NotificationState,
    pub active_view: ActivityView,
    pub has_background: bool,
    pub transition: bool,
    pub actions: NotificationActions,
    pub request: Rc<dyn Fn(NotificationRequest, &mut Window, &mut App)>,
}
pub struct NotificationCenter {
    list: gpui::ListState,
    cache: VirtualListSignatureCache,
    frame: Rc<NotificationFrame>,
    audit: Option<gpui::AnyView>,
}
impl NotificationCenter {
    pub fn new(frame: NotificationFrame, audit: Option<gpui::AnyView>) -> Self {
        Self {
            frame: Rc::new(frame),
            audit,
            list: tauri_virtual_list_state(
                0,
                gpui::ListAlignment::Top,
                TauriVirtualListSpec::new(
                    px(NOTIFICATION_SIDEBAR_ROW_HEIGHT_ESTIMATE),
                    NOTIFICATION_SIDEBAR_VIRTUAL_OVERSCAN,
                ),
            ),
            cache: Default::default(),
        }
    }
    pub fn update_frame(&mut self, frame: NotificationFrame, cx: &mut Context<Self>) {
        self.frame = Rc::new(frame);
        cx.notify();
    }
}
impl Render for NotificationCenter {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let list = if self.frame.active_view == ActivityView::Notifications {
            self.frame.content(&self.list, &mut self.cache)
        } else {
            div().into_any_element()
        };
        self.frame
            .page(list, self.audit.clone().map(|view| view.into_any_element()))
    }
}
impl NotificationFrame {
    fn listener(
        &self,
        event: NotificationRequest,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + use<> {
        let request = self.request.clone();
        move |_, window, cx| {
            request(event, window, cx);
            cx.stop_propagation();
        }
    }
    fn render_lucide_icon(icon: LucideIcon, size: f32, color: gpui::Rgba) -> AnyElement {
        svg()
            .path(icon.path())
            .size(px(size))
            .text_color(color)
            .into_any_element()
    }
    fn page(self: &Rc<Self>, list: AnyElement, audit: Option<AnyElement>) -> AnyElement {
        let theme = self.tokens.ui;
        let has_background = self.has_background;
        let page_padding = self.tokens.metrics.settings_content_padding;
        let page_gap = self.tokens.metrics.settings_page_gap;
        div()
            .relative()
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
                                    .child(self.i18n.t("event_log.audit.center_title")),
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
                    .when(audit.is_some(), |row| {
                        row.child(self.render_notification_center_tab_bar(has_background))
                    }),
            )
            .child(div().w_full().h(px(1.0)).bg(rgb(theme.border)))
            .child(match self.active_view {
                ActivityView::EventLog => audit.unwrap_or_else(|| div().into_any_element()),
                ActivityView::Notifications => semantic_surface(
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
                .child(list)
                .into_any_element(),
            })
            .into_any_element()
    }

    fn render_notification_center_tab_bar(&self, has_background: bool) -> AnyElement {
        let notification_count = if self.notifications.dnd_enabled {
            0
        } else {
            self.notifications.unread_count
        };
        let notifications_label = self.i18n.t("event_log.audit.notifications");
        let audit_label = self.i18n.t("event_log.audit.title");
        let notifications_badge = (notification_count > 0).then(|| notification_count.to_string());
        let tab_bar_width = oxideterm_gpui_ui::estimate_segmented_control_width(
            &self.tokens,
            NOTIFICATION_CENTER_TAB_BAR_WIDTH,
            480.0,
            true,
            [
                (notifications_label.as_str(), notifications_badge.as_deref()),
                (audit_label.as_str(), None),
            ],
        );
        let items = vec![
            self.render_notification_center_tab(
                ActivityView::Notifications,
                LucideIcon::Bell,
                notifications_label,
                notification_count,
                !self.notifications.dnd_enabled && self.notifications.unread_critical_count > 0,
            ),
            self.render_notification_center_tab(
                ActivityView::EventLog,
                LucideIcon::History,
                audit_label,
                0,
                false,
            ),
        ];
        let active_index = notification_center_view_index(self.active_view);
        let user_transition_active = self.transition;
        // The opposite view is valid transition history only while a user
        // click token is active; restored state and remounts render settled.
        let previous_index = if user_transition_active {
            1usize.saturating_sub(active_index)
        } else {
            active_index
        };
        segmented_control(
            &self.tokens,
            NOTIFICATION_CENTER_SWITCHER_ID,
            oxideterm_gpui_ui::SegmentedControlOptions::new(active_index, previous_index, 2)
                .user_transition_active(user_transition_active)
                .has_background_image(has_background)
                .compact(tab_bar_width),
            items,
        )
        .into_any_element()
    }

    fn render_notification_center_tab(
        &self,
        view: ActivityView,
        icon: LucideIcon,
        label: String,
        badge_count: u32,
        badge_error: bool,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let active = self.active_view == view;
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
                self.listener(NotificationRequest::SelectView(view)),
            )
            .into_any_element()
    }
    fn content(
        self: &Rc<Self>,
        state: &gpui::ListState,
        cache: &mut VirtualListSignatureCache,
    ) -> AnyElement {
        let filtered = self
            .notifications
            .entries
            .iter()
            .rev()
            .filter(|entry| self.notifications.matches_filter(entry))
            .cloned()
            .collect::<Vec<_>>();
        let row_count = filtered.len();
        let signatures = notification_sidebar_row_signatures(&filtered);
        let notification_spec = TauriVirtualListSpec::new(
            px(NOTIFICATION_SIDEBAR_ROW_HEIGHT_ESTIMATE),
            NOTIFICATION_SIDEBAR_VIRTUAL_OVERSCAN,
        );
        {
            sync_tauri_variable_list_state_by_signatures(
                state,
                cache,
                "notifications-sidebar",
                &signatures,
                notification_spec,
            );
        }
        let notification_rows = Arc::new(filtered);
        let notification_list_state = state.clone();
        let frame = self.clone();

        div()
            .flex_1()
            .min_h(px(0.0))
            .w_full()
            .flex()
            .flex_col()
            .child(self.render_notifications_toolbar())
            .child(
                div()
                    .id("notifications-sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_hidden()
                    .when(row_count == 0, |content| {
                        content.child(self.render_activity_empty_state(
                            LucideIcon::Inbox,
                            self.i18n.t("event_log.notification_empty"),
                        ))
                    })
                    .when(row_count > 0, |content| {
                        content.child(tauri_virtual_list(
                            notification_list_state,
                            notification_spec,
                            move |index, _window, _cx| {
                                frame.render_notification_row(&notification_rows[index])
                            },
                        ))
                    }),
            )
            .into_any_element()
    }

    fn render_notifications_toolbar(&self) -> AnyElement {
        self.activity_toolbar_shell()
            .flex()
            .items_center()
            .child(
                self.activity_toolbar_group()
                    .child(self.render_activity_icon_button(
                        LucideIcon::Bell,
                        self.notifications.dnd_enabled,
                        NotificationRequest::ToggleDnd,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::ListTree,
                        self.notifications.filter.status != NotificationStatusFilter::All,
                        NotificationRequest::CycleStatus,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::AlertCircle,
                        self.notifications.filter.severity != NotificationSeverityFilter::All,
                        NotificationRequest::CycleSeverity,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::Hash,
                        self.notifications.filter.kind != NotificationKindFilter::All,
                        NotificationRequest::CycleKind,
                    )),
            )
            .when(self.notifications.dnd_enabled, |toolbar| {
                toolbar.child(oxideterm_gpui_ui::status_pill(
                    &self.tokens,
                    self.i18n.t("event_log.dnd.on"),
                    oxideterm_gpui_ui::StatusPillOptions::new(
                        oxideterm_gpui_ui::StatusTone::Warning,
                    )
                    .compact(),
                ))
            })
            .child(div().flex_1())
            .child(
                self.activity_toolbar_group()
                    .child(self.render_activity_icon_button(
                        LucideIcon::Check,
                        false,
                        NotificationRequest::MarkAllRead,
                    ))
                    .child(self.render_activity_icon_button(
                        LucideIcon::Trash2,
                        false,
                        NotificationRequest::Clear,
                    )),
            )
            .into_any_element()
    }

    fn render_activity_icon_button(
        &self,
        icon: LucideIcon,
        active: bool,
        request: NotificationRequest,
    ) -> AnyElement {
        let theme = self.tokens.ui;
        let icon_color = if active {
            rgb(theme.accent)
        } else {
            rgb(theme.text_muted)
        };
        oxideterm_gpui_ui::button::icon_button(
            &self.tokens,
            Self::render_lucide_icon(icon, ACTIVITY_TOOLBAR_ICON_SIZE, icon_color),
            oxideterm_gpui_ui::button::IconButtonOptions {
                background: active
                    .then(|| rgba((theme.accent << 8) | ACTIVITY_TOOLBAR_ACTIVE_BACKGROUND_ALPHA)),
                border: active
                    .then(|| rgba((theme.accent << 8) | ACTIVITY_TOOLBAR_ACTIVE_BORDER_ALPHA)),
                hover_background: Some(rgb(theme.bg_hover)),
                ..oxideterm_gpui_ui::button::IconButtonOptions::opaque_toolbar(
                    ACTIVITY_TOOLBAR_BUTTON_SIZE,
                    oxideterm_gpui_ui::button::ButtonRadius::Sm,
                )
            },
        )
        .on_mouse_down(MouseButton::Left, self.listener(request))
        .into_any_element()
    }

    fn activity_toolbar_shell(&self) -> gpui::Div {
        let theme = self.tokens.ui;
        // The full-page toolbar uses compact grouped controls so icon actions
        // remain subordinate to the notification content and page switcher.
        div()
            .h(px(ACTIVITY_TOOLBAR_BUTTON_SIZE + self.tokens.spacing.three))
            .gap(px(self.tokens.spacing.two))
            .px(px(self.tokens.spacing.three))
            .border_b_1()
            .border_color(rgb(theme.border))
    }

    fn activity_toolbar_group(&self) -> gpui::Div {
        oxideterm_gpui_ui::semantic_surface(
            &self.tokens,
            oxideterm_gpui_ui::SurfaceOptions::new(oxideterm_gpui_ui::SurfaceKind::InsetGroup)
                .padding(oxideterm_gpui_ui::SurfacePadding::None)
                .has_background_image(self.has_background),
        )
        .flex()
        .items_center()
        .gap(px(ACTIVITY_TOOLBAR_GROUP_PADDING))
        .p(px(ACTIVITY_TOOLBAR_GROUP_PADDING))
    }

    fn render_activity_empty_state(&self, icon: LucideIcon, label: String) -> AnyElement {
        let theme = self.tokens.ui;
        oxideterm_gpui_ui::empty_state(
            &self.tokens,
            Self::render_lucide_icon(icon, ACTIVITY_EMPTY_STATE_ICON_SIZE, rgb(theme.accent)),
            label,
            None,
            None,
        )
        .into_any_element()
    }

    fn render_notification_row(&self, entry: &NotificationEntry) -> AnyElement {
        let theme = self.tokens.ui;
        let id = entry.id;
        let (icon, accent) = match entry.severity {
            NotificationSeverity::Info => (LucideIcon::Info, theme.accent),
            NotificationSeverity::Warning => (LucideIcon::AlertTriangle, theme.warning),
            NotificationSeverity::Error => (LucideIcon::AlertCircle, theme.error),
            NotificationSeverity::Critical => (LucideIcon::Shield, theme.error),
        };
        let kind = notification_kind_label(entry.kind);
        let status_unread = entry.status == NotificationStatus::Unread;
        let timestamp = entry
            .created_at
            .duration_since(web_time::SystemTime::UNIX_EPOCH)
            .map(|duration| duration.as_secs().to_string())
            .unwrap_or_else(|_| "0".to_string());
        let scope = match &entry.scope {
            NotificationScope::Global => "global".to_string(),
            NotificationScope::Node(node_id) => node_id.clone(),
            NotificationScope::Connection(connection_id) => connection_id.clone(),
        };

        let action = self.actions.label(entry);

        div()
            .w_full()
            .mb_2()
            .p_2()
            .rounded(px(self.tokens.radii.md))
            .bg(if status_unread {
                rgb(theme.bg_hover)
            } else {
                rgb(theme.bg)
            })
            .border_1()
            .border_color(rgb(theme.border))
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.0))
                    .child(div().mt(px(1.0)).child(Self::render_lucide_icon(
                        icon,
                        14.0,
                        rgb(accent),
                    )))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .min_w(px(0.0))
                                            .flex_1()
                                            .truncate()
                                            .text_size(px(12.0))
                                            .font_weight(if status_unread {
                                                gpui::FontWeight::SEMIBOLD
                                            } else {
                                                gpui::FontWeight::NORMAL
                                            })
                                            .text_color(rgb(theme.text_heading))
                                            .child(entry.title.clone()),
                                    )
                                    .when(
                                        status_unread && !self.notifications.dnd_enabled,
                                        |row| {
                                            row.child(
                                                div()
                                                    .size(px(6.0))
                                                    .rounded_full()
                                                    .bg(rgb(theme.accent)),
                                            )
                                        },
                                    ),
                            )
                            .when_some(entry.body.clone(), |body, detail| {
                                body.child(
                                    div()
                                        .mt_1()
                                        .text_size(px(11.0))
                                        .text_color(rgb(theme.text_muted))
                                        .child(detail),
                                )
                            })
                            .child(
                                div()
                                    .mt_1()
                                    .truncate()
                                    .text_size(px(10.0))
                                    .text_color(rgb(theme.text_muted))
                                    .child(format!("{timestamp} | {kind} | {scope}")),
                            ),
                    )
                    .when_some(action, |row, label| {
                        row.child(oxideterm_gpui_ui::button::toolbar_action_button(
                            &self.tokens,
                            self.i18n.t(label),
                            None,
                            oxideterm_gpui_ui::button::ToolbarButtonOptions::default(),
                            self.listener(NotificationRequest::InvokeAction(id)),
                        ))
                    })
                    .child(
                        div()
                            .size(px(20.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(self.tokens.radii.sm))
                            .cursor_pointer()
                            .hover(move |button| button.bg(rgb(theme.bg_hover)))
                            .child(Self::render_lucide_icon(
                                LucideIcon::X,
                                12.0,
                                rgb(theme.text_muted),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                self.listener(NotificationRequest::Dismiss(id)),
                            ),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                self.listener(NotificationRequest::MarkRead(id)),
            )
            .into_any_element()
    }
}
fn notification_kind_label(kind: NotificationKind) -> &'static str {
    match kind {
        NotificationKind::Connection => "connection",
        NotificationKind::Security => "security",
        NotificationKind::Transfer => "transfer",
        NotificationKind::Update => "update",
        NotificationKind::Health => "health",
        NotificationKind::Plugin => "plugin",
        NotificationKind::Agent => "agent",
    }
}

fn notification_sidebar_row_signatures(entries: &[NotificationEntry]) -> Vec<u64> {
    entries
        .iter()
        .map(|entry| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            // The variable-height list must be invalidated when visible text or
            // read state changes, mirroring React keys plus prop updates in
            // Tauri's NotificationsPanel.
            std::hash::Hash::hash(&entry.id, &mut hasher);
            std::hash::Hash::hash(&(entry.status as u8), &mut hasher);
            std::hash::Hash::hash(&(entry.kind as u8), &mut hasher);
            std::hash::Hash::hash(&(entry.severity as u8), &mut hasher);
            std::hash::Hash::hash(&entry.title, &mut hasher);
            std::hash::Hash::hash(&entry.body, &mut hasher);
            match &entry.scope {
                NotificationScope::Global => {
                    std::hash::Hash::hash(&0u8, &mut hasher);
                }
                NotificationScope::Node(node_id) => {
                    std::hash::Hash::hash(&1u8, &mut hasher);
                    std::hash::Hash::hash(node_id, &mut hasher);
                }
                NotificationScope::Connection(connection_id) => {
                    std::hash::Hash::hash(&2u8, &mut hasher);
                    std::hash::Hash::hash(connection_id, &mut hasher);
                }
            }
            std::hash::Hasher::finish(&hasher)
        })
        .collect()
}

pub fn notification_center_view_index(view: ActivityView) -> usize {
    match view {
        ActivityView::Notifications => 0,
        ActivityView::EventLog => 1,
    }
}
