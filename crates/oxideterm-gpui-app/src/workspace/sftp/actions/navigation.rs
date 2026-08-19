use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn handle_sftp_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        if matches!(
            self.sftp_view.read(cx).dialog,
            Some(SftpDialog::Editor { .. })
        ) {
            if event.keystroke.modifiers.platform && key == "s" {
                self.save_sftp_preview_editor(cx);
                cx.notify();
                return true;
            }
            if key == "escape" {
                self.request_close_sftp_editor(cx);
                cx.notify();
                return true;
            }
            return false;
        }
        if key == "escape" && self.dismiss_workspace_context_menus(cx) {
            cx.notify();
            return true;
        }
        let (dialog, focused_input) = {
            let sftp = self.sftp_view.read(cx);
            (sftp.dialog.clone(), sftp.focused_input)
        };
        if dialog.is_some() && focused_input.is_none() {
            match key {
                "escape" => {
                    if let Some(SftpDialog::EditorCloseConfirm { name }) = dialog {
                        self.cancel_sftp_editor_close_confirm(name, window, cx);
                    } else {
                        self.close_sftp_dialog(cx);
                    }
                    cx.notify();
                    return true;
                }
                "u" => {
                    if matches!(dialog, Some(SftpDialog::Preview { .. }))
                        && self.sftp_preview_is_markdown_content(cx)
                    {
                        self.sftp_view.update(cx, |sftp, cx| {
                            sftp.preview_markdown_source_mode = !sftp.preview_markdown_source_mode;
                            cx.notify();
                        });
                        cx.notify();
                        return true;
                    }
                }
                "enter" => {
                    if matches!(dialog, Some(SftpDialog::EditorCloseConfirm { .. })) {
                        self.discard_sftp_editor_changes(cx);
                    } else {
                        self.accept_sftp_dialog(cx);
                    }
                    cx.notify();
                    return true;
                }
                _ => {}
            }
            return false;
        }
        if let Some(input) = focused_input {
            // Focused inline inputs must keep browser-style editing shortcuts;
            // pane-level shortcuts are only considered after text input declines them.
            if self.handle_active_text_input_edit_shortcut(&event.keystroke, cx) {
                return true;
            }
            if matches!(input, SftpInput::LocalPath | SftpInput::RemotePath)
                && self.handle_sftp_path_completion_key(input, event, cx)
            {
                cx.notify();
                return true;
            }
            match key {
                "tab"
                    if !event.keystroke.modifiers.platform
                        && !event.keystroke.modifiers.control =>
                {
                    self.handle_sftp_input_tab(input, cx);
                    cx.notify();
                    return true;
                }
                "escape" => {
                    match input {
                        SftpInput::LocalPath => self.cancel_sftp_path_edit(SftpPane::Local, cx),
                        SftpInput::RemotePath => self.cancel_sftp_path_edit(SftpPane::Remote, cx),
                        _ => {
                            self.sftp_view.update(cx, |sftp, cx| {
                                sftp.focused_input = None;
                                cx.notify();
                            });
                            self.ime_marked_text = None;
                            self.clear_ime_selection();
                        }
                    }
                    cx.notify();
                    return true;
                }
                "enter" => {
                    match input {
                        SftpInput::LocalPath | SftpInput::RemotePath => {
                            let pane = if input == SftpInput::LocalPath {
                                SftpPane::Local
                            } else {
                                SftpPane::Remote
                            };
                            self.commit_sftp_path_input(pane, cx);
                        }
                        SftpInput::DialogValue => self.accept_sftp_dialog(cx),
                        _ => {}
                    }
                    cx.notify();
                    return true;
                }
                _ => {}
            }
            if self.handle_active_text_input_navigation(&event.keystroke, cx) {
                return true;
            }
            if self.handle_active_text_input_delete_selection(&event.keystroke, cx) {
                return true;
            }
            if self.handle_active_text_input_transpose(&event.keystroke, cx) {
                return true;
            }
        }
        let active_pane = self.sftp_view.read(cx).active_pane;
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            match key {
                "a" => {
                    self.select_all_sftp_files(active_pane, cx);
                    self.sftp_view
                        .update(cx, |sftp, cx| sftp.dismiss_context_menu(cx));
                    cx.notify();
                    return true;
                }
                "l" => {
                    self.start_sftp_path_edit(active_pane, cx);
                    self.sftp_view
                        .update(cx, |sftp, cx| sftp.dismiss_context_menu(cx));
                    cx.notify();
                    return true;
                }
                _ => return false,
            }
        }
        match key {
            "escape" => {
                self.sftp_view
                    .update(cx, |sftp, cx| sftp.dismiss_context_menu(cx));
                self.sftp_view.update(cx, |sftp, cx| {
                    sftp.focused_input = None;
                    cx.notify();
                });
                cx.notify();
                true
            }
            "enter" => {
                if let Some(file) = self.single_selected_sftp_file(active_pane, cx) {
                    // Tauri SFTP only opens directories on Enter; file quick-look is
                    // intentionally bound to Space and double-click.
                    if file.file_type == SftpFileType::Directory {
                        self.open_or_preview_sftp_file(active_pane, &file, cx);
                        cx.notify();
                        return true;
                    }
                    false
                } else {
                    false
                }
            }
            "space" | " " => {
                if active_pane == SftpPane::Remote
                    && let Some(file) = self.single_selected_sftp_file(active_pane, cx)
                    && file.file_type != SftpFileType::Directory
                {
                    self.open_or_preview_sftp_file(active_pane, &file, cx);
                    cx.notify();
                    return true;
                }
                false
            }
            "right" | "arrowright" => {
                if active_pane == SftpPane::Local
                    && !self.sftp_view.read(cx).local_selected.is_empty()
                {
                    self.queue_sftp_transfers(SftpPane::Local, SftpTransferDirection::Upload, cx);
                    cx.notify();
                    return true;
                }
                false
            }
            "left" | "arrowleft" => {
                if active_pane == SftpPane::Remote
                    && !self.sftp_view.read(cx).remote_selected.is_empty()
                {
                    self.queue_sftp_transfers(
                        SftpPane::Remote,
                        SftpTransferDirection::Download,
                        cx,
                    );
                    cx.notify();
                    return true;
                }
                false
            }
            "delete" | "backspace" => {
                let files = self.sftp_selected_names(active_pane, cx);
                if !files.is_empty() {
                    self.sftp_view.update(cx, |sftp, cx| {
                        sftp.set_dialog(SftpDialog::Delete {
                            pane: active_pane,
                            files,
                        });
                        cx.notify();
                    });
                    cx.notify();
                    return true;
                }
                false
            }
            "f2" | "F2" => {
                if let Some(file) = self.single_selected_sftp_file(active_pane, cx) {
                    self.sftp_view.update(cx, |sftp, cx| {
                        sftp.open_rename_dialog(active_pane, file.name, cx);
                    });
                    cx.notify();
                    return true;
                }
                false
            }
            "up" | "arrowup" => {
                if self.move_sftp_selection(active_pane, -1, cx) {
                    cx.notify();
                }
                true
            }
            "down" | "arrowdown" => {
                if self.move_sftp_selection(active_pane, 1, cx) {
                    cx.notify();
                }
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace::sftp) fn set_sftp_path(
        &mut self,
        pane: SftpPane,
        path: String,
        cx: &mut Context<Self>,
    ) {
        match pane {
            SftpPane::Local => {
                if self.sftp_pair_primary_remote_id(cx).is_some() {
                    self.sftp_view.update(cx, |sftp, cx| {
                        sftp.apply_pair_primary_path(path);
                        cx.notify();
                    });
                    self.request_sftp_pair_primary_load(cx);
                    return;
                }
                self.sftp_view.update(cx, |sftp, cx| {
                    if let Some(remote_id) = sftp.current_remote_id.clone() {
                        sftp.local_path_by_remote.insert(remote_id, path.clone());
                    }
                    sftp.apply_local_path(path);
                    cx.notify();
                });
            }
            SftpPane::Remote => {
                self.sftp_view.update(cx, |sftp, cx| {
                    sftp.apply_remote_path(path);
                    cx.notify();
                });
                self.request_sftp_remote_load(cx);
            }
        }
    }

    fn cancel_sftp_path_edit(&mut self, pane: SftpPane, cx: &mut Context<Self>) {
        // Tauri's editable SFTP path input cancels on DOM blur unless the Go
        // button takes focus. Native does not model that button focus target
        // yet, so Tab/Escape restore the current committed path explicitly.
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.cancel_path_edit(pane);
            cx.notify();
        });
        self.ime_marked_text = None;
        self.clear_ime_selection();
    }

    fn handle_sftp_input_tab(&mut self, input: SftpInput, cx: &mut Context<Self>) {
        // Browser Tab moves focus out of the current input. Until the native
        // toolbar buttons have first-class focus targets, mirror the observable
        // blur side-effect so path edits do not get stuck in captured input mode.
        match input {
            SftpInput::LocalPath => self.cancel_sftp_path_edit(SftpPane::Local, cx),
            SftpInput::RemotePath => self.cancel_sftp_path_edit(SftpPane::Remote, cx),
            SftpInput::LocalFilter | SftpInput::RemoteFilter | SftpInput::DialogValue => {
                self.sftp_view.update(cx, |sftp, cx| {
                    sftp.focused_input = None;
                    cx.notify();
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
            }
        }
    }

    pub(in crate::workspace::sftp) fn start_sftp_path_edit(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.start_path_edit(pane);
            cx.notify();
        });
    }

    /// Refreshes local or remote path suggestions without creating an independent SSH owner.
    pub(in crate::workspace) fn refresh_sftp_path_completion(
        &mut self,
        input: SftpInput,
        cx: &mut Context<Self>,
    ) {
        match input {
            SftpInput::LocalPath if self.sftp_pair_primary_remote_id(cx).is_some() => {
                self.refresh_sftp_pair_primary_path_completion(cx)
            }
            SftpInput::LocalPath => self.refresh_sftp_local_path_completion(cx),
            SftpInput::RemotePath => self.refresh_sftp_remote_path_completion(cx),
            SftpInput::LocalFilter | SftpInput::RemoteFilter | SftpInput::DialogValue => {}
        }
    }

    fn refresh_sftp_local_path_completion(&mut self, cx: &mut Context<Self>) {
        let path_input = self.sftp_view.read(cx).local_path_input.clone();
        let Some(request) = local_path_completion_request(&path_input) else {
            self.sftp_view
                .update(cx, |sftp, _cx| sftp.local_path_completion.dismiss());
            return;
        };
        let request = self
            .sftp_view
            .update(cx, |sftp, _cx| sftp.local_path_completion.request(request));
        let Some((generation, parent_path)) = request else {
            return;
        };
        let entries = list_local_files(&parent_path)
            .unwrap_or_default()
            .into_iter()
            .map(sftp_path_completion_candidate)
            .collect();
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.local_path_completion
                .apply_entries(generation, &parent_path, entries);
            cx.notify();
        });
    }

    fn refresh_sftp_remote_path_completion(&mut self, cx: &mut Context<Self>) {
        let path_input = self.sftp_view.read(cx).remote_path_input.clone();
        let Some(request) = remote_path_completion_request(&path_input) else {
            self.sftp_view
                .update(cx, |sftp, _cx| sftp.remote_path_completion.dismiss());
            return;
        };
        let request = self
            .sftp_view
            .update(cx, |sftp, _cx| sftp.remote_path_completion.request(request));
        let Some((generation, parent_path)) = request else {
            return;
        };

        let current_entries = {
            let sftp = self.sftp_view.read(cx);
            (parent_path == sftp.remote_path && !sftp.remote_loading).then(|| {
                sftp.remote_files
                    .iter()
                    .cloned()
                    .map(sftp_path_completion_candidate)
                    .collect::<Vec<_>>()
            })
        };
        if let Some(entries) = current_entries {
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.remote_path_completion
                    .apply_entries(generation, &parent_path, entries);
                cx.notify();
            });
            return;
        }

        let Some(remote_id) = self.sftp_view.read(cx).current_remote_id.clone() else {
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.remote_path_completion
                    .apply_entries(generation, &parent_path, Vec::new());
                cx.notify();
            });
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = self.sftp_view.read(cx).worker_sender();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            // Completion borrows a short-lived channel from the selected remote owner.
            let result = load_remote_sftp_completion_listing(backend, &parent_path)
                .await
                .map(|listing| {
                    listing
                        .files
                        .into_iter()
                        .map(sftp_path_completion_candidate)
                        .collect()
                });
            let _ = tx.send(SftpWorkerResult::RemotePathCompletion {
                generation,
                remote_id,
                parent_path,
                result,
            });
        });
    }

    fn refresh_sftp_pair_primary_path_completion(&mut self, cx: &mut Context<Self>) {
        let path_input = self.sftp_view.read(cx).local_path_input.clone();
        let Some(request) = remote_path_completion_request(&path_input) else {
            self.sftp_view
                .update(cx, |sftp, _cx| sftp.local_path_completion.dismiss());
            return;
        };
        let request = self
            .sftp_view
            .update(cx, |sftp, _cx| sftp.local_path_completion.request(request));
        let Some((generation, parent_path)) = request else {
            return;
        };
        let Some(remote_id) = self.sftp_pair_primary_remote_id(cx) else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = self.sftp_view.read(cx).worker_sender();
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
            let _ = tx.send(SftpWorkerResult::PairPrimaryPathCompletion {
                generation,
                remote_id,
                parent_path,
                result,
            });
        });
    }

    pub(in crate::workspace) fn accept_sftp_path_completion(
        &mut self,
        pane: SftpPane,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some((candidate, parent_path)) = ({
            let sftp = self.sftp_view.read(cx);
            let state = match pane {
                SftpPane::Local => &sftp.local_path_completion,
                SftpPane::Remote => &sftp.remote_path_completion,
            };
            state.candidate(index).cloned().map(|candidate| {
                let parent_path =
                    state
                        .parent_path()
                        .map(str::to_string)
                        .unwrap_or_else(|| match pane {
                            SftpPane::Local => sftp.local_path.clone(),
                            SftpPane::Remote => sftp.remote_path.clone(),
                        });
                (candidate, parent_path)
            })
        }) else {
            return;
        };

        if candidate.is_directory {
            self.set_sftp_path(pane, candidate.path, cx);
            return;
        }
        self.set_sftp_path(pane, parent_path.clone(), cx);
        match pane {
            SftpPane::Local => {
                self.sftp_view.update(cx, |sftp, cx| {
                    if sftp
                        .local_files
                        .iter()
                        .any(|entry| entry.name == candidate.name)
                    {
                        sftp.local_selected.insert(candidate.name.clone());
                        sftp.local_last_selected = Some(candidate.name);
                        cx.notify();
                    }
                });
            }
            SftpPane::Remote => {
                // The parent listing arrives asynchronously; apply selection with that result.
                self.sftp_view.update(cx, |sftp, _cx| {
                    sftp.remote_path_completion_pending_selection =
                        Some((parent_path, candidate.name));
                });
            }
        }
    }

    fn handle_sftp_path_completion_key(
        &mut self,
        input: SftpInput,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return false;
        }
        let pane = if input == SftpInput::LocalPath {
            SftpPane::Local
        } else {
            SftpPane::Remote
        };
        let is_visible = {
            let sftp = self.sftp_view.read(cx);
            match pane {
                SftpPane::Local => sftp.local_path_completion.is_visible(),
                SftpPane::Remote => sftp.remote_path_completion.is_visible(),
            }
        };
        if !is_visible {
            return false;
        }
        match event.keystroke.key.as_str() {
            "up" | "arrowup" => self.sftp_view.update(cx, |sftp, cx| {
                let changed = match pane {
                    SftpPane::Local => sftp.local_path_completion.move_selection(-1),
                    SftpPane::Remote => sftp.remote_path_completion.move_selection(-1),
                };
                if changed {
                    cx.notify();
                }
                changed
            }),
            "down" | "arrowdown" => self.sftp_view.update(cx, |sftp, cx| {
                let changed = match pane {
                    SftpPane::Local => sftp.local_path_completion.move_selection(1),
                    SftpPane::Remote => sftp.remote_path_completion.move_selection(1),
                };
                if changed {
                    cx.notify();
                }
                changed
            }),
            "enter" | "tab" => {
                let index = {
                    let sftp = self.sftp_view.read(cx);
                    match pane {
                        SftpPane::Local => sftp.local_path_completion.selected_index(),
                        SftpPane::Remote => sftp.remote_path_completion.selected_index(),
                    }
                };
                self.accept_sftp_path_completion(pane, index, cx);
                true
            }
            "escape" => {
                self.sftp_view.update(cx, |sftp, cx| {
                    match pane {
                        SftpPane::Local => sftp.local_path_completion.dismiss(),
                        SftpPane::Remote => sftp.remote_path_completion.dismiss(),
                    }
                    cx.notify();
                });
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace::sftp) fn handle_sftp_breadcrumb_scroll(
        &mut self,
        pane: SftpPane,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) {
        let scroll_handle = match pane {
            SftpPane::Local => self.sftp_view.read(cx).local_path_scroll.clone(),
            SftpPane::Remote => self.sftp_view.read(cx).remote_path_scroll.clone(),
        };
        if let Some(changed) =
            scroll_breadcrumb_by_wheel(&scroll_handle, event, px(SFTP_PANE_HEADER_HEIGHT))
        {
            cx.stop_propagation();
            if changed {
                cx.notify();
            }
        }
    }

    pub(in crate::workspace::sftp) fn commit_sftp_path_input(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) {
        let path = {
            let sftp = self.sftp_view.read(cx);
            match pane {
                SftpPane::Local if sftp.pair_primary_remote_id.is_some() => {
                    normalize_remote_path(&sftp.local_path_input)
                }
                SftpPane::Local => sftp.local_path_input.trim().to_string(),
                SftpPane::Remote => normalize_remote_path(&sftp.remote_path_input),
            }
        };
        if !path.is_empty() {
            self.set_sftp_path(pane, path, cx);
        }
    }

    pub(in crate::workspace::sftp) fn navigate_sftp_path(
        &mut self,
        pane: SftpPane,
        target: &str,
        cx: &mut Context<Self>,
    ) {
        let next = match (pane, target) {
            (SftpPane::Local, "~") if self.sftp_pair_primary_remote_id(cx).is_some() => {
                let sftp = self.sftp_view.read(cx);
                sftp.pair_primary_remote_id
                    .as_ref()
                    .and_then(|remote_id| sftp.remote_home_by_remote.get(remote_id))
                    .cloned()
                    .unwrap_or_else(|| "/".to_string())
            }
            (SftpPane::Local, "~") => home_path(),
            (SftpPane::Remote, "~") => {
                let sftp = self.sftp_view.read(cx);
                sftp.current_remote_id
                    .as_ref()
                    .and_then(|remote_id| sftp.remote_home_by_remote.get(remote_id))
                    .cloned()
                    .unwrap_or_else(|| "/".to_string())
            }
            (SftpPane::Local, "..") => parent_path(
                &self.sftp_view.read(cx).local_path,
                self.sftp_pair_primary_remote_id(cx).is_some(),
            ),
            (SftpPane::Remote, "..") => parent_path(&self.sftp_view.read(cx).remote_path, true),
            _ => target.to_string(),
        };
        self.set_sftp_path(pane, next, cx);
    }

    pub(in crate::workspace::sftp) fn toggle_sftp_sort(
        &mut self,
        pane: SftpPane,
        field: SftpSortField,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.toggle_sort(pane, field);
            cx.notify();
        });
    }

    pub(in crate::workspace::sftp) fn update_sftp_drag(
        &mut self,
        pane: SftpPane,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sftp_view
            .update(cx, |sftp, _cx| sftp.update_drag(pane, x, y))
    }

    pub(in crate::workspace) fn update_sftp_drag_capture(
        &mut self,
        position: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        // GPUI does not give DOM-style pointer capture for free. The root view
        // keeps the candidate alive after the pointer leaves the file list, but
        // only pane-level move handlers may nominate a drop target.
        if self
            .sftp_view
            .update(cx, |sftp, cx| sftp.update_drag_capture(position, cx))
        {
            cx.notify();
        }
    }

    pub(in crate::workspace::sftp) fn finish_sftp_drag(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) -> bool {
        let (drag, had_target) = self.sftp_view.update(cx, |sftp, _cx| {
            let drag = sftp.drag_state.take();
            let had_target = sftp.drag_over_pane.take().is_some();
            sftp.stop_drag_autoscroll();
            (drag, had_target)
        });
        let Some(drag) = drag else {
            return had_target;
        };
        if !drag.active || drag.source_pane == pane {
            return had_target || drag.active;
        }
        match (drag.source_pane, pane) {
            (SftpPane::Local, SftpPane::Remote) => {
                self.queue_sftp_named_transfers(
                    SftpPane::Local,
                    SftpTransferDirection::Upload,
                    drag.names,
                    cx,
                );
            }
            (SftpPane::Remote, SftpPane::Local) => {
                self.queue_sftp_named_transfers(
                    SftpPane::Remote,
                    SftpTransferDirection::Download,
                    drag.names,
                    cx,
                );
            }
            _ => {}
        }
        true
    }

    pub(in crate::workspace) fn cancel_sftp_drag_capture(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        // Browser pointer capture always produces a terminal mouse-up. If the
        // user releases outside both panes, cancel the candidate so hover rings
        // and pending drag state cannot remain latched.
        self.sftp_view
            .update(cx, |sftp, _cx| sftp.cancel_drag_capture())
    }

    pub(in crate::workspace::sftp) fn clear_sftp_selection(
        &mut self,
        pane: SftpPane,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sftp_view
            .update(cx, |sftp, _cx| sftp.clear_selection(pane))
    }

    fn select_all_sftp_files(&mut self, pane: SftpPane, cx: &mut Context<Self>) {
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.select_all_files(pane);
            cx.notify();
        });
    }

    fn move_sftp_selection(
        &mut self,
        pane: SftpPane,
        delta: isize,
        cx: &mut Context<Self>,
    ) -> bool {
        self.sftp_view
            .update(cx, |sftp, _cx| sftp.move_selection(pane, delta))
    }

    pub(in crate::workspace::sftp) fn sftp_selected_names(
        &self,
        pane: SftpPane,
        cx: &App,
    ) -> Vec<String> {
        self.sftp_view.read(cx).selected_names(pane)
    }

    fn single_selected_sftp_file(&self, pane: SftpPane, cx: &App) -> Option<SftpFileEntry> {
        self.sftp_view.read(cx).single_selected_file(pane)
    }
}

