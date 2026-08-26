use super::external::open_path_in_external_app;
use super::*;

impl WorkspaceApp {
    pub(in crate::workspace) fn open_or_preview_sftp_file(
        &mut self,
        pane: SftpPane,
        file: &SftpFileEntry,
        cx: &mut Context<Self>,
    ) {
        if file.file_type == SftpFileType::Directory {
            let base = match pane {
                SftpPane::Local => self.sftp_view.read(cx).local_path.clone(),
                SftpPane::Remote => self.sftp_view.read(cx).remote_path.clone(),
            };
            self.set_sftp_path(pane, join_sftp_path(&base, &file.name), cx);
        } else if pane == SftpPane::Remote {
            let generation = self.sftp_view.update(cx, |sftp, cx| {
                sftp.active_pane = pane;
                sftp.clear_context_menu_immediately();
                sftp.stop_preview_media();
                sftp.preview_generation = sftp.preview_generation.wrapping_add(1);
                sftp.reset_preview_editor();
                sftp.preview_pane = Some(pane);
                sftp.preview_path = Some(file.path.clone());
                sftp.preview_content = None;
                sftp.preview_asset_owner = None;
                sftp.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
                sftp.preview_document_scroll = ScrollHandle::new();
                sftp.font_preview_scroll = ScrollHandle::new();
                sftp.preview_error = None;
                sftp.preview_loading = true;
                sftp.preview_hex_loading_more = false;
                sftp.preview_markdown_source_mode = false;
                sftp.preview_font_family = None;
                sftp.preview_font_error = None;
                sftp.preview_font_size = SFTP_PREVIEW_FONT_DEFAULT_SIZE;
                sftp.set_dialog(SftpDialog::Preview {
                    name: file.name.clone(),
                });
                cx.notify();
                sftp.preview_generation
            });
            self.spawn_remote_sftp_preview(file.path.clone(), generation, cx);
        }
    }

    pub(in crate::workspace::sftp) fn can_compare_sftp_preview(
        &self,
        name: &str,
        cx: &App,
    ) -> bool {
        let sftp = self.sftp_view.read(cx);
        if sftp.preview_pane != Some(SftpPane::Remote) {
            return false;
        }
        matches!(
            sftp.preview_content.as_deref(),
            Some(PreviewContent::Text { .. })
        ) && sftp
            .local_files
            .iter()
            .any(|file| file.name == name && file.file_type == SftpFileType::File)
    }

    pub(in crate::workspace::sftp) fn can_edit_sftp_preview(&self, cx: &App) -> bool {
        let sftp = self.sftp_view.read(cx);
        sftp.preview_pane == Some(SftpPane::Remote)
            && matches!(
                sftp.preview_content.as_deref(),
                Some(PreviewContent::Text { .. })
            )
    }

