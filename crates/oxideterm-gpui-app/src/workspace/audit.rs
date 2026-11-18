use super::*;
use oxideterm_audit::AuditCategory;
use oxideterm_gpui_audit::{AuditBindings, AuditFrame, AuditState, AuditView, AuditWindowRequest};

impl WorkspaceApp {
    pub(super) fn mount_audit(
        state: AuditState,
        tokens: ThemeTokens,
        i18n: &I18n,
        cx: &mut Context<Self>,
    ) -> Entity<AuditView> {
        let owner = cx.weak_entity();
        let frame_owner = owner.clone();
        let visibility_owner = owner.clone();
        let playback_owner = owner.clone();
        let bindings = AuditBindings {
            frame: Rc::new(move |cx| {
                let owner = frame_owner.upgrade()?;
                let app = owner.read(cx);
                Some(AuditFrame {
                    tokens: app.tokens,
                    i18n: app.i18n.clone(),
                    has_background: app.background_surface_active("notification_center"),
                })
            }),
            is_showing: Rc::new(move |cx| {
                visibility_owner.upgrade().is_some_and(|owner| {
                    let app = owner.read(cx);
                    app.notification_center.active_view == WorkspaceActivityView::EventLog
                        && app
                            .active_content_tab(cx)
                            .is_some_and(|tab| tab.kind == TabKind::NotificationCenter)
                })
            }),
            dispatch: Rc::new(move |request, cx| {
                let _ = owner.update(cx, |app, cx| app.apply_audit_window_request(request, cx));
            }),
            save_export: Rc::new(|name, cx| {
                let picker = cx.prompt_for_new_path(
                    &std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
                    Some(name),
                );
                cx.background_executor()
                    .spawn(async move { picker.await.ok()?.ok()? })
            }),
            playback: Rc::new(move |window, cx| {
                let mut preferences = playback_owner
                    .update(cx, |app, cx| {
                        app.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx)
                    })
                    .ok()?;
                // Playback uses a bounded offline terminal surface, not a PTY.
                preferences.scrollback_lines = preferences.scrollback_lines.min(2000);
                Some(cx.new(|cx| {
                    TerminalPane::new_recording_playback(80, 24, preferences, window, cx)
                        .expect("audit playback does not create a PTY")
                }))
            }),
        };
        cx.new(|cx| {
            AuditView::new(
                state,
                bindings,
                AuditFrame {
                    tokens,
                    i18n: i18n.clone(),
                    has_background: false,
                },
                cx,
            )
        })
    }

    fn apply_audit_window_request(&mut self, request: AuditWindowRequest, cx: &mut Context<Self>) {
        match request {
            AuditWindowRequest::ClearWorkspaceTextFocus => {
                self.ime_marked_text = None;
                self.clear_ime_selection();
            }
            AuditWindowRequest::PrepareFilter => self.prepare_other_modal_interactions(cx),
            AuditWindowRequest::StorageUnavailable => {
                self.push_notification_entry(
                    WorkspaceNotificationKind::Security,
                    WorkspaceNotificationSeverity::Error,
                    self.i18n.t("event_log.audit.center_title"),
                    Some(self.i18n.t("event_log.audit.unavailable")),
                    WorkspaceNotificationScope::Global,
                    Some("audit-storage-failed".into()),
                );
                cx.notify();
            }
            AuditWindowRequest::Notice {
                title,
                description,
                success,
            } => self.push_workspace_notice(
                TerminalNotice {
                    title,
                    description,
                    status_text: None,
                    progress: None,
                    variant: if success {
                        TerminalNoticeVariant::Success
                    } else {
                        TerminalNoticeVariant::Error
                    },
                },
                cx,
            ),
        }
    }

    pub(in crate::workspace) fn open_notification_audit(
        &mut self,
        entry: &WorkspaceNotificationEntry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let connection_id = match &entry.scope {
            WorkspaceNotificationScope::Connection(id) => Some(id.clone()),
            WorkspaceNotificationScope::Node(id) => {
                self.node_router.connection_id_for_node(&NodeId(id.clone()))
            }
            WorkspaceNotificationScope::Global => None,
        };
        let category = if entry.dedupe_key.as_deref() == Some("audit-storage-failed") {
            Some(AuditCategory::Audit)
        } else if entry.kind == WorkspaceNotificationKind::Security {
            Some(AuditCategory::Security)
        } else {
            None
        };
        self.clear_ime_selection();
        self.audit.update(cx, |audit, cx| {
            audit.show_notification_events(connection_id, category, entry.created_at, cx);
        });
        self.notification_center.active_view = WorkspaceActivityView::EventLog;
        self.open_notification_center_tab(window, cx);
    }
}
