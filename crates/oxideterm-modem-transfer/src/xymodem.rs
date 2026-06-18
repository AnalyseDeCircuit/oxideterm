// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::path::Path;

use crate::crc::crc16_xmodem;
use crate::error::ModemError;

pub const SOH: u8 = 0x01;
pub const STX: u8 = 0x02;
pub const EOT: u8 = 0x04;
pub const ACK: u8 = 0x06;
pub const NAK: u8 = 0x15;
pub const CAN: u8 = 0x18;
pub const WANT_CRC: u8 = b'C';
pub const CPMEOF: u8 = 0x1a;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XyBlockSize {
    Bytes128,
    Bytes1024,
}

impl XyBlockSize {
    pub const fn len(self) -> usize {
        match self {
            Self::Bytes128 => 128,
            Self::Bytes1024 => 1024,
        }
    }

    pub const fn marker(self) -> u8 {
        match self {
            Self::Bytes128 => SOH,
            Self::Bytes1024 => STX,
        }
    }

    pub const fn from_marker(marker: u8) -> Option<Self> {
        match marker {
            SOH => Some(Self::Bytes128),
            STX => Some(Self::Bytes1024),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XyBlock {
    pub number: u8,
    pub size: XyBlockSize,
    pub payload: Vec<u8>,
}

impl XyBlock {
    pub fn new(number: u8, size: XyBlockSize, payload: &[u8]) -> Result<Self, ModemError> {
        if payload.len() > size.len() {
            return Err(ModemError::InvalidLength);
        }

        let mut padded = vec![CPMEOF; size.len()];
        padded[..payload.len()].copy_from_slice(payload);

        Ok(Self {
            number,
            size,
            payload: padded,
        })
    }

    pub fn encode_crc(&self) -> Vec<u8> {
        let crc = crc16_xmodem(&self.payload);
        let mut bytes = Vec::with_capacity(3 + self.payload.len() + 2);
        bytes.extend_from_slice(&[self.size.marker(), self.number, !self.number]);
        bytes.extend_from_slice(&self.payload);
        bytes.extend_from_slice(&crc.to_be_bytes());
        bytes
    }

    pub fn encode_checksum(&self) -> Vec<u8> {
        let checksum = self
            .payload
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        let mut bytes = Vec::with_capacity(3 + self.payload.len() + 1);
        bytes.extend_from_slice(&[self.size.marker(), self.number, !self.number]);
        bytes.extend_from_slice(&self.payload);
        bytes.push(checksum);
        bytes
    }

    pub fn parse_crc(bytes: &[u8]) -> Result<Self, ModemError> {
        if bytes.len() < 3 {
            return Err(ModemError::Incomplete);
        }

        let size = XyBlockSize::from_marker(bytes[0]).ok_or(ModemError::InvalidMarker)?;
        let expected_len = 3 + size.len() + 2;
        if bytes.len() < expected_len {
            return Err(ModemError::Incomplete);
        }
        if bytes.len() != expected_len {
            return Err(ModemError::InvalidLength);
        }

        let number = bytes[1];
        if number.wrapping_add(bytes[2]) != 0xff {
            return Err(ModemError::InvalidBlockNumber);
        }

        let payload = bytes[3..3 + size.len()].to_vec();
        let received_crc = u16::from_be_bytes([bytes[3 + size.len()], bytes[3 + size.len() + 1]]);
        if crc16_xmodem(&payload) != received_crc {
            return Err(ModemError::InvalidCrc);
        }

        Ok(Self {
            number,
            size,
            payload,
        })
    }

    pub fn parse_checksum(bytes: &[u8]) -> Result<Self, ModemError> {
        if bytes.len() < 3 {
            return Err(ModemError::Incomplete);
        }

        let size = XyBlockSize::from_marker(bytes[0]).ok_or(ModemError::InvalidMarker)?;
        let expected_len = 3 + size.len() + 1;
        if bytes.len() < expected_len {
            return Err(ModemError::Incomplete);
        }
        if bytes.len() != expected_len {
            return Err(ModemError::InvalidLength);
        }

        let number = bytes[1];
        if number.wrapping_add(bytes[2]) != 0xff {
            return Err(ModemError::InvalidBlockNumber);
        }

        let payload = bytes[3..3 + size.len()].to_vec();
        let checksum = payload
            .iter()
            .fold(0u8, |sum, byte| sum.wrapping_add(*byte));
        if checksum != bytes[3 + size.len()] {
            return Err(ModemError::InvalidChecksum);
        }

        Ok(Self {
            number,
            size,
            payload,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YmodemFileHeader {
    pub file_name: String,
    pub file_size: Option<u64>,
}

impl YmodemFileHeader {
    pub fn new(file_name: impl Into<String>, file_size: Option<u64>) -> Result<Self, ModemError> {
        let file_name = file_name.into();
        validate_ymodem_file_name(&file_name)?;
        Ok(Self {
            file_name,
            file_size,
        })
    }

    pub fn encode_block0_payload(&self) -> Result<Vec<u8>, ModemError> {
        let mut payload = vec![0u8; XyBlockSize::Bytes128.len()];
        let name_bytes = self.file_name.as_bytes();
        if name_bytes.is_empty() || name_bytes.len() >= payload.len() {
            return Err(ModemError::InvalidFileName);
        }

        payload[..name_bytes.len()].copy_from_slice(name_bytes);
        if let Some(file_size) = self.file_size {
            let size_text = file_size.to_string();
            let size_start = name_bytes.len() + 1;
            let size_end = size_start + size_text.len();
            if size_end >= payload.len() {
                return Err(ModemError::InvalidLength);
            }
            payload[size_start..size_end].copy_from_slice(size_text.as_bytes());
        }

        Ok(payload)
    }

    pub fn parse_block0_payload(payload: &[u8]) -> Result<Option<Self>, ModemError> {
        let nul = payload
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(ModemError::InvalidLength)?;
        if nul == 0 {
            return Ok(None);
        }

        let file_name = String::from_utf8_lossy(&payload[..nul]).to_string();
        validate_ymodem_file_name(&file_name)?;

        let metadata = &payload[nul + 1..];
        let size_end = metadata
            .iter()
            .position(|byte| *byte == b' ' || *byte == 0)
            .unwrap_or(metadata.len());
        let file_size = if size_end == 0 {
            None
        } else {
            String::from_utf8_lossy(&metadata[..size_end]).parse().ok()
        };

        Ok(Some(Self {
            file_name,
            file_size,
        }))
    }
}

fn validate_ymodem_file_name(file_name: &str) -> Result<(), ModemError> {
    let path = Path::new(file_name);
    let is_plain_name = path.file_name().and_then(|name| name.to_str()) == Some(file_name);
    if file_name.is_empty()
        || file_name == "."
        || file_name == ".."
        || file_name.contains(['/', '\\'])
        || path.is_absolute()
        || !is_plain_name
    {
        return Err(ModemError::InvalidFileName);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xmodem_crc_block_round_trips() {
        let block = XyBlock::new(1, XyBlockSize::Bytes128, b"hello").unwrap();
        let encoded = block.encode_crc();
        let parsed = XyBlock::parse_crc(&encoded).unwrap();
        assert_eq!(parsed.number, 1);
        assert_eq!(&parsed.payload[..5], b"hello");
        assert!(parsed.payload[5..].iter().all(|byte| *byte == CPMEOF));
    }

    #[test]
    fn xmodem_block_rejects_bad_number_complement() {
        let block = XyBlock::new(7, XyBlockSize::Bytes128, b"hello").unwrap();
        let mut encoded = block.encode_crc();
        encoded[2] = 7;
        assert_eq!(
            XyBlock::parse_crc(&encoded),
            Err(ModemError::InvalidBlockNumber)
        );
    }

    #[test]
    fn ymodem_header_round_trips() {
        let header = YmodemFileHeader::new("firmware.bin", Some(4096)).unwrap();
        let parsed =
            YmodemFileHeader::parse_block0_payload(&header.encode_block0_payload().unwrap())
                .unwrap()
                .unwrap();
        assert_eq!(parsed.file_name, "firmware.bin");
        assert_eq!(parsed.file_size, Some(4096));
    }

    #[test]
    fn ymodem_header_rejects_paths() {
        assert_eq!(
            YmodemFileHeader::new("../secret", Some(1)),
            Err(ModemError::InvalidFileName)
        );
        assert_eq!(
            YmodemFileHeader::new("/tmp/secret", Some(1)),
            Err(ModemError::InvalidFileName)
        );
    }
}
