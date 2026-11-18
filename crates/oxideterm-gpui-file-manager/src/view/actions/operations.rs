use super::*;

impl FileManagerView {
    pub(in crate::view) fn create_file_manager_folder(&mut self, cx: &mut Context<Self>) {
        let (name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        let audit = local_file_operation("file_mkdir", &join_local_path(&current_path, &name));
        let result = validate_local_name(&name)
            .map(|_| join_local_path(&current_path, &name))
            .and_then(|path| std::fs::create_dir(&path).map_err(|error| error.to_string()));
        audit.result(&result);
        match result {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.folderCreated"),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        cx.notify();
    }

    pub(in crate::view) fn create_file_manager_file(&mut self, cx: &mut Context<Self>) {
        let (name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        let audit = local_file_operation(
            "file_save",
            &format!(
                "path={}; mode=create_new",
                join_local_path(&current_path, &name)
            ),
        );
        let result = validate_local_name(&name)
            .map(|_| join_local_path(&current_path, &name))
            .and_then(|path| {
                std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            });
        audit.finish(
            if result.is_ok() {
                oxideterm_audit::AuditOutcome::Succeeded
            } else {
                oxideterm_audit::AuditOutcome::Failed
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            result.is_ok().then_some(0),
        );
        match result {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.fileCreated"),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        cx.notify();
    }

    pub(in crate::view) fn rename_file_manager_entry(
        &mut self,
        old_name: String,
        cx: &mut Context<Self>,
    ) {
        let (new_name, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (
                file_manager.dialog_value.trim().to_string(),
                file_manager.path.clone(),
            )
        };
        let old_path = join_local_path(&current_path, &old_name);
        let new_path = join_local_path(&current_path, &new_name);
        let audit = local_file_operation("file_rename", &format!("{old_path} → {new_path}"));
        let result = validate_local_name(&new_name)
            .and_then(|_| std::fs::rename(&old_path, &new_path).map_err(|error| error.to_string()));
        audit.result(&result);
        match result {
            Ok(()) => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.renamed"),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        cx.notify();
    }

    pub(in crate::view) fn delete_file_manager_entries(
        &mut self,
        names: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        let current_path = self.file_manager.read(cx).path.clone();
        let context = super::local_file_audit_context(oxideterm_audit::AuditSource::User);
        let mut audit = oxideterm_audit::AuditOperation::in_context(
            context.as_ref(),
            oxideterm_audit::AuditCategory::File,
            "file_delete_recursive",
            Some(&format!("path={current_path}; requested={}", names.len())),
        );
        let child_context = context.map(|mut context| {
            context.parent_id = audit.id().map(str::to_owned);
            context
        });
        let mut error = None;
        let mut removed = 0usize;
        let mut failed_path = None;
        for name in &names {
            let path = join_local_path(&current_path, name);
            let path_ref = std::path::Path::new(&path);
            let is_directory = path_ref.is_dir();
            let child = oxideterm_audit::AuditOperation::in_context(
                child_context.as_ref(),
                oxideterm_audit::AuditCategory::File,
                if is_directory {
                    "file_delete_recursive"
                } else {
                    "file_delete"
                },
                Some(&path),
            );
            let result = if is_directory {
                std::fs::remove_dir_all(path_ref)
            } else {
                std::fs::remove_file(path_ref)
            };
            child.result(&result);
            if let Err(err) = result {
                error = Some(err.to_string());
                failed_path = Some(path);
                break;
            }
            removed += 1;
        }
        audit.summary(&format!("path={current_path}; requested={}; removed={removed}; unprocessed={}; failed_path={}; nested_items=unavailable:remove_dir_all", names.len(), names.len().saturating_sub(removed + usize::from(error.is_some())), failed_path.as_deref().unwrap_or("none")));
        audit.finish(
            if error.is_none() {
                oxideterm_audit::AuditOutcome::Succeeded
            } else if removed > 0 {
                oxideterm_audit::AuditOutcome::Partial
            } else {
                oxideterm_audit::AuditOutcome::Failed
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            None,
        );
        match error {
            Some(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
            None => {
                self.close_file_manager_dialog(cx);
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n
                        .t("fileManager.deletedCount")
                        .replace("{{count}}", &names.len().to_string()),
                    None,
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
        }
        cx.notify();
    }

    pub(in crate::view) fn copy_file_manager_selection(
        &mut self,
        cut: bool,
        cx: &mut Context<Self>,
    ) {
        let entries = self.selected_file_manager_entries(cx);
        if entries.is_empty() {
            return;
        }
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.clipboard = Some(LocalClipboard {
                mode: if cut {
                    LocalClipboardMode::Cut
                } else {
                    LocalClipboardMode::Copy
                },
                paths: entries.iter().map(|entry| entry.path.clone()).collect(),
                source_dir: file_manager.path.clone(),
            });
            file_manager.clear_context_menu_immediately();
            cx.notify();
        });
        let key = if cut {
            "fileManager.cutCount"
        } else {
            "fileManager.copiedCount"
        };
        self.push_file_manager_toast(
            self.i18n
                .t(key)
                .replace("{{count}}", &entries.len().to_string()),
            None,
            FileManagerNoticeKind::Default,
            cx,
        );
    }

    pub(in crate::view) fn start_file_manager_operation(
        &mut self,
        total: usize,
        work: impl FnOnce(
            delivery::ActiveDeliverySender<FileManagerOperationEvent>,
        ) -> Result<(), String>
        + Send
        + 'static,
        cx: &mut Context<Self>,
    ) {
        self.file_manager.update(cx, |file_manager, cx| {
            file_manager.start_operation(total, work, cx);
        });
    }

    pub(in crate::view) fn paste_file_manager_clipboard(&mut self, cx: &mut Context<Self>) {
        let (clipboard, current_path) = {
            let file_manager = self.file_manager.read(cx);
            (file_manager.clipboard.clone(), file_manager.path.clone())
        };
        let Some(clipboard) = clipboard else {
            return;
        };
        if clipboard.mode == LocalClipboardMode::Cut && clipboard.source_dir == current_path {
            self.dismiss_file_manager_context_menu(cx);
            return;
        }
        let destination = current_path;
        let sources = clipboard.paths.clone();
        let mode = clipboard.mode;
        let audit_context = super::local_file_audit_context(oxideterm_audit::AuditSource::User);
        let total = sources
            .iter()
            .map(|source| local_operation_unit_count(std::path::Path::new(source)))
            .sum::<usize>();
        self.start_file_manager_operation(
            total,
            move |tx| {
                oxideterm_local_files::paste_local_files(
                    sources,
                    destination,
                    mode,
                    total,
                    audit_context,
                    |progress| report_batch_progress(&tx, total, progress),
                )
            },
            cx,
        );
        if clipboard.mode == LocalClipboardMode::Cut {
            self.file_manager.update(cx, |file_manager, cx| {
                file_manager.clipboard = None;
                file_manager.clear_context_menu_immediately();
                cx.notify();
            });
        } else {
            self.dismiss_file_manager_context_menu(cx);
        }
    }

    pub(in crate::view) fn duplicate_file_manager_selection(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_file_manager_entries(cx);
        if entries.is_empty() {
            return;
        }
        let paths = entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let total = paths
            .iter()
            .map(|path| local_operation_unit_count(std::path::Path::new(path)))
            .sum::<usize>();
        let audit_context = super::local_file_audit_context(oxideterm_audit::AuditSource::User);
        self.start_file_manager_operation(
            total,
            move |tx| {
                oxideterm_local_files::duplicate_local_files(
                    paths,
                    total,
                    audit_context,
                    |progress| report_batch_progress(&tx, total, progress),
                )
            },
            cx,
        );
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(in crate::view) fn queue_file_manager_external_drop_paths(
        &mut self,
        paths: &[std::path::PathBuf],
        cx: &mut Context<Self>,
    ) {
        let sources = paths
            .iter()
            .filter(|path| path.exists())
            .map(|path| path.to_string_lossy().to_string())
            .collect::<Vec<_>>();
        if sources.is_empty() {
            return;
        }
        let destination = self.file_manager.read(cx).path.clone();
        let total = sources
            .iter()
            .map(|source| local_operation_unit_count(std::path::Path::new(source)))
            .sum::<usize>();
        let audit_context = super::local_file_audit_context(oxideterm_audit::AuditSource::User);
        self.start_file_manager_operation(
            total,
            move |tx| {
                oxideterm_local_files::copy_dropped_local_files(
                    sources,
                    destination,
                    total,
                    audit_context,
                    |progress| report_batch_progress(&tx, total, progress),
                )
            },
            cx,
        );
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(in crate::view) fn compress_file_manager_selection(&mut self, cx: &mut Context<Self>) {
        let entries = self.selected_file_manager_entries(cx);
        if entries.is_empty() {
            return;
        }
        let archive_name = if entries.len() == 1 {
            format!("{}.zip", entries[0].name)
        } else {
            format!("Archive_{}.zip", chrono::Local::now().format("%Y-%m-%d"))
        };
        let current_path = self.file_manager.read(cx).path.clone();
        let archive_path =
            unique_copy_path(&std::path::Path::new(&current_path).join(archive_name));
        let paths = entries
            .iter()
            .map(|entry| entry.path.clone())
            .collect::<Vec<_>>();
        let result = oxideterm_local_files::compress_local_files_audited(&paths, &archive_path);
        match result {
            Ok(()) => {
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.operationSuccess"),
                    Some(format!("{}", archive_path.display())),
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        self.dismiss_file_manager_context_menu(cx);
    }

    pub(in crate::view) fn extract_selected_file_manager_archive(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.single_selected_file_manager_file(cx) else {
            return;
        };
        if !can_extract_archive(&entry.name) {
            return;
        }
        let current_path = self.file_manager.read(cx).path.clone();
        let result =
            oxideterm_local_files::extract_local_archive_audited(&entry.path, &current_path);
        match result {
            Ok(()) => {
                self.refresh_file_manager(cx);
                self.push_file_manager_toast(
                    self.i18n.t("fileManager.operationSuccess"),
                    Some(entry.name),
                    FileManagerNoticeKind::Success,
                    cx,
                );
            }
            Err(error) => self.push_file_manager_toast(
                self.i18n.t("fileManager.error"),
                Some(error),
                FileManagerNoticeKind::Error,
                cx,
            ),
        }
        self.dismiss_file_manager_context_menu(cx);
    }
}

fn report_batch_progress(
    tx: &delivery::ActiveDeliverySender<FileManagerOperationEvent>,
    total: usize,
    progress: oxideterm_local_files::LocalBatchProgress,
) {
    let _ = tx.send(FileManagerOperationEvent::Progress(
        FileManagerOperationProgress {
            current: progress.current,
            total: total.max(1),
            file_name: progress.file_name,
            active: true,
        },
    ));
}
