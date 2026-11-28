use super::*;

#[derive(Default)]
pub(in crate::view) struct BookmarkEdits {
    task: Option<Task<()>>,
    queue: std::collections::VecDeque<Edit>,
}

enum Edit {
    Toggle(String),
    Remove(String),
    Rename {
        id: String,
        name: String,
    },
    Reorder {
        source: String,
        target: String,
        after: bool,
    },
}

#[derive(Clone, Debug)]
pub(in crate::view) enum BookmarkChange {
    Toggled(bool),
    Removed,
    Renamed { id: String, name: String },
    Reordered,
}

impl Edit {
    fn apply(self, current: &[LocalBookmark]) -> Option<(Vec<LocalBookmark>, BookmarkChange)> {
        let mut next = current.to_vec();
        let effect = match self {
            Self::Toggle(path) => {
                if let Some(index) = next.iter().position(|b| b.path == path) {
                    next.remove(index);
                    BookmarkChange::Toggled(false)
                } else {
                    next.push(LocalBookmark {
                        id: new_file_manager_bookmark_id(),
                        name: bookmark_name_for_path(&path),
                        path,
                        created_at: now_ms(),
                    });
                    BookmarkChange::Toggled(true)
                }
            }
            Self::Remove(id) => {
                next.remove(next.iter().position(|b| b.id == id)?);
                BookmarkChange::Removed
            }
            Self::Rename { id, name } => {
                next.iter_mut().find(|b| b.id == id)?.name.clone_from(&name);
                BookmarkChange::Renamed { id, name }
            }
            Self::Reorder {
                source,
                target,
                after,
            } => {
                next =
                    oxideterm_local_files::reordered_bookmarks(current, &source, &target, after)?;
                BookmarkChange::Reordered
            }
        };
        Some((next, effect))
    }
}

impl FileManagerState {
    pub(in crate::view) fn read_bookmarks(&mut self, cx: &mut Context<Self>) {
        let work = self.fs.load_bookmarks(&self.bookmarks_path);
        let task = work.deliver(self, cx, |s, result, cx| {
            s.bookmark_edits.task = None;
            match result {
                Ok(items) => {
                    s.bookmarks = items;
                    s.next_bookmark_edit(cx);
                }
                Err(error) => {
                    s.bookmark_edits.queue.clear();
                    cx.emit(FileManagerWorkspaceEvent::Error(error));
                }
            }
            cx.notify();
        });
        if let Some(task) = task {
            self.bookmark_edits.task = Some(task);
        }
    }

    fn edit_bookmarks(&mut self, edit: Edit, cx: &mut Context<Self>) {
        self.bookmark_edits.queue.push_back(edit);
        if self.bookmark_edits.task.is_none() {
            self.next_bookmark_edit(cx);
        }
    }

    fn next_bookmark_edit(&mut self, cx: &mut Context<Self>) {
        while let Some(edit) = self.bookmark_edits.queue.pop_front() {
            let Some((next, effect)) = edit.apply(&self.bookmarks) else {
                continue;
            };
            let work = self
                .fs
                .save_bookmarks(&self.bookmarks_path, &self.bookmarks, &next);
            let task = work.deliver(self, cx, move |s, result, cx| {
                s.bookmark_edits.task = None;
                match result {
                    Ok(()) => {
                        s.bookmarks = next;
                        cx.emit(FileManagerWorkspaceEvent::BookmarksSaved(effect));
                        s.next_bookmark_edit(cx);
                    }
                    Err(error) => {
                        s.bookmark_edits.queue.clear();
                        if error == "fileManager.favoritesChanged" {
                            s.read_bookmarks(cx);
                        }
                        cx.emit(FileManagerWorkspaceEvent::Error(error));
                    }
                }
                cx.notify();
            });
            if let Some(task) = task {
                self.bookmark_edits.task = Some(task);
            }
            return;
        }
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

    fn reorder(
        state: &Entity<FileManagerState>,
        source: &str,
        target: &str,
        after: bool,
        cx: &mut gpui::TestAppContext,
    ) {
        state.update(cx, |s, cx| {
            s.edit_bookmarks(
                Edit::Reorder {
                    source: source.into(),
                    target: target.into(),
                    after,
                },
                cx,
            )
        });
    }

    #[gpui::test]
    fn reordering_favorites_saves_ids_and_metadata_in_display_order(cx: &mut gpui::TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE_MANAGER_BOOKMARKS_FILENAME);
        let state = cx.new(|_| FileManagerState {
            bookmarks: bookmarks(),
            bookmarks_path: path.clone(),
            ..FileManagerState::default()
        });
        let mut events = cx.events(&state);
        for (source, target, after, expected) in [
            ("a", "b", true, ["b", "a", "c"]),
            ("c", "b", false, ["c", "b", "a"]),
        ] {
            reorder(&state, source, target, after, cx);
            assert!(matches!(
                events.try_recv(),
                Ok(FileManagerWorkspaceEvent::BookmarksSaved(
                    BookmarkChange::Reordered
                ))
            ));
            let saved: Vec<LocalBookmark> =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(
                saved
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<Vec<_>>(),
                expected
            );
        }
        let saved = std::fs::read_to_string(&path).unwrap();
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
            reorder(&state, source, target, after, cx);
            assert!(events.try_recv().is_err());
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    }

