use crate::demo::{DemoFiles, Files, LOCAL};
use gpui::*;
use oxideterm_gpui_file_manager::{FileSystemBridge, filesystem::*};
use oxideterm_gpui_sftp::{SftpFileOperation, SftpLocalHost};
use oxideterm_local_files::*;
use oxideterm_sftp::{SftpFileEntry, SftpFileType};
use std::{
    cell::RefCell,
    io,
    path::{Path, PathBuf},
};

const DEMO_BOOKMARKS: &str = "oxideterm.demo.file-manager.bookmarks";

fn bookmark_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub struct MemoryFs {
    pub files: Files,
    pub(super) cached_image: RefCell<Option<(u64, std::sync::Weak<Image>)>>,
}
impl MemoryFs {
    pub fn new(files: Files) -> Self {
        Self {
            files,
            cached_image: Default::default(),
        }
    }
    pub fn bytes(&self, p: &str) -> Result<Vec<u8>, String> {
        self.files
            .borrow()
            .entries
            .get(p)
            .and_then(|e| e.data.clone())
            .ok_or_else(|| "ENOENT".into())
    }
    pub fn create(&self, path: &str, dir: bool) -> Result<(), String> {
        let mut fs = self.files.borrow_mut();
        let parent = Path::new(path)
            .parent()
            .and_then(Path::to_str)
            .ok_or("EINVAL")?;
        fs.list(parent)?;
        if fs.entries.contains_key(path) {
            return Err("EEXIST".into());
        }
        fs.put(path, (!dir).then(Vec::new));
        Ok(())
    }
    pub fn remove(&self, p: &str, recursive: bool) -> Result<(), String> {
        self.remove_counted(p, recursive).map(|_| ())
    }
    pub fn remove_counted(&self, p: &str, recursive: bool) -> Result<u64, String> {
        let mut fs = self.files.borrow_mut();
        if !fs.entries.contains_key(p) {
            return Err("ENOENT".into());
        }
        if !recursive && fs.list(p).is_ok_and(|v| !v.is_empty()) {
            return Err("ENOTEMPTY".into());
        }
        let before = fs.entries.len();
        fs.entries.retain(|k, _| !Path::new(k).starts_with(p));
        Ok((before - fs.entries.len()) as u64)
    }
    pub fn duplicate(&self, from: &str, to: &str, cut: bool) -> Result<(), String> {
        self.files.borrow_mut().duplicate(from, to, cut)
    }
    pub fn sftp_entry(&self, path: &str) -> Option<SftpFileEntry> {
        self.files
            .borrow()
            .entries
            .get(path)
            .map(|e| sftp_entry(path, e))
    }
    pub fn sftp_list(&self, p: &str) -> Result<Vec<SftpFileEntry>, String> {
        Ok(self
            .files
            .borrow()
            .list(p)?
            .into_iter()
            .map(|(path, e)| sftp_entry(path, e))
            .collect())
    }
}
fn sftp_entry(path: &str, e: &crate::demo::Entry) -> SftpFileEntry {
    SftpFileEntry {
        name: name(path).into(),
        path: path.into(),
        file_type: if e.data.is_some() {
            SftpFileType::File
        } else {
            SftpFileType::Directory
        },
        size: e.data.as_ref().map_or(0, |v| v.len() as u64),
        size_known: true,
        modified: e.modified,
        permissions: None,
        owner: None,
        group: None,
        is_symlink: false,
        symlink_target: None,
    }
}

