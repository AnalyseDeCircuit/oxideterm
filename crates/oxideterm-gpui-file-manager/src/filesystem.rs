use gpui::{App, ImageSource, Task};
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
    #[cfg(target_family = "wasm")]
    BrowserDrop {
        destination: String,
        payload: gpui::BrowserFileDrop,
    },
    Paste {
        destination: String,
        mode: LocalClipboardMode,
    },
    Duplicate,
    #[cfg(not(target_family = "wasm"))]
    Drop {
        destination: String,
    },
}

#[cfg(not(target_family = "wasm"))]
pub type FolderSelection = Pin<Box<dyn Future<Output = Option<PathBuf>>>>;

/// A desktop listing is available immediately; a browser directory handle yields to the event loop.
pub enum FileIo<T> {
    Ready(T),
    Pending(Pin<Box<dyn Future<Output = T>>>),
}

impl<T: 'static> FileIo<T> {
    pub(crate) fn deliver<V: 'static>(
        self,
        owner: &mut V,
        cx: &mut gpui::Context<V>,
        receive: impl FnOnce(&mut V, T, &mut gpui::Context<V>) + 'static,
    ) -> Option<Task<()>> {
        match self {
            Self::Ready(value) => {
                receive(owner, value, cx);
                None
            }
            Self::Pending(work) => Some(cx.spawn(async move |owner, cx| {
                let value = work.await;
                let _ = owner.update(cx, |owner, cx| receive(owner, value, cx));
            })),
        }
    }
}
/// Capture the caller before the picker yields; begin on selection and finish after listing.
pub type BrowseAudit = Box<dyn FnOnce(&str) -> Box<dyn FnOnce(bool)>>;
pub type BatchProgress = Box<dyn Fn(LocalBatchProgress) + Send>;
pub type BatchFinished = Box<dyn FnOnce(Result<(), String>) + Send>;
/// The view retains the worker until completion, then calls this to reap it.
/// Dropping the view releases the handle without waiting on the UI thread.
pub type BatchHandle = Box<dyn FnOnce()>;

pub enum FileMutation {
    CreateFolder(String),
    CreateFile(String),
    Rename { from: String, to: String },
    Delete(Vec<String>),
}

/// Host operations for one file manager. Paths identify entries in the host filesystem.
pub trait FileSystemBridge {
    fn home(&self) -> String;
    fn can_move(&self, _entry: &LocalFileEntry) -> bool {
        true
    }
    fn device_name(&self) -> Option<&str> {
        None
    }
    fn sidebar_locations(&self) -> Vec<LocalSidebarLocation>;
    fn drives(&self) -> Vec<LocalDrive>;
    fn load_bookmarks(&self, path: &Path) -> FileIo<Result<Vec<LocalBookmark>, String>>;
    fn save_bookmarks(
        &self,
        path: &Path,
        before: &[LocalBookmark],
        next: &[LocalBookmark],
    ) -> FileIo<Result<(), String>>;
    fn list(&self, path: &str) -> FileIo<Result<Vec<LocalFileEntry>, String>>;
    fn mutate(&self, dir: &str, action: FileMutation) -> FileIo<Result<(), String>>;
    #[cfg(not(target_family = "wasm"))]
    fn resolve_drop(&self, paths: &[PathBuf]) -> Vec<String>;
    fn batch_units(&self, paths: &[String]) -> usize;
    fn compress(&self, sources: &[String], destination: &Path) -> FileIo<Result<PathBuf, String>>;
    fn can_extract(&self, name: &str) -> bool {
        oxideterm_local_files::can_extract_archive(name)
    }
    fn extract(&self, archive: &str, destination: &str) -> FileIo<Result<(), String>>;
    fn properties(&self, entry: &LocalFileEntry) -> LocalFileProperties;
    fn checksum(&self, path: String, cx: &App) -> Task<Result<LocalChecksumResult, String>>;
    fn metadata(&self, path: &str) -> Option<LocalPreviewMetadata>;
    fn font_bytes(&self, path: &str) -> Result<Vec<u8>, String>;
    fn image(&self, path: &str, quarter_turns: u8) -> ImageSource;
    #[cfg(not(target_family = "wasm"))]
    fn open_external(&self, path: &str) -> Result<(), String>;
    #[cfg(not(target_family = "wasm"))]
    fn reveal(&self, path: &str) -> Result<(), String>;
    #[cfg(target_family = "wasm")]
    fn download(&self, path: &str) -> FileIo<Result<(), String>>;
    fn audio(&self) -> Box<dyn AudioPreviewBackend> {
        Box::new(oxideterm_preview::UnsupportedAudioPreviewBackend)
    }
    fn release_preview(&self) {}
    fn preview(&self, path: &str) -> FileIo<LocalPreview>;
    fn preview_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
    ) -> FileIo<Result<LocalPreviewChunk, String>>;
    fn can_pick_folder(&self) -> bool {
        true
    }
    #[cfg(not(target_family = "wasm"))]
    fn pick_folder(&self, prompt: gpui::SharedString, cx: &mut App) -> FolderSelection;
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
