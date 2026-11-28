//! File Manager operations are scoped to the directory the user chose, not an OS path.
use super::device::{self, DeviceFolder, slice};
use super::preview;
use crate::browser_file::validate_filename;
use gpui::{App, Image, ImageFormat, ImageSource, Resource, Task};
use oxideterm_gpui_file_manager::filesystem::*;
use oxideterm_gpui_ide::portable::PortableIdeHost;
use oxideterm_ide_core::{FileKind, IdeFileError, IdeFileErrorKind};
use oxideterm_local_files::*;
use std::{
    cell::RefCell,
    collections::HashSet,
    path::{Path, PathBuf},
    rc::Rc,
    sync::Arc,
};
use web_sys::File;

mod transfer;

pub(super) fn bridge(root: DeviceFolder) -> Rc<dyn FileSystemBridge> {
    Rc::new(DeviceManager {
        root,
        preview: Default::default(),
    })
}

struct DeviceManager {
    root: DeviceFolder,
    preview: Rc<RefCell<Option<OpenFile>>>,
}

struct OpenFile {
    path: String,
    file: File,
    mime: String,
    image: Option<Arc<Image>>,
    font: Option<Vec<u8>>,
}

fn err(error: IdeFileError) -> String {
    match error.kind {
        IdeFileErrorKind::PermissionDenied => "sftp.toast.permission_denied",
        IdeFileErrorKind::Conflict => "sftp.conflict.description",
        IdeFileErrorKind::Unsupported => "fileManager.browserOperationUnavailable",
        IdeFileErrorKind::NotFound => "ENOENT",
        _ => "fileManager.browserFolderFailed",
    }
    .into()
}

fn child(dir: &str, name: &str) -> Result<String, String> {
    validate_filename(name)?;
    Ok(format!("{}/{}", dir.trim_end_matches('/'), name))
}

async fn entries(root: &DeviceFolder, path: &str) -> Result<Vec<LocalFileEntry>, String> {
    root.list("", path)
        .await
        .map_err(err)?
        .into_iter()
        .map(|e| {
            Ok(LocalFileEntry {
                path: child(path, &e.name)?,
                name: e.name,
                file_type: if e.kind == FileKind::Directory {
                    LocalFileType::Directory
                } else {
                    LocalFileType::File
                },
                size: e.version.size_bytes.unwrap_or_default(),
                modified: e.version.modified_millis.map(|ms| ms / 1000),
                symlink_target: None,
                readonly: false,
            })
        })
        .collect()
}

impl FileSystemBridge for DeviceManager {
    fn can_move(&self, entry: &LocalFileEntry) -> bool {
        device::can_move_entry(entry.is_directory_like())
    }

    fn device_name(&self) -> Option<&str> {
        Some(&self.root.name)
    }
    fn home(&self) -> String {
        "/".into()
    }
    fn sidebar_locations(&self) -> Vec<LocalSidebarLocation> {
        vec![LocalSidebarLocation {
            kind: LocalSidebarLocationKind::Home,
            path: self.home(),
        }]
    }
    fn drives(&self) -> Vec<LocalDrive> {
        Vec::new()
    }
    fn load_bookmarks(&self, _: &Path) -> FileIo<Result<Vec<LocalBookmark>, String>> {
        let root = self.root.clone();
        FileIo::Pending(Box::pin(async move { root.load_favorites().await }))
    }
    fn save_bookmarks(
        &self,
        _: &Path,
        before: &[LocalBookmark],
        next: &[LocalBookmark],
    ) -> FileIo<Result<(), String>> {
        let root = self.root.clone();
        let before = serde_json::to_string(before);
        let next = serde_json::to_string(next);
        FileIo::Pending(Box::pin(async move {
            root.save_favorites(
                before.map_err(|e| e.to_string())?,
                next.map_err(|e| e.to_string())?,
            )
            .await
        }))
    }
    fn list(&self, path: &str) -> FileIo<Result<Vec<LocalFileEntry>, String>> {
        let (root, path) = (self.root.clone(), path.to_owned());
        FileIo::Pending(Box::pin(async move { entries(&root, &path).await }))
    }
    fn mutate(&self, dir: &str, action: FileMutation) -> FileIo<Result<(), String>> {
        let (root, dir) = (self.root.clone(), dir.to_owned());
        FileIo::Pending(Box::pin(async move {
            match action {
                FileMutation::CreateFile(name) => root
                    .save_bytes(&child(&dir, &name)?, &[], None, true)
                    .await
                    .map(drop)
                    .map_err(err),
                FileMutation::CreateFolder(name) => root
                    .create_folder(String::new(), child(&dir, &name)?)
                    .await
                    .map_err(err),
                FileMutation::Rename { from, to } => root
                    .rename_item(String::new(), child(&dir, &from)?, child(&dir, &to)?)
                    .await
                    .map_err(err),
                FileMutation::Delete(names) => {
                    for name in names {
                        root.delete_item(String::new(), child(&dir, &name)?, true)
                            .await
                            .map_err(err)?;
                    }
                    Ok(())
                }
            }
        }))
    }