    pub(in crate::workspace::sftp) fn sftp_preview_is_markdown_content(&self, cx: &App) -> bool {
        matches!(
            self.sftp_view.read(cx).preview_content.as_deref(),
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
        let Some((data, language, encoding, preview_path)) = ({
            let sftp = self.sftp_view.read(cx);
            if sftp.preview_pane != Some(SftpPane::Remote) {
                None
            } else {
                match sftp.preview_content.as_deref() {
                    Some(PreviewContent::Text {
                        data,
                        language,
                        encoding,
                        ..
                    }) => Some((
                        data.clone(),
                        language.clone(),
                        encoding.clone(),
                        sftp.preview_path.clone(),
                    )),
                    _ => None,
                }
            }
        }) else {
            return;
        };

        self.stop_sftp_preview_media(cx);
        let editor_language = sftp_editor_language(language.as_deref(), name);
        let syntax_language =
            sftp_editor_language_id(language.as_deref(), preview_path.as_deref(), name, &data);
        let tokens = self.tokens;
        let runtime_settings = self.ide_runtime_settings();
        let context_menu_labels = EditorContextMenuLabels {
            copy: self.i18n.t("menu.copy"),
            cut: self.i18n.t("fileManager.cut"),
            paste: self.i18n.t("menu.paste"),
            select_all: self.i18n.t("fileManager.selectAll"),
        };
        let sftp_entity = self.sftp_view.clone();
        let (editor_text, line_ending) = normalize_text_line_endings(&data);
        let initial_editor_text: Arc<str> = Arc::from(editor_text.as_str());
        let existing_editor = self.sftp_view.read(cx).preview_editor.clone();
        let configure_editor =
            move |editor: &mut TextEditorView, cx: &mut Context<TextEditorView>| {
                editor.set_read_only(false);
                editor.set_context_menu_labels(context_menu_labels);
                editor.apply_ide_runtime_settings(
                    &tokens,
                    runtime_settings.editor_font_fallback.clone(),
                    runtime_settings.editor_font_size,
                    runtime_settings.editor_line_height,
                    runtime_settings.word_wrap,
                    runtime_settings.background_active,
                    cx,
                );
                editor.set_language(syntax_language, cx);
                editor.set_on_save(Box::new(move |text, _window, cx| {
                    let text = text.to_string();
                    let _ = sftp_entity.update(cx, |sftp, cx| {
                        sftp.save_preview_editor_content(text, cx);
                    });
                    Ok(())
                }));
            };
        let editor = if let Some(editor) = existing_editor {
            // Reusing the painted read-only preview preserves the Windows IME
            // input handler across the transition into editable mode.
            editor.update(cx, configure_editor);
            editor
        } else {
            cx.new(|cx| {
                let mut editor = TextEditorView::new(editor_text, &tokens, cx);
                configure_editor(&mut editor, cx);
                editor
            })
        };
        let observer = self.sftp_view.update(cx, |_sftp, cx| {
            cx.observe(&editor, |sftp, editor, cx| {
                sftp.sync_preview_editor_state(&editor, cx);
            })
        });
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.preview_editor = Some(editor.clone());
            sftp.preview_editor_observer = Some(observer);
            sftp.preview_editor_initial_content = initial_editor_text.clone();
            sftp.preview_editor_observed_content = initial_editor_text;
            sftp.preview_editor_language = Some(editor_language);
            sftp.preview_editor_encoding = encoding;
            sftp.preview_editor_line_ending = line_ending;
            sftp.preview_editor_dirty = false;
            sftp.preview_editor_saving = false;
            sftp.preview_editor_save_error = None;
            sftp.preview_editor_network_error = false;
            sftp.preview_editor_retry_count = 0;
            sftp.preview_editor_last_saved_mtime = None;
            sftp.preview_editor_last_atomic_write = None;
            sftp.set_dialog(SftpDialog::Editor {
                name: name.to_string(),
            });
            cx.notify();
        });
        // Commit modal ownership before focusing so root key routing observes
        // the editor and its FocusId in the same interaction.
        let focus_handle = editor.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
    }

    pub(in crate::workspace::sftp) fn save_sftp_preview_editor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = ({
            let sftp = self.sftp_view.read(cx);
            (!sftp.preview_editor_saving)
                .then(|| sftp.preview_editor.clone())
                .flatten()
        }) else {
            return;
        };
        let content = editor.read(cx).buffer().text();
        self.sftp_view.update(cx, |sftp, cx| {
            sftp.sync_preview_editor_state(&editor, cx);
            sftp.save_preview_editor_content(content, cx);
        });
    }

    pub(in crate::workspace::sftp) fn retry_sftp_preview_editor_save(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view
            .update(cx, |sftp, cx| sftp.retry_preview_editor_save(cx));
    }

    pub(in crate::workspace::sftp) fn request_close_sftp_editor(&mut self, cx: &mut Context<Self>) {
        let (name, dirty) = {
            let sftp = self.sftp_view.read(cx);
            let name = match sftp.dialog.clone() {
                Some(SftpDialog::Editor { name }) => name,
                Some(SftpDialog::EditorCloseConfirm { name }) => name,
                _ => return,
            };
            (name, sftp.preview_editor_dirty)
        };
        if dirty {
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.set_dialog(SftpDialog::EditorCloseConfirm { name });
                cx.notify();
            });
        } else {
            self.close_sftp_dialog(cx);
        }
    }

    pub(in crate::workspace::sftp) fn cancel_sftp_editor_close_confirm(
        &mut self,
        name: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = self.sftp_view.update(cx, |sftp, cx| {
            sftp.set_dialog(SftpDialog::Editor { name });
            cx.notify();
            sftp.preview_editor.clone()
        });
        if let Some(editor) = editor {
            // Cancel returns ownership to the same document editor for mouse,
            // backdrop, and keyboard dismissal paths.
            let focus_handle = editor.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
        }
    }

    pub(in crate::workspace::sftp) fn discard_sftp_editor_changes(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        self.close_sftp_dialog(cx);
    }

    pub(in crate::workspace::sftp) fn download_sftp_preview(
        &mut self,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let Some((remote_path, local_path, size)) = ({
            let sftp = self.sftp_view.read(cx);
            sftp.preview_path.clone().map(|remote_path| {
                let local_path = join_local_path(&sftp.local_path, name);
                let size = sftp
                    .remote_files
                    .iter()
                    .find(|file| file.path == remote_path)
                    .map(|file| file.size)
                    .unwrap_or_default()
                    .max(1);
                (remote_path, local_path, size)
            })
        }) else {
            return;
        };
        let transfer_id = new_sftp_transfer_id(&remote_id, name);
        let protocol =
            configured_transfer_protocol(self.settings_store.settings().sftp.transfer_protocol);
        let id = self.sftp_view.update(cx, |sftp, cx| {
            let id = sftp.next_transfer_id;
            sftp.next_transfer_id += 1;
            sftp.transfers.push(SftpTransferItem {
                id,
                transfer_id: transfer_id.clone(),
                batch_id: None,
                remote_id: remote_id.clone(),
                name: name.to_string(),
                local_path: local_path.clone(),
                remote_path: remote_path.clone(),
                direction: SftpTransferDirection::Download,
                protocol,
                size,
                transferred: 0,
                speed: 0,
                state: SftpTransferState::Pending,
                error: None,
            });
            cx.notify();
            id
        });
        self.spawn_sftp_transfer_task(
            id,
            transfer_id,
            remote_id,
            SftpTransferDirection::Download,
            false,
            local_path,
            remote_path,
            None,
            LocalDownloadDisposition::CreateNew,
            None,
            cx,
        );
    }

    pub(in crate::workspace::sftp) fn open_sftp_preview_compare(
        &mut self,
        name: &str,
        cx: &mut Context<Self>,
    ) {
        if !self.can_compare_sftp_preview(name, cx) {
            return;
        }
        let Some((remote_content, local_file, remote_path)) = ({
            let sftp = self.sftp_view.read(cx);
            let remote_content = match sftp.preview_content.as_deref() {
                Some(PreviewContent::Text { data, .. }) => Some(data.clone()),
                _ => None,
            };
            remote_content.and_then(|remote_content| {
                sftp.local_files
                    .iter()
                    .find(|file| file.name == name && file.file_type == SftpFileType::File)
                    .cloned()
                    .map(|local_file| {
                        (
                            remote_content,
                            local_file,
                            sftp.preview_path.clone().unwrap_or_default(),
                        )
                    })
            })
        }) else {
            let error = format!(
                "{}: {}",
                self.i18n.t("sftp.toast.compare_failed"),
                self.i18n.t("sftp.toast.compare_no_local")
            );
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.preview_error = Some(error);
                cx.notify();
            });
            return;
        };

        match std::fs::read_to_string(&local_file.path) {
            Ok(local_content) => {
                self.sftp_view.update(cx, |sftp, cx| {
                    sftp.diff_scroll = UniformListScrollHandle::new();
                    sftp.diff_document_scroll = ScrollHandle::new();
                    sftp.set_dialog(SftpDialog::Diff {
                        local_path: local_file.path,
                        local_content,
                        remote_path,
                        remote_content,
                    });
                    cx.notify();
                });
            }
            Err(error) => {
                let error = format!("{}: {}", self.i18n.t("sftp.toast.compare_failed"), error);
                self.sftp_view.update(cx, |sftp, cx| {
                    sftp.preview_error = Some(error);
                    cx.notify();
                });
            }
        }
    }

    pub(in crate::workspace::sftp) fn open_sftp_preview_external(
        &mut self,
        path: &str,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = open_path_in_external_app(path) {
            let error = format!(
                "{}: {}",
                self.i18n.t("sftp.toast.open_external_failed"),
                error
            );
            self.sftp_view.update(cx, |sftp, cx| {
                sftp.preview_error = Some(error);
                cx.notify();
            });
        }
    }

    fn spawn_remote_sftp_preview(&self, path: String, generation: u64, cx: &App) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = self.sftp_view.read(cx).worker_sender();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = load_remote_sftp_preview(backend, &path).await;
            let _ = tx.send(SftpWorkerResult::PreviewLoaded {
                generation,
                path,
                result,
            });
        });
    }

    pub(in crate::workspace::sftp) fn load_more_sftp_preview_hex(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let request = self.sftp_view.update(cx, |sftp, cx| {
            if sftp.preview_loading || sftp.preview_hex_loading_more {
                return None;
            }
            let path = sftp.preview_path.clone()?;
            let PreviewContent::Hex {
                offset, has_more, ..
            } = sftp.preview_content.as_deref()?
            else {
                return None;
            };
            if !*has_more {
                return None;
            }
            let next_offset = offset.saturating_add(SFTP_HEX_PREVIEW_CHUNK_SIZE);
            sftp.preview_hex_loading_more = true;
            sftp.preview_error = None;
            cx.notify();
            Some((path, next_offset, sftp.preview_generation))
        });
        if let Some((path, next_offset, generation)) = request {
            self.spawn_remote_sftp_preview_hex(path, next_offset, generation, cx);
        }
    }

    fn spawn_remote_sftp_preview_hex(&self, path: String, offset: u64, generation: u64, cx: &App) {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = self.sftp_view.read(cx).worker_sender();
        let error_prefix = self.i18n.t("sftp.toast.load_more_failed");
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = load_remote_sftp_preview_hex(backend, &path, offset).await;
            let _ = tx.send(SftpWorkerResult::PreviewHexLoaded {
                generation,
                path,
                error_prefix,
                result,
            });
        });
    }

    pub(in crate::workspace) fn spawn_remote_sftp_preview_save(
        &self,
        path: String,
        content: Arc<str>,
        encoding: Arc<str>,
        line_ending: TextLineEnding,
        generation: u64,
        tx: delivery::ActiveDeliverySender<SftpWorkerResult>,
        cx: &App,
    ) -> bool {
        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
            return false;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return false;
        };
        let network_error_message = self.i18n.t("sftp.preview.network_error");
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = save_remote_sftp_preview(
                backend,
                &path,
                content.as_ref(),
                encoding.as_ref(),
                line_ending,
            )
            .await;
            let _ = tx.send(SftpWorkerResult::PreviewSaved {
                generation,
                path,
                content,
                network_error_message,
                result,
            });
        });
        true
    }
}

