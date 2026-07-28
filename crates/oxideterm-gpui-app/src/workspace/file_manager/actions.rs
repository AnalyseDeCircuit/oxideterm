use super::*;

mod bookmarks;
mod external;

pub(in crate::workspace::file_manager) use external::{open_path_external, reveal_path_external};

const FILE_MANAGER_DIALOG_FOOTER_ACTIONS: [ConfirmDialogAction; 2] =
    [ConfirmDialogAction::Cancel, ConfirmDialogAction::Confirm];

impl WorkspaceApp {
    pub(in crate::workspace) fn open_file_manager_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_file_manager_drives(cx);
        let initial_path = self.active_local_terminal_cwd_path(cx);
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::FileManager)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id(cx);
            self.tabs.push(Tab {
                id: tab_id,
                kind: TabKind::FileManager,
                title: self.i18n.t("fileManager.title"),
                title_source: TabTitleSource::I18nKey("fileManager.title"),
                root_pane: None,
                active_pane_id: None,
            });
            tab_id
        };
        if self.focus_detached_tab_window(tab_id, cx) {
            return;
        }
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = false;
        // Opening a workspace tab is independent from sidebar visibility, so
        // preserve the user's collapsed or expanded state across navigation.
        if let Some(path) = initial_path {
            // Opening File Manager from a local terminal should start where the
            // user is already working, without turning cwd into global state.
            self.set_file_manager_path(path, cx);
        } else {
            self.refresh_file_manager(cx);
        }
        self.persist_sidebar_settings();
        self.reveal_active_tab(window);
        cx.notify();
    }

    pub(in crate::workspace) fn open_file_manager_tab_at_path(
        &mut self,
        path: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file_manager_tab(window, cx);
        if !path.trim().is_empty() {
            // The cwd picker can browse away from the active terminal's
            // confirmed cwd before opening the surface, so apply that explicit
            // selection after the tab is active.
            self.set_file_manager_path(path, cx);
        }
        cx.notify();
    }

    pub(in crate::workspace) fn handle_file_manager_key(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let key = event.keystroke.key.as_str();
        if event.keystroke.modifiers.platform || event.keystroke.modifiers.control {
            match key {
                "a" => {
                    self.select_all_file_manager_files(cx);
                    return true;
                }
                "c" => {
                    self.copy_file_manager_selection(false, cx);
                    return true;
                }
                "x" => {
                    self.copy_file_manager_selection(true, cx);
                    return true;
                }
                "v" => {
                    self.paste_file_manager_clipboard(cx);
                    return true;
                }
                "l" => {
                    self.start_file_manager_path_edit(cx);
                    return true;
                }
                _ => return false,
            }
        }
        if key == "escape" && self.dismiss_workspace_context_menus(cx) {
            cx.notify();
            return true;
        }
        if self.handle_file_manager_dialog_input_footer_key(event, cx) {
            return true;
        }
        if let Some(input) = self.file_manager.read(cx).focused_input {
            // Inline inputs share the workspace IME/text-editing model so
            // selection, caret movement, and platform shortcuts stay browser-like.
            if self.handle_active_text_input_edit_shortcut(&event.keystroke, cx) {
                return true;
            }
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
                            self.ime_marked_text = None;
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
            match key {
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
        match key {
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
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.refresh_drives();
            file_manager.refresh();
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

    fn refresh_file_manager_drives(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.refresh_drives();
            cx.notify();
        });
    }

    pub(super) fn set_file_manager_path(&mut self, path: String, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.set_path(path);
            cx.notify();
        });
    }

    pub(in crate::workspace::file_manager) fn handle_file_manager_breadcrumb_scroll(
        &mut self,
        event: &ScrollWheelEvent,
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

    pub(super) fn open_file_manager_entry(
        &mut self,
        entry: LocalFileEntry,
        cx: &mut Context<Self>,
    ) {
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
        self.ime_marked_text = None;
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
        self.ime_marked_text = None;
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
                self.ime_marked_text = None;
            }
        }
        self.clear_ime_selection();
    }

    fn handle_file_manager_dialog_input_footer_key(
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

        match crate::workspace::browser_behavior::modal_footer_input_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &FILE_MANAGER_DIALOG_FOOTER_ACTIONS,
            input_available,
            focused_input == Some(FileManagerInput::DialogValue),
            focused_footer,
            ConfirmDialogAction::Cancel,
            Some(ConfirmDialogAction::Confirm),
        ) {
            Some(crate::workspace::browser_behavior::ModalFooterInputKeyAction::Cancel) => {
                self.close_file_manager_dialog(cx);
                cx.notify();
                true
            }
            Some(crate::workspace::browser_behavior::ModalFooterInputKeyAction::FocusInput) => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.focused_input = Some(FileManagerInput::DialogValue);
                    file_manager.focused_dialog_footer_action = None;
                    cx.notify();
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                true
            }
            Some(crate::workspace::browser_behavior::ModalFooterInputKeyAction::FocusFooter(
                action,
            )) => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.focused_input = None;
                    file_manager.focused_dialog_footer_action = Some(action);
                    cx.notify();
                });
                self.ime_marked_text = None;
                self.clear_ime_selection();
                cx.notify();
                true
            }
            Some(crate::workspace::browser_behavior::ModalFooterInputKeyAction::Activate(
                action,
            )) => {
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
            self.ime_marked_text = None;
        }
    }

    /// Refreshes path suggestions from one cached parent-directory listing.
    pub(in crate::workspace) fn refresh_file_manager_path_completion(
        &mut self,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::workspace) fn accept_file_manager_path_completion(
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
                let index = self.file_manager.read(cx).path_completion.selected_index();
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

    pub(super) fn select_file_manager_entry(
        &mut self,
        name: String,
        modifiers: gpui::Modifiers,
        visible_files: &[LocalFileEntry],
        cx: &mut Context<Self>,
    ) {
        self.blur_file_manager_inline_inputs(cx);
        self.file_manager.update(cx, |file_manager, cx| {
            if modifiers.shift {
                let anchor = file_manager
                    .last_selected
                    .clone()
                    .unwrap_or_else(|| name.clone());
                let start = visible_files
                    .iter()
                    .position(|file| file.name == anchor)
                    .unwrap_or(0);
                let end = visible_files
                    .iter()
                    .position(|file| file.name == name)
                    .unwrap_or(start);
                file_manager.selected.clear();
                for file in &visible_files[start.min(end)..=start.max(end)] {
                    file_manager.selected.insert(file.name.clone());
                }
            } else if modifiers.platform || modifiers.control {
                if !file_manager.selected.insert(name.clone()) {
                    file_manager.selected.remove(&name);
                }
                file_manager.last_selected = Some(name);
            } else {
                file_manager.selected.clear();
                file_manager.selected.insert(name.clone());
                file_manager.last_selected = Some(name);
            }
            cx.notify();
        });
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
                && crate::workspace::browser_behavior::preserve_or_move_context_selection(
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

    pub(in crate::workspace) fn dismiss_file_manager_context_menu(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        // Radix ContextMenu has one dismissal owner regardless of whether the
        // close came from outside click, Esc, or an item activation. Keep the
        // FileManager menu payload behind this helper so global browser
        // dismissal does not mutate feature state ad hoc.
        if self.file_manager.read(cx).context_menu.is_none() {
            return false;
        }
        self.clear_file_manager_context_menu_immediately(cx)
    }

    pub(in crate::workspace) fn clear_file_manager_context_menu_immediately(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
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
            file_manager.preview_metadata = None;
            file_manager.preview_markdown_source = false;
            file_manager.preview_code_scroll = UniformListScrollHandle::new();
            file_manager.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
            file_manager.preview_stream = FileManagerPreviewStreamState::default();
            file_manager.preview_font_family = None;
            file_manager.preview_font_error = None;
            file_manager.focused_input = None;
            file_manager.preview_video_surface.detach();
            cx.notify();
            file_manager
                .preview_audio
                .command(AudioPreviewCommand::Stop)
                .err()
        });
        self.ime_marked_text = None;
        if let Some(error) = audio_stop_error {
            self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
            );
        }
        let preview = read_local_preview(&entry.path);
        match &preview {
            LocalPreview::Audio { path, .. } => {
                if let Err(error) = self.file_manager.update(cx, |file_manager, _cx| {
                    file_manager.preview_audio.load(std::path::Path::new(path))
                }) {
                    self.push_file_manager_toast(
                        self.i18n.t("fileManager.error"),
                        Some(error),
                        TerminalNoticeVariant::Error,
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
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.preview_stream.loading = false;
            match result {
                Ok(chunk) => {
                    file_manager.preview_stream.loaded_bytes += chunk.data.len() as u64;
                    append_file_manager_stream_preview_chunk(
                        &mut file_manager.preview_stream,
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
                TerminalNoticeVariant::Error,
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
                TerminalNoticeVariant::Error,
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
            TerminalNoticeVariant::Success,
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
        match crate::workspace::browser_behavior::modal_footer_key_action(
            event.keystroke.key.as_str(),
            event.keystroke.modifiers.shift,
            &FILE_MANAGER_DIALOG_FOOTER_ACTIONS,
            Some(focused),
            ConfirmDialogAction::Cancel,
        ) {
            Some(crate::workspace::browser_behavior::ModalFooterKeyAction::Cancel) => {
                self.close_file_manager_dialog(cx);
                cx.notify();
                true
            }
            Some(crate::workspace::browser_behavior::ModalFooterKeyAction::Focus(action)) => {
                self.file_manager.update(cx, |file_manager, cx| {
                    file_manager.focused_dialog_footer_action = Some(action);
                    cx.notify();
                });
                true
            }
            Some(crate::workspace::browser_behavior::ModalFooterKeyAction::Activate(action)) => {
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

    pub(super) fn create_file_manager_folder(&mut self, cx: &mut Context<Self>) {
        let (name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        match validate_local_name(&name)
            .map(|_| join_local_path(&current_path, &name))
            .and_then(|path| std::fs::create_dir(&path).map_err(|error| error.to_string()))
        {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.folderCreated"),
                    None,
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
            ),
        }
        cx.notify();
    }

    pub(super) fn create_file_manager_file(&mut self, cx: &mut Context<Self>) {
        let (name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        match validate_local_name(&name)
            .map(|_| join_local_path(&current_path, &name))
            .and_then(|path| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }) {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.fileCreated"),
                    None,
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
            ),
        }
        cx.notify();
    }

    pub(super) fn rename_file_manager_entry(&mut self, old_name: String, cx: &mut Context<Self>) {
        let (new_name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        let result = validate_local_name(&new_name).and_then(|_| {
            let old_path = join_local_path(&current_path, &old_name);
            let new_path = join_local_path(&current_path, &new_name);
            std::fs::rename(old_path, new_path).map_err(|error| error.to_string())
        });
        match result {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.renamed"),
                    None,
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
            ),
        }
        cx.notify();
    }

    pub(super) fn delete_file_manager_entries(
        &mut self,
        names: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let current_path = self.file_manager.read(cx).path.clone();
        let mut error = None;
        for name in &names {
            let path = join_local_path(&current_path, name);
            let path_ref = std::path::Path::new(&path);
            let result = if path_ref.is_dir() {
                std::fs::remove_dir_all(path_ref)
            } else {
                std::fs::remove_file(path_ref)
            };
            if let Err(err) = result {
                error = Some(err.to_string());
                break;
            }
        }
        match error {
            Some(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
            ),
            None => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n
                        .t("fileManager.deletedCount")
                        .replace("{{count}}", &names.len().to_string()),
                    None,
                    TerminalNoticeVariant::Success,
                );
            }
        }
        cx.notify();
    }

    pub(super) fn copy_file_manager_selection(&mut self, cut: bool, cx: &mut Context<Self>) {
        let entries = self.selected_file_manager_entries(cx);
        if entries.is_empty() {
            return;
        }
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.clipboard = Some(LocalClipboard {
                mode: if cut {
                    LocalClipboardMode::Cut
                } else {
                    LocalClipboardMode::Copy
                },
                paths: entries.iter().map(|entry| entry.path.clone()).collect(),
                source_dir: file_manager.path.clone(),
            });
            file_manager.clear_context_menu_immediately();
            cx.notify();
        });
        let key = if cut {
            "fileManager.cutCount"
        } else {
            "fileManager.copiedCount"
        };
        self.push_file_manager_toast(
            self.i18n
                .t(key)
                .replace("{{count}}", &entries.len().to_string()),
            None,
            TerminalNoticeVariant::Default,
        );
    }

    fn start_file_manager_operation(
        &mut self,
        total: usize,
        work: impl FnOnce(
            delivery::ActiveDeliverySender<FileManagerOperationEvent>,
        ) -> Result<(), String>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.start_operation(total, work, cx);
        });
    }

    pub(super) fn paste_file_manager_clipboard(&mut self, cx: &mut Context<Self>) {
        let (clipboard, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (file_manager.clipboard.clone(), file_manager.path.clone())
        };
        let Some(clipboard) = clipboard else {
            return;
        };
        if clipboard.mode == LocalClipboardMode::Cut && clipboard.source_dir == current_path {
            self.dismiss_file_manager_context_menu(cx);
            return;
        }
        let destination = current_path;
        let sources = clipboard.paths.clone();
        let mode = clipboard.mode;
        let total = sources
            .iter()
            .map(|source| local_operation_unit_count(std::path::Path::new(source)))
            .sum::<usize>();
        self.start_file_manager_operation(
            total,
            move |tx| {
                let mut done = 0usize;
                for source in &sources {
                    let source_path = std::path::Path::new(source);
                    let Some(name) = source_path.file_name() else {
                        continue;
                    };
                    let target = unique_copy_path(&std::path::Path::new(&destination).join(name));
                    if mode == LocalClipboardMode::Cut
                        && would_move_directory_into_itself(source_path, &target)
                    {
                        return Err("cannot move a folder into itself".to_string());
                    }
                    let mut progress = |path: &std::path::Path| {
                        done += 1;
                        let file_name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let _ = tx.send(FileManagerOperationEvent::Progress(
                            FileManagerOperationProgress {
                                current: done,
                                total: total.max(1),
                                file_name,
                                active: true,
                            },
                        ));
                    };
                    if mode == LocalClipboardMode::Cut {
                        match std::fs::rename(source_path, &target) {
                            Ok(()) => {
                                progress(source_path);
                                Ok(())
                            }
                            Err(_) => {
                                copy_recursively_with_progress(source_path, &target, &mut progress)
                                    .map_err(|error| error.to_string())?;
                                if source_path.is_dir() {
                                    std::fs::remove_dir_all(source_path)
                                } else {
                                    std::fs::remove_file(source_path)
                                }
                                .map_err(|error| error.to_string())
                            }
                        }
                    } else {
                        copy_recursively_with_progress(source_path, &target, &mut progress)
                            .map_err(|error| error.to_string())
                    }?;
                }
                Ok(())
            },
            cx,
        );
        if clipboard.mode == LocalClipboardMode::Cut {
            self.file_manager.update(cx, |file_manager, cx| {
                file_manager.clipboard = None;
                file_manager.clear_context_menu_immediately();
                cx.notify();
            });
        } else {
            self.dismiss_file_manager_context_menu(cx);
        }
    }

    pub(super) fn duplicate_file_manager_selection(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_file_manager_entries(cx);
        if entries.is_empty() {
            return;
        }
        let paths = entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let total = paths
            .iter()
            .map(|path| local_operation_unit_count(std::path::Path::new(path)))
            .sum::<usize>();
        self.start_file_manager_operation(
            total,
            move |tx| {
                let mut done = 0usize;
                for path in paths {
                    let source = std::path::Path::new(&path);
                    let target = unique_copy_path(source);
                    let mut progress = |path: &std::path::Path| {
                        done += 1;
                        let file_name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let _ = tx.send(FileManagerOperationEvent::Progress(
                            FileManagerOperationProgress {
                                current: done,
                                total: total.max(1),
                                file_name,
                                active: true,
                            },
                        ));
                    };
                    copy_recursively_with_progress(source, &target, &mut progress)
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            },
            cx,
        );
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(super) fn queue_file_manager_external_drop_paths(
        &mut self,
        paths: &[std::path::PathBuf],
        cx: &mut Context<Self>,
    ) {
        let sources = paths
            .iter()
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return;
        }
        let destination = self.file_manager.read(cx).path.clone();
        let total = sources
            .iter()
            .map(|source| local_operation_unit_count(std::path::Path::new(source)))
            .sum::<usize>();
        self.start_file_manager_operation(
            total,
            move |tx| {
                let mut done = 0usize;
                for source in &sources {
                    let source_path = std::path::Path::new(source);
                    let Some(name) = source_path.file_name() else {
                        continue;
                    };
                    let target = unique_copy_path(&std::path::Path::new(&destination).join(name));
                    let mut progress = |path: &std::path::Path| {
                        done += 1;
                        let file_name = path
                            .file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_default();
                        let _ = tx.send(FileManagerOperationEvent::Progress(
                            FileManagerOperationProgress {
                                current: done,
                                total: total.max(1),
                                file_name,
                                active: true,
                            },
                        ));
                    };
                    copy_recursively_with_progress(source_path, &target, &mut progress)
                        .map_err(|error| error.to_string())?;
                }
                Ok(())
            },
            cx,
        );
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(super) fn compress_file_manager_selection(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_file_manager_entries(cx);
        if entries.is_empty() {
            return;
        }
        let archive_name = if entries.len() == 1 {
            format!("{}.zip", entries[0].name)
        } else {
            format!("Archive_{}.zip", chrono::Local::now().format("%Y-%m-%d"))
        };
        let current_path = self.file_manager.read(cx).path.clone();
        let archive_path =
            unique_copy_path(&std::path::Path::new(&current_path).join(archive_name));
        let paths = entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        match compress_local_files(&paths, &archive_path.to_string_lossy()) {
            Ok(()) => {
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.operationSuccess"),
                    Some(format!("{}", archive_path.display())),
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
            ),
        }
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(super) fn extract_selected_file_manager_archive(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.single_selected_file_manager_file(cx) else {
            return;
        };
        if !can_extract_archive(&entry.name) {
            return;
        }
        let current_path = self.file_manager.read(cx).path.clone();
        match extract_local_archive(&entry.path, &current_path) {
            Ok(()) => {
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.operationSuccess"),
                    Some(entry.name),
                    TerminalNoticeVariant::Success,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
            ),
        }
        self.dismiss_file_manager_context_menu(cx);
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
        let tab_id = self.alloc_tab_id(cx);
        let pane_id = self.alloc_pane_id(cx);
        let session_id = self.alloc_session_id(cx);
        let mut terminal_config = self.local_terminal_config();
        terminal_config.cwd = Some(PathBuf::from(self.file_manager.read(cx).path.clone()));
        let preferences =
            self.prepare_terminal_preferences_for_tab_kind(&TabKind::LocalTerminal, cx);
        let pane = cx.new(|cx| {
            TerminalPane::new_local_with_config_and_preferences(
                terminal_config,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize terminal pane")
        });
        self.register_terminal_pane(pane_id, session_id, pane.clone(), window, cx);
        self.refresh_native_plugin_terminal_hooks(cx);
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: self.local_terminal_tab_title(),
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.bind_terminal_location(tab_id, pane_id, session_id, cx);
        self.set_main_window_active_tab(Some(tab_id), cx);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.update(cx, |pane, cx| pane.focus(window, cx));
        self.reveal_active_tab(window);
        self.push_file_manager_toast(
            self.i18n.t("fileManager.terminalOpened"),
            None,
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(super) fn close_file_manager_dialog(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.close_dialog(cx);
        });
        self.ime_marked_text = None;
    }

    pub(super) fn begin_file_manager_rich_dialog_exit(&mut self, cx: &mut Context<Self>) -> bool {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Overlay,
        );
        self.ime_marked_text = None;
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.begin_rich_dialog_exit(delay, cx)
        })
    }

    pub(in crate::workspace) fn push_file_manager_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    ) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description,
            status_text: None,
            progress: None,
            variant,
        });
    }
}

