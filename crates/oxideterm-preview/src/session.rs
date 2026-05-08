// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
};

use crate::{
    PreviewAssetKind, PreviewAssetOwner, PreviewContent, PreviewKind, classify_preview_path,
    detect_and_decode_with_hint, extension_to_language, generate_hex_dump, is_likely_text_content,
};
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

#[derive(Clone, Debug)]
pub enum PreviewSource {
    LocalPath {
        path: PathBuf,
        mime_type: Option<String>,
        encoding_hint: Option<String>,
    },
    OwnedTempAsset(PreviewAssetOwner),
    Inline(PreviewContent),
}

#[derive(Clone, Debug)]
pub struct PreviewLoadOptions {
    pub max_text_size: u64,
    pub max_preview_size: u64,
    pub max_media_preview_size: u64,
    pub hex_chunk_size: u64,
    pub hex_offset: u64,
    pub mmap_threshold: u64,
    pub encoding_hint: Option<String>,
}

impl Default for PreviewLoadOptions {
    fn default() -> Self {
        Self {
            max_text_size: 1024 * 1024,
            max_preview_size: 10 * 1024 * 1024,
            max_media_preview_size: 50 * 1024 * 1024,
            hex_chunk_size: 16 * 1024,
            hex_offset: 0,
            mmap_threshold: 1024 * 1024,
            encoding_hint: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum PreviewLoadError {
    #[error("failed to read preview source: {0}")]
    Io(#[from] std::io::Error),
    #[error("preview I/O task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
    #[error("preview source is a directory")]
    Directory,
}

#[derive(Clone, Debug)]
pub enum PreviewSessionState {
    Empty,
    Loading,
    Ready {
        content: PreviewContent,
        asset: Option<PreviewAssetOwner>,
    },
    Error(String),
}

impl Default for PreviewSessionState {
    fn default() -> Self {
        Self::Empty
    }
}

#[derive(Clone, Debug, Default)]
pub struct PreviewSession {
    state: PreviewSessionState,
    zoom: f32,
    rotation_degrees: i32,
    metadata_visible: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PreviewAction {
    Close,
    Previous,
    Next,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    RotateClockwise,
    ToggleMetadata,
    PlayPause,
    Seek(f64),
}

impl PreviewSession {
    pub async fn load(source: PreviewSource) -> Self {
        Self::load_with_options(source, PreviewLoadOptions::default()).await
    }

    pub async fn load_with_options(source: PreviewSource, options: PreviewLoadOptions) -> Self {
        match Self::try_load_with_options(source, options).await {
            Ok(session) => session,
            Err(error) => Self::error(error.to_string()),
        }
    }

    pub async fn try_load_with_options(
        source: PreviewSource,
        options: PreviewLoadOptions,
    ) -> Result<Self, PreviewLoadError> {
        match source {
            PreviewSource::Inline(content) => Ok(Self::ready(content, None)),
            PreviewSource::OwnedTempAsset(asset) => {
                let content = PreviewContent::AssetFile {
                    path: asset.path().to_string_lossy().to_string(),
                    mime_type: asset.mime_type().to_string(),
                    kind: asset.kind(),
                };
                Ok(Self::ready(content, Some(asset)))
            }
            PreviewSource::LocalPath {
                path,
                mime_type,
                encoding_hint,
            } => {
                let loaded = load_local_path(path, mime_type, encoding_hint, options).await?;
                Ok(Self::ready(loaded.content, loaded.asset))
            }
        }
    }

    pub fn ready(content: PreviewContent, asset: Option<PreviewAssetOwner>) -> Self {
        Self {
            state: PreviewSessionState::Ready { content, asset },
            zoom: 1.0,
            rotation_degrees: 0,
            metadata_visible: true,
        }
    }

    pub fn loading() -> Self {
        Self {
            state: PreviewSessionState::Loading,
            zoom: 1.0,
            rotation_degrees: 0,
            metadata_visible: true,
        }
    }

    pub fn error(error: impl Into<String>) -> Self {
        Self {
            state: PreviewSessionState::Error(error.into()),
            zoom: 1.0,
            rotation_degrees: 0,
            metadata_visible: true,
        }
    }

    pub fn state(&self) -> &PreviewSessionState {
        &self.state
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn rotation_degrees(&self) -> i32 {
        self.rotation_degrees
    }

    pub fn metadata_visible(&self) -> bool {
        self.metadata_visible
    }

    pub fn apply(&mut self, action: PreviewAction) {
        match action {
            PreviewAction::ZoomIn => self.zoom = (self.zoom + 0.25).min(3.0),
            PreviewAction::ZoomOut => self.zoom = (self.zoom - 0.25).max(0.25),
            PreviewAction::ResetZoom => {
                self.zoom = 1.0;
                self.rotation_degrees = 0;
            }
            PreviewAction::RotateClockwise => {
                self.rotation_degrees = (self.rotation_degrees + 90) % 360;
            }
            PreviewAction::ToggleMetadata => self.metadata_visible = !self.metadata_visible,
            PreviewAction::Close
            | PreviewAction::Previous
            | PreviewAction::Next
            | PreviewAction::PlayPause
            | PreviewAction::Seek(_) => {}
        }
    }
}

struct LoadedPreview {
    content: PreviewContent,
    asset: Option<PreviewAssetOwner>,
}

async fn load_local_path(
    path: PathBuf,
    mime_type: Option<String>,
    encoding_hint: Option<String>,
    options: PreviewLoadOptions,
) -> Result<LoadedPreview, PreviewLoadError> {
    let metadata = tokio::fs::metadata(&path).await?;
    if metadata.is_dir() {
        return Err(PreviewLoadError::Directory);
    }

    let size = metadata.len();
    let mime_type = mime_type.unwrap_or_else(|| {
        mime_guess::from_path(&path)
            .first_or_octet_stream()
            .essence_str()
            .to_string()
    });
    let kind = classify_preview_path(&path);
    match kind {
        PreviewKind::Image | PreviewKind::Pdf | PreviewKind::Office | PreviewKind::Font => {
            let asset_kind = preview_asset_kind(kind);
            load_local_asset(path, mime_type, asset_kind, size, options.max_preview_size)
        }
        PreviewKind::Audio | PreviewKind::Video => load_local_asset(
            path,
            mime_type,
            preview_asset_kind(kind),
            size,
            options.max_media_preview_size,
        ),
        PreviewKind::Text => load_local_text(path, mime_type, size, encoding_hint, options).await,
        PreviewKind::Hex | PreviewKind::Unsupported => load_local_hex(path, size, options).await,
        PreviewKind::TooLarge => Ok(LoadedPreview {
            content: PreviewContent::TooLarge {
                size,
                max_size: options.max_preview_size,
                recommend_download: true,
            },
            asset: None,
        }),
    }
}

fn load_local_asset(
    path: PathBuf,
    mime_type: String,
    kind: PreviewAssetKind,
    size: u64,
    max_size: u64,
) -> Result<LoadedPreview, PreviewLoadError> {
    if size > max_size {
        return Ok(LoadedPreview {
            content: PreviewContent::TooLarge {
                size,
                max_size,
                recommend_download: true,
            },
            asset: None,
        });
    }
    let content = PreviewContent::AssetFile {
        path: path.to_string_lossy().to_string(),
        mime_type: mime_type.clone(),
        kind,
    };
    Ok(LoadedPreview {
        content,
        asset: Some(PreviewAssetOwner::local(path, mime_type, kind)),
    })
}

async fn load_local_text(
    path: PathBuf,
    mime_type: String,
    size: u64,
    source_encoding_hint: Option<String>,
    options: PreviewLoadOptions,
) -> Result<LoadedPreview, PreviewLoadError> {
    if size > options.max_text_size {
        return Ok(LoadedPreview {
            content: PreviewContent::TooLarge {
                size,
                max_size: options.max_text_size,
                recommend_download: true,
            },
            asset: None,
        });
    }

    let bytes = read_local_range(&path, 0, size as usize, options.mmap_threshold).await?;
    let encoding_hint = source_encoding_hint
        .as_deref()
        .or(options.encoding_hint.as_deref());
    let (data, encoding, confidence, has_bom) = detect_and_decode_with_hint(&bytes, encoding_hint);
    Ok(LoadedPreview {
        content: PreviewContent::Text {
            data,
            mime_type: Some(mime_type),
            language: path
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(extension_to_language),
            encoding,
            confidence,
            has_bom,
        },
        asset: None,
    })
}

async fn load_local_hex(
    path: PathBuf,
    total_size: u64,
    options: PreviewLoadOptions,
) -> Result<LoadedPreview, PreviewLoadError> {
    let offset = options.hex_offset.min(total_size);
    let bytes_to_read = options
        .hex_chunk_size
        .min(total_size.saturating_sub(offset)) as usize;
    let bytes = read_local_range(&path, offset, bytes_to_read, options.mmap_threshold).await?;
    if offset == 0 && total_size <= options.max_text_size && is_likely_text_content(&bytes) {
        let mime_type = mime_guess::from_path(&path)
            .first_or_text_plain()
            .essence_str()
            .to_string();
        return load_local_text(path, mime_type, total_size, None, options).await;
    }

    Ok(LoadedPreview {
        content: PreviewContent::Hex {
            data: generate_hex_dump(&bytes, offset),
            total_size,
            offset,
            chunk_size: bytes.len() as u64,
            has_more: offset + (bytes.len() as u64) < total_size,
        },
        asset: None,
    })
}

async fn read_local_range(
    path: &Path,
    offset: u64,
    max_bytes: usize,
    mmap_threshold: u64,
) -> Result<Vec<u8>, PreviewLoadError> {
    if max_bytes == 0 {
        return Ok(Vec::new());
    }
    let path = path.to_path_buf();
    if max_bytes as u64 >= mmap_threshold {
        return Ok(tokio::task::spawn_blocking(move || {
            read_local_range_mmap(&path, offset, max_bytes)
        })
        .await??);
    }

    let mut file = tokio::fs::File::open(path).await?;
    if offset > 0 {
        file.seek(SeekFrom::Start(offset)).await?;
    }
    let mut buffer = vec![0u8; max_bytes];
    let read = file.read(&mut buffer).await?;
    buffer.truncate(read);
    Ok(buffer)
}

fn read_local_range_mmap(
    path: &Path,
    offset: u64,
    max_bytes: usize,
) -> Result<Vec<u8>, std::io::Error> {
    let file = std::fs::File::open(path)?;
    // SAFETY: this is a read-only mapping, the bytes are copied into an owned
    // Vec before returning, and no mutable mapping of the same file is created
    // by this preview owner.
    let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
    let start = offset.min(mmap.len() as u64) as usize;
    let end = (start + max_bytes).min(mmap.len());
    Ok(mmap[start..end].to_vec())
}

fn preview_asset_kind(kind: PreviewKind) -> PreviewAssetKind {
    match kind {
        PreviewKind::Image => PreviewAssetKind::Image,
        PreviewKind::Pdf => PreviewAssetKind::Pdf,
        PreviewKind::Audio => PreviewAssetKind::Audio,
        PreviewKind::Video => PreviewAssetKind::Video,
        PreviewKind::Office => PreviewAssetKind::Office,
        PreviewKind::Font => PreviewAssetKind::Font,
        _ => PreviewAssetKind::Office,
    }
}

#[cfg(test)]
fn block_on_ready<F: std::future::Future>(future: F) -> F::Output {
    use std::{
        pin::pin,
        task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
    };

    fn noop_raw_waker() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        fn wake(_: *const ()) {}
        fn wake_by_ref(_: *const ()) {}
        fn drop(_: *const ()) {}
        RawWaker::new(
            std::ptr::null(),
            &RawWakerVTable::new(clone, wake, wake_by_ref, drop),
        )
    }

    // SAFETY: the vtable above never touches the data pointer and only drives
    // tests for futures that complete without requiring an external reactor.
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("preview future unexpectedly yielded in sync test helper"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_and_rotation_actions_are_clamped() {
        let mut session = PreviewSession::ready(
            PreviewContent::Image {
                data: String::new(),
                mime_type: "image/png".to_string(),
            },
            None,
        );
        session.apply(PreviewAction::ZoomOut);
        session.apply(PreviewAction::ZoomOut);
        session.apply(PreviewAction::ZoomOut);
        session.apply(PreviewAction::ZoomOut);
        assert_eq!(session.zoom(), 0.25);

        session.apply(PreviewAction::RotateClockwise);
        assert_eq!(session.rotation_degrees(), 90);
        session.apply(PreviewAction::ResetZoom);
        assert_eq!(session.zoom(), 1.0);
        assert_eq!(session.rotation_degrees(), 0);
    }

    #[test]
    fn preview_session_types_are_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PreviewSource>();
        assert_send_sync::<PreviewSessionState>();
        assert_send_sync::<PreviewSession>();
        assert_send_sync::<PreviewContent>();
    }

    #[test]
    fn load_inline_uses_async_entrypoint() {
        let session = block_on_ready(PreviewSession::load(PreviewSource::Inline(
            PreviewContent::Unsupported {
                mime_type: "application/x-test".to_string(),
                reason: "test".to_string(),
            },
        )));
        assert!(matches!(session.state(), PreviewSessionState::Ready { .. }));
    }

    #[test]
    fn load_local_text_honors_encoding_hint() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.txt");
        let (encoded, _, _) = encoding_rs::GBK.encode("中文");
        std::fs::write(&path, encoded.as_ref()).unwrap();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let session = runtime.block_on(PreviewSession::load(PreviewSource::LocalPath {
            path,
            mime_type: Some("text/plain".to_string()),
            encoding_hint: Some("gbk".to_string()),
        }));

        match session.state() {
            PreviewSessionState::Ready {
                content: PreviewContent::Text { data, encoding, .. },
                ..
            } => {
                assert_eq!(data, "中文");
                assert_eq!(encoding, "GBK");
            }
            other => panic!("expected text preview, got {other:?}"),
        }
    }

    #[test]
    fn load_local_hex_uses_chunked_mmap_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("payload.bin");
        let bytes: Vec<u8> = (0..=255).cycle().take(4096).collect();
        std::fs::write(&path, bytes).unwrap();

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let session = runtime.block_on(PreviewSession::load_with_options(
            PreviewSource::LocalPath {
                path,
                mime_type: Some("application/octet-stream".to_string()),
                encoding_hint: None,
            },
            PreviewLoadOptions {
                hex_chunk_size: 32,
                mmap_threshold: 1,
                ..PreviewLoadOptions::default()
            },
        ));

        match session.state() {
            PreviewSessionState::Ready {
                content:
                    PreviewContent::Hex {
                        chunk_size,
                        has_more,
                        ..
                    },
                ..
            } => {
                assert_eq!(*chunk_size, 32);
                assert!(*has_more);
            }
            other => panic!("expected hex preview, got {other:?}"),
        }
    }
}
