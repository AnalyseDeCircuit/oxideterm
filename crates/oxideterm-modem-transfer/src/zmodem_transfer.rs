// Copyright (C) 2026 OxideTerm contributors.
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::time::Duration;

use crate::crc::{crc16_xmodem_update, crc32_ieee_update};
use crate::error::{ModemError, ModemTransferError};
use crate::io::{MemoryModemIo, ModemIo};
use crate::zmodem::{
    XON, ZBIN, ZBIN32, ZDLE, ZDataEnd, ZFrameType, ZHeader, ZHeaderEncoding, ZPAD,
    encode_hex_header, position_header, push_zdle_escaped,
};

const ZMODEM_TIMEOUT: Duration = Duration::from_secs(10);
const ZMODEM_MAX_CHUNK: usize = 8192;
const ZMODEM_MAX_POSITION: u64 = u32::MAX as u64;
const ZRINIT_FLAGS: [u8; 4] = [0, 0, 0, 0x23];

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
                let file_header_data = read_zmodem_data(io, use_crc32)?.payload;
                let Some(file_header) = parse_zfile_header(&file_header_data)? else {
                    send_zfin(io)?;
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
                        ZFrameType::ZData => loop {
                            let frame = match read_zmodem_data(io, use_crc32) {
                                Ok(frame) => frame,
                                Err(ModemTransferError::Protocol(ModemError::InvalidCrc)) => {
                                    write_zheader(
                                        io,
                                        ZFrameType::ZRpos,
                                        position_header_checked(received_offset)?,
                                    )?;
                                    break;
                                }
                                Err(error) => return Err(error),
                            };
                            writer.write_all(&frame.payload)?;
                            received_offset =
                                received_offset.saturating_add(frame.payload.len() as u64);
                            if !matches!(frame.end, ZDataEnd::Continue | ZDataEnd::ContinueWithAck)
                            {
                                break;
                            }
                        },
                        ZFrameType::ZEof => {
                            if data_header.position_u32() as u64 != received_offset {
                                write_zheader(
                                    io,
                                    ZFrameType::ZRpos,
                                    position_header_checked(received_offset)?,
                                )?;
                                continue;
                            }
                            send_zrinit(io)?;
                            received.push(file_header);
                            break;
                        }
                        ZFrameType::ZFin => {
                            send_zfin(io)?;
                            received.push(file_header);
                            return Ok(received);
                        }
                        ZFrameType::ZAbort | ZFrameType::ZCan => {
                            return Err(ModemTransferError::Cancelled);
                        }
                        _ => return Err(ModemTransferError::UnexpectedFrame),
                    }
                }
            }
            ZFrameType::ZFin => {
                send_zfin(io)?;
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

    wait_for_zrinit(io)?;
    let mut total = 0u64;

    for entry in entries {
        let header_payload = build_zfile_header(&entry.file_name, entry.file_size);
        write_zheader(io, ZFrameType::ZFile, position_header(0))?;
        write_zdata(io, &header_payload, ZDataEnd::EndWithAck, false)?;

        loop {
            let response = read_zmodem_header(io)?;
            match response.frame_type {
                ZFrameType::ZRpos => {
                    let mut offset = response.position_u32() as u64;
                    loop {
                        send_zfile_data_stream(io, &mut entry.reader, entry.file_size, offset)?;
                        let followup = read_zmodem_header(io)?;
                        match followup.frame_type {
                            ZFrameType::ZrInit => {
                                total += entry.file_size;
                                break;
                            }
                            ZFrameType::ZRpos => {
                                offset = followup.position_u32() as u64;
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

fn wait_for_zrinit<I: ModemIo>(io: &mut I) -> Result<(), ModemTransferError> {
    loop {
        let header = read_zmodem_header(io)?;
        match header.frame_type {
            ZFrameType::ZrInit => return Ok(()),
            ZFrameType::ZrqInit => write_zheader(io, ZFrameType::ZrInit, ZRINIT_FLAGS)?,
            ZFrameType::ZAbort | ZFrameType::ZCan => return Err(ModemTransferError::Cancelled),
            _ => {}
        }
    }
}

fn send_zfile_data_stream<I, R>(
    io: &mut I,
    reader: &mut R,
    file_size: u64,
    offset: u64,
) -> Result<(), ModemTransferError>
where
    I: ModemIo,
    R: Read + Seek,
{
    let start = offset.min(file_size);
    reader.seek(SeekFrom::Start(start))?;
    write_zheader(io, ZFrameType::ZData, position_header_checked(start)?)?;
    let mut buffer = vec![0u8; ZMODEM_MAX_CHUNK];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        write_zdata(io, &buffer[..read], ZDataEnd::Continue, false)?;
    }
    write_zdata(io, &[], ZDataEnd::End, false)?;
    write_zheader(io, ZFrameType::ZEof, position_header_checked(file_size)?)?;
    Ok(())
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
    for _ in 0..3 {
        write_zheader(io, ZFrameType::ZrInit, ZRINIT_FLAGS)?;
    }
    Ok(())
}

fn send_zfin<I: ModemIo>(io: &mut I) -> Result<(), ModemTransferError> {
    write_zheader(io, ZFrameType::ZFin, position_header(0))?;
    io.write_all(b"OO")
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

fn write_zdata<I: ModemIo>(
    io: &mut I,
    payload: &[u8],
    end: ZDataEnd,
    use_crc32: bool,
) -> Result<(), ModemTransferError> {
    let mut out = Vec::with_capacity(payload.len() * 2 + 8);
    if use_crc32 {
        let mut crc = 0xffff_ffffu32;
        for byte in payload {
            push_zdle_escaped(&mut out, *byte);
            crc = crc32_ieee_update(crc, *byte);
        }
        out.extend_from_slice(&[ZDLE, end.marker()]);
        crc = crc32_ieee_update(crc, end.marker());
        crc = !crc;
        for byte in crc.to_le_bytes() {
            push_zdle_escaped(&mut out, byte);
        }
    } else {
        let mut crc = 0u16;
        for byte in payload {
            push_zdle_escaped(&mut out, *byte);
            crc = crc16_xmodem_update(crc, *byte);
        }
        out.extend_from_slice(&[ZDLE, end.marker()]);
        crc = crc16_xmodem_update(crc, end.marker());
        push_zdle_escaped(&mut out, (crc >> 8) as u8);
        push_zdle_escaped(&mut out, crc as u8);
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
) -> Result<ZDataFrame, ModemTransferError> {
    let mut payload = Vec::new();
    let mut frame_start = true;
    let end = loop {
        let byte = io.read_byte(ZMODEM_TIMEOUT)?;
        if frame_start && byte == XON {
            continue;
        }
        frame_start = false;
        if byte != ZDLE {
            payload.push(byte);
            continue;
        }
        let escaped = io.read_byte(ZMODEM_TIMEOUT)?;
        if let Some(end) = ZDataEnd::from_marker(escaped) {
            break end;
        }
        payload.push(escaped ^ 0x40);
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
    let byte = io.read_byte(ZMODEM_TIMEOUT)?;
    if byte == ZDLE {
        Ok(io.read_byte(ZMODEM_TIMEOUT)? ^ 0x40)
    } else {
        Ok(byte)
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
        .and_then(|size| size.parse().ok());
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
        let mut io = MemoryModemIo::with_input(input);
        let received = receive_zmodem(&mut io, |_header| {
            Ok::<Vec<u8>, ModemTransferError>(Vec::new())
        })
        .expect("zmodem receive");
        let output = io.take_output();
        assert!(received.is_empty());
        assert!(output.starts_with(&encode_hex_header(ZFrameType::ZrInit, ZRINIT_FLAGS, true)));
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
        assert!(output.starts_with(&encode_hex_header(
            ZFrameType::ZFile,
            position_header(0),
            true
        )));
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
        let resumed_data_header = encode_hex_header(ZFrameType::ZData, position_header(3), true);
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
        )
        .expect("file header data");
        let zfile_data = data_io.take_output();

        let mut payload_io = MemoryModemIo::default();
        write_zdata(&mut payload_io, b"hello", ZDataEnd::End, false).expect("payload data");
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

        let mut probe = MemoryModemIo::with_input(input.clone());
        let zfile_header = read_zmodem_header(&mut probe).expect("probe zfile header");
        assert_eq!(zfile_header.frame_type, ZFrameType::ZFile);
        let zfile_frame = read_zmodem_data(&mut probe, false).expect("probe zfile data");
        assert!(
            parse_zfile_header(&zfile_frame.payload)
                .expect("probe zfile parse")
                .is_some()
        );
        let data_header = read_zmodem_header(&mut probe).expect("probe data header");
        assert_eq!(data_header.frame_type, ZFrameType::ZData);
        let data_frame = read_zmodem_data(&mut probe, false).expect("probe payload data");
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
}
