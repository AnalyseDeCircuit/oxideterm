// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

pub type EscapeCode = [u8; 3];

pub fn escape_chars_to_codes(escape_chars: &[Vec<String>]) -> Vec<EscapeCode> {
    escape_chars
        .iter()
        .filter_map(|escape_char| {
            let from = escape_char.first()?.as_bytes().first().copied()?;
            let to = escape_char.get(1)?.as_bytes();
            if to.len() < 2 {
                return None;
            }
            Some([from, to[0], to[1]])
        })
        .collect()
}

pub fn escape_data(data: &[u8], escape_codes: &[EscapeCode]) -> Vec<u8> {
    if escape_codes.is_empty() {
        return data.to_vec();
    }

    let mut buffer = Vec::with_capacity(data.len() * 2);
    for value in data {
        if let Some(escape_code) = escape_codes
            .iter()
            .find(|escape_code| *value == escape_code[0])
        {
            buffer.push(escape_code[1]);
            buffer.push(escape_code[2]);
        } else {
            buffer.push(*value);
        }
    }
    buffer
}

pub fn unescape_data(data: &[u8], escape_codes: &[EscapeCode]) -> Vec<u8> {
    if escape_codes.is_empty() {
        return data.to_vec();
    }

    let mut buffer = Vec::with_capacity(data.len());
    let mut source_index = 0;
    while source_index < data.len() {
        let escape_code = if source_index < data.len() - 1 {
            escape_codes.iter().find(|escape_code| {
                data[source_index] == escape_code[1] && data[source_index + 1] == escape_code[2]
            })
        } else {
            None
        };

        if let Some(escape_code) = escape_code {
            buffer.push(escape_code[0]);
            source_index += 2;
        } else {
            buffer.push(data[source_index]);
            source_index += 1;
        }
    }
    buffer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_and_unescapes_binary_data_like_tauri() {
        let codes = vec![[b'\n', b'!', b'N'], [b'\r', b'!', b'R']];
        let escaped = escape_data(b"a\nb\rc", &codes);
        assert_eq!(escaped, b"a!Nb!Rc");
        assert_eq!(unescape_data(&escaped, &codes), b"a\nb\rc");
    }
}
