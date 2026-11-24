// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::PreviewAssetKind;
#[cfg(not(target_family = "wasm"))]
use crate::PreviewContent;

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

    #[cfg(not(target_family = "wasm"))]
    pub fn owned_temp(
        path: impl Into<PathBuf>,
        mime_type: impl Into<String>,
        kind: PreviewAssetKind,
    ) -> Self {
        Self::new(path, mime_type, kind, PreviewAssetOwnership::OwnedTemp)
    }

    #[cfg(not(target_family = "wasm"))]
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

#[cfg(not(target_family = "wasm"))]
impl Drop for PreviewAssetInner {
    fn drop(&mut self) {
        if self.ownership == PreviewAssetOwnership::OwnedTemp {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn only_owned_temp_assets_are_removed_after_the_last_owner_drops() {
        for owned_temp in [false, true] {
            let temp = tempdir().unwrap();
            let path = temp.path().join("image.png");
            fs::write(&path, b"png").unwrap();
            let owner = if owned_temp {
                PreviewAssetOwner::owned_temp(&path, "image/png", PreviewAssetKind::Image)
            } else {
                PreviewAssetOwner::local(&path, "image/png", PreviewAssetKind::Image)
            };
            let clone = owner.clone();
            drop(owner);
            assert!(path.exists());
            drop(clone);
            assert_eq!(path.exists(), !owned_temp, "owned_temp={owned_temp}");
        }
    }
}
