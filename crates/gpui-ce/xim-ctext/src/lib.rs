//! Encodes UTF-8 Compound Text and decodes the UTF-8 and common CJK forms used by XIM.

#![no_std]

extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

#[cfg(feature = "std")]
use std::io::{self, Write};

const UTF8_START: &[u8] = &[0x1B, 0x25, 0x47];
const UTF8_END: &[u8] = &[0x1B, 0x25, 0x40];

/// Wrapper for reduce allocation
#[derive(Clone, Copy)]
#[repr(transparent)]
pub struct CText<'s> {
    utf8: &'s str,
}

impl<'s> fmt::Debug for CText<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.utf8)
    }
}

impl<'s> fmt::Display for CText<'s> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.utf8)
    }
}

impl<'s> CText<'s> {
    pub const fn new(utf8: &'s str) -> Self {
        Self { utf8 }
    }

    pub const fn len(self) -> usize {
        self.utf8.len() + UTF8_START.len() + UTF8_END.len()
    }

    #[cfg(feature = "std")]
    pub fn write(self, mut out: impl Write) -> io::Result<usize> {
        let mut writed = 0;
        writed += out.write(UTF8_START)?;
        writed += out.write(self.utf8.as_bytes())?;
        writed += out.write(UTF8_END)?;
        Ok(writed)
    }
}

/// Encoding utf8 to COMPOUND_TEXT with utf8 escape
pub fn utf8_to_compound_text(text: &str) -> Vec<u8> {
    let mut ret = Vec::with_capacity(text.len() + 6);
    ret.extend_from_slice(UTF8_START);
    ret.extend_from_slice(text.as_bytes());
    ret.extend_from_slice(UTF8_END);
    ret
}

#[derive(Debug, Clone)]
pub enum DecodeError {
    InvalidEncoding,
    UnsupportedEncoding,
    Utf8Error(alloc::string::FromUtf8Error),
}

impl From<alloc::string::FromUtf8Error> for DecodeError {
    fn from(err: alloc::string::FromUtf8Error) -> Self {
        DecodeError::Utf8Error(err)
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEncoding => write!(f, "Invalid compound text"),
            Self::UnsupportedEncoding => write!(f, "Unsupported compound text encoding"),
            Self::Utf8Error(e) => write!(f, "Not a valid utf8 {}", e),
        }
    }
}

macro_rules! decode {
    ($decoder:expr, $out:expr, $bytes:expr, $last:expr) => {
        let mut remaining: &[u8] = $bytes;
        loop {
            let (ret, consumed, _) = $decoder.decode_to_string(remaining, $out, $last);
            remaining = &remaining[consumed..];

            match ret {
                encoding_rs::CoderResult::InputEmpty => break,
                encoding_rs::CoderResult::OutputFull => {
                    $out.reserve(
                        $decoder
                            .max_utf8_buffer_length(remaining.len())
                            .unwrap_or_default(),
                    );
                }
            }
        }
    };
}

pub fn compound_text_to_utf8(bytes: &[u8]) -> Result<String, DecodeError> {
    let mut output = String::new();
    let mut offset = 0;

    while offset < bytes.len() {
        if bytes[offset] != 0x1B {
            let segment_end = bytes[offset..]
                .iter()
                .position(|byte| *byte == 0x1B)
                .map_or(bytes.len(), |index| offset + index);
            output.push_str(&String::from_utf8(bytes[offset..segment_end].to_vec())?);
            offset = segment_end;
            continue;
        }

        let escape = bytes
            .get(offset + 1..)
            .ok_or(DecodeError::InvalidEncoding)?;
        match escape {
            [0x25, 0x47, ..] => {
                offset += 3;
                let segment_end = bytes[offset..]
                    .iter()
                    .position(|byte| *byte == 0x1B)
                    .map_or(bytes.len(), |index| offset + index);
                output.push_str(&String::from_utf8(bytes[offset..segment_end].to_vec())?);
                offset = segment_end;
            }
            [0x25, 0x40, ..] => {
                // UTF-8 mode terminators carry no text of their own.
                offset += 3;
            }
            [0x28, 0x42 | 0x4A, ..] => {
                // ASCII and JIS X 0201 designations apply to the following raw segment.
                offset += 3;
            }
            [0x24, 0x28, charset, ..] => {
                offset += 4;
                let segment_end = bytes[offset..]
                    .iter()
                    .position(|byte| *byte == 0x1B)
                    .map_or(bytes.len(), |index| offset + index);
                let segment = &bytes[offset..segment_end];

                match charset {
                    0x42 => {
                        let mut decoder =
                            encoding_rs::ISO_2022_JP.new_decoder_without_bom_handling();
                        decode!(decoder, &mut output, &[0x1B, 0x24, 0x42], false);
                        decode!(decoder, &mut output, segment, true);
                    }
                    0x41 => decode_high_bit_segment(encoding_rs::GBK, segment, &mut output)?,
                    0x43 => decode_high_bit_segment(encoding_rs::EUC_KR, segment, &mut output)?,
                    _ => return Err(DecodeError::UnsupportedEncoding),
                }
                offset = segment_end;
            }
            _ => return Err(DecodeError::UnsupportedEncoding),
        }
    }

    Ok(output)
}

fn decode_high_bit_segment(
    encoding: &'static encoding_rs::Encoding,
    segment: &[u8],
    output: &mut String,
) -> Result<(), DecodeError> {
    // Compound Text stores each byte of GB2312 and KS C 5601 with the high bit cleared.
    let encoded: Vec<u8> = segment.iter().map(|byte| byte | 0x80).collect();
    let (decoded, had_errors) = encoding.decode_without_bom_handling(&encoded);
    if had_errors {
        return Err(DecodeError::InvalidEncoding);
    }
    output.push_str(&decoded);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn korean() {
        const UTF8: &str = "가나다";
        const COMP: &[u8] = &[
            27, 37, 71, 234, 176, 128, 235, 130, 152, 235, 139, 164, 27, 37, 64,
        ];
        assert_eq!(crate::utf8_to_compound_text(UTF8), COMP);
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn iso_2011_jp() {
        const UTF8: &str = "東京";
        const COMP: &[u8] = &[27, 36, 40, 66, 69, 108, 53, 126];
        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), UTF8);
    }

    #[test]
    fn chinese_gb2312_with_ascii_suffix() {
        const COMP: &[u8] = &[
            0x1B, 0x24, 0x28, 0x41, 0x56, 0x50, 0x4E, 0x44, 0x1B, 0x28, 0x42, b'.',
        ];

        assert_eq!(crate::compound_text_to_utf8(COMP).unwrap(), "中文.");
    }

    #[test]
    fn unsupported_charset_is_recoverable() {
        const COMP: &[u8] = &[0x1B, 0x24, 0x28, 0x7F, 0x41, 0x42];

        assert!(matches!(
            crate::compound_text_to_utf8(COMP),
            Err(crate::DecodeError::UnsupportedEncoding)
        ));
    }
}
