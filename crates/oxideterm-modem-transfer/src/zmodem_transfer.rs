// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::time::Duration;

use crate::crc::{crc16_xmodem_update, crc32_ieee_update};
use crate::error::{ModemError, ModemTransferError};
use crate::io::{MemoryModemIo, ModemIo};
use crate::zmodem::{
    XON, ZBIN, ZBIN32, ZDLE, ZDataEnd, ZFrameType, ZHeader, ZHeaderEncoding, ZPAD, ZRUB0, ZRUB1,
    encode_bin16_header_with_escape, encode_bin32_header_with_escape, encode_hex_header,
    position_header, push_zdle_escaped_with_control,
};

const ZMODEM_TIMEOUT: Duration = Duration::from_secs(10);
const ZMODEM_MAX_CHUNK: usize = 8192;
const ZMODEM_MAX_POSITION: u64 = u32::MAX as u64;
const ZMODEM_CAN_FULL_DUPLEX: u8 = 0x01;
const ZMODEM_CAN_OVERLAPPED_IO: u8 = 0x02;
const ZMODEM_CAN_CRC32: u8 = 0x20;
const ZMODEM_ESCAPE_CONTROL: u8 = 0x40;
const ZMODEM_BINARY_FILE_CONVERSION: u8 = 0x01;
const ZMODEM_ATTENTION_MAX_BYTES: usize = 32;
const ZFILE_BINARY_FLAGS: [u8; 4] = [0, 0, 0, ZMODEM_BINARY_FILE_CONVERSION];
const ZRINIT_FLAGS: [u8; 4] = [
    (ZMODEM_MAX_CHUNK & 0xff) as u8,
    (ZMODEM_MAX_CHUNK >> 8) as u8,
    0,
    ZMODEM_CAN_FULL_DUPLEX | ZMODEM_CAN_OVERLAPPED_IO | ZMODEM_CAN_CRC32 | ZMODEM_ESCAPE_CONTROL,
];