impl SftpWorkspaceEntity {
    fn cancel_path_edit(&mut self, pane: SftpPane) {
        // Cancelling a path edit restores the committed path and releases only
        // the SFTP-owned input focus; workspace IME coordination stays at root.
        match pane {
            SftpPane::Local => {
                self.local_path_completion.dismiss();
                self.local_path_input.clone_from(&self.local_path);
                self.editing_local_path = false;
                if self.focused_input == Some(SftpInput::LocalPath) {
                    self.focused_input = None;
                }
            }
            SftpPane::Remote => {
                self.remote_path_completion.dismiss();
                self.remote_path_input.clone_from(&self.remote_path);
                self.editing_remote_path = false;
                if self.focused_input == Some(SftpInput::RemotePath) {
                    self.focused_input = None;
                }
            }
        }
    }

    fn start_path_edit(&mut self, pane: SftpPane) {
        self.active_pane = pane;
        match pane {
            SftpPane::Local => {
                self.local_path_completion.dismiss();
                self.editing_local_path = true;
                self.local_path_input.clone_from(&self.local_path);
                self.focused_input = Some(SftpInput::LocalPath);
            }
            SftpPane::Remote => {
                self.remote_path_completion.dismiss();
                self.editing_remote_path = true;
                self.remote_path_input.clone_from(&self.remote_path);
                self.focused_input = Some(SftpInput::RemotePath);
            }
        }
    }

