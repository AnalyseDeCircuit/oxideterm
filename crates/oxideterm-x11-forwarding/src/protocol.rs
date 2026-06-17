// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt, ops::Range};

use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use crate::{X11AuthCookie, X11AuthMaterial, X11AuthProtocol, X11ForwardingError, X11Result};

const SETUP_HEADER_LEN: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X11ByteOrder {
    BigEndian,
    LittleEndian,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11SetupRequest {
    pub byte_order: X11ByteOrder,
    pub protocol_major: u16,
    pub protocol_minor: u16,
    pub auth_protocol: String,
    pub auth_data_len: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct X11SetupAuthentication {
    pub request: X11SetupRequest,
    pub protocol: X11AuthProtocol,
    pub fake_cookie: X11AuthCookie,
}

pub struct X11AuthFailureResponse {
    pub request: X11SetupRequest,
    pub response: Zeroizing<Vec<u8>>,
}

impl fmt::Debug for X11AuthFailureResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("X11AuthFailureResponse")
            .field("request", &self.request)
            .field("response_len", &self.response.len())
            .finish()
    }
}

pub fn inspect_setup_request(packet: &[u8]) -> X11Result<X11SetupRequest> {
    Ok(parse_setup_packet(packet)?.request)
}

pub fn inspect_setup_authentication(packet: &[u8]) -> X11Result<X11SetupAuthentication> {
    let parsed = parse_setup_packet(packet)?;
    let protocol = X11AuthProtocol::parse(&parsed.request.auth_protocol)?;
    let fake_cookie = X11AuthCookie::from_bytes(packet[parsed.auth_data_range].to_vec())?;

    Ok(X11SetupAuthentication {
        request: parsed.request,
        protocol,
        fake_cookie,
    })
}

pub fn required_setup_packet_len(packet: &[u8]) -> X11Result<Option<usize>> {
    setup_packet_total_len(packet)
}

pub fn rewrite_setup_authentication(
    packet: &mut Vec<u8>,
    auth: &X11AuthMaterial,
) -> X11Result<X11SetupRequest> {
    let parsed = parse_setup_packet(packet)?;
    let request = parsed.request.clone();

    if parsed.auth_protocol_bytes != auth.protocol.ssh_name().as_bytes() {
        return Err(X11ForwardingError::UnsupportedAuthProtocol(
            String::from_utf8_lossy(&parsed.auth_protocol_bytes).into_owned(),
        ));
    }

    let supplied_cookie = &packet[parsed.auth_data_range.clone()];
    if !constant_time_bytes_eq(supplied_cookie, auth.fake_cookie.as_bytes()) {
        return Err(X11ForwardingError::AuthCookieMismatch);
    }

    rewrite_auth_data(packet, &parsed, auth.local_cookie.as_bytes())?;
    Ok(request)
}

pub fn build_auth_failure_response(
    request: &X11SetupRequest,
    reason: &str,
) -> X11Result<X11AuthFailureResponse> {
    let reason_bytes = reason.as_bytes();
    if reason_bytes.len() > u8::MAX as usize {
        return Err(X11ForwardingError::InvalidSetupPacketLength);
    }

    let padded_reason_len = padded_len(reason_bytes.len())?;
    let length_units = padded_reason_len
        .checked_div(4)
        .ok_or(X11ForwardingError::InvalidSetupPacketLength)?;
    if length_units > u16::MAX as usize {
        return Err(X11ForwardingError::InvalidSetupPacketLength);
    }

    let mut response = Vec::with_capacity(8 + padded_reason_len);
    response.push(0);
    response.push(reason_bytes.len() as u8);
    response.extend_from_slice(&[0, 0]);
    response.extend_from_slice(&[0, 0]);
    response.extend_from_slice(&[0, 0]);
    write_u16(
        &mut response[2..4],
        request.protocol_major,
        request.byte_order,
    );
    write_u16(
        &mut response[4..6],
        request.protocol_minor,
        request.byte_order,
    );
    write_u16(&mut response[6..8], length_units as u16, request.byte_order);
    response.extend_from_slice(reason_bytes);
    response.extend(std::iter::repeat_n(
        0,
        padded_reason_len - reason_bytes.len(),
    ));

    Ok(X11AuthFailureResponse {
        request: request.clone(),
        response: Zeroizing::new(response),
    })
}

