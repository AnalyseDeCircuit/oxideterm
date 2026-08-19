use super::*;

impl WorkspaceApp {
    fn spawn_remote_sftp_mutation<F>(
        &self,
        operation: F,
        toast: Option<SftpMutationToast>,
        cx: &App,
    ) where
        F: FnOnce(
                SftpSession,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
            > + Send
            + 'static,
    {
        self.spawn_sftp_pane_remote_mutation(SftpPane::Remote, operation, toast, cx);
    }

    fn spawn_sftp_pane_remote_mutation<F>(
        &self,
        pane: SftpPane,
        operation: F,
        toast: Option<SftpMutationToast>,
        cx: &App,
    ) where
        F: FnOnce(
                SftpSession,
            ) -> std::pin::Pin<
                Box<dyn std::future::Future<Output = Result<(), String>> + Send>,
            > + Send
            + 'static,
    {
        let remote_id = match pane {
            SftpPane::Local => self.sftp_pair_primary_remote_id(cx),
            SftpPane::Remote => self.visible_sftp_remote_id(cx),
        };
        let Some(remote_id) = remote_id else {
            return;
        };
        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
            return;
        };
        let tx = self.sftp_view.read(cx).worker_sender();
        let runtime = self.forwarding_runtime.clone();
        runtime.spawn(async move {
            let result = async {
                let sftp = backend
                    .acquire_transfer_sftp()
                    .await
                    .map_err(|error| error.to_string())?;
                operation(sftp).await
            }
            .await;
            let _ = tx.send(SftpWorkerResult::RemoteMutationComplete {
                result,
                refresh_remote: pane == SftpPane::Remote,
                refresh_local: pane == SftpPane::Local,
                toast,
            });
        });
    }

    pub(in crate::workspace::sftp) fn push_sftp_toast(
        &self,
        title: String,
        description: Option<String>,
        variant: TerminalNoticeVariant,
        cx: &App,
    ) {
        self.push_workspace_notice(
            TerminalNotice {
                title,
                description,
                status_text: None,
                progress: None,
                variant,
            },
            cx,
        );
    }

    pub(in crate::workspace::sftp) fn close_sftp_dialog(&mut self, cx: &mut Context<Self>) {
        let delay = oxideterm_gpui_ui::motion::duration(
            &self.tokens,
            oxideterm_gpui_ui::motion::MotionDuration::Control,
        );
        if self
            .sftp_view
            .update(cx, |sftp, cx| sftp.begin_dialog_exit(delay, cx))
        {
            self.ime_marked_text = None;
        }
    }

    pub(in crate::workspace::sftp) fn stop_sftp_preview_media(&mut self, cx: &mut Context<Self>) {
        self.sftp_view
            .update(cx, |sftp, _cx| sftp.stop_preview_media());
    }

    pub(in crate::workspace::sftp) fn toggle_sftp_preview_audio(&mut self, cx: &mut Context<Self>) {
        self.sftp_view
            .update(cx, |sftp, cx| sftp.toggle_preview_audio(cx));
    }

    pub(in crate::workspace::sftp) fn seek_sftp_preview_audio(
        &mut self,
        position: std::time::Duration,
        cx: &mut Context<Self>,
    ) {
        self.sftp_view
            .update(cx, |sftp, cx| sftp.seek_preview_audio(position, cx));
    }

