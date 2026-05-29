impl WorkspaceApp {
    fn spawn_remote_sftp_mutation<F>(&self, operation: F, toast: Option<SftpMutationToast>)
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
                toast,
            });
        });
    }

    fn push_sftp_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
    ) {
        let _ = self.terminal_notice_tx.send(TerminalNotice {
            title,
            description,
            status_text: None,
            progress: None,
            variant,
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
                            match std::fs::rename(old_path, new_path) {
                                Ok(()) => {
                                    if let Ok(files) = list_local_files(&self.sftp_view.local_path)
                                    {
                                        self.sftp_view.local_files = files;
                                    }
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.renamed"),
                                        Some(sftp_i18n_rename_detail(
                                            self.i18n.t("sftp.toast.renamed_detail"),
                                            &old_name,
                                            &new_name,
                                        )),
                                        TerminalNoticeVariant::Success,
                                    );
                                }
                                Err(error) => {
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.rename_failed"),
                                        Some(error.to_string()),
                                        TerminalNoticeVariant::Error,
                                    );
                                }
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
                            let toast = SftpMutationToast {
                                success_title: self.i18n.t("sftp.toast.renamed"),
                                success_description: Some(sftp_i18n_rename_detail(
                                    self.i18n.t("sftp.toast.renamed_detail"),
                                    &old_name,
                                    &new_name,
                                )),
                                error_title: self.i18n.t("sftp.toast.rename_failed"),
                            };
                            self.spawn_remote_sftp_mutation(
                                move |sftp| {
                                    Box::pin(async move {
                                        sftp.rename(&old_path, &new_path)
                                            .await
                                            .map_err(|error| error.to_string())
                                    })
                                },
                                Some(toast),
                            );
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
                            match std::fs::create_dir_all(path) {
                                Ok(()) => {
                                    if let Ok(files) = list_local_files(&self.sftp_view.local_path)
                                    {
                                        self.sftp_view.local_files = files;
                                    }
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.folder_created"),
                                        Some(name),
                                        TerminalNoticeVariant::Success,
                                    );
                                }
                                Err(error) => {
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.create_folder_failed"),
                                        Some(error.to_string()),
                                        TerminalNoticeVariant::Error,
                                    );
                                }
                            }
                        }
                        SftpPane::Remote => {
                            let path = join_sftp_path(&self.sftp_view.remote_path, &name);
                            let toast = SftpMutationToast {
                                success_title: self.i18n.t("sftp.toast.folder_created"),
                                success_description: Some(name),
                                error_title: self.i18n.t("sftp.toast.create_folder_failed"),
                            };
                            self.spawn_remote_sftp_mutation(
                                move |sftp| {
                                    Box::pin(async move {
                                        sftp.mkdir(&path).await.map_err(|error| error.to_string())
                                    })
                                },
                                Some(toast),
                            );
                        }
                    }
                }
            }
            SftpDialog::Delete { pane, files } => {
                match pane {
                    SftpPane::Local => {
                        let count = files.len();
                        let mut result = Ok(());
                        for name in files {
                            let path = join_local_path(&self.sftp_view.local_path, &name);
                            result = if std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_dir()) {
                                std::fs::remove_dir_all(path)
                            } else {
                                std::fs::remove_file(path)
                            };
                            if result.is_err() {
                                break;
                            }
                        }
                        match result {
                            Ok(()) => {
                                if let Ok(files) = list_local_files(&self.sftp_view.local_path) {
                                    self.sftp_view.local_files = files;
                                }
                                self.push_sftp_toast(
                                    self.i18n.t("sftp.toast.deleted"),
                                    Some(sftp_i18n_count(
                                        self.i18n.t("sftp.toast.deleted_count"),
                                        count,
                                    )),
                                    TerminalNoticeVariant::Success,
                                );
                            }
                            Err(error) => {
                                self.push_sftp_toast(
                                    self.i18n.t("sftp.toast.delete_failed"),
                                    Some(error.to_string()),
                                    TerminalNoticeVariant::Error,
                                );
                            }
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
                        let Some(tab_id) = self.active_tab_id else {
                            self.close_sftp_dialog();
                            return;
                        };
                        let Some(node_id) = self.sftp_tab_nodes.get(&tab_id).cloned() else {
                            self.close_sftp_dialog();
                            return;
                        };
                        let router = self.node_router.clone();
                        let tx = self.sftp_worker_tx.clone();
                        let runtime = self.forwarding_runtime.clone();
                        let success_title = self.i18n.t("sftp.toast.deleted");
                        let success_template = self.i18n.t("sftp.toast.deleted_count");
                        let error_title = self.i18n.t("sftp.toast.delete_failed");
                        runtime.spawn(async move {
                            let result = async {
                                let sftp = router
                                    .acquire_transfer_sftp(&node_id)
                                    .await
                                    .map_err(|error| error.to_string())?;
                                let mut deleted = 0_u64;
                                for path in targets {
                                    // Tauri nodeSftpDeleteRecursive returns the
                                    // recursive item count; keep the success
                                    // toast tied to the same backend count.
                                    deleted = deleted
                                        .saturating_add(sftp.delete_recursive(&path).await.map_err(
                                            |error| error.to_string(),
                                        )?);
                                }
                                Ok(deleted)
                            }
                            .await;
                            let (result, toast) = match result {
                                Ok(deleted) => (
                                    Ok(()),
                                    Some(SftpMutationToast {
                                        success_title,
                                        success_description: Some(sftp_i18n_count(
                                            success_template,
                                            deleted.try_into().unwrap_or(usize::MAX),
                                        )),
                                        error_title,
                                    }),
                                ),
                                Err(error) => (
                                    Err(error),
                                    Some(SftpMutationToast {
                                        success_title,
                                        success_description: None,
                                        error_title,
                                    }),
                                ),
                            };
                            let _ = tx.send(SftpWorkerResult::RemoteMutationComplete {
                                result,
                                refresh_remote: true,
                                refresh_local: false,
                                toast,
                            });
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

fn sftp_i18n_count(template: String, count: usize) -> String {
    template.replace("{{count}}", &count.to_string())
}

fn sftp_i18n_rename_detail(template: String, old_name: &str, new_name: &str) -> String {
    template
        .replace("{{old}}", old_name)
        .replace("{{new}}", new_name)
}
