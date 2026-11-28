use super::transfer::BatchSource;
use super::*;
use crate::browser_file::BrowserImport;
use futures::channel::oneshot;
use oxideterm_sftp::LocalDownloadDisposition;
use std::collections::{HashMap, VecDeque};

pub(super) type PickedFiles = HashMap<String, VecDeque<Result<BrowserImport, String>>>;

#[derive(Default)]
pub(super) struct UploadJobs {
    pub(super) running: HashMap<u64, Task<()>>,
    enumerating: HashMap<u64, Task<()>>,
    next_selection: u64,
    pub(super) paused: HashMap<u64, oneshot::Sender<()>>,
}
impl UploadJobs {
    pub(super) fn wake(&mut self, id: u64) {
        if let Some(wake) = self.paused.remove(&id) {
            let _ = wake.send(());
        }
    }
}

enum UploadStep {
    Read,
    Wait(oneshot::Receiver<()>),
    Finish(Result<u64, String>),
    Cancelled,
}
use wasm_bindgen::JsCast;

impl Browser {
    pub(super) fn download_from_sidebar(&self, paths: &[String]) -> Result<(), String> {
        let files = self.right.files.borrow();
        let name = std::path::Path::new(&paths[0])
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("ide.validation.nameInvalid")?;
        if paths.len() == 1
            && let Some(bytes) = files.entries.get(&paths[0]).and_then(|e| e.data.as_deref())
        {
            crate::browser_file::download(name, bytes)
                .map_err(|_| "sftp.toast.download_failed".into())
        } else {
            let bytes = files.zip_selection(paths)?;
            let filename = if paths.len() == 1 {
                format!("{name}.zip")
            } else {
                "Archive.zip".into()
            };
            crate::browser_file::download(&filename, &bytes)
                .map_err(|_| "sftp.toast.download_failed".into())
        }
    }

    pub(super) fn choose_uploads(&mut self, cx: &mut Context<Self>) {
        let selection = crate::browser_file::pick_uploads();
        self.prepare_upload(
            async move {
                let value = selection.await.map_err(|_| "sftp.toast.upload_failed")?;
                Ok(js_sys::Array::from(&value)
                    .iter()
                    .map(|f| BrowserDropEntry::File(f.unchecked_into()))
                    .collect())
            },
            cx,
        );
    }

    pub(super) fn dropped_uploads(&mut self, files: BrowserFileDrop, cx: &mut Context<Self>) {
        self.prepare_upload(std::future::ready(Ok(files.0)), cx);
    }

    fn prepare_upload(
        &mut self,
        selection: impl std::future::Future<Output = Result<Vec<BrowserDropEntry>, String>> + 'static,
        cx: &mut Context<Self>,
    ) {
        self.prepare_upload_entries(
            async move {
                let mut entries = Vec::new();
                for root in selection.await? {
                    let name = root.name();
                    crate::browser_file::validate_filename(&name)?;
                    let (kind, size, modified) = match &root {
                        BrowserDropEntry::File(f) => (
                            SftpFileType::File,
                            f.size() as u64,
                            Some((f.last_modified() / 1000.) as i64),
                        ),
                        BrowserDropEntry::Directory(_) => (SftpFileType::Directory, 0, None),
                    };
                    let import = BrowserImport::open(root).await;
                    let size = import.as_ref().map_or(size, |v| v.size);
                    entries.push((
                        sftp_file_entry(name.clone(), name, kind, size, modified),
                        import,
                    ));
                }
                Ok(entries)
            },
            None,
            cx,
        );
    }

