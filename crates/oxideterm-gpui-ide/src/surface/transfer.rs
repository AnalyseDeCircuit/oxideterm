struct TreeTransfer {
    label: String,
    percent: u64,
    cancel: tokio::sync::watch::Sender<bool>,
}

#[derive(PartialEq)]
struct UploadTabRevision {
    tab_id: EditorTabId,
    revision: u64,
    version: SavedFileVersion,
}

impl IdeSurface {
    fn cancel_tree_transfer(&self) {
        if let Some(transfer) = &self.desktop.transfer {
            transfer.cancel.send_replace(true);
        }
    }

    fn pick_transfer_file(
        &mut self,
        menu: TreeContextMenu,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use std::{future::Future, path::PathBuf, pin::Pin};
        let IdeLocation::Remote { node_id, path } = menu.location else {
            return;
        };
        let upload = menu.is_directory;
        let picker: Pin<Box<dyn Future<Output = gpui::Result<Option<PathBuf>>>>> = if upload {
            let picker = cx.prompt_for_paths(gpui::PathPromptOptions {
                files: true,
                directories: false,
                multiple: false,
                prompt: Some(self.labels.upload.clone().into()),
            });
            Box::pin(async move { Ok(picker.await??.and_then(|paths| paths.into_iter().next())) })
        } else {
            let picker = cx.prompt_for_new_path(Path::new("."), Some(&menu.name));
            Box::pin(async move { Ok(picker.await??) })
        };
        let generation = self.generation;
        let fs = self.fs.clone();
        let jobs = self.file_jobs();
        let conflict = self.labels.transfer_conflict.clone();
        let replace = self.labels.transfer_overwrite.clone();
        let cancel = self.labels.cancel.clone();
        cx.spawn_in(window, async move |weak, cx| {
            let result = async {
                let Some(local) = picker.await.map_err(|e| e.to_string())? else {
                    return Ok(());
                };
                let target = if upload {
                    join_remote_child(
                        &path,
                        local.file_name().and_then(|s| s.to_str()).ok_or_else(|| {
                            std::io::Error::from(std::io::ErrorKind::InvalidFilename).to_string()
                        })?,
                    )
                } else {
                    path.clone()
                };
                let current = |this: &Self| {
                    this.generation == generation
                        && this.remote_actions_ready()
                        && this.node_id.as_deref() == Some(node_id.as_str())
                        && !this.shell_return_requested()
                        && this.desktop.transfer.is_none()
                };
                if !weak.update(cx, |this, _| current(this)).unwrap_or(false) {
                    return Ok(());
                }
                let overwrite = if upload {
                    let node = node_id.clone();
                    let target = target.clone();
                    jobs.run(async move { fs.batch_stat(node, vec![target]).await })
                        .await
                        .map_err(|e| e.message)?
                        .into_iter()
                        .flatten()
                        .next()
                        .is_some()
                } else {
                    local.try_exists().map_err(|e| e.to_string())?
                };
                if overwrite {
                    let destination = if upload {
                        target.clone()
                    } else {
                        local.display().to_string()
                    };
                    if cx
                        .prompt(
                            gpui::PromptLevel::Warning,
                            &conflict,
                            Some(&destination),
                            &[cancel.as_str(), replace.as_str()],
                        )
                        .await
                        != Ok(1)
                    {
                        return Ok(());
                    }
                }
                let _ = weak.update(cx, |this, cx| {
                    if current(this) {
                        this.start_tree_transfer(
                            IdeLocation::remote(node_id.clone(), target),
                            local,
                            upload,
                            overwrite,
                            cx,
                        );
                    }
                });
                Ok::<_, String>(())
            }
            .await;
            if let Err(error) = result {
                let _ = weak.update(cx, |this, cx| {
                    if this.generation == generation {
                        this.last_error = Some(error);
                        cx.notify();
                    }
                });
            }
        })
        .detach();
    }

