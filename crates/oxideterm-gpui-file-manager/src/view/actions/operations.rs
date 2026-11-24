use super::*;

impl FileManagerView {
    pub(in crate::view) fn create_file_manager_folder(&mut self, cx: &mut Context<Self>) {
        let (name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        let result = self
            .file_manager
            .read(cx)
            .fs
            .create_folder(&current_path, &name);
        match result {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.folderCreated"),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        cx.notify();
    }

    pub(in crate::view) fn create_file_manager_file(&mut self, cx: &mut Context<Self>) {
        let (name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        let result = self
            .file_manager
            .read(cx)
            .fs
            .create_file(&current_path, &name);
        match result {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.fileCreated"),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        cx.notify();
    }

    pub(in crate::view) fn rename_file_manager_entry(
        &mut self,
        old_name: String,
        cx: &mut Context<Self>,
    ) {
        let (new_name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        let result = self
            .file_manager
            .read(cx)
            .fs
            .rename(&current_path, &old_name, &new_name);
        match result {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.renamed"),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        cx.notify();
    }

    pub(in crate::view) fn delete_file_manager_entries(
        &mut self,
        names: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let current_path = self.file_manager.read(cx).path.clone();
        let error = self.file_manager.read(cx).fs.delete(&current_path, &names);
        match error {
            Some(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
            None => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n
                        .t("fileManager.deletedCount")
                        .replace("{{count}}", &names.len().to_string()),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
        }
        cx.notify();
    }

    pub(in crate::view) fn copy_file_manager_selection(
        &mut self,
        cut: bool,
        cx: &mut Context<Self>,
    ) {
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
            FileManagerNoticeKind::Default,
            cx,
        );
    }

    pub(in crate::view) fn paste_file_manager_clipboard(&mut self, cx: &mut Context<Self>) {
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
        self.file_manager.update(cx, |state, cx| {
            state.start_operation(FileBatch::Paste { destination, mode }, sources, cx)
        });
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

    pub(in crate::view) fn duplicate_file_manager_selection(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_file_manager_entries(cx);
        if entries.is_empty() {
            return;
        }
        let paths = entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        self.file_manager.update(cx, |state, cx| {
            state.start_operation(FileBatch::Duplicate, paths, cx)
        });
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(in crate::view) fn queue_file_manager_external_drop_paths(
        &mut self,
        paths: &[std::path::PathBuf],
        cx: &mut Context<Self>,
    ) {
        let sources = self.file_manager.read(cx).fs.resolve_drop(paths);
        if sources.is_empty() {
            return;
        }
        let destination = self.file_manager.read(cx).path.clone();
        self.file_manager.update(cx, |state, cx| {
            state.start_operation(FileBatch::Drop { destination }, sources, cx)
        });
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(in crate::view) fn compress_file_manager_selection(&mut self, cx: &mut Context<Self>) {
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
        let archive_path = Path::new(&current_path).join(archive_name);
        let paths = entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let result = self
            .file_manager
            .read(cx)
            .fs
            .compress(&paths, &archive_path);
        match result {
            Ok(archive_path) => {
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.operationSuccess"),
                    Some(format!("{}", archive_path.display())),
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(in crate::view) fn extract_selected_file_manager_archive(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.single_selected_file_manager_file(cx) else {
            return;
        };
        if !can_extract_archive(&entry.name) {
            return;
        }
        let current_path = self.file_manager.read(cx).path.clone();
        let result = self
            .file_manager
            .read(cx)
            .fs
            .extract(&entry.path, &current_path);
        match result {
            Ok(()) => {
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.operationSuccess"),
                    Some(entry.name),
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        self.dismiss_file_manager_context_menu(cx);
    }
}
