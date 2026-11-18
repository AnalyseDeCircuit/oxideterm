pub fn append_local_preview_chunk(
    lines: &mut Vec<String>,
    carry_text: &mut String,
    carry_bytes: &mut Vec<u8>,
    data: Vec<u8>,
    eof: bool,
) {
    if data.is_empty() && !eof {
        return;
    }

    let mut bytes = std::mem::take(carry_bytes);
    bytes.extend_from_slice(&data);
    let mut text = String::new();

    match std::str::from_utf8(&bytes) {
        Ok(valid) => text.push_str(valid),
        Err(error) => {
            let valid_up_to = error.valid_up_to();
            if valid_up_to > 0 {
                if let Ok(valid) = std::str::from_utf8(&bytes[..valid_up_to]) {
                    text.push_str(valid);
                }
            }
            let tail = &bytes[valid_up_to..];
            if eof {
                text.push_str(&String::from_utf8_lossy(tail));
            } else {
                carry_bytes.extend_from_slice(tail);
            }
        }
    }

    if eof && !carry_bytes.is_empty() {
        text.push_str(&String::from_utf8_lossy(&carry_bytes));
        carry_bytes.clear();
    }

    append_local_preview_text(lines, carry_text, &text, eof);
}

pub fn append_local_preview_text(
    lines: &mut Vec<String>,
    carry_text: &mut String,
    text: &str,
    eof: bool,
) {
    if text.is_empty() && !eof {
        return;
    }
    let combined = format!("{}{}", carry_text, text);
    let mut parts = combined.split('\n').map(str::to_string).collect::<Vec<_>>();

    if eof {
        carry_text.clear();
        lines.extend(parts);
    } else {
        *carry_text = parts.pop().unwrap_or_default();
        lines.extend(parts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_chunks_keep_unicode_and_unterminated_lines_across_reads() {
        let mut lines = Vec::new();
        let mut text = String::new();
        let mut carry = Vec::new();
        let bytes = "one\nstréam\nlast".as_bytes();
        append_local_preview_chunk(
            &mut lines,
            &mut text,
            &mut carry,
            bytes[..8].to_vec(),
            false,
        );
        assert_eq!(lines, ["one"]);
        append_local_preview_chunk(&mut lines, &mut text, &mut carry, bytes[8..].to_vec(), true);
        assert_eq!(lines, ["one", "stréam", "last"]);
    }
}