#[derive(Clone, Copy, Debug)]
struct ZmodemPeerCapabilities {
    chunk_size: usize,
    use_crc32: bool,
    escape_control: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZmodemFileHeader {
    pub file_name: String,
    pub file_size: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ZmodemSendEntry {
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct ZmodemSendStreamEntry<R> {
    pub file_name: String,
    pub file_size: u64,
    pub reader: R,
}

pub fn receive_zmodem<I, F, W>(
    io: &mut I,
    mut open_writer: F,
) -> Result<Vec<ZmodemFileHeader>, ModemTransferError>
where
    I: ModemIo,
    F: FnMut(&ZmodemFileHeader) -> Result<W, ModemTransferError>,
    W: Write,
{
    let mut received = Vec::new();
    send_zrinit(io)?;

    loop {
        let header = read_zmodem_header(io)?;
        match header.frame_type {
            ZFrameType::ZrqInit => {
                send_zrinit(io)?;
            }
            ZFrameType::ZFile => {
                let use_crc32 = header.encoding == ZHeaderEncoding::Bin32;
                let file_header_frame = read_zmodem_data(io, use_crc32, ZMODEM_MAX_CHUNK)?;
                if file_header_frame.end != ZDataEnd::EndWithAck {
                    return Err(ModemTransferError::UnexpectedFrame);
                }
                let file_header_data = file_header_frame.payload;
                let Some(file_header) = parse_zfile_header(&file_header_data)? else {
                    finish_zmodem_receive(io)?;
                    return Ok(received);
                };
                if let Some(file_size) = file_header.file_size {
                    validate_zmodem_file_size(file_size)?;
                }

                let mut writer = open_writer(&file_header)?;
                let mut received_offset = 0u64;
                write_zheader(
                    io,
                    ZFrameType::ZRpos,
                    position_header_checked(received_offset)?,
                )?;

                loop {
                    let data_header = read_zmodem_header(io)?;
                    match data_header.frame_type {
                        ZFrameType::ZData => {
                            if data_header.position_u32() as u64 != received_offset {
                                write_zheader(
                                    io,
                                    ZFrameType::ZRpos,
                                    position_header_checked(received_offset)?,
                                )?;
                                continue;
                            }
                            let data_uses_crc32 = data_header.encoding == ZHeaderEncoding::Bin32;
                            loop {
                                let frame =
                                    match read_zmodem_data(io, data_uses_crc32, ZMODEM_MAX_CHUNK) {
                                        Ok(frame) => frame,
                                        Err(ModemTransferError::Protocol(
                                            ModemError::InvalidCrc,
                                        )) => {
                                            write_zheader(
                                                io,
                                                ZFrameType::ZRpos,
                                                position_header_checked(received_offset)?,
                                            )?;
                                            break;
                                        }
                                        Err(error) => return Err(error),
                                    };
                                let next_offset = received_offset
                                    .checked_add(frame.payload.len() as u64)
                                    .ok_or(ModemTransferError::UnexpectedFrame)?;
                                if file_header
                                    .file_size
                                    .is_some_and(|file_size| next_offset > file_size)
                                {
                                    return Err(ModemTransferError::UnexpectedFrame);
                                }
                                writer.write_all(&frame.payload)?;
                                received_offset = next_offset;
                                match frame.end {
                                    ZDataEnd::Continue => {}
                                    ZDataEnd::ContinueWithAck => write_zheader(
                                        io,
                                        ZFrameType::ZAck,
                                        position_header_checked(received_offset)?,
                                    )?,
                                    ZDataEnd::End => break,
                                    ZDataEnd::EndWithAck => {
                                        write_zheader(
                                            io,
                                            ZFrameType::ZAck,
                                            position_header_checked(received_offset)?,
                                        )?;
                                        break;
                                    }
                                }
                            }
                        }
                        ZFrameType::ZEof => {
                            let declared_size_matches = file_header
                                .file_size
                                .is_none_or(|file_size| file_size == received_offset);
                            if data_header.position_u32() as u64 != received_offset
                                || !declared_size_matches
                            {
                                write_zheader(
                                    io,
                                    ZFrameType::ZRpos,
                                    position_header_checked(received_offset)?,
                                )?;
                                continue;
                            }
                            writer.flush()?;
                            send_zrinit(io)?;
                            received.push(file_header);
                            break;
                        }
                        ZFrameType::ZFin => {
                            return Err(ModemTransferError::UnexpectedFrame);
                        }
                        ZFrameType::ZAbort | ZFrameType::ZCan => {
                            return Err(ModemTransferError::Cancelled);
                        }
                        _ => return Err(ModemTransferError::UnexpectedFrame),
                    }
                }
            }
            ZFrameType::ZsInit => {
                let use_crc32 = header.encoding == ZHeaderEncoding::Bin32;
                let frame = read_zmodem_data(io, use_crc32, ZMODEM_ATTENTION_MAX_BYTES)?;
                if frame.end != ZDataEnd::EndWithAck {
                    return Err(ModemTransferError::UnexpectedFrame);
                }
                write_zheader(io, ZFrameType::ZAck, position_header(1))?;
            }
            ZFrameType::ZFin => {
                finish_zmodem_receive(io)?;
                return Ok(received);
            }
            ZFrameType::ZAbort | ZFrameType::ZCan => return Err(ModemTransferError::Cancelled),
            _ => {}
        }
    }
}

pub fn send_zmodem<I: ModemIo>(
    io: &mut I,
    entries: &[ZmodemSendEntry],
) -> Result<u64, ModemTransferError> {
    let mut stream_entries = entries
        .iter()
        .map(|entry| ZmodemSendStreamEntry {
            file_name: entry.file_name.clone(),
            file_size: entry.bytes.len() as u64,
            reader: Cursor::new(entry.bytes.as_slice()),
        })
        .collect::<Vec<_>>();
    send_zmodem_stream(io, &mut stream_entries)
}

pub fn send_zmodem_stream<I, R>(
    io: &mut I,
    entries: &mut [ZmodemSendStreamEntry<R>],
) -> Result<u64, ModemTransferError>
where
    I: ModemIo,
    R: Read + Seek,
{
    for entry in entries.iter() {
        validate_zmodem_file_size(entry.file_size)?;
    }

    let peer = wait_for_zrinit(io)?;
    let mut total = 0u64;

    for entry in entries {
        let header_payload = build_zfile_header(&entry.file_name, entry.file_size);
        write_zbinary_header(
            io,
            ZFrameType::ZFile,
            ZFILE_BINARY_FLAGS,
            peer.use_crc32,
            peer.escape_control,
        )?;
        write_zdata(
            io,
            &header_payload,
            ZDataEnd::EndWithAck,
            peer.use_crc32,
            peer.escape_control,
        )?;

        loop {
            let response = read_zmodem_header(io)?;
            match response.frame_type {
                ZFrameType::ZRpos => {
                    let mut offset = response.position_u32() as u64;
                    if offset > entry.file_size {
                        return Err(ModemTransferError::UnexpectedFrame);
                    }
                    loop {
                        match send_zfile_data_stream(
                            io,
                            &mut entry.reader,
                            entry.file_size,
                            offset,
                            peer,
                        )? {
                            ZmodemSendDataResult::Complete => {}
                            ZmodemSendDataResult::ResumeAt(next_offset) => {
                                if next_offset > entry.file_size {
                                    return Err(ModemTransferError::UnexpectedFrame);
                                }
                                offset = next_offset;
                                continue;
                            }
                            ZmodemSendDataResult::Skip => break,
                        }
                        let followup = read_zmodem_header(io)?;
                        match followup.frame_type {
                            ZFrameType::ZrInit => {
                                total += entry.file_size;
                                break;
                            }
                            ZFrameType::ZRpos => {
                                let next_offset = followup.position_u32() as u64;
                                if next_offset > entry.file_size {
                                    return Err(ModemTransferError::UnexpectedFrame);
                                }
                                offset = next_offset;
                                continue;
                            }
                            ZFrameType::ZSkip => break,
                            ZFrameType::ZAbort | ZFrameType::ZCan => {
                                return Err(ModemTransferError::Cancelled);
                            }
                            _ => return Err(ModemTransferError::UnexpectedFrame),
                        }
                    }
                    break;
                }
                ZFrameType::ZSkip => break,
                ZFrameType::ZAbort | ZFrameType::ZCan => {
                    return Err(ModemTransferError::Cancelled);
                }
                _ => {}
            }
        }
    }

    finish_zmodem_send(io)?;
    Ok(total)
}

pub fn parse_zmodem_header_prefix(bytes: &[u8]) -> Result<Option<ZHeader>, ModemError> {
    let mut io = MemoryModemIo::with_input(bytes.to_vec());
    match read_zmodem_header(&mut io) {
        Ok(header) => Ok(Some(header)),
        Err(ModemTransferError::Timeout) => Ok(None),
        Err(ModemTransferError::Protocol(error)) => Err(error),
        Err(_) => Err(ModemError::InvalidMarker),
    }
}

#[derive(Debug)]
struct ZDataFrame {
    payload: Vec<u8>,
    end: ZDataEnd,
}

fn wait_for_zrinit<I: ModemIo>(io: &mut I) -> Result<ZmodemPeerCapabilities, ModemTransferError> {
    loop {
        let header = read_zmodem_header(io)?;
        match header.frame_type {
            ZFrameType::ZrInit => {
                let advertised_chunk =
                    u16::from_le_bytes([header.position[0], header.position[1]]) as usize;
                let chunk_size = if advertised_chunk < 32 {
                    1024
                } else {
                    advertised_chunk.min(ZMODEM_MAX_CHUNK)
                };
                let flags = header.position[3];
                return Ok(ZmodemPeerCapabilities {
                    chunk_size,
                    use_crc32: flags & ZMODEM_CAN_CRC32 != 0,
                    escape_control: flags & ZMODEM_ESCAPE_CONTROL != 0,
                });
            }
            ZFrameType::ZrqInit => write_zheader(io, ZFrameType::ZrInit, ZRINIT_FLAGS)?,
            ZFrameType::ZAbort | ZFrameType::ZCan => return Err(ModemTransferError::Cancelled),
            _ => {}
        }
    }
}

enum ZmodemSendDataResult {
    Complete,
    ResumeAt(u64),
    Skip,
}

fn send_zfile_data_stream<I, R>(
    io: &mut I,
    reader: &mut R,
    file_size: u64,
    offset: u64,
    peer: ZmodemPeerCapabilities,
) -> Result<ZmodemSendDataResult, ModemTransferError>
where
    I: ModemIo,
    R: Read + Seek,
{
    let start = offset.min(file_size);
    reader.seek(SeekFrom::Start(start))?;
    write_zbinary_header(
        io,
        ZFrameType::ZData,
        position_header_checked(start)?,
        peer.use_crc32,
        peer.escape_control,
    )?;
    let mut buffer = vec![0u8; peer.chunk_size];
    let mut sent_offset = start;
    while sent_offset < file_size {
        let remaining = (file_size - sent_offset).min(buffer.len() as u64) as usize;
        let read = read_padded_exact(reader, &mut buffer[..remaining])?;
        if read == 0 {
            return Err(ModemTransferError::UnexpectedFrame);
        }
        sent_offset = sent_offset.saturating_add(read as u64);
        let frame_end = if sent_offset < file_size {
            ZDataEnd::ContinueWithAck
        } else {
            ZDataEnd::End
        };
        write_zdata(
            io,
            &buffer[..read],
            frame_end,
            peer.use_crc32,
            peer.escape_control,
        )?;
        if frame_end == ZDataEnd::ContinueWithAck {
            let response = read_zmodem_header(io)?;
            match response.frame_type {
                ZFrameType::ZAck if response.position_u32() as u64 == sent_offset => {}
                ZFrameType::ZRpos => {
                    return Ok(ZmodemSendDataResult::ResumeAt(
                        response.position_u32() as u64
                    ));
                }
                ZFrameType::ZSkip => return Ok(ZmodemSendDataResult::Skip),
                ZFrameType::ZAbort | ZFrameType::ZCan => {
                    return Err(ModemTransferError::Cancelled);
                }
                _ => return Err(ModemTransferError::UnexpectedFrame),
            }
        }
    }
    if start == file_size {
        write_zdata(io, &[], ZDataEnd::End, peer.use_crc32, peer.escape_control)?;
    }
    write_zheader(io, ZFrameType::ZEof, position_header_checked(file_size)?)?;
    Ok(ZmodemSendDataResult::Complete)
}

fn read_padded_exact<R: Read>(
    reader: &mut R,
    buffer: &mut [u8],
) -> Result<usize, ModemTransferError> {
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

fn validate_zmodem_file_size(file_size: u64) -> Result<(), ModemTransferError> {
    // Classic ZMODEM frame positions are 32-bit; fail explicitly instead of wrapping offsets.
    if file_size > ZMODEM_MAX_POSITION {
        return Err(ModemTransferError::UnsupportedFileSize(file_size));
    }
    Ok(())
}

fn position_header_checked(position: u64) -> Result<[u8; 4], ModemTransferError> {
    if position > ZMODEM_MAX_POSITION {
        return Err(ModemTransferError::UnsupportedFileSize(position));
    }
    Ok(position_header(position as u32))
}

fn send_zrinit<I: ModemIo>(io: &mut I) -> Result<(), ModemTransferError> {
    // Send one response per negotiation event. Back-to-back duplicates remain
    // queued as stale replies after lrzsz has already advanced to ZFILE/ZSINIT.
    write_zheader(io, ZFrameType::ZrInit, ZRINIT_FLAGS)
}

fn finish_zmodem_receive<I: ModemIo>(io: &mut I) -> Result<(), ModemTransferError> {
    write_zheader(io, ZFrameType::ZFin, position_header(0))?;
    let mut consecutive_o = 0usize;
    let mut consecutive_cancel = 0usize;
    loop {
        let byte = io.read_byte(ZMODEM_TIMEOUT)?;
        consecutive_o = if byte == b'O' { consecutive_o + 1 } else { 0 };
        if consecutive_o == 2 {
            return Ok(());
        }
        consecutive_cancel = if byte == ZDLE {
            consecutive_cancel + 1
        } else {
            0
        };
        if consecutive_cancel == 5 {
            return Err(ModemTransferError::Cancelled);
        }
    }
}

fn finish_zmodem_send<I: ModemIo>(io: &mut I) -> Result<(), ModemTransferError> {
    for _ in 0..3 {
        write_zheader(io, ZFrameType::ZFin, position_header(0))?;
        match read_zmodem_header(io)? {
            header if header.frame_type == ZFrameType::ZFin => return io.write_all(b"OO"),
            header
                if matches!(
                    header.frame_type,
                    ZFrameType::ZAbort | ZFrameType::ZCan | ZFrameType::ZFerr
                ) =>
            {
                return Err(ModemTransferError::Cancelled);
            }
            _ => continue,
        }
    }
    Err(ModemTransferError::Timeout)
}

fn write_zheader<I: ModemIo>(
    io: &mut I,
    frame_type: ZFrameType,
    position: [u8; 4],
) -> Result<(), ModemTransferError> {
    let include_xon = !matches!(frame_type, ZFrameType::ZFin | ZFrameType::ZAck);
    io.write_all(&encode_hex_header(frame_type, position, include_xon))
}

fn write_zbinary_header<I: ModemIo>(
    io: &mut I,
    frame_type: ZFrameType,
    position: [u8; 4],
    use_crc32: bool,
    escape_control: bool,
) -> Result<(), ModemTransferError> {
    let header = if use_crc32 {
        encode_bin32_header_with_escape(frame_type, position, escape_control)
    } else {
        encode_bin16_header_with_escape(frame_type, position, escape_control)
    };
    io.write_all(&header)
}

fn write_zdata<I: ModemIo>(
    io: &mut I,
    payload: &[u8],
    end: ZDataEnd,
    use_crc32: bool,
    escape_control: bool,
) -> Result<(), ModemTransferError> {
    let mut out = Vec::with_capacity(payload.len() * 2 + 8);
    if use_crc32 {
        let mut crc = 0xffff_ffffu32;
        for byte in payload {
            push_zdle_escaped_with_control(&mut out, *byte, escape_control);
            crc = crc32_ieee_update(crc, *byte);
        }
        out.extend_from_slice(&[ZDLE, end.marker()]);
        crc = crc32_ieee_update(crc, end.marker());
        crc = !crc;
        for byte in crc.to_le_bytes() {
            push_zdle_escaped_with_control(&mut out, byte, escape_control);
        }
    } else {
        let mut crc = 0u16;
        for byte in payload {
            push_zdle_escaped_with_control(&mut out, *byte, escape_control);
            crc = crc16_xmodem_update(crc, *byte);
        }
        out.extend_from_slice(&[ZDLE, end.marker()]);
        crc = crc16_xmodem_update(crc, end.marker());
        push_zdle_escaped_with_control(&mut out, (crc >> 8) as u8, escape_control);
        push_zdle_escaped_with_control(&mut out, crc as u8, escape_control);
    }
    if end == ZDataEnd::EndWithAck {
        // lrzsz emits XON after ZCRCW to release software flow control before
        // it waits for the peer's acknowledgment.
        out.push(XON);
    }
    io.write_all(&out)
}

fn read_zmodem_header<I: ModemIo>(io: &mut I) -> Result<ZHeader, ModemTransferError> {
    loop {
        if io.read_byte(ZMODEM_TIMEOUT)? != ZPAD {
            continue;
        }
        let next = io.read_byte(ZMODEM_TIMEOUT)?;
        let encoding = if next == ZPAD {
            if io.read_byte(ZMODEM_TIMEOUT)? != ZDLE {
                continue;
            }
            io.read_byte(ZMODEM_TIMEOUT)?
        } else if next == ZDLE {
            io.read_byte(ZMODEM_TIMEOUT)?
        } else {
            continue;
        };

        return match encoding {
            crate::zmodem::ZHEX => read_hex_zheader(io),
            ZBIN => read_binary_zheader(io, ZHeaderEncoding::Bin16),
            ZBIN32 => read_binary_zheader(io, ZHeaderEncoding::Bin32),
            byte => Err(ModemTransferError::UnexpectedByte(byte)),
        };
    }
}

fn read_hex_zheader<I: ModemIo>(io: &mut I) -> Result<ZHeader, ModemTransferError> {
    let mut decoded = [0u8; 7];
    for slot in &mut decoded {
        let high = read_hex_nibble(io)?;
        let low = read_hex_nibble(io)?;
        *slot = (high << 4) | low;
    }

    let expected_crc = crate::crc::crc16_xmodem(&decoded[..5]);
    let received_crc = u16::from_be_bytes([decoded[5], decoded[6]]);
    if expected_crc != received_crc {
        return Err(ModemTransferError::Protocol(ModemError::InvalidCrc));
    }
    consume_hex_header_line_end(io)?;
    let frame_type = ZFrameType::from_byte(decoded[0]).ok_or(ModemError::InvalidFrameType)?;
    Ok(ZHeader::new(
        frame_type,
        [decoded[1], decoded[2], decoded[3], decoded[4]],
        ZHeaderEncoding::Hex,
    ))
}

fn read_binary_zheader<I: ModemIo>(
    io: &mut I,
    encoding: ZHeaderEncoding,
) -> Result<ZHeader, ModemTransferError> {
    let crc_len = if encoding == ZHeaderEncoding::Bin32 {
        4
    } else {
        2
    };
    let mut decoded = Vec::with_capacity(5 + crc_len);
    for _ in 0..5 + crc_len {
        decoded.push(read_zescaped_byte(io)?);
    }

    match encoding {
        ZHeaderEncoding::Bin16 => {
            let mut crc = 0u16;
            for byte in &decoded[..7] {
                crc = crc16_xmodem_update(crc, *byte);
            }
            if crc != 0 {
                return Err(ModemTransferError::Protocol(ModemError::InvalidCrc));
            }
        }
        ZHeaderEncoding::Bin32 => {
            let expected = !decoded[..5]
                .iter()
                .fold(0xffff_ffffu32, |crc, byte| crc32_ieee_update(crc, *byte));
            let received = u32::from_le_bytes([decoded[5], decoded[6], decoded[7], decoded[8]]);
            if expected != received {
                return Err(ModemTransferError::Protocol(ModemError::InvalidCrc));
            }
        }
        ZHeaderEncoding::Hex => return Err(ModemTransferError::UnexpectedFrame),
    }

    let frame_type = ZFrameType::from_byte(decoded[0]).ok_or(ModemError::InvalidFrameType)?;
    Ok(ZHeader::new(
        frame_type,
        [decoded[1], decoded[2], decoded[3], decoded[4]],
        encoding,
    ))
}

fn read_zmodem_data<I: ModemIo>(
    io: &mut I,
    use_crc32: bool,
    max_payload: usize,
) -> Result<ZDataFrame, ModemTransferError> {
    let mut payload = Vec::new();
    let end = loop {
        let byte = io.read_byte(ZMODEM_TIMEOUT)?;
        if matches!(byte, XON | 0x13 | 0x91 | 0x93) {
            continue;
        }
        if byte != ZDLE {
            if payload.len() >= max_payload {
                return Err(ModemTransferError::Protocol(ModemError::InvalidLength));
            }
            payload.push(byte);
            continue;
        }
        let escaped = io.read_byte(ZMODEM_TIMEOUT)?;
        if let Some(end) = ZDataEnd::from_marker(escaped) {
            break end;
        }
        if payload.len() >= max_payload {
            return Err(ModemTransferError::Protocol(ModemError::InvalidLength));
        }
        payload.push(decode_zescaped_followup(io, escaped)?);
    };

    if use_crc32 {
        let mut crc_bytes = [0u8; 4];
        for byte in &mut crc_bytes {
            *byte = read_zescaped_byte(io)?;
        }
        let mut crc = 0xffff_ffffu32;
        for byte in &payload {
            crc = crc32_ieee_update(crc, *byte);
        }
        crc = crc32_ieee_update(crc, end.marker());
        let expected = !crc;
        let received = u32::from_le_bytes(crc_bytes);
        if expected != received {
            return Err(ModemTransferError::Protocol(ModemError::InvalidCrc));
        }
    } else {
        let high = read_zescaped_byte(io)?;
        let low = read_zescaped_byte(io)?;
        let mut crc = 0u16;
        for byte in &payload {
            crc = crc16_xmodem_update(crc, *byte);
        }
        crc = crc16_xmodem_update(crc, end.marker());
        crc = crc16_xmodem_update(crc, high);
        crc = crc16_xmodem_update(crc, low);
        if crc != 0 {
            return Err(ModemTransferError::Protocol(ModemError::InvalidCrc));
        }
    }

    Ok(ZDataFrame { payload, end })
}

fn consume_hex_header_line_end<I: ModemIo>(io: &mut I) -> Result<(), ModemTransferError> {
    // Hex ZMODEM headers end with CR/LF before optional XON or frame data.
    let carriage_return = io.read_byte(ZMODEM_TIMEOUT)?;
    if carriage_return != b'\r' {
        return Err(ModemTransferError::UnexpectedByte(carriage_return));
    }
    let line_feed = io.read_byte(ZMODEM_TIMEOUT)?;
    if line_feed & 0x7f != b'\n' {
        return Err(ModemTransferError::UnexpectedByte(line_feed));
    }
    Ok(())
}

fn read_zescaped_byte<I: ModemIo>(io: &mut I) -> Result<u8, ModemTransferError> {
    loop {
        let byte = io.read_byte(ZMODEM_TIMEOUT)?;
        if matches!(byte, XON | 0x13 | 0x91 | 0x93) {
            continue;
        }
        if byte == ZDLE {
            let escaped = io.read_byte(ZMODEM_TIMEOUT)?;
            return decode_zescaped_followup(io, escaped);
        }
        return Ok(byte);
    }
}

fn decode_zescaped_followup<I: ModemIo>(io: &mut I, escaped: u8) -> Result<u8, ModemTransferError> {
    match escaped {
        ZRUB0 => Ok(0x7f),
        ZRUB1 => Ok(0xff),
        byte if byte & 0x60 == 0x40 => Ok(byte ^ 0x40),
        ZDLE => {
            for _ in 0..3 {
                if io.read_byte(ZMODEM_TIMEOUT)? != ZDLE {
                    return Err(ModemTransferError::Protocol(ModemError::InvalidEscape));
                }
            }
            Err(ModemTransferError::Cancelled)
        }
        _ => Err(ModemTransferError::Protocol(ModemError::InvalidEscape)),
    }
}

fn read_hex_nibble<I: ModemIo>(io: &mut I) -> Result<u8, ModemTransferError> {
    match io.read_byte(ZMODEM_TIMEOUT)? {
        byte @ b'0'..=b'9' => Ok(byte - b'0'),
        byte @ b'a'..=b'f' => Ok(byte - b'a' + 10),
        byte @ b'A'..=b'F' => Ok(byte - b'A' + 10),
        byte => Err(ModemTransferError::UnexpectedByte(byte)),
    }
}

fn parse_zfile_header(bytes: &[u8]) -> Result<Option<ZmodemFileHeader>, ModemTransferError> {
    let nul = bytes
        .iter()
        .position(|byte| *byte == 0)
        .ok_or(ModemError::InvalidLength)?;
    if nul == 0 {
        return Ok(None);
    }
    let file_name = String::from_utf8_lossy(&bytes[..nul]).to_string();
    if file_name.contains(['/', '\\']) || file_name == "." || file_name == ".." {
        return Err(ModemTransferError::Protocol(ModemError::InvalidFileName));
    }
    let metadata = String::from_utf8_lossy(&bytes[nul + 1..]);
    let file_size = metadata
        .split_whitespace()
        .next()
        .map(str::parse)
        .transpose()
        .map_err(|_| ModemTransferError::Protocol(ModemError::InvalidLength))?;
    Ok(Some(ZmodemFileHeader {
        file_name,
        file_size,
    }))
}

fn build_zfile_header(file_name: &str, file_size: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(file_name.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(file_size.to_string().as_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::MemoryModemIo;

    #[test]
    fn zmodem_receive_responds_to_zrqinit() {
        let mut input = encode_hex_header(ZFrameType::ZrqInit, position_header(0), true);
        input.extend(encode_hex_header(
            ZFrameType::ZFin,
            position_header(0),
            false,
        ));
        input.extend_from_slice(b"OO");
        let mut io = MemoryModemIo::with_input(input);
        let received = receive_zmodem(&mut io, |_header| {
            Ok::<Vec<u8>, ModemTransferError>(Vec::new())
        })
        .expect("zmodem receive");
        let output = io.take_output();
        assert!(received.is_empty());
        assert!(output.starts_with(&encode_hex_header(ZFrameType::ZrInit, ZRINIT_FLAGS, true)));
        assert!(!output.ends_with(b"OO"));
    }

    #[test]
    fn zmodem_send_waits_for_zrinit_and_emits_zfile() {
        let input = encode_hex_header(ZFrameType::ZrInit, ZRINIT_FLAGS, true);
        let mut io = MemoryModemIo::with_input(input);
        let result = send_zmodem(
            &mut io,
            &[ZmodemSendEntry {
                file_name: "hello.txt".to_string(),
                bytes: b"hello".to_vec(),
            }],
        );
        assert!(matches!(result, Err(ModemTransferError::Timeout)));
        let output = io.take_output();
        assert!(output.starts_with(&encode_bin32_header_with_escape(
            ZFrameType::ZFile,
            ZFILE_BINARY_FLAGS,
            true,
        )));
    }

    #[test]
    fn zcrcw_data_releases_software_flow_control() {
        let mut io = MemoryModemIo::default();

        write_zdata(&mut io, b"header", ZDataEnd::EndWithAck, true, true).unwrap();

        assert_eq!(io.take_output().last(), Some(&XON));
    }

    #[test]
    fn zmodem_send_resumes_from_zrpos_offset() {
        let mut input = encode_hex_header(ZFrameType::ZrInit, ZRINIT_FLAGS, true);
        input.extend(encode_hex_header(
            ZFrameType::ZRpos,
            position_header(3),
            true,
        ));
        input.extend(encode_hex_header(ZFrameType::ZrInit, ZRINIT_FLAGS, true));
        input.extend(encode_hex_header(
            ZFrameType::ZFin,
            position_header(0),
            false,
        ));
        let mut io = MemoryModemIo::with_input(input);
        let mut entries = [ZmodemSendStreamEntry {
            file_name: "payload.bin".to_string(),
            file_size: 6,
            reader: std::io::Cursor::new(b"abcdef".to_vec()),
        }];

        let sent = send_zmodem_stream(&mut io, &mut entries).expect("zmodem send");
        let output = io.take_output();

        assert_eq!(sent, 6);
        assert!(output.windows(3).any(|window| window == b"def"));
        assert!(!output.windows(3).any(|window| window == b"abc"));
        let resumed_data_header =
            encode_bin32_header_with_escape(ZFrameType::ZData, position_header(3), true);
        assert!(
            output
                .windows(resumed_data_header.len())
                .any(|window| window == resumed_data_header.as_slice())
        );
        assert!(output.ends_with(b"OO"));
    }

    #[test]
    fn zmodem_receive_requests_resume_after_mismatched_eof() {
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

        let mut data_io = MemoryModemIo::default();
        write_zdata(
            &mut data_io,
            &build_zfile_header("hello.bin", 5),
            ZDataEnd::EndWithAck,
            false,
            false,
        )
        .expect("file header data");
        let zfile_data = data_io.take_output();

        let mut payload_io = MemoryModemIo::default();
        write_zdata(&mut payload_io, b"hello", ZDataEnd::End, false, false).expect("payload data");
        let payload_data = payload_io.take_output();

        let mut input = encode_hex_header(ZFrameType::ZFile, position_header(0), true);
        input.extend(zfile_data);
        input.extend(encode_hex_header(
            ZFrameType::ZData,
            position_header(0),
            true,
        ));
        input.extend(payload_data);
        input.extend(encode_hex_header(
            ZFrameType::ZEof,
            position_header(9),
            true,
        ));
        input.extend(encode_hex_header(
            ZFrameType::ZEof,
            position_header(5),
            true,
        ));
        input.extend(encode_hex_header(
            ZFrameType::ZFin,
            position_header(0),
            false,
        ));
        input.extend_from_slice(b"OO");

        let mut probe = MemoryModemIo::with_input(input.clone());
        let zfile_header = read_zmodem_header(&mut probe).expect("probe zfile header");
        assert_eq!(zfile_header.frame_type, ZFrameType::ZFile);
        let zfile_frame =
            read_zmodem_data(&mut probe, false, ZMODEM_MAX_CHUNK).expect("probe zfile data");
        assert!(
            parse_zfile_header(&zfile_frame.payload)
                .expect("probe zfile parse")
                .is_some()
        );
        let data_header = read_zmodem_header(&mut probe).expect("probe data header");
        assert_eq!(data_header.frame_type, ZFrameType::ZData);
        let data_frame =
            read_zmodem_data(&mut probe, false, ZMODEM_MAX_CHUNK).expect("probe payload data");
        assert_eq!(data_frame.payload, b"hello");

        let output = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let writer = SharedWriter(output.clone());
        let mut io = MemoryModemIo::with_input(input);
        let received = receive_zmodem(&mut io, |_header| {
            Ok::<SharedWriter, ModemTransferError>(writer.clone())
        })
        .expect("zmodem receive");
        let replies = io.take_output();

        assert_eq!(received.len(), 1);
        assert_eq!(&*output.borrow(), b"hello");
        let resume_reply = encode_hex_header(ZFrameType::ZRpos, position_header(5), true);
        assert!(
            replies
                .windows(resume_reply.len())
                .any(|window| window == resume_reply.as_slice())
        );
    }

    #[test]
    fn zmodem_receive_acknowledges_crcq_at_the_committed_offset() {
        let mut header_io = MemoryModemIo::default();
        write_zdata(
            &mut header_io,
            &build_zfile_header("hello.bin", 5),
            ZDataEnd::EndWithAck,
            false,
            false,
        )
        .unwrap();
        let mut first_data_io = MemoryModemIo::default();
        write_zdata(
            &mut first_data_io,
            b"hel",
            ZDataEnd::ContinueWithAck,
            false,
            false,
        )
        .unwrap();
        let mut final_data_io = MemoryModemIo::default();
        write_zdata(&mut final_data_io, b"lo", ZDataEnd::End, false, false).unwrap();

        let mut input = encode_hex_header(ZFrameType::ZFile, position_header(0), true);
        input.extend(header_io.take_output());
        input.extend(encode_hex_header(
            ZFrameType::ZData,
            position_header(0),
            true,
        ));
        input.extend(first_data_io.take_output());
        input.extend(final_data_io.take_output());
        input.extend(encode_hex_header(
            ZFrameType::ZEof,
            position_header(5),
            true,
        ));
        input.extend(encode_hex_header(
            ZFrameType::ZFin,
            position_header(0),
            false,
        ));
        input.extend_from_slice(b"OO");

        let mut io = MemoryModemIo::with_input(input);
        let received = receive_zmodem(&mut io, |_header| {
            Ok::<Vec<u8>, ModemTransferError>(Vec::new())
        })
        .unwrap();
        let replies = io.take_output();
        let expected_ack = encode_hex_header(ZFrameType::ZAck, position_header(3), false);

        assert_eq!(received.len(), 1);
        assert!(
            replies
                .windows(expected_ack.len())
                .any(|window| window == expected_ack.as_slice())
        );
    }

    #[test]
    fn zmodem_receive_rejects_zfin_before_verified_eof() {
        let mut header_io = MemoryModemIo::default();
        write_zdata(
            &mut header_io,
            &build_zfile_header("partial.bin", 5),
            ZDataEnd::EndWithAck,
            false,
            false,
        )
        .unwrap();
        let mut input = encode_hex_header(ZFrameType::ZFile, position_header(0), true);
        input.extend(header_io.take_output());
        input.extend(encode_hex_header(
            ZFrameType::ZFin,
            position_header(0),
            false,
        ));
        let mut io = MemoryModemIo::with_input(input);

        let result = receive_zmodem(&mut io, |_header| {
            Ok::<Vec<u8>, ModemTransferError>(Vec::new())
        });

        assert!(matches!(result, Err(ModemTransferError::UnexpectedFrame)));
    }

    #[test]
    fn zmodem_data_rejects_payloads_beyond_the_negotiated_limit() {
        let mut io = MemoryModemIo::with_input(vec![b'x'; ZMODEM_MAX_CHUNK + 1]);

        let result = read_zmodem_data(&mut io, false, ZMODEM_MAX_CHUNK);

        assert!(matches!(
            result,
            Err(ModemTransferError::Protocol(ModemError::InvalidLength))
        ));
    }

    #[test]
    fn zmodem_send_rejects_files_beyond_32_bit_positions() {
        let file_size = u32::MAX as u64 + 1;
        let mut entries = [ZmodemSendStreamEntry {
            file_name: "huge.bin".to_string(),
            file_size,
            reader: std::io::Cursor::new(Vec::<u8>::new()),
        }];
        let mut io = MemoryModemIo::default();

        let result = send_zmodem_stream(&mut io, &mut entries);

        assert!(matches!(
            result,
            Err(ModemTransferError::UnsupportedFileSize(size)) if size == file_size
        ));
        assert!(io.take_output().is_empty());
    }

    #[test]
    fn zmodem_receive_finish_ignores_a_retried_zfin_header() {
        let mut input = encode_hex_header(ZFrameType::ZFin, position_header(0), false);
        input.extend_from_slice(b"OO");
        let mut io = MemoryModemIo::with_input(input);

        finish_zmodem_receive(&mut io).unwrap();

        assert_eq!(
            io.take_output(),
            encode_hex_header(ZFrameType::ZFin, position_header(0), false)
        );
    }

    #[test]
    fn zmodem_send_rejects_resume_positions_past_end_of_file() {
        let mut input = encode_hex_header(ZFrameType::ZrInit, ZRINIT_FLAGS, true);
        input.extend(encode_hex_header(
            ZFrameType::ZRpos,
            position_header(7),
            true,
        ));
        let mut io = MemoryModemIo::with_input(input);
        let mut entries = [ZmodemSendStreamEntry {
            file_name: "short.bin".to_string(),
            file_size: 5,
            reader: std::io::Cursor::new(b"short".to_vec()),
        }];

        let result = send_zmodem_stream(&mut io, &mut entries);

        assert!(matches!(result, Err(ModemTransferError::UnexpectedFrame)));
    }
}
