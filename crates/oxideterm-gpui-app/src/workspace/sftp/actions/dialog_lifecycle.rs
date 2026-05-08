impl WorkspaceApp {
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
        self.sftp_view.preview_generation = self.sftp_view.preview_generation.wrapping_add(1);
        self.sftp_view.dialog = None;
        self.sftp_view.conflict_state = None;
        self.sftp_view.dialog_value.clear();
        self.sftp_view.preview_asset_owner = None;
        self.sftp_view.preview_session = PreviewSession::default();
        self.sftp_view.preview_hex_loading_more = false;
        self.sftp_view.preview_markdown_source_mode = false;
        self.sftp_view.preview_markdown_scroll = MarkdownVirtualListScrollHandle::new();
        self.sftp_view.preview_font_family = None;
        self.sftp_view.preview_font_error = None;
        self.sftp_view.preview_font_size = SFTP_PREVIEW_FONT_DEFAULT_SIZE;
        self.reset_sftp_preview_editor();
        self.sftp_view.focused_input = None;
        self.ime_marked_text = None;
    }

    fn reset_sftp_preview_editor(&mut self) {
        self.sftp_view.preview_editor_input = None;
        self.sftp_view.preview_editor_subscription = None;
        self.sftp_view.preview_editor_initial_content.clear();
        self.sftp_view.preview_editor_language = None;
        self.sftp_view.preview_editor_encoding = "UTF-8".to_string();
        self.sftp_view.preview_editor_dirty = false;
        self.sftp_view.preview_editor_saving = false;
        self.sftp_view.preview_editor_save_error = None;
        self.sftp_view.preview_editor_network_error = false;
        self.sftp_view.preview_editor_retry_count = 0;
        self.sftp_view.preview_editor_last_saved_mtime = None;
        self.sftp_view.preview_editor_last_atomic_write = None;
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
            SftpDialog::Conflict => {
                self.resolve_sftp_transfer_conflict(SftpConflictResolution::Overwrite);
                return;
            }
            _ => {}
        }
        self.close_sftp_dialog();
    }
}
