// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

use crate::{X11AuthCookie, X11AuthProtocol, X11Display, X11ForwardingError, X11Result};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum X11AuthorityFile {
    Default,
    Path(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11AuthCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl X11AuthCommand {
    pub fn list(display: &X11Display, authority_file: X11AuthorityFile) -> Self {
        let mut args = Vec::new();
        if let X11AuthorityFile::Path(path) = authority_file {
            args.push("-f".to_string());
            args.push(path);
        }
        args.push("list".to_string());
        args.push(display.xauth_query_display());
        Self {
            program: "xauth".to_string(),
            args,
        }
    }

    pub fn nlist(display: &X11Display, authority_file: X11AuthorityFile) -> Self {
        let mut command = Self::list(display, authority_file);
        let subcommand_index = command.args.len() - 2;
        command.args[subcommand_index] = "nlist".to_string();
        command
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11AuthEntry {
    pub display_name: String,
    pub protocol: X11AuthProtocol,
    pub cookie: X11AuthCookie,
}

impl X11AuthEntry {
    pub fn parse_list_line(line: &str) -> X11Result<Option<Self>> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return Ok(None);
        }

        let mut parts = trimmed.split_whitespace();
        let display_name = parts
            .next()
            .ok_or(X11ForwardingError::InvalidXauthRecord("missing display"))?;
        let protocol = parts
            .next()
            .ok_or(X11ForwardingError::InvalidXauthRecord("missing protocol"))?;
        let cookie = parts
            .next()
            .ok_or(X11ForwardingError::InvalidXauthRecord("missing cookie"))?;

        if parts.next().is_some() {
            return Err(X11ForwardingError::InvalidXauthRecord(
                "too many fields in xauth record",
            ));
        }

        let protocol = match X11AuthProtocol::parse(protocol) {
            Ok(protocol) => protocol,
            Err(X11ForwardingError::UnsupportedAuthProtocol(_)) => return Ok(None),
            Err(error) => return Err(error),
        };

        Ok(Some(Self {
            display_name: display_name.to_string(),
            protocol,
            cookie: X11AuthCookie::from_hex(cookie)?,
        }))
    }

    pub fn parse_nlist_line(line: &str) -> X11Result<Option<Self>> {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return Ok(None);
        }

        let parts = trimmed.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 9 {
            return Err(X11ForwardingError::InvalidXauthRecord(
                "nlist record has too few fields",
            ));
        }

        let address = decode_nlist_field(parts[1], parts[2], "address")?;
        let number = decode_nlist_field(parts[3], parts[4], "display number")?;
        let protocol = decode_nlist_field(parts[5], parts[6], "protocol")?;
        let cookie_hex = checked_nlist_hex_field(parts[7], parts[8], "cookie")?;

        let protocol = match X11AuthProtocol::parse(&protocol) {
            Ok(protocol) => protocol,
            Err(X11ForwardingError::UnsupportedAuthProtocol(_)) => return Ok(None),
            Err(error) => return Err(error),
        };

        let display_name = if address.is_empty() {
            format!(":{number}")
        } else {
            format!("{address}:{number}")
        };

        Ok(Some(Self {
            display_name,
            protocol,
            cookie: X11AuthCookie::from_hex(cookie_hex)?,
        }))
    }

    fn match_score(&self, display: &X11Display) -> Option<u8> {
        let (entry_display, entry_screen) = parse_xauth_display_suffix(&self.display_name)?;
        if entry_display != display.display {
            return None;
        }

        match entry_screen {
            Some(screen) if screen == display.screen => Some(3),
            Some(_) => None,
            None => Some(2),
        }
    }
}

pub fn parse_xauth_list(output: &str) -> X11Result<Vec<X11AuthEntry>> {
    output
        .lines()
        .filter_map(|line| X11AuthEntry::parse_list_line(line).transpose())
        .collect()
}

pub fn parse_xauth_nlist(output: &str) -> X11Result<Vec<X11AuthEntry>> {
    output
        .lines()
        .filter_map(|line| X11AuthEntry::parse_nlist_line(line).transpose())
        .collect()
}

pub fn select_xauth_entry<'a>(
    entries: &'a [X11AuthEntry],
    display: &X11Display,
) -> Option<&'a X11AuthEntry> {
    entries
        .iter()
        .filter_map(|entry| entry.match_score(display).map(|score| (score, entry)))
        .max_by_key(|(score, _)| *score)
        .map(|(_, entry)| entry)
}

