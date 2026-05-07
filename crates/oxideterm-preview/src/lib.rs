// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native preview core shared by SFTP and the local file manager.
//!
//! The crate owns preview classification, asset lifetime, and backend-facing
//! state. Feature UIs keep file-manager/SFTP business state outside this crate.

mod asset;
mod audio;
mod pdf;
mod renderer;
mod session;
mod types;
mod video;

pub use asset::{PreviewAssetOwner, PreviewAssetOwnership};
pub use audio::{
    AudioPreviewBackend, AudioPreviewCommand, AudioPreviewSnapshot, AudioPreviewState,
    MemoryAudioPreviewBackend, UnsupportedAudioPreviewBackend,
};
pub use pdf::{
    PdfDocumentInfo, PdfPageBitmap, PdfPreviewBackend, PdfPreviewError, PdfiumPreviewBackend,
};
pub use renderer::PreviewRenderer;
pub use session::{PreviewAction, PreviewSession, PreviewSessionState, PreviewSource};
pub use types::{
    PreviewAssetKind, PreviewContent, PreviewKind, classify_preview_path, classify_preview_type,
};
pub use video::{
    PlatformVideoBackend, PlatformVideoSnapshot, PlatformVideoState,
    UnsupportedPlatformVideoBackend, VideoPreviewCommand,
};