impl SftpWorkspaceEntity {
    fn sync_preview_editor_state(
        &mut self,
        editor: &Entity<TextEditorView>,
        cx: &mut Context<Self>,
    ) {
        let content = editor.read(cx).buffer().text();
        let content_changed = content.as_str() != self.preview_editor_observed_content.as_ref();
        self.preview_editor_dirty =
            content.as_str() != self.preview_editor_initial_content.as_ref();
        if content_changed {
            // Editor notifications include cursor-only movement. Only content
            // changes clear a previous save failure.
            self.preview_editor_observed_content = Arc::from(content);
            self.preview_editor_save_error = None;
            self.preview_editor_network_error = false;
            self.preview_editor_last_atomic_write = None;
            cx.notify();
        }
    }

    fn save_preview_editor_content(&mut self, content: String, cx: &mut Context<Self>) {
        if self.preview_editor_saving {
            return;
        }
        self.preview_editor_dirty =
            content.as_str() != self.preview_editor_initial_content.as_ref();
        self.preview_editor_observed_content = Arc::from(content.as_str());
        if !self.preview_editor_dirty {
            return;
        }
        let Some(path) = self.preview_path.clone() else {
            return;
        };
        self.preview_editor_saving = true;
        self.preview_editor_save_error = None;
        self.preview_editor_network_error = false;
        self.preview_generation = self.preview_generation.wrapping_add(1);
        cx.emit(SftpWorkspaceEvent::PreviewSaveRequested {
            path,
            content: Arc::<str>::from(content),
            encoding: Arc::<str>::from(self.preview_editor_encoding.as_str()),
            line_ending: self.preview_editor_line_ending,
            generation: self.preview_generation,
            delivery: self.worker_tx.clone(),
        });
        cx.notify();
    }

    fn retry_preview_editor_save(&mut self, cx: &mut Context<Self>) {
        if self.preview_editor_saving {
            return;
        }
        let Some(editor) = self.preview_editor.clone() else {
            return;
        };
        self.preview_editor_retry_count = self.preview_editor_retry_count.saturating_add(1);
        self.preview_editor_network_error = false;
        self.preview_editor_save_error = None;
        self.preview_editor_retry_task = Some(cx.spawn(async move |entity, cx| {
            gpui::Timer::after(Duration::from_millis(500)).await;
            let _ = entity.update(cx, |sftp, cx| {
                sftp.preview_editor_retry_task = None;
                sftp.sync_preview_editor_state(&editor, cx);
                let content = editor.read(cx).buffer().text();
                sftp.save_preview_editor_content(content, cx);
            });
        }));
        cx.notify();
    }
}