    pub(in crate::workspace::sftp) fn apply_local_path(&mut self, path: String) {
        self.local_path_completion.dismiss();
        // Preserve the horizontal breadcrumb position when navigating through a long path.
        self.local_path = path.clone();
        self.local_path_input.clone_from(&path);
        self.editing_local_path = false;
        self.local_files = refreshed_local_files(&path);
        self.local_selected.clear();
        self.local_last_selected = None;
        self.focused_input = None;
        self.clear_context_menu_immediately();
    }

    pub(in crate::workspace::sftp) fn apply_pair_primary_path(&mut self, path: String) {
        self.local_path_completion.dismiss();
        self.local_path = normalize_remote_path(&path);
        self.local_path_input.clone_from(&self.local_path);
        self.editing_local_path = false;
        self.local_selected.clear();
        self.local_last_selected = None;
        self.focused_input = None;
        self.clear_context_menu_immediately();
        self.pair_primary_loading = true;
    }

    fn apply_remote_path(&mut self, path: String) {
        self.remote_path_completion.dismiss();
        self.remote_path_completion_pending_selection = None;
        // Preserve the horizontal breadcrumb position when navigating through a long path.
        self.remote_path = path.clone();
        self.remote_path_input = path;
        self.editing_remote_path = false;
        self.remote_selected.clear();
        self.remote_last_selected = None;
        self.focused_input = None;
        self.clear_context_menu_immediately();
    }