    pub(in crate::workspace::sftp) fn accept_sftp_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(dialog) = self.sftp_view.read(cx).dialog() else {
            return;
        };
        match dialog {
            SftpDialog::Rename { pane, old_name } => {
                let new_name = self
                    .sftp_view
                    .read(cx)
                    .input_value(SftpInput::DialogValue)
                    .trim()
                    .to_string();
                if pane == SftpPane::Local
                    && self.sftp_pair_primary_remote_id(cx).is_some()
                    && !new_name.is_empty()
                {
                    let old_path = {
                        let sftp = self.sftp_view.read(cx);
                        sftp.local_files
                            .iter()
                            .find(|file| file.name == old_name)
                            .map(|file| file.path.clone())
                            .unwrap_or_else(|| join_sftp_path(&sftp.local_path, &old_name))
                    };
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
                    self.spawn_sftp_pane_remote_mutation(
                        pane,
                        move |sftp| {
                            Box::pin(async move {
                                sftp.rename(&old_path, &new_path)
                                    .await
                                    .map_err(|error| error.to_string())
                            })
                        },
                        Some(toast),
                        cx,
                    );
                    self.close_sftp_dialog(cx);
                    return;
                }
                if !new_name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let local_path = self.sftp_view.read(cx).local_path.clone();
                            let old_path = join_local_path(&local_path, &old_name);
                            let new_path = join_local_path(&local_path, &new_name);
                            match std::fs::rename(old_path, new_path) {
                                Ok(()) => {
                                    if let Ok(files) = list_local_files(&local_path) {
                                        self.sftp_view.update(cx, |sftp, cx| {
                                            sftp.local_files = files;
                                            cx.notify();
                                        });
                                    }
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.renamed"),
                                        Some(sftp_i18n_rename_detail(
                                            self.i18n.t("sftp.toast.renamed_detail"),
                                            &old_name,
                                            &new_name,
                                        )),
                                        TerminalNoticeVariant::Success,
                                        cx,
                                    );
                                }
                                Err(error) => {
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.rename_failed"),
                                        Some(error.to_string()),
                                        TerminalNoticeVariant::Error,
                                        cx,
                                    );
                                }
                            }
                        }
                        SftpPane::Remote => {
                            let old_path = {
                                let sftp = self.sftp_view.read(cx);
                                sftp.remote_files
                                    .iter()
                                    .find(|file| file.name == old_name)
                                    .map(|file| file.path.clone())
                                    .unwrap_or_else(|| join_sftp_path(&sftp.remote_path, &old_name))
                            };
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
                                cx,
                            );
                        }
                    }
                }
            }
            SftpDialog::NewFolder { pane } => {
                let name = self
                    .sftp_view
                    .read(cx)
                    .input_value(SftpInput::DialogValue)
                    .trim()
                    .to_string();
                if pane == SftpPane::Local
                    && self.sftp_pair_primary_remote_id(cx).is_some()
                    && !name.is_empty()
                {
                    let path = join_sftp_path(&self.sftp_view.read(cx).local_path, &name);
                    let toast = SftpMutationToast {
                        success_title: self.i18n.t("sftp.toast.folder_created"),
                        success_description: Some(name),
                        error_title: self.i18n.t("sftp.toast.create_folder_failed"),
                    };
                    self.spawn_sftp_pane_remote_mutation(
                        pane,
                        move |sftp| {
                            Box::pin(async move {
                                sftp.mkdir(&path).await.map_err(|error| error.to_string())
                            })
                        },
                        Some(toast),
                        cx,
                    );
                    self.close_sftp_dialog(cx);
                    return;
                }
                if !name.is_empty() {
                    match pane {
                        SftpPane::Local => {
                            let local_path = self.sftp_view.read(cx).local_path.clone();
                            let path = join_local_path(&local_path, &name);
                            match std::fs::create_dir_all(path) {
                                Ok(()) => {
                                    if let Ok(files) = list_local_files(&local_path) {
                                        self.sftp_view.update(cx, |sftp, cx| {
                                            sftp.local_files = files;
                                            cx.notify();
                                        });
                                    }
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.folder_created"),
                                        Some(name),
                                        TerminalNoticeVariant::Success,
                                        cx,
                                    );
                                }
                                Err(error) => {
                                    self.push_sftp_toast(
                                        self.i18n.t("sftp.toast.create_folder_failed"),
                                        Some(error.to_string()),
                                        TerminalNoticeVariant::Error,
                                        cx,
                                    );
                                }
                            }
                        }
                        SftpPane::Remote => {
                            let remote_path = self.sftp_view.read(cx).remote_path.clone();
                            let path = join_sftp_path(&remote_path, &name);
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
                                cx,
                            );
                        }
                    }
                }
            }
            SftpDialog::Delete { pane, files } => {
                if pane == SftpPane::Local && self.sftp_pair_primary_remote_id(cx).is_some() {
                    let local_files = self.sftp_view.read(cx).local_files.clone();
                    let targets = files
                        .iter()
                        .filter_map(|name| {
                            local_files
                                .iter()
                                .find(|file| file.name == *name)
                                .map(|file| file.path.clone())
                        })
                        .collect::<Vec<_>>();
                    let count = targets.len();
                    let toast = SftpMutationToast {
                        success_title: self.i18n.t("sftp.toast.deleted"),
                        success_description: Some(sftp_i18n_count(
                            self.i18n.t("sftp.toast.deleted_count"),
                            count,
                        )),
                        error_title: self.i18n.t("sftp.toast.delete_failed"),
                    };
                    self.spawn_sftp_pane_remote_mutation(
                        pane,
                        move |sftp| {
                            Box::pin(async move {
                                for path in targets {
                                    sftp.delete_recursive(&path)
                                        .await
                                        .map_err(|error| error.to_string())?;
                                }
                                Ok(())
                            })
                        },
                        Some(toast),
                        cx,
                    );
                    self.close_sftp_dialog(cx);
                    return;
                }
                match pane {
                    SftpPane::Local => {
                        let local_path = self.sftp_view.read(cx).local_path.clone();
                        let count = files.len();
                        let mut result = Ok(());
                        for name in files {
                            let path = join_local_path(&local_path, &name);
                            result = if std::fs::metadata(&path)
                                .is_ok_and(|metadata| metadata.is_dir())
                            {
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
                                if let Ok(files) = list_local_files(&local_path) {
                                    self.sftp_view.update(cx, |sftp, cx| {
                                        sftp.local_files = files;
                                        cx.notify();
                                    });
                                }
                                self.push_sftp_toast(
                                    self.i18n.t("sftp.toast.deleted"),
                                    Some(sftp_i18n_count(
                                        self.i18n.t("sftp.toast.deleted_count"),
                                        count,
                                    )),
                                    TerminalNoticeVariant::Success,
                                    cx,
                                );
                            }
                            Err(error) => {
                                self.push_sftp_toast(
                                    self.i18n.t("sftp.toast.delete_failed"),
                                    Some(error.to_string()),
                                    TerminalNoticeVariant::Error,
                                    cx,
                                );
                            }
                        }
                    }
                    SftpPane::Remote => {
                        let remote_files = self.sftp_view.read(cx).remote_files.clone();
                        let targets = files
                            .into_iter()
                            .filter_map(|name| {
                                remote_files
                                    .iter()
                                    .find(|file| file.name == name)
                                    .map(|file| file.path.clone())
                            })
                            .collect::<Vec<_>>();
                        let Some(remote_id) = self.visible_sftp_remote_id(cx) else {
                            self.close_sftp_dialog(cx);
                            return;
                        };
                        let Some(backend) = self.sftp_remote_backend(&remote_id) else {
                            self.close_sftp_dialog(cx);
                            return;
                        };
                        let tx = self.sftp_view.read(cx).worker_sender();
                        let runtime = self.forwarding_runtime.clone();
                        let success_title = self.i18n.t("sftp.toast.deleted");
                        let success_template = self.i18n.t("sftp.toast.deleted_count");
                        let error_title = self.i18n.t("sftp.toast.delete_failed");
                        runtime.spawn(async move {
                            let result = async {
                                let sftp = backend
                                    .acquire_transfer_sftp()
                                    .await
                                    .map_err(|error| error.to_string())?;
                                let mut deleted = 0_u64;
                                for path in targets {
                                    // Tauri nodeSftpDeleteRecursive returns the
                                    // recursive item count; keep the success
                                    // toast tied to the same backend count.
                                    deleted = deleted.saturating_add(
                                        sftp.delete_recursive(&path)
                                            .await
                                            .map_err(|error| error.to_string())?,
                                    );
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
                self.clear_sftp_selection(pane, cx);
            }
            SftpDialog::Conflict => {
                self.resolve_sftp_transfer_conflict(SftpConflictResolution::Rename, cx);
                return;
            }
            _ => {}
        }
        self.close_sftp_dialog(cx);
    }
}

pub(in crate::workspace::sftp) fn sftp_i18n_count(template: String, count: usize) -> String {
    template.replace("{{count}}", &count.to_string())
}

fn sftp_i18n_rename_detail(template: String, old_name: &str, new_name: &str) -> String {
    template
        .replace("{{old}}", old_name)
        .replace("{{new}}", new_name)
}