fn parse_setup_packet(packet: &[u8]) -> X11Result<ParsedSetup> {
    let Some(packet_min_len) = setup_packet_total_len(packet)? else {
        return Err(X11ForwardingError::IncompleteSetupPacket);
    };
    if packet.len() < packet_min_len {
        return Err(X11ForwardingError::IncompleteSetupPacket);
    }

    let byte_order = parse_byte_order(packet[0])?;
    let protocol_major = read_u16(&packet[2..4], byte_order);
    let protocol_minor = read_u16(&packet[4..6], byte_order);
    let auth_protocol_len = read_u16(&packet[6..8], byte_order) as usize;
    let auth_data_len = read_u16(&packet[8..10], byte_order) as usize;

    let auth_protocol_start = SETUP_HEADER_LEN;
    let auth_protocol_padded_len = padded_len(auth_protocol_len)?;
    let auth_protocol_end = auth_protocol_start
        .checked_add(auth_protocol_len)
        .ok_or(X11ForwardingError::InvalidSetupPacketLength)?;
    let auth_data_start = auth_protocol_start
        .checked_add(auth_protocol_padded_len)
        .ok_or(X11ForwardingError::InvalidSetupPacketLength)?;
    let auth_data_end = auth_data_start
        .checked_add(auth_data_len)
        .ok_or(X11ForwardingError::InvalidSetupPacketLength)?;
    let auth_protocol_bytes = packet[auth_protocol_start..auth_protocol_end].to_vec();
    let auth_protocol = String::from_utf8_lossy(&auth_protocol_bytes).into_owned();

    Ok(ParsedSetup {
        request: X11SetupRequest {
            byte_order,
            protocol_major,
            protocol_minor,
            auth_protocol,
            auth_data_len,
        },
        auth_protocol_bytes,
        auth_data_range: auth_data_start..auth_data_end,
        auth_data_padded_end: packet_min_len,
    })
}

fn setup_packet_total_len(packet: &[u8]) -> X11Result<Option<usize>> {
    if packet.len() < SETUP_HEADER_LEN {
        return Ok(None);
    }

    let byte_order = parse_byte_order(packet[0])?;
    let auth_protocol_len = read_u16(&packet[6..8], byte_order) as usize;
    let auth_data_len = read_u16(&packet[8..10], byte_order) as usize;

    let auth_protocol_padded_len = padded_len(auth_protocol_len)?;
    let auth_data_padded_len = padded_len(auth_data_len)?;
    SETUP_HEADER_LEN
        .checked_add(auth_protocol_padded_len)
        .and_then(|value| value.checked_add(auth_data_padded_len))
        .ok_or(X11ForwardingError::InvalidSetupPacketLength)
        .map(Some)
}

fn parse_byte_order(byte: u8) -> X11Result<X11ByteOrder> {
    match byte {
        b'B' => Ok(X11ByteOrder::BigEndian),
        b'l' => Ok(X11ByteOrder::LittleEndian),
        other => Err(X11ForwardingError::UnsupportedByteOrder(other)),
    }
}

fn rewrite_auth_data(
    packet: &mut Vec<u8>,
    parsed: &ParsedSetup,
    replacement: &[u8],
) -> X11Result<()> {
    if replacement.len() > u16::MAX as usize {
        return Err(X11ForwardingError::InvalidSetupPacketLength);
    }

    write_u16(
        &mut packet[8..10],
        replacement.len() as u16,
        parsed.request.byte_order,
    );

    if replacement.len() == parsed.auth_data_range.len() {
        packet[parsed.auth_data_range.clone()].copy_from_slice(replacement);
        return Ok(());
    }

    let tail = packet[parsed.auth_data_padded_end..].to_vec();
    packet.truncate(parsed.auth_data_range.start);
    packet.extend_from_slice(replacement);
    packet.extend(std::iter::repeat_n(
        0,
        padded_len(replacement.len())? - replacement.len(),
    ));
    packet.extend_from_slice(&tail);
    Ok(())
}

fn read_u16(bytes: &[u8], byte_order: X11ByteOrder) -> u16 {
    match byte_order {
        X11ByteOrder::BigEndian => u16::from_be_bytes([bytes[0], bytes[1]]),
        X11ByteOrder::LittleEndian => u16::from_le_bytes([bytes[0], bytes[1]]),
    }
}

fn write_u16(bytes: &mut [u8], value: u16, byte_order: X11ByteOrder) {
    let encoded = match byte_order {
        X11ByteOrder::BigEndian => value.to_be_bytes(),
        X11ByteOrder::LittleEndian => value.to_le_bytes(),
    };
    bytes.copy_from_slice(&encoded);
}

fn padded_len(len: usize) -> X11Result<usize> {
    len.checked_add(3)
        .map(|value| value & !3)
        .ok_or(X11ForwardingError::InvalidSetupPacketLength)
}

fn constant_time_bytes_eq(left: &[u8], right: &[u8]) -> bool {
    left.len() == right.len() && bool::from(left.ct_eq(right))
}

struct ParsedSetup {
    request: X11SetupRequest,
    auth_protocol_bytes: Vec<u8>,
    auth_data_range: Range<usize>,
    auth_data_padded_end: usize,
}

