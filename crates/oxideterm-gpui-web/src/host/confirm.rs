use super::*;
use oxideterm_gpui_ui::confirm::{
    ConfirmDialogAction, ConfirmDialogVariant, ConfirmDialogView, ModalFooterKeyAction,
    confirm_dialog_with_focus, modal_footer_key_action,
};

pub(crate) enum CloseTarget {
    Tab(TabId),
    Others(Vec<TabId>),
    Right(Vec<TabId>),
    All(Vec<TabId>),
    Pane(TabId, PaneId, TerminalSessionId),
    Terminals {
        sessions: Vec<TerminalSessionId>,
        title: String,
    },
}

pub(super) enum ConfirmationAction {
    Close(CloseTarget),
    ResetSettings,
    DeleteProfile(oxideterm_gpui_connections::session_manager::SessionManagerSelectionTarget),
}

pub(super) struct Confirmation {
    action: ConfirmationAction,
    title: String,
    detail: String,
    confirm_label: &'static str,
    focused: Option<ConfirmDialogAction>,
}

impl Workspace {
    pub(crate) fn request_close(
        &mut self,
        target: CloseTarget,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ssh = |sid: &TerminalSessionId| {
            self.sessions
                .get(sid)
                .is_some_and(|s| !s.is_ble() && s.node != "local")
        };
        let has_ssh = |tab: TabId| {
            let mut ids = Vec::new();
            if let Some(root) = self
                .layout
                .tab_by_id(tab)
                .and_then(|t| t.root_pane.as_ref())
            {
                root.collect_session_ids(&mut ids);
            }
            ids.iter().any(ssh)
        };
        let confirm = matches!(target, CloseTarget::Terminals { .. })
            || self
                .settings
                .read(cx)
                .settings(cx)
                .terminal
                .confirm_before_closing_ssh
                && match &target {
                    CloseTarget::Terminals { .. } => true,
                    CloseTarget::Pane(_, _, sid) => ssh(sid),
                    CloseTarget::Tab(id) => has_ssh(*id),
                    CloseTarget::Right(ids) | CloseTarget::Others(ids) | CloseTarget::All(ids) => {
                        ids.iter().copied().any(has_ssh)
                    }
                };
        let closing: &[TabId] = match &target {
            CloseTarget::Tab(id) => std::slice::from_ref(id),
            CloseTarget::Right(ids) | CloseTarget::Others(ids) | CloseTarget::All(ids) => ids,
            _ => &[],
        };
        let mut dirty_file_pages=self.projects.dirty_pages(closing,&self.layout,cx);
        if let Some(files)=&self.files {
            dirty_file_pages.extend(files.sftp_pages.iter().filter(|(id,_)|closing.contains(id)||closing.contains(&self.layout.container_tab_id(**id)))
                .filter_map(|(_,page)|page.read(cx).dirty_preview(cx)));
        }
        if confirm || !dirty_file_pages.is_empty() {
            let (title, detail) = self.close_description(&target, &dirty_file_pages);
            self.ask(
                ConfirmationAction::Close(target),
                title,
                detail,
                "common.actions.confirm",
                w,
                cx,
            );
        } else {
            self.close_target(target, w, cx);
        }
    }

    fn close_target(&mut self, target: CloseTarget, w: &mut Window, cx: &mut Context<Self>) {
        let ids = match target {
            CloseTarget::Terminals { sessions, .. } => {
                for sid in sessions {
                    if let Some(tab) = self.session_tab(sid)
                        && let Some(pane) = self
                            .layout
                            .tab_by_id(tab)
                            .and_then(|t| t.root_pane.as_ref()?.pane_id_for_session(sid))
                    {
                        self.remove_terminal_pane(tab, pane, sid, cx);
                    }
                }
                return;
            }
            CloseTarget::Tab(id) => vec![id],
            CloseTarget::All(ids) | CloseTarget::Right(ids) | CloseTarget::Others(ids) => ids,
            CloseTarget::Pane(tab, pane, session) => {
                self.remove_terminal_pane(tab, pane, session, cx);
                return;
            }
        };
        for id in ids {
            if let Some(dirty) = self.knowledge.views.guard_tab_close(
                id,
                w.window_handle(),
                |tab, _| self.layout.container_tab_id(tab),
                cx,
            ) {
                self.activate_page(dirty, cx);
            } else {
                self.close(id, cx);
            }
        }
        self.changed(cx);
    }

