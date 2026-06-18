// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use crate::crc::{crc16_xmodem, crc16_xmodem_update, crc32_ieee, crc32_ieee_update};
use crate::error::ModemError;

pub const ZPAD: u8 = b'*';
pub const ZDLE: u8 = 0x18;
pub const ZBIN: u8 = b'A';
pub const ZHEX: u8 = b'B';
pub const ZBIN32: u8 = b'C';
pub const XON: u8 = 0x11;
pub const ZCRCE: u8 = b'h';
pub const ZCRCG: u8 = b'i';
pub const ZCRCQ: u8 = b'j';
pub const ZCRCW: u8 = b'k';

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ZFrameType {
    ZrqInit = 0,
    ZrInit = 1,
    ZsInit = 2,
    ZAck = 3,
    ZFile = 4,
    ZSkip = 5,
    ZNak = 6,
    ZAbort = 7,
    ZFin = 8,
    ZRpos = 9,
    ZData = 10,
    ZEof = 11,
    ZFerr = 12,
    ZCrc = 13,
    ZChallenge = 14,
    ZCompl = 15,
    ZCan = 16,
    ZFreeCnt = 17,
    ZCommand = 18,
    ZStderr = 19,
}

impl ZFrameType {
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::ZrqInit),
            1 => Some(Self::ZrInit),
            2 => Some(Self::ZsInit),
            3 => Some(Self::ZAck),
            4 => Some(Self::ZFile),
            5 => Some(Self::ZSkip),
            6 => Some(Self::ZNak),
            7 => Some(Self::ZAbort),
            8 => Some(Self::ZFin),
            9 => Some(Self::ZRpos),
            10 => Some(Self::ZData),
            11 => Some(Self::ZEof),
            12 => Some(Self::ZFerr),
            13 => Some(Self::ZCrc),
            14 => Some(Self::ZChallenge),
            15 => Some(Self::ZCompl),
            16 => Some(Self::ZCan),
            17 => Some(Self::ZFreeCnt),
            18 => Some(Self::ZCommand),
            19 => Some(Self::ZStderr),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZHeaderEncoding {
    Bin16,
    Bin32,
    Hex,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZHeader {
    pub frame_type: ZFrameType,
    pub position: [u8; 4],
    pub encoding: ZHeaderEncoding,
}

impl ZHeader {
    pub const fn new(frame_type: ZFrameType, position: [u8; 4], encoding: ZHeaderEncoding) -> Self {
        Self {
            frame_type,
            position,
            encoding,
        }
    }

    pub fn position_u32(self) -> u32 {
        u32::from_le_bytes(self.position)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZDataEnd {
    End,
    Continue,
    ContinueWithAck,
    EndWithAck,
}

impl ZDataEnd {
    pub const fn marker(self) -> u8 {
        match self {
            Self::End => ZCRCE,
            Self::Continue => ZCRCG,
            Self::ContinueWithAck => ZCRCQ,
            Self::EndWithAck => ZCRCW,
        }
    }

    pub const fn from_marker(marker: u8) -> Option<Self> {
        match marker {
            ZCRCE => Some(Self::End),
            ZCRCG => Some(Self::Continue),
            ZCRCQ => Some(Self::ContinueWithAck),
            ZCRCW => Some(Self::EndWithAck),
            _ => None,
        }
    }
}

pub fn position_header(position: u32) -> [u8; 4] {
    position.to_le_bytes()
}

pub fn encode_hex_header(frame_type: ZFrameType, position: [u8; 4], include_xon: bool) -> Vec<u8> {
    let mut payload = [0u8; 5];
    payload[0] = frame_type as u8;
    payload[1..].copy_from_slice(&position);
    let crc = crc16_xmodem(&payload);

    let mut out = Vec::with_capacity(4 + 14 + 3);
    out.extend_from_slice(&[ZPAD, ZPAD, ZDLE, ZHEX]);
    for byte in payload {
        push_hex_byte(&mut out, byte);
    }
    push_hex_byte(&mut out, (crc >> 8) as u8);
    push_hex_byte(&mut out, crc as u8);
    out.extend_from_slice(&[b'\r', b'\n' | 0x80]);
    if include_xon {
        out.push(XON);
    }
    out
}

pub fn parse_hex_header(bytes: &[u8]) -> Result<ZHeader, ModemError> {
    if bytes.len() < 18 {
        return Err(ModemError::Incomplete);
    }
    if !bytes.starts_with(&[ZPAD, ZPAD, ZDLE, ZHEX]) {
        return Err(ModemError::InvalidMarker);
    }

    let mut decoded = [0u8; 7];
    for (index, slot) in decoded.iter_mut().enumerate() {
        let offset = 4 + index * 2;
        *slot = read_hex_byte(&bytes[offset..offset + 2])?;
    }

    let expected_crc = crc16_xmodem(&decoded[..5]);
    let received_crc = u16::from_be_bytes([decoded[5], decoded[6]]);
    if expected_crc != received_crc {
        return Err(ModemError::InvalidCrc);
    }

    let frame_type = ZFrameType::from_byte(decoded[0]).ok_or(ModemError::InvalidFrameType)?;
    Ok(ZHeader {
        frame_type,
        position: [decoded[1], decoded[2], decoded[3], decoded[4]],
        encoding: ZHeaderEncoding::Hex,
    })
}

pub fn encode_bin16_header(frame_type: ZFrameType, position: [u8; 4]) -> Vec<u8> {
    let mut escaped_payload = Vec::with_capacity(8);
    let mut crc_payload = [0u8; 5];
    crc_payload[0] = frame_type as u8;
    crc_payload[1..].copy_from_slice(&position);

    for byte in crc_payload {
        push_zdle_escaped(&mut escaped_payload, byte);
    }

    let mut crc = crc_payload
        .iter()
        .fold(0u16, |crc, byte| crc16_xmodem_update(crc, *byte));
    crc = crc16_xmodem_update(crc16_xmodem_update(crc, 0), 0);
    push_zdle_escaped(&mut escaped_payload, (crc >> 8) as u8);
    push_zdle_escaped(&mut escaped_payload, crc as u8);

    let mut out = Vec::with_capacity(3 + escaped_payload.len());
    out.extend_from_slice(&[ZPAD, ZDLE, ZBIN]);
    out.extend_from_slice(&escaped_payload);
    out
}

pub fn encode_bin32_header(frame_type: ZFrameType, position: [u8; 4]) -> Vec<u8> {
    let mut escaped_payload = Vec::with_capacity(10);
    let mut crc_payload = [0u8; 5];
    crc_payload[0] = frame_type as u8;
    crc_payload[1..].copy_from_slice(&position);

    for byte in crc_payload {
        push_zdle_escaped(&mut escaped_payload, byte);
    }

    let mut crc = crc_payload
        .iter()
        .fold(0xffff_ffffu32, |crc, byte| crc32_ieee_update(crc, *byte));
    crc = !crc;
    for byte in crc.to_le_bytes() {
        push_zdle_escaped(&mut escaped_payload, byte);
    }

    let mut out = Vec::with_capacity(3 + escaped_payload.len());
    out.extend_from_slice(&[ZPAD, ZDLE, ZBIN32]);
    out.extend_from_slice(&escaped_payload);
    out
}

pub fn parse_binary_header(bytes: &[u8]) -> Result<ZHeader, ModemError> {
    if bytes.len() < 3 {
        return Err(ModemError::Incomplete);
    }
    if bytes[0] != ZPAD || bytes[1] != ZDLE {
        return Err(ModemError::InvalidMarker);
    }

    let (encoding, crc_len) = match bytes[2] {
        ZBIN => (ZHeaderEncoding::Bin16, 2),
        ZBIN32 => (ZHeaderEncoding::Bin32, 4),
        _ => return Err(ModemError::InvalidFrameEncoding),
    };

    let decoded = decode_zdle_payload(&bytes[3..], 5 + crc_len)?;
    let frame_type = ZFrameType::from_byte(decoded[0]).ok_or(ModemError::InvalidFrameType)?;
    let mut position = [0u8; 4];
    position.copy_from_slice(&decoded[1..5]);

    match encoding {
        ZHeaderEncoding::Bin16 => {
            let mut expected_crc = decoded[..5]
                .iter()
                .fold(0u16, |crc, byte| crc16_xmodem_update(crc, *byte));
            expected_crc = crc16_xmodem_update(crc16_xmodem_update(expected_crc, 0), 0);
            let received_crc = u16::from_be_bytes([decoded[5], decoded[6]]);
            if expected_crc != received_crc {
                return Err(ModemError::InvalidCrc);
            }
        }
        ZHeaderEncoding::Bin32 => {
            let expected = crc32_ieee(&decoded[..5]);
            let received = u32::from_le_bytes([decoded[5], decoded[6], decoded[7], decoded[8]]);
            if expected != received {
                return Err(ModemError::InvalidCrc);
            }
        }
        ZHeaderEncoding::Hex => return Err(ModemError::InvalidFrameEncoding),
    }

    Ok(ZHeader {
        frame_type,
        position,
        encoding,
    })
}

pub fn decode_zdle_payload(bytes: &[u8], decoded_len: usize) -> Result<Vec<u8>, ModemError> {
    let mut out = Vec::with_capacity(decoded_len);
    let mut index = 0;
    while out.len() < decoded_len {
        if index >= bytes.len() {
            return Err(ModemError::Incomplete);
        }
        let byte = bytes[index];
        index += 1;
        if byte == ZDLE {
            if index >= bytes.len() {
                return Err(ModemError::Incomplete);
            }
            let escaped = bytes[index];
            index += 1;
            out.push(escaped ^ 0x40);
        } else {
            out.push(byte);
        }
    }
    Ok(out)
}

pub fn push_zdle_escaped(out: &mut Vec<u8>, byte: u8) {
    if should_escape_zdle(byte) {
        out.push(ZDLE);
        out.push(byte ^ 0x40);
    } else {
        out.push(byte);
    }
}

fn should_escape_zdle(byte: u8) -> bool {
    matches!(byte, ZDLE | 0x10 | 0x11 | 0x13 | 0x90 | 0x91 | 0x93)
}

fn push_hex_byte(out: &mut Vec<u8>, byte: u8) {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    out.push(HEX[(byte >> 4) as usize]);
    out.push(HEX[(byte & 0x0f) as usize]);
}

fn read_hex_byte(bytes: &[u8]) -> Result<u8, ModemError> {
    if bytes.len() != 2 {
        return Err(ModemError::Incomplete);
    }
    let high = hex_value(bytes[0])?;
    let low = hex_value(bytes[1])?;
    Ok((high << 4) | low)
}

fn hex_value(byte: u8) -> Result<u8, ModemError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ModemError::InvalidFrameEncoding),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_header_round_trips() {
        let encoded = encode_hex_header(ZFrameType::ZrqInit, position_header(0), true);
        let parsed = parse_hex_header(&encoded).unwrap();
        assert_eq!(parsed.frame_type, ZFrameType::ZrqInit);
        assert_eq!(parsed.position_u32(), 0);
        assert_eq!(parsed.encoding, ZHeaderEncoding::Hex);
    }

    #[test]
    fn hex_header_rejects_bad_crc() {
        let mut encoded = encode_hex_header(ZFrameType::ZrInit, position_header(7), true);
        encoded[16] = b'0';
        assert_eq!(parse_hex_header(&encoded), Err(ModemError::InvalidCrc));
    }

    #[test]
    fn binary_16_header_round_trips() {
        let encoded = encode_bin16_header(ZFrameType::ZData, position_header(4096));
        let parsed = parse_binary_header(&encoded).unwrap();
        assert_eq!(parsed.frame_type, ZFrameType::ZData);
        assert_eq!(parsed.position_u32(), 4096);
        assert_eq!(parsed.encoding, ZHeaderEncoding::Bin16);
    }

    #[test]
    fn binary_32_header_round_trips() {
        let encoded = encode_bin32_header(ZFrameType::ZFile, position_header(0));
        let parsed = parse_binary_header(&encoded).unwrap();
        assert_eq!(parsed.frame_type, ZFrameType::ZFile);
        assert_eq!(parsed.encoding, ZHeaderEncoding::Bin32);
    }

    #[test]
    fn zdle_payload_decodes_escaped_control_bytes() {
        let mut encoded = Vec::new();
        push_zdle_escaped(&mut encoded, ZDLE);
        push_zdle_escaped(&mut encoded, XON);
        assert_eq!(decode_zdle_payload(&encoded, 2).unwrap(), vec![ZDLE, XON]);
    }
}
