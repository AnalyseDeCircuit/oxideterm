// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use encoding_rs::{EncoderResult, Encoding, UTF_8, UTF_16BE, UTF_16LE};
use oxideterm_ide_core::{
    IdeFileData, IdeFileError, IdeFileErrorKind, LineEnding, MAX_EDITABLE_FILE_SIZE,
    SavedFileVersion, TextFileFormat,
};
use oxideterm_preview::{
    TextLineEnding, detect_and_decode_with_hint, is_likely_text_content,
    normalize_text_line_endings,
};
use zeroize::{Zeroize, Zeroizing};

pub(crate) const FILE_PREFIX_BYTES: usize = 8192;

pub(crate) fn check_text_prefix(bytes: &[u8], hint: Option<&str>) -> Result<(), IdeFileError> {
    let explicit_unicode = hint
        .and_then(|h| Encoding::for_label(h.as_bytes()))
        .is_some_and(|e| e == UTF_16LE || e == UTF_16BE);
    if explicit_unicode
        || bytes.starts_with(&[0xff, 0xfe])
        || bytes.starts_with(&[0xfe, 0xff])
        || is_likely_text_content(bytes)
    {
        Ok(())
    } else {
        Err(IdeFileError::new(
            IdeFileErrorKind::Binary,
            "File is not text",
        ))
    }
}

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
    check_text_prefix(bytes, hint)?;
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

#[cfg(test)]
fn encode_file(text: &str, format: &TextFileFormat) -> Result<Zeroizing<Vec<u8>>, IdeFileError> {
    encode_chunks(std::iter::once(text), format)
}

