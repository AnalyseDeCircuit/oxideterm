// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use encoding_rs::{Encoding, UTF_8, UTF_16BE, UTF_16LE};
use oxideterm_ide_core::{
    IdeFileData, IdeFileError, IdeFileErrorKind, LineEnding, MAX_EDITABLE_FILE_SIZE,
    SavedFileVersion, TextFileFormat,
};
use oxideterm_preview::{
    TextLineEnding, detect_and_decode_with_hint, is_likely_text_content,
    normalize_text_line_endings, restore_text_line_endings,
};
use zeroize::Zeroizing;

pub(crate) fn check_size(size: u64) -> Result<(), IdeFileError> {
    if size > MAX_EDITABLE_FILE_SIZE {
        return Err(IdeFileError::new(
            IdeFileErrorKind::TooLarge,
            "File exceeds the 100 MiB IDE limit",
        ));
    }
    Ok(())
}

pub(crate) fn decode_file(
    bytes: &[u8],
    hint: Option<&str>,
    version: SavedFileVersion,
) -> Result<IdeFileData, IdeFileError> {
    check_size(bytes.len() as u64)?;
    let unicode_bom = bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]);
    let explicit_unicode = hint
        .and_then(|h| Encoding::for_label(h.as_bytes()))
        .is_some_and(|e| e == UTF_16LE || e == UTF_16BE);
    if !unicode_bom && !explicit_unicode && !is_likely_text_content(bytes) {
        return Err(IdeFileError::new(
            IdeFileErrorKind::Unsupported,
            "File is not text",
        ));
    }
    // A manual reopen must honor the selected encoding, even when a conflicting BOM exists.
    // Automatic detection remains shared with previews; editing rejects lossy decoding.
    let (decoded, name, has_bom, errors) = if let Some(hint) = hint {
        let encoding = Encoding::for_label(hint.as_bytes()).ok_or_else(invalid_encoding)?;
        let bom_len = Encoding::for_bom(bytes)
            .filter(|(bom_encoding, _)| *bom_encoding == encoding)
            .map(|(_, len)| len)
            .unwrap_or(0);
        let (text, errors) = encoding.decode_without_bom_handling(&bytes[bom_len..]);
        (
            text.into_owned(),
            encoding.name().to_string(),
            bom_len > 0,
            errors,
        )
    } else {
        let (text, name, _, bom, errors) = detect_and_decode_with_hint(bytes, None);
        (text, name, bom, errors)
    };
    let decoded = Zeroizing::new(decoded);
    if errors {
        return Err(invalid_encoding());
    }
    let (text, line_ending) = normalize_text_line_endings(&decoded);
    let line_ending = match line_ending {
        TextLineEnding::Lf => LineEnding::Lf,
        TextLineEnding::CrLf => LineEnding::CrLf,
        TextLineEnding::Cr => LineEnding::Cr,
    };
    Ok(IdeFileData {
        text,
        version,
        format: TextFileFormat {
            encoding: name,
            has_bom,
            line_ending,
        },
    })
}

pub(crate) fn encode_file(
    text: &str,
    format: &TextFileFormat,
) -> Result<Zeroizing<Vec<u8>>, IdeFileError> {
    let encoding = Encoding::for_label(format.encoding.as_bytes()).ok_or_else(invalid_encoding)?;
    let ending = match format.line_ending {
        LineEnding::Lf => TextLineEnding::Lf,
        LineEnding::CrLf => TextLineEnding::CrLf,
        LineEnding::Cr => TextLineEnding::Cr,
    };
    let text = Zeroizing::new(restore_text_line_endings(text, ending));
    let mut bytes = Zeroizing::new(Vec::new());
    if encoding == UTF_16LE || encoding == UTF_16BE {
        if format.has_bom {
            bytes.extend_from_slice(if encoding == UTF_16LE {
                &[0xff, 0xfe]
            } else {
                &[0xfe, 0xff]
            });
        }
        for unit in text.encode_utf16() {
            bytes.extend_from_slice(&if encoding == UTF_16LE {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
    } else {
        if format.has_bom && encoding == UTF_8 {
            bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
        let (encoded, _, errors) = encoding.encode(&text);
        let encoded = Zeroizing::new(encoded.into_owned());
        if errors {
            return Err(IdeFileError::new(
                IdeFileErrorKind::UnrepresentableText,
                "The selected encoding cannot represent this text",
            ));
        }
        bytes.extend_from_slice(&encoded);
    }
    check_size(bytes.len() as u64)?;
    Ok(bytes)
}

fn invalid_encoding() -> IdeFileError {
    IdeFileError::new(
        IdeFileErrorKind::InvalidEncoding,
        "Cannot decode using this encoding",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gb2312_crlf_round_trip_above_preview_limit() {
        let source = "中文小说内容\r\n".repeat(250_000);
        let (bytes, _, _) = encoding_rs::GBK.encode(&source);
        let data = decode_file(&bytes, Some("gb2312"), SavedFileVersion::unknown()).unwrap();
        assert_eq!(data.format.encoding, "GBK");
        assert_eq!(data.format.line_ending, LineEnding::CrLf);
        assert!(!data.text.contains('\r'));
        assert_eq!(
            encode_file(&data.text, &data.format).unwrap().as_slice(),
            bytes.as_ref()
        );
    }
    #[test]
    fn unicode_boms_and_endings_round_trip() {
        for encoding in ["UTF-8", "UTF-16LE", "UTF-16BE"] {
            for line_ending in [LineEnding::Lf, LineEnding::CrLf, LineEnding::Cr] {
                let format = TextFileFormat {
                    encoding: encoding.into(),
                    has_bom: true,
                    line_ending,
                };
                let bytes = encode_file("中文\nsecond\n", &format).unwrap();
                let data = decode_file(&bytes, None, SavedFileVersion::unknown()).unwrap();
                assert_eq!(data.text, "中文\nsecond\n");
                assert_eq!(data.format, format);
                assert_eq!(encode_file(&data.text, &data.format).unwrap(), bytes);
            }
        }
    }
    #[test]
    fn explicit_encoding_overrides_detection_and_preserves_matching_bom() {
        let bytes = b"\xef\xbb\xbfa";
        let automatic = decode_file(bytes, None, SavedFileVersion::unknown()).unwrap();
        assert_eq!(automatic.text, "a");
        assert!(automatic.format.has_bom);
        let reopened =
            decode_file(bytes, Some("windows-1252"), SavedFileVersion::unknown()).unwrap();
        assert_eq!(reopened.text, "ï»¿a");
        assert_eq!(reopened.format.encoding, "windows-1252");
        assert!(!reopened.format.has_bom);
        assert_eq!(
            encode_file(&reopened.text, &reopened.format)
                .unwrap()
                .as_slice(),
            bytes
        );
    }

    #[test]
    fn lossy_operations_are_rejected() {
        assert!(decode_file(&[0xff], Some("UTF-8"), SavedFileVersion::unknown()).is_err());
        let format = TextFileFormat {
            encoding: "GBK".into(),
            ..Default::default()
        };
        assert_eq!(
            encode_file("😀", &format).unwrap_err().kind,
            IdeFileErrorKind::UnrepresentableText
        );
    }
    #[test]
    fn ide_limit_accepts_100_mib() {
        assert!(check_size(MAX_EDITABLE_FILE_SIZE).is_ok());
        assert!(check_size(MAX_EDITABLE_FILE_SIZE + 1).is_err());
    }
}
