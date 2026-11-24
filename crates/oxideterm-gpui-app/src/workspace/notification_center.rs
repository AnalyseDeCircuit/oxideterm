use super::*;
use oxideterm_gpui_shell::notifications::{
    NOTIFICATION_CENTER_SWITCHER_ID, NotificationRequest, notification_center_view_index,
};

impl WorkspaceApp {
    pub(super) fn open_notification_center_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self
            .tabs(cx)
            .iter()
            .find(|tab| tab.kind == TabKind::NotificationCenter)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.insert_tab(
                Tab {
                    id: tab_id,
                    kind: TabKind::NotificationCenter,
                    title: self.i18n.t("event_log.audit.center_title"),
                    title_source: TabTitleSource::I18nKey("event_log.audit.center_title"),
                    root_pane: None,
                    active_pane_id: None,
                },
                cx,
            );
            tab_id
        };
        if self.notification_center.active_view == WorkspaceActivityView::EventLog {
            self.audit.update(cx, |audit, cx| audit.show_page(cx));
        } else {
            self.audit.update(cx, |audit, cx| audit.hide_audit_page(cx));
        }
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
        use oxideterm_gpui_shell::notifications::*;
        let host = cx.weak_entity();
        let frame = NotificationFrame {
            tokens: self.tokens.clone(),
            i18n: self.i18n.clone(),
            notifications: self.notification_center.notifications.clone(),
            active_view: self.notification_center.active_view,
            has_background: self.background_surface_active("notification_center"),
            actions_available: true,
            transition: self.segmented_control_user_transition_active(
                NOTIFICATION_CENTER_SWITCHER_ID,
                notification_center_view_index(self.notification_center.active_view),
            ),
            request: Rc::new(move |request, window, cx| {
                let _ = host.update(cx, |app, cx| app.notification_request(request, window, cx));
            }),
        };
        let mut mounted = self.notification_surface.borrow_mut();
        if let Some(surface) = mounted.as_ref() {
            surface.update(cx, |view, cx| view.update_frame(frame, cx));
            surface.clone().into_any_element()
        } else {
            mounted
                .insert(cx.new(|_| NotificationCenter::new(frame, Some(self.audit.clone().into()))))
                .clone()
                .into_any_element()
        }
    }
    fn notification_request(
        &mut self,
        request: NotificationRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = &mut self.notification_center.notifications;
        match request {
            NotificationRequest::ToggleDnd => state.toggle_dnd(),
            NotificationRequest::CycleStatus => state.cycle_status_filter(),
            NotificationRequest::CycleSeverity => state.cycle_severity_filter(),
            NotificationRequest::CycleKind => state.cycle_kind_filter(),
            NotificationRequest::MarkAllRead => state.mark_all_read(),
            NotificationRequest::Clear => state.clear(),
            NotificationRequest::MarkRead(id) => state.mark_read(id),
            NotificationRequest::Dismiss(id) => state.remove(id),
            NotificationRequest::InvokeAction(id) => {
                if let Some(entry) = state.entries.iter().find(|entry| entry.id == id).cloned() {
                    self.open_notification_audit(&entry, window, cx);
                }
            }
            NotificationRequest::SelectView(view) => {
                if self.notification_center.active_view == view {
                    return;
                }
                self.notification_center.active_view = view;
                self.mark_active_notification_center_view_read();
                self.audit.update(cx, |audit, cx| {
                    if view == WorkspaceActivityView::EventLog {
                        audit.show_page(cx)
                    } else {
                        audit.hide_audit_page(cx)
                    }
                });
                self.begin_user_segmented_control_transition(
                    NOTIFICATION_CENTER_SWITCHER_ID,
                    notification_center_view_index(view),
                    cx,
                );
            }
        }
        cx.notify();
    }
}
