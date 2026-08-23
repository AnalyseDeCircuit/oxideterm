// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::borrow::Cow;

/// Decodes the byte escaping used by `%output` and `%extended-output`.
///
/// Plain payloads remain borrowed. Escaped payloads allocate exactly one
/// destination buffer, which can then move directly into a terminal emulator.
pub fn decode_output(source: &[u8]) -> Cow<'_, [u8]> {
    let Some(first_escape) = source.iter().position(|byte| *byte == b'\\') else {
        return Cow::Borrowed(source);
    };

    let mut decoded = Vec::with_capacity(source.len());
    decoded.extend_from_slice(&source[..first_escape]);
    let mut cursor = first_escape;

    while cursor < source.len() {
        if source[cursor] != b'\\' {
            decoded.push(source[cursor]);
            cursor += 1;
            continue;
        }

        if source.get(cursor + 1) == Some(&b'\\') {
            decoded.push(b'\\');
            cursor += 2;
            continue;
        }

        let Some(digits) = source.get(cursor + 1..cursor + 4) else {
            decoded.push(b'\\');
            cursor += 1;
            continue;
        };
        let Some(byte) = decode_octal_byte(digits) else {
            // Unknown escapes are retained byte-for-byte so protocol drift does
            // not silently corrupt terminal output.
            decoded.push(b'\\');
            cursor += 1;
            continue;
        };

        decoded.push(byte);
        cursor += 4;
    }

    Cow::Owned(decoded)
}

fn decode_octal_byte(digits: &[u8]) -> Option<u8> {
    let value = digits.iter().try_fold(0_u16, |value, digit| {
        let digit = digit.checked_sub(b'0')?;
        (digit <= 7)
            .then_some(value)?
            .checked_mul(8)?
            .checked_add(u16::from(digit))
    })?;
    u8::try_from(value).ok()
}
