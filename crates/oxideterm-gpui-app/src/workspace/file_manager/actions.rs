use super::*;

mod bookmarks;
mod external;

pub(in crate::workspace::file_manager) use external::{open_path_external, reveal_path_external};

impl WorkspaceApp {
    pub(in crate::workspace) fn open_file_manager_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.kind == TabKind::FileManager)
        {
            tab.id
        } else {
            let tab_id = self.alloc_tab_id();
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
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.active_sidebar_section = SidebarSection::Files;
        self.needs_active_pane_focus = false;
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
        }
        self.refresh_file_manager();
        self.persist_sidebar_settings();
        self.reveal_active_tab(window);
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
                    self.select_all_file_manager_files();
                    cx.notify();
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
                    self.start_file_manager_path_edit();
                    cx.notify();
                    return true;
                }
                _ => return false,
            }
        }
        if self.file_manager.context_menu.is_some() && key == "escape" {
            self.file_manager.context_menu = None;
            cx.notify();
            return true;
        }
        if let Some(input) = self.file_manager.focused_input {
            match key {
                "escape" => {
                    match input {
                        FileManagerInput::Path => self.cancel_file_manager_path_edit(),
                        FileManagerInput::Filter => {
                            self.file_manager.focused_input = None;
                            self.ime_marked_text = None;
                        }
                        FileManagerInput::DialogValue => {
                            self.file_manager.focused_input = None;
                            self.file_manager.dialog = None;
                            self.file_manager.dialog_value.clear();
                            self.ime_marked_text = None;
                        }
                    }
                    cx.notify();
                    return true;
                }
                "enter" => {
                    match input {
                        FileManagerInput::Path => self.commit_file_manager_path_input(),
                        FileManagerInput::DialogValue => self.accept_file_manager_dialog(cx),
                        FileManagerInput::Filter => {}
                    }
                    cx.notify();
                    return true;
                }
                "backspace" => {
                    self.file_manager_input_value_mut(input).pop();
                    cx.notify();
                    return true;
                }
                _ => {}
            }
        }
        if matches!(
            self.file_manager.dialog,
            Some(FileManagerDialog::Preview { .. })
        ) {
            let is_video_preview =
                matches!(self.file_manager.preview, Some(LocalPreview::Video { .. }));
            match key {
                "escape" => {
                    self.close_file_manager_dialog();
                    cx.notify();
                    return true;
                }
                "space" | " " if !is_video_preview => {
                    self.close_file_manager_dialog();
                    cx.notify();
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
                    self.file_manager.preview_show_metadata =
                        !self.file_manager.preview_show_metadata;
                    cx.notify();
                    return true;
                }
                "u" => {
                    if matches!(
                        self.file_manager.preview,
                        Some(LocalPreview::Markdown { .. })
                    ) {
                        self.file_manager.preview_markdown_source =
                            !self.file_manager.preview_markdown_source;
                        cx.notify();
                        return true;
                    }
                }
                "+" | "=" => {
                    if matches!(self.file_manager.preview, Some(LocalPreview::Image { .. })) {
                        self.file_manager.preview_image_zoom =
                            (self.file_manager.preview_image_zoom + 0.25).min(4.0);
                        cx.notify();
                        return true;
                    }
                }
                "-" => {
                    if matches!(self.file_manager.preview, Some(LocalPreview::Image { .. })) {
                        self.file_manager.preview_image_zoom =
                            (self.file_manager.preview_image_zoom - 0.25).max(0.25);
                        cx.notify();
                        return true;
                    }
                }
                "0" => {
                    if matches!(self.file_manager.preview, Some(LocalPreview::Image { .. })) {
                        self.file_manager.preview_image_zoom = 1.0;
                        self.file_manager.preview_image_rotation = 0;
                        cx.notify();
                        return true;
                    }
                }
                "r" => {
                    if matches!(self.file_manager.preview, Some(LocalPreview::Image { .. })) {
                        self.file_manager.preview_image_rotation =
                            (self.file_manager.preview_image_rotation + 90) % 360;
                        cx.notify();
                        return true;
                    }
                }
                _ => {}
            }
        }
        match key {
            "escape" => {
                self.file_manager.context_menu = None;
                self.file_manager.dialog = None;
                self.file_manager.focused_input = None;
                cx.notify();
                true
            }
            "enter" => {
                if let Some(file) = self.single_selected_file_manager_file() {
                    self.open_file_manager_entry(file, cx);
                    cx.notify();
                    return true;
                }
                false
            }
            "space" | " " => {
                if let Some(file) = self.single_selected_file_manager_file()
                    && file.file_type != LocalFileType::Directory
                {
                    self.open_file_manager_preview(file, cx);
                    cx.notify();
                    return true;
                }
                false
            }
            "backspace" => {
                self.navigate_file_manager_parent();
                cx.notify();
                true
            }
            _ => false,
        }
    }

    pub(in crate::workspace) fn file_manager_input_value(&self, input: FileManagerInput) -> &str {
        match input {
            FileManagerInput::Path => &self.file_manager.path_input,
            FileManagerInput::Filter => &self.file_manager.filter,
            FileManagerInput::DialogValue => &self.file_manager.dialog_value,
        }
    }

    pub(in crate::workspace) fn file_manager_input_value_mut(
        &mut self,
        input: FileManagerInput,
    ) -> &mut String {
        match input {
            FileManagerInput::Path => &mut self.file_manager.path_input,
            FileManagerInput::Filter => &mut self.file_manager.filter,
            FileManagerInput::DialogValue => &mut self.file_manager.dialog_value,
        }
    }

    pub(super) fn refresh_file_manager(&mut self) {
        self.file_manager.loading = true;
        match list_local_files(&self.file_manager.path) {
            Ok(files) => {
                self.file_manager.files = files;
                self.file_manager.error = None;
                self.prune_file_manager_selection();
            }
            Err(error) => {
                self.file_manager.files.clear();
                self.file_manager.error = Some(error.to_string());
            }
        }
        self.file_manager.loading = false;
    }

    pub(super) fn set_file_manager_path(&mut self, path: String) {
        let normalized = normalize_local_path(&path);
        self.file_manager.path = normalized.clone();
        self.file_manager.path_input = normalized;
        self.file_manager.editing_path = false;
        self.file_manager.focused_input = None;
        self.file_manager.selected.clear();
        self.file_manager.last_selected = None;
        self.file_manager.context_menu = None;
        self.file_manager.list_scroll = UniformListScrollHandle::new();
        self.refresh_file_manager();
    }

    pub(super) fn commit_file_manager_path_input(&mut self) {
        let path = self.file_manager.path_input.trim().to_string();
        if path.is_empty() {
            return;
        }
        self.set_file_manager_path(path);
    }

    pub(super) fn navigate_file_manager_parent(&mut self) {
        if let Some(parent) = local_parent_path(&self.file_manager.path) {
            self.set_file_manager_path(parent);
        } else {
            self.file_manager.dialog = Some(FileManagerDialog::Drives);
        }
    }

    pub(super) fn open_file_manager_entry(
        &mut self,
        entry: LocalFileEntry,
        cx: &mut Context<Self>,
    ) {
        match entry.file_type {
            LocalFileType::Directory => self.set_file_manager_path(entry.path),
            LocalFileType::File | LocalFileType::Symlink => {
                if let Err(error) = open_path_external(&entry.path) {
                    self.push_file_manager_toast(
                        self.i18n.t("fileManager.error"),
                        Some(error),
                        TerminalNoticeVariant::Error,
                    );
                    cx.notify();
                }
            }
        }
    }

    pub(super) fn start_file_manager_path_edit(&mut self) {
        self.file_manager.path_input = self.file_manager.path.clone();
        self.file_manager.editing_path = true;
        self.file_manager.focused_input = Some(FileManagerInput::Path);
        self.ime_marked_text = None;
    }

    pub(super) fn cancel_file_manager_path_edit(&mut self) {
        self.file_manager.path_input = self.file_manager.path.clone();
        if self.file_manager.focused_input == Some(FileManagerInput::Path) {
            self.file_manager.focused_input = None;
        }
        self.file_manager.editing_path = false;
        self.ime_marked_text = None;
    }

    pub(super) fn blur_file_manager_inline_inputs(&mut self) {
        if self.file_manager.editing_path
            || self.file_manager.focused_input == Some(FileManagerInput::Path)
        {
            self.cancel_file_manager_path_edit();
        } else if self.file_manager.focused_input == Some(FileManagerInput::Filter) {
            self.file_manager.focused_input = None;
            self.ime_marked_text = None;
        }
    }

    pub(super) fn select_file_manager_entry(
        &mut self,
        name: String,
        modifiers: gpui::Modifiers,
        visible_files: &[LocalFileEntry],
    ) {
        self.blur_file_manager_inline_inputs();
        if modifiers.shift {
            let anchor = self
                .file_manager
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
            self.file_manager.selected.clear();
            for file in &visible_files[start.min(end)..=start.max(end)] {
                self.file_manager.selected.insert(file.name.clone());
            }
        } else if modifiers.platform || modifiers.control {
            if !self.file_manager.selected.insert(name.clone()) {
                self.file_manager.selected.remove(&name);
            }
            self.file_manager.last_selected = Some(name);
        } else {
            self.file_manager.selected.clear();
            self.file_manager.selected.insert(name.clone());
            self.file_manager.last_selected = Some(name);
        }
    }

    pub(super) fn open_file_manager_context_menu(
        &mut self,
        file: Option<LocalFileEntry>,
        x: f32,
        y: f32,
    ) {
        self.blur_file_manager_inline_inputs();
        if let Some(file) = file.as_ref()
            && !self.file_manager.selected.contains(&file.name)
        {
            self.file_manager.selected.clear();
            self.file_manager.selected.insert(file.name.clone());
            self.file_manager.last_selected = Some(file.name.clone());
        }
        self.file_manager.context_menu = Some(FileManagerContextMenu { file, x, y });
    }

    pub(super) fn clear_file_manager_selection(&mut self) {
        self.file_manager.selected.clear();
        self.file_manager.last_selected = None;
    }

    pub(super) fn select_all_file_manager_files(&mut self) {
        let files = sorted_local_files(
            &self.file_manager.files,
            &self.file_manager.filter,
            self.file_manager.sort_field,
            self.file_manager.sort_direction,
        );
        self.file_manager.selected = files.into_iter().map(|file| file.name).collect();
        self.file_manager.last_selected = self.file_manager.selected.iter().next().cloned();
    }

    pub(super) fn selected_file_manager_names(&self) -> Vec<String> {
        self.file_manager.selected.iter().cloned().collect()
    }

    pub(super) fn selected_file_manager_entries(&self) -> Vec<LocalFileEntry> {
        self.file_manager
            .files
            .iter()
            .filter(|file| self.file_manager.selected.contains(&file.name))
            .cloned()
            .collect()
    }

    pub(super) fn single_selected_file_manager_file(&self) -> Option<LocalFileEntry> {
        if self.file_manager.selected.len() != 1 {
            return None;
        }
        let name = self.file_manager.selected.iter().next()?;
        self.file_manager
            .files
            .iter()
            .find(|file| &file.name == name)
            .cloned()
    }

    fn prune_file_manager_selection(&mut self) {
        let names = self
            .file_manager
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect::<HashSet<_>>();
        self.file_manager
            .selected
            .retain(|name| names.contains(name.as_str()));
        if self
            .file_manager
            .last_selected
            .as_ref()
            .is_some_and(|name| !names.contains(name.as_str()))
        {
            self.file_manager.last_selected = None;
        }
    }

    pub(super) fn toggle_file_manager_sort(&mut self, field: LocalSortField) {
        self.blur_file_manager_inline_inputs();
        if self.file_manager.sort_field == field {
            self.file_manager.sort_direction = match self.file_manager.sort_direction {
                LocalSortDirection::Asc => LocalSortDirection::Desc,
                LocalSortDirection::Desc => LocalSortDirection::Asc,
            };
        } else {
            self.file_manager.sort_field = field;
            self.file_manager.sort_direction = LocalSortDirection::Asc;
        }
    }

    pub(super) fn open_file_manager_new_folder_dialog(&mut self) {
        self.file_manager.dialog = Some(FileManagerDialog::NewFolder);
        self.file_manager.dialog_value.clear();
        self.file_manager.focused_input = Some(FileManagerInput::DialogValue);
    }

    pub(super) fn open_file_manager_new_file_dialog(&mut self) {
        self.file_manager.dialog = Some(FileManagerDialog::NewFile);
        self.file_manager.dialog_value.clear();
        self.file_manager.focused_input = Some(FileManagerInput::DialogValue);
    }

    pub(super) fn open_file_manager_rename_dialog(&mut self, old_name: String) {
        self.file_manager.dialog = Some(FileManagerDialog::Rename {
            old_name: old_name.clone(),
        });
        self.file_manager.dialog_value = old_name;
        self.file_manager.focused_input = Some(FileManagerInput::DialogValue);
    }

    pub(super) fn open_file_manager_delete_dialog(&mut self) {
        let files = self.selected_file_manager_names();
        if files.is_empty() {
            return;
        }
        self.file_manager.dialog = Some(FileManagerDialog::Delete { files });
        self.file_manager.context_menu = None;
    }

    pub(super) fn open_file_manager_properties(&mut self, entry: LocalFileEntry) {
        let details = local_file_properties(&entry);
        self.file_manager.properties_checksum = None;
        self.file_manager.properties_checksum_loading = false;
        self.file_manager.properties_checksum_rx = None;
        self.file_manager.properties_checksum_poll_active = false;
        self.file_manager.dialog = Some(FileManagerDialog::Properties { entry, details });
        self.file_manager.context_menu = None;
    }

    pub(super) fn calculate_file_manager_properties_checksum(&mut self, cx: &mut Context<Self>) {
        if self.file_manager.properties_checksum_loading {
            return;
        }
        let Some(FileManagerDialog::Properties { entry, .. }) = self.file_manager.dialog.clone()
        else {
            return;
        };
        if entry.file_type != LocalFileType::File {
            return;
        }
        let path = entry.path.clone();
        let (tx, rx) = std::sync::mpsc::channel();
        self.file_manager.properties_checksum = None;
        self.file_manager.properties_checksum_loading = true;
        self.file_manager.properties_checksum_rx = Some(rx);
        std::thread::spawn(move || {
            let _ = tx.send(calculate_local_checksum(&path));
        });
        self.schedule_file_manager_checksum_poll(cx);
        cx.notify();
    }

    fn schedule_file_manager_checksum_poll(&mut self, cx: &mut Context<Self>) {
        if self.file_manager.properties_checksum_poll_active {
            return;
        }
        self.file_manager.properties_checksum_poll_active = true;
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(80))
                    .await;
                let keep_polling = weak
                    .update(cx, |this, cx| {
                        let result = this
                            .file_manager
                            .properties_checksum_rx
                            .as_ref()
                            .and_then(|rx| rx.try_recv().ok());
                        let Some(result) = result else {
                            cx.notify();
                            return true;
                        };
                        this.file_manager.properties_checksum_rx = None;
                        this.file_manager.properties_checksum_loading = false;
                        this.file_manager.properties_checksum_poll_active = false;
                        match result {
                            Ok(checksum) => {
                                this.file_manager.properties_checksum = Some(checksum);
                            }
                            Err(error) => this.push_file_manager_toast(
                                this.i18n.t("fileManager.error"),
                                Some(error),
                                TerminalNoticeVariant::Error,
                            ),
                        }
                        cx.notify();
                        false
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn open_file_manager_preview(
        &mut self,
        entry: LocalFileEntry,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.preview = Some(LocalPreview::Loading);
        self.file_manager.preview_metadata = None;
        self.file_manager.preview_markdown_source = false;
        self.file_manager.preview_code_scroll = UniformListScrollHandle::new();
        self.file_manager.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
        self.file_manager.preview_stream = FileManagerPreviewStreamState::default();
        self.file_manager.preview_font_family = None;
        self.file_manager.preview_font_error = None;
        self.file_manager.focused_input = None;
        self.ime_marked_text = None;
        let _ = self
            .file_manager
            .preview_audio
            .command(AudioPreviewCommand::Stop);
        self.file_manager.preview_video_surface.detach();
        let preview = read_local_preview(&entry.path);
        match &preview {
            LocalPreview::Audio { path, .. } => {
                if let Err(error) = self
                    .file_manager
                    .preview_audio
                    .load(std::path::Path::new(path))
                {
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
                        Ok(()) => self.file_manager.preview_font_family = family,
                        Err(error) => {
                            self.file_manager.preview_font_error = Some(error.to_string());
                        }
                    }
                }
                Err(error) => self.file_manager.preview_font_error = Some(error.to_string()),
            },
            LocalPreview::TextStream {
                path,
                size,
                language,
            } => {
                self.file_manager.preview_stream = FileManagerPreviewStreamState {
                    path: path.clone(),
                    size: *size,
                    language: language.clone(),
                    ..Default::default()
                };
                self.load_more_file_manager_stream_preview(cx);
            }
            _ => {}
        }
        self.file_manager.preview = Some(preview);
        self.file_manager.preview_metadata = local_preview_metadata(&entry.path);
        self.file_manager.preview_image_zoom = 1.0;
        self.file_manager.preview_image_rotation = 0;
        self.file_manager.dialog = Some(FileManagerDialog::Preview { entry });
        self.file_manager.context_menu = None;
    }

    pub(super) fn load_more_file_manager_stream_preview(&mut self, cx: &mut Context<Self>) {
        if self.file_manager.preview_stream.path.is_empty() {
            return;
        }
        if self.file_manager.preview_stream.loading
            || self.file_manager.preview_stream.eof
            || self.file_manager.preview_stream.error.is_some()
        {
            return;
        }

        self.file_manager.preview_stream.loading = true;
        let path = self.file_manager.preview_stream.path.clone();
        let offset = self.file_manager.preview_stream.loaded_bytes;
        let result =
            read_local_preview_range(&path, offset, FILE_MANAGER_PREVIEW_STREAM_CHUNK_SIZE);
        self.file_manager.preview_stream.loading = false;

        match result {
            Ok(chunk) => {
                self.file_manager.preview_stream.loaded_bytes += chunk.data.len() as u64;
                append_file_manager_stream_preview_chunk(
                    &mut self.file_manager.preview_stream,
                    chunk.data,
                    chunk.eof,
                );
                if chunk.eof
                    || self.file_manager.preview_stream.loaded_bytes
                        >= self.file_manager.preview_stream.size
                {
                    self.file_manager.preview_stream.eof = true;
                }
            }
            Err(error) => {
                self.file_manager.preview_stream.error = Some(error);
                self.file_manager.preview_stream.eof = true;
            }
        }
        cx.notify();
    }

    pub(super) fn navigate_file_manager_preview(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(FileManagerDialog::Preview { entry }) = self.file_manager.dialog.clone() else {
            return;
        };
        let files = sorted_local_files(
            &self.file_manager.files,
            &self.file_manager.filter,
            self.file_manager.sort_field,
            self.file_manager.sort_direction,
        )
        .into_iter()
        .filter(|file| file.file_type != LocalFileType::Directory)
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
        if let Err(error) = self
            .file_manager
            .preview_audio
            .command(AudioPreviewCommand::PlayPause)
        {
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
        if let Err(error) = self
            .file_manager
            .preview_audio
            .command(AudioPreviewCommand::Seek(position))
        {
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
            .preview
            .as_ref()
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
        match self.file_manager.dialog.clone() {
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
                self.file_manager.dialog = None;
                self.file_manager.focused_input = None;
            }
        }
    }

    pub(super) fn create_file_manager_folder(&mut self, cx: &mut Context<Self>) {
        let name = self.file_manager.dialog_value.trim().to_string();
        match validate_local_name(&name)
            .map(|_| join_local_path(&self.file_manager.path, &name))
            .and_then(|path| std::fs::create_dir(&path).map_err(|error| error.to_string()))
        {
            Ok(()) => {
                self.close_file_manager_dialog();
                self.refresh_file_manager();
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
        let name = self.file_manager.dialog_value.trim().to_string();
        match validate_local_name(&name)
            .map(|_| join_local_path(&self.file_manager.path, &name))
            .and_then(|path| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }) {
            Ok(()) => {
                self.close_file_manager_dialog();
                self.refresh_file_manager();
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
        let new_name = self.file_manager.dialog_value.trim().to_string();
        let result = validate_local_name(&new_name).and_then(|_| {
            let old_path = join_local_path(&self.file_manager.path, &old_name);
            let new_path = join_local_path(&self.file_manager.path, &new_name);
            std::fs::rename(old_path, new_path).map_err(|error| error.to_string())
        });
        match result {
            Ok(()) => {
                self.close_file_manager_dialog();
                self.refresh_file_manager();
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
        let mut error = None;
        for name in &names {
            let path = join_local_path(&self.file_manager.path, name);
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
                self.close_file_manager_dialog();
                self.refresh_file_manager();
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
        let entries = self.selected_file_manager_entries();
        if entries.is_empty() {
            return;
        }
        self.file_manager.clipboard = Some(LocalClipboard {
            mode: if cut {
                LocalClipboardMode::Cut
            } else {
                LocalClipboardMode::Copy
            },
            paths: entries.iter().map(|entry| entry.path.clone()).collect(),
            source_dir: self.file_manager.path.clone(),
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
        self.file_manager.context_menu = None;
        cx.notify();
    }

    fn start_file_manager_operation(
        &mut self,
        total: usize,
        work: impl FnOnce(std::sync::mpsc::Sender<FileManagerOperationEvent>) -> Result<(), String>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.file_manager.operation_progress = Some(FileManagerOperationProgress {
            current: 0,
            total: total.max(1),
            file_name: String::new(),
            active: true,
        });
        self.file_manager.operation_rx = Some(rx);
        std::thread::spawn(move || {
            let result = work(tx.clone());
            let _ = tx.send(FileManagerOperationEvent::Finished(result));
        });
        self.schedule_file_manager_operation_poll(cx);
    }

    fn schedule_file_manager_operation_poll(&mut self, cx: &mut Context<Self>) {
        if self.file_manager.operation_poll_active {
            return;
        }
        self.file_manager.operation_poll_active = true;
        cx.spawn(async move |weak, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(80))
                    .await;
                let keep_polling = weak
                    .update(cx, |this, cx| {
                        let mut finished = None;
                        if let Some(rx) = this.file_manager.operation_rx.as_ref() {
                            while let Ok(event) = rx.try_recv() {
                                match event {
                                    FileManagerOperationEvent::Progress(progress) => {
                                        this.file_manager.operation_progress = Some(progress);
                                    }
                                    FileManagerOperationEvent::Finished(result) => {
                                        finished = Some(result);
                                    }
                                }
                            }
                        }
                        if let Some(result) = finished {
                            this.file_manager.operation_rx = None;
                            this.file_manager.operation_poll_active = false;
                            if let Some(progress) = this.file_manager.operation_progress.as_mut() {
                                progress.active = false;
                                progress.current = progress.total;
                                progress.file_name.clear();
                            }
                            match result {
                                Ok(()) => {
                                    this.refresh_file_manager();
                                    this.push_file_manager_toast(
                                        this.i18n.t("fileManager.operationSuccess"),
                                        None,
                                        TerminalNoticeVariant::Success,
                                    );
                                }
                                Err(error) => this.push_file_manager_toast(
                                    this.i18n.t("fileManager.error"),
                                    Some(error),
                                    TerminalNoticeVariant::Error,
                                ),
                            }
                            cx.notify();
                            false
                        } else {
                            cx.notify();
                            true
                        }
                    })
                    .unwrap_or(false);
                if !keep_polling {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn paste_file_manager_clipboard(&mut self, cx: &mut Context<Self>) {
        let Some(clipboard) = self.file_manager.clipboard.clone() else {
            return;
        };
        if clipboard.mode == LocalClipboardMode::Cut
            && clipboard.source_dir == self.file_manager.path
        {
            self.file_manager.context_menu = None;
            return;
        }
        let destination = self.file_manager.path.clone();
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
            self.file_manager.clipboard = None;
        }
        self.file_manager.context_menu = None;
        cx.notify();
    }

    pub(super) fn duplicate_file_manager_selection(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_file_manager_entries();
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
        self.file_manager.context_menu = None;
        cx.notify();
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
        let destination = self.file_manager.path.clone();
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
        self.file_manager.context_menu = None;
        cx.notify();
    }

    pub(super) fn compress_file_manager_selection(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_file_manager_entries();
        if entries.is_empty() {
            return;
        }
        let archive_name = if entries.len() == 1 {
            format!("{}.zip", entries[0].name)
        } else {
            format!("Archive_{}.zip", chrono::Local::now().format("%Y-%m-%d"))
        };
        let archive_path =
            unique_copy_path(&std::path::Path::new(&self.file_manager.path).join(archive_name));
        let paths = entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        match compress_local_files(&paths, &archive_path.to_string_lossy()) {
            Ok(()) => {
                self.refresh_file_manager();
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
        self.file_manager.context_menu = None;
        cx.notify();
    }

    pub(super) fn extract_selected_file_manager_archive(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.single_selected_file_manager_file() else {
            return;
        };
        if !can_extract_archive(&entry.name) {
            return;
        }
        match extract_local_archive(&entry.path, &self.file_manager.path) {
            Ok(()) => {
                self.refresh_file_manager();
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
        self.file_manager.context_menu = None;
        cx.notify();
    }

    pub(super) fn copy_file_manager_path_to_clipboard(
        &mut self,
        name_only: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(file) = self.single_selected_file_manager_file() else {
            return;
        };
        let value = if name_only { file.name } else { file.path };
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.file_manager.context_menu = None;
        cx.notify();
    }

    pub(super) fn browse_file_manager_folder(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(SharedString::from(self.i18n.t("fileManager.browse"))),
        });
        cx.spawn(async move |weak, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let path = path.to_string_lossy().to_string();
            let _ = weak.update(cx, |this, cx| {
                this.set_file_manager_path(path);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_terminal_at_file_manager_path(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = self.alloc_tab_id();
        let pane_id = self.alloc_pane_id();
        let session_id = self.alloc_session_id();
        let mut terminal_config = self.local_terminal_config();
        terminal_config.cwd = Some(PathBuf::from(self.file_manager.path.clone()));
        let preferences = self.terminal_preferences_for_tab_kind(&TabKind::LocalTerminal);
        let pane = cx.new(|cx| {
            TerminalPane::new_local_with_config_and_preferences(
                terminal_config,
                preferences,
                window,
                cx,
            )
            .expect("failed to initialize terminal pane")
        });
        self.panes.insert(pane_id, pane.clone());
        self.tabs.push(Tab {
            id: tab_id,
            kind: TabKind::LocalTerminal,
            title: self.local_terminal_tab_title(),
            title_source: TabTitleSource::Static,
            root_pane: Some(PaneNode::leaf(pane_id, session_id)),
            active_pane_id: Some(pane_id),
        });
        self.active_tab_id = Some(tab_id);
        self.active_surface = ActiveSurface::Terminal;
        self.needs_active_pane_focus = true;
        pane.read(cx).focus(window);
        self.reveal_active_tab(window);
        self.push_file_manager_toast(
            self.i18n.t("fileManager.terminalOpened"),
            None,
            TerminalNoticeVariant::Success,
        );
        cx.notify();
    }

    pub(super) fn close_file_manager_dialog(&mut self) {
        let _ = self
            .file_manager
            .preview_audio
            .command(AudioPreviewCommand::Stop);
        self.file_manager.preview_video_surface.detach();
        self.file_manager.dialog = None;
        self.file_manager.focused_input = None;
        self.file_manager.dialog_value.clear();
        self.file_manager.preview = None;
        self.file_manager.preview_metadata = None;
        self.file_manager.preview_markdown_source = false;
        self.file_manager.preview_code_scroll = UniformListScrollHandle::new();
        self.file_manager.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
        self.file_manager.preview_stream = FileManagerPreviewStreamState::default();
        self.file_manager.properties_checksum = None;
        self.file_manager.properties_checksum_loading = false;
        self.file_manager.properties_checksum_rx = None;
        self.file_manager.properties_checksum_poll_active = false;
        self.ime_marked_text = None;
    }

    pub(super) fn push_file_manager_toast(
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
