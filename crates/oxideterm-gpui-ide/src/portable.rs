//! File operations for the shared IDE surface on a browser's event loop.
//! Node IDs name host-owned projects; the view never owns a transport.
//!
//! Pass an `Rc<dyn PortableIdeHost>` to [`IdeSurface::new`](crate::IdeSurface::new).
//! Mount the surface and its [`IdeModalLayer`](crate::IdeModalLayer) together
//! so dirty-close and save-conflict prompts can receive input.
use oxideterm_ide_core::{
    FileStat, FileSystemCapabilities, FileTreeEntry, IdeFileData, IdeFileError, IdeLocation,
    IdeProjectInfo, IdeSearchQuery, SavedFileVersion, SharedText, TextFileFormat, WriteMode,
};
use std::{future::Future, pin::Pin};

/// Host futures may borrow browser handles and need not be `Send`.
pub type FileFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, IdeFileError>> + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdeSearchMatch {
    pub path: String,
    pub line: u32,
    pub column: u32,
    pub preview: String,
    pub match_start: usize,
    pub match_end: usize,
}

pub trait PortableIdeHost {
    fn capabilities(&self) -> FileSystemCapabilities;
    fn open_project(&self, node: String, path: String) -> FileFuture<'_, IdeProjectInfo>;
    fn list_dir<'a>(&'a self, location: &'a IdeLocation) -> FileFuture<'a, Vec<FileTreeEntry>>;
    /// Return decoded editable text, or the appropriate binary, size, or encoding error.
    fn read_file<'a>(
        &'a self,
        location: &'a IdeLocation,
        encoding: Option<&'a str>,
    ) -> FileFuture<'a, IdeFileData>;
    fn stat<'a>(&'a self, location: &'a IdeLocation) -> FileFuture<'a, FileStat>;
    /// Preserve the requested format. If the expected version no longer matches,
    /// return a conflict without writing; an explicit overwrite omits the version.
    fn write_file<'a>(
        &'a self,
        location: &'a IdeLocation,
        text: &'a SharedText,
        format: &'a TextFileFormat,
        expected: Option<&'a SavedFileVersion>,
        mode: WriteMode,
    ) -> FileFuture<'a, SavedFileVersion>;
    fn create_file(&self, node: String, path: String) -> FileFuture<'_, SavedFileVersion>;
    fn create_folder(&self, node: String, path: String) -> FileFuture<'_, ()>;
    fn rename_item(&self, node: String, from: String, to: String) -> FileFuture<'_, ()>;
    fn copy_item(&self, node: String, from: String, to: String) -> FileFuture<'_, ()>;
    fn delete_item(&self, node: String, path: String, recursive: bool) -> FileFuture<'_, ()>;
    fn search_project(
        &self,
        node: String,
        query: IdeSearchQuery,
    ) -> FileFuture<'_, Vec<IdeSearchMatch>>;
}
