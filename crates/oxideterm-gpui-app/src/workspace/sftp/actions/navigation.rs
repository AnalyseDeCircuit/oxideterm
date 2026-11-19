use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_sftp_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        use SftpKeyAction as Action;

        let view = self.sftp_view().clone();
        let state = view.read(cx);
        let dialog = state.dialog();
        let editor = matches!(dialog, Some(SftpDialog::Editor { .. }));
        let focused_input = state.focused_input();
        let keystroke = &event.keystroke;
        if !editor && keystroke.key == "escape" && self.dismiss_workspace_context_menus(cx) {
            cx.notify();
            return true;
        }
        if !editor
            && focused_input.is_some()
            && self.handle_active_text_input_edit_shortcut(keystroke, cx)
        {
            return true;
        }

        let overrides = &self.settings_store.settings().keybindings.overrides;
        let action = [
            ("sftp.selectAll", Action::SelectAll),
            ("sftp.editPath", Action::EditPath),
            (
                "sftp.revealTerminalDirectory",
                Action::RevealTerminalDirectory,
            ),
            ("sftp.open", Action::Open),
            ("sftp.preview", Action::Preview),
            ("sftp.upload", Action::Upload),
            ("sftp.download", Action::Download),
            ("sftp.delete", Action::Delete),
            ("sftp.rename", Action::Rename),
            ("sftp.togglePreviewSource", Action::TogglePreviewSource),
            ("editor.save", Action::SaveEditor),
        ]
        .into_iter()
        .find(|(id, _)| {
            let routed = if editor {
                *id == "editor.save"
            } else if dialog.is_some() {
                *id == "sftp.togglePreviewSource"
            } else {
                !matches!(*id, "editor.save" | "sftp.togglePreviewSource")
            };
            routed && crate::keybindings::keystroke_matches_action(keystroke, id, overrides)
        })
        .map(|(_, action)| action);
        let tree_mode = self.sftp_tree_mode(cx);
        let result = view.update(cx, |state, cx| {
            state.handle_key(keystroke, action, tree_mode, window, cx)
        });
        match result {
            SftpKeyResult::Ignored => {
                return !editor
                    && focused_input.is_some()
                    && (self.handle_active_text_input_navigation(keystroke, cx)
                        || self.handle_active_text_input_delete_selection(keystroke, cx)
                        || self.handle_active_text_input_transpose(keystroke, cx));
            }
            SftpKeyResult::Handled => {}
            SftpKeyResult::ResetIme => {
                self.ime_marked_text = None;
                self.clear_ime_selection();
            }
            SftpKeyResult::CloseDialog => self.close_sftp_dialog(cx),
            SftpKeyResult::AcceptDialog => self.accept_sftp_dialog(cx),
            SftpKeyResult::CommitPath(pane) => self.commit_sftp_path_input(pane, cx),
            SftpKeyResult::AcceptCompletion(pane, index) => {
                self.accept_sftp_path_completion(pane, index, cx)
            }
            SftpKeyResult::OpenFile(pane, file) => self.open_or_preview_sftp_file(pane, &file, cx),
            SftpKeyResult::Transfer(pane, direction) => {
                self.queue_sftp_transfers(pane, direction, cx)
            }
            SftpKeyResult::RevealTerminalDirectory => self.reveal_sftp_terminal_directory(cx),
        }
        true
    }

    pub(in crate::workspace::sftp) fn set_sftp_path(
        &mut self,
        pane: SftpPane,
        path: String,
        cx: &mut Context<Self>,
    ) {
        let load = self
            .sftp_view()
            .update(cx, |state, cx| state.set_path(pane, path, cx));
        self.dispatch_sftp_path_load(load, cx);
    }

    fn dispatch_sftp_path_load(&mut self, load: SftpPathLoad, cx: &mut Context<Self>) {
        match load {
            SftpPathLoad::Local => (),
            SftpPathLoad::Remote => self.request_sftp_remote_load(cx),
            SftpPathLoad::PairPrimary => self.request_sftp_pair_primary_load(cx),
        }
    }

    pub(in crate::workspace::sftp) fn start_sftp_path_edit(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view().update(cx, |sftp, cx| {
            sftp.start_path_edit(pane, cx);
            cx.notify();
        });
    }

    pub(in crate::workspace) fn refresh_sftp_path_completion(
        &mut self,
        input: SftpInput,
        cx: &mut Context<Self>,
    ) {
        let view = self.sftp_view().clone();
        let Some(SftpCompletionLoad {
            pane,
            generation,
            remote_id,
            parent_path,
        }) = view.update(cx, |state, cx| state.refresh_path_completion(input, cx))
        else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = view.read(cx).worker_sender();
        self.forwarding_runtime.spawn(async move {
            let result = load_remote_sftp_completion_listing(backend, &parent_path)
                .await
                .map(|listing| {
                    listing
                        .files
                        .into_iter()
                        .map(sftp_path_completion_candidate)
                        .collect()
                });
            let _ = tx.send(match pane {
                SftpPane::Local => SftpWorkerResult::PairPrimaryPathCompletion {
                    generation,
                    remote_id,
                    parent_path,
                    result,
                },
                SftpPane::Remote => SftpWorkerResult::RemotePathCompletion {
                    generation,
                    remote_id,
                    parent_path,
                    result,
                },
            });
        });
    }

    pub(in crate::workspace) fn accept_sftp_path_completion(
        &mut self,
        pane: SftpPane,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        if let Some(load) = self
            .sftp_view()
            .update(cx, |state, cx| state.accept_completion(pane, index, cx))
        {
            self.dispatch_sftp_path_load(load, cx);
        }
    }

    pub(in crate::workspace::sftp) fn commit_sftp_path_input(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) {
        let path = self.sftp_view().read(cx).committed_path_input(pane);
        if !path.is_empty() {
            self.set_sftp_path(pane, path, cx);
        }
    }

    pub(in crate::workspace) fn update_sftp_drag_capture(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        // GPUI does not give DOM-style pointer capture for free. The root view
        // keeps the candidate alive after the pointer leaves the file list, but
        // only pane-level move handlers may nominate a drop target.
        self.sftp_view().update(cx, |state, cx| {
            if state.update_drag_capture(position, cx) {
                cx.notify();
            }
        });
    }

    pub(in crate::workspace) fn cancel_sftp_drag_capture(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        // Browser pointer capture always produces a terminal mouse-up. If the
        // user releases outside both panes, cancel the candidate so hover rings
        // and pending drag state cannot remain latched.
        self.sftp_view().update(cx, |state, cx| {
            let changed = state.cancel_drag_capture();
            if changed {
                cx.notify();
            }
            changed
        })
    }

    pub(in crate::workspace::sftp) fn clear_sftp_selection(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sftp_view().update(cx, |state, cx| {
            let changed = state.clear_selection(pane);
            if changed {
                cx.notify();
            }
            changed
        })
    }
}
