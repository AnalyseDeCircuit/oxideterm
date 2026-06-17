// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::{
    X11AuthCookie, X11AuthProtocol, X11Display, X11DisplayTransport, X11ForwardingError, X11Result,
};

const FAMILY_INTERNET: u16 = 0;
const FAMILY_DECNET: u16 = 1;
const FAMILY_INTERNET6: u16 = 6;
const FAMILY_LOCAL: u16 = 256;
const FAMILY_WILD: u16 = 65535;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X11AuthorityFamily {
    Internet,
    Decnet,
    Internet6,
    Local,
    Wild,
    Other(u16),
}

impl X11AuthorityFamily {
    pub fn from_code(code: u16) -> Self {
        match code {
            FAMILY_INTERNET => Self::Internet,
            FAMILY_DECNET => Self::Decnet,
            FAMILY_INTERNET6 => Self::Internet6,
            FAMILY_LOCAL => Self::Local,
            FAMILY_WILD => Self::Wild,
            other => Self::Other(other),
        }
    }

    pub fn code(self) -> u16 {
        match self {
            Self::Internet => FAMILY_INTERNET,
            Self::Decnet => FAMILY_DECNET,
            Self::Internet6 => FAMILY_INTERNET6,
            Self::Local => FAMILY_LOCAL,
            Self::Wild => FAMILY_WILD,
            Self::Other(code) => code,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11BinaryAuthorityEntry {
    pub family: X11AuthorityFamily,
    pub address: Vec<u8>,
    pub display_number: String,
    pub protocol: X11AuthProtocol,
    pub cookie: X11AuthCookie,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct X11AuthorityMatchContext {
    pub local_hostname: Option<String>,
    pub resolved_addresses: Vec<IpAddr>,
}

impl X11AuthorityMatchContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_local_hostname(mut self, hostname: impl Into<String>) -> Self {
        self.local_hostname = Some(hostname.into());
        self
    }

    pub fn with_resolved_address(mut self, address: IpAddr) -> Self {
        self.resolved_addresses.push(address);
        self
    }
}

pub fn parse_xauthority_file(bytes: &[u8]) -> X11Result<Vec<X11BinaryAuthorityEntry>> {
    let mut cursor = 0;
    let mut entries = Vec::new();

    while cursor < bytes.len() {
        let family = X11AuthorityFamily::from_code(read_u16(bytes, &mut cursor)?);
        let address = read_field(bytes, &mut cursor)?;
        let display_number = read_text_field(bytes, &mut cursor)?;
        let protocol_name = read_text_field(bytes, &mut cursor)?;
        let cookie = read_field(bytes, &mut cursor)?;

        let protocol = match X11AuthProtocol::parse(&protocol_name) {
            Ok(protocol) => protocol,
            Err(X11ForwardingError::UnsupportedAuthProtocol(_)) => continue,
            Err(error) => return Err(error),
        };

        entries.push(X11BinaryAuthorityEntry {
            family,
            address,
            display_number,
            protocol,
            cookie: X11AuthCookie::from_bytes(cookie)?,
        });
    }

    Ok(entries)
}

pub fn select_xauthority_entry<'a>(
    entries: &'a [X11BinaryAuthorityEntry],
    display: &X11Display,
    context: &X11AuthorityMatchContext,
) -> Option<&'a X11BinaryAuthorityEntry> {
    entries
        .iter()
        .filter_map(|entry| {
            entry
                .match_score(display, context)
                .map(|score| (score, entry))
        })
        .max_by_key(|(score, _)| *score)
        .map(|(_, entry)| entry)
}

impl X11BinaryAuthorityEntry {
    fn match_score(&self, display: &X11Display, context: &X11AuthorityMatchContext) -> Option<u8> {
        if self.display_number != display.display.to_string() {
            return None;
        }

        match self.family {
            X11AuthorityFamily::Wild => Some(1),
            X11AuthorityFamily::Local => local_family_score(&self.address, display, context),
            X11AuthorityFamily::Internet => {
                internet_family_score(&self.address, display, context, IpFamily::V4)
            }
            X11AuthorityFamily::Internet6 => {
                internet_family_score(&self.address, display, context, IpFamily::V6)
            }
            X11AuthorityFamily::Decnet | X11AuthorityFamily::Other(_) => None,
        }
    }
}

fn local_family_score(
    address: &[u8],
    display: &X11Display,
    context: &X11AuthorityMatchContext,
) -> Option<u8> {
    if !display_looks_local(display) {
        return None;
    }

    if address.is_empty()
        || address == b"localhost"
        || context
            .local_hostname
            .as_deref()
            .is_some_and(|hostname| address == hostname.as_bytes())
    {
        Some(4)
    } else {
        // Some xauth files store the local hostname even when the caller did
        // not provide one. Prefer this over wildcard, but below exact IP hits.
        Some(2)
    }
}

fn internet_family_score(
    address: &[u8],
    display: &X11Display,
    context: &X11AuthorityMatchContext,
    family: IpFamily,
) -> Option<u8> {
    let expected = match family {
        IpFamily::V4 if address.len() == 4 => IpAddr::V4(Ipv4Addr::new(
            address[0], address[1], address[2], address[3],
        )),
        IpFamily::V6 if address.len() == 16 => {
            let mut bytes = [0u8; 16];
            bytes.copy_from_slice(address);
            IpAddr::V6(Ipv6Addr::from(bytes))
        }
        _ => return None,
    };

    let host_ip = match &display.transport {
        X11DisplayTransport::Tcp { host } => host.parse::<IpAddr>().ok(),
        X11DisplayTransport::Unix | X11DisplayTransport::UnixSocket { .. } => None,
    };

    if host_ip == Some(expected) || context.resolved_addresses.contains(&expected) {
        Some(5)
    } else if display_looks_local(display) && expected.is_loopback() {
        Some(3)
    } else {
        None
    }
}

fn display_looks_local(display: &X11Display) -> bool {
    match &display.transport {
        X11DisplayTransport::Unix | X11DisplayTransport::UnixSocket { .. } => true,
        X11DisplayTransport::Tcp { host } => {
            matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1")
        }
    }
}

fn read_u16(bytes: &[u8], cursor: &mut usize) -> X11Result<u16> {
    let end = cursor
        .checked_add(2)
        .ok_or(X11ForwardingError::InvalidXauthRecord(
            "xauthority cursor overflow",
        ))?;
    let chunk = bytes
        .get(*cursor..end)
        .ok_or(X11ForwardingError::InvalidXauthRecord(
            "truncated xauthority record",
        ))?;
    *cursor = end;
    Ok(u16::from_be_bytes([chunk[0], chunk[1]]))
}

fn read_field(bytes: &[u8], cursor: &mut usize) -> X11Result<Vec<u8>> {
    let len = read_u16(bytes, cursor)? as usize;
    let end = cursor
        .checked_add(len)
        .ok_or(X11ForwardingError::InvalidXauthRecord(
            "xauthority field length overflow",
        ))?;
    let value = bytes
        .get(*cursor..end)
        .ok_or(X11ForwardingError::InvalidXauthRecord(
            "truncated xauthority field",
        ))?
        .to_vec();
    *cursor = end;
    Ok(value)
}

fn read_text_field(bytes: &[u8], cursor: &mut usize) -> X11Result<String> {
    let field = read_field(bytes, cursor)?;
    Ok(String::from_utf8_lossy(&field).into_owned())
}

#[derive(Clone, Copy)]
enum IpFamily {
    V4,
    V6,
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    use super::*;

