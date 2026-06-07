//! Filesystem manipulation operations.
//!
//! This module contains methods for interacting with remote entities on high-level.
//! The architecture is quite simple because it is built as an analogue of [`std::fs`]

mod dir;
mod file;

use crate::protocol::FileAttributes;

pub use dir::{DirEntry, ReadDir};
pub use file::{
    File, FileDownloadParts, FileUploadParts, PipelinedFileDownloader, PipelinedFileUploader,
    PipelinedReadChunk,
};
pub type Metadata = FileAttributes;
