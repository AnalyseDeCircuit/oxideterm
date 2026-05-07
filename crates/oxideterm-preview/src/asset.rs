// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{PreviewAssetKind, PreviewContent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewAssetOwnership {
    Local,
    OwnedTemp,
}

#[derive(Clone, Debug)]
pub struct PreviewAssetOwner {
    inner: Arc<PreviewAssetInner>,
}

#[derive(Debug)]
struct PreviewAssetInner {
    path: PathBuf,
    mime_type: String,
    kind: PreviewAssetKind,
    ownership: PreviewAssetOwnership,
}

impl PreviewAssetOwner {
    pub fn local(
        path: impl Into<PathBuf>,
        mime_type: impl Into<String>,
        kind: PreviewAssetKind,
    ) -> Self {
        Self::new(path, mime_type, kind, PreviewAssetOwnership::Local)
    }

    pub fn owned_temp(
        path: impl Into<PathBuf>,
        mime_type: impl Into<String>,
        kind: PreviewAssetKind,
    ) -> Self {
        Self::new(path, mime_type, kind, PreviewAssetOwnership::OwnedTemp)
    }

    pub fn from_asset_content_owned_temp(content: &PreviewContent) -> Option<Self> {
        match content {
            PreviewContent::AssetFile {
                path,
                mime_type,
                kind,
            } => Some(Self::owned_temp(path, mime_type, *kind)),
            _ => None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    pub fn mime_type(&self) -> &str {
        &self.inner.mime_type
    }

    pub fn kind(&self) -> PreviewAssetKind {
        self.inner.kind
    }

    pub fn ownership(&self) -> PreviewAssetOwnership {
        self.inner.ownership
    }

    fn new(
        path: impl Into<PathBuf>,
        mime_type: impl Into<String>,
        kind: PreviewAssetKind,
        ownership: PreviewAssetOwnership,
    ) -> Self {
        Self {
            inner: Arc::new(PreviewAssetInner {
                path: path.into(),
                mime_type: mime_type.into(),
                kind,
                ownership,
            }),
        }
    }
}

impl Drop for PreviewAssetInner {
    fn drop(&mut self) {
        if self.ownership == PreviewAssetOwnership::OwnedTemp {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn owned_temp_asset_is_removed_when_owner_drops() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("preview.pdf");
        fs::write(&path, b"pdf").unwrap();
        {
            let _owner =
                PreviewAssetOwner::owned_temp(&path, "application/pdf", PreviewAssetKind::Pdf);
            assert!(path.exists());
        }
        assert!(!path.exists());
    }

    #[test]
    fn local_asset_is_not_removed_when_owner_drops() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("image.png");
        fs::write(&path, b"png").unwrap();
        {
            let _owner = PreviewAssetOwner::local(&path, "image/png", PreviewAssetKind::Image);
            assert!(path.exists());
        }
        assert!(path.exists());
    }

    #[test]
    fn cloned_owned_temp_asset_is_removed_after_last_owner_drops() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("audio.mp3");
        fs::write(&path, b"audio").unwrap();
        let owner = PreviewAssetOwner::owned_temp(&path, "audio/mpeg", PreviewAssetKind::Audio);
        let clone = owner.clone();
        drop(owner);
        assert!(path.exists());
        drop(clone);
        assert!(!path.exists());
    }

    #[test]
    fn cloned_owned_temp_asset_survives_concurrent_clone_drops_until_last_owner() {
        let temp = tempdir().unwrap();
        let path = temp.path().join("video.mp4");
        fs::write(&path, b"video").unwrap();

        let owner = PreviewAssetOwner::owned_temp(&path, "video/mp4", PreviewAssetKind::Video);
        let owner = Arc::new(owner);
        let mut handles = Vec::new();
        for _ in 0..16 {
            let clone = owner.clone();
            handles.push(std::thread::spawn(move || {
                let local_owner = (*clone).clone();
                drop(local_owner);
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert!(path.exists());
        drop(owner);
        assert!(!path.exists());
    }

    #[test]
    fn asset_owner_is_thread_safe() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<PreviewAssetOwner>();
    }
}
