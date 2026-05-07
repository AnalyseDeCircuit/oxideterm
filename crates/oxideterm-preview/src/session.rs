// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;

use crate::{
    PreviewAssetKind, PreviewAssetOwner, PreviewContent, PreviewKind, classify_preview_path,
};

#[derive(Clone, Debug)]
pub enum PreviewSource {
    LocalPath {
        path: PathBuf,
        mime_type: Option<String>,
    },
    OwnedTempAsset(PreviewAssetOwner),
    Inline(PreviewContent),
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
    pub fn load(source: PreviewSource) -> Self {
        match source {
            PreviewSource::Inline(content) => Self::ready(content, None),
            PreviewSource::OwnedTempAsset(asset) => {
                let content = PreviewContent::AssetFile {
                    path: asset.path().to_string_lossy().to_string(),
                    mime_type: asset.mime_type().to_string(),
                    kind: asset.kind(),
                };
                Self::ready(content, Some(asset))
            }
            PreviewSource::LocalPath { path, mime_type } => {
                let kind = preview_asset_kind_from_path(&path);
                let mime_type = mime_type.unwrap_or_else(|| {
                    mime_guess::from_path(&path)
                        .first_or_octet_stream()
                        .essence_str()
                        .to_string()
                });
                let content = PreviewContent::AssetFile {
                    path: path.to_string_lossy().to_string(),
                    mime_type: mime_type.clone(),
                    kind,
                };
                Self::ready(
                    content,
                    Some(PreviewAssetOwner::local(path, mime_type, kind)),
                )
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

fn preview_asset_kind_from_path(path: &PathBuf) -> PreviewAssetKind {
    match classify_preview_path(path) {
        PreviewKind::Image => PreviewAssetKind::Image,
        PreviewKind::Pdf => PreviewAssetKind::Pdf,
        PreviewKind::Audio => PreviewAssetKind::Audio,
        PreviewKind::Video => PreviewAssetKind::Video,
        PreviewKind::Office => PreviewAssetKind::Office,
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
    fn preview_session_types_are_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PreviewSource>();
        assert_send_sync::<PreviewSessionState>();
        assert_send_sync::<PreviewSession>();
        assert_send_sync::<PreviewContent>();
    }
}
