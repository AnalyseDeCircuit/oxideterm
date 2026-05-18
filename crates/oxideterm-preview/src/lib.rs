// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Native preview core shared by SFTP and the local file manager.
//!
//! The crate owns preview classification, asset lifetime, and backend-facing
//! state. Feature UIs keep file-manager/SFTP business state outside this crate.

mod asset;
mod audio;
mod renderer;
mod session;
mod text;
mod types;
mod video;

pub use asset::{PreviewAssetOwner, PreviewAssetOwnership};
pub use audio::{
    AudioPreviewBackend, AudioPreviewCommand, AudioPreviewSnapshot, AudioPreviewState,
    MemoryAudioPreviewBackend, RodioAudioPreviewBackend, UnsupportedAudioPreviewBackend,
};
pub use renderer::PreviewRenderer;
pub use session::{
    PreviewAction, PreviewLoadError, PreviewLoadOptions, PreviewSession, PreviewSessionState,
    PreviewSource,
};
pub use text::{
    detect_and_decode, detect_and_decode_with_hint, encode_to_encoding, extension_to_language,
    generate_hex_dump, is_likely_text_content,
};
pub use types::{
    PreviewAssetKind, PreviewContent, PreviewKind, classify_preview_path, classify_preview_type,
    font_family_name_from_bytes, font_mime_type, is_font_extension,
};
pub use video::{
    PlatformVideoBackend, PlatformVideoSnapshot, PlatformVideoState,
    UnsupportedPlatformVideoBackend, VideoPreviewCommand,
};
