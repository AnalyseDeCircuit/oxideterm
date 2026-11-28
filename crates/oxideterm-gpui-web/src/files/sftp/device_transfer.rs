use super::{device::error_key, transfer::Batch, *};
use crate::{
    browser_file::{BrowserImport, validate_filename},
    files::device::DeviceFolder,
};
use futures::channel::oneshot;
use oxideterm_gpui_ide::portable::PortableIdeHost;
use oxideterm_ide_core::{FileKind, IdeLocation, SavedFileVersion};
use oxideterm_sftp::LocalDownloadDisposition;
use std::collections::{HashMap, HashSet};

pub(super) struct DownloadTarget {
    folder: DeviceFolder,
    generation: u64,
    path: String,
    files: HashMap<String, SavedFileVersion>,
    dirs: HashMap<String, DeviceFolder>,
}

fn join(parent: &str, name: &str) -> String {
    format!("{}/{name}", parent.trim_end_matches('/'))
}

enum TransferStep {
    Cancel,
    Resume,
    Wait(oneshot::Receiver<()>),
}

impl DownloadTarget {
    async fn ready(
        &self,
        view: &WeakEntity<Browser>,
        cx: &mut AsyncApp,
        id: u64,
        total: u64,
        transferred: u64,
    ) -> Result<bool, &'static str> {
        loop {
            let step = view
                .update(cx, |b, cx| {
                    let s = b.state.read(cx);
                    let Some(item) = s.transfers.iter().find(|item| item.id == id) else {
                        return Ok(TransferStep::Cancel);
                    };
                    if item.state == SftpTransferState::Cancelled {
                        return Ok(TransferStep::Cancel);
                    }
                    if b.device.generation != self.generation || s.local_path != self.path {
                        return Err("sftp.toast.destination_changed");
                    }
                    if item.state == SftpTransferState::Paused {
                        let (tx, rx) = oneshot::channel();
                        b.uploads.paused.insert(id, tx);
                        return Ok(TransferStep::Wait(rx));
                    }
                    b.state.update(cx, |s, cx| {
                        if let Some(item) = s.transfers.iter_mut().find(|t| t.id == id) {
                            item.size = total;
                            item.transferred = transferred;
                            item.state = SftpTransferState::Active;
                            cx.notify();
                        }
                    });
                    Ok(TransferStep::Resume)
                })
                .map_err(|_| "sftp.toast.destination_changed")??;
            match step {
                TransferStep::Cancel => return Ok(false),
                TransferStep::Resume => return Ok(true),
                TransferStep::Wait(rx) => {
                    let _ = rx.await;
                }
            }
        }
    }
    async fn capture(
        root: &DeviceFolder,
        generation: u64,
        path: String,
        wanted: &HashSet<String>,
    ) -> Result<(Self, Vec<SftpFileEntry>), String> {
        let folder = root.directory(&path).await.map_err(error_key)?;
        let mut target = Self {
            folder: folder.clone(),
            generation,
            path,
            files: Default::default(),
            dirs: Default::default(),
        };
        let mut dirs = vec![(String::new(), folder)];
        let mut rows = Vec::new();
        while let Some((base, dir)) = dirs.pop() {
            for entry in dir.list("", "/").await.map_err(error_key)? {
                let key = join(&base, &entry.name);
                if base.is_empty() {
                    rows.push(sftp_file_entry(
                        entry.name.clone(),
                        join(&target.path, &entry.name),
                        if entry.kind == FileKind::Directory {
                            SftpFileType::Directory
                        } else {
                            SftpFileType::File
                        },
                        entry.version.size_bytes.unwrap_or(0),
                        entry.version.modified_millis.map(|n| n / 1000),
                    ));
                }
                if !wanted.contains(&key) {
                    continue;
                }
                if entry.kind == FileKind::Directory {
                    let sub = dir
                        .directory(&join("/", &entry.name))
                        .await
                        .map_err(error_key)?;
                    target.dirs.insert(key.clone(), sub.clone());
                    dirs.push((key, sub));
                } else {
                    target.files.insert(key, entry.version);
                }
            }
        }
        Ok((target, rows))
    }
}

