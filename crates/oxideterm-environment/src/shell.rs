// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt::Write as _;

const ENCODED_POSIX_PREFIX: &str = "/bin/sh -c 'eval \"$(printf \"%b\" \"$1\")\"' oxide '";

/// Quote one value for use as a single POSIX shell argument.
pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// Carry a POSIX script containing dynamic paths or control syntax through any
/// remote login shell (including Fish, which interprets `\'` and `\\` inside
/// single quotes) without relying on the login shell's parser or quoting rules.
pub fn encoded_posix_shell_command(script: &str) -> String {
    fn literal(byte: u8) -> bool {
        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'.' | b'_' | b'-')
    }
    let size = script.len() + 4 * script.bytes().filter(|&byte| !literal(byte)).count();
    let mut line = String::with_capacity(ENCODED_POSIX_PREFIX.len() + size + 1);
    line.push_str(ENCODED_POSIX_PREFIX);
    for byte in script.bytes() {
        if literal(byte) {
            line.push(char::from(byte));
        } else {
            write!(line, "\\0{byte:03o}").expect("writing to a String is infallible");
        }
    }
    line.push('\'');
    line
}

#[cfg(test)]
pub(crate) fn decoded_posix_script(line: &str) -> String {
    let data = line
        .strip_prefix(ENCODED_POSIX_PREFIX)
        .and_then(|data| data.strip_suffix('\''))
        .expect("encoded posix wrapper");
    let mut rest = data.as_bytes();
    let mut bytes = Vec::new();
    while let Some((&head, tail)) = rest.split_first() {
        if head == b'\\' {
            assert_eq!(tail[0], b'0');
            let digits = std::str::from_utf8(&tail[1..4]).unwrap();
            bytes.push(u8::from_str_radix(digits, 8).unwrap());
            rest = &tail[4..];
        } else {
            bytes.push(head);
            rest = tail;
        }
    }
    String::from_utf8(bytes).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn posix_shell_quote_preserves_empty_space_and_apostrophe_values() {
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("Oxide Term"), "'Oxide Term'");
        assert_eq!(shell_quote("it's-ready"), "'it'\\''s-ready'");
    }

    #[test]
    fn encoded_posix_shell_command_round_trips_quotes_and_backslashes_without_fish_escapes() {
        let script = r#"cd -- '/repo\name/it'\''s' && printf 'ok\0'"#;
        let wrapped = encoded_posix_shell_command(script);
        let payload = wrapped
            .strip_prefix(ENCODED_POSIX_PREFIX)
            .and_then(|rest| rest.strip_suffix('\''))
            .expect("encoded wrapper");
        assert!(!payload.contains('\''));
        assert!(!payload.contains("\\\\"));
        assert_eq!(decoded_posix_script(&wrapped), script);
    }
}