    fn properties(&self, e: &LocalFileEntry) -> LocalFileProperties {
        LocalFileProperties {
            location: e.path.clone(),
            size: e.size,
            modified: e.modified,
            accessed: None,
            readonly: e.readonly,
            dir_files: None,
            dir_dirs: None,
            total_size: None,
            created: None,
            mode: None,
            mime_type: (!e.is_directory_like())
                .then(|| mime_type_for_extension(&local_file_extension(&e.name)).into()),
            is_symlink: false,
        }
    }
    fn checksum(&self, path: String, cx: &App) -> Task<Result<LocalChecksumResult, String>> {
        let root = self.root.clone();
        cx.spawn(async move |_| {
            let file = root.snapshot(&path).await.map_err(err)?;
            let mut sums = FileChecksums::default();
            let size = file.size() as u64;
            for start in (0..size).step_by(256 * 1024) {
                sums.update(&slice(&file, start, (start + 256 * 1024).min(size)).await?);
            }
            Ok(sums.finish())
        })
    }
    fn metadata(&self, path: &str) -> Option<LocalPreviewMetadata> {
        let state = self.preview.borrow();
        let f = state.as_ref().filter(|f| f.path == path)?;
        Some(LocalPreviewMetadata {
            size: f.file.size() as u64,
            modified: Some(f.file.last_modified() as i64 / 1000),
            created: None,
            accessed: None,
            readonly: false,
            is_dir: false,
            is_symlink: false,
            mode: None,
            mime_type: Some(f.mime.clone()),
        })
    }
    fn font_bytes(&self, path: &str) -> Result<Vec<u8>, String> {
        self.preview
            .borrow_mut()
            .as_mut()
            .filter(|p| p.path == path)
            .and_then(|p| p.font.take())
            .ok_or_else(|| "fileManager.browserImportReadFailed".into())
    }
    fn image(&self, path: &str, turns: u8) -> ImageSource {
        let state = self.preview.borrow();
        match state
            .as_ref()
            .filter(|p| p.path == path)
            .and_then(|p| p.image.clone())
        {
            Some(image) if turns % 4 == 0 => image.into(),
            Some(image) => ImageSource::Resource(Resource::RotatedBytes {
                image,
                quarter_turns: turns % 4,
            }),
            None => "lucide/file.svg".into(),
        }
    }
    fn release_preview(&self) {
        self.preview.borrow_mut().take();
    }
    fn preview(&self, path: &str) -> FileIo<LocalPreview> {
        self.release_preview();
        let (root, path, slot) = (self.root.clone(), path.to_owned(), self.preview.clone());
        FileIo::Pending(Box::pin(async move {
            let result = async {
                let file = root.snapshot(&path).await.map_err(err)?;
                let size = file.size() as u64;
                let prefix = slice(&file, 0, size.min(8192)).await?;
                let (kind, mime) = preview::format(&path, &prefix);
                let mut open = OpenFile {
                    path: path.clone(),
                    file,
                    mime: mime.clone(),
                    image: None,
                    font: None,
                };
                let p = if size > STREAM_PREVIEW_THRESHOLD
                    && kind == oxideterm_preview::PreviewKind::Text
                    && mime != "text/markdown"
                    && !prefix.starts_with(&[0xef, 0xbb, 0xbf])
                    && std::str::from_utf8(&prefix)
                        .err()
                        .is_none_or(|e| e.error_len().is_none())
                {
                    LocalPreview::TextStream {
                        path: path.clone(),
                        size,
                        language: preview::language(&path),
                    }
                } else if size > MAX_PREVIEW_SIZE {
                    LocalPreview::TooLarge { size }
                } else {
                    let mut bytes = prefix;
                    if (bytes.len() as u64) < size {
                        bytes.extend(slice(&open.file, bytes.len() as u64, size).await?);
                    }
                    let p = preview::local_preview_bytes(&path, &bytes);
                    match &p {
                        LocalPreview::Image { .. } => {
                            open.image = ImageFormat::from_mime_type(&mime)
                                .map(|f| Arc::new(Image::from_bytes(f, bytes)))
                        }
                        LocalPreview::Font { .. } => open.font = Some(bytes),
                        _ => {}
                    }
                    p
                };
                slot.replace(Some(open));
                Ok::<_, String>(p)
            }
            .await;
            result.unwrap_or_else(LocalPreview::Error)
        }))
    }
    fn preview_range(
        &self,
        path: &str,
        offset: u64,
        len: u64,
    ) -> FileIo<Result<LocalPreviewChunk, String>> {
        let file = self
            .preview
            .borrow()
            .as_ref()
            .filter(|f| f.path == path)
            .map(|f| f.file.clone());
        FileIo::Pending(Box::pin(async move {
            let f = file.ok_or("fileManager.browserImportReadFailed")?;
            let size = f.size() as u64;
            let end = size.min(offset.saturating_add(len));
            Ok(LocalPreviewChunk {
                data: slice(&f, offset.min(size), end).await?,
                eof: end == size,
            })
        }))
    }
    fn can_pick_folder(&self) -> bool {
        device::can_pick_directory()
    }
    fn download(&self, path: &str) -> FileIo<Result<(), String>> {
        let (root, path) = (self.root.clone(), path.to_owned());
        FileIo::Pending(Box::pin(async move {
            let file = root.snapshot(&path).await.map_err(err)?;
            crate::browser_file::download_blob(&file.name(), &file)
                .map_err(|_| "sftp.toast.download_failed".into())
        }))
    }
    fn batch_units(&self, paths: &[String]) -> usize {
        paths.len()
    }
    fn start_batch(
        &self,
        kind: FileBatch,
        paths: Vec<String>,
        _: usize,
        progress: BatchProgress,
        done: BatchFinished,
        cx: &mut App,
    ) -> BatchHandle {
        let root = self.root.clone();
        let task =
            cx.spawn(async move |_| done(transfer::batch(root, kind, paths, progress).await));
        Box::new(move || drop(task))
    }
    fn compress(&self, sources: &[String], destination: &Path) -> FileIo<Result<PathBuf, String>> {
        let (root, paths, to) = (self.root.clone(), sources.to_vec(), destination.to_owned());
        FileIo::Pending(Box::pin(async move {
            transfer::compress(&root, paths, &to).await
        }))
    }
    fn can_extract(&self, name: &str) -> bool {
        mime_type_for_extension(&local_file_extension(name)) == "application/zip"
    }
    fn extract(&self, archive: &str, destination: &str) -> FileIo<Result<(), String>> {
        let (root, path, to) = (
            self.root.clone(),
            archive.to_owned(),
            destination.to_owned(),
        );
        FileIo::Pending(Box::pin(async move {
            transfer::extract(&root, &path, &to).await
        }))
    }
}