impl Browser {
    pub(super) fn upload_device_selection(
        &mut self,
        selected: Vec<SftpFileEntry>,
        cx: &mut Context<Self>,
    ) {
        let binding = (
            self.device.generation,
            self.state.read(cx).local_path.clone(),
        );
        let folder = self.device.folder.clone().unwrap();
        self.prepare_upload_entries(
            async move {
                let mut sources = Vec::new();
                for entry in selected {
                    let prepared = async {
                        if entry.file_type != SftpFileType::Directory {
                            let file = folder.snapshot(&entry.path).await.map_err(error_key)?;
                            return BrowserImport::open(BrowserDropEntry::File(file)).await;
                        }
                        let mut import = BrowserImport::folder(entry.name.clone())?;
                        let mut dirs = vec![(entry.path.clone(), String::new())];
                        while let Some((dir, relative)) = dirs.pop() {
                            for e in folder.list("", &dir).await.map_err(error_key)? {
                                validate_filename(&e.name)?;
                                let child = if relative.is_empty() {
                                    e.name
                                } else {
                                    join(&relative, &e.name)
                                };
                                let IdeLocation::Remote { path, .. } = e.location else {
                                    unreachable!()
                                };
                                if e.kind == FileKind::Directory {
                                    import.add_directory(child.clone())?;
                                    dirs.push((path, child));
                                } else {
                                    import.add_file(
                                        child,
                                        folder.snapshot(&path).await.map_err(error_key)?,
                                    )?;
                                }
                            }
                        }
                        Ok(import)
                    }
                    .await;
                    sources.push((entry, prepared));
                }
                Ok(sources)
            },
            Some(binding),
            cx,
        );
    }

    pub(super) fn prepare_device_download(
        &mut self,
        pending: Result<Vec<SftpPendingTransfer>, String>,
        cx: &mut Context<Self>,
    ) {
        let root = self.device.folder.clone().unwrap();
        let generation = self.device.generation;
        let path = self.state.read(cx).local_path.clone();
        let source = self.right.files.clone();
        self.device.planning = Some(cx.spawn(async move |view, cx| {
            let prepared = async {
                let entries = pending?;
                let mut wanted = HashSet::new();
                {
                    let fs = source.borrow();
                    for transfer in &entries {
                        validate_filename(&transfer.name)?;
                        let dest = join("/", &transfer.name);
                        wanted.insert(dest.clone());
                        if transfer.source.file_type == SftpFileType::Directory {
                            let root = &transfer.source.path;
                            let prefix = format!("{root}/");
                            for (p, _) in fs
                                .entries
                                .range(prefix.clone()..)
                                .take_while(|(p, _)| p.starts_with(&prefix))
                            {
                                wanted.insert(format!("{dest}{}", &p[root.len()..]));
                            }
                        }
                    }
                }
                let (target, rows) =
                    DownloadTarget::capture(&root, generation, path.clone(), &wanted).await?;
                Ok::<_, String>((entries, target, rows))
            }
            .await;
            let _ = view.update(cx, |b, cx| match prepared {
                Ok((entries, target, rows))
                    if b.device.generation == generation
                        && b.state.read(cx).local_path == path
                        && !b.state.read(cx).dialog_is_open() =>
                {
                    b.state.update(cx, |s, cx| {
                        s.local_files = rows;
                        cx.notify();
                    });
                    b.plan_batch(
                        TransferDirection::Download,
                        entries,
                        Batch::Device(target),
                        cx,
                    );
                }
                Ok(_) => b.notice(
                    b.i18n.t("sftp.toast.destination_changed"),
                    None,
                    TerminalNoticeVariant::Error,
                    cx,
                ),
                Err(key) => b.notice(
                    b.i18n.t("sftp.toast.download_failed"),
                    Some(b.i18n.t(&key)),
                    TerminalNoticeVariant::Error,
                    cx,
                ),
            });
        }));
    }

