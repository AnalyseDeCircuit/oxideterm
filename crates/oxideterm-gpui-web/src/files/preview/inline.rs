use base64::{Engine, engine::general_purpose::STANDARD};
use oxideterm_local_files::{local_file_extension, mime_type_for_extension};
use oxideterm_preview::{PreviewContent, PreviewKind, PreviewLoadOptions};
use std::path::Path;

fn suffix(path: &str) -> String {
    local_file_extension(path.rsplit('/').next().unwrap_or(path))
}

pub(crate) fn format(path: &str, bytes: &[u8]) -> (PreviewKind, String) {
    let ext = suffix(path);
    let sample = &bytes[..bytes.len().min(8192)];
    let image = if sample.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if sample.starts_with(b"\xff\xd8\xff") {
        Some("image/jpeg")
    } else if sample.starts_with(b"GIF87a") || sample.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if sample.starts_with(b"RIFF") && sample.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else {
        None
    };
    if let Some(mime) = image {
        return (PreviewKind::Image, mime.into());
    }
    if let Some(mime) = oxideterm_preview::inspection_mime_type(&ext, sample) {
        return (PreviewKind::Document, mime.into());
    }
    let mut mime = mime_type_for_extension(&ext);
    if mime == "application/zip" {
        return (PreviewKind::Document, mime);
    }
    let mut kind = oxideterm_preview::classify_preview_type(&ext, &mime);
    if matches!(kind, PreviewKind::Hex | PreviewKind::Text) {
        if sample.starts_with(&[0xff, 0xfe])
            || sample.starts_with(&[0xfe, 0xff])
            || oxideterm_preview::is_likely_text_content(sample)
        {
            kind = PreviewKind::Text;
            if mime == "application/octet-stream" {
                mime = "text/plain".into();
            }
        } else {
            kind = PreviewKind::Hex;
            mime = "application/octet-stream".into();
        }
    }
    (kind, mime)
}

pub(crate) fn language(path: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let ext = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| name.trim_start_matches('.'));
    oxideterm_preview::extension_to_language(ext)
}

pub(crate) fn sftp_content(
    path: &str,
    bytes: &[u8],
    revision: u64,
    offset: u64,
) -> Result<PreviewContent, String> {
    let (kind, mime) = format(path, bytes);
    let size = bytes.len() as u64;
    let limits = PreviewLoadOptions::default();
    let limit = match kind {
        PreviewKind::Text => limits.max_text_size,
        PreviewKind::Image => limits.max_preview_size,
        _ => u64::MAX,
    };
    let content = if size > limit {
        PreviewContent::TooLarge {
            size,
            max_size: limit,
            recommend_download: true,
        }
    } else if kind == PreviewKind::Image {
        PreviewContent::Image {
            data: STANDARD.encode(bytes),
            mime_type: mime,
        }
    } else if kind == PreviewKind::Text {
        let (data, encoding, confidence, has_bom, lossy) =
            oxideterm_preview::detect_and_decode_with_hint(bytes, None);
        if lossy {
            return Err("ide.invalid_encoding".into());
        }
        PreviewContent::Text {
            data,
            encoding,
            confidence,
            has_bom,
            version: Some(revision.to_string()),
            mime_type: Some(mime),
            language: language(path),
        }
    } else if mime == "application/pkix-cert" {
        return Err("fileManager.unsupportedFormat".into());
    } else {
        let start = offset.min(size);
        let end = start.saturating_add(limits.hex_chunk_size).min(size);
        PreviewContent::Hex {
            data: oxideterm_preview::generate_hex_dump(&bytes[start as usize..end as usize], start),
            total_size: size,
            offset: start,
            chunk_size: end - start,
            has_more: end < size,
        }
    };
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_preview_keeps_the_encoding_and_version_and_binary_pages_respect_the_offset() {
        let bytes = [0xff, 0xfe, 0x7b, 0, 0x7d, 0, 0x0d, 0, 0x0a, 0];
        let PreviewContent::Text {
            data,
            version,
            language,
            encoding,
            has_bom,
            mime_type,
            ..
        } = sftp_content("a.json", &bytes, 17, 0).unwrap()
        else {
            panic!("UTF-16 JSON must be editable");
        };
        assert_eq!(
            (data.as_str(), encoding.as_str(), has_bom),
            ("{}\r\n", "UTF-16LE", true)
        );
        assert_eq!(
            (
                version.as_deref(),
                language.as_deref(),
                mime_type.as_deref()
            ),
            (Some("17"), Some("json"), Some("application/json"))
        );
        let mut binary = vec![0; 16 * 1024];
        binary.extend_from_slice(b"tail");
        let PreviewContent::Hex {
            has_more: true,
            chunk_size: 16384,
            ..
        } = sftp_content("output.log", &binary, 18, 0).unwrap()
        else {
            panic!("binary file labelled .log must be paged");
        };
        let PreviewContent::Hex {
            data,
            total_size,
            offset,
            has_more,
            ..
        } = sftp_content("output.log", &binary, 18, 16384).unwrap()
        else {
            panic!("missing next page");
        };
        assert_eq!((total_size, offset, has_more), (16388, 16384, false));
        assert!(data.starts_with("00004000  74 61 69 6C"));
    }
}
