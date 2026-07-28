use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::file_manager) fn is_file_manager_path_bookmarked(
        &self,
        path: &str,
        cx: &App,
    ) -> bool {
        self.file_manager
            .read(cx)
            .bookmarks
            .iter()
            .any(|bookmark| bookmark.path == path)
    }

    pub(in crate::workspace::file_manager) fn toggle_file_manager_current_bookmark(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let added = self.file_manager.update(cx, |file_manager, cx| {
            let path = file_manager.path.clone();
            let added = if let Some(index) = file_manager
                .bookmarks
                .iter()
                .position(|bookmark| bookmark.path == path)
            {
                file_manager.bookmarks.remove(index);
                false
            } else {
                file_manager.bookmarks.push(LocalBookmark {
                    id: new_file_manager_bookmark_id(),
                    name: bookmark_name_for_path(&path),
                    path,
                    created_at: now_ms(),
                });
                true
            };
            cx.notify();
            added
        });
        if added {
            self.push_file_manager_toast(
                self.i18n.t("fileManager.bookmarked"),
                None,
                TerminalNoticeVariant::Success,
            );
        } else {
            self.push_file_manager_toast(
                self.i18n.t("fileManager.removeBookmark"),
                None,
                TerminalNoticeVariant::Default,
            );
        }
        self.persist_file_manager_bookmarks(cx);
    }

    pub(in crate::workspace::file_manager) fn remove_file_manager_bookmark(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.bookmarks.retain(|bookmark| bookmark.id != id);
            cx.notify();
        });
        self.persist_file_manager_bookmarks(cx);
    }

    pub(in crate::workspace::file_manager) fn open_file_manager_edit_bookmark_dialog(
        &mut self,
        bookmark: LocalBookmark,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.dialog = Some(FileManagerDialog::EditBookmark {
                id: bookmark.id,
                path: bookmark.path,
            });
            file_manager.dialog_value = bookmark.name;
            file_manager.focused_input = Some(FileManagerInput::DialogValue);
            cx.notify();
        });
        self.ime_marked_text = None;
    }

    pub(super) fn update_file_manager_bookmark_name(&mut self, id: String, cx: &mut Context<Self>) {
        let name = self.file_manager.read(cx).dialog_value.trim().to_string();
        if name.is_empty() {
            return;
        }
        let changed = self.file_manager.update(cx, |file_manager, cx| {
            let changed = if let Some(bookmark) = file_manager
                .bookmarks
                .iter_mut()
                .find(|bookmark| bookmark.id == id)
            {
                bookmark.name = name;
                true
            } else {
                false
            };
            cx.notify();
            changed
        });
        if changed {
            self.persist_file_manager_bookmarks(cx);
        }
        self.close_file_manager_dialog(cx);
    }

    fn persist_file_manager_bookmarks(&mut self, cx: &App) {
        let (bookmarks_path, bookmarks) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.bookmarks_path.clone(),
                file_manager.bookmarks.clone(),
            )
        };
        if let Some(parent) = bookmarks_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match serde_json::to_vec_pretty(&bookmarks)
            .map_err(|error| error.to_string())
            .and_then(|bytes| {
                std::fs::write(&bookmarks_path, bytes).map_err(|error| error.to_string())
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