    pub(super) fn start_device_download(
        &mut self,
        launches: Vec<SftpTransferLaunch>,
        count: usize,
        target: DownloadTarget,
        cx: &mut Context<Self>,
    ) {
        let Some(job_id) = launches.first().map(|l| l.id) else {
            self.transfer_notice(TransferDirection::Download, 0, 0, count, cx);
            return;
        };
        let source = self.right.files.clone();
        let mut skipped = count - launches.len();
        let job = cx.spawn(async move |view, cx| {
            let (mut completed, mut failed) = (0, 0);
            for launch in launches {
                let id = launch.id;
                let disk = &target.folder;
                // Pin source revisions before yielding. Never borrow the demo disk across browser I/O.
                let items = {
                    let fs = source.borrow();
                    let root = &launch.remote_path;
                    let prefix = format!("{root}/");
                    fs.entries.get(root).map(|first| {
                        std::iter::once((root.as_str(), first))
                            .chain(
                                fs.entries
                                    .range(prefix.clone()..)
                                    .take_while(|(p, _)| p.starts_with(&prefix))
                                    .map(|(p, e)| (p.as_str(), e)),
                            )
                            .map(|(path, e)| {
                                (
                                    path.to_owned(),
                                    e.revision,
                                    e.data.as_ref().map(|d| d.len()),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                };
                let mut transferred = 0;
                let result = async {
                    let items = items.ok_or("sftp.toast.destination_changed")?;
                    let total = items.iter().filter_map(|(_, _, n)| *n).sum::<usize>() as u64;
                    let name = std::path::Path::new(&launch.local_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .ok_or("ide.validation.nameInvalid")?;
                    validate_filename(name)?;
                    let dest = join("/", name);
                    let replace =
                        launch.download_disposition == LocalDownloadDisposition::ReplaceExisting;
                    let mut parents = HashMap::from([(String::new(), disk.clone())]);
                    for (path, rev, len) in items {
                        let rel = path
                            .strip_prefix(&launch.remote_path)
                            .ok_or("ide.validation.nameInvalid")?;
                        let to = format!("{dest}{rel}");
                        let (parent, leaf) =
                            to.rsplit_once('/').ok_or("ide.validation.nameInvalid")?;
                        validate_filename(leaf)?;
                        let pinned = parents
                            .get(parent)
                            .ok_or("sftp.toast.destination_changed")?;
                        let local = join("/", leaf);
                        let mut writer = None;
                        let mut offset = 0;
                        loop {
                            if !target.ready(&view, cx, id, total, transferred).await? {
                                return Ok(None);
                            }
                            if source
                                .borrow()
                                .entries
                                .get(&path)
                                .is_none_or(|e| e.revision != rev)
                            {
                                return Err("sftp.toast.destination_changed".to_owned());
                            }
                            let Some(len) = len else {
                                let dir = if let Some(existing) =
                                    target.dirs.get(&to).filter(|_| replace)
                                {
                                    let current =
                                        pinned.directory(&local).await.map_err(error_key)?;
                                    if !current.same_entry(existing).await {
                                        return Err("sftp.toast.destination_changed".into());
                                    }
                                    current
                                } else {
                                    pinned
                                        .create_folder(String::new(), local.clone())
                                        .await
                                        .map_err(error_key)?;
                                    pinned.directory(&local).await.map_err(error_key)?
                                };
                                parents.insert(to.clone(), dir);
                                break;
                            };
                            if writer.is_none() {
                                let expected = target.files.get(&to).filter(|_| replace);
                                writer =
                                    Some(pinned.writer(&local, expected).await.map_err(error_key)?);
                                continue;
                            }
                            if offset == len {
                                writer.take().unwrap().finish().await.map_err(error_key)?;
                                break;
                            }
                            let end = len.min(offset + 1024 * 1024);
                            let chunk = {
                                let fs = source.borrow();
                                let bytes = fs.entries[&path]
                                    .data
                                    .as_deref()
                                    .ok_or("sftp.toast.destination_changed")?;
                                js_sys::Uint8Array::from(&bytes[offset..end])
                            };
                            writer
                                .as_ref()
                                .unwrap()
                                .write(&chunk)
                                .await
                                .map_err(error_key)?;
                            transferred += (end - offset) as u64;
                            offset = end;
                        }
                    }
                    Ok::<_, String>(Some(transferred))
                }
                .await;
                match result {
                    Ok(None) => skipped += 1,
                    result => {
                        let result = result.map(|n| n.unwrap());
                        if result.is_ok() {
                            completed += 1
                        } else {
                            failed += 1
                        }
                        let _ = view.update(cx, |b, cx| b.finish_item(id, result, cx));
                    }
                }
            }
            let _ = view.update(cx, |b, cx| {
                if let Some(job) = b.uploads.running.remove(&job_id) {
                    job.detach();
                }
                if b.device.generation == target.generation {
                    b.reload_device(cx);
                }
                b.transfer_notice(TransferDirection::Download, completed, failed, skipped, cx);
            });
        });
        self.uploads.running.insert(job_id, job);
    }
}
