use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn push_sftp_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    pub(in crate::workspace::sftp) fn close_sftp_dialog(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if self
            .sftp_view()
            .update(cx, |sftp, cx| sftp.begin_dialog_exit(delay, cx))
        {
            self.ime_marked_text = None;
        }
    }

    pub(in crate::workspace::sftp) fn accept_sftp_dialog(&mut self, cx: &mut Context<Self>) {
        let submission = self
            .sftp_view()
            .update(cx, |state, cx| state.submit_dialog(&self.i18n, cx));
        match submission {
            SftpDialogSubmission::Wait => return,
            SftpDialogSubmission::Close => {}
            SftpDialogSubmission::ResolveConflict => {
                self.resolve_sftp_transfer_conflict(SftpConflictResolution::Rename, cx);
                return;
            }
            SftpDialogSubmission::RenameBookmark { id, name } => {
                let result = self
                    .sftp_bookmarks
                    .update(cx, |bookmarks, cx| bookmarks.rename(&id, name, cx));
                if !self.sftp_bookmark_write_result(result, cx) {
                    return;
                }
            }
            SftpDialogSubmission::LocalNotice(notice) => self.push_workspace_notice(notice, cx),
            SftpDialogSubmission::Remote(mutation) => {
                let remote = match mutation.pane {
                    SftpPane::Local => self.sftp_pair_primary_remote_id(cx),
                    SftpPane::Remote => self.visible_sftp_remote_id(cx),
                };
                if let Some(backend) = remote
                    .as_ref()
                    .and_then(|remote| self.sftp_remote_backend(remote))
                {
                    let sender = self.sftp_view().read(cx).worker_sender();
                    if mutation.clear_selection {
                        self.clear_sftp_selection(mutation.pane, cx);
                    }
                    self.forwarding_runtime.spawn(async move {
                        let session = backend.mutation_session().await;
                        let _ = sender.send(mutation.execute(session).await);
                    });
                }
            }
        }
        self.close_sftp_dialog(cx);
    }
}
