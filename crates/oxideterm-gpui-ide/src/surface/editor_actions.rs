impl IdeSurface {
    pub fn project_root_path(&self) -> Option<String> {
        self.root_path.clone()
    }

    pub fn open_file_paths(&self) -> Vec<String> {
        self.workspace
            .tabs()
            .iter()
            .filter_map(|tab| match &tab.location {
                IdeLocation::Remote { path, .. } => Some(path.clone()),
                IdeLocation::Local { .. } => None,
            })
            .collect()
    }

    pub fn retry_open_project(&mut self, cx: &mut Context<Self>) {
        let Some(node_id) = self.node_id.clone() else {
            return;
        };
        let Some(root_path) = self.root_path.clone() else {
            return;
        };
        self.open_remote_project(node_id, root_path, cx);
    }

    pub fn restore_snapshot(&mut self, snapshot: WorkspaceSnapshot, cx: &mut Context<Self>) {
        let node_id = match &snapshot.project.root {
            IdeLocation::Remote { node_id, .. } => node_id.clone(),
            IdeLocation::Local { .. } => return,
        };
        let root_path = match &snapshot.project.root {
            IdeLocation::Remote { path, .. } => path.clone(),
            IdeLocation::Local { .. } => return,
        };
        let buffers = snapshot.buffers.clone();
        let result = self.workspace.restore_snapshot(snapshot);
        if !matches!(
            result,
            oxideterm_ide_core::RestoreSnapshotResult::Restored { .. }
        ) {
            return;
        }

        self.node_id = Some(node_id);
        self.root_path = Some(root_path);
        self.load_state = IdeLoadState::Ready;
        self.editors.clear();
        for buffer in buffers {
            self.create_editor(buffer.tab_id, &buffer.location, buffer.text, cx);
        }
        self.refresh_agent_status(cx);
        self.schedule_next_agent_status_poll(cx);
        cx.notify();
    }

    fn apply_project_open(&mut self, result: ProjectOpenResult, cx: &mut Context<Self>) {
        let root = result.root.clone();
        self.workspace.open_project(root.clone(), result.title);
        let _ = self.workspace.set_tree_expanded(&root, true);
        let _ = self.workspace.set_tree_children(root, result.children);
        self.node_id = Some(result.node_id);
        self.git_branch = result.git_branch;
        self.load_state = IdeLoadState::Ready;
        self.agent_opt_in_open = self.runtime_settings.agent_mode == NodeAgentMode::Ask;
        self.refresh_agent_status(cx);
        self.schedule_next_agent_status_poll(cx);
        cx.emit(IdeSurfaceEvent::ProjectOpened);
        let node_id = self.node_id.clone();
        for path in std::mem::take(&mut self.pending_restore_files) {
            if let Some(node_id) = node_id.clone() {
                self.open_remote_file(IdeLocation::remote(node_id, path), cx);
            }
        }
        cx.notify();
    }

    fn load_directory(&mut self, directory: IdeLocation, cx: &mut Context<Self>) {
        let key = directory.stable_key();
        if self.loading_paths.contains(&key) {
            return;
        }
        self.loading_paths.insert(key.clone());
        let fs = self.fs.clone();
        let generation = self.generation;
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let directory_for_task = directory.clone();
            let result = await_ide_backend(backend_runtime.spawn(async move {
                fs.list_dir(&directory_for_task)
                    .await
                    .map(sort_tree_entries)
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_paths.remove(&key);
                match result {
                    Ok(children) => {
                        let _ = this.workspace.set_tree_expanded(&directory, true);
                        let _ = this.workspace.set_tree_children(directory, children);
                    }
                    Err(error) => this.last_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_tree_entry(&mut self, entry: FileTreeEntry, cx: &mut Context<Self>) {
        let _ = self
            .workspace
            .select_tree_entry(Some(entry.location.clone()));
        match entry.kind {
            FileKind::Directory => {
                if self.workspace.file_tree().is_expanded(&entry.location) {
                    let _ = self.workspace.set_tree_expanded(&entry.location, false);
                    cx.notify();
                } else {
                    self.load_directory(entry.location, cx);
                }
            }
            FileKind::File | FileKind::Symlink | FileKind::Other => {
                self.open_remote_file(entry.location, cx);
            }
        }
    }

    fn open_remote_file(&mut self, location: IdeLocation, cx: &mut Context<Self>) {
        if let Some(tab_id) = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.location == location)
            .map(|tab| tab.id)
        {
            let _ = self.workspace.set_active_tab(tab_id);
            self.apply_pending_reconnect_dirty_for_tab(tab_id, cx);
            cx.notify();
            return;
        }
        let key = location.stable_key();
        if self.loading_paths.contains(&key) {
            return;
        }
        self.loading_paths.insert(key.clone());
        let fs = self.fs.clone();
        let generation = self.generation;
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(backend_runtime.spawn({
                let location = location.clone();
                async move { open_text_file(fs, location).await }
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                this.loading_paths.remove(&key);
                match result {
                    Ok(result) => {
                        let dirty_text = remote_path(&result.location)
                            .and_then(|path| this.pending_restore_dirty_contents.remove(path));
                        match this.workspace.open_file(
                            result.location.clone(),
                            result.text.clone(),
                            result.version,
                        ) {
                            Ok(outcome) => {
                                let tab_id = match outcome {
                                    oxideterm_ide_core::OpenFileOutcome::Opened(tab_id)
                                    | oxideterm_ide_core::OpenFileOutcome::Reused(tab_id) => tab_id,
                                };
                                this.create_editor(tab_id, &result.location, result.text, cx);
                                if let Some(dirty_text) = dirty_text {
                                    this.apply_reconnect_dirty_text(tab_id, dirty_text, cx);
                                }
                            }
                            Err(error) => this.last_error = Some(error.to_string()),
                        }
                    }
                    Err(error) => this.last_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn create_editor(
        &mut self,
        tab_id: EditorTabId,
        location: &IdeLocation,
        text: String,
        cx: &mut Context<Self>,
    ) {
        let tokens = self.tokens;
        let runtime_settings = self.runtime_settings;
        let language = language_for_location(location, &text);
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(text, &tokens, cx);
            editor.apply_ide_runtime_settings(
                &tokens,
                runtime_settings.editor_font_size,
                runtime_settings.editor_line_height,
                runtime_settings.word_wrap,
                runtime_settings.background_active,
                cx,
            );
            editor.set_language(language, cx);
            editor
        });
        self.editors.insert(tab_id, editor);
    }

    fn apply_pending_reconnect_dirty_for_tab(
        &mut self,
        tab_id: EditorTabId,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self
            .workspace
            .buffer(tab_id)
            .and_then(|buffer| remote_path(&buffer.location).map(ToOwned::to_owned))
        else {
            return;
        };
        let Some(dirty_text) = self.pending_restore_dirty_contents.remove(&path) else {
            return;
        };
        self.apply_reconnect_dirty_text(tab_id, dirty_text, cx);
    }

    fn apply_reconnect_dirty_text(
        &mut self,
        tab_id: EditorTabId,
        dirty_text: String,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.workspace.buffer(tab_id).cloned() else {
            return;
        };
        if buffer.is_dirty() || dirty_text == buffer.saved_text {
            return;
        }

        // Tauri only writes snapshot dirtyContents back into clean tabs. Native
        // keeps the same user-intent rule so edits made after the snapshot win.
        let _ = self
            .workspace
            .replace_buffer_text(tab_id, dirty_text.clone());
        if let Some(editor) = self.editors.get(&tab_id) {
            editor.update(cx, |editor, cx| {
                editor.replace_text_external(dirty_text, cx);
            });
        }
    }

    fn activate_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let previous = self.workspace.active_tab();
        if previous == Some(tab_id) {
            return;
        }
        // Tauri auto-saves the previously active dirty tab when activeTabId
        // changes. Window-blur save-all still needs a GPUI focus-loss hook.
        if self.runtime_settings.auto_save
            && let Some(previous_tab_id) = previous
            && self.is_tab_dirty(previous_tab_id, cx)
            && !self.saving_tabs.contains(&previous_tab_id)
        {
            self.save_tab(previous_tab_id, cx);
        }
        let _ = self.workspace.set_active_tab(tab_id);
        cx.notify();
    }

    fn close_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.sync_editor_to_workspace(tab_id, cx);
        match self.workspace.request_close_tab(tab_id) {
            Ok(None) => {
                self.editors.remove(&tab_id);
                if self
                    .conflict_state
                    .as_ref()
                    .is_some_and(|conflict| conflict.tab_id == tab_id)
                {
                    self.conflict_state = None;
                }
                cx.notify();
            }
            Ok(Some(_)) => cx.notify(),
            Err(error) => {
                self.last_error = Some(error.to_string());
                cx.notify();
            }
        }
    }

    fn toggle_tab_pin(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        if let Err(error) = self.workspace.toggle_tab_pin(tab_id) {
            self.last_error = Some(error.to_string());
        }
        cx.notify();
    }

    fn start_tab_drag(&mut self, tab_id: EditorTabId, position: Point<Pixels>) {
        self.tab_drag = Some(TabDrag {
            tab_id,
            start_position: position,
            over_tab_id: tab_id,
            activated: false,
        });
    }

    fn update_tab_drag(
        &mut self,
        target_tab_id: EditorTabId,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(mut drag) = self.tab_drag else {
            return;
        };
        if event.pressed_button != Some(MouseButton::Left) {
            return;
        }
        let distance = f32::from(event.position.x - drag.start_position.x).abs();
        if !drag.activated && distance < IDE_TAB_REORDER_ACTIVATION_PX {
            return;
        }
        drag.activated = true;
        drag.over_tab_id = target_tab_id;
        self.tab_drag = Some(drag);
        cx.notify();
    }

    fn finish_tab_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.tab_drag.take() {
            if drag.activated
                && drag.tab_id != drag.over_tab_id
                && let Some(target_index) = self
                    .workspace
                    .tabs()
                    .iter()
                    .position(|tab| tab.id == drag.over_tab_id)
            {
                let _ = self.workspace.move_tab_to_index(drag.tab_id, target_index);
            }
            cx.notify();
        }
    }

    fn resolve_dirty_close(&mut self, decision: DirtyCloseDecision, cx: &mut Context<Self>) {
        let Some(request) = self.workspace.pending_close().cloned() else {
            return;
        };
        match decision {
            DirtyCloseDecision::Save => {
                self.save_after_close = Some(request.id);
                self.save_tab(request.tab_id, cx);
            }
            DirtyCloseDecision::Discard | DirtyCloseDecision::Cancel => {
                let closing_tab = request.tab_id;
                let resolved = self.workspace.resolve_dirty_close(request.id, decision);
                if matches!(resolved, Ok(None)) && decision == DirtyCloseDecision::Discard {
                    self.editors.remove(&closing_tab);
                    if self
                        .conflict_state
                        .as_ref()
                        .is_some_and(|conflict| conflict.tab_id == closing_tab)
                    {
                        self.conflict_state = None;
                    }
                }
                cx.notify();
            }
        }
    }

    fn save_tab(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.sync_editor_to_workspace(tab_id, cx);
        let Some(buffer) = self.workspace.buffer(tab_id).cloned() else {
            return;
        };
        let title = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| tab.id == tab_id)
            .map(|tab| tab.title.clone())
            .unwrap_or_else(|| buffer.location.display_name());
        let local_mtime = buffer.version.modified_millis.map(|millis| millis / 1000);
        if self.saving_tabs.contains(&tab_id) {
            return;
        }
        self.saving_tabs.insert(tab_id);
        let close_request = self.save_after_close.take();
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let mode = if fs.capabilities().atomic_write {
                WriteMode::AtomicReplace
            } else {
                WriteMode::CreateOrReplace
            };
            let result = backend_runtime
                .spawn(async move {
                    match fs
                        .write_file(&buffer.location, &buffer.text, Some(&buffer.version), mode)
                        .await
                    {
                        Ok(version) => Ok(version),
                        Err(error) if error.kind == IdeFileErrorKind::Conflict => {
                            let remote_version = fs
                                .stat(&buffer.location)
                                .await
                                .ok()
                                .map(|stat| stat.version);
                            Err((error, remote_version))
                        }
                        Err(error) => Err((error, None)),
                    }
                })
                .await
                .unwrap_or_else(|error| {
                    Err((
                        IdeFileError::new(
                            IdeFileErrorKind::Other,
                            format!("IDE backend task failed: {error}"),
                        ),
                        None,
                    ))
                });
            let _ = weak.update(cx, |this, cx| {
                this.saving_tabs.remove(&tab_id);
                match result {
                    Ok(version) => {
                        if let Some(request_id) = close_request {
                            let _ = this
                                .workspace
                                .complete_dirty_close_after_save(request_id, version.clone());
                            this.editors.remove(&tab_id);
                        } else {
                            let _ = this.workspace.mark_saved(tab_id, version);
                        }
                        if let Some(editor) = this.editors.get(&tab_id) {
                            editor.update(cx, |editor, cx| editor.mark_saved_external(cx));
                        }
                    }
                    Err((error, remote_version)) if error.kind == IdeFileErrorKind::Conflict => {
                        this.conflict_state = Some(ConflictState {
                            tab_id,
                            title,
                            local_mtime,
                            remote_mtime: remote_version
                                .and_then(|version| version.modified_millis)
                                .map(|millis| millis / 1000),
                            close_request,
                        });
                        if let Some(editor) = this.editors.get(&tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.mark_save_failed_external(
                                    this.labels.conflict_title.clone(),
                                    cx,
                                )
                            });
                        }
                    }
                    Err((error, _)) => {
                        let message = format!("{}: {}", this.labels.save_failed, error.message);
                        this.last_error = Some(message.clone());
                        if let Some(editor) = this.editors.get(&tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.mark_save_failed_external(message, cx)
                            });
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn clear_conflict(&mut self, cx: &mut Context<Self>) {
        self.conflict_state = None;
        cx.notify();
    }

    fn overwrite_conflict(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.conflict_state.clone() else {
            return;
        };
        let Some(buffer) = self.workspace.buffer(conflict.tab_id).cloned() else {
            self.conflict_state = None;
            cx.notify();
            return;
        };
        if self.saving_tabs.contains(&conflict.tab_id) {
            return;
        }
        self.saving_tabs.insert(conflict.tab_id);
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let mode = if fs.capabilities().atomic_write {
                WriteMode::AtomicReplace
            } else {
                WriteMode::CreateOrReplace
            };
            let result = await_ide_backend(backend_runtime.spawn(async move {
                // Tauri resolveConflict('overwrite') force-saves without the
                // agent hash / SFTP mtime expectation after the user confirms.
                fs.write_file(&buffer.location, &buffer.text, None, mode)
                    .await
            }))
            .await;
            let _ = weak.update(cx, |this, cx| {
                this.saving_tabs.remove(&conflict.tab_id);
                match result {
                    Ok(version) => {
                        this.conflict_state = None;
                        if let Some(request_id) = conflict.close_request {
                            let _ = this
                                .workspace
                                .complete_dirty_close_after_save(request_id, version.clone());
                            this.editors.remove(&conflict.tab_id);
                        } else {
                            let _ = this.workspace.mark_saved(conflict.tab_id, version);
                        }
                        if let Some(editor) = this.editors.get(&conflict.tab_id) {
                            editor.update(cx, |editor, cx| editor.mark_saved_external(cx));
                        }
                    }
                    Err(error) => {
                        let message = format!("{}: {}", this.labels.save_failed, error.message);
                        this.last_error = Some(message.clone());
                        if let Some(editor) = this.editors.get(&conflict.tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.mark_save_failed_external(message, cx)
                            });
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn reload_conflict(&mut self, cx: &mut Context<Self>) {
        let Some(conflict) = self.conflict_state.clone() else {
            return;
        };
        let Some(buffer) = self.workspace.buffer(conflict.tab_id).cloned() else {
            self.conflict_state = None;
            cx.notify();
            return;
        };
        let fs = self.fs.clone();
        let backend_runtime = self.backend_runtime.clone();
        cx.notify();

        cx.spawn(async move |weak, cx| {
            let result = await_ide_backend(
                backend_runtime.spawn(async move { fs.read_file(&buffer.location).await }),
            )
            .await;
            let _ = weak.update(cx, |this, cx| {
                match result {
                    Ok(data) => {
                        this.conflict_state = None;
                        let _ = this
                            .workspace
                            .replace_buffer_text(conflict.tab_id, data.text.clone());
                        let _ = this.workspace.mark_saved(conflict.tab_id, data.version);
                        if let Some(editor) = this.editors.get(&conflict.tab_id) {
                            editor.update(cx, |editor, cx| {
                                editor.replace_text_external(data.text, cx);
                                editor.mark_saved_external(cx);
                            });
                        }
                    }
                    Err(error) => {
                        this.last_error =
                            Some(format!("{}: {}", this.labels.open_failed, error.message));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn sync_editor_to_workspace(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(editor) = self.editors.get(&tab_id) else {
            return;
        };
        let text = editor.read(cx).buffer().text();
        let _ = self.workspace.replace_buffer_text(tab_id, text);
    }

    fn sync_all_editors(&mut self, cx: &mut Context<Self>) {
        let tab_ids = self.editors.keys().copied().collect::<Vec<_>>();
        for tab_id in tab_ids {
            self.sync_editor_to_workspace(tab_id, cx);
        }
    }

    fn active_editor(&self) -> Option<Entity<TextEditorView>> {
        self.workspace
            .active_tab()
            .and_then(|tab_id| self.editors.get(&tab_id).cloned())
    }

    fn is_tab_dirty(&self, tab_id: EditorTabId, cx: &mut Context<Self>) -> bool {
        self.editors
            .get(&tab_id)
            .map(|editor| editor.read(cx).buffer().is_dirty())
            .or_else(|| self.workspace.buffer(tab_id).map(|buffer| buffer.is_dirty()))
            .unwrap_or(false)
    }
}
