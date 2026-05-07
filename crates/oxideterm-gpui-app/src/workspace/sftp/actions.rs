impl WorkspaceApp {
    pub(super) fn handle_sftp_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let key = event.keystroke.key.as_str();
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
        if self.sftp_view.context_menu.is_some() && key == "escape" {
            self.sftp_view.context_menu = None;
            cx.notify();
            return true;
        }
        if self.sftp_view.dialog.is_some() && self.sftp_view.focused_input.is_none() {
            match key {
                "escape" => {
                    self.close_sftp_dialog();
                    cx.notify();
                    return true;
                }
                "enter" => {
                    self.accept_sftp_dialog();
                    cx.notify();
                    return true;
                }
                _ => {}
            }
        }
        if let Some(input) = self.sftp_view.focused_input {
            match key {
                "escape" => {
                    self.sftp_view.focused_input = None;
                    self.sftp_view.editing_local_path = false;
                    self.sftp_view.editing_remote_path = false;
                    self.ime_marked_text = None;
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
                    self.open_or_preview_sftp_file(self.sftp_view.active_pane, &file);
                    cx.notify();
                    true
                } else {
                    false
                }
            }
            "space" | " " => {
                if let Some(file) = self.single_selected_sftp_file(self.sftp_view.active_pane)
                    && file.file_type != SftpFileType::Directory
                {
                    self.sftp_view.dialog = Some(SftpDialog::Preview { name: file.name });
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
                self.sftp_view.local_path = path.clone();
                self.sftp_view.local_path_input = path.clone();
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
            (SftpPane::Local, "~") => home_path_mock(),
            (SftpPane::Remote, "~") => "/home/lipsc".to_string(),
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

    fn open_or_preview_sftp_file(&mut self, pane: SftpPane, file: &SftpFileEntry) {
        self.sftp_view.active_pane = pane;
        self.sftp_view.context_menu = None;
        if file.file_type == SftpFileType::Directory {
            let base = match pane {
                SftpPane::Local => self.sftp_view.local_path.clone(),
                SftpPane::Remote => self.sftp_view.remote_path.clone(),
            };
            self.set_sftp_path(pane, join_sftp_path(&base, &file.name));
        } else {
            self.stop_sftp_preview_media();
            self.sftp_view.preview_path = Some(file.path.clone());
            self.sftp_view.preview_content = None;
            self.sftp_view.preview_asset_owner = None;
            self.sftp_view.preview_session = PreviewSession::loading();
            self.sftp_view.preview_error = None;
            self.sftp_view.preview_loading = pane == SftpPane::Remote;
            self.sftp_view.dialog = Some(SftpDialog::Preview {
                name: file.name.clone(),
            });
            if pane == SftpPane::Remote {
                self.spawn_remote_sftp_preview(file.path.clone());
            } else {
                self.sftp_view.preview_loading = true;
                self.spawn_local_sftp_preview(file.path.clone());
            }
        }
    }

    fn can_compare_sftp_preview(&self, name: &str) -> bool {
        matches!(
            self.sftp_view.preview_content.as_ref(),
            Some(PreviewContent::Text { .. })
        ) && self
            .sftp_view
            .local_files
            .iter()
            .any(|file| file.name == name && file.file_type == SftpFileType::File)
    }

    fn open_sftp_preview_compare(&mut self, name: &str) {
        let Some(PreviewContent::Text { data, .. }) = self.sftp_view.preview_content.clone() else {
            return;
        };
        let Some(local_file) = self
            .sftp_view
            .local_files
            .iter()
            .find(|file| file.name == name && file.file_type == SftpFileType::File)
            .cloned()
        else {
            self.sftp_view.preview_error = Some(format!(
                "{}: {}",
                self.i18n.t("sftp.toast.compare_failed"),
                self.i18n.t("sftp.toast.compare_no_local")
            ));
            return;
        };

        match std::fs::read_to_string(&local_file.path) {
            Ok(local_content) => {
                let remote_path = self.sftp_view.preview_path.clone().unwrap_or_default();
                self.sftp_view.dialog = Some(SftpDialog::Diff {
                    local_path: local_file.path,
                    local_content,
                    remote_path,
                    remote_content: data,
                });
            }
            Err(error) => {
                self.sftp_view.preview_error = Some(format!(
                    "{}: {}",
                    self.i18n.t("sftp.toast.compare_failed"),
                    error
                ));
            }
        }
    }

    fn spawn_remote_sftp_preview(&self, path: String) {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let router = self.node_router.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = load_remote_sftp_preview(router, &node_id, &path).await;
            let _ = tx.send(SftpWorkerResult::PreviewLoaded { path, result });
        });
    }

    fn spawn_local_sftp_preview(&self, path: String) {
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = load_local_sftp_preview(&path).await;
            let _ = tx.send(SftpWorkerResult::PreviewLoaded { path, result });
        });
    }

    fn open_sftp_context_menu(
        &mut self,
        pane: SftpPane,
        file: Option<SftpFileEntry>,
        x: f32,
        y: f32,
    ) {
        self.sftp_view.active_pane = pane;
        if let Some(file) = file.as_ref() {
            let selected = match pane {
                SftpPane::Local => &mut self.sftp_view.local_selected,
                SftpPane::Remote => &mut self.sftp_view.remote_selected,
            };
            if !selected.contains(&file.name) {
                selected.clear();
                selected.insert(file.name.clone());
                match pane {
                    SftpPane::Local => self.sftp_view.local_last_selected = Some(file.name.clone()),
                    SftpPane::Remote => {
                        self.sftp_view.remote_last_selected = Some(file.name.clone())
                    }
                }
            }
        }
        self.sftp_view.context_menu = Some(SftpContextMenu { pane, file, x, y });
    }

    fn open_sftp_rename_dialog(&mut self, pane: SftpPane, old_name: String) {
        self.sftp_view.dialog_value = old_name.clone();
        self.sftp_view.dialog = Some(SftpDialog::Rename { pane, old_name });
        self.sftp_view.focused_input = Some(SftpInput::DialogValue);
    }

    fn open_sftp_new_folder_dialog(&mut self, pane: SftpPane) {
        self.sftp_view.dialog_value.clear();
        self.sftp_view.dialog = Some(SftpDialog::NewFolder { pane });
        self.sftp_view.focused_input = Some(SftpInput::DialogValue);
    }

    fn queue_sftp_transfers(&mut self, pane: SftpPane, direction: SftpTransferDirection) {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let selected = match pane {
            SftpPane::Local => self.sftp_view.local_selected.clone(),
            SftpPane::Remote => self.sftp_view.remote_selected.clone(),
        };
        if selected.is_empty() {
            return;
        }
        let source_files = match pane {
            SftpPane::Local => self.sftp_view.local_files.clone(),
            SftpPane::Remote => self.sftp_view.remote_files.clone(),
        };
        for name in selected {
            let file = source_files.iter().find(|file| file.name == name);
            let id = self.sftp_view.next_transfer_id;
            self.sftp_view.next_transfer_id += 1;
            let size = file.map(|file| file.size).unwrap_or_default().max(1);
            let is_directory = file.is_some_and(|file| file.file_type == SftpFileType::Directory);
            let local_path = match (direction, file) {
                (SftpTransferDirection::Upload, Some(file)) => file.path.clone(),
                _ => join_local_path(&self.sftp_view.local_path, &name),
            };
            let remote_path = match (direction, file) {
                (SftpTransferDirection::Download, Some(file)) => file.path.clone(),
                _ => join_sftp_path(&self.sftp_view.remote_path, &name),
            };
            self.sftp_view.transfers.push(SftpTransferItem {
                id,
                name: if is_directory {
                    format!("{name}/")
                } else {
                    name.clone()
                },
                local_path: local_path.clone(),
                remote_path: remote_path.clone(),
                direction,
                size,
                transferred: 0,
                state: SftpTransferState::Pending,
                error: None,
            });
            self.spawn_sftp_transfer_task(
                id,
                node_id.clone(),
                direction,
                is_directory,
                local_path,
                remote_path,
            );
        }
        self.clear_sftp_selection(pane);
    }

    fn spawn_sftp_transfer_task(
        &self,
        id: u64,
        node_id: NodeId,
        direction: SftpTransferDirection,
        is_directory: bool,
        local_path: String,
        remote_path: String,
    ) {
        let router = self.node_router.clone();
        let manager = self.sftp_transfer_manager.clone();
        let progress_store = self.sftp_progress_store.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let transfer_id = id.to_string();
            let mut directory_progress = is_directory.then(|| {
                let transfer_type = match direction {
                    SftpTransferDirection::Upload => RemoteTransferType::Upload,
                    SftpTransferDirection::Download => RemoteTransferType::Download,
                };
                let mut progress = StoredTransferProgress::new(
                    transfer_id.clone(),
                    transfer_type,
                    match direction {
                        SftpTransferDirection::Upload => local_path.clone().into(),
                        SftpTransferDirection::Download => remote_path.clone().into(),
                    },
                    match direction {
                        SftpTransferDirection::Upload => remote_path.clone().into(),
                        SftpTransferDirection::Download => local_path.clone().into(),
                    },
                    0,
                    format!("node:{}", node_id.0),
                );
                progress.strategy = RemoteTransferStrategy::DirectoryRecursive;
                progress
            });
            if let Some(progress) = directory_progress.as_ref() {
                let _ = progress_store.save(progress).await;
            }
            let _ = tx.send(SftpWorkerResult::TransferProgress {
                id,
                transferred: 0,
                total: 0,
                state: SftpTransferState::Active,
                error: None,
            });
            let (progress_tx, mut progress_rx) =
                tokio::sync::mpsc::channel::<TransferProgress>(100);
            let progress_ui_tx = tx.clone();
            let progress_store_for_task = progress_store.clone();
            tokio::spawn(async move {
                let mut accumulator = DirectoryProgressAccumulator::default();
                while let Some(progress) = progress_rx.recv().await {
                    let progress = if is_directory {
                        accumulator.update(progress)
                    } else {
                        progress
                    };
                    if let Some(stored) = directory_progress.as_mut() {
                        stored.total_bytes = stored.total_bytes.max(progress.total_bytes);
                        stored.update_progress(progress.transferred_bytes);
                        let _ = progress_store_for_task.save(stored).await;
                    }
                    let _ = progress_ui_tx.send(SftpWorkerResult::TransferProgress {
                        id,
                        transferred: progress.transferred_bytes,
                        total: progress.total_bytes,
                        state: sftp_transfer_state_from_remote(progress.state),
                        error: progress.error,
                    });
                }
            });

            let result = async {
                let _permit = manager.acquire_permit().await;
                let sftp = router
                    .acquire_transfer_sftp(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                match (direction, is_directory) {
                    (SftpTransferDirection::Upload, true) => {
                        let resolved = router
                            .resolve_connection(&node_id)
                            .map_err(|error| error.to_string())?;
                        if probe_tar_support(&resolved.handle).await {
                            {
                                let shared = router
                                    .acquire_sftp(&node_id)
                                    .await
                                    .map_err(|error| error.to_string())?;
                                let shared = shared.lock().await;
                                for prefix in remote_directory_prefixes(&remote_path) {
                                    let _ = shared.mkdir(&prefix).await;
                                }
                            }
                            let compression = probe_tar_compression(&resolved.handle).await;
                            let tar_result = tar_upload_directory(
                                &resolved.handle,
                                &local_path,
                                &remote_path,
                                &transfer_id,
                                Some(progress_tx.clone()),
                                Some(manager.clone()),
                                Some(compression),
                            )
                            .await;
                            match tar_result {
                                Ok(_) => {}
                                Err(error)
                                    if !manager
                                        .get_control(&transfer_id)
                                        .is_some_and(|control| control.is_cancelled()) =>
                                {
                                    sftp.upload_dir(
                                        &local_path,
                                        &remote_path,
                                        &transfer_id,
                                        Some(progress_tx),
                                        Some(manager.clone()),
                                    )
                                    .await
                                    .map_err(|fallback_error| {
                                        format!(
                                            "tar upload failed ({error}); recursive fallback failed ({fallback_error})"
                                        )
                                    })?;
                                }
                                Err(error) => return Err(error.to_string()),
                            }
                        } else {
                            sftp.upload_dir(
                                &local_path,
                                &remote_path,
                                &transfer_id,
                                Some(progress_tx),
                                Some(manager.clone()),
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                        }
                    }
                    (SftpTransferDirection::Upload, false) => {
                        sftp.upload_with_resume(
                            &local_path,
                            &remote_path,
                            progress_store.clone(),
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(transfer_id.clone()),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    }
                    (SftpTransferDirection::Download, true) => {
                        let resolved = router
                            .resolve_connection(&node_id)
                            .map_err(|error| error.to_string())?;
                        if probe_tar_support(&resolved.handle).await {
                            let compression = probe_tar_compression(&resolved.handle).await;
                            let tar_result = tar_download_directory(
                                &resolved.handle,
                                &remote_path,
                                &local_path,
                                &transfer_id,
                                Some(progress_tx.clone()),
                                Some(manager.clone()),
                                Some(compression),
                            )
                            .await;
                            match tar_result {
                                Ok(_) => {}
                                Err(error)
                                    if !manager
                                        .get_control(&transfer_id)
                                        .is_some_and(|control| control.is_cancelled()) =>
                                {
                                    sftp.download_dir(
                                        &remote_path,
                                        &local_path,
                                        &transfer_id,
                                        Some(progress_tx),
                                        Some(manager.clone()),
                                    )
                                    .await
                                    .map_err(|fallback_error| {
                                        format!(
                                            "tar download failed ({error}); recursive fallback failed ({fallback_error})"
                                        )
                                    })?;
                                }
                                Err(error) => return Err(error.to_string()),
                            }
                        } else {
                            sftp.download_dir(
                                &remote_path,
                                &local_path,
                                &transfer_id,
                                Some(progress_tx),
                                Some(manager.clone()),
                            )
                            .await
                            .map_err(|error| error.to_string())?;
                        }
                    }
                    (SftpTransferDirection::Download, false) => {
                        sftp.download_with_resume(
                            &remote_path,
                            &local_path,
                            progress_store.clone(),
                            Some(progress_tx),
                            Some(manager.clone()),
                            Some(transfer_id.clone()),
                        )
                        .await
                        .map_err(|error| error.to_string())?;
                    }
                }
                Ok::<(), String>(())
            }
            .await
            .map_err(|error| error.to_string());

            if is_directory {
                match &result {
                    Ok(()) => {
                        let _ = progress_store.delete(&transfer_id).await;
                    }
                    Err(error) if error.to_ascii_lowercase().contains("cancel") => {
                        let _ = progress_store.delete(&transfer_id).await;
                    }
                    Err(error) => {
                        if let Ok(Some(mut progress)) = progress_store.load(&transfer_id).await {
                            progress.mark_failed(error.clone());
                            let _ = progress_store.save(&progress).await;
                        }
                    }
                }
            }

            let _ = tx.send(SftpWorkerResult::TransferComplete {
                id,
                result,
                refresh_remote: matches!(direction, SftpTransferDirection::Upload),
                refresh_local: matches!(direction, SftpTransferDirection::Download),
            });
        });
    }

    fn set_sftp_transfer_state(&mut self, id: u64, state: SftpTransferState) {
        match state {
            SftpTransferState::Paused => {
                self.sftp_transfer_manager.pause(&id.to_string());
            }
            SftpTransferState::Pending | SftpTransferState::Active => {
                self.sftp_transfer_manager.resume(&id.to_string());
            }
            SftpTransferState::Cancelled => {
                self.sftp_transfer_manager.cancel(&id.to_string());
            }
            SftpTransferState::Completed | SftpTransferState::Error => {}
        }
        if let Some(item) = self
            .sftp_view
            .transfers
            .iter_mut()
            .find(|item| item.id == id)
        {
            item.state = state;
        }
    }

    fn cancel_or_remove_sftp_transfer(&mut self, id: u64) {
        if let Some(index) = self
            .sftp_view
            .transfers
            .iter()
            .position(|item| item.id == id)
        {
            let active = matches!(
                self.sftp_view.transfers[index].state,
                SftpTransferState::Active | SftpTransferState::Pending | SftpTransferState::Paused
            );
            if active {
                self.sftp_transfer_manager.cancel(&id.to_string());
                self.sftp_view.transfers[index].state = SftpTransferState::Cancelled;
            } else {
                self.sftp_view.transfers.remove(index);
            }
        }
    }

    fn spawn_remote_sftp_mutation<F>(&self, operation: F)
    where
        F: FnOnce(
                SftpSession,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
            > + Send
            + 'static,
    {
        let Some(tab_id) = self.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let router = self.node_router.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = async {
                let sftp = router
                    .acquire_transfer_sftp(&node_id)
                    .await
                    .map_err(|error| error.to_string())?;
                operation(sftp).await
            }
            .await;
            let _ = tx.send(SftpWorkerResult::RemoteMutationComplete {
                result,
                refresh_remote: true,
                refresh_local: false,
            });
        });
    }

    fn close_sftp_dialog(&mut self) {
        self.stop_sftp_preview_media();
        self.sftp_view.dialog = None;
        self.sftp_view.dialog_value.clear();
        self.sftp_view.preview_asset_owner = None;
        self.sftp_view.preview_session = PreviewSession::default();
        self.sftp_view.focused_input = None;
        self.ime_marked_text = None;
    }

    fn stop_sftp_preview_media(&mut self) {
        let _ = self
            .sftp_view
            .preview_audio
            .command(AudioPreviewCommand::Stop);
        self.sftp_view.preview_audio_tick_active = false;
        self.sftp_view.preview_video_surface.detach();
    }

    fn toggle_sftp_preview_audio(&mut self, cx: &mut Context<Self>) {
        let _ = self
            .sftp_view
            .preview_audio
            .command(AudioPreviewCommand::PlayPause);
        self.schedule_sftp_preview_audio_tick(cx);
    }

    fn seek_sftp_preview_audio(&mut self, position: std::time::Duration, cx: &mut Context<Self>) {
        let _ = self
            .sftp_view
            .preview_audio
            .command(AudioPreviewCommand::Seek(position));
        self.schedule_sftp_preview_audio_tick(cx);
    }

    fn schedule_sftp_preview_audio_tick(&mut self, cx: &mut Context<Self>) {
        if self.sftp_view.preview_audio_tick_active {
            return;
        }
        self.sftp_view.preview_audio_tick_active = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(250))
                    .await;
                let should_continue = this
                    .update(cx, |this, cx| {
                        let playing = matches!(
                            this.sftp_view.preview_audio.snapshot().state,
                            AudioPreviewState::Playing
                        );
                        if !playing {
                            this.sftp_view.preview_audio_tick_active = false;
                        }
                        cx.notify();
                        playing
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        })
        .detach();
    }

    fn accept_sftp_dialog(&mut self) {
        let Some(dialog) = self.sftp_view.dialog.clone() else {
            return;
        };
        match dialog {
            SftpDialog::Rename { pane, old_name } => {
                let new_name = self.sftp_view.dialog_value.trim().to_string();
                if !new_name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let old_path = join_local_path(&self.sftp_view.local_path, &old_name);
                            let new_path = join_local_path(&self.sftp_view.local_path, &new_name);
                            let _ = std::fs::rename(old_path, new_path);
                            if let Ok(files) = list_local_files(&self.sftp_view.local_path) {
                                self.sftp_view.local_files = files;
                            }
                        }
                        SftpPane::Remote => {
                            let old_path = self
                                .sftp_view
                                .remote_files
                                .iter()
                                .find(|file| file.name == old_name)
                                .map(|file| file.path.clone())
                                .unwrap_or_else(|| {
                                    join_sftp_path(&self.sftp_view.remote_path, &old_name)
                                });
                            let new_path = join_sftp_path(&parent_path(&old_path, true), &new_name);
                            self.spawn_remote_sftp_mutation(move |sftp| {
                                Box::pin(async move {
                                    sftp.rename(&old_path, &new_path)
                                        .await
                                        .map_err(|error| error.to_string())
                                })
                            });
                        }
                    }
                }
            }
            SftpDialog::NewFolder { pane } => {
                let name = self.sftp_view.dialog_value.trim().to_string();
                if !name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let path = join_local_path(&self.sftp_view.local_path, &name);
                            let _ = std::fs::create_dir_all(path);
                            if let Ok(files) = list_local_files(&self.sftp_view.local_path) {
                                self.sftp_view.local_files = files;
                            }
                        }
                        SftpPane::Remote => {
                            let path = join_sftp_path(&self.sftp_view.remote_path, &name);
                            self.spawn_remote_sftp_mutation(move |sftp| {
                                Box::pin(async move {
                                    sftp.mkdir(&path).await.map_err(|error| error.to_string())
                                })
                            });
                        }
                    }
                }
            }
            SftpDialog::Delete { pane, files } => {
                match pane {
                    SftpPane::Local => {
                        for name in files {
                            let path = join_local_path(&self.sftp_view.local_path, &name);
                            if std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
                                let _ = std::fs::remove_dir_all(path);
                            } else {
                                let _ = std::fs::remove_file(path);
                            }
                        }
                        if let Ok(files) = list_local_files(&self.sftp_view.local_path) {
                            self.sftp_view.local_files = files;
                        }
                    }
                    SftpPane::Remote => {
                        let remote_files = self.sftp_view.remote_files.clone();
                        let targets = files
                            .into_iter()
                            .filter_map(|name| {
                                remote_files
                                    .iter()
                                    .find(|file| file.name == name)
                                    .map(|file| file.path.clone())
                            })
                            .collect::<Vec<_>>();
                        self.spawn_remote_sftp_mutation(move |sftp| {
                            Box::pin(async move {
                                for path in targets {
                                    sftp.delete_recursive(&path)
                                        .await
                                        .map_err(|error| error.to_string())?;
                                }
                                Ok(())
                            })
                        });
                    }
                }
                self.clear_sftp_selection(pane);
            }
            _ => {}
        }
        self.close_sftp_dialog();
    }
}
