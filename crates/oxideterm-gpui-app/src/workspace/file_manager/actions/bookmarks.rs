use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::file_manager) fn is_file_manager_path_bookmarked(
        &self,
        path: &str,
    ) -> bool {
        self.file_manager
            .bookmarks
            .iter()
            .any(|bookmark| bookmark.path == path)
    }

    pub(in crate::workspace::file_manager) fn toggle_file_manager_current_bookmark(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let path = self.file_manager.path.clone();
        if let Some(index) = self
            .file_manager
            .bookmarks
            .iter()
            .position(|bookmark| bookmark.path == path)
        {
            self.file_manager.bookmarks.remove(index);
            self.push_file_manager_toast(
                self.i18n.t("fileManager.removeBookmark"),
                None,
                TerminalNoticeVariant::Default,
            );
        } else {
            self.file_manager.bookmarks.push(LocalBookmark {
                id: new_file_manager_bookmark_id(),
                name: bookmark_name_for_path(&path),
                path,
                created_at: now_ms(),
            });
            self.push_file_manager_toast(
                self.i18n.t("fileManager.bookmarked"),
                None,
                TerminalNoticeVariant::Success,
            );
        }
        self.persist_file_manager_bookmarks();
        cx.notify();
    }

    pub(in crate::workspace::file_manager) fn remove_file_manager_bookmark(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        self.file_manager
            .bookmarks
            .retain(|bookmark| bookmark.id != id);
        self.persist_file_manager_bookmarks();
        cx.notify();
    }

    pub(in crate::workspace::file_manager) fn open_file_manager_edit_bookmark_dialog(
        &mut self,
        bookmark: LocalBookmark,
    ) {
        self.file_manager.dialog = Some(FileManagerDialog::EditBookmark {
            id: bookmark.id,
            path: bookmark.path,
        });
        self.file_manager.dialog_value = bookmark.name;
        self.file_manager.focused_input = Some(FileManagerInput::DialogValue);
        self.ime_marked_text = None;
    }

    pub(super) fn update_file_manager_bookmark_name(&mut self, id: String, cx: &mut Context<Self>) {
        let name = self.file_manager.dialog_value.trim().to_string();
        if name.is_empty() {
            return;
        }
        if let Some(bookmark) = self
            .file_manager
            .bookmarks
            .iter_mut()
            .find(|bookmark| bookmark.id == id)
        {
            bookmark.name = name;
            self.persist_file_manager_bookmarks();
        }
        self.close_file_manager_dialog();
        cx.notify();
    }

    fn persist_file_manager_bookmarks(&mut self) {
        if let Some(parent) = self.file_manager.bookmarks_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec_pretty(&self.file_manager.bookmarks)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                std::fs::write(&self.file_manager.bookmarks_path, bytes)
                    .map_err(|error| error.to_string())
            }) {
            Ok(()) => {}
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
            ),
        }
    }
}
