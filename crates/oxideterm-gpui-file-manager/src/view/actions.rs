mod operations;
use super::*;

mod bookmarks;

const FILE_MANAGER_DIALOG_FOOTER_ACTIONS: [ConfirmDialogAction; 2] =
    [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];

impl FileManagerView {
    pub fn open_location(&mut self, initial_path: Option<String>, cx: &mut Context<Self>) {
        if let Some(path) = initial_path {
            self.set_file_manager_path(path, cx);
        } else {
            self.refresh_file_manager(cx);
        }
    }
    pub fn handle_file_manager_key(
        &mut self,
        event: &KeyDownEvent,
        file_action: Option<&'static str>,
        preview_action: Option<&'static str>,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        if let Some(input) = self.file_manager.read(cx).focused_input {
            // Inline inputs share the workspace IME/text-editing model so
            // selection, caret movement, and platform shortcuts stay browser-like.
            if input == FileManagerInput::Path
                && self.handle_file_manager_path_completion_key(event, cx)
            {
                cx.notify();
                return true;
            }
            match key {
                "tab"
                    if !event.keystroke.modifiers.platform
                        && !event.keystroke.modifiers.control =>
                {
                    self.handle_file_manager_input_tab(input, event.keystroke.modifiers.shift, cx);
                    cx.notify();
                    return true;
                }
                "escape" => {
                    match input {
                        FileManagerInput::Path => self.cancel_file_manager_path_edit(cx),
                        FileManagerInput::Filter => {
                            self.file_manager.update(cx, |file_manager, cx| {
                                file_manager.focused_input = None;
                                cx.notify();
                            });
                            self.request(FileManagerRequest::ClearIme, cx);
                        }
                        FileManagerInput::DialogValue => {}
                    }
                    cx.notify();
                    return true;
                }
                "enter" => {
                    match input {
                        FileManagerInput::Path => self.commit_file_manager_path_input(cx),
                        FileManagerInput::DialogValue => {}
                        FileManagerInput::Filter => {}
                    }
                    cx.notify();
                    return true;
                }
                _ => {}
            }
            return false;
        }
        if self.handle_file_manager_dialog_footer_key(event, cx) {
            return true;
        }
        let (preview_dialog_open, is_video_preview) = {
            let file_manager = self.file_manager.read(cx);
            (
                matches!(file_manager.dialog, Some(FileManagerDialog::Preview { .. })),
                matches!(
                    file_manager.preview.as_deref(),
                    Some(LocalPreview::Video { .. })
                ),
            )
        };
        if preview_dialog_open {
            let command = match preview_action {
                Some("preview.previous") => "left",
                Some("preview.next") => "right",
                Some("preview.metadata") => "i",
                Some("preview.source") => "u",
                Some("preview.zoomIn") => "+",
                Some("preview.zoomOut") => "-",
                Some("preview.resetZoom") => "0",
                Some("preview.rotate") => "r",
                _ if file_action == Some("fileManager.preview") => "space",
                _ if key == "escape" => "escape",
                _ => "",
            };
            match command {
                "escape" => {
                    self.begin_file_manager_rich_dialog_exit(cx);
                    return true;
                }
                "space" | " " if !is_video_preview => {
                    self.begin_file_manager_rich_dialog_exit(cx);
                    return true;
                }
                "arrowleft" | "left" if !is_video_preview => {
                    self.navigate_file_manager_preview(-1, cx);
                    cx.notify();
                    return true;
                }
                "arrowright" | "right" if !is_video_preview => {
                    self.navigate_file_manager_preview(1, cx);
                    cx.notify();
                    return true;
                }
                "i" => {
                    self.file_manager.update(cx, |file_manager, cx| {
                        file_manager.preview_show_metadata = !file_manager.preview_show_metadata;
                        cx.notify();
                    });
                    return true;
                }
                "u" => {
                    if matches!(
                        self.file_manager.read(cx).preview.as_deref(),
                        Some(LocalPreview::Markdown { .. })
                    ) {
                        self.file_manager.update(cx, |file_manager, cx| {
                            file_manager.preview_markdown_source =
                                !file_manager.preview_markdown_source;
                            if file_manager.preview_markdown_source {
                                file_manager
                                    .preview_markdown_cache
                                    .release(&mut file_manager.preview_markdown_scroll);
                            }
                            cx.notify();
                        });
                        return true;
                    }
                }
                "+" | "=" => {
                    if matches!(
                        self.file_manager.read(cx).preview.as_deref(),
                        Some(LocalPreview::Image { .. })
                    ) {
                        self.file_manager.update(cx, |file_manager, cx| {
                            file_manager.preview_image_zoom = (file_manager.preview_image_zoom
                                + 0.25)
                                .min(FILE_MANAGER_PREVIEW_MAX_ZOOM);
                            cx.notify();
                        });
                        return true;
                    }
                }
                "-" => {
                    if matches!(
                        self.file_manager.read(cx).preview.as_deref(),
                        Some(LocalPreview::Image { .. })
                    ) {
                        self.file_manager.update(cx, |file_manager, cx| {
                            file_manager.preview_image_zoom = (file_manager.preview_image_zoom
                                - 0.25)
                                .max(FILE_MANAGER_PREVIEW_MIN_ZOOM);
                            cx.notify();
                        });
                        return true;
                    }
                }
                "0" => {
                    if matches!(
                        self.file_manager.read(cx).preview.as_deref(),
                        Some(LocalPreview::Image { .. })
                    ) {
                        self.file_manager.update(cx, |file_manager, cx| {
                            file_manager.preview_image_zoom = 1.0;
                            file_manager.preview_image_rotation = 0;
                            cx.notify();
                        });
                        return true;
                    }
                }
                "r" => {
                    if matches!(
                        self.file_manager.read(cx).preview.as_deref(),
                        Some(LocalPreview::Image { .. })
                    ) {
                        self.file_manager.update(cx, |file_manager, cx| {
                            file_manager.preview_image_rotation =
                                (file_manager.preview_image_rotation + 90) % 360;
                            cx.notify();
                        });
                        return true;
                    }
                }
                _ => {}
            }
        }
        if self.file_manager.read(cx).dialog.is_some() {
            return false;
        }
        let command = match file_action {
            Some("fileManager.selectAll") => {
                self.select_all_file_manager_files(cx);
                return true;
            }
            Some("fileManager.copy") => {
                self.copy_file_manager_selection(false, cx);
                return true;
            }
            Some("fileManager.cut") => {
                self.copy_file_manager_selection(true, cx);
                return true;
            }
            Some("fileManager.paste") => {
                self.paste_file_manager_clipboard(cx);
                return true;
            }
            Some("fileManager.editPath") => {
                self.start_file_manager_path_edit(cx);
                return true;
            }
            Some("fileManager.open") => "enter",
            Some("fileManager.preview") => "space",
            Some("fileManager.delete") => "delete",
            Some("fileManager.deleteOrParent") => "backspace",
            Some("fileManager.rename") => "f2",
            _ if key == "escape" => "escape",
            _ => "",
        };
        match command {
            "escape" => {
                self.dismiss_file_manager_context_menu(cx);
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.dialog = None;
                    file_manager.focused_input = None;
                    file_manager.focused_dialog_footer_action = None;
                    cx.notify();
                });
                true
            }
            "enter" => {
                if let Some(file) = self.single_selected_file_manager_file(cx) {
                    self.open_file_manager_entry(file, cx);
                    cx.notify();
                    return true;
                }
                false
            }
            "space" | " " => {
                if let Some(file) = self.single_selected_file_manager_file(cx)
                    && file.file_type != LocalFileType::Directory
                {
                    self.open_file_manager_preview(file, cx);
                    cx.notify();
                    return true;
                }
                false
            }
            "delete" => {
                // Tauri FileList handles Delete as the selected-file delete
                // shortcut. Keep it ahead of any navigation fallback.
                if !self.file_manager.read(cx).selected.is_empty() {
                    self.open_file_manager_delete_dialog(cx);
                    return true;
                }
                false
            }
            "backspace" => {
                // Browser/React FileList receives Backspace while the list is
                // focused: selected rows delete; an empty selection keeps the
                // native file-manager convenience of navigating to the parent.
                if !self.file_manager.read(cx).selected.is_empty() {
                    self.open_file_manager_delete_dialog(cx);
                    return true;
                }
                self.navigate_file_manager_parent(cx);
                true
            }
            "f2" | "F2" => {
                if let Some(file) = self.single_selected_file_manager_file(cx) {
                    self.open_file_manager_rename_dialog(file.name, cx);
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    pub(super) fn refresh_file_manager(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.refresh();
            cx.notify();
        });
    }

    pub(super) fn refresh_file_manager_with_drives(&mut self, cx: &mut Context<Self>) {
        // Explicit refresh updates both the current directory and mounted volumes.
        let path = self.file_manager.read(cx).path.clone();
        let audit = local_file_operation("file_browse", &path);
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.refresh_drives();
            file_manager.refresh();
            audit.finish(
                if file_manager.error.is_none() {
                    oxideterm_audit::AuditOutcome::Succeeded
                } else {
                    oxideterm_audit::AuditOutcome::Failed
                },
                oxideterm_audit::AuditEvidence::Protocol,
                None,
                None,
            );
            cx.notify();
        });
    }

    pub(super) fn open_file_manager_drives_dialog(&mut self, cx: &mut Context<Self>) {
        // Refresh before presenting the picker so newly mounted volumes are visible.
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.refresh_drives();
            file_manager.dialog = Some(FileManagerDialog::Drives);
            cx.notify();
        });
    }

    pub fn refresh_file_manager_drives(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.refresh_drives();
            cx.notify();
        });
    }

    pub fn set_file_manager_path(&mut self, path: String, cx: &mut Context<Self>) {
        let audit = local_file_operation("file_browse", &path);
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.set_path(path);
            audit.finish(
                if file_manager.error.is_none() {
                    oxideterm_audit::AuditOutcome::Succeeded
                } else {
                    oxideterm_audit::AuditOutcome::Failed
                },
                oxideterm_audit::AuditEvidence::Protocol,
                None,
                None,
            );
            cx.notify();
        });
    }

    pub(in crate::view) fn handle_file_manager_breadcrumb_scroll(
        &mut self,
        event: &ScrollEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(changed) = scroll_breadcrumb_by_wheel(
            &self.file_manager.read(cx).path_scroll,
            event,
            px(FILE_MANAGER_HEADER_HEIGHT),
        ) {
            cx.stop_propagation();
            if changed {
                cx.notify();
            }
        }
    }

    pub(super) fn commit_file_manager_path_input(&mut self, cx: &mut Context<Self>) {
        let path = self.file_manager.read(cx).path_input.trim().to_string();
        if path.is_empty() {
            return;
        }
        self.set_file_manager_path(path, cx);
    }

    pub(super) fn navigate_file_manager_parent(&mut self, cx: &mut Context<Self>) {
        let current_path = self.file_manager.read(cx).path.clone();
        if let Some(parent) = local_parent_path(&current_path) {
            self.set_file_manager_path(parent, cx);
        } else {
            self.open_file_manager_drives_dialog(cx);
        }
    }

    pub fn open_file_manager_entry(&mut self, entry: LocalFileEntry, cx: &mut Context<Self>) {
        match entry.file_type {
            LocalFileType::Directory => self.set_file_manager_path(entry.path, cx),
            LocalFileType::File | LocalFileType::Symlink => {
                // Tauri's FileList treats both Enter and double-click on files
                // as Quick Look. External open remains an explicit context-menu
                // action, so the default list action stays non-destructive.
                self.open_file_manager_preview(entry, cx);
            }
        }
    }

    pub(super) fn start_file_manager_path_edit(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.path_input = file_manager.path.clone();
            file_manager.editing_path = true;
            file_manager.focused_input = Some(FileManagerInput::Path);
            file_manager.focused_dialog_footer_action = None;
            cx.notify();
        });
        self.request(FileManagerRequest::ClearIme, cx);
    }

    pub(super) fn cancel_file_manager_path_edit(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.path_input = file_manager.path.clone();
            file_manager.path_completion.dismiss();
            if file_manager.focused_input == Some(FileManagerInput::Path) {
                file_manager.focused_input = None;
            }
            file_manager.editing_path = false;
            cx.notify();
        });
        self.request(FileManagerRequest::ClearIme, cx);
    }

    fn handle_file_manager_input_tab(
        &mut self,
        input: FileManagerInput,
        shift: bool,
        cx: &mut Context<Self>,
    ) {
        // Tauri FileList inputs are real DOM controls: Tab first blurs the
        // current text field, and the path editor's onBlur cancels unsubmitted
        // edits unless the Go button receives focus. Native has no button focus
        // owner here yet, so preserve the blur/cancel half explicitly.
        match input {
            FileManagerInput::Path => self.cancel_file_manager_path_edit(cx),
            FileManagerInput::Filter | FileManagerInput::DialogValue => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.focused_input = None;
                    if input == FileManagerInput::DialogValue {
                        file_manager.focused_dialog_footer_action = Some(if shift {
                            ConfirmDialogAction::Confirm
                        } else {
                            ConfirmDialogAction::Cancel
                        });
                    }
                    cx.notify();
                });
                self.request(FileManagerRequest::ClearIme, cx);
            }
        }
        self.request(FileManagerRequest::ClearImeSelection, cx);
    }

    pub fn handle_file_manager_dialog_input_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let (input_available, focused_input, focused_footer) = {
            let file_manager = self.file_manager.read(cx);
            (
                matches!(
                    file_manager.dialog,
                    Some(
                        FileManagerDialog::NewFolder
                            | FileManagerDialog::NewFile
                            | FileManagerDialog::Rename { .. }
                            | FileManagerDialog::EditBookmark { .. }
                    )
                ),
                file_manager.focused_input,
                file_manager.focused_dialog_footer_action,
            )
        };
        if !input_available && focused_input != Some(FileManagerInput::DialogValue) {
            return false;
        }

        match browser_behavior::modal_footer_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &FILE_MANAGER_DIALOG_FOOTER_ACTIONS,
            input_available,
            focused_input == Some(FileManagerInput::DialogValue),
            focused_footer,
            ConfirmDialogAction::Cancel,
            Some(ConfirmDialogAction::Confirm),
        ) {
            Some(browser_behavior::ModalFooterInputKeyAction::Cancel) => {
                self.close_file_manager_dialog(cx);
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::FocusInput) => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.focused_input = Some(FileManagerInput::DialogValue);
                    file_manager.focused_dialog_footer_action = None;
                    cx.notify();
                });
                self.request(FileManagerRequest::ClearIme, cx);
                self.request(FileManagerRequest::ClearImeSelection, cx);
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::FocusFooter(action)) => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.focused_input = None;
                    file_manager.focused_dialog_footer_action = Some(action);
                    cx.notify();
                });
                self.request(FileManagerRequest::ClearIme, cx);
                self.request(FileManagerRequest::ClearImeSelection, cx);
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterInputKeyAction::Activate(action)) => {
                match action {
                    ConfirmDialogAction::Cancel => self.close_file_manager_dialog(cx),
                    ConfirmDialogAction::Confirm => self.accept_file_manager_dialog(cx),
                }
                cx.notify();
                true
            }
            None => false,
        }
    }

    pub(super) fn blur_file_manager_inline_inputs(&mut self, cx: &mut Context<Self>) {
        let (editing_path, focused_input) = {
            let file_manager = self.file_manager.read(cx);
            (file_manager.editing_path, file_manager.focused_input)
        };
        if editing_path || focused_input == Some(FileManagerInput::Path) {
            self.cancel_file_manager_path_edit(cx);
        } else if focused_input == Some(FileManagerInput::Filter) {
            self.file_manager.update(cx, |file_manager, cx| {
                file_manager.focused_input = None;
                cx.notify();
            });
            self.request(FileManagerRequest::ClearIme, cx);
        }
    }

    /// Refreshes path suggestions from one cached parent-directory listing.
    pub fn refresh_file_manager_path_completion(&mut self, cx: &mut Context<Self>) {
        let request = {
            let file_manager = self.file_manager.read(cx);
            local_path_completion_request(&file_manager.path_input)
        };
        let Some(request) = request else {
            self.file_manager.update(cx, |file_manager, cx| {
                file_manager.path_completion.dismiss();
                cx.notify();
            });
            return;
        };
        let request_state = self.file_manager.update(cx, |file_manager, _cx| {
            file_manager.path_completion.request(request)
        });
        let Some((generation, parent_path)) = request_state else {
            return;
        };
        let entries = list_local_files(&parent_path)
            .unwrap_or_default()
            .into_iter()
            .map(|entry| {
                let is_directory = entry.is_directory_like();
                PathCompletionCandidate {
                    name: entry.name,
                    path: entry.path,
                    is_directory,
                }
            })
            .collect();
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager
                .path_completion
                .apply_entries(generation, &parent_path, entries);
            cx.notify();
        });
    }

    pub(super) fn accept_file_manager_path_completion(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        self.accept_file_manager_path_completion_without_context(index, cx);
    }

    fn handle_file_manager_path_completion_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.file_manager.read(cx).path_completion.is_visible()
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.control
            || event.keystroke.modifiers.alt
        {
            return false;
        }
        match event.keystroke.key.as_str() {
            "up" | "arrowup" => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.path_completion.move_selection(-1);
                    cx.notify();
                });
                true
            }
            "down" | "arrowdown" => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.path_completion.move_selection(1);
                    cx.notify();
                });
                true
            }
            "enter" | "tab" => {
                let Some(index) = self
                    .file_manager
                    .read(cx)
                    .path_completion
                    .acceptance_index(&event.keystroke.key)
                else {
                    return false;
                };
                self.accept_file_manager_path_completion_without_context(index, cx);
                true
            }
            "escape" => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.path_completion.dismiss();
                    cx.notify();
                });
                true
            }
            _ => false,
        }
    }

    fn accept_file_manager_path_completion_without_context(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let (candidate, parent_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.path_completion.candidate(index).cloned(),
                file_manager
                    .path_completion
                    .parent_path()
                    .map(str::to_string)
                    .unwrap_or_else(|| file_manager.path.clone()),
            )
        };
        let Some(candidate) = candidate else {
            return;
        };
        if candidate.is_directory {
            self.set_file_manager_path(candidate.path, cx);
        } else {
            // File suggestions navigate to their parent and preserve the intended row selection.
            self.set_file_manager_path(parent_path, cx);
            self.file_manager.update(cx, |file_manager, cx| {
                if file_manager
                    .files
                    .iter()
                    .any(|entry| entry.name == candidate.name)
                {
                    file_manager.selected.insert(candidate.name.clone());
                    file_manager.last_selected = Some(candidate.name);
                    cx.notify();
                }
            });
        }
    }

    pub(super) fn open_file_manager_context_menu(
        &mut self,
        file: Option<LocalFileEntry>,
        x: f32,
        y: f32,
        cx: &mut Context<Self>,
    ) {
        self.blur_file_manager_inline_inputs(cx);
        self.file_manager.update(cx, |file_manager, cx| {
            if let Some(file) = file.as_ref()
                && browser_behavior::preserve_or_move_context_selection(
                    &mut file_manager.selected,
                    file.name.clone(),
                )
            {
                file_manager.last_selected = Some(file.name.clone());
            }
            file_manager.context_menu_presence.reopen();
            file_manager.context_menu_exit_generation = None;
            file_manager.context_menu = Some(FileManagerContextMenu { file, x, y });
            cx.notify();
        });
    }

    pub fn dismiss_file_manager_context_menu(&mut self, cx: &mut Context<Self>) -> bool {
        // Radix ContextMenu has one dismissal owner regardless of whether the
        // close came from outside click, Esc, or an item activation. Keep the
        // FileManager menu payload behind this helper so global browser
        // dismissal does not mutate feature state ad hoc.
        if self.file_manager.read(cx).context_menu.is_none() {
            return false;
        }
        self.clear_file_manager_context_menu_immediately(cx)
    }

    pub fn clear_file_manager_context_menu_immediately(&mut self, cx: &mut Context<Self>) -> bool {
        self.file_manager.update(cx, |file_manager, cx| {
            let changed = file_manager.clear_context_menu_immediately();
            if changed {
                cx.notify();
            }
            changed
        })
    }

    pub(super) fn clear_file_manager_selection(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.selected.clear();
            file_manager.last_selected = None;
            cx.notify();
        });
    }

    pub(super) fn select_all_file_manager_files(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            let files = file_manager.sorted_files();
            file_manager.selected = files.iter().map(|file| file.name.clone()).collect();
            file_manager.last_selected = file_manager.selected.iter().next().cloned();
            cx.notify();
        });
    }

    pub(super) fn selected_file_manager_names(&self, cx: &App) -> Vec<String> {
        self.file_manager.read(cx).selected_names()
    }

    pub(super) fn selected_file_manager_entries(&self, cx: &App) -> Vec<LocalFileEntry> {
        self.file_manager.read(cx).selected_entries()
    }

    pub(super) fn single_selected_file_manager_file(&self, cx: &App) -> Option<LocalFileEntry> {
        self.file_manager.read(cx).single_selected_file()
    }

    pub(super) fn toggle_file_manager_sort(
        &mut self,
        field: LocalSortField,
        cx: &mut Context<Self>,
    ) {
        self.blur_file_manager_inline_inputs(cx);
        self.file_manager.update(cx, |file_manager, cx| {
            if file_manager.sort_field == field {
                file_manager.sort_direction = match file_manager.sort_direction {
                    LocalSortDirection::Asc => LocalSortDirection::Desc,
                    LocalSortDirection::Desc => LocalSortDirection::Asc,
                };
            } else {
                file_manager.sort_field = field;
                file_manager.sort_direction = LocalSortDirection::Asc;
            }
            cx.notify();
        });
    }

    pub(super) fn open_file_manager_new_folder_dialog(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.dialog = Some(FileManagerDialog::NewFolder);
            file_manager.dialog_value.clear();
            file_manager.focused_input = Some(FileManagerInput::DialogValue);
            file_manager.focused_dialog_footer_action = None;
            cx.notify();
        });
    }

    pub(super) fn open_file_manager_new_file_dialog(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.dialog = Some(FileManagerDialog::NewFile);
            file_manager.dialog_value.clear();
            file_manager.focused_input = Some(FileManagerInput::DialogValue);
            file_manager.focused_dialog_footer_action = None;
            cx.notify();
        });
    }

    pub(super) fn open_file_manager_rename_dialog(
        &mut self,
        old_name: String,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.dialog = Some(FileManagerDialog::Rename {
                old_name: old_name.clone(),
            });
            file_manager.dialog_value = old_name;
            file_manager.focused_input = Some(FileManagerInput::DialogValue);
            file_manager.focused_dialog_footer_action = None;
            cx.notify();
        });
    }

    pub(super) fn open_file_manager_delete_dialog(&mut self, cx: &mut Context<Self>) {
        let files = self.selected_file_manager_names(cx);
        if files.is_empty() {
            return;
        }
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.dialog = Some(FileManagerDialog::Delete { files });
            // The delete confirm has no text input, so keyboard focus starts at
            // the same first footer action that a browser/Radix dialog exposes.
            file_manager.focused_dialog_footer_action = Some(ConfirmDialogAction::Cancel);
            file_manager.clear_context_menu_immediately();
            cx.notify();
        });
    }

    pub(super) fn open_file_manager_properties(
        &mut self,
        entry: LocalFileEntry,
        cx: &mut Context<Self>,
    ) {
        let details = local_file_properties(&entry);
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.properties_checksum = None;
            file_manager.properties_checksum_loading = false;
            file_manager.properties_checksum_task = None;
            file_manager.dialog = Some(FileManagerDialog::Properties { entry, details });
            file_manager.dialog_presence.reopen();
            file_manager.focused_dialog_footer_action = None;
            file_manager.clear_context_menu_immediately();
            cx.notify();
        });
    }

    pub(super) fn calculate_file_manager_properties_checksum(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.calculate_properties_checksum(cx);
        });
    }

    pub(super) fn open_file_manager_preview(
        &mut self,
        entry: LocalFileEntry,
        cx: &mut Context<Self>,
    ) {
        let audio_stop_error = self.file_manager.update(cx, |file_manager, cx| {
            file_manager.preview = Some(Arc::new(LocalPreview::Loading));
            file_manager.preview_plugin = None;
            file_manager.preview_editor = None;
            file_manager.preview_metadata = None;
            file_manager.preview_markdown_source = false;
            file_manager.preview_code_scroll = UniformListScrollHandle::new();
            file_manager.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
            file_manager.preview_markdown_cache = MarkdownPreviewCache::default();
            file_manager.preview_document_scroll = ScrollHandle::new();
            file_manager.preview_metadata_scroll = ScrollHandle::new();
            file_manager.preview_stream = FileManagerPreviewStreamState::default();
            file_manager.preview_font_family = None;
            file_manager.preview_font_error = None;
            file_manager.focused_input = None;
            (file_manager.preview_video_surface.detach)();
            cx.notify();
            file_manager
                .preview_audio
                .command(AudioPreviewCommand::Stop)
                .err()
        });
        self.request(FileManagerRequest::ClearIme, cx);
        if let Some(error) = audio_stop_error {
            self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            );
        }
        let audit = local_file_operation("file_preview", &entry.path);
        let preview = oxideterm_local_files::local_inspection_preview(
            &entry.path,
            oxideterm_preview::inspection_mime_type,
        )
        .unwrap_or_else(|| read_local_preview(&entry.path));
        audit.finish(
            match &preview {
                LocalPreview::Error(_) => oxideterm_audit::AuditOutcome::Failed,
                LocalPreview::TooLarge { .. } | LocalPreview::Unsupported(_) => {
                    oxideterm_audit::AuditOutcome::Unchanged
                }
                LocalPreview::Text { .. }
                | LocalPreview::Markdown { .. }
                | LocalPreview::Archive { .. } => oxideterm_audit::AuditOutcome::Succeeded,
                _ => oxideterm_audit::AuditOutcome::Sent,
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            None,
        );
        match &preview {
            LocalPreview::Document { path, mime_type } => {
                let view =
                    (self.bindings.document)(path, mime_type, self.tokens, self.i18n.clone(), cx);
                self.file_manager
                    .update(cx, |file_manager, _| file_manager.preview_plugin = view);
            }
            LocalPreview::Audio { path, .. } => {
                if let Err(error) = self.file_manager.update(cx, |file_manager, _cx| {
                    file_manager.preview_audio.load(std::path::Path::new(path))
                }) {
                    self.push_file_manager_toast(
                        self.i18n.t("fileManager.error"),
                        Some(error),
                        FileManagerNoticeKind::Error,
                        cx,
                    );
                }
            }
            LocalPreview::Font { path, .. } => match std::fs::read(path) {
                Ok(bytes) => {
                    let family = font_family_name_from_bytes(&bytes).or_else(|| {
                        std::path::Path::new(path)
                            .file_stem()
                            .and_then(|name| name.to_str())
                            .map(str::to_string)
                    });
                    match cx.text_system().add_fonts(vec![Cow::Owned(bytes)]) {
                        Ok(()) => {
                            self.file_manager.update(cx, |file_manager, cx| {
                                file_manager.preview_font_family = family;
                                cx.notify();
                            });
                        }
                        Err(error) => {
                            self.file_manager.update(cx, |file_manager, cx| {
                                file_manager.preview_font_error = Some(error.to_string());
                                cx.notify();
                            });
                        }
                    }
                }
                Err(error) => {
                    self.file_manager.update(cx, |file_manager, cx| {
                        file_manager.preview_font_error = Some(error.to_string());
                        cx.notify();
                    });
                }
            },
            LocalPreview::TextStream {
                path,
                size,
                language,
            } => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.preview_stream = FileManagerPreviewStreamState {
                        path: path.clone(),
                        size: *size,
                        language: language.clone(),
                        ..Default::default()
                    };
                    cx.notify();
                });
                self.load_more_file_manager_stream_preview(cx);
            }
            _ => {}
        }
        let preview_metadata = local_preview_metadata(&entry.path);
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.preview = Some(Arc::new(preview));
            file_manager.preview_metadata = preview_metadata;
            file_manager.preview_image_zoom = 1.0;
            file_manager.preview_image_rotation = 0;
            file_manager.dialog = Some(FileManagerDialog::Preview { entry });
            file_manager.dialog_presence.reopen();
            file_manager.focused_dialog_footer_action = None;
            file_manager.clear_context_menu_immediately();
            cx.notify();
        });
    }

    pub(super) fn load_more_file_manager_stream_preview(&mut self, cx: &mut Context<Self>) {
        let request = self.file_manager.update(cx, |file_manager, cx| {
            if file_manager.preview_stream.path.is_empty()
                || file_manager.preview_stream.loading
                || file_manager.preview_stream.eof
                || file_manager.preview_stream.error.is_some()
            {
                return None;
            }
            file_manager.preview_stream.loading = true;
            cx.notify();
            Some((
                file_manager.preview_stream.path.clone(),
                file_manager.preview_stream.loaded_bytes,
            ))
        });
        let Some((path, offset)) = request else {
            return;
        };
        let result =
            read_local_preview_range(&path, offset, FILE_MANAGER_PREVIEW_STREAM_CHUNK_SIZE);
        let audit = local_file_operation("file_preview", &format!("path={path}; offset={offset}"));
        audit.finish(
            if result.is_ok() {
                oxideterm_audit::AuditOutcome::Succeeded
            } else {
                oxideterm_audit::AuditOutcome::Failed
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            result.as_ref().ok().map(|chunk| chunk.data.len() as u64),
        );
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.preview_stream.loading = false;
            match result {
                Ok(chunk) => {
                    file_manager.preview_stream.loaded_bytes += chunk.data.len() as u64;
                    let stream = &mut file_manager.preview_stream;
                    append_local_preview_chunk(
                        &mut stream.lines,
                        &mut stream.carry_text,
                        &mut stream.carry_bytes,
                        chunk.data,
                        chunk.eof,
                    );
                    if chunk.eof
                        || file_manager.preview_stream.loaded_bytes
                            >= file_manager.preview_stream.size
                    {
                        file_manager.preview_stream.eof = true;
                    }
                }
                Err(error) => {
                    file_manager.preview_stream.error = Some(error);
                    file_manager.preview_stream.eof = true;
                }
            }
            cx.notify();
        });
    }

    pub(super) fn navigate_file_manager_preview(&mut self, delta: isize, cx: &mut Context<Self>) {
        let (entry, sorted_files) = {
            let file_manager = self.file_manager.read(cx);
            let Some(FileManagerDialog::Preview { entry }) = file_manager.dialog.as_ref() else {
                return;
            };
            (entry.clone(), file_manager.sorted_files())
        };
        let files = sorted_files
            .iter()
            .filter(|file| file.file_type != LocalFileType::Directory)
            .cloned()
            .collect::<Vec<_>>();
        if files.len() < 2 {
            return;
        }
        let index = files
            .iter()
            .position(|file| file.path == entry.path)
            .unwrap_or(0);
        let next = if delta < 0 {
            if index == 0 {
                files.len() - 1
            } else {
                index - 1
            }
        } else if index + 1 >= files.len() {
            0
        } else {
            index + 1
        };
        self.open_file_manager_preview(files[next].clone(), cx);
    }

    pub(super) fn toggle_file_manager_preview_audio(&mut self, cx: &mut Context<Self>) {
        if let Err(error) = self.file_manager.update(cx, |file_manager, _cx| {
            file_manager
                .preview_audio
                .command(AudioPreviewCommand::PlayPause)
        }) {
            self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn seek_file_manager_preview_audio(
        &mut self,
        position: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.file_manager.update(cx, |file_manager, _cx| {
            file_manager
                .preview_audio
                .command(AudioPreviewCommand::Seek(position))
        }) {
            self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            );
        }
        cx.notify();
    }

    pub(super) fn copy_file_manager_preview_content(&mut self, cx: &mut Context<Self>) {
        let Some(content) = self
            .file_manager
            .read(cx)
            .preview
            .as_deref()
            .and_then(|preview| match preview {
                LocalPreview::Text { content, .. } | LocalPreview::Markdown { content } => {
                    Some(content.clone())
                }
                _ => None,
            })
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(content));
        self.push_file_manager_toast(
            self.i18n.t("fileManager.copyContent"),
            None,
            FileManagerNoticeKind::Success,
            cx,
        );
        cx.notify();
    }

    pub(super) fn accept_file_manager_dialog(&mut self, cx: &mut Context<Self>) {
        if self.file_manager_dialog_primary_disabled(cx) {
            return;
        }
        let dialog = self.file_manager.read(cx).dialog.clone();
        match dialog {
            Some(FileManagerDialog::NewFolder) => self.create_file_manager_folder(cx),
            Some(FileManagerDialog::NewFile) => self.create_file_manager_file(cx),
            Some(FileManagerDialog::Rename { old_name }) => {
                self.rename_file_manager_entry(old_name, cx)
            }
            Some(FileManagerDialog::EditBookmark { id, .. }) => {
                self.update_file_manager_bookmark_name(id, cx)
            }
            Some(FileManagerDialog::Delete { files }) => {
                self.delete_file_manager_entries(files, cx)
            }
            _ => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.dialog = None;
                    file_manager.focused_input = None;
                    file_manager.focused_dialog_footer_action = None;
                    cx.notify();
                });
            }
        }
    }

    pub(super) fn file_manager_dialog_primary_disabled(&self, cx: &App) -> bool {
        let file_manager = self.file_manager.read(cx);
        match file_manager.dialog {
            Some(
                FileManagerDialog::NewFolder
                | FileManagerDialog::NewFile
                | FileManagerDialog::Rename { .. }
                | FileManagerDialog::EditBookmark { .. },
            ) => file_manager.dialog_value.trim().is_empty(),
            _ => false,
        }
    }

    fn handle_file_manager_dialog_footer_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            return false;
        }
        let Some(focused) = self.file_manager.read(cx).focused_dialog_footer_action else {
            return false;
        };
        match browser_behavior::modal_footer_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &FILE_MANAGER_DIALOG_FOOTER_ACTIONS,
            Some(focused),
            ConfirmDialogAction::Cancel,
        ) {
            Some(browser_behavior::ModalFooterKeyAction::Cancel) => {
                self.close_file_manager_dialog(cx);
                cx.notify();
                true
            }
            Some(browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.focused_dialog_footer_action = Some(action);
                    cx.notify();
                });
                true
            }
            Some(browser_behavior::ModalFooterKeyAction::Activate(action)) => {
                match action {
                    ConfirmDialogAction::Cancel => self.close_file_manager_dialog(cx),
                    ConfirmDialogAction::Confirm => self.accept_file_manager_dialog(cx),
                }
                cx.notify();
                true
            }
            None => false,
        }
    }

    pub(super) fn copy_file_manager_path_to_clipboard(
        &mut self,
        name_only: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.single_selected_file_manager_file(cx) else {
            return;
        };
        let value = if name_only { file.name } else { file.path };
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(super) fn browse_file_manager_folder(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(self.i18n.t("fileManager.browse"))),
        });
        let selection = async move {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return None;
            };
            paths.into_iter().next()
        };
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.start_folder_picker(selection, cx);
        });
    }

    pub(super) fn open_terminal_at_file_manager_path(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pointer(
            FileManagerPointer::OpenTerminal(self.file_manager.read(cx).path.clone()),
            window,
            cx,
        );
    }

    pub fn close_file_manager_dialog(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.close_dialog(cx);
        });
        self.request(FileManagerRequest::ClearIme, cx);
    }

    pub(super) fn begin_file_manager_rich_dialog_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.request(FileManagerRequest::ClearIme, cx);
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.begin_rich_dialog_exit(delay, cx)
        })
    }

    pub fn push_file_manager_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: FileManagerNoticeKind,
        cx: &mut App,
    ) {
        self.request(
            FileManagerRequest::Notice {
                title,
                description,
                kind: variant,
            },
            cx,
        );
    }
}
