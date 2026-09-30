use super::*;
use oxideterm_audit::{AuditSessionQuery, AuditSessionSummary};

#[derive(Default)]
pub(super) struct AuditSessions {
    items: Arc<Vec<Arc<AuditSessionSummary>>>,
    cursors: Vec<Option<(i64, String)>>,
    next_cursor: Option<(i64, String)>,
}

impl WorkspaceApp {
    pub(super) fn refresh_audit_sessions(&mut self, reset: bool, cx: &mut Context<Self>) {
        if reset || self.audit.sessions.cursors.is_empty() {
            self.audit.sessions.cursors = vec![None];
        }
        let Some(client) = self.audit.client.clone() else {
            return;
        };
        let query = AuditSessionQuery {
            before: self.audit.sessions.cursors.last().cloned().flatten(),
            after_ms: self.audit.query.after_ms,
            until_ms: self.audit.query.until_ms,
            limit: 100,
        };
        self.audit.generation += 1;
        let generation = self.audit.generation;
        self.audit.loading = true;
        self.audit.error = None;
        self.audit.task = Some(cx.spawn(async move |weak, cx| {
            let result = client.list_sessions(query).await;
            let policy = client.policy().await;
            let _ = weak.update(cx, |this, cx| {
                if this.audit.generation != generation {
                    return;
                }
                this.audit.loading = false;
                if let Ok(policy) = policy {
                    this.audit.policy = policy;
                }
                match result {
                    Ok(page) => {
                        this.audit.sessions.items =
                            Arc::new(page.sessions.into_iter().map(Arc::new).collect());
                        this.audit.sessions.next_cursor = page.next_cursor;
                        if reset {
                            this.audit.scroll = UniformListScrollHandle::new();
                        }
                    }
                    Err(error) => this.audit.error = Some(error),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn show_audit_session(&mut self, session_id: String, cx: &mut Context<Self>) {
        self.audit.view = AuditView::Events;
        self.audit.policy_draft = None;
        self.clear_ime_selection();
        self.audit.query = AuditQuery {
            session_id: Some(session_id),
            latest_only: true,
            limit: 100,
            ..Default::default()
        };
        self.audit.search.clear();
        self.audit.time_filter = 0;
        self.refresh_audit(true, cx);
    }

    pub(super) fn render_audit_sessions(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = self.tokens.ui;
        let state = &self.audit.sessions;
        let items = state.items.clone();
        let owner = cx.entity();
        let body = if items.is_empty() {
            oxideterm_gpui_ui::empty_state(
                &self.tokens,
                Self::render_lucide_icon(LucideIcon::Terminal, 20.0, rgb(theme.accent)),
                self.i18n.t(if self.audit.loading {
                    "event_log.audit.loading"
                } else {
                    "event_log.audit.empty"
                }),
                None,
                None,
            )
            .into_any_element()
        } else {
            tauri_virtual_uniform_list("audit-session-list", items.len(), self.audit.scroll.clone(), TauriVirtualListSpec::new(px(88.0), 4), move |range, _, app| owner.update(app, |this, cx| {
                range.filter_map(|index| items.get(index).map(|session| {
                    let session = session.clone();
                    let summary = this.i18n.t("event_log.sessions.counts")
                        .replace("{{commands}}", &session.command_count.to_string())
                        .replace("{{files}}", &session.file_count.to_string())
                        .replace("{{operations}}", &session.operation_count.to_string())
                        .replace("{{transports}}", &session.transport_count.to_string());
                    div().id(("audit-session", index)).w_full().h(px(88.0)).px_3().flex().items_center().gap_3().border_b_1().border_color(rgb(theme.border)).hover(|v| v.bg(rgb(theme.bg_hover))).cursor_pointer()
                        .child(Self::render_lucide_icon(LucideIcon::Terminal, 16.0, rgb(theme.text_muted)))
                        .child(div().flex_1().min_w(px(0.0)).flex().flex_col().gap_1()
                            .child(div().truncate().child(session.target.as_ref().map(|v| v.to_string()).unwrap_or_else(|| this.i18n.t("event_log.audit.unknown_target"))))
                            .child(div().truncate().text_size(px(this.tokens.metrics.ui_text_xs)).text_color(rgb(theme.text_muted)).child(format!("{} · {}", summary, this.i18n.t(&format!("event_log.capture.{}", session.capture.key())))))
                            .child(div().truncate().text_size(px(this.tokens.metrics.ui_text_xs)).text_color(rgb(theme.text_muted)).child(format!("{} · {} {}", audit_time(session.started_at_ms), this.i18n.t("event_log.sessions.last_activity"), audit_time(session.last_event_at_ms)))))
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| { this.show_audit_session(session.session_id.clone(), cx); cx.stop_propagation(); }))
                        .into_any_element()
                })).collect()
            })).into_any_element()
        };
        div()
            .flex_1()
            .min_h(px(0.0))
            .min_w(px(0.0))
            .flex()
            .flex_col()
            .text_size(px(self.tokens.metrics.ui_text_sm))
            .text_color(rgb(theme.text))
            .child(
                div()
                    .flex_none()
                    .min_h(px(40.0))
                    .px_3()
                    .py_1()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(theme.border))
                    .child(self.audit_view_button(AuditView::Events, cx))
                    .child(self.audit_view_button(AuditView::Sessions, cx))
                    .child(self.audit_view_button(AuditView::Recordings, cx))
                    .child(self.render_audit_filter(AuditFilter::Time, cx))
                    .child(div().flex_1())
                    .child(self.render_audit_toggle(cx))
                    .child(self.audit_icon_button(
                        LucideIcon::RefreshCw,
                        "event_log.audit.refresh",
                        self.audit.loading,
                        |this, _, _, cx| this.refresh_audit_sessions(true, cx),
                        cx,
                    )),
            )
            .children(self.render_audit_disabled_hint())
            .when(self.audit.error.is_some(), |v| {
                v.child(
                    div()
                        .px_3()
                        .py_2()
                        .text_color(rgb(theme.error))
                        .child(self.i18n.t("event_log.audit.unavailable")),
                )
            })
            .child(body)
            .child(
                div()
                    .flex_none()
                    .h(px(36.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_t_1()
                    .border_color(rgb(theme.border))
                    .child(
                        self.i18n
                            .t("event_log.audit.page_summary")
                            .replace("{{page}}", &state.cursors.len().to_string())
                            .replace("{{count}}", &state.items.len().to_string()),
                    )
                    .child(div().flex_1())
                    .child(self.audit_icon_button(
                        LucideIcon::ChevronLeft,
                        "event_log.audit.newer",
                        self.audit.loading || state.cursors.len() <= 1,
                        |this, _, _, cx| {
                            this.audit.sessions.cursors.pop();
                            this.refresh_audit_sessions(false, cx);
                        },
                        cx,
                    ))
                    .child(self.audit_icon_button(
                        LucideIcon::ChevronRight,
                        "event_log.audit.older",
                        self.audit.loading || state.next_cursor.is_none(),
                        |this, _, _, cx| {
                            this.audit
                                .sessions
                                .cursors
                                .push(this.audit.sessions.next_cursor.clone());
                            this.refresh_audit_sessions(false, cx);
                        },
                        cx,
                    )),
            )
            .into_any_element()
    }
}
