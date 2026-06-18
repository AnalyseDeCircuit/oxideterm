// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{Read, Write};
use std::time::Duration;

use crate::error::ModemTransferError;
use crate::io::ModemIo;
use crate::xymodem::{
    ACK, CAN, CPMEOF, EOT, NAK, WANT_CRC, XyBlock, XyBlockSize, YmodemFileHeader,
};

const XYMODEM_TIMEOUT: Duration = Duration::from_secs(10);
const XYMODEM_RETRIES: usize = 10;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum XmodemBlockMode {
    Bytes128,
    Bytes1024,
}

impl XmodemBlockMode {
    const fn block_size(self) -> XyBlockSize {
        match self {
            Self::Bytes128 => XyBlockSize::Bytes128,
            Self::Bytes1024 => XyBlockSize::Bytes1024,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct YmodemSendEntry {
    pub file_name: String,
    pub file_size: u64,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct YmodemSendStreamEntry<R> {
    pub file_name: String,
    pub file_size: u64,
    pub reader: R,
}

pub fn send_xmodem<I: ModemIo, R: Read>(
    io: &mut I,
    reader: &mut R,
    block_mode: XmodemBlockMode,
) -> Result<u64, ModemTransferError> {
    let use_crc = wait_for_xymodem_start(io)?;
    let bytes_sent = send_xymodem_blocks(io, reader, block_mode.block_size(), use_crc)?;
    finish_xymodem_send(io)?;
    Ok(bytes_sent)
}

pub fn receive_xmodem<I: ModemIo, W: Write>(
    io: &mut I,
    writer: &mut W,
    use_crc: bool,
) -> Result<u64, ModemTransferError> {
    io.write_all(&[if use_crc { WANT_CRC } else { NAK }])?;
    receive_xymodem_blocks(io, writer, use_crc, None)
}

pub fn send_ymodem<I: ModemIo>(
    io: &mut I,
    entries: &[YmodemSendEntry],
) -> Result<u64, ModemTransferError> {
    let mut stream_entries = entries
        .iter()
        .map(|entry| YmodemSendStreamEntry {
            file_name: entry.file_name.clone(),
            file_size: entry.file_size,
            reader: entry.bytes.as_slice(),
        })
        .collect::<Vec<_>>();
    send_ymodem_stream(io, &mut stream_entries)
}

pub fn send_ymodem_stream<I, R>(
    io: &mut I,
    entries: &mut [YmodemSendStreamEntry<R>],
) -> Result<u64, ModemTransferError>
where
    I: ModemIo,
    R: Read,
{
    let mut total = 0u64;
    for entry in entries {
        let _ = wait_for_xymodem_start(io)?;
        let header = YmodemFileHeader::new(entry.file_name.clone(), Some(entry.file_size))?;
        send_xymodem_block(
            io,
            0,
            XyBlockSize::Bytes128,
            &header.encode_block0_payload()?,
            true,
        )?;
        expect_byte_or_cancel(io, WANT_CRC)?;
        total += send_xymodem_blocks(io, &mut entry.reader, XyBlockSize::Bytes1024, true)?;
        finish_xymodem_send(io)?;
    }

    let _ = wait_for_xymodem_start(io)?;
    send_xymodem_block(io, 0, XyBlockSize::Bytes128, &[0u8; 128], true)?;
    Ok(total)
}

pub fn receive_ymodem<I, F, W>(
    io: &mut I,
    mut open_writer: F,
) -> Result<Vec<YmodemFileHeader>, ModemTransferError>
where
    I: ModemIo,
    F: FnMut(&YmodemFileHeader) -> Result<W, ModemTransferError>,
    W: Write,
{
    let mut received = Vec::new();
    loop {
        io.write_all(&[WANT_CRC])?;
        let header_block = read_xymodem_block(io, true)?;
        if header_block.number != 0 {
            io.write_all(&[NAK])?;
            continue;
        }
        io.write_all(&[ACK])?;

        let Some(header) = YmodemFileHeader::parse_block0_payload(&header_block.payload)? else {
            break;
        };

        io.write_all(&[WANT_CRC])?;
        let mut writer = open_writer(&header)?;
        let bytes = receive_xymodem_blocks(io, &mut writer, true, header.file_size)?;
        if let Some(size) = header.file_size {
            if bytes < size {
                return Err(ModemTransferError::UnexpectedFrame);
            }
        }
        received.push(header);
    }
    Ok(received)
}

fn send_xymodem_blocks<I: ModemIo, R: Read>(
    io: &mut I,
    reader: &mut R,
    block_size: XyBlockSize,
    use_crc: bool,
) -> Result<u64, ModemTransferError> {
    let mut number = 1u8;
    let mut total = 0u64;
    let mut buffer = vec![CPMEOF; block_size.len()];
    loop {
        buffer.fill(CPMEOF);
        let read = read_padded(reader, &mut buffer)?;
        if read == 0 {
            break;
        }
        send_xymodem_block(io, number, block_size, &buffer, use_crc)?;
        number = number.wrapping_add(1);
        total += read as u64;
    }
    Ok(total)
}

fn receive_xymodem_blocks<I: ModemIo, W: Write>(
    io: &mut I,
    writer: &mut W,
    use_crc: bool,
    expected_size: Option<u64>,
) -> Result<u64, ModemTransferError> {
    let mut expected = 1u8;
    let mut total = 0u64;
    let mut retries = 0usize;
    loop {
        match read_xymodem_packet(io, use_crc) {
            Ok(XymodemPacket::EndOfTransmission) => {
                io.write_all(&[ACK])?;
                return Ok(total);
            }
            Ok(XymodemPacket::Block(block)) => {
                retries = 0;
                if block.number == expected {
                    let write_len = expected_size
                        .map(|size| size.saturating_sub(total).min(block.payload.len() as u64))
                        .unwrap_or(block.payload.len() as u64)
                        as usize;
                    if write_len > 0 {
                        writer.write_all(&block.payload[..write_len])?;
                        total += write_len as u64;
                    }
                    expected = expected.wrapping_add(1);
                } else if block.number != expected.wrapping_sub(1) {
                    io.write_all(&[NAK])?;
                    continue;
                }
                io.write_all(&[ACK])?;
            }
            Err(ModemTransferError::Cancelled) => return Err(ModemTransferError::Cancelled),
            Err(error) if retries < XYMODEM_RETRIES => {
                retries += 1;
                io.write_all(&[NAK])?;
                if matches!(error, ModemTransferError::UnexpectedByte(EOT)) {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    }
}

fn send_xymodem_block<I: ModemIo>(
    io: &mut I,
    number: u8,
    block_size: XyBlockSize,
    payload: &[u8],
    use_crc: bool,
) -> Result<(), ModemTransferError> {
    if !use_crc {
        let block = XyBlock::new(number, block_size, payload)?;
        for _ in 0..XYMODEM_RETRIES {
            io.write_all(&block.encode_checksum())?;
            match io.read_byte(XYMODEM_TIMEOUT)? {
                ACK => return Ok(()),
                NAK | WANT_CRC => continue,
                CAN => {
                    if io.read_byte(XYMODEM_TIMEOUT)? == CAN {
                        return Err(ModemTransferError::Cancelled);
                    }
                }
                byte => return Err(ModemTransferError::UnexpectedByte(byte)),
            }
        }
        return Err(ModemTransferError::Timeout);
    }
    let block = XyBlock::new(number, block_size, payload)?;
    for _ in 0..XYMODEM_RETRIES {
        io.write_all(&block.encode_crc())?;
        match io.read_byte(XYMODEM_TIMEOUT)? {
            ACK => return Ok(()),
            NAK | WANT_CRC => continue,
            CAN => {
                if io.read_byte(XYMODEM_TIMEOUT)? == CAN {
                    return Err(ModemTransferError::Cancelled);
                }
            }
            byte => return Err(ModemTransferError::UnexpectedByte(byte)),
        }
    }
    Err(ModemTransferError::Timeout)
}

fn read_xymodem_block<I: ModemIo>(
    io: &mut I,
    use_crc: bool,
) -> Result<XyBlock, ModemTransferError> {
    let marker = io.read_byte(XYMODEM_TIMEOUT)?;
    let size =
        XyBlockSize::from_marker(marker).ok_or(ModemTransferError::UnexpectedByte(marker))?;
    let mut raw = vec![marker];
    let check_len = if use_crc { 2 } else { 1 };
    for _ in 0..(2 + size.len() + check_len) {
        raw.push(io.read_byte(XYMODEM_TIMEOUT)?);
    }
    if use_crc {
        Ok(XyBlock::parse_crc(&raw)?)
    } else {
        Ok(XyBlock::parse_checksum(&raw)?)
    }
}

enum XymodemPacket {
    Block(XyBlock),
    EndOfTransmission,
}

fn read_xymodem_packet<I: ModemIo>(
    io: &mut I,
    use_crc: bool,
) -> Result<XymodemPacket, ModemTransferError> {
    let marker = io.read_byte(XYMODEM_TIMEOUT)?;
    match marker {
        EOT => Ok(XymodemPacket::EndOfTransmission),
        CAN => {
            if io.read_byte(XYMODEM_TIMEOUT)? == CAN {
                Err(ModemTransferError::Cancelled)
            } else {
                Err(ModemTransferError::UnexpectedByte(CAN))
            }
        }
        marker => {
            let size = XyBlockSize::from_marker(marker)
                .ok_or(ModemTransferError::UnexpectedByte(marker))?;
            let mut raw = vec![marker];
            let check_len = if use_crc { 2 } else { 1 };
            for _ in 0..(2 + size.len() + check_len) {
                raw.push(io.read_byte(XYMODEM_TIMEOUT)?);
            }
            let block = if use_crc {
                XyBlock::parse_crc(&raw)?
            } else {
                XyBlock::parse_checksum(&raw)?
            };
            Ok(XymodemPacket::Block(block))
        }
    }
}

fn wait_for_xymodem_start<I: ModemIo>(io: &mut I) -> Result<bool, ModemTransferError> {
    for _ in 0..XYMODEM_RETRIES {
        match io.read_byte(XYMODEM_TIMEOUT)? {
            WANT_CRC => return Ok(true),
            NAK => return Ok(false),
            CAN => {
                if io.read_byte(XYMODEM_TIMEOUT)? == CAN {
                    return Err(ModemTransferError::Cancelled);
                }
            }
            _ => {}
        }
    }
    Err(ModemTransferError::Timeout)
}

fn finish_xymodem_send<I: ModemIo>(io: &mut I) -> Result<(), ModemTransferError> {
    for _ in 0..XYMODEM_RETRIES {
        io.write_all(&[EOT])?;
        match io.read_byte(XYMODEM_TIMEOUT)? {
            ACK => return Ok(()),
            NAK => continue,
            CAN => {
                if io.read_byte(XYMODEM_TIMEOUT)? == CAN {
                    return Err(ModemTransferError::Cancelled);
                }
            }
            byte => return Err(ModemTransferError::UnexpectedByte(byte)),
        }
    }
    Err(ModemTransferError::Timeout)
}

fn expect_byte_or_cancel<I: ModemIo>(io: &mut I, expected: u8) -> Result<(), ModemTransferError> {
    match io.read_byte(XYMODEM_TIMEOUT)? {
        byte if byte == expected => Ok(()),
        CAN => {
            if io.read_byte(XYMODEM_TIMEOUT)? == CAN {
                Err(ModemTransferError::Cancelled)
            } else {
                Err(ModemTransferError::UnexpectedByte(CAN))
            }
        }
        byte => Err(ModemTransferError::UnexpectedByte(byte)),
    }
}

fn read_padded<R: Read>(reader: &mut R, buffer: &mut [u8]) -> Result<usize, ModemTransferError> {
    let mut total = 0usize;
    while total < buffer.len() {
        let read = reader.read(&mut buffer[total..])?;
        if read == 0 {
            break;
        }
        total += read;
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MemoryModemIo;

    #[test]
    fn xmodem_send_emits_crc_blocks_and_eot() {
        let mut input = Vec::new();
        input.push(WANT_CRC);
        input.extend([ACK, ACK]);
        let mut io = MemoryModemIo::with_input(input);
        let sent = send_xmodem(&mut io, &mut b"hello".as_slice(), XmodemBlockMode::Bytes128)
            .expect("xmodem send");
        let output = io.take_output();
        assert_eq!(sent, 5);
        assert_eq!(output[0], crate::xymodem::SOH);
        assert_eq!(output[1], 1);
        assert_eq!(*output.last().unwrap(), EOT);
    }

    #[test]
    fn xmodem_send_supports_checksum_negotiation() {
        let mut input = Vec::new();
        input.push(NAK);
        input.extend([ACK, ACK]);
        let mut io = MemoryModemIo::with_input(input);
        let sent = send_xmodem(&mut io, &mut b"hello".as_slice(), XmodemBlockMode::Bytes128)
            .expect("xmodem send");
        let output = io.take_output();
        let block_len = 3 + XyBlockSize::Bytes128.len() + 1;
        let block = XyBlock::parse_checksum(&output[..block_len]).expect("checksum block");
        assert_eq!(sent, 5);
        assert_eq!(block.number, 1);
        assert_eq!(&block.payload[..5], b"hello");
        assert_eq!(output[block_len], EOT);
    }

    #[test]
    fn ymodem_receive_truncates_cpm_padding_to_file_size() {
        #[derive(Clone)]
        struct SharedWriter(std::rc::Rc<std::cell::RefCell<Vec<u8>>>);

        impl std::io::Write for SharedWriter {
            fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
                self.0.borrow_mut().extend_from_slice(buffer);
                Ok(buffer.len())
            }

            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }

        let header = YmodemFileHeader::new("hello.txt", Some(5)).unwrap();
        let mut input = XyBlock::new(
            0,
            XyBlockSize::Bytes128,
            &header.encode_block0_payload().unwrap(),
        )
        .unwrap()
        .encode_crc();
        input.extend(
            XyBlock::new(1, XyBlockSize::Bytes128, b"hello")
                .unwrap()
                .encode_crc(),
        );
        input.push(EOT);
        input.extend(
            XyBlock::new(0, XyBlockSize::Bytes128, &[0u8; 128])
                .unwrap()
                .encode_crc(),
        );

        let mut io = MemoryModemIo::with_input(input);
        let output = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let writer = SharedWriter(output.clone());
        let received = receive_ymodem(&mut io, |_header| {
            Ok::<SharedWriter, ModemTransferError>(writer.clone())
        })
        .expect("ymodem receive");

        assert_eq!(received.len(), 1);
        assert_eq!(&*output.borrow(), b"hello");
    }

    #[test]
    fn ymodem_send_starts_with_block_zero() {
        let mut input = Vec::new();
        input.extend([WANT_CRC, ACK, WANT_CRC, ACK, ACK, WANT_CRC, ACK]);
        let mut io = MemoryModemIo::with_input(input);
        let entry = YmodemSendEntry {
            file_name: "hello.txt".to_string(),
            file_size: 5,
            bytes: b"hello".to_vec(),
        };
        let sent = send_ymodem(&mut io, &[entry]).expect("ymodem send");
        let output = io.take_output();
        assert_eq!(sent, 5);
        assert_eq!(output[0], crate::xymodem::SOH);
        assert_eq!(output[1], 0);
        assert!(output.windows("hello.txt".len()).any(|w| w == b"hello.txt"));
    }
}
