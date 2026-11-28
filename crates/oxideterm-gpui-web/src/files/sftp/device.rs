use super::*;
use crate::files::device::{DeviceFolder, version};
use oxideterm_gpui_ide::portable::PortableIdeHost;
use oxideterm_ide_core::{
    FileKind, FileTreeEntry, IdeFileError, IdeFileErrorKind, IdeLocation, SavedFileVersion,
    TextFileFormat,
};
use oxideterm_preview::{PreviewKind, PreviewLoadOptions, TextLineEnding};
use std::sync::Arc;
use web_sys::File;

#[derive(Default)]
pub(super) struct DevicePane {
    pub(super) folder: Option<DeviceFolder>,
    pub(super) generation: u64,
    picker: Option<Task<()>>,
    pub(super) planning: Option<Task<()>>,
    listing: Option<Task<()>>,
    completion: Option<Task<()>>,
    preview: Option<Task<()>>,
    save: Option<Task<()>>,
    mutation: Option<Task<()>>,
    snapshot: Option<(String, File)>,
    saved_version: Option<SavedFileVersion>,
}

pub(super) fn error_key(err: IdeFileError) -> &'static str {
    match err.kind {
        IdeFileErrorKind::PermissionDenied => "sftp.toast.permission_denied",
        IdeFileErrorKind::Conflict => "sftp.toast.destination_changed",
        IdeFileErrorKind::Unsupported => "fileManager.browserOperationUnavailable",
        IdeFileErrorKind::UnrepresentableText => "sftp.preview.unrepresentable",
        IdeFileErrorKind::TooLarge => "ide.file_too_large",
        _ => "fileManager.browserFolderFailed",
    }
}

fn row(e: FileTreeEntry) -> SftpFileEntry {
    let IdeLocation::Remote { path, .. } = e.location else {
        unreachable!()
    };
    sftp_file_entry(
        e.name,
        path,
        if e.kind == FileKind::Directory {
            SftpFileType::Directory
        } else {
            SftpFileType::File
        },
        e.version.size_bytes.unwrap_or(0),
        e.version.modified_millis.map(|ms| ms / 1000),
    )
}

pub(super) use crate::files::device::slice;

async fn hex(file: &File, offset: u64) -> Result<PreviewContent, String> {
    let size = file.size() as u64;
    let start = offset.min(size);
    let end = size.min(start.saturating_add(PreviewLoadOptions::default().hex_chunk_size));
    let bytes = slice(file, start, end).await?;
    Ok(PreviewContent::Hex {
        data: oxideterm_preview::generate_hex_dump(&bytes, start),
        total_size: size,
        offset: start,
        chunk_size: end - start,
        has_more: end < size,
    })
}

async fn preview(file: &File, path: &str) -> Result<PreviewContent, String> {
    let size = file.size() as u64;
    let mut bytes = slice(file, 0, size.min(8192)).await?;
    let (kind, mime) = crate::files::preview::format(path, &bytes);
    let limits = PreviewLoadOptions::default();
    let max = match kind {
        PreviewKind::Text => limits.max_text_size,
        PreviewKind::Image => limits.max_preview_size,
        _ if mime == "application/pkix-cert" => return Err("fileManager.unsupportedFormat".into()),
        _ => return hex(file, 0).await,
    };
    if size > max {
        return Ok(PreviewContent::TooLarge {
            size,
            max_size: max,
            recommend_download: true,
        });
    }
    if size > bytes.len() as u64 {
        bytes.extend(slice(file, bytes.len() as u64, size).await?);
    }
    crate::files::preview::sftp_content(path, &bytes, file.last_modified() as u64, 0)
}

impl Browser {
    pub(super) fn pick_local_directory(&mut self, cx: &mut Context<Self>) {
        if self.primary_title.is_some()
            || self.sidebar.is_some()
            || self.device.picker.is_some()
            || !crate::files::can_pick_directory()
        {
            return;
        }
        let choice = crate::files::device::pick_directory();
        self.device.picker = Some(cx.spawn(async move |view, cx| {
            let result = crate::files::device::picked(choice).await;
            let _ = view.update(cx, |b, cx| {
                b.device.picker = None;
                let folder = match result {
                    Ok(None) => return,
                    Ok(Some(f)) => f,
                    Err(key) => {
                        b.notice(b.i18n.t(key), None, TerminalNoticeVariant::Error, cx);
                        return;
                    }
                };
                if b.state.read(cx).dialog_is_open() {
                    return;
                }
                b.device.generation += 1;
                b.device.folder = Some(folder);
                b.device.snapshot = None;
                b.device.saved_version = None;
                b.state.update(cx, |s, cx| {
                    s.set_primary_endpoint(
                        SftpRemoteId::DeviceFolder(b.device.generation),
                        Some("/".into()),
                    );
                    cx.notify();
                });
                b.reload_device(cx);
            });
        }));
    }