    #[gpui::test]
    fn failed_favorites_save_restores_the_previous_order(cx: &mut gpui::TestAppContext) {
        let directory = tempfile::tempdir().unwrap();
        let blocked = directory.path().join("blocked");
        std::fs::write(&blocked, "existing file").unwrap();
        let state = cx.new(|_| FileManagerState {
            bookmarks: bookmarks(),
            bookmarks_path: blocked.join(FILE_MANAGER_BOOKMARKS_FILENAME),
            ..FileManagerState::default()
        });
        let mut events = cx.events(&state);
        reorder(&state, "a", "c", true, cx);
        assert!(matches!(
            events.try_recv(),
            Ok(FileManagerWorkspaceEvent::Error(_))
        ));
        state.read_with(cx, |state, _| {
            assert_eq!(
                state
                    .bookmarks
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<Vec<_>>(),
                ["a", "b", "c"]
            );
        });
        assert_eq!(std::fs::read_to_string(blocked).unwrap(), "existing file");
    }
}

impl FileManagerView {
    pub(in crate::view) fn is_file_manager_path_bookmarked(&self, path: &str, cx: &App) -> bool {
        self.file_manager
            .read(cx)
            .bookmarks
            .iter()
            .any(|bookmark| bookmark.path == path)
    }

    pub(in crate::view) fn toggle_file_manager_current_bookmark(&mut self, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |s, cx| {
            s.edit_bookmarks(Edit::Toggle(s.path.clone()), cx)
        });
    }

    pub(in crate::view) fn remove_file_manager_bookmark(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
    ) {
        self.file_manager
            .update(cx, |s, cx| s.edit_bookmarks(Edit::Remove(id.into()), cx));
    }

    pub(in crate::view) fn open_file_manager_edit_bookmark_dialog(
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
        self.request(FileManagerRequest::ClearIme, cx);
    }

    pub(super) fn update_file_manager_bookmark_name(&mut self, id: String, cx: &mut Context<Self>) {
        self.file_manager.update(cx, |s, cx| {
            let name = s.dialog_value.trim().to_string();
            if !name.is_empty() {
                s.edit_bookmarks(Edit::Rename { id, name }, cx);
            }
        });
    }

    pub(in crate::view) fn reorder_file_manager_bookmark(
        &mut self,
        source: &str,
        target: &str,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.update(cx, |s, cx| {
            s.edit_bookmarks(
                Edit::Reorder {
                    source: source.into(),
                    target: target.into(),
                    after,
                },
                cx,
            )
        });
    }

    pub(in crate::view) fn bookmark_saved(
        &mut self,
        effect: &BookmarkChange,
        cx: &mut Context<Self>,
    ) {
        match effect {
            BookmarkChange::Toggled(added) => {
                let (key, kind) = if *added {
                    ("fileManager.bookmarked", FileManagerNoticeKind::Success)
                } else {
                    ("fileManager.removeBookmark", FileManagerNoticeKind::Default)
                };
                self.push_file_manager_toast(self.i18n.t(key), None, kind, cx);
            }
            BookmarkChange::Renamed { id, name } => {
                let s = self.file_manager.read(cx);
                if matches!(&s.dialog,Some(FileManagerDialog::EditBookmark{id:open,..}) if open==id)
                    && s.dialog_value.trim() == name
                {
                    self.close_file_manager_dialog(cx);
                }
            }
            BookmarkChange::Reordered => {
                self.segmented_control_user_motion
                    .clear(FILE_MANAGER_NAVIGATION_ID);
                cx.notify();
            }
            BookmarkChange::Removed => {}
        }
    }
}
