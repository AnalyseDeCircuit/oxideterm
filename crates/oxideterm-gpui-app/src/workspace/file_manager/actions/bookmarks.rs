use super::*;

impl FileManagerState {
    fn save_bookmarks(&self) -> Result<(), String> {
        let bytes =
            serde_json::to_vec_pretty(&self.bookmarks).map_err(|error| error.to_string())?;
        oxideterm_atomic_file::durable_write(&self.bookmarks_path, &bytes)
            .map_err(|error| error.to_string())
    }

    fn reorder_bookmark(
        &mut self,
        source: &str,
        target: &str,
        after: bool,
    ) -> Result<bool, String> {
        let Some(from) = self
            .bookmarks
            .iter()
            .position(|bookmark| bookmark.id == source)
        else {
            return Ok(false);
        };
        let Some(target) = self
            .bookmarks
            .iter()
            .position(|bookmark| bookmark.id == target)
        else {
            return Ok(false);
        };
        if from == target {
            return Ok(false);
        }
        let slot = target + usize::from(after);
        let to = slot - usize::from(from < slot);
        if from == to {
            return Ok(false);
        }
        let previous = self.bookmarks.clone();
        let moved = self.bookmarks.remove(from);
        self.bookmarks.insert(to, moved);
        if let Err(error) = self.save_bookmarks() {
            self.bookmarks = previous;
            return Err(error);
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bookmarks() -> Vec<LocalBookmark> {
        [
            ("a", "/projects/a", 10),
            ("b", "/projects/b", 20),
            ("c", "/projects/c", 30),
        ]
        .into_iter()
        .map(|(id, path, created_at)| LocalBookmark {
            id: id.into(),
            name: "Projects".into(),
            path: path.into(),
            created_at,
        })
        .collect()
    }

    #[test]
    fn reordering_favorites_saves_ids_and_metadata_in_display_order() {
        let directory = tempfile::tempdir().unwrap();
        let mut state = FileManagerState {
            bookmarks: bookmarks(),
            bookmarks_path: directory.path().join(FILE_MANAGER_BOOKMARKS_FILENAME),
            ..FileManagerState::default()
        };
        for (source, target, after, expected) in [
            ("a", "b", true, ["b", "a", "c"]),
            ("c", "b", false, ["c", "b", "a"]),
        ] {
            assert!(state.reorder_bookmark(source, target, after).unwrap());
            let saved: Vec<LocalBookmark> =
                serde_json::from_slice(&std::fs::read(&state.bookmarks_path).unwrap()).unwrap();
            assert_eq!(
                saved
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
        let saved = std::fs::read_to_string(&state.bookmarks_path).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&saved).unwrap(),
            serde_json::json!([
                {"id": "c", "name": "Projects", "path": "/projects/c", "createdAt": 30},
                {"id": "b", "name": "Projects", "path": "/projects/b", "createdAt": 20},
                {"id": "a", "name": "Projects", "path": "/projects/a", "createdAt": 10},
            ])
        );
        for (source, target, after) in [("c", "b", false), ("a", "a", true), ("missing", "b", true)]
        {
            assert!(!state.reorder_bookmark(source, target, after).unwrap());
        }
        assert_eq!(
            std::fs::read_to_string(&state.bookmarks_path).unwrap(),
            saved
        );
    }

    #[test]
    fn failed_favorites_save_restores_the_previous_order() {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("blocked");
        std::fs::write(&blocked, "existing file").unwrap();
        let mut state = FileManagerState {
            bookmarks: bookmarks(),
            bookmarks_path: blocked.join(FILE_MANAGER_BOOKMARKS_FILENAME),
            ..FileManagerState::default()
        };
        assert!(state.reorder_bookmark("a", "c", true).is_err());
        assert_eq!(
            state
                .bookmarks
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["a", "b", "c"]
        );
        assert_eq!(std::fs::read_to_string(blocked).unwrap(), "existing file");
    }
}

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
                cx,
            );
        } else {
            self.push_file_manager_toast(
                self.i18n.t("fileManager.removeBookmark"),
                None,
                TerminalNoticeVariant::Default,
                cx,
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

    pub(in crate::workspace::file_manager) fn reorder_file_manager_bookmark(
        &mut self,
        source: &str,
        target: &str,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        let result = self.file_manager.update(cx, |file_manager, cx| {
            let result = file_manager.reorder_bookmark(source, target, after);
            if matches!(result, Ok(true)) {
                cx.notify();
            }
            result
        });
        match result {
            Ok(true) => {
                self.segmented_control_user_motion
                    .clear(selection_motion::FILE_MANAGER_NAVIGATION_ID);
                cx.notify();
            }
            Ok(false) => {}
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
                cx,
            ),
        }
    }

    fn persist_file_manager_bookmarks(&mut self, cx: &App) {
        match self.file_manager.read(cx).save_bookmarks() {
            Ok(()) => {}
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                TerminalNoticeVariant::Error,
                cx,
            ),
        }
    }
}