    pub(super) fn reload_device(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.device.folder.clone() else {
            return;
        };
        let id = self.device.generation;
        let path = self.state.read(cx).local_path.clone();
        self.state.update(cx, |s, cx| {
            s.pair_primary_loading = true;
            cx.notify()
        });
        self.device.listing = Some(cx.spawn(async move |view, cx| {
            let listing = folder.list("", &path).await;
            let _ = view.update(cx, |b, cx| {
                if id != b.device.generation || b.state.read(cx).local_path != path {
                    return;
                }
                let result = listing
                    .map(|entries| {
                        let files = entries.into_iter().map(row).collect();
                        oxideterm_sftp::RemoteSftpListing {
                            cwd: path.clone(),
                            files,
                        }
                    })
                    .map_err(|e| b.i18n.t(error_key(e)));
                let s = b.state.read(cx);
                let _ = s.worker_sender().send(SftpWorkerResult::PairPrimaryList {
                    surface_id: b.surface,
                    remote_id: SftpRemoteId::DeviceFolder(id),
                    view_generation: s.view_generation,
                    path,
                    result,
                });
            });
        }));
    }

    pub(super) fn complete_device_path(
        &mut self,
        load: SftpCompletionLoad,
        cx: &mut Context<Self>,
    ) {
        let folder = self.device.folder.clone().unwrap();
        let tx = self.state.read(cx).worker_sender();
        let i = self.i18n.clone();
        self.device.completion = Some(cx.spawn(async move |_, _| {
            let result = folder
                .list("", &load.parent_path)
                .await
                .map(|v| {
                    v.into_iter()
                        .map(row)
                        .map(sftp_path_completion_candidate)
                        .collect()
                })
                .map_err(|e| i.t(error_key(e)));
            let _ = tx.send(SftpWorkerResult::PairPrimaryPathCompletion {
                generation: load.generation,
                remote_id: load.remote_id,
                parent_path: load.parent_path,
                result,
            });
        }));
    }

    pub(super) fn open_device_file(&mut self, f: &SftpFileEntry, cx: &mut Context<Self>) {
        let folder = self.device.folder.clone().unwrap();
        self.device.snapshot = None;
        self.device.saved_version = None;
        let generation = self.state.update(cx, |s, cx| {
            s.begin_remote_preview(SftpPane::Local, f, None, cx)
        });
        let path = f.path.clone();
        self.device.preview = Some(cx.spawn(async move |view, cx| {
            let result = async {
                let file = folder.snapshot(&path).await.map_err(error_key)?;
                let data = preview(&file, &path).await?;
                Ok::<_, String>((file, data))
            }
            .await;
            let _ = view.update(cx, |b, cx| {
                if b.state.read(cx).preview_generation != generation {
                    return;
                }
                let content = result
                    .map(|(file, data)| {
                        b.device.saved_version = Some(version(&file));
                        b.device.snapshot = Some((path.clone(), file));
                        data
                    })
                    .map_err(|e| b.i18n.t(&e));
                b.state.update(cx, |s, cx| {
                    s.apply_preview_loaded(generation, path, content, None, cx);
                    cx.notify();
                });
            });
        }));
    }

    pub(super) fn device_hex(
        &mut self,
        path: String,
        offset: u64,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let Some((_, file)) = self.device.snapshot.clone().filter(|(p, _)| p == &path) else {
            return;
        };
        let tx = self.state.read(cx).worker_sender();
        let i = self.i18n.clone();
        self.device.preview = Some(cx.spawn(async move |_, _| {
            let result = hex(&file, offset).await.map_err(|k| i.t(&k));
            let _ = tx.send(SftpWorkerResult::PreviewHexLoaded {
                generation,
                path,
                result,
                error_prefix: i.t("sftp.toast.load_more_failed"),
            });
        }));
    }