    fn toggle_sort(&mut self, pane: SftpPane, field: SftpSortField) {
        let (sort_field, sort_direction) = match pane {
            SftpPane::Local => (&mut self.local_sort_field, &mut self.local_sort_direction),
            SftpPane::Remote => (&mut self.remote_sort_field, &mut self.remote_sort_direction),
        };
        if *sort_field == field {
            *sort_direction = match *sort_direction {
                SftpSortDirection::Asc => SftpSortDirection::Desc,
                SftpSortDirection::Desc => SftpSortDirection::Asc,
            };
        } else {
            *sort_field = field;
            *sort_direction = SftpSortDirection::Asc;
        }
    }

    fn ordered_file_names(&self, pane: SftpPane) -> Vec<String> {
        let (files, filter, field, direction) = match pane {
            SftpPane::Local => (
                &self.local_files,
                &self.local_filter,
                self.local_sort_field,
                self.local_sort_direction,
            ),
            SftpPane::Remote => (
                &self.remote_files,
                &self.remote_filter,
                self.remote_sort_field,
                self.remote_sort_direction,
            ),
        };
        sorted_sftp_files(files, filter, field, direction)
            .into_iter()
            .map(|file| file.name)
            .collect()
    }

    fn selected_names(&self, pane: SftpPane) -> Vec<String> {
        let selected = match pane {
            SftpPane::Local => &self.local_selected,
            SftpPane::Remote => &self.remote_selected,
        };
        self.ordered_file_names(pane)
            .into_iter()
            .filter(|name| selected.contains(name))
            .collect()
    }