fn append_file_manager_stream_preview_chunk(
    state: &mut FileManagerPreviewStreamState,
    data: Vec<u8>,
    eof: bool,
) {
    if data.is_empty() && !eof {
        return;
    }

    let mut bytes = std::mem::take(&mut state.carry_bytes);
    bytes.extend_from_slice(&data);
    let mut text = String::new();

    match std::str::from_utf8(&bytes) {
        Ok(valid) => text.push_str(valid),
        Err(error) => {
            let valid_up_to = error.valid_up_to();
            if valid_up_to > 0 {
                if let Ok(valid) = std::str::from_utf8(&bytes[..valid_up_to]) {
                    text.push_str(valid);
                }
            }
            let tail = &bytes[valid_up_to..];
            if eof {
                text.push_str(&String::from_utf8_lossy(tail));
            } else {
                state.carry_bytes.extend_from_slice(tail);
            }
        }
    }

    if eof && !state.carry_bytes.is_empty() {
        text.push_str(&String::from_utf8_lossy(&state.carry_bytes));
        state.carry_bytes.clear();
    }

    append_file_manager_stream_preview_text(state, &text, eof);
}

fn append_file_manager_stream_preview_text(
    state: &mut FileManagerPreviewStreamState,
    text: &str,
    eof: bool,
) {
    if text.is_empty() && !eof {
        return;
    }
    let combined = format!("{}{}", state.carry_text, text);
    let mut parts = combined.split('\n').map(str::to_string).collect::<Vec<_>>();

    if eof {
        state.carry_text.clear();
        state.lines.extend(parts);
    } else {
        state.carry_text = parts.pop().unwrap_or_default();
        state.lines.extend(parts);
    }
}
