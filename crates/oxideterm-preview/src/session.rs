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
    pub max_pdf_preview_size: u64,
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
            max_pdf_preview_size: 100 * 1024 * 1024,
            // Local players read from the file; remote downloads enforce their own limits.
            max_media_preview_size: u64::MAX,
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

#[derive(Clone, Debug, Default)]
pub enum PreviewSessionState {
    #[default]
    Empty,
    Loading,
    Ready {
        content: PreviewContent,
        asset: Option<PreviewAssetOwner>,
    },
    Error(String),
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
    let mut mime_type = mime_type.unwrap_or_else(|| {
        mime_guess::from_path(&path)
            .first_or_octet_stream()
            .essence_str()
            .to_string()
    });
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let mut header = zeroize::Zeroizing::new([0; 64]);
    let mut source = tokio::fs::File::open(&path).await?;
    let header_len = tokio::io::AsyncReadExt::read(&mut source, &mut header[..]).await?;
    if let Some(mime) = crate::inspection_mime_type(&extension, &header[..header_len]) {
        return load_local_asset(
            path,
            mime.into(),
            PreviewAssetKind::Document,
            size,
            options.max_preview_size,
        );
    }
    let sqlite_candidate = matches!(extension.as_str(), "sqlite" | "sqlite3" | "db" | "db3");
    let sqlite_header = if matches!(extension.as_str(), "db" | "db3") {
        use tokio::io::AsyncReadExt;
        let mut header = [0; 16];
        let mut file = tokio::fs::File::open(&path).await?;
        file.read_exact(&mut header).await.is_ok() && &header == b"SQLite format 3\0"
    } else {
        sqlite_candidate
    };
    if sqlite_header {
        mime_type = "application/vnd.sqlite3".into();
    }
    let kind = if sqlite_candidate && !sqlite_header {
        PreviewKind::Hex
    } else if matches!(
        mime_type.as_str(),
        "application/pdf" | "application/vnd.sqlite3" | "application/x-sqlite3"
    ) {
        PreviewKind::Document
    } else {
        classify_preview_path(&path)
    };
    match kind {
        PreviewKind::Image | PreviewKind::Office | PreviewKind::Font | PreviewKind::Document => {
            let asset_kind = preview_asset_kind(kind);
            let max_size = if kind == PreviewKind::Document && mime_type == "application/pdf" {
                options.max_pdf_preview_size
            } else {
                options.max_preview_size
            };
            load_local_asset(path, mime_type, asset_kind, size, max_size)
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
    let (data, encoding, confidence, has_bom, _) =
        detect_and_decode_with_hint(&bytes, encoding_hint);
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
        PreviewKind::Audio => PreviewAssetKind::Audio,
        PreviewKind::Video => PreviewAssetKind::Video,
        PreviewKind::Office => PreviewAssetKind::Office,
        PreviewKind::Font => PreviewAssetKind::Font,
        PreviewKind::Document => PreviewAssetKind::Document,
        _ => PreviewAssetKind::Office,
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
    fn local_asset_limits_distinguish_pdf_media_and_other_documents() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        for (name, size, mime, kind, rejected) in [
            (
                "manual.pdf",
                100 * 1024 * 1024,
                "application/pdf",
                PreviewAssetKind::Document,
                false,
            ),
            (
                "manual.pdf",
                100 * 1024 * 1024 + 1,
                "application/pdf",
                PreviewAssetKind::Document,
                true,
            ),
            (
                "recording.mp4",
                1024 * 1024 * 1024,
                "video/mp4",
                PreviewAssetKind::Video,
                false,
            ),
            (
                "recording.mp3",
                1024 * 1024 * 1024,
                "audio/mpeg",
                PreviewAssetKind::Audio,
                false,
            ),
            (
                "database.sqlite",
                10 * 1024 * 1024 + 1,
                "application/vnd.sqlite3",
                PreviewAssetKind::Document,
                true,
            ),
        ] {
            let path = directory.path().join(name);
            std::fs::File::create(&path).unwrap().set_len(size).unwrap();
            let session = runtime.block_on(PreviewSession::load(PreviewSource::LocalPath {
                path: path.clone(),
                mime_type: None,
                encoding_hint: None,
            }));
            match session.state() {
                PreviewSessionState::Ready {
                    content:
                        PreviewContent::AssetFile {
                            path: source,
                            mime_type,
                            kind: asset_kind,
                        },
                    asset: Some(owner),
                } if !rejected => {
                    assert_eq!(Path::new(source), path);
                    assert_eq!(mime_type, mime);
                    assert_eq!(*asset_kind, kind);
                    assert_eq!(owner.path(), path);
                }
                PreviewSessionState::Ready {
                    content:
                        PreviewContent::TooLarge {
                            size: actual,
                            max_size,
                            ..
                        },
                    asset: None,
                } if rejected => {
                    assert_eq!(*actual, size);
                    assert_eq!(
                        *max_size,
                        if name == "manual.pdf" {
                            100 * 1024 * 1024
                        } else {
                            10 * 1024 * 1024
                        }
                    );
                }
                other => panic!("Unexpected local preview for {name} ({size} bytes): {other:?}"),
            }
            drop(session);
            assert_eq!(std::fs::metadata(path).unwrap().len(), size);
        }
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
        let path = dir.path().join("payload.dat");
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

    #[test]
    fn inspection_sources_route_before_text_and_keep_local_file_ownership() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        for (name, bytes, mime) in [
            (
                "bundle.pem",
                b"-----BEGIN PRIVATE KEY-----\nfixture".as_slice(),
                "application/pkix-cert",
            ),
            (
                "certificate.cer",
                b"DER fixture".as_slice(),
                "application/pkix-cert",
            ),
            (
                "executable",
                b"\x7fELF\x02\x01".as_slice(),
                "application/x-oxideterm-binary",
            ),
        ] {
            let path = directory.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            let session = runtime.block_on(PreviewSession::load(PreviewSource::LocalPath {
                path: path.clone(),
                mime_type: Some("text/plain".into()),
                encoding_hint: None,
            }));
            match session.state() {
                PreviewSessionState::Ready {
                    content:
                        PreviewContent::AssetFile {
                            path: source,
                            mime_type,
                            kind,
                        },
                    ..
                } => {
                    assert_eq!(Path::new(source), path);
                    assert_eq!(mime_type, mime);
                    assert_eq!(*kind, PreviewAssetKind::Document);
                }
                _ => panic!("Inspection source must stay out of text preview"),
            }
            drop(session);
            assert_eq!(std::fs::read(path).unwrap(), bytes);
        }
    }
}