    fn single_selected_file(&self, pane: SftpPane) -> Option<SftpFileEntry> {
        let selected = self.selected_names(pane);
        if selected.len() != 1 {
            return None;
        }
        let name = selected.first()?;
        let files = match pane {
            SftpPane::Local => &self.local_files,
            SftpPane::Remote => &self.remote_files,
        };
        files.iter().find(|file| &file.name == name).cloned()
    }

    fn clear_selection(&mut self, pane: SftpPane) -> bool {
        match pane {
            SftpPane::Local => {
                let changed = !self.local_selected.is_empty() || self.local_last_selected.is_some();
                self.local_selected.clear();
                self.local_last_selected = None;
                changed
            }
            SftpPane::Remote => {
                let changed =
                    !self.remote_selected.is_empty() || self.remote_last_selected.is_some();
                self.remote_selected.clear();
                self.remote_last_selected = None;
                changed
            }
        }
    }

    fn select_all_files(&mut self, pane: SftpPane) {
        let names = self.ordered_file_names(pane);
        match pane {
            SftpPane::Local => {
                self.local_selected = names.iter().cloned().collect();
                self.local_last_selected = names.last().cloned();
            }
            SftpPane::Remote => {
                self.remote_selected = names.iter().cloned().collect();
                self.remote_last_selected = names.last().cloned();
            }
        }
    }

