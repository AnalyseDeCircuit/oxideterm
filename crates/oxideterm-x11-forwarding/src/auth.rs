// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use rand::{RngCore, rngs::OsRng};
use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::{X11ForwardingError, X11Result};

const DEFAULT_COOKIE_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X11AuthProtocol {
    MitMagicCookie1,
}

impl X11AuthProtocol {
    pub fn parse(input: &str) -> X11Result<Self> {
        match input.trim() {
            "MIT-MAGIC-COOKIE-1" => Ok(Self::MitMagicCookie1),
            other => Err(X11ForwardingError::UnsupportedAuthProtocol(
                other.to_string(),
            )),
        }
    }

    pub fn ssh_name(self) -> &'static str {
        "MIT-MAGIC-COOKIE-1"
    }
}

impl fmt::Display for X11AuthProtocol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.ssh_name())
    }
}

#[derive(Clone)]
pub struct X11AuthCookie {
    bytes: Zeroizing<Vec<u8>>,
}

impl X11AuthCookie {
    pub fn random() -> Self {
        Self::random_with_len(DEFAULT_COOKIE_BYTES)
    }

    pub fn random_with_len(len: usize) -> Self {
        // A zero-length bearer cookie is never useful, but keep the method
        // infallible for callers that mirror OpenSSH's fake-cookie generation.
        let len = len.max(1);
        let mut bytes = vec![0u8; len];
        OsRng.fill_bytes(&mut bytes);
        Self {
            bytes: Zeroizing::new(bytes),
        }
    }

    pub fn from_bytes(bytes: Vec<u8>) -> X11Result<Self> {
        if bytes.is_empty() {
            return Err(X11ForwardingError::InvalidAuthCookie(
                "cookie bytes must not be empty".to_string(),
            ));
        }
        Ok(Self {
            bytes: Zeroizing::new(bytes),
        })
    }

    pub fn from_hex(input: &str) -> X11Result<Self> {
        let hex = input.trim();
        if hex.is_empty() {
            return Err(X11ForwardingError::InvalidAuthCookie(
                "hex cookie must not be empty".to_string(),
            ));
        }
        if hex.len() % 2 != 0 {
            return Err(X11ForwardingError::InvalidAuthCookie(
                "hex cookie must contain an even number of digits".to_string(),
            ));
        }

        let mut bytes = Vec::with_capacity(hex.len() / 2);
        for pair in hex.as_bytes().chunks_exact(2) {
            let high = decode_hex_nibble(pair[0])?;
            let low = decode_hex_nibble(pair[1])?;
            bytes.push((high << 4) | low);
        }
        Self::from_bytes(bytes)
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.bytes.as_slice()
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    pub fn to_hex(&self) -> String {
        let mut output = String::with_capacity(self.bytes.len() * 2);
        for byte in self.bytes.iter().copied() {
            push_hex_byte(&mut output, byte);
        }
        output
    }

    pub fn constant_time_eq(&self, other: &Self) -> bool {
        self.as_bytes().ct_eq(other.as_bytes()).into()
    }
}

impl fmt::Debug for X11AuthCookie {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // X11 cookies are bearer credentials; Debug must expose shape only.
        f.debug_struct("X11AuthCookie")
            .field("len", &self.len())
            .field("bytes", &"<redacted>")
            .finish()
    }
}

impl PartialEq for X11AuthCookie {
    fn eq(&self, other: &Self) -> bool {
        self.constant_time_eq(other)
    }
}

impl Eq for X11AuthCookie {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11AuthMaterial {
    pub protocol: X11AuthProtocol,
    pub fake_cookie: X11AuthCookie,
    pub local_cookie: X11AuthCookie,
}

impl X11AuthMaterial {
    pub fn mit_magic_cookie(local_cookie: X11AuthCookie) -> Self {
        let fake_cookie = X11AuthCookie::random_with_len(local_cookie.len());
        Self {
            protocol: X11AuthProtocol::MitMagicCookie1,
            fake_cookie,
            local_cookie,
        }
    }

    pub fn with_fake_cookie(fake_cookie: X11AuthCookie, local_cookie: X11AuthCookie) -> Self {
        Self {
            protocol: X11AuthProtocol::MitMagicCookie1,
            fake_cookie,
            local_cookie,
        }
    }

    pub fn ssh_auth_protocol(&self) -> &'static str {
        self.protocol.ssh_name()
    }

    pub fn ssh_auth_cookie(&self) -> String {
        self.fake_cookie.to_hex()
    }
}

fn decode_hex_nibble(byte: u8) -> X11Result<u8> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(X11ForwardingError::InvalidAuthCookie(
            "hex cookie contains a non-hex digit".to_string(),
        )),
    }
}

fn push_hex_byte(output: &mut String, byte: u8) {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    output.push(TABLE[(byte >> 4) as usize] as char);
    output.push(TABLE[(byte & 0x0f) as usize] as char);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_hex_round_trips_without_debug_leaking_secret() {
        let cookie = X11AuthCookie::from_hex("001122AABBccDDee").unwrap();

        assert_eq!(cookie.to_hex(), "001122aabbccddee");
        assert_eq!(cookie.len(), 8);
        let debug = format!("{cookie:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("001122"));
    }

    #[test]
    fn cookie_rejects_invalid_hex() {
        assert!(matches!(
            X11AuthCookie::from_hex("abc"),
            Err(X11ForwardingError::InvalidAuthCookie(_))
        ));
        assert!(matches!(
            X11AuthCookie::from_hex("00zz"),
            Err(X11ForwardingError::InvalidAuthCookie(_))
        ));
    }

    #[test]
    fn auth_material_exposes_fake_cookie_for_ssh_only() {
        let fake = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let real = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let material = X11AuthMaterial::with_fake_cookie(fake.clone(), real);

        assert_eq!(material.ssh_auth_protocol(), "MIT-MAGIC-COOKIE-1");
        assert_eq!(
            material.ssh_auth_cookie(),
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
        assert_eq!(material.fake_cookie, fake);
    }

    #[test]
    fn parses_auth_protocol_names_strictly() {
        assert_eq!(
            X11AuthProtocol::parse("MIT-MAGIC-COOKIE-1").unwrap(),
            X11AuthProtocol::MitMagicCookie1
        );
        assert!(matches!(
            X11AuthProtocol::parse("XDM-AUTHORIZATION-1"),
            Err(X11ForwardingError::UnsupportedAuthProtocol(_))
        ));
    }
}