    pub(super) fn prepare_upload_entries(
        &mut self,
        selection: impl std::future::Future<
            Output = Result<Vec<(SftpFileEntry, Result<BrowserImport, String>)>, String>,
        > + 'static,
        device: Option<(u64, String)>,
        cx: &mut Context<Self>,
    ) {
        let dir = self.state.read(cx).remote_path.clone();
        let Some(revision) = self
            .right
            .files
            .borrow()
            .entries
            .get(&dir)
            .filter(|e| e.data.is_none())
            .map(|e| e.revision)
        else {
            self.notice(
                self.i18n.t("sftp.sidebar.upload_target_changed"),
                None,
                TerminalNoticeVariant::Error,
                cx,
            );
            return;
        };
        self.uploads.next_selection += 1;
        let id = self.uploads.next_selection;
        let task = cx.spawn(async move |view, cx| {
            let prepared = async {
                let mut picked = PickedFiles::new();
                let mut pending = Vec::new();
                for (source, import) in selection.await? {
                    picked
                        .entry(source.path.clone())
                        .or_default()
                        .push_back(import);
                    pending.push(SftpPendingTransfer {
                        name: source.name.clone(),
                        source,
                        direction: TransferDirection::Upload,
                        protocol_override: None,
                    });
                }
                Ok::<_, String>((pending, picked))
            }
            .await;
            let _ = view.update(cx, |b, cx| {
                if let Some(task) = b.uploads.enumerating.remove(&id) {
                    task.detach();
                }
                match prepared {
                    Ok((pending, _)) if pending.is_empty() => (),
                    Ok((pending, picked))
                        if device.as_ref().is_none_or(|(id, path)| {
                            b.device.generation == *id && b.state.read(cx).local_path == *path
                        }) && b.upload_target_visible(&dir, cx)
                            && !b.state.read(cx).dialog_is_open()
                            && b.right
                                .files
                                .borrow()
                                .entries
                                .get(&dir)
                                .is_some_and(|e| e.revision == revision) =>
                    {
                        b.begin_batch(
                            TransferDirection::Upload,
                            Ok(pending),
                            BatchSource::Picked(picked),
                            cx,
                        )
                    }
                    Ok(_) => b.notice(
                        b.i18n.t("sftp.sidebar.upload_target_changed"),
                        None,
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                    Err(key) => b.notice(
                        b.i18n.t("sftp.toast.upload_failed"),
                        Some(b.i18n.t(&key)),
                        TerminalNoticeVariant::Error,
                        cx,
                    ),
                }
            });
        });
        self.uploads.enumerating.insert(id, task);
    }

    fn upload_target_visible(&self, dir: &str, cx: &App) -> bool {
        self.state.read(cx).remote_path == dir
            && (self.surface != SftpSurfaceId::Sidebar
                || self
                    .host
                    .read_with(cx, |h, cx| h.shows_files_for_node(&self.target.id, cx))
                    .unwrap_or(false))
    }

    pub(super) fn start_uploads(
        &mut self,
        launches: Vec<SftpTransferLaunch>,
        count: usize,
        target: TransferDestination,
        mut picked: PickedFiles,
        cx: &mut Context<Self>,
    ) {
        let Some(first) = launches.first().map(|l| l.id) else {
            self.transfer_notice(TransferDirection::Upload, 0, 0, count, cx);
            return;
        };
        let mut skipped = count - launches.len();
        let job = cx.spawn(async move |view, cx| {
            let (mut completed, mut failed) = (0, 0);
            for launch in launches {
                let prepared = picked
                    .get_mut(&launch.local_path)
                    .and_then(VecDeque::pop_front)
                    .expect("planned upload has a selected source");
                let mut import = match prepared {
                    Ok(import) => import,
                    Err(error) => {
                        let _ = view.update(cx, |b, cx| b.finish_item(launch.id, Err(error), cx));
                        failed += 1;
                        continue;
                    }
                };
                let size = import.size;
                let mut error = None;
                loop {
                    let Ok(step) = view.update(cx, |b, cx| {
                        let state = b
                            .state
                            .read(cx)
                            .transfers
                            .iter()
                            .find(|i| i.id == launch.id)
                            .map(|i| i.state);
                        match state {
                            None | Some(SftpTransferState::Cancelled) => {
                                return UploadStep::Cancelled;
                            }
                            Some(SftpTransferState::Paused) => {
                                let (wake, rx) = oneshot::channel();
                                b.uploads.paused.insert(launch.id, wake);
                                return UploadStep::Wait(rx);
                            }
                            _ => {}
                        }
                        let directory = std::path::Path::new(&launch.remote_path)
                            .parent()
                            .and_then(|p| p.to_str());
                        if !directory.is_some_and(|dir| b.upload_target_visible(dir, cx)) {
                            return UploadStep::Finish(Err(
                                "sftp.sidebar.upload_target_changed".into()
                            ));
                        }
                        if let Some(key) = error.take() {
                            return UploadStep::Finish(Err(key));
                        }
                        if import.complete() {
                            let result = target.import_tree(
                                &mut b.right.files.borrow_mut(),
                                &launch.remote_path,
                                launch.download_disposition
                                    == LocalDownloadDisposition::ReplaceExisting,
                                import.take_tree(),
                            );
                            if result.is_ok() {
                                b.files_changed(SftpPane::Remote, cx);
                            }
                            return UploadStep::Finish(result);
                        }
                        b.state.update(cx, |s, cx| {
                            if let Some(item) = s.transfers.iter_mut().find(|i| i.id == launch.id) {
                                item.size = size;
                                item.transferred = import.read;
                                item.state = SftpTransferState::Active;
                                cx.notify();
                            }
                        });
                        UploadStep::Read
                    }) else {
                        return;
                    };
                    match step {
                        UploadStep::Cancelled => {
                            skipped += 1;
                            break;
                        }
                        UploadStep::Wait(wake) => {
                            let _ = wake.await;
                        }
                        UploadStep::Finish(result) => {
                            if result.is_ok() {
                                completed += 1;
                            } else {
                                failed += 1;
                            }
                            let _ = view.update(cx, |b, cx| b.finish_item(launch.id, result, cx));
                            break;
                        }
                        UploadStep::Read => {
                            error = import.read_chunk().await.err();
                        }
                    }
                }
            }
            let _ = view.update(cx, |b, cx| {
                b.transfer_notice(TransferDirection::Upload, completed, failed, skipped, cx);
                if let Some(job) = b.uploads.running.remove(&first) {
                    job.detach();
                }
            });
        });
        self.uploads.running.insert(first, job);
    }
}
