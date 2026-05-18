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
    Office,
    Font,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Text,
    Image,
    Hex,
    Audio,
    Video,
    Office,
    Font,
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
                PreviewAssetKind::Office => PreviewKind::Office,
                PreviewAssetKind::Font => PreviewKind::Font,
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
        return PreviewKind::Unsupported;
    }
    if is_office_extension(extension) {
        return PreviewKind::Office;
    }
    if is_font_extension(extension) || mime_type.starts_with("font/") {
        return PreviewKind::Font;
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

pub fn is_font_extension(extension: &str) -> bool {
    matches!(extension, "ttf" | "otf" | "woff" | "woff2" | "eot")
}

pub fn font_mime_type(extension: &str, fallback: &str) -> String {
    match extension {
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "eot" => "application/vnd.ms-fontobject",
        _ => fallback,
    }
    .to_string()
}

pub fn font_family_name_from_bytes(bytes: &[u8]) -> Option<String> {
    use ttf_parser::name_id;

    let face = ttf_parser::Face::parse(bytes, 0).ok()?;
    let names = face.names();

    [
        name_id::TYPOGRAPHIC_FAMILY,
        name_id::FAMILY,
        name_id::FULL_NAME,
    ]
    .into_iter()
    .find_map(|name_id| {
        names
            .into_iter()
            .find(|name| name.name_id == name_id)
            .and_then(|name| name.to_string())
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
    })
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
            PreviewKind::Unsupported
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
            classify_preview_type("ttf", "application/octet-stream"),
            PreviewKind::Font
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
            path: "/tmp/a.png".to_string(),
            mime_type: "image/png".to_string(),
            kind: PreviewAssetKind::Image,
        };
        let json = serde_json::to_value(content).unwrap();
        assert_eq!(json["AssetFile"]["path"], "/tmp/a.png");
        assert_eq!(json["AssetFile"]["mime_type"], "image/png");
        assert_eq!(json["AssetFile"]["kind"], "image");
    }
}
