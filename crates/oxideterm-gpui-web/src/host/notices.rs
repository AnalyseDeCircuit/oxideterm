use super::*;
use oxideterm_gpui_ui::toast::ToastVariant;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoticeTarget {
    Session(TerminalSessionId),
    Page(TabId),
    EmbeddedFiles(EntityId),
}

impl Workspace {
    pub fn notifications_notice(&mut self, title: String, cx: &mut Context<Self>) {
        self.record_notice(
            NotificationKind::Connection,
            ToastVariant::Default,
            title,
            None,
            None,
            cx,
        );
    }

    pub(crate) fn session_error(
        &mut self,
        id: TerminalSessionId,
        title: String,
        cx: &mut Context<Self>,
    ) {
        if self.sessions.contains_key(&id) {
            self.record_notice(
                NotificationKind::Connection,
                ToastVariant::Error,
                title,
                None,
                Some(NoticeTarget::Session(id)),
                cx,
            );
        }
    }

    pub(crate) fn record_notice(
        &mut self,
        kind: NotificationKind,
        variant: ToastVariant,
        title: String,
        body: Option<String>,
        target: Option<NoticeTarget>,
        cx: &mut Context<Self>,
    ) {
        let severity = match variant {
            ToastVariant::Error => NotificationSeverity::Error,
            ToastVariant::Warning => NotificationSeverity::Warning,
            _ => NotificationSeverity::Info,
        };
        let scope = match target {
            Some(NoticeTarget::Session(id)) => self
                .sessions
                .get(&id)
                .map(|s| NotificationScope::Node(s.node.clone()))
                .unwrap_or(NotificationScope::Global),
            _ => NotificationScope::Global,
        };
        self.notifications
            .push(kind, severity, title.clone(), body.clone(), scope, None);
        if let Some(oldest) = self.notifications.entries.front() {
            self.notice_targets.retain(|id, _| *id >= oldest.id);
        }
        if let Some(target) = target
            && let Some(entry) = self.notifications.entries.back()
        {
            self.notice_targets.insert(entry.id, target);
        }
        self.transients.push_notice(
            ToastContent {
                title,
                description: body,
                variant,
                status_text: None,
                progress: None,
            },
            Duration::from_secs(if variant == ToastVariant::Error { 8 } else { 4 }),
            target.map(|_| ()),
            Instant::now(),
        );
        self.arm_deadline(cx);
        self.refresh_center(cx);
        cx.notify();
    }

    pub(crate) fn forget_notice_target(&mut self, target: NoticeTarget, cx: &mut Context<Self>) {
        let before = self.notice_targets.len();
        self.notice_targets.retain(|_, t| *t != target);
        if before != self.notice_targets.len() {
            self.refresh_center(cx);
        }
    }

    fn show_notice_target(&mut self, id: u64, w: &mut Window, cx: &mut Context<Self>) {
        match self.notice_targets.get(&id).copied() {
            Some(NoticeTarget::EmbeddedFiles(entity)) => self.show_sidebar_notice(entity,cx),
            Some(NoticeTarget::Page(page)) if self.layout.tab_by_id(page).is_some() => {
                self.activate_page(page, cx);
                w.focus(&self.focus, cx);
            }
            Some(NoticeTarget::Session(session)) => {
                let Some(tab) = self.session_tab(session) else {
                    return;
                };
                let Some(pane) = self
                    .layout
                    .tab_by_id(tab)
                    .and_then(|t| t.root_pane.as_ref()?.pane_id_for_session(session))
                else {
                    return;
                };
                self.activate_page(tab, cx);
                self.pane_request(
                    ShellPaneRequest::Focus(tab, pane, PaneContent::Terminal(session)),
                    w,
                    cx,
                );
            }
            _ => return,
        }
        self.notifications.mark_read(id);
        self.changed(cx);
    }
    pub(crate) fn refresh_center(&self, cx: &mut Context<Self>) {
        let frame = frame(
            self.tokens,
            &self.i18n,
            self.wallpaper.as_ref().is_some_and(|bg|bg.includes("notification_center")),
            &self.notifications,
            &self.notice_targets,
            cx.weak_entity(),
        );
        self.center.update(cx, |v, cx| v.update_frame(frame, cx));
    }
}
pub(super) fn frame(
    tokens: ThemeTokens,
    i: &I18n,
    has_background:bool,
    n: &NotificationState,
    targets: &HashMap<u64, NoticeTarget>,
    host: WeakEntity<Workspace>,
) -> NotificationFrame {
    NotificationFrame {
        tokens,
        i18n: i.clone(),
        notifications: n.clone(),
        active_view: ActivityView::Notifications,
        has_background,
        transition: false,
        actions: NotificationActions::Objects(
            targets
                .keys()
                .map(|id| (*id, "event_log.actions.show_target"))
                .collect(),
        ),
        request: Rc::new(move |r, w, cx| {
            let _ = host.update(cx, |h, cx| {
                use NotificationRequest::*;
                match r {
                    MarkRead(id) => h.notifications.mark_read(id),
                    MarkAllRead => h.notifications.mark_all_read(),
                    ToggleDnd => h.notifications.toggle_dnd(),
                    CycleStatus => h.notifications.cycle_status_filter(),
                    CycleSeverity => h.notifications.cycle_severity_filter(),
                    CycleKind => h.notifications.cycle_kind_filter(),
                    Clear => {
                        h.notifications.clear();
                        h.notice_targets.clear();
                    }
                    Dismiss(id) => {
                        h.notifications.remove(id);
                        h.notice_targets.remove(&id);
                    }
                    InvokeAction(id) => h.show_notice_target(id, w, cx),
                    SelectView(_) => return,
                }
                h.refresh_center(cx);
                cx.notify();
            });
        }),
    }
}