    fn ask(
        &mut self,
        action: ConfirmationAction,
        title: String,
        detail: String,
        confirm_label: &'static str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.confirmation = Some(Confirmation {
            action,
            title,
            detail,
            confirm_label,
            focused: None,
        });
        w.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn request_settings_reset(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        let key = "command_palette.cmd_reset_settings";
        self.ask(
            ConfirmationAction::ResetSettings,
            self.i18n.t(key),
            self.i18n.t("command_palette.confirm_reset_settings"),
            key,
            w,
            cx,
        );
    }

    pub(super) fn request_profile_delete(
        &mut self,
        id: &str,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_gpui_connections::session_manager::{
            SessionManagerOpenTarget as Open, SessionManagerSelectionTarget as Target,
        };
        let Some(row) = self.catalog_data.items.iter().find(|r| r.id() == id) else {
            return;
        };
        let (target, message) = match &row.target {
            Open::Connection(id) => (
                Target::Connection(id.clone()),
                "sessionManager.actions.confirm_delete",
            ),
            Open::Ble(id) => (
                Target::Ble(id.clone()),
                "sessionManager.ble_profiles.confirm_delete",
            ),
            Open::StandaloneSftp(id) => (
                Target::StandaloneSftp(id.clone()),
                "sessionManager.standalone_sftp_profiles.confirm_delete",
            ),
            _ => return,
        };
        let detail = self.i18n.t(message).replace("{{name}}", &row.name);
        let key = "sessionManager.actions.delete";
        self.ask(
            ConfirmationAction::DeleteProfile(target),
            self.i18n.t(key),
            detail,
            key,
            w,
            cx,
        );
    }

    fn finish_confirmation(
        &mut self,
        action: ConfirmDialogAction,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(prompt) = self.confirmation.take() else {
            return;
        };
        if action == ConfirmDialogAction::Confirm {
            match prompt.action {
                ConfirmationAction::Close(target) => self.close_target(target, w, cx),
                ConfirmationAction::ResetSettings => self.settings.update(cx, |s, cx| {
                    s.edit_preferences(cx, |prefs| *prefs = Default::default());
                }),
                ConfirmationAction::DeleteProfile(target) => self.catalog_event(
                    &oxideterm_gpui_connections::views::PortableCatalogEvent::Delete(vec![target]),
                    w,
                    cx,
                ),
            }
        }
        if let Some(id) = self.active_session().and_then(|id| self.sessions.get(&id)) {
            w.focus(&id.view.read(cx).focus_handle(cx), cx);
        } else {
            w.focus(&self.focus, cx);
        }
        cx.notify();
    }

    pub(super) fn confirmation_key(
        &mut self,
        key: &Keystroke,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use ConfirmDialogAction::{Cancel, Confirm};
        let Some(prompt) = &mut self.confirmation else {
            return false;
        };
        match modal_footer_key_action(
            &key.key,
            key.modifiers.shift,
            &[Cancel, Confirm],
            prompt.focused,
            Cancel,
        ) {
            Some(ModalFooterKeyAction::Focus(next)) => {
                prompt.focused = Some(next);
                cx.notify();
            }
            Some(ModalFooterKeyAction::Cancel) => self.finish_confirmation(Cancel, w, cx),
            Some(ModalFooterKeyAction::Activate(action)) => self.finish_confirmation(action, w, cx),
            None => {}
        }
        true
    }

    fn close_description(
        &self,
        target: &CloseTarget,
        dirty_file_pages: &[String],
    ) -> (String, String) {
        let (mut title, mut detail) = match target {
            CloseTarget::Terminals { sessions, .. } => (
                "tabbar.close_terminals",
                self.i18n
                    .t("tabbar.close_terminals_desc")
                    .replace("{{count}}", &sessions.len().to_string()),
            ),
            CloseTarget::Pane(..) | CloseTarget::Tab(_) => (
                "tabbar.confirm_close_terminal_title",
                self.i18n.t("tabbar.confirm_close_terminal_desc"),
            ),
            CloseTarget::Right(ids) | CloseTarget::Others(ids) | CloseTarget::All(ids) => {
                let title = if matches!(target, CloseTarget::Right(_)) {
                    "tabbar.confirm_close_tabs_to_right_title"
                } else if matches!(target, CloseTarget::Others(_)) {
                    "tabbar.confirm_close_other_title"
                } else {
                    "tabbar.confirm_close_all_title"
                };
                (
                    title,
                    self.i18n
                        .t("tabbar.web_close_tabs")
                        .replace("{{count}}", &ids.len().to_string()),
                )
            }
        };
        if !dirty_file_pages.is_empty() {
            let warning = format!(
                "{}: {}",
                dirty_file_pages.join(", "),
                self.i18n.t("sftp.preview.unsaved_changes_confirm")
            );
            if matches!(target, CloseTarget::Tab(_)) {
                title = "ide.unsaved_changes";
                detail = warning;
            } else {
                detail.push_str("\n\n");
                detail.push_str(&warning);
            }
        }
        let title = match target {
            CloseTarget::Terminals { title, .. } => title.clone(),
            _ => self.i18n.t(title),
        };
        (title, detail)
    }

    pub(super) fn render_confirmation(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let prompt = self.confirmation.as_ref()?;
        Some(confirm_dialog_with_focus(
            &self.tokens,
            ConfirmDialogView {
                variant: ConfirmDialogVariant::Danger,
                title: prompt.title.clone().into_any_element(),
                description: Some(prompt.detail.clone().into_any_element()),
                cancel_label: self.i18n.t("common.cancel").into_any_element(),
                confirm_label: self.i18n.t(prompt.confirm_label).into_any_element(),
            },
            prompt.focused,
            cx.listener(|h, _, w, cx| h.finish_confirmation(ConfirmDialogAction::Cancel, w, cx)),
            cx.listener(|h, _, w, cx| h.finish_confirmation(ConfirmDialogAction::Confirm, w, cx)),
        ))
    }
}
