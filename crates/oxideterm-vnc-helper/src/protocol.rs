// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

pub(super) fn decode_vnc_clipboard_text(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|byte| char::from(*byte)).collect(),
    }
}

pub(super) fn encode_vnc_clipboard_text(text: &str) -> Vec<u8> {
    text.chars()
        .map(|character| {
            // Baseline ClientCutText is Latin-1; unsupported codepoints use the
            // conventional replacement byte until Extended Clipboard exists.
            u8::try_from(character as u32).unwrap_or(b'?')
        })
        .collect()
}

pub(super) fn client_cut_text_message(text: &str) -> Result<Vec<u8>, String> {
    let bytes = encode_vnc_clipboard_text(text);
    let len = u32::try_from(bytes.len())
        .map_err(|_| "VNC clipboard text is too large to send.".to_string())?;
    let mut message = Vec::with_capacity(8 + bytes.len());
    message.push(6);
    message.extend_from_slice(&[0, 0, 0]);
    push_be_u32(&mut message, len);
    message.extend_from_slice(&bytes);
    Ok(message)
}

pub(super) fn request_framebuffer_update(
    writer: &SharedVncWriter,
    incremental: bool,
    width: u16,
    height: u16,
) -> Result<(), String> {
    write_vnc_message(
        writer,
        &framebuffer_update_request_message(incremental, width, height),
    )
}

pub(super) fn framebuffer_update_request_message(
    incremental: bool,
    width: u16,
    height: u16,
) -> Vec<u8> {
    let mut message = Vec::with_capacity(10);
    message.push(3);
    message.push(u8::from(incremental));
    push_be_u16(&mut message, 0);
    push_be_u16(&mut message, 0);
    push_be_u16(&mut message, width);
    push_be_u16(&mut message, height);
    message
}

pub(super) fn write_vnc_message(writer: &SharedVncWriter, message: &[u8]) -> Result<(), String> {
    write_vnc_owned_message(writer, message.to_vec())
}

pub(super) fn write_vnc_owned_message(
    writer: &SharedVncWriter,
    message: Vec<u8>,
) -> Result<(), String> {
    writer
        .try_send(VncIoCommand::Write(message))
        .map_err(|error| match error {
            std::sync::mpsc::TrySendError::Full(_) => "VNC I/O command queue is full.".to_string(),
            std::sync::mpsc::TrySendError::Disconnected(_) => {
                "VNC I/O command owner is unavailable.".to_string()
            }
        })
}

pub(super) fn send_event(
    writer: &SharedEventWriter,
    event: RemoteDesktopHelperEvent,
) -> Result<(), String> {
    let mut writer = writer
        .lock()
        .map_err(|_| "VNC event writer lock is poisoned.".to_string())?;
    write_event_line(&mut *writer, &event).map_err(|error| error.to_string())
}

pub(super) fn rect_byte_len(rect: RfbRect) -> Result<usize, String> {
    let pixels = usize::from(rect.width)
        .checked_mul(usize::from(rect.height))
        .ok_or_else(|| "VNC rectangle dimensions overflowed.".to_string())?;
    let bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| "VNC rectangle byte count overflowed.".to_string())?;
    if bytes > MAX_VNC_FRAME_BYTES {
        return Err("VNC rectangle is larger than the helper limit.".to_string());
    }
    Ok(bytes)
}

pub(super) fn read_rich_cursor(
    reader: &mut impl Read,
    rect: RfbRect,
) -> Result<VncServerEvent, String> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(VncServerEvent::CursorHidden);
    }

    let byte_len = rect_byte_len(rect)?;
    let bytes = read_exact_vec(reader, byte_len)
        .map_err(|error| format!("VNC cursor pixels read failed: {error}"))?;
    let mask = read_exact_vec(reader, cursor_mask_len(rect)?)
        .map_err(|error| format!("VNC cursor mask read failed: {error}"))?;

    rich_cursor_event(rect, bytes, &mask)
}

pub(super) fn read_x_cursor(
    reader: &mut impl Read,
    rect: RfbRect,
) -> Result<VncServerEvent, String> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(VncServerEvent::CursorHidden);
    }

    let colors = read_exact_array::<6, _>(reader)
        .map_err(|error| format!("VNC X cursor colors read failed: {error}"))?;
    let mask_len = cursor_mask_len(rect)?;
    let bitmap = read_exact_vec(reader, mask_len)
        .map_err(|error| format!("VNC X cursor bitmap read failed: {error}"))?;
    let mask = read_exact_vec(reader, mask_len)
        .map_err(|error| format!("VNC X cursor mask read failed: {error}"))?;
    x_cursor_event(rect, colors, &bitmap, &mask)
}

pub(super) fn rich_cursor_event(
    rect: RfbRect,
    mut bytes: Vec<u8>,
    mask: &[u8],
) -> Result<VncServerEvent, String> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(VncServerEvent::CursorHidden);
    }
    if bytes.len() < rect_byte_len(rect)? || mask.len() < cursor_mask_len(rect)? {
        return Err("VNC cursor payload is incomplete.".to_string());
    }

    for y in 0..rect.height {
        for x in 0..rect.width {
            let pixel_start = cursor_pixel_offset(rect.width, x, y)?;
            bytes[pixel_start + 3] = if cursor_mask_bit(mask, rect.width, x, y) {
                u8::MAX
            } else {
                0
            };
        }
    }

    cursor_shape_event(rect, bytes)
}