    pub(super) fn save_device_preview(
        &mut self,
        generation: u64,
        path: String,
        content: Arc<str>,
        encoding: &str,
        ending: TextLineEnding,
        bom: bool,
        cx: &mut Context<Self>,
    ) {
        let folder = self.device.folder.clone().unwrap();
        let expected = self.device.saved_version.clone();
        let format = TextFileFormat {
            encoding: encoding.into(),
            has_bom: bom,
            line_ending: match ending {
                TextLineEnding::Lf => oxideterm_ide_core::LineEnding::Lf,
                TextLineEnding::Cr => oxideterm_ide_core::LineEnding::Cr,
                TextLineEnding::CrLf => oxideterm_ide_core::LineEnding::CrLf,
            },
        };
        self.device.save = Some(cx.spawn(async move |view, cx| {
            let saved = async {
                let expected = expected.ok_or("ide.conflict_title")?;
                let bytes = oxideterm_ide_fs::text::encode_chunks(
                    std::iter::once(content.as_ref()),
                    &format,
                )
                .map_err(error_key)?;
                let saved = folder
                    .save_bytes(&path, &bytes, Some(&expected), false)
                    .await
                    .map_err(|err| match err.kind {
                        IdeFileErrorKind::Conflict => "ide.conflict_title",
                        _ => error_key(err),
                    })?;
                Ok::<_, &'static str>(oxideterm_sftp::SftpPreviewSaveResult {
                    version: saved.modified_millis.map(|v| v.to_string()),
                    mtime: saved.modified_millis.map(|v| (v / 1000) as u64),
                    size: saved.size_bytes,
                    encoding_used: format.encoding,
                    atomic_write: true,
                })
            }
            .await;
            let _ = view.update(cx, |b, cx| {
                let ok = saved.is_ok();
                if let Ok(v) = &saved
                    && b.state.read(cx).preview_generation == generation
                {
                    b.device.saved_version = Some(SavedFileVersion {
                        size_bytes: v.size,
                        modified_millis: v.version.as_ref().and_then(|s| s.parse().ok()),
                        etag: None,
                    });
                }
                let result = saved.map_err(|e| b.i18n.t(e));
                b.state.update(cx, |s, cx| {
                    s.apply_preview_saved(
                        generation,
                        path.clone(),
                        content,
                        result,
                        &b.i18n.t("sftp.preview.network_error"),
                        cx,
                    );
                    cx.notify();
                });
                if ok {
                    b.reload_device(cx);
                }
            });
        }));
    }

    pub(super) fn compare_device_file(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(folder) = self.device.folder.clone() else {
            return;
        };
        let s = self.state.read(cx);
        let Some(local) = s
            .local_files
            .iter()
            .find(|e| e.name == name && e.file_type == SftpFileType::File)
            .map(|e| e.path.clone())
        else {
            return;
        };
        let Some(remote) = s.preview_path.clone() else {
            return;
        };
        let generation = s.preview_generation;
        self.device.preview = Some(cx.spawn(async move |view, cx| {
            let result = async {
                let file = folder.snapshot(&local).await.map_err(error_key)?;
                match preview(&file, &local).await? {
                    PreviewContent::Text { data, .. } => Ok(data),
                    _ => Err("fileManager.unsupportedFormat".to_owned()),
                }
            }
            .await;
            let _ = view.update(cx, |b, cx| {
                b.state.update(cx, |s, cx| {
                    if s.preview_generation != generation
                        || !matches!(s.dialog(), Some(SftpDialog::Preview { .. }))
                    {
                        return;
                    }
                    match result {
                        Ok(left) => {
                            let Some(PreviewContent::Text { data, .. }) =
                                s.preview_content.as_deref()
                            else {
                                return;
                            };
                            let diff = cx.new(|_| SftpDiffView::new(local, &left, remote, data));
                            s.set_dialog(SftpDialog::Diff { view: diff });
                        }
                        Err(k) => s.preview_error = Some(b.i18n.t(&k)),
                    }
                    cx.notify();
                })
            });
        }));
    }

    pub(super) fn mutate_device(&mut self, m: SftpRemoteMutation, cx: &mut Context<Self>) {
        let folder = self.device.folder.clone().unwrap();
        let i = self.i18n.clone();
        let id = self.device.generation;
        let directory = self.state.read(cx).local_path.clone();
        if self.device.mutation.is_some() {
            return;
        }
        self.state
            .update(cx, |s, cx| s.begin_dialog_exit(Duration::ZERO, cx));
        self.device.mutation = Some(cx.spawn(async move |view, cx| {
            let result = async {
                match m.operation() {
                    SftpFileOperation::NewFolder(p) => folder
                        .create_folder(String::new(), p.clone())
                        .await
                        .map(|_| None),
                    SftpFileOperation::Rename { from, to } => folder
                        .rename_item(String::new(), from.clone(), to.clone())
                        .await
                        .map(|_| None),
                    SftpFileOperation::Delete(paths) => {
                        for path in paths {
                            folder
                                .delete_item(String::new(), path.clone(), true)
                                .await?;
                        }
                        Ok(None)
                    }
                }
            }
            .await
            .map_err(|e| i.t(error_key(e)));
            let _ = view.update(cx, |b, cx| {
                b.device.mutation = None;
                if result.is_ok()
                    && m.clear_selection
                    && b.device.generation == id
                    && b.state.read(cx).local_path == directory
                {
                    b.state.update(cx, |s, cx| {
                        s.clear_selection(SftpPane::Local);
                        cx.notify();
                    });
                }
                let _ = b
                    .state
                    .read(cx)
                    .worker_sender()
                    .send(m.complete_with_count(result));
            });
        }));
    }
}
