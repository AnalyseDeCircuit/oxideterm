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

pub struct MemoryFs {
    pub files: Files,
    bookmarks: RefCell<Vec<LocalBookmark>>,
}
impl MemoryFs {
    pub fn new(files: Files) -> Self {
        Self {
            files,
            bookmarks: Default::default(),
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
        let mut fs = self.files.borrow_mut();
        if !fs.entries.contains_key(p) {
            return Err("ENOENT".into());
        }
        if !recursive && fs.list(p).is_ok_and(|v| !v.is_empty()) {
            return Err("ENOTEMPTY".into());
        }
        fs.entries.retain(|k, _| !Path::new(k).starts_with(p));
        Ok(())
    }
    pub fn duplicate(&self, from: &str, to: &str, cut: bool) -> Result<(), String> {
        let mut fs = self.files.borrow_mut();
        if fs.entries.contains_key(to) || Path::new(to).starts_with(from) {
            return Err("EEXIST".into());
        }
        fs.list(
            Path::new(to)
                .parent()
                .and_then(Path::to_str)
                .ok_or("EINVAL")?,
        )?;
        if !fs.entries.contains_key(from) {
            return Err("ENOENT".into());
        }
        let items = fs
            .entries
            .iter()
            .filter(|(k, _)| Path::new(k).starts_with(from))
            .map(|(k, v)| (k.clone(), v.data.clone()))
            .collect::<Vec<_>>();
        for (key, data) in items {
            fs.put(&format!("{to}{}", &key[from.len()..]), data);
            if cut {
                fs.entries.remove(&key);
            }
        }
        Ok(())
    }
    pub fn sftp_list(&self, p: &str) -> Result<Vec<SftpFileEntry>, String> {
        Ok(self
            .files
            .borrow()
            .list(p)?
            .into_iter()
            .map(|(path, e)| SftpFileEntry {
                name: name(path).into(),
                path: path.into(),
                file_type: if e.data.is_some() {
                    SftpFileType::File
                } else {
                    SftpFileType::Directory
                },
                size: e.data.as_ref().map_or(0, |v| v.len() as u64),
                size_known: true,
                modified: None,
                permissions: None,
                owner: Some("demo".into()),
                group: None,
                is_symlink: false,
                symlink_target: None,
            })
            .collect())
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
    fn image(&self, _: &str) -> ImageSource {
        "lucide/file.svg".into()
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
    fn load_bookmarks(&self, _: &Path) -> Vec<LocalBookmark> {
        self.bookmarks.borrow().clone()
    }
    fn save_bookmarks(&self, _: &Path, b: &[LocalBookmark]) -> Result<(), String> {
        self.bookmarks.replace(b.to_vec());
        Ok(())
    }
    fn reorder_bookmarks(
        &self,
        b: &mut Vec<LocalBookmark>,
        p: &Path,
        from: &str,
        to: &str,
        after: bool,
    ) -> Result<bool, String> {
        if from == to {
            return Ok(false);
        }
        let (Some(i), Some(_)) = (
            b.iter().position(|v| v.id == from),
            b.iter().position(|v| v.id == to),
        ) else {
            return Ok(false);
        };
        let v = b.remove(i);
        let j = b.iter().position(|v| v.id == to).unwrap();
        b.insert(j + usize::from(after), v);
        self.save_bookmarks(p, b)?;
        Ok(true)
    }
    fn list(&self, p: &str) -> Result<Vec<LocalFileEntry>, String> {
        Ok(self
            .files
            .borrow()
            .list(p)?
            .into_iter()
            .map(|(path, e)| LocalFileEntry {
                name: name(path).into(),
                path: path.into(),
                file_type: if e.data.is_none() {
                    LocalFileType::Directory
                } else {
                    LocalFileType::File
                },
                size: e.data.as_ref().map_or(0, |v| v.len() as u64),
                modified: None,
                readonly: false,
                symlink_target: None,
            })
            .collect())
    }
    fn create_folder(&self, d: &str, n: &str) -> Result<(), String> {
        self.create(&child(d, n)?, true)
    }
    fn create_file(&self, d: &str, n: &str) -> Result<(), String> {
        self.create(&child(d, n)?, false)
    }
    fn rename(&self, d: &str, a: &str, b: &str) -> Result<(), String> {
        self.duplicate(&child(d, a)?, &child(d, b)?, true)
    }
    fn delete(&self, d: &str, n: &[String]) -> Option<String> {
        n.iter()
            .try_for_each(|n| self.remove(&child(d, n)?, true))
            .err()
    }
    fn resolve_drop(&self, p: &[PathBuf]) -> Vec<String> {
        p.iter()
            .filter_map(|p| p.to_str())
            .filter(|p| self.files.borrow().entries.contains_key(*p))
            .map(str::to_owned)
            .collect()
    }
    fn batch_units(&self, p: &[String]) -> usize {
        p.len()
    }
    fn compress(&self, _: &[String], _: &Path) -> Result<PathBuf, String> {
        Err("ENOTSUP".into())
    }
    fn extract(&self, _: &str, _: &str) -> Result<(), String> {
        Err("ENOTSUP".into())
    }
    fn properties(&self, e: &LocalFileEntry) -> LocalFileProperties {
        LocalFileProperties {
            location: e.path.clone(),
            size: e.size,
            modified: None,
            accessed: None,
            readonly: false,
            dir_files: None,
            dir_dirs: None,
            total_size: None,
            created: None,
            mode: None,
            mime_type: None,
            is_symlink: false,
        }
    }
    fn checksum(&self, _: String, _: &App) -> Task<Result<LocalChecksumResult, String>> {
        Task::ready(Err("ENOTSUP".into()))
    }
    fn metadata(&self, p: &str) -> Option<LocalPreviewMetadata> {
        self.files
            .borrow()
            .entries
            .get(p)
            .map(|e| LocalPreviewMetadata {
                size: e.data.as_ref().map_or(0, |v| v.len() as u64),
                modified: None,
                created: None,
                accessed: None,
                readonly: false,
                is_dir: e.data.is_none(),
                is_symlink: false,
                mode: None,
                mime_type: Some("text/plain".into()),
            })
    }
    fn font_bytes(&self, p: &str) -> Result<Vec<u8>, String> {
        self.bytes(p)
    }
    fn image(&self, _: &str, _: u8) -> ImageSource {
        "lucide/file.svg".into()
    }
    fn open_external(&self, _: &str) -> Result<(), String> {
        Err("ENOTSUP".into())
    }
    fn reveal(&self, _: &str) -> Result<(), String> {
        Err("ENOTSUP".into())
    }
    fn preview(&self, p: &str) -> LocalPreview {
        match self
            .bytes(p)
            .and_then(|v| String::from_utf8(v).map_err(|_| "EILSEQ".into()))
        {
            Ok(content) => LocalPreview::Text {
                content,
                language: None,
            },
            Err(e) => LocalPreview::Error(e),
        }
    }
    fn preview_range(&self, p: &str, offset: u64, len: u64) -> Result<LocalPreviewChunk, String> {
        let bytes = self.bytes(p)?;
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
    }
    fn pick_folder(&self, _: SharedString, _: &mut App) -> FolderSelection {
        Box::pin(async { None })
    }
    fn start_batch(
        &self,
        k: FileBatch,
        paths: Vec<String>,
        _: usize,
        progress: BatchProgress,
        done: BatchFinished,
        _: &mut App,
    ) -> BatchHandle {
        done(paths.iter().enumerate().try_for_each(|(i, path)| {
            let n = name(path);
            let (dest, cut) = match &k {
                FileBatch::Paste { destination, mode } => (
                    child(destination, n)?,
                    matches!(mode, LocalClipboardMode::Cut),
                ),
                FileBatch::Duplicate => (format!("{path}.copy"), false),
                FileBatch::Drop { destination } => (child(destination, n)?, false),
            };
            self.duplicate(path, &dest, cut)?;
            progress(LocalBatchProgress {
                current: i + 1,
                file_name: n.into(),
            });
            Ok(())
        }));
        Box::new(|| {})
    }
}