    pub(in crate::workspace::sftp) fn select_file(
        &mut self,
        pane: SftpPane,
        name: String,
        modifiers: gpui::Modifiers,
    ) {
        self.active_pane = pane;
        self.clear_context_menu_immediately();
        let range_names = self.ordered_file_names(pane);
        let (selected, last_selected) = match pane {
            SftpPane::Local => (&mut self.local_selected, &mut self.local_last_selected),
            SftpPane::Remote => (&mut self.remote_selected, &mut self.remote_last_selected),
        };
        if modifiers.shift
            && let Some(last) = last_selected.as_ref()
            && let (Some(start), Some(end)) = (
                range_names.iter().position(|item| item == last),
                range_names.iter().position(|item| item == &name),
            )
        {
            selected.clear();
            let (min, max) = (start.min(end), start.max(end));
            selected.extend(range_names[min..=max].iter().cloned());
            *last_selected = Some(name);
            return;
        }
        if modifiers.platform || modifiers.control {
            if !selected.insert(name.clone()) {
                selected.remove(&name);
            }
        } else {
            selected.clear();
            selected.insert(name.clone());
        }
        *last_selected = Some(name);
    }

    fn move_selection(&mut self, pane: SftpPane, delta: isize) -> bool {
        let names = self.ordered_file_names(pane);
        if names.is_empty() {
            return false;
        }
        let current = self
            .selected_names(pane)
            .first()
            .and_then(|name| names.iter().position(|candidate| candidate == name))
            .unwrap_or(if delta > 0 { names.len() - 1 } else { 0 });
        let next = if delta > 0 {
            (current + 1) % names.len()
        } else if current == 0 {
            names.len() - 1
        } else {
            current - 1
        };
        let name = names[next].clone();
        let selected_names = self.selected_names(pane);
        let last_selected = match pane {
            SftpPane::Local => self.local_last_selected.as_ref(),
            SftpPane::Remote => self.remote_last_selected.as_ref(),
        };
        if selected_names.len() == 1
            && selected_names.first() == Some(&name)
            && last_selected == Some(&name)
        {
            return false;
        }
        match pane {
            SftpPane::Local => {
                self.local_selected.clear();
                self.local_selected.insert(name.clone());
                self.local_last_selected = Some(name);
                scroll_tauri_virtual_list_to_index(
                    &self.local_file_scroll,
                    next,
                    sftp_file_list_virtual_spec(),
                    TauriVirtualScrollAlign::Nearest,
                );
            }
            SftpPane::Remote => {
                self.remote_selected.clear();
                self.remote_selected.insert(name.clone());
                self.remote_last_selected = Some(name);
                scroll_tauri_virtual_list_to_index(
                    &self.remote_file_scroll,
                    next,
                    sftp_file_list_virtual_spec(),
                    TauriVirtualScrollAlign::Nearest,
                );
            }
        }
        true
    }