    #[test]
    fn parses_binary_xauthority_records_and_redacts_cookie_debug() {
        let mut bytes = Vec::new();
        push_record(
            &mut bytes,
            X11AuthorityFamily::Local,
            b"workstation",
            b"0",
            b"MIT-MAGIC-COOKIE-1",
            &[0xaa; 16],
        );

        let entries = parse_xauthority_file(&bytes).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].family, X11AuthorityFamily::Local);
        assert_eq!(entries[0].display_number, "0");
        assert!(!format!("{:?}", entries[0]).contains("aaaaaaaa"));
    }

    #[test]
    fn select_xauthority_prefers_exact_ip_family_over_wildcard() {
        let mut bytes = Vec::new();
        push_record(
            &mut bytes,
            X11AuthorityFamily::Wild,
            b"",
            b"10",
            b"MIT-MAGIC-COOKIE-1",
            &[0xaa; 16],
        );
        push_record(
            &mut bytes,
            X11AuthorityFamily::Internet,
            &[192, 168, 1, 12],
            b"10",
            b"MIT-MAGIC-COOKIE-1",
            &[0xbb; 16],
        );
        let entries = parse_xauthority_file(&bytes).unwrap();
        let display = X11Display::parse("192.168.1.12:10").unwrap();

        let selected =
            select_xauthority_entry(&entries, &display, &X11AuthorityMatchContext::new()).unwrap();

        assert_eq!(selected.cookie.to_hex(), "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    }

    #[test]
    fn select_xauthority_uses_local_hostname_for_unix_display() {
        let mut bytes = Vec::new();
        push_record(
            &mut bytes,
            X11AuthorityFamily::Local,
            b"workstation",
            b"0",
            b"MIT-MAGIC-COOKIE-1",
            &[0xbb; 16],
        );
        let entries = parse_xauthority_file(&bytes).unwrap();
        let display = X11Display::parse(":0").unwrap();
        let context = X11AuthorityMatchContext::new().with_local_hostname("workstation");

        let selected = select_xauthority_entry(&entries, &display, &context).unwrap();

        assert_eq!(selected.cookie.to_hex(), "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
    }

    #[test]
    fn select_xauthority_can_use_resolved_ipv6_address() {
        let mut bytes = Vec::new();
        let address = Ipv6Addr::LOCALHOST.octets();
        push_record(
            &mut bytes,
            X11AuthorityFamily::Internet6,
            &address,
            b"4",
            b"MIT-MAGIC-COOKIE-1",
            &[0xcc; 16],
        );
        let entries = parse_xauthority_file(&bytes).unwrap();
        let display = X11Display::parse("xserver.local:4").unwrap();
        let context =
            X11AuthorityMatchContext::new().with_resolved_address(IpAddr::V6(Ipv6Addr::LOCALHOST));

        let selected = select_xauthority_entry(&entries, &display, &context).unwrap();

        assert_eq!(selected.cookie.to_hex(), "cccccccccccccccccccccccccccccccc");
    }

    #[test]
    fn parse_xauthority_rejects_truncated_record() {
        let error = parse_xauthority_file(&[0, 0, 0, 10, 1, 2]).unwrap_err();

        assert!(matches!(
            error,
            X11ForwardingError::InvalidXauthRecord("truncated xauthority field")
        ));
    }

    #[test]
    fn unsupported_xauthority_protocols_are_skipped() {
        let mut bytes = Vec::new();
        push_record(
            &mut bytes,
            X11AuthorityFamily::Local,
            b"workstation",
            b"0",
            b"XDM-AUTHORIZATION-1",
            &[0xaa; 16],
        );

        assert!(parse_xauthority_file(&bytes).unwrap().is_empty());
    }

    fn push_record(
        output: &mut Vec<u8>,
        family: X11AuthorityFamily,
        address: &[u8],
        display_number: &[u8],
        protocol: &[u8],
        cookie: &[u8],
    ) {
        push_u16(output, family.code());
        push_field(output, address);
        push_field(output, display_number);
        push_field(output, protocol);
        push_field(output, cookie);
    }

    fn push_field(output: &mut Vec<u8>, value: &[u8]) {
        push_u16(output, value.len() as u16);
        output.extend_from_slice(value);
    }

    fn push_u16(output: &mut Vec<u8>, value: u16) {
        output.extend_from_slice(&value.to_be_bytes());
    }

    #[test]
    fn loopback_ipv4_matches_local_display_when_hostname_is_localhost() {
        let mut bytes = Vec::new();
        push_record(
            &mut bytes,
            X11AuthorityFamily::Internet,
            &Ipv4Addr::LOCALHOST.octets(),
            b"0",
            b"MIT-MAGIC-COOKIE-1",
            &[0xdd; 16],
        );
        let entries = parse_xauthority_file(&bytes).unwrap();
        let display = X11Display::parse("localhost:0").unwrap();

        let selected =
            select_xauthority_entry(&entries, &display, &X11AuthorityMatchContext::new()).unwrap();

        assert_eq!(selected.cookie.to_hex(), "dddddddddddddddddddddddddddddddd");
    }
}
