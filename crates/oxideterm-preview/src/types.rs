// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewAssetKind {
    Image,
    Video,
    Audio,
    Pdf,
    Office,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Text,
    Image,
    Hex,
    Pdf,
    Audio,
    Video,
    Office,
    TooLarge,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreviewContent {
    Text {
        data: String,
        mime_type: Option<String>,
        language: Option<String>,
        encoding: String,
        #[serde(default)]
        confidence: f32,
        #[serde(default)]
        has_bom: bool,
    },
    Image {
        data: String,
        mime_type: String,
    },
    AssetFile {
        path: String,
        mime_type: String,
        kind: PreviewAssetKind,
    },
    Hex {
        data: String,
        total_size: u64,
        offset: u64,
        chunk_size: u64,
        has_more: bool,
    },
    TooLarge {
        size: u64,
        max_size: u64,
        recommend_download: bool,
    },
    Unsupported {
        mime_type: String,
        reason: String,
    },
}

impl PreviewContent {
    pub fn kind(&self) -> PreviewKind {
        match self {
            Self::Text { .. } => PreviewKind::Text,
            Self::Image { .. } => PreviewKind::Image,
            Self::AssetFile { kind, .. } => match kind {
                PreviewAssetKind::Image => PreviewKind::Image,
                PreviewAssetKind::Video => PreviewKind::Video,
                PreviewAssetKind::Audio => PreviewKind::Audio,
                PreviewAssetKind::Pdf => PreviewKind::Pdf,
                PreviewAssetKind::Office => PreviewKind::Office,
            },
            Self::Hex { .. } => PreviewKind::Hex,
            Self::TooLarge { .. } => PreviewKind::TooLarge,
            Self::Unsupported { .. } => PreviewKind::Unsupported,
        }
    }

    pub fn asset_path(&self) -> Option<&str> {
        match self {
            Self::AssetFile { path, .. } => Some(path),
            _ => None,
        }
    }
}

pub fn classify_preview_path(path: impl AsRef<Path>) -> PreviewKind {
    let path = path.as_ref();
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mime = mime.essence_str();
    let ext = path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    classify_preview_type(&ext, mime)
}

pub fn classify_preview_type(extension: &str, mime_type: &str) -> PreviewKind {
    if extension == "pdf" || mime_type == "application/pdf" {
        return PreviewKind::Pdf;
    }
    if is_office_extension(extension) {
        return PreviewKind::Office;
    }
    if mime_type.starts_with("image/") {
        return PreviewKind::Image;
    }
    if mime_type.starts_with("audio/") {
        return PreviewKind::Audio;
    }
    if mime_type.starts_with("video/") {
        return PreviewKind::Video;
    }
    if mime_type.starts_with("text/")
        || matches!(
            mime_type,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/toml"
                | "application/yaml"
        )
    {
        return PreviewKind::Text;
    }
    PreviewKind::Hex
}

fn is_office_extension(extension: &str) -> bool {
    matches!(
        extension,
        "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" | "rtf"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_common_native_preview_types() {
        assert_eq!(
            classify_preview_type("png", "image/png"),
            PreviewKind::Image
        );
        assert_eq!(
            classify_preview_type("pdf", "application/pdf"),
            PreviewKind::Pdf
        );
        assert_eq!(
            classify_preview_type("mp3", "audio/mpeg"),
            PreviewKind::Audio
        );
        assert_eq!(
            classify_preview_type("mp4", "video/mp4"),
            PreviewKind::Video
        );
        assert_eq!(
            classify_preview_type("docx", "application/octet-stream"),
            PreviewKind::Office
        );
        assert_eq!(
            classify_preview_type("txt", "text/plain"),
            PreviewKind::Text
        );
        assert_eq!(
            classify_preview_type("bin", "application/octet-stream"),
            PreviewKind::Hex
        );
    }

    #[test]
    fn preview_content_asset_shape_matches_sftp_wire_shape() {
        let content = PreviewContent::AssetFile {
            path: "/tmp/a.pdf".to_string(),
            mime_type: "application/pdf".to_string(),
            kind: PreviewAssetKind::Pdf,
        };
        let json = serde_json::to_value(content).unwrap();
        assert_eq!(json["AssetFile"]["path"], "/tmp/a.pdf");
        assert_eq!(json["AssetFile"]["mime_type"], "application/pdf");
        assert_eq!(json["AssetFile"]["kind"], "pdf");
    }
}