#[cfg(test)]
mod tests {
    use crate::{X11AuthCookie, X11AuthMaterial};

    use super::*;

    #[test]
    fn rewrites_little_endian_setup_cookie_in_place() {
        let fake = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let real = X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap();
        let auth = X11AuthMaterial::with_fake_cookie(fake, real);
        let mut packet = setup_packet(X11ByteOrder::LittleEndian, auth.fake_cookie.as_bytes());

        let request = rewrite_setup_authentication(&mut packet, &auth).unwrap();

        assert_eq!(request.byte_order, X11ByteOrder::LittleEndian);
        assert_eq!(request.protocol_major, 11);
        assert_eq!(request.auth_protocol, "MIT-MAGIC-COOKIE-1");
        assert!(
            packet
                .windows(16)
                .any(|window| window == auth.local_cookie.as_bytes())
        );
        assert!(
            !packet
                .windows(16)
                .any(|window| window == auth.fake_cookie.as_bytes())
        );
    }

    #[test]
    fn rewrites_big_endian_cookie_when_replacement_length_changes() {
        let fake = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let real = X11AuthCookie::from_hex("bbbbbbbb").unwrap();
        let auth = X11AuthMaterial::with_fake_cookie(fake, real);
        let mut packet = setup_packet(X11ByteOrder::BigEndian, auth.fake_cookie.as_bytes());

        let request = rewrite_setup_authentication(&mut packet, &auth).unwrap();

        assert_eq!(request.byte_order, X11ByteOrder::BigEndian);
        assert_eq!(read_u16(&packet[8..10], X11ByteOrder::BigEndian), 4);
        assert!(
            packet
                .windows(4)
                .any(|window| window == auth.local_cookie.as_bytes())
        );
    }

    #[test]
    fn rejects_mismatched_cookie_without_leaking_values() {
        let auth = X11AuthMaterial::with_fake_cookie(
            X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap(),
            X11AuthCookie::from_hex("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").unwrap(),
        );
        let wrong = X11AuthCookie::from_hex("cccccccccccccccccccccccccccccccc").unwrap();
        let mut packet = setup_packet(X11ByteOrder::LittleEndian, wrong.as_bytes());

        let error = rewrite_setup_authentication(&mut packet, &auth).unwrap_err();

        assert_eq!(error, X11ForwardingError::AuthCookieMismatch);
        assert!(!error.to_string().contains("aaaa"));
    }

    #[test]
    fn reports_required_setup_packet_length_from_header() {
        let cookie = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let packet = setup_packet(X11ByteOrder::LittleEndian, cookie.as_bytes());

        assert_eq!(required_setup_packet_len(&packet[..5]).unwrap(), None);
        assert_eq!(
            required_setup_packet_len(&packet).unwrap(),
            Some(packet.len())
        );
    }

    #[test]
    fn inspects_setup_authentication_for_registry_lookup() {
        let cookie = X11AuthCookie::from_hex("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa").unwrap();
        let packet = setup_packet(X11ByteOrder::BigEndian, cookie.as_bytes());

        let auth = inspect_setup_authentication(&packet).unwrap();

        assert_eq!(auth.protocol, X11AuthProtocol::MitMagicCookie1);
        assert_eq!(auth.fake_cookie, cookie);
        assert_eq!(auth.request.protocol_major, 11);
    }

    fn setup_packet(byte_order: X11ByteOrder, cookie: &[u8]) -> Vec<u8> {
        let protocol = b"MIT-MAGIC-COOKIE-1";
        let mut packet = Vec::new();
        packet.push(match byte_order {
            X11ByteOrder::BigEndian => b'B',
            X11ByteOrder::LittleEndian => b'l',
        });
        packet.push(0);
        push_u16(&mut packet, 11, byte_order);
        push_u16(&mut packet, 0, byte_order);
        push_u16(&mut packet, protocol.len() as u16, byte_order);
        push_u16(&mut packet, cookie.len() as u16, byte_order);
        push_u16(&mut packet, 0, byte_order);
        packet.extend_from_slice(protocol);
        packet.extend(std::iter::repeat_n(
            0,
            padded_len(protocol.len()).unwrap() - protocol.len(),
        ));
        packet.extend_from_slice(cookie);
        packet.extend(std::iter::repeat_n(
            0,
            padded_len(cookie.len()).unwrap() - cookie.len(),
        ));
        packet
    }

    fn push_u16(packet: &mut Vec<u8>, value: u16, byte_order: X11ByteOrder) {
        packet.extend_from_slice(&match byte_order {
            X11ByteOrder::BigEndian => value.to_be_bytes(),
            X11ByteOrder::LittleEndian => value.to_le_bytes(),
        });
    }
}