fn parse_xauth_display_suffix(value: &str) -> Option<(u16, Option<u16>)> {
    let (_, suffix) = value.rsplit_once(':')?;
    let (display, screen) = match suffix.split_once('.') {
        Some((display, screen)) => (display, Some(screen)),
        None => (suffix, None),
    };
    let display = display.parse::<u16>().ok()?;
    let screen = screen.and_then(|screen| screen.parse::<u16>().ok());
    Some((display, screen))
}

fn decode_nlist_field(len_hex: &str, data_hex: &str, label: &'static str) -> X11Result<String> {
    let bytes = checked_nlist_hex_field(len_hex, data_hex, label)?;
    let mut output = String::with_capacity(bytes.len() / 2);
    for pair in bytes.as_bytes().chunks_exact(2) {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        output.push((high << 4 | low) as char);
    }
    Ok(output)
}

fn checked_nlist_hex_field<'a>(
    len_hex: &str,
    data_hex: &'a str,
    label: &'static str,
) -> X11Result<&'a str> {
    let len = usize::from_str_radix(len_hex, 16)
        .map_err(|_| X11ForwardingError::InvalidXauthRecord("invalid nlist length"))?;
    if data_hex.len() != len * 2 || !data_hex.len().is_multiple_of(2) {
        return Err(X11ForwardingError::InvalidXauthRecord(label));
    }
    Ok(data_hex)
}

fn hex_nibble(byte: u8) -> X11Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(X11ForwardingError::InvalidXauthRecord("non-hex nlist data")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xauth_text_formats_preserve_display_and_cookie_records() {
        let cases = [
            (
                parse_xauth_list as fn(&str) -> X11Result<Vec<X11AuthEntry>>,
                r#"
                    host/unix:0  MIT-MAGIC-COOKIE-1  00112233445566778899aabbccddeeff
                    host/unix:0  XDM-AUTHORIZATION-1  deadbeef
                    localhost:10.1 MIT-MAGIC-COOKIE-1 aabbccdd
                "#,
                &[
                    ("host/unix:0", "00112233445566778899aabbccddeeff"),
                    ("localhost:10.1", "aabbccdd"),
                ][..],
            ),
            (
                parse_xauth_nlist,
                concat!(
                    "0100 ",
                    "0009 6c6f63616c686f7374 ",
                    "0004 31302e31 ",
                    "0012 4d49542d4d414749432d434f4f4b49452d31 ",
                    "0004 aabbccdd"
                ),
                &[("localhost:10.1", "aabbccdd")][..],
            ),
        ];
        for (parse, input, expected) in cases {
            let entries = parse(input).unwrap();
            let actual = entries
                .iter()
                .map(|entry| (entry.display_name.as_str(), entry.cookie.to_hex()))
                .collect::<Vec<_>>();
            let expected = expected
                .iter()
                .map(|(display, cookie)| (*display, cookie.to_string()))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn selects_exact_screen_before_display_only() {
        let display = X11Display::parse(":10.1").unwrap();
        let entries = parse_xauth_list(
            r#"
            localhost:10 MIT-MAGIC-COOKIE-1 aabbccdd
            localhost:10.1 MIT-MAGIC-COOKIE-1 00112233
        "#,
        )
        .unwrap();

        let selected = select_xauth_entry(&entries, &display).unwrap();

        assert_eq!(selected.cookie.to_hex(), "00112233");
    }

    #[test]
    fn builds_xauth_commands_without_running_them() {
        let display = X11Display::parse(":0").unwrap();

        assert_eq!(
            X11AuthCommand::list(&display, X11AuthorityFile::Default).args,
            vec!["list".to_string(), ":0".to_string()]
        );
        assert_eq!(
            X11AuthCommand::nlist(&display, X11AuthorityFile::Path("/tmp/auth".to_string())).args,
            vec![
                "-f".to_string(),
                "/tmp/auth".to_string(),
                "nlist".to_string(),
                ":0".to_string()
            ]
        );
    }

    #[test]
    fn xauth_errors_do_not_include_cookie_material() {
        let error =
            X11AuthEntry::parse_list_line("host/unix:0 MIT-MAGIC-COOKIE-1 abc").unwrap_err();

        assert!(!error.to_string().contains("abc"));
    }
}
