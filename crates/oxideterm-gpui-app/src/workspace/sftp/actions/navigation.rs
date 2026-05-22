impl WorkspaceApp {
    pub(super) fn handle_sftp_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
        if matches!(self.sftp_view.dialog, Some(SftpDialog::Editor { .. })) {
            if event.keystroke.modifiers.platform && key == "s" {
                self.save_sftp_preview_editor(cx);
                cx.notify();
                return true;
            }
            if key == "escape" {
                self.request_close_sftp_editor();
                cx.notify();
                return true;
            }
            return false;
        }
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            match key {
                "a" => {
                    self.select_all_sftp_files(self.sftp_view.active_pane);
                    self.sftp_view.context_menu = None;
                    cx.notify();
                    return true;
                }
                "l" => {
                    self.start_sftp_path_edit(self.sftp_view.active_pane);
                    self.sftp_view.context_menu = None;
                    cx.notify();
                    return true;
                }
                _ => return false,
            }
        }
        if key == "escape" && self.dismiss_workspace_context_menus() {
            cx.notify();
            return true;
        }
        if self.sftp_view.dialog.is_some() && self.sftp_view.focused_input.is_none() {
            match key {
                "escape" => {
                    if let Some(SftpDialog::EditorCloseConfirm { name }) =
                        self.sftp_view.dialog.clone()
                    {
                        self.cancel_sftp_editor_close_confirm(name);
                    } else {
                        self.close_sftp_dialog();
                    }
                    cx.notify();
                    return true;
                }
                "u" => {
                    if matches!(self.sftp_view.dialog, Some(SftpDialog::Preview { .. }))
                        && self.sftp_preview_is_markdown_content()
                    {
                        self.sftp_view.preview_markdown_source_mode =
                            !self.sftp_view.preview_markdown_source_mode;
                        cx.notify();
                        return true;
                    }
                }
                "enter" => {
                    if matches!(
                        self.sftp_view.dialog,
                        Some(SftpDialog::EditorCloseConfirm { .. })
                    ) {
                        self.discard_sftp_editor_changes();
                    } else {
                        self.accept_sftp_dialog();
                    }
                    cx.notify();
                    return true;
                }
                _ => {}
            }
            return false;
        }
        if let Some(input) = self.sftp_view.focused_input {
            match key {
                "tab" if !event.keystroke.modifiers.platform && !event.keystroke.modifiers.control => {
                    self.handle_sftp_input_tab(input);
                    cx.notify();
                    return true;
                }
                "escape" => {
                    match input {
                        SftpInput::LocalPath => self.cancel_sftp_path_edit(SftpPane::Local),
                        SftpInput::RemotePath => self.cancel_sftp_path_edit(SftpPane::Remote),
                        _ => {
                            self.sftp_view.focused_input = None;
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
                            self.commit_sftp_path_input(pane);
                        }
                        SftpInput::DialogValue => self.accept_sftp_dialog(),
                        _ => {}
                    }
                    cx.notify();
                    return true;
                }
                "backspace" => {
                    self.sftp_input_value_mut(input).pop();
                    cx.notify();
                    return true;
                }
                _ => {}
            }
        }
        match key {
            "escape" => {
                self.sftp_view.context_menu = None;
                self.sftp_view.focused_input = None;
                cx.notify();
                true
            }
            "enter" => {
                if let Some(file) = self.single_selected_sftp_file(self.sftp_view.active_pane) {
                    // Tauri SFTP only opens directories on Enter; file quick-look is
                    // intentionally bound to Space and double-click.
                    if file.file_type == SftpFileType::Directory {
                        self.open_or_preview_sftp_file(self.sftp_view.active_pane, &file);
                        cx.notify();
                        return true;
                    }
                    false
                } else {
                    false
                }
            }
            "space" | " " => {
                if self.sftp_view.active_pane == SftpPane::Remote
                    && let Some(file) = self.single_selected_sftp_file(self.sftp_view.active_pane)
                    && file.file_type != SftpFileType::Directory
                {
                    self.open_or_preview_sftp_file(self.sftp_view.active_pane, &file);
                    cx.notify();
                    return true;
                }
                false
            }
            "right" | "arrowright" => {
                if self.sftp_view.active_pane == SftpPane::Local
                    && !self.sftp_view.local_selected.is_empty()
                {
                    self.queue_sftp_transfers(SftpPane::Local, SftpTransferDirection::Upload);
                    cx.notify();
                    return true;
                }
                false
            }
            "left" | "arrowleft" => {
                if self.sftp_view.active_pane == SftpPane::Remote
                    && !self.sftp_view.remote_selected.is_empty()
                {
                    self.queue_sftp_transfers(SftpPane::Remote, SftpTransferDirection::Download);
                    cx.notify();
                    return true;
                }
                false
            }
            "delete" | "backspace" => {
                let files = self.sftp_selected_names(self.sftp_view.active_pane);
                if !files.is_empty() {
                    self.sftp_view.dialog = Some(SftpDialog::Delete {
                        pane: self.sftp_view.active_pane,
                        files,
                    });
                    cx.notify();
                    return true;
                }
                false
            }
            "f2" | "F2" => {
                if let Some(file) = self.single_selected_sftp_file(self.sftp_view.active_pane) {
                    self.open_sftp_rename_dialog(self.sftp_view.active_pane, file.name);
                    cx.notify();
                    return true;
                }
                false
            }
            "up" | "arrowup" => {
                self.move_sftp_selection(self.sftp_view.active_pane, -1);
                cx.notify();
                true
            }
            "down" | "arrowdown" => {
                self.move_sftp_selection(self.sftp_view.active_pane, 1);
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(super) fn sftp_input_value(&self, input: SftpInput) -> &str {
        match input {
            SftpInput::LocalPath => &self.sftp_view.local_path_input,
            SftpInput::RemotePath => &self.sftp_view.remote_path_input,
            SftpInput::LocalFilter => &self.sftp_view.local_filter,
            SftpInput::RemoteFilter => &self.sftp_view.remote_filter,
            SftpInput::DialogValue => &self.sftp_view.dialog_value,
        }
    }

    pub(super) fn sftp_input_value_mut(&mut self, input: SftpInput) -> &mut String {
        match input {
            SftpInput::LocalPath => &mut self.sftp_view.local_path_input,
            SftpInput::RemotePath => &mut self.sftp_view.remote_path_input,
            SftpInput::LocalFilter => &mut self.sftp_view.local_filter,
            SftpInput::RemoteFilter => &mut self.sftp_view.remote_filter,
            SftpInput::DialogValue => &mut self.sftp_view.dialog_value,
        }
    }

    fn set_sftp_path(&mut self, pane: SftpPane, path: String) {
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_path_scroll_x = 0.0;
                self.sftp_view.local_path = path.clone();
                self.sftp_view.local_path_input = path.clone();
                if let Some(node_id) = self.sftp_view_node.clone() {
                    self.sftp_local_path_memory.insert(node_id, path.clone());
                }
                self.sftp_view.editing_local_path = false;
                self.sftp_view.local_files = list_local_files(&path).unwrap_or_else(|error| {
                    vec![sftp_file_entry(
                        format!("Unable to read folder: {error}"),
                        path.clone(),
                        SftpFileType::File,
                        0,
                        None,
                    )]
                });
                self.sftp_view.local_selected.clear();
                self.sftp_view.local_last_selected = None;
            }
            SftpPane::Remote => {
                self.sftp_view.remote_path_scroll_x = 0.0;
                self.sftp_view.remote_path = path.clone();
                self.sftp_view.remote_path_input = path;
                self.sftp_view.editing_remote_path = false;
                self.sftp_view.remote_loading = true;
                self.sftp_view.remote_load_pending = true;
                self.sftp_view.remote_selected.clear();
                self.sftp_view.remote_last_selected = None;
            }
        }
        self.sftp_view.focused_input = None;
        self.sftp_view.context_menu = None;
    }

    fn cancel_sftp_path_edit(&mut self, pane: SftpPane) {
        // Tauri's editable SFTP path input cancels on DOM blur unless the Go
        // button takes focus. Native does not model that button focus target
        // yet, so Tab/Escape restore the current committed path explicitly.
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_path_input = self.sftp_view.local_path.clone();
                self.sftp_view.editing_local_path = false;
                if self.sftp_view.focused_input == Some(SftpInput::LocalPath) {
                    self.sftp_view.focused_input = None;
                }
            }
            SftpPane::Remote => {
                self.sftp_view.remote_path_input = self.sftp_view.remote_path.clone();
                self.sftp_view.editing_remote_path = false;
                if self.sftp_view.focused_input == Some(SftpInput::RemotePath) {
                    self.sftp_view.focused_input = None;
                }
            }
        }
        self.ime_marked_text = None;
        self.clear_ime_selection();
    }

    fn handle_sftp_input_tab(&mut self, input: SftpInput) {
        // Browser Tab moves focus out of the current input. Until the native
        // toolbar buttons have first-class focus targets, mirror the observable
        // blur side-effect so path edits do not get stuck in captured input mode.
        match input {
            SftpInput::LocalPath => self.cancel_sftp_path_edit(SftpPane::Local),
            SftpInput::RemotePath => self.cancel_sftp_path_edit(SftpPane::Remote),
            SftpInput::LocalFilter | SftpInput::RemoteFilter | SftpInput::DialogValue => {
                self.sftp_view.focused_input = None;
                self.ime_marked_text = None;
                self.clear_ime_selection();
            }
        }
    }

    fn start_sftp_path_edit(&mut self, pane: SftpPane) {
        self.sftp_view.active_pane = pane;
        match pane {
            SftpPane::Local => {
                self.sftp_view.editing_local_path = true;
                self.sftp_view.local_path_input = self.sftp_view.local_path.clone();
                self.sftp_view.focused_input = Some(SftpInput::LocalPath);
            }
            SftpPane::Remote => {
                self.sftp_view.editing_remote_path = true;
                self.sftp_view.remote_path_input = self.sftp_view.remote_path.clone();
                self.sftp_view.focused_input = Some(SftpInput::RemotePath);
            }
        }
    }

    fn handle_sftp_breadcrumb_scroll(
        &mut self,
        pane: SftpPane,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let delta = event.delta.pixel_delta(px(SFTP_PANE_HEADER_HEIGHT));
        let horizontal = if f32::from(delta.x).abs() > f32::from(delta.y).abs() {
            f32::from(delta.x)
        } else {
            f32::from(delta.y)
        };
        if horizontal == 0.0 {
            return;
        }

        let path = match pane {
            SftpPane::Local => &self.sftp_view.local_path,
            SftpPane::Remote => &self.sftp_view.remote_path,
        };
        let segments = sftp_path_segments(path, pane == SftpPane::Remote);
        let max_scroll = sftp_breadcrumb_max_scroll(
            &segments,
            sftp_path_bar_viewport_width(window),
            SFTP_ICON_MD,
        );
        if max_scroll <= 0.0 {
            return;
        }

        let scroll = match pane {
            SftpPane::Local => &mut self.sftp_view.local_path_scroll_x,
            SftpPane::Remote => &mut self.sftp_view.remote_path_scroll_x,
        };
        *scroll = (*scroll + horizontal).clamp(0.0, max_scroll);
        cx.stop_propagation();
        cx.notify();
    }

    fn commit_sftp_path_input(&mut self, pane: SftpPane) {
        let path = match pane {
            SftpPane::Local => self.sftp_view.local_path_input.trim().to_string(),
            SftpPane::Remote => normalize_remote_path(&self.sftp_view.remote_path_input),
        };
        if !path.is_empty() {
            self.set_sftp_path(pane, path);
        }
    }

    fn navigate_sftp_path(&mut self, pane: SftpPane, target: &str) {
        let next = match (pane, target) {
            (SftpPane::Local, "~") => home_path(),
            (SftpPane::Remote, "~") => self
                .active_tab_id
                .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
                .and_then(|node_id| self.sftp_remote_home_by_node.get(node_id))
                .cloned()
                .unwrap_or_else(|| "/".to_string()),
            (SftpPane::Local, "..") => parent_path(&self.sftp_view.local_path, false),
            (SftpPane::Remote, "..") => parent_path(&self.sftp_view.remote_path, true),
            _ => target.to_string(),
        };
        self.set_sftp_path(pane, next);
    }

    fn toggle_sftp_sort(&mut self, pane: SftpPane, field: SftpSortField) {
        let (sort_field, sort_direction) = match pane {
            SftpPane::Local => (
                &mut self.sftp_view.local_sort_field,
                &mut self.sftp_view.local_sort_direction,
            ),
            SftpPane::Remote => (
                &mut self.sftp_view.remote_sort_field,
                &mut self.sftp_view.remote_sort_direction,
            ),
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

    fn select_sftp_file(&mut self, pane: SftpPane, name: String, modifiers: gpui::Modifiers) {
        self.sftp_view.active_pane = pane;
        self.sftp_view.context_menu = None;
        let range_names = self.sftp_ordered_file_names(pane);
        let (selected, last_selected) = match pane {
            SftpPane::Local => (
                &mut self.sftp_view.local_selected,
                &mut self.sftp_view.local_last_selected,
            ),
            SftpPane::Remote => (
                &mut self.sftp_view.remote_selected,
                &mut self.sftp_view.remote_last_selected,
            ),
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

    fn start_sftp_drag_candidate(&mut self, pane: SftpPane, x: f32, y: f32) {
        let names = self.sftp_selected_names(pane);
        if names.is_empty() {
            self.sftp_view.drag_state = None;
            self.stop_sftp_drag_autoscroll();
            return;
        }
        self.sftp_view.drag_state = Some(SftpDragState {
            source_pane: pane,
            names,
            start_x: x,
            start_y: y,
            active: false,
        });
        self.sftp_view.drag_over_pane = None;
        self.stop_sftp_drag_autoscroll();
    }

    fn update_sftp_drag(&mut self, pane: SftpPane, x: f32, y: f32) {
        if self.update_sftp_drag_activation(x, y) {
            self.sftp_view.drag_over_pane = Some(pane);
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
        if self.update_sftp_drag_activation(f32::from(position.x), f32::from(position.y)) {
            self.sftp_view.drag_autoscroll_position = Some(position);
            if self.apply_sftp_drag_autoscroll(position) {
                cx.notify();
            }
            self.schedule_sftp_drag_autoscroll(cx);
        } else {
            self.stop_sftp_drag_autoscroll();
        }
    }

    fn update_sftp_drag_activation(&mut self, x: f32, y: f32) -> bool {
        let Some(drag) = self.sftp_view.drag_state.as_mut() else {
            return false;
        };
        let dx = x - drag.start_x;
        let dy = y - drag.start_y;
        if !drag.active && (dx * dx + dy * dy).sqrt() >= 5.0 {
            drag.active = true;
        }
        drag.active
    }

    fn finish_sftp_drag(&mut self, pane: SftpPane) {
        let Some(drag) = self.sftp_view.drag_state.take() else {
            self.sftp_view.drag_over_pane = None;
            self.stop_sftp_drag_autoscroll();
            return;
        };
        self.sftp_view.drag_over_pane = None;
        self.stop_sftp_drag_autoscroll();
        if !drag.active || drag.source_pane == pane {
            return;
        }
        match (drag.source_pane, pane) {
            (SftpPane::Local, SftpPane::Remote) => {
                self.queue_sftp_named_transfers(
                    SftpPane::Local,
                    SftpTransferDirection::Upload,
                    drag.names,
                );
            }
            (SftpPane::Remote, SftpPane::Local) => {
                self.queue_sftp_named_transfers(
                    SftpPane::Remote,
                    SftpTransferDirection::Download,
                    drag.names,
                );
            }
            _ => {}
        }
    }

    pub(in crate::workspace) fn cancel_sftp_drag_capture(&mut self) -> bool {
        // Browser pointer capture always produces a terminal mouse-up. If the
        // user releases outside both panes, cancel the candidate so hover rings
        // and pending drag state cannot remain latched.
        let had_drag = self.sftp_view.drag_state.take().is_some();
        let had_target = self.sftp_view.drag_over_pane.take().is_some();
        self.stop_sftp_drag_autoscroll();
        had_drag || had_target
    }

    fn schedule_sftp_drag_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.sftp_view.drag_autoscroll_scheduled {
            return;
        }
        self.sftp_view.drag_autoscroll_scheduled = true;
        cx.spawn(async move |weak, cx| {
            gpui::Timer::after(std::time::Duration::from_millis(16)).await;
            let _ = weak.update(cx, |this, cx| {
                this.sftp_view.drag_autoscroll_scheduled = false;
                let Some(position) = this.sftp_view.drag_autoscroll_position else {
                    return;
                };
                if !this
                    .sftp_view
                    .drag_state
                    .as_ref()
                    .is_some_and(|drag| drag.active)
                {
                    this.stop_sftp_drag_autoscroll();
                    return;
                }
                if this.apply_sftp_drag_autoscroll(position) {
                    cx.notify();
                }
                this.schedule_sftp_drag_autoscroll(cx);
            });
        })
        .detach();
    }

    fn apply_sftp_drag_autoscroll(&mut self, position: gpui::Point<gpui::Pixels>) -> bool {
        // Tauri file panes inherit browser drag-scroll behavior from their
        // overflow containers. Native SFTP uses GPUI uniform lists, so bridge
        // the pointer position to each pane's tracked scroll handle.
        uniform_list_edge_autoscroll(&self.sftp_view.local_file_scroll, position)
            | uniform_list_edge_autoscroll(&self.sftp_view.remote_file_scroll, position)
    }

    fn stop_sftp_drag_autoscroll(&mut self) {
        self.sftp_view.drag_autoscroll_position = None;
        self.sftp_view.drag_autoscroll_scheduled = false;
    }

    fn clear_sftp_selection(&mut self, pane: SftpPane) {
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_selected.clear();
                self.sftp_view.local_last_selected = None;
            }
            SftpPane::Remote => {
                self.sftp_view.remote_selected.clear();
                self.sftp_view.remote_last_selected = None;
            }
        }
    }

    fn select_all_sftp_files(&mut self, pane: SftpPane) {
        let names = self.sftp_ordered_file_names(pane);
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_selected = names.iter().cloned().collect();
                self.sftp_view.local_last_selected = names.last().cloned();
            }
            SftpPane::Remote => {
                self.sftp_view.remote_selected = names.iter().cloned().collect();
                self.sftp_view.remote_last_selected = names.last().cloned();
            }
        }
    }

    fn move_sftp_selection(&mut self, pane: SftpPane, delta: isize) {
        let names = self.sftp_ordered_file_names(pane);
        if names.is_empty() {
            return;
        }
        let current = self
            .sftp_selected_names(pane)
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
        match pane {
            SftpPane::Local => {
                self.sftp_view.local_selected.clear();
                self.sftp_view.local_selected.insert(name.clone());
                self.sftp_view.local_last_selected = Some(name);
            }
            SftpPane::Remote => {
                self.sftp_view.remote_selected.clear();
                self.sftp_view.remote_selected.insert(name.clone());
                self.sftp_view.remote_last_selected = Some(name);
            }
        }
        // Tauri calls `scrollIntoView({ block: 'nearest' })` after keyboard
        // movement. GPUI's uniform list exposes the same deferred "reveal if
        // needed" behavior through a non-strict scroll request.
        match pane {
            SftpPane::Local => scroll_tauri_virtual_list_to_index(
                &self.sftp_view.local_file_scroll,
                next,
                TauriVirtualScrollAlign::Nearest,
            ),
            SftpPane::Remote => scroll_tauri_virtual_list_to_index(
                &self.sftp_view.remote_file_scroll,
                next,
                TauriVirtualScrollAlign::Nearest,
            ),
        }
    }

    fn sftp_ordered_file_names(&self, pane: SftpPane) -> Vec<String> {
        let (files, filter, field, direction) = match pane {
            SftpPane::Local => (
                &self.sftp_view.local_files,
                &self.sftp_view.local_filter,
                self.sftp_view.local_sort_field,
                self.sftp_view.local_sort_direction,
            ),
            SftpPane::Remote => (
                &self.sftp_view.remote_files,
                &self.sftp_view.remote_filter,
                self.sftp_view.remote_sort_field,
                self.sftp_view.remote_sort_direction,
            ),
        };
        sorted_sftp_files(files, filter, field, direction)
            .into_iter()
            .map(|file| file.name)
            .collect()
    }

    fn sftp_selected_names(&self, pane: SftpPane) -> Vec<String> {
        let selected = match pane {
            SftpPane::Local => &self.sftp_view.local_selected,
            SftpPane::Remote => &self.sftp_view.remote_selected,
        };
        self.sftp_ordered_file_names(pane)
            .into_iter()
            .filter(|name| selected.contains(name))
            .collect()
    }

    fn single_selected_sftp_file(&self, pane: SftpPane) -> Option<SftpFileEntry> {
        let selected = self.sftp_selected_names(pane);
        if selected.len() != 1 {
            return None;
        }
        let name = selected.first()?;
        let files = match pane {
            SftpPane::Local => &self.sftp_view.local_files,
            SftpPane::Remote => &self.sftp_view.remote_files,
        };
        files.iter().find(|file| &file.name == name).cloned()
    }
}
