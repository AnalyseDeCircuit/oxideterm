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
    Document,
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
    Document,
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
                PreviewAssetKind::Document => PreviewKind::Document,
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
    if inspection_mime_type(extension, &[]).is_some() {
        return PreviewKind::Document;
    }
    if matches!(extension, "pdf" | "sqlite" | "sqlite3" | "db" | "db3")
        || matches!(
            mime_type,
            "application/pdf" | "application/vnd.sqlite3" | "application/x-sqlite3"
        )
    {
        return PreviewKind::Document;
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

/// Inspection routes precede text preview so mixed certificate/key PEM files stay opaque to the host.
pub fn inspection_mime_type(extension: &str, header: &[u8]) -> Option<&'static str> {
    if matches!(extension, "pem" | "der" | "crt" | "cer") || header.starts_with(b"-----BEGIN ") {
        return Some("application/pkix-cert");
    }
    if matches!(
        extension,
        "exe" | "dll" | "elf" | "so" | "dylib" | "macho" | "bin" | "o"
    ) || header.starts_with(b"\x7fELF")
        || header.starts_with(b"MZ")
        || header.get(..4).is_some_and(|magic| {
            matches!(
                magic,
                [0xfe, 0xed, 0xfa, 0xce]
                    | [0xce, 0xfa, 0xed, 0xfe]
                    | [0xfe, 0xed, 0xfa, 0xcf]
                    | [0xcf, 0xfa, 0xed, 0xfe]
                    | [0xca, 0xfe, 0xba, 0xbe]
                    | [0xca, 0xfe, 0xba, 0xbf]
            )
        })
    {
        return Some("application/x-oxideterm-binary");
    }
    None
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
