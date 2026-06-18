// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

/// Computes the CRC-16/XMODEM value used by X/YMODEM and ZMODEM hex frames.
pub fn crc16_xmodem(bytes: &[u8]) -> u16 {
    bytes
        .iter()
        .fold(0u16, |crc, byte| crc16_xmodem_update(crc, *byte))
}

/// Updates a CRC-16/XMODEM accumulator with one byte.
pub fn crc16_xmodem_update(mut crc: u16, byte: u8) -> u16 {
    crc ^= (byte as u16) << 8;
    for _ in 0..8 {
        crc = if crc & 0x8000 != 0 {
            (crc << 1) ^ 0x1021
        } else {
            crc << 1
        };
    }
    crc
}

/// Computes the finalized IEEE CRC-32 used by ZMODEM CRC-32 frames.
pub fn crc32_ieee(bytes: &[u8]) -> u32 {
    !bytes
        .iter()
        .fold(0xffff_ffffu32, |crc, byte| crc32_ieee_update(crc, *byte))
}

/// Updates an unfinalized IEEE CRC-32 accumulator with one byte.
pub fn crc32_ieee_update(mut crc: u32, byte: u8) -> u32 {
    crc ^= byte as u32;
    for _ in 0..8 {
        crc = if crc & 1 != 0 {
            (crc >> 1) ^ 0xedb8_8320
        } else {
            crc >> 1
        };
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_matches_xmodem_check_value() {
        assert_eq!(crc16_xmodem(b"123456789"), 0x31c3);
    }

    #[test]
    fn crc32_matches_ieee_check_value() {
        assert_eq!(crc32_ieee(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn crc16_update_matches_bulk() {
        let crc = b"hello"
            .iter()
            .fold(0u16, |crc, byte| crc16_xmodem_update(crc, *byte));
        assert_eq!(crc, crc16_xmodem(b"hello"));
    }
}