fn name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}
fn child(dir: &str, n: &str) -> Result<String, String> {
    if n.is_empty() || matches!(n, "." | "..") || n.contains('/') {
        Err("EINVAL".into())
    } else {
        Ok(DemoFiles::path(dir, n))
    }
}
impl SftpLocalHost for MemoryFs {
    fn home(&self) -> String {
        LOCAL.into()
    }
    fn download_dir(&self) -> String {
        LOCAL.into()
    }
    fn drives(&self) -> Vec<LocalDrive> {
        vec![]
    }
    fn list(&self, p: &str) -> io::Result<Vec<SftpFileEntry>> {
        self.sftp_list(p).map_err(io::Error::other)
    }
    fn mutate(&self, _: &str, op: SftpFileOperation) -> io::Result<()> {
        match op {
            SftpFileOperation::NewFolder(path) => self.create(&path, true),
            SftpFileOperation::Rename { from, to } => self.duplicate(&from, &to, true),
            SftpFileOperation::Delete(paths) => paths.iter().try_for_each(|p| self.remove(p, true)),
        }
        .map_err(io::Error::other)
    }
    fn read_text(&self, p: &str) -> io::Result<String> {
        String::from_utf8(self.bytes(p).map_err(io::Error::other)?).map_err(io::Error::other)
    }
    fn font_bytes(&self, p: &Path) -> io::Result<Vec<u8>> {
        self.bytes(p.to_str().ok_or(io::ErrorKind::InvalidInput)?)
            .map_err(io::Error::other)
    }
    fn image(&self, path: &str) -> ImageSource {
        self.file_image(path, 0)
    }
}
impl FileSystemBridge for MemoryFs {
    fn home(&self) -> String {
        LOCAL.into()
    }
    fn sidebar_locations(&self) -> Vec<LocalSidebarLocation> {
        vec![LocalSidebarLocation {
            kind: LocalSidebarLocationKind::Home,
            path: LOCAL.into(),
        }]
    }
    fn drives(&self) -> Vec<LocalDrive> {
        vec![]
    }
    fn load_bookmarks(&self, _: &Path) -> FileIo<Result<Vec<LocalBookmark>, String>> {
        FileIo::Ready(Ok(bookmark_storage()
            .and_then(|store| store.get_item(DEMO_BOOKMARKS).ok().flatten())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()))
    }
    fn save_bookmarks(
        &self,
        _: &Path,
        _: &[LocalBookmark],
        b: &[LocalBookmark],
    ) -> FileIo<Result<(), String>> {
        FileIo::Ready((|| {
            let json = serde_json::to_string(b).map_err(|e| e.to_string())?;
            bookmark_storage()
                .ok_or("StorageAccessDenied")?
                .set_item(DEMO_BOOKMARKS, &json)
                .map_err(|error| {
                    js_sys::Error::from(error)
                        .name()
                        .as_string()
                        .unwrap_or_default()
                })
        })())
    }
    fn list(&self, p: &str) -> FileIo<Result<Vec<LocalFileEntry>, String>> {
        FileIo::Ready(self.files.borrow().list(p).map(|rows| {
            rows.into_iter()
                .map(|(path, e)| LocalFileEntry {
                    name: name(path).into(),
                    path: path.into(),
                    file_type: if e.data.is_none() {
                        LocalFileType::Directory
                    } else {
                        LocalFileType::File
                    },
                    size: e.data.as_ref().map_or(0, |v| v.len() as u64),
                    modified: e.modified,
                    readonly: false,
                    symlink_target: None,
                })
                .collect()
        }))
    }
    fn mutate(&self, dir: &str, action: FileMutation) -> FileIo<Result<(), String>> {
        FileIo::Ready((|| match action {
            FileMutation::CreateFolder(n) => self.create(&child(dir, &n)?, true),
            FileMutation::CreateFile(n) => self.create(&child(dir, &n)?, false),
            FileMutation::Rename { from, to } => {
                self.duplicate(&child(dir, &from)?, &child(dir, &to)?, true)
            }
            FileMutation::Delete(names) => names
                .iter()
                .try_for_each(|n| self.remove(&child(dir, n)?, true)),
        })())
    }
    fn batch_units(&self, p: &[String]) -> usize {
        p.len()
    }
    fn compress(&self, sources: &[String], destination: &Path) -> FileIo<Result<PathBuf, String>> {
        FileIo::Ready((|| {
            self.files.borrow_mut().compress_zip(sources, destination)
        })())
    }
    fn can_extract(&self, name: &str) -> bool {
        mime_type_for_extension(&local_file_extension(name)) == "application/zip"
    }
    fn extract(&self, archive: &str, destination: &str) -> FileIo<Result<(), String>> {
        FileIo::Ready((|| {
            self.files.borrow_mut().extract_zip(archive, destination)
        })())
    }
    fn properties(&self, e: &LocalFileEntry) -> LocalFileProperties {
        let metadata = self.metadata(&e.path);
        LocalFileProperties {
            location: e.path.clone(),
            size: e.size,
            modified: e.modified,
            accessed: None,
            readonly: false,
            dir_files: None,
            dir_dirs: None,
            total_size: None,
            created: metadata.as_ref().and_then(|m| m.created),
            mode: None,
            mime_type: metadata.and_then(|m| m.mime_type),
            is_symlink: false,
        }
    }
    fn checksum(&self, path: String, cx: &App) -> Task<Result<LocalChecksumResult, String>> {
        let files = self.files.clone();
        let Some(revision) = files.borrow().entries.get(&path).map(|e| e.revision) else {
            return Task::ready(Err("fileManager.checksumChanged".into()));
        };
        cx.spawn(async move |cx| {
            let mut sums = FileChecksums::default();
            let mut offset = 0;
            loop {
                {
                    let fs = files.borrow();
                    let bytes = fs
                        .entries
                        .get(&path)
                        .filter(|e| e.revision == revision)
                        .and_then(|e| e.data.as_deref())
                        .ok_or("fileManager.checksumChanged")?;
                    let end = bytes.len().min(offset + 256 * 1024);
                    sums.update(&bytes[offset..end]);
                    if end == bytes.len() {
                        return Ok(sums.finish());
                    }
                    offset = end;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1))
                    .await;
            }
        })
    }
    fn metadata(&self, p: &str) -> Option<LocalPreviewMetadata> {
        self.files
            .borrow()
            .entries
            .get(p)
            .map(|e| LocalPreviewMetadata {
                size: e.data.as_ref().map_or(0, |v| v.len() as u64),
                modified: e.modified,
                created: e.created,
                accessed: None,
                readonly: false,
                is_dir: e.data.is_none(),
                is_symlink: false,
                mode: None,
                mime_type: e.data.as_deref().map(|b| super::preview::format(p, b).1),
            })
    }
    fn font_bytes(&self, p: &str) -> Result<Vec<u8>, String> {
        self.bytes(p)
    }
    fn image(&self, path: &str, turns: u8) -> ImageSource {
        self.file_image(path, turns)
    }
    fn download(&self, path: &str) -> FileIo<Result<(), String>> {
        FileIo::Ready((|| {
            let fs = self.files.borrow();
            let bytes = fs
                .entries
                .get(path)
                .and_then(|e| e.data.as_deref())
                .ok_or("ENOENT")?;
            crate::browser_file::download(name(path), bytes)
                .map_err(|_| "sftp.toast.download_failed".into())
        })())
    }
    fn preview(&self, p: &str) -> FileIo<LocalPreview> {
        FileIo::Ready(self.local_preview(p))
    }
    fn preview_range(
        &self,
        p: &str,
        offset: u64,
        len: u64,
    ) -> FileIo<Result<LocalPreviewChunk, String>> {
        FileIo::Ready((|| {
            let fs = self.files.borrow();
            let bytes = fs
                .entries
                .get(p)
                .and_then(|e| e.data.as_deref())
                .ok_or("ENOENT")?;
            let a = usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            let b = usize::try_from(offset.saturating_add(len))
                .unwrap_or(usize::MAX)
                .min(bytes.len());
            Ok(LocalPreviewChunk {
                data: bytes[a..b].into(),
                eof: b == bytes.len(),
            })
        })())
    }
    fn can_pick_folder(&self) -> bool {
        super::can_pick_directory()
    }
    fn start_batch(
        &self,
        k: FileBatch,
        paths: Vec<String>,
        _: usize,
        progress: BatchProgress,
        done: BatchFinished,
        cx: &mut App,
    ) -> BatchHandle {
        let files = self.files.clone();
        let (dir, cut) = match k {
            FileBatch::BrowserDrop {
                destination,
                payload,
            } => {
                let target = crate::demo::TransferDestination::capture(
                    &files.borrow(),
                    &destination,
                    std::iter::empty(),
                );
                let task = cx.spawn(async move |_| {
                    let result = async {
                        let target = target?;
                        let mut reserved: std::collections::HashSet<_> =
                            payload.0.iter().map(|root| root.name()).collect();
                        for (index, root) in payload.0.into_iter().enumerate() {
                            let mut import = crate::browser_file::BrowserImport::open(root).await?;
                            while !import.complete() {
                                import.read_chunk().await?;
                            }
                            let name = import.name.clone();
                            let mut fs = files.borrow_mut();
                            let desired = Path::new(&destination).join(&name);
                            reserved.remove(&name);
                            let to = unique_copy_path_with(&desired, |p| {
                                fs.entries.contains_key(p.to_str().unwrap())
                                    || reserved.contains(p.file_name().unwrap().to_str().unwrap())
                            });
                            target.import_tree(
                                &mut fs,
                                to.to_str().unwrap(),
                                false,
                                import.take_tree(),
                            )?;
                            progress(LocalBatchProgress {
                                current: index + 1,
                                file_name: name,
                            });
                        }
                        Ok(())
                    }
                    .await;
                    done(result);
                });
                return Box::new(move || drop(task));
            }
            FileBatch::Paste { destination, mode } => {
                (Some(destination), matches!(mode, LocalClipboardMode::Cut))
            }
            FileBatch::Duplicate => (None, false),
        };
        let worker = cx.spawn(async move |cx| {
            let mut slice = web_time::Instant::now();
            for (index, path) in paths.iter().enumerate() {
                if let Err(error) = files
                    .borrow_mut()
                    .copy_batch_entry(path, dir.as_deref(), cut)
                {
                    return done(Err(error));
                }
                progress(LocalBatchProgress {
                    current: index + 1,
                    file_name: name(path).into(),
                });
                if slice.elapsed() >= std::time::Duration::from_millis(8) && index + 1 < paths.len()
                {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(1))
                        .await;
                    slice = web_time::Instant::now();
                }
            }
            done(Ok(()));
        });
        Box::new(move || drop(worker))
    }
}