pub(super) fn x_cursor_event(
    rect: RfbRect,
    colors: [u8; 6],
    bitmap: &[u8],
    mask: &[u8],
) -> Result<VncServerEvent, String> {
    if rect.width == 0 || rect.height == 0 {
        return Ok(VncServerEvent::CursorHidden);
    }
    let mask_len = cursor_mask_len(rect)?;
    if bitmap.len() < mask_len || mask.len() < mask_len {
        return Err("VNC X cursor payload is incomplete.".to_string());
    }

    let mut bytes = vec![0; rect_byte_len(rect)?];

    for y in 0..rect.height {
        for x in 0..rect.width {
            let pixel_start = cursor_pixel_offset(rect.width, x, y)?;
            if !cursor_mask_bit(mask, rect.width, x, y) {
                continue;
            }

            let color_start = if cursor_mask_bit(bitmap, rect.width, x, y) {
                0
            } else {
                3
            };
            bytes[pixel_start] = colors[color_start + 2];
            bytes[pixel_start + 1] = colors[color_start + 1];
            bytes[pixel_start + 2] = colors[color_start];
            bytes[pixel_start + 3] = u8::MAX;
        }
    }

    cursor_shape_event(rect, bytes)
}

pub(super) fn cursor_shape_event(rect: RfbRect, bytes: Vec<u8>) -> Result<VncServerEvent, String> {
    // Cursor pseudo-encoding uses rect x/y as the hotspot, not screen position.
    let shape = RemoteDesktopCursorShape::new(
        RemoteDesktopSize {
            width: u32::from(rect.width),
            height: u32::from(rect.height),
        },
        u32::from(rect.x.min(rect.width.saturating_sub(1))),
        u32::from(rect.y.min(rect.height.saturating_sub(1))),
        RemoteDesktopFrameFormat::Bgra8,
        bytes,
    );
    if shape.is_complete() {
        Ok(VncServerEvent::CursorShape(shape))
    } else {
        Err("VNC cursor shape is incomplete.".to_string())
    }
}

pub(super) fn cursor_mask_len(rect: RfbRect) -> Result<usize, String> {
    let row_bytes = usize::from(rect.width)
        .checked_add(7)
        .ok_or_else(|| "VNC cursor mask row length overflowed.".to_string())?
        / 8;
    row_bytes
        .checked_mul(usize::from(rect.height))
        .ok_or_else(|| "VNC cursor mask length overflowed.".to_string())
}

pub(super) fn cursor_mask_bit(mask: &[u8], width: u16, x: u16, y: u16) -> bool {
    let row_bytes = (usize::from(width) + 7) / 8;
    let byte_index = usize::from(y) * row_bytes + usize::from(x) / 8;
    let bit_index = 7 - usize::from(x) % 8;
    mask.get(byte_index)
        .is_some_and(|byte| (byte & (1u8 << bit_index)) != 0)
}

pub(super) fn cursor_pixel_offset(width: u16, x: u16, y: u16) -> Result<usize, String> {
    usize::from(y)
        .checked_mul(usize::from(width))
        .and_then(|row| row.checked_add(usize::from(x)))
        .and_then(|pixel| pixel.checked_mul(4))
        .ok_or_else(|| "VNC cursor pixel offset overflowed.".to_string())
}

pub(super) fn read_u8(reader: &mut impl Read) -> io::Result<u8> {
    let mut byte = [0; 1];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

pub(super) fn read_be_u16(reader: &mut impl Read) -> io::Result<u16> {
    let bytes = read_exact_array::<2, _>(reader)?;
    Ok(be_u16(&bytes))
}

pub(super) fn read_be_u32(reader: &mut impl Read) -> io::Result<u32> {
    let bytes = read_exact_array::<4, _>(reader)?;
    Ok(be_u32(&bytes))
}

pub(super) fn read_exact_array<const N: usize, R: Read>(reader: &mut R) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub(super) fn read_exact_vec(reader: &mut impl Read, len: usize) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).map_err(|_| {
        io::Error::new(
            io::ErrorKind::OutOfMemory,
            "VNC payload allocation exceeds available memory",
        )
    })?;
    bytes.resize(len, 0);
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

pub(super) fn be_u16(bytes: &[u8]) -> u16 {
    u16::from_be_bytes([bytes[0], bytes[1]])
}

pub(super) fn be_u32(bytes: &[u8]) -> u32 {
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

pub(super) fn be_i32(bytes: &[u8]) -> i32 {
    i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

pub(super) fn push_be_u16(message: &mut Vec<u8>, value: u16) {
    message.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn push_be_u32(message: &mut Vec<u8>, value: u32) {
    message.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn push_be_i32(message: &mut Vec<u8>, value: i32) {
    message.extend_from_slice(&value.to_be_bytes());
}

pub(super) fn clamp_u32_to_u16(value: u32) -> u16 {
    value.min(u16::MAX as u32) as u16
}