    fn start_tree_transfer(
        &mut self,
        target: IdeLocation,
        local: std::path::PathBuf,
        upload: bool,
        overwrite: bool,
        cx: &mut Context<Self>,
    ) {
        use oxideterm_ide_fs::TransferDirection;
        let (cancel, cancel_rx) = tokio::sync::watch::channel(false);
        let (progress, mut updates) = tokio::sync::watch::channel((0, 0));
        self.desktop.transfer = Some(TreeTransfer {
            label: if upload {
                self.labels.upload.clone()
            } else {
                self.labels.download.clone()
            },
            percent: 0,
            cancel,
        });
        let generation = self.generation;
        let uploaded = upload.then(|| target.clone());
        let fs = self.fs.clone();
        let direction = if upload {
            TransferDirection::Upload
        } else {
            TransferDirection::Download
        };
        let work = self.backend_runtime.spawn(async move {
            fs.transfer_file(target, local, direction, overwrite, cancel_rx, progress)
                .await
        });
        cx.spawn(async move |weak, cx| {
            while updates.changed().await.is_ok() {
                let (done, total) = *updates.borrow_and_update();
                let percent = done.saturating_mul(100).checked_div(total).unwrap_or(0);
                let _ = weak.update(cx, |this, cx| {
                    if let Some(transfer) = &mut this.desktop.transfer
                        && transfer.percent != percent
                    {
                        transfer.percent = percent;
                        cx.notify();
                    }
                });
            }
            let result = await_ide_backend(work).await;
            if result.is_ok()
                && let Some(target) = &uploaded
            {
                let read = weak
                    .update(cx, |this, cx| {
                        if this.generation != generation || this.shell_return_requested() {
                            return None;
                        }
                        this.clean_upload_tab(target, cx)
                            .map(|tab| (tab, this.fs.clone(), this.file_jobs()))
                    })
                    .ok()
                    .flatten();
                if let Some((tab, fs, jobs)) = read {
                    let contents = jobs.run(open_text_file(fs, target.clone(), None)).await;
                    let _ = weak.update(cx, |this, cx| {
                        if this.generation == generation {
                            this.apply_upload_tab_refresh(target, tab, contents, cx);
                        }
                    });
                }
            }
            let _ = weak.update(cx, |this, cx| {
                this.complete_tree_transfer(generation, uploaded.as_ref(), result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn complete_tree_transfer(
        &mut self,
        generation: u64,
        uploaded: Option<&IdeLocation>,
        result: Result<(), IdeFileError>,
        cx: &mut Context<Self>,
    ) {
        self.desktop.transfer.take();
        if self.generation == generation {
            match result {
                Ok(())
                    if let Some((node, path)) =
                        uploaded.and_then(|target| paste_target_directory(target, false)) =>
                {
                    self.clear_search_cache();
                    self.load_directory(IdeLocation::remote(node, path), None, cx);
                }
                Err(error) if error.kind != IdeFileErrorKind::Cancelled => {
                    self.last_error = Some(self.file_error_message(&error))
                }
                _ => {}
            }
        }
        self.advance_tab_close_batch(cx);
        cx.notify();
    }

    fn clean_upload_tab(
        &self,
        target: &IdeLocation,
        cx: &mut Context<Self>,
    ) -> Option<UploadTabRevision> {
        let tab_id = self
            .workspace
            .tabs()
            .iter()
            .find(|tab| &tab.location == target)?
            .id;
        if self.loading_file_tabs.contains(&tab_id)
            || self.saving_tabs.contains(&tab_id)
            || self.is_tab_dirty(tab_id, cx)
        {
            return None;
        }
        let buffer = self.workspace.buffer(tab_id)?;
        Some(UploadTabRevision {
            tab_id,
            revision: buffer.revision,
            version: buffer.version.clone(),
        })
    }

    fn apply_upload_tab_refresh(
        &mut self,
        target: &IdeLocation,
        tab: UploadTabRevision,
        contents: Result<FileOpenResult, IdeFileError>,
        cx: &mut Context<Self>,
    ) {
        // Typing or saving during the remote read takes precedence over the upload refresh.
        if self.clean_upload_tab(target, cx).as_ref() == Some(&tab) {
            self.complete_file_open(tab.tab_id, target.clone(), contents, cx);
        }
    }
}
