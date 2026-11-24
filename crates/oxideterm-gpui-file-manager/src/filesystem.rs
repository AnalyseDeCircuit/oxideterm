use gpui::{App, ImageSource, SharedString, Task};
use oxideterm_local_files::{
    LocalBatchProgress, LocalBookmark, LocalChecksumResult, LocalClipboardMode, LocalDrive,
    LocalFileEntry, LocalFileProperties, LocalPreview, LocalPreviewChunk, LocalPreviewMetadata,
    LocalSidebarLocation,
};
use oxideterm_preview::AudioPreviewBackend;
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
};

#[cfg(not(target_family = "wasm"))]
pub(crate) mod desktop;

pub enum FileBatch {
    Paste {
        destination: String,
        mode: LocalClipboardMode,
    },
    Duplicate,
    Drop {
        destination: String,
    },
}

pub type FolderSelection = Pin<Box<dyn Future<Output = Option<PathBuf>>>>;
/// Capture the caller before the picker yields; begin on selection and finish after listing.
pub type BrowseAudit = Box<dyn FnOnce(&str) -> Box<dyn FnOnce(bool)>>;
pub type BatchProgress = Box<dyn Fn(LocalBatchProgress) + Send>;
pub type BatchFinished = Box<dyn FnOnce(Result<(), String>) + Send>;
/// The view retains the worker until completion, then calls this to reap it.
/// Dropping the view releases the handle without waiting on the UI thread.
pub type BatchHandle = Box<dyn FnOnce()>;

/// Host operations for one file manager. Paths identify entries in the host filesystem.
pub trait FileSystemBridge {
    fn home(&self) -> String;
    fn sidebar_locations(&self) -> Vec<LocalSidebarLocation>;
    fn drives(&self) -> Vec<LocalDrive>;
    fn load_bookmarks(&self, path: &Path) -> Vec<LocalBookmark>;
    fn save_bookmarks(&self, path: &Path, bookmarks: &[LocalBookmark]) -> Result<(), String>;
    fn reorder_bookmarks(
        &self,
        bookmarks: &mut Vec<LocalBookmark>,
        path: &Path,
        source: &str,
        target: &str,
        after: bool,
    ) -> Result<bool, String>;
    fn list(&self, path: &str) -> Result<Vec<LocalFileEntry>, String>;
    fn create_folder(&self, dir: &str, name: &str) -> Result<(), String>;
    fn create_file(&self, dir: &str, name: &str) -> Result<(), String>;
    fn rename(&self, dir: &str, old: &str, new: &str) -> Result<(), String>;
    fn delete(&self, dir: &str, names: &[String]) -> Option<String>;
    fn resolve_drop(&self, paths: &[PathBuf]) -> Vec<String>;
    fn batch_units(&self, paths: &[String]) -> usize;
    fn compress(&self, sources: &[String], destination: &Path) -> Result<PathBuf, String>;
    fn extract(&self, archive: &str, destination: &str) -> Result<(), String>;
    fn properties(&self, entry: &LocalFileEntry) -> LocalFileProperties;
    fn checksum(&self, path: String, cx: &App) -> Task<Result<LocalChecksumResult, String>>;
    fn metadata(&self, path: &str) -> Option<LocalPreviewMetadata>;
    fn font_bytes(&self, path: &str) -> Result<Vec<u8>, String>;
    fn image(&self, path: &str, quarter_turns: u8) -> ImageSource;
    fn open_external(&self, path: &str) -> Result<(), String>;
    fn reveal(&self, path: &str) -> Result<(), String>;
    fn audio(&self) -> Box<dyn AudioPreviewBackend> {
        Box::new(oxideterm_preview::UnsupportedAudioPreviewBackend)
    }
    fn preview(&self, path: &str) -> LocalPreview;
    fn preview_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> Result<LocalPreviewChunk, String>;
    fn pick_folder(&self, prompt: SharedString, cx: &mut App) -> FolderSelection;
    fn browse_audit(&self) -> BrowseAudit {
        Box::new(|_| Box::new(|_| {}))
    }
    /// Report progress in source units, then call `finished` once as the worker's final action.
    fn start_batch(
        &self,
        kind: FileBatch,
        paths: Vec<String>,
        total: usize,
        progress: BatchProgress,
        finished: BatchFinished,
        cx: &mut App,
    ) -> BatchHandle;
}
