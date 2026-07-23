use super::external::open_path_in_external_app;
use super::*;

impl WorkspaceApp {
    pub(in crate::workspace::sftp) fn open_or_preview_sftp_file(
        &mut self,
        pane: SftpPane,
        file: &SftpFileEntry,
    ) {
        self.sftp_view.active_pane = pane;
        self.dismiss_sftp_context_menu();
        if file.file_type == SftpFileType::Directory {
            let base = match pane {
                SftpPane::Local => self.sftp_view.local_path.clone(),
                SftpPane::Remote => self.sftp_view.remote_path.clone(),
            };
            self.set_sftp_path(pane, join_sftp_path(&base, &file.name));
        } else if pane == SftpPane::Remote {
            self.stop_sftp_preview_media();
            self.sftp_view.preview_generation = self.sftp_view.preview_generation.wrapping_add(1);
            let generation = self.sftp_view.preview_generation;
            self.reset_sftp_preview_editor();
            self.sftp_view.preview_pane = Some(pane);
            self.sftp_view.preview_path = Some(file.path.clone());
            self.sftp_view.preview_content = None;
            self.sftp_view.preview_asset_owner = None;
            self.sftp_view.preview_session = PreviewSession::loading();
            self.sftp_view.preview_code_scroll = UniformListScrollHandle::new();
            self.sftp_view.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
            self.sftp_view.preview_error = None;
            self.sftp_view.preview_loading = pane == SftpPane::Remote;
            self.sftp_view.preview_hex_loading_more = false;
            self.sftp_view.preview_markdown_source_mode = false;
            self.sftp_view.preview_font_family = None;
            self.sftp_view.preview_font_error = None;
            self.sftp_view.preview_font_size = SFTP_PREVIEW_FONT_DEFAULT_SIZE;
            self.sftp_view.set_dialog(SftpDialog::Preview {
                name: file.name.clone(),
            });
            self.spawn_remote_sftp_preview(file.path.clone(), generation);
        }
    }

    pub(in crate::workspace::sftp) fn can_compare_sftp_preview(&self, name: &str) -> bool {
        if self.sftp_view.preview_pane != Some(SftpPane::Remote) {
            return false;
        }
        matches!(
            self.sftp_view.preview_content.as_ref(),
            Some(PreviewContent::Text { .. })
        ) && self
            .sftp_view
            .local_files
            .iter()
            .any(|file| file.name == name && file.file_type == SftpFileType::File)
    }

    pub(in crate::workspace::sftp) fn can_edit_sftp_preview(&self) -> bool {
        self.sftp_view.preview_pane == Some(SftpPane::Remote)
            && matches!(
                self.sftp_view.preview_content.as_ref(),
                Some(PreviewContent::Text { .. })
            )
    }

    pub(in crate::workspace::sftp) fn sftp_preview_is_markdown_content(&self) -> bool {
        matches!(
            self.sftp_view.preview_content.as_ref(),
            Some(PreviewContent::Text {
                language,
                mime_type,
                ..
            }) if sftp_preview_is_markdown(language.as_deref(), mime_type.as_deref())
        )
    }

    pub(in crate::workspace::sftp) fn open_sftp_preview_editor(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sftp_view.preview_pane != Some(SftpPane::Remote) {
            return;
        }
        let Some(PreviewContent::Text {
            data,
            language,
            encoding,
            ..
        }) = self.sftp_view.preview_content.clone()
        else {
            return;
        };

        self.stop_sftp_preview_media();
        let editor_language = sftp_editor_language(language.as_deref(), name);
        let syntax_language = sftp_editor_language_id(
            language.as_deref(),
            self.sftp_view.preview_path.as_deref(),
            name,
            &data,
        );
        let tokens = self.tokens;
        let runtime_settings = self.ide_runtime_settings();
        let context_menu_labels = EditorContextMenuLabels {
            copy: self.i18n.t("menu.copy"),
            cut: self.i18n.t("fileManager.cut"),
            paste: self.i18n.t("menu.paste"),
            select_all: self.i18n.t("fileManager.selectAll"),
        };
        let workspace = cx.entity();
        let (editor_text, line_ending) = normalize_text_line_endings(&data);
        let initial_editor_text = editor_text.clone();
        let editor = cx.new(|cx| {
            let mut editor = TextEditorView::new(editor_text, &tokens, cx);
            editor.set_context_menu_labels(context_menu_labels);
            editor.apply_ide_runtime_settings(
                &tokens,
                runtime_settings.editor_font_size,
                runtime_settings.editor_line_height,
                runtime_settings.word_wrap,
                runtime_settings.background_active,
                cx,
            );
            editor.set_language(syntax_language, cx);
            editor.set_on_save(Box::new(move |text, _window, cx| {
                let text = text.to_string();
                let _ = workspace.update(cx, |this, _cx| {
                    this.save_sftp_preview_editor_content(text);
                });
                Ok(())
            }));
            editor
        });
        let observer = cx.observe(&editor, |this: &mut WorkspaceApp, editor, cx| {
            this.sync_sftp_preview_editor_state(&editor, cx);
            cx.notify();
        });
        let focus_handle = editor.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);