pub(crate) fn encode_chunks<'a>(
    chunks: impl Iterator<Item = &'a str> + Clone,
    format: &TextFileFormat,
) -> Result<Zeroizing<Vec<u8>>, IdeFileError> {
    let encoding = Encoding::for_label(format.encoding.as_bytes()).ok_or_else(invalid_encoding)?;
    let utf16 = encoding == UTF_16LE || encoding == UTF_16BE;
    let estimated = chunks
        .clone()
        .map(|text| {
            let base = if utf16 {
                text.encode_utf16().count() * 2
            } else {
                text.len()
            };
            let extra = if format.line_ending == LineEnding::CrLf {
                text.bytes().filter(|&byte| byte == b'\n').count() * if utf16 { 2 } else { 1 }
            } else {
                0
            };
            base + extra
        })
        .sum::<usize>();
    let bom_bytes = if format.has_bom {
        if utf16 {
            2
        } else if encoding == UTF_8 {
            3
        } else {
            0
        }
    } else {
        0
    };
    if utf16 || encoding == UTF_8 {
        check_size((estimated + bom_bytes) as u64)?;
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(
        (estimated + bom_bytes).min(MAX_EDITABLE_FILE_SIZE as usize),
    ));
    if format.has_bom {
        if encoding == UTF_16LE {
            bytes.extend_from_slice(&[0xff, 0xfe]);
        } else if encoding == UTF_16BE {
            bytes.extend_from_slice(&[0xfe, 0xff]);
        } else if encoding == UTF_8 {
            bytes.extend_from_slice(&[0xef, 0xbb, 0xbf]);
        }
    }
    // Keep one encoder across pieces: stateful encodings must not reset at edit boundaries.
    let mut encoder = encoding.new_encoder();
    let mut encode = |mut text: &str, last: bool| -> Result<(), IdeFileError> {
        if utf16 {
            for unit in text.encode_utf16() {
                bytes.extend_from_slice(&if encoding == UTF_16LE {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                });
            }
        } else if encoding == UTF_8 {
            bytes.extend_from_slice(text.as_bytes());
        } else {
            loop {
                let (result, read) =
                    encoder.encode_from_utf8_to_vec_without_replacement(text, &mut bytes, last);
                check_size(bytes.len() as u64)?;
                text = &text[read..];
                match result {
                    EncoderResult::InputEmpty => break,
                    EncoderResult::OutputFull => {
                        let growth = (MAX_EDITABLE_FILE_SIZE as usize)
                            .saturating_sub(bytes.len())
                            .min(8192);
                        if growth == 0 {
                            check_size(MAX_EDITABLE_FILE_SIZE + 1)?;
                        }
                        let spare = bytes.capacity() - bytes.len();
                        bytes.reserve_exact(spare + growth);
                    }
                    EncoderResult::Unmappable(_) => {
                        return Err(IdeFileError::new(
                            IdeFileErrorKind::UnrepresentableText,
                            "The selected encoding cannot represent this text",
                        ));
                    }
                }
            }
        }
        check_size(bytes.len() as u64)
    };
    if format.line_ending == LineEnding::Lf {
        for chunk in chunks {
            encode(chunk, false)?;
        }
    } else {
        let ending = if format.line_ending == LineEnding::CrLf {
            "\r\n"
        } else {
            "\r"
        };
        let mut normalized = Zeroizing::new(String::with_capacity(8192));
        for chunk in chunks {
            for part in chunk.split_inclusive('\n') {
                let (part, suffix) = part
                    .strip_suffix('\n')
                    .map_or((part, ""), |part| (part, ending));
                for mut text in [part, suffix] {
                    while !text.is_empty() {
                        let mut count = text.len().min(8192 - normalized.len());
                        while !text.is_char_boundary(count) {
                            count -= 1;
                        }
                        if count > 0 {
                            normalized.push_str(&text[..count]);
                            text = &text[count..];
                        }
                        if normalized.len() >= 8192 - 3 {
                            encode(&normalized, false)?;
                            normalized.zeroize();
                        }
                    }
                }
            }
        }
        encode(&normalized, false)?;
    }
    encode("", true)?;
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
    fn chunked_encoding_preserves_boms_endings_and_encoder_state() {
        for (encoding, ending, chunks, expected) in [
            (
                "UTF-8",
                LineEnding::CrLf,
                vec!["中", "\n", "🙂"],
                "\u{feff}中\r\n🙂".as_bytes().to_vec(),
            ),
            (
                "UTF-16LE",
                LineEnding::Cr,
                vec!["中", "\n", "🙂"],
                vec![0xff, 0xfe, 0x2d, 0x4e, 0x0d, 0, 0x3d, 0xd8, 0x42, 0xde],
            ),
            (
                "UTF-16BE",
                LineEnding::CrLf,
                vec!["中", "\n", "🙂"],
                vec![
                    0xfe, 0xff, 0x4e, 0x2d, 0, 0x0d, 0, 0x0a, 0xd8, 0x3d, 0xde, 0x42,
                ],
            ),
            (
                "GBK",
                LineEnding::CrLf,
                vec!["中", "文\n"],
                b"\xd6\xd0\xce\xc4\r\n".to_vec(),
            ),
            (
                "ISO-2022-JP",
                LineEnding::Lf,
                vec!["あ", "あ"],
                b"\x1b$B$\x22$\x22\x1b(B".to_vec(),
            ),
        ] {
            let format = TextFileFormat {
                encoding: encoding.into(),
                line_ending: ending,
                has_bom: true,
            };
            assert_eq!(
                *encode_chunks(chunks.into_iter(), &format).unwrap(),
                expected,
                "{encoding}"
            );
        }
        let format = TextFileFormat {
            encoding: "GBK".into(),
            ..Default::default()
        };
        assert_eq!(
            encode_chunks(["valid中", "🙂"].into_iter(), &format)
                .unwrap_err()
                .kind,
            IdeFileErrorKind::UnrepresentableText
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "manual file encoding peak allocation benchmark"]
    fn file_encoding_memory() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        #[repr(C)]
        #[derive(Default)]
        struct Statistics {
            blocks: u32,
            live: usize,
            peak: usize,
            allocated: usize,
        }
        unsafe extern "C" {
            fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut Statistics);
        }
        let allocated = || {
            let mut stats = Statistics::default();
            unsafe { malloc_zone_statistics(std::ptr::null_mut(), &mut stats) };
            stats.live
        };
        let source = "中文 line\n".repeat(1_400_000);
        for (encoding, line_ending) in [
            ("UTF-8", LineEnding::Lf),
            ("UTF-8", LineEnding::CrLf),
            ("GBK", LineEnding::CrLf),
        ] {
            let format = TextFileFormat {
                encoding: encoding.into(),
                line_ending,
                has_bom: false,
            };
            let baseline = allocated();
            let peak = AtomicUsize::new(baseline);
            let stop = AtomicBool::new(false);
            let started = std::time::Instant::now();
            let bytes = std::thread::scope(|scope| {
                scope.spawn(|| {
                    while !stop.load(Ordering::Acquire) {
                        peak.fetch_max(allocated(), Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    }
                });
                let bytes = encode_file(&source, &format).unwrap();
                peak.fetch_max(allocated(), Ordering::Relaxed);
                stop.store(true, Ordering::Release);
                bytes
            });
            let elapsed = started.elapsed().as_secs_f64() * 1000.0;
            let expected_prefix: &[u8] = if encoding == "GBK" {
                b"\xd6\xd0\xce\xc4 line\r\n"
            } else if line_ending == LineEnding::CrLf {
                "中文 line\r\n".as_bytes()
            } else {
                "中文 line\n".as_bytes()
            };
            assert_eq!(bytes.len(), expected_prefix.len() * 1_400_000);
            assert!(
                bytes
                    .chunks_exact(expected_prefix.len())
                    .all(|chunk| chunk == expected_prefix)
            );
            eprintln!(
                "FILE_ENCODING encoding={encoding} ending={line_ending:?} peak_growth={} retained={} elapsed_ms={elapsed:.3}",
                peak.load(Ordering::Relaxed) - baseline,
                allocated() as isize - baseline as isize
            );
        }
    }
    #[test]
    fn binary_prefix_rejection_preserves_unicode_encoding_choices() {
        assert_eq!(
            check_text_prefix(b"\x7fELF\0\0", None).unwrap_err().kind,
            IdeFileErrorKind::Binary
        );
        for (bytes, hint, expected) in [
            (b"\xff\xfeA\0B\0".as_slice(), None, "AB"),
            (b"A\0B\0".as_slice(), Some("UTF-16LE"), "AB"),
            (b"\0A\0B".as_slice(), Some("UTF-16BE"), "AB"),
        ] {
            check_text_prefix(bytes, hint).unwrap();
            assert_eq!(
                decode_file(bytes, hint, SavedFileVersion::unknown())
                    .unwrap()
                    .text
                    .as_str(),
                expected
            );
        }
    }

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
        assert_eq!(
            decode_file(b"\x7fELF\0\0\0\0", None, SavedFileVersion::unknown())
                .unwrap_err()
                .kind,
            IdeFileErrorKind::Binary
        );
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
        assert!(check_size(100 * 1024 * 1024).is_ok());
        assert!(check_size(100 * 1024 * 1024 + 1).is_err());
    }
}