    pub(in crate::workspace::sftp) fn start_drag_candidate(
        &mut self,
        pane: SftpPane,
        x: f32,
        y: f32,
    ) {
        let names = self.selected_names(pane);
        if names.is_empty() {
            self.drag_state = None;
            self.stop_drag_autoscroll();
            return;
        }
        self.drag_state = Some(SftpDragState {
            source_pane: pane,
            names,
            start_x: x,
            start_y: y,
            active: false,
        });
        self.drag_over_pane = None;
        self.stop_drag_autoscroll();
    }

    fn update_drag_activation(&mut self, x: f32, y: f32) -> bool {
        let Some(drag) = self.drag_state.as_mut() else {
            return false;
        };
        let dx = x - drag.start_x;
        let dy = y - drag.start_y;
        if !drag.active && (dx * dx + dy * dy).sqrt() >= 5.0 {
            drag.active = true;
        }
        drag.active
    }

    fn update_drag(&mut self, pane: SftpPane, x: f32, y: f32) -> bool {
        let Some(was_active) = self.drag_state.as_ref().map(|drag| drag.active) else {
            return false;
        };
        if !self.update_drag_activation(x, y) {
            return false;
        }
        let active_changed = !was_active;
        let pane_changed = self.drag_over_pane != Some(pane);
        if pane_changed {
            self.drag_over_pane = Some(pane);
        }
        active_changed || pane_changed
    }