        self.sftp_view.preview_editor = Some(editor);
        self.sftp_view.preview_editor_observer = Some(observer);
        self.sftp_view.preview_editor_initial_content = initial_editor_text.clone();
        self.sftp_view.preview_editor_observed_content = initial_editor_text;
        self.sftp_view.preview_editor_language = Some(editor_language);
        self.sftp_view.preview_editor_encoding = encoding;
        self.sftp_view.preview_editor_line_ending = line_ending;
        self.sftp_view.preview_editor_dirty = false;
        self.sftp_view.preview_editor_saving = false;
        self.sftp_view.preview_editor_save_error = None;
        self.sftp_view.preview_editor_network_error = false;
        self.sftp_view.preview_editor_retry_count = 0;
        self.sftp_view.preview_editor_last_saved_mtime = None;
        self.sftp_view.preview_editor_last_atomic_write = None;
        self.sftp_view.set_dialog(SftpDialog::Editor {
            name: name.to_string(),
        });
    }

    pub(in crate::workspace::sftp) fn save_sftp_preview_editor(&mut self, cx: &mut Context<Self>) {
        if self.sftp_view.preview_editor_saving {
            return;
        }
        let Some(editor) = self.sftp_view.preview_editor.clone() else {
            return;
        };
        self.sync_sftp_preview_editor_state(&editor, cx);
        let content = editor.read(cx).buffer().text();
        self.save_sftp_preview_editor_content(content);
    }

    fn save_sftp_preview_editor_content(&mut self, content: String) {
        if self.sftp_view.preview_editor_saving {
            return;
        }
        self.sftp_view.preview_editor_dirty =
            content != self.sftp_view.preview_editor_initial_content;
        self.sftp_view.preview_editor_observed_content = content.clone();
        if !self.sftp_view.preview_editor_dirty {
            return;
        }
        let Some(path) = self.sftp_view.preview_path.clone() else {
            return;
        };
        let can_spawn = self
            .main_window_tabs
            .active_tab_id
            .and_then(|tab_id| self.sftp_tab_nodes.get(&tab_id))
            .is_some();
        if !can_spawn {
            self.sftp_view.preview_editor_save_error =
                Some(self.i18n.t("sftp.errors.connection_lost"));
            return;
        }
        let encoding = self.sftp_view.preview_editor_encoding.clone();
        let line_ending = self.sftp_view.preview_editor_line_ending;
        self.sftp_view.preview_editor_saving = true;
        self.sftp_view.preview_editor_save_error = None;
        self.sftp_view.preview_editor_network_error = false;
        self.sftp_view.preview_generation = self.sftp_view.preview_generation.wrapping_add(1);
        let generation = self.sftp_view.preview_generation;
        self.spawn_remote_sftp_preview_save(path, content, encoding, line_ending, generation);
    }

    fn sync_sftp_preview_editor_state(
        &mut self,
        editor: &Entity<TextEditorView>,
        cx: &mut Context<Self>,
    ) {
        let content = editor.read(cx).buffer().text();
        let content_changed = content != self.sftp_view.preview_editor_observed_content;
        self.sftp_view.preview_editor_dirty =
            content != self.sftp_view.preview_editor_initial_content;
        if content_changed {
            // Editor notifications also cover cursor-only movement; only content edits should clear save errors.
            self.sftp_view.preview_editor_observed_content = content;
            self.sftp_view.preview_editor_save_error = None;
            self.sftp_view.preview_editor_network_error = false;
            self.sftp_view.preview_editor_last_atomic_write = None;
        }
    }

    pub(in crate::workspace::sftp) fn retry_sftp_preview_editor_save(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        if self.sftp_view.preview_editor_saving {
            return;
        }
        self.sftp_view.preview_editor_retry_count =
            self.sftp_view.preview_editor_retry_count.saturating_add(1);
        self.sftp_view.preview_editor_network_error = false;
        self.sftp_view.preview_editor_save_error = None;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(500))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.save_sftp_preview_editor(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(in crate::workspace::sftp) fn request_close_sftp_editor(&mut self) {
        let name = match self.sftp_view.dialog.clone() {
            Some(SftpDialog::Editor { name }) => name,
            Some(SftpDialog::EditorCloseConfirm { name }) => name,
            _ => return,
        };
        if self.sftp_view.preview_editor_dirty {
            self.sftp_view
                .set_dialog(SftpDialog::EditorCloseConfirm { name });
        } else {
            self.close_sftp_dialog();
        }
    }

    pub(in crate::workspace::sftp) fn cancel_sftp_editor_close_confirm(&mut self, name: String) {
        self.sftp_view.set_dialog(SftpDialog::Editor { name });
    }

    pub(in crate::workspace::sftp) fn discard_sftp_editor_changes(&mut self) {
        self.close_sftp_dialog();
    }

    pub(in crate::workspace::sftp) fn download_sftp_preview(&mut self, name: &str) {
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let Some(remote_path) = self.sftp_view.preview_path.clone() else {
            return;
        };
        let local_path = join_local_path(&self.sftp_view.local_path, name);
        let size = self
            .sftp_view
            .remote_files
            .iter()
            .find(|file| file.path == remote_path)
            .map(|file| file.size)
            .unwrap_or_default()
            .max(1);
        let id = self.sftp_view.next_transfer_id;
        self.sftp_view.next_transfer_id += 1;
        let transfer_id = new_sftp_transfer_id(&node_id, name);
        self.sftp_view.transfers.push(SftpTransferItem {
            id,
            transfer_id: transfer_id.clone(),
            batch_id: None,
            node_id: node_id.clone(),
            name: name.to_string(),
            local_path: local_path.clone(),
            remote_path: remote_path.clone(),
            direction: SftpTransferDirection::Download,
            protocol: configured_transfer_protocol(
                self.settings_store.settings().sftp.transfer_protocol,
            ),
            size,
            transferred: 0,
            speed: 0,
            state: SftpTransferState::Pending,
            error: None,
        });
        self.spawn_sftp_transfer_task(
            id,
            transfer_id,
            node_id,
            SftpTransferDirection::Download,
            false,
            local_path,
            remote_path,
            None,
            None,
        );
    }

    pub(in crate::workspace::sftp) fn open_sftp_preview_compare(&mut self, name: &str) {
        if !self.can_compare_sftp_preview(name) {
            return;
        }
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
                self.sftp_view.diff_scroll = UniformListScrollHandle::new();
                self.sftp_view.set_dialog(SftpDialog::Diff {
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

    pub(in crate::workspace::sftp) fn open_sftp_preview_external(&mut self, path: &str) {
        if let Err(error) = open_path_in_external_app(path) {
            self.sftp_view.preview_error = Some(format!(
                "{}: {}",
                self.i18n.t("sftp.toast.open_external_failed"),
                error
            ));
        }
    }

    fn spawn_remote_sftp_preview(&self, path: String, generation: u64) {
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
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
            let _ = tx.send(SftpWorkerResult::PreviewLoaded {
                generation,
                path,
                result,
            });
        });
    }

    pub(in crate::workspace::sftp) fn load_more_sftp_preview_hex(&mut self) {
        if self.sftp_view.preview_loading || self.sftp_view.preview_hex_loading_more {
            return;
        }
        let Some(path) = self.sftp_view.preview_path.clone() else {
            return;
        };
        let Some(PreviewContent::Hex {
            offset, has_more, ..
        }) = self.sftp_view.preview_content.as_ref()
        else {
            return;
        };
        if !*has_more {
            return;
        }
        let next_offset = offset.saturating_add(SFTP_HEX_PREVIEW_CHUNK_SIZE);
        self.sftp_view.preview_hex_loading_more = true;
        self.sftp_view.preview_error = None;
        self.spawn_remote_sftp_preview_hex(path, next_offset, self.sftp_view.preview_generation);
    }

    fn spawn_remote_sftp_preview_hex(&self, path: String, offset: u64, generation: u64) {
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let router = self.node_router.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = load_remote_sftp_preview_hex(router, &node_id, &path, offset).await;
            let _ = tx.send(SftpWorkerResult::PreviewHexLoaded {
                generation,
                path,
                offset,
                result,
            });
        });
    }

    fn spawn_remote_sftp_preview_save(
        &self,
        path: String,
        content: String,
        encoding: String,
        line_ending: TextLineEnding,
        generation: u64,
    ) {
        let Some(tab_id) = self.main_window_tabs.active_tab_id else {
            return;
        };
        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
            return;
        };
        let router = self.node_router.clone();
        let tx = self.sftp_worker_tx.clone();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result =
                save_remote_sftp_preview(router, &node_id, &path, &content, &encoding, line_ending)
                    .await;
            let _ = tx.send(SftpWorkerResult::PreviewSaved {
                generation,
                path,
                content,
                encoding,
                result,
            });
        });
    }
}