    fn update_drag_capture(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) -> bool {
        if self.update_drag_activation(f32::from(position.x), f32::from(position.y)) {
            self.drag_autoscroll_position = Some(position);
            let changed = self.apply_drag_autoscroll(position);
            self.schedule_drag_autoscroll(cx);
            changed
        } else {
            self.stop_drag_autoscroll();
            false
        }
    }

    fn schedule_drag_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.drag_autoscroll_scheduled {
            return;
        }
        self.drag_autoscroll_scheduled = true;
        cx.spawn(async move |entity, cx| {
            gpui::Timer::after(Duration::from_millis(16)).await;
            let _ = entity.update(cx, |sftp, cx| {
                sftp.drag_autoscroll_scheduled = false;
                let Some(position) = sftp.drag_autoscroll_position else {
                    return;
                };
                if !sftp.drag_state.as_ref().is_some_and(|drag| drag.active) {
                    sftp.stop_drag_autoscroll();
                    return;
                }
                if sftp.apply_drag_autoscroll(position) {
                    cx.notify();
                }
                sftp.schedule_drag_autoscroll(cx);
            });
        })
        .detach();
    }

    fn apply_drag_autoscroll(&self, position: Point<Pixels>) -> bool {
        uniform_list_edge_autoscroll(&self.local_file_scroll, position)
            | uniform_list_edge_autoscroll(&self.remote_file_scroll, position)
    }

    fn stop_drag_autoscroll(&mut self) {
        self.drag_autoscroll_position = None;
        self.drag_autoscroll_scheduled = false;
    }

    fn cancel_drag_capture(&mut self) -> bool {
        let had_drag = self.drag_state.take().is_some();
        let had_target = self.drag_over_pane.take().is_some();
        self.stop_drag_autoscroll();
        had_drag || had_target
    }
}

fn sftp_path_completion_candidate(entry: SftpFileEntry) -> PathCompletionCandidate {
    PathCompletionCandidate {
        name: entry.name,
        path: entry.path,
        is_directory: entry.file_type == SftpFileType::Directory,
    }
}
