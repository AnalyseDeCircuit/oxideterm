// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use super::*;

const MAX_VNC_RECTS_PER_UPDATE: usize = 4096;
const MAX_VNC_UPDATE_DECODED_BYTES: usize = MAX_VNC_FRAME_BYTES;

pub(super) struct VncDecodeState {
    zrle_decompressor: Decompress,
    tight: VncTightState,
    h264: Option<VncH264State>,
}

#[derive(Default)]
struct VncFramebufferUpdateBudget {
    rectangles: usize,
    decoded_bytes: usize,
}

impl VncFramebufferUpdateBudget {
    fn consume(&mut self, encoding: i32, rect: RfbRect) -> Result<(), String> {
        self.rectangles = self
            .rectangles
            .checked_add(1)
            .ok_or_else(|| "VNC framebuffer rectangle count overflowed.".to_string())?;
        if self.rectangles > MAX_VNC_RECTS_PER_UPDATE {
            return Err(format!(
                "VNC framebuffer update exceeds the {MAX_VNC_RECTS_PER_UPDATE}-rectangle limit."
            ));
        }
        if is_vnc_pixel_encoding(encoding) {
            self.decoded_bytes = self
                .decoded_bytes
                .checked_add(rect_byte_len(rect)?)
                .ok_or_else(|| "VNC framebuffer update byte budget overflowed.".to_string())?;
            if self.decoded_bytes > MAX_VNC_UPDATE_DECODED_BYTES {
                return Err(format!(
                    "VNC framebuffer update exceeds the {MAX_VNC_UPDATE_DECODED_BYTES}-byte decoded limit."
                ));
            }
        }
        Ok(())
    }
}

fn is_vnc_pixel_encoding(encoding: i32) -> bool {
    matches!(
        encoding,
        VNC_ENCODING_RAW
            | VNC_ENCODING_COPY_RECT
            | VNC_ENCODING_HEXTILE
            | VNC_ENCODING_TIGHT
            | VNC_ENCODING_ZRLE
            | VNC_ENCODING_OPEN_H264
            | VNC_ENCODING_CURSOR
            | VNC_ENCODING_X_CURSOR
    )
}

#[cfg(test)]
mod budget_tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn framebuffer_update_budget_rejects_rectangle_and_aggregate_limits() {
        let tiny = RfbRect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let mut rectangle_budget = VncFramebufferUpdateBudget::default();
        for _ in 0..MAX_VNC_RECTS_PER_UPDATE {
            rectangle_budget
                .consume(VNC_ENCODING_COPY_RECT, tiny)
                .unwrap();
        }
        assert!(
            rectangle_budget
                .consume(VNC_ENCODING_COPY_RECT, tiny)
                .unwrap_err()
                .contains("rectangle limit")
        );

        let large = RfbRect {
            x: 0,
            y: 0,
            width: 4096,
            height: 4096,
        };
        let mut byte_budget = VncFramebufferUpdateBudget::default();
        byte_budget.consume(VNC_ENCODING_RAW, large).unwrap();
        byte_budget.consume(VNC_ENCODING_RAW, large).unwrap();
        assert!(
            byte_budget
                .consume(VNC_ENCODING_RAW, large)
                .unwrap_err()
                .contains("decoded limit")
        );

        let mut cursor_budget = VncFramebufferUpdateBudget::default();
        cursor_budget.consume(VNC_ENCODING_CURSOR, large).unwrap();
        cursor_budget.consume(VNC_ENCODING_X_CURSOR, large).unwrap();
        assert!(
            cursor_budget
                .consume(VNC_ENCODING_CURSOR, large)
                .unwrap_err()
                .contains("decoded limit")
        );
    }

    #[test]
    fn unknown_wire_count_can_end_early_with_last_rect() {
        let mut wire = vec![0];
        push_be_u16(&mut wire, u16::MAX);
        wire.extend_from_slice(&[0; 8]);
        push_be_i32(&mut wire, VNC_ENCODING_LAST_RECT);

        let event = read_framebuffer_update(&mut Cursor::new(wire), &mut VncDecodeState::default())
            .unwrap();
        assert_eq!(
            event,
            VncServerEvent::Batch(vec![VncServerEvent::ObservedCapability(
                VncObservedCapability::LastRect
            )])
        );
    }

    #[test]
    fn unknown_wire_count_without_last_rect_is_bounded() {
        let mut wire = vec![0];
        push_be_u16(&mut wire, u16::MAX);
        for _ in 0..MAX_VNC_RECTS_PER_UPDATE {
            wire.extend_from_slice(&[0; 8]);
            push_be_i32(&mut wire, VNC_ENCODING_DESKTOP_SIZE);
        }

        assert!(
            read_framebuffer_update(&mut Cursor::new(wire), &mut VncDecodeState::default())
                .unwrap_err()
                .contains("LastRect")
        );
    }
}

impl Default for VncDecodeState {
    fn default() -> Self {
        Self::new(None)
    }
}

impl VncDecodeState {
    pub(super) fn new(h264: Option<VncH264State>) -> Self {
        Self {
            // ZRLE uses one zlib stream object for the lifetime of the RFB
            // connection, so keep the inflater in reader state.
            zrle_decompressor: Decompress::new(true),
            tight: VncTightState::default(),
            h264,
        }
    }
}

pub(super) fn vnc_frame_event_for_change(
    framebuffer: &VncFramebuffer,
    change: VncFramebufferChange,
    sent_initial_frame: &mut bool,
    force_base_frame: bool,
) -> RemoteDesktopHelperEvent {
    // Recovery requests rebuild the UI backing buffer from the helper's
    // complete framebuffer snapshot, even when the server reports a dirty rect.
    if force_base_frame {
        *sent_initial_frame = true;
        return RemoteDesktopHelperEvent::Frame {
            frame: framebuffer.frame(),
        };
    }

    match change {
        VncFramebufferChange::Full => {
            *sent_initial_frame = true;
            RemoteDesktopHelperEvent::Frame {
                frame: framebuffer.frame(),
            }
        }
        VncFramebufferChange::Rect(rect) if *sent_initial_frame => {
            if let Some(update) = framebuffer.frame_update(rect) {
                RemoteDesktopHelperEvent::FrameUpdate { update }
            } else {
                *sent_initial_frame = true;
                RemoteDesktopHelperEvent::Frame {
                    frame: framebuffer.frame(),
                }
            }
        }
        VncFramebufferChange::Rect(_) => {
            *sent_initial_frame = true;
            RemoteDesktopHelperEvent::Frame {
                frame: framebuffer.frame(),
            }
        }
    }
}

pub(super) fn vnc_helper_events(event: &VncServerEvent) -> Vec<RemoteDesktopHelperEvent> {
    // Framebuffer updates can carry non-frame pseudo-rectangles, so collect
    // side-channel helper events before mutating the backing framebuffer.
    match event {
        VncServerEvent::ClipboardText(text) => {
            vec![RemoteDesktopHelperEvent::ClipboardText { text: text.clone() }]
        }
        VncServerEvent::CursorShape(shape) => {
            vec![RemoteDesktopHelperEvent::CursorShape {
                shape: shape.clone(),
            }]
        }
        VncServerEvent::CursorHidden => vec![RemoteDesktopHelperEvent::CursorHidden],
        VncServerEvent::Batch(events) => {
            let mut helper_events = Vec::new();
            for event in events {
                helper_events.extend(vnc_helper_events(event));
            }
            helper_events
        }
        _ => Vec::new(),
    }
}

pub(super) fn vnc_server_event_summary(event: &VncServerEvent) -> VncServerEventSummary {
    match event {
        VncServerEvent::SetResolution { width, height } => VncServerEventSummary {
            dirty_rects: 1,
            dirty_pixels: u64::from(*width) * u64::from(*height),
            side_events: 0,
        },
        VncServerEvent::ExtendedDesktopSize(update) if update.applies_layout() => {
            VncServerEventSummary {
                dirty_rects: 1,
                dirty_pixels: u64::from(update.layout.width) * u64::from(update.layout.height),
                side_events: 1,
            }
        }
        VncServerEvent::RawImage(rect, _) | VncServerEvent::CopyRect { dst: rect, .. } => {
            VncServerEventSummary {
                dirty_rects: 1,
                dirty_pixels: rfb_rect_pixels(*rect),
                side_events: 0,
            }
        }
        VncServerEvent::ClipboardText(_)
        | VncServerEvent::ClipboardExtended(_)
        | VncServerEvent::CursorShape(_)
        | VncServerEvent::CursorHidden
        | VncServerEvent::ExtendedDesktopSize(_)
        | VncServerEvent::QemuAudioCapability
        | VncServerEvent::QemuAudio(_)
        | VncServerEvent::QemuExtendedKeyEvents
        | VncServerEvent::ExtendedMouseButtons
        | VncServerEvent::LockKeys(_)
        | VncServerEvent::ObservedCapability(_)
        | VncServerEvent::ServerFence(_)
        | VncServerEvent::EndOfContinuousUpdates => VncServerEventSummary {
            dirty_rects: 0,
            dirty_pixels: 0,
            side_events: 1,
        },
        VncServerEvent::Batch(events) => {
            let mut summary = VncServerEventSummary::default();
            for event in events {
                let event_summary = vnc_server_event_summary(event);
                summary.dirty_rects = summary
                    .dirty_rects
                    .saturating_add(event_summary.dirty_rects);
                summary.dirty_pixels = summary
                    .dirty_pixels
                    .saturating_add(event_summary.dirty_pixels);
                summary.side_events = summary
                    .side_events
                    .saturating_add(event_summary.side_events);
            }
            summary
        }
        VncServerEvent::Noop => VncServerEventSummary::default(),
    }
}

pub(super) fn rfb_rect_pixels(rect: RfbRect) -> u64 {
    u64::from(rect.width) * u64::from(rect.height)
}

pub(super) fn read_vnc_event_after_type(
    reader: &mut impl Read,
    decode_state: &mut VncDecodeState,
    message_type: u8,
) -> Result<VncServerEvent, String> {
    match message_type {
        0 => read_framebuffer_update(reader, decode_state),
        1 => {
            skip_color_map_entries(reader)?;
            Ok(VncServerEvent::Noop)
        }
        2 => Ok(VncServerEvent::Noop),
        3 => match read_server_cut_text(reader)? {
            VncClipboardMessage::Legacy(text) => Ok(VncServerEvent::ClipboardText(text)),
            VncClipboardMessage::Extended(message) => {
                Ok(VncServerEvent::ClipboardExtended(message))
            }
        },
        150 => Ok(VncServerEvent::EndOfContinuousUpdates),
        248 => read_server_fence(reader).map(VncServerEvent::ServerFence),
        255 => read_qemu_audio_server_message(reader).map(VncServerEvent::QemuAudio),
        other => Err(format!("Unsupported VNC server message type {other}.")),
    }
}

pub(super) fn read_framebuffer_update(
    reader: &mut impl Read,
    decode_state: &mut VncDecodeState,
) -> Result<VncServerEvent, String> {
    let _padding =
        read_u8(reader).map_err(|error| format!("VNC framebuffer padding read failed: {error}"))?;
    let rect_count = read_be_u16(reader)
        .map_err(|error| format!("VNC framebuffer rect count read failed: {error}"))?;
    let count_is_unknown = rect_count == u16::MAX;
    if !count_is_unknown && usize::from(rect_count) > MAX_VNC_RECTS_PER_UPDATE {
        return Err(format!(
            "VNC framebuffer update exceeds the {MAX_VNC_RECTS_PER_UPDATE}-rectangle limit."
        ));
    }
    let actual_limit = if count_is_unknown {
        MAX_VNC_RECTS_PER_UPDATE
    } else {
        usize::from(rect_count)
    };
    let mut events = Vec::with_capacity(actual_limit);
    let mut budget = VncFramebufferUpdateBudget::default();
    let mut found_last_rect = false;

    for _ in 0..actual_limit {
        let header = read_exact_array::<12, _>(reader)
            .map_err(|error| format!("VNC framebuffer rect header read failed: {error}"))?;
        let rect = RfbRect {
            x: be_u16(&header[0..2]),
            y: be_u16(&header[2..4]),
            width: be_u16(&header[4..6]),
            height: be_u16(&header[6..8]),
        };
        let encoding = be_i32(&header[8..12]);
        budget.consume(encoding, rect)?;
        match encoding {
            VNC_ENCODING_RAW => {
                let byte_len = rect_byte_len(rect)?;
                let data = read_exact_vec(reader, byte_len)
                    .map_err(|error| format!("VNC raw rectangle read failed: {error}"))?;
                events.push(VncServerEvent::RawImage(rect, data));
            }
            VNC_ENCODING_COPY_RECT => {
                let source = read_exact_array::<4, _>(reader)
                    .map_err(|error| format!("VNC copy-rect source read failed: {error}"))?;
                events.push(VncServerEvent::CopyRect {
                    dst: rect,
                    src_x: be_u16(&source[0..2]),
                    src_y: be_u16(&source[2..4]),
                });
            }
            VNC_ENCODING_HEXTILE => {
                events.push(VncServerEvent::RawImage(
                    rect,
                    read_hextile_rect(reader, rect)?,
                ));
            }
            VNC_ENCODING_TIGHT => {
                let tight = read_tight_rect(reader, rect, &mut decode_state.tight)?;
                events.push(VncServerEvent::ObservedCapability(
                    VncObservedCapability::Tight,
                ));
                if tight.used_jpeg {
                    events.push(VncServerEvent::ObservedCapability(
                        VncObservedCapability::Jpeg,
                    ));
                }
                events.push(VncServerEvent::RawImage(rect, tight.bgra));
            }
            VNC_ENCODING_ZRLE => {
                events.push(VncServerEvent::RawImage(
                    rect,
                    read_zrle_rect(reader, rect, decode_state)?,
                ));
            }
            VNC_ENCODING_OPEN_H264 => {
                let h264 = decode_state.h264.as_mut().ok_or_else(|| {
                    "VNC server selected Open H.264 without an available decoder.".to_string()
                })?;
                let decoded = h264.decode_rectangle(reader, rect)?;
                events.push(VncServerEvent::ObservedCapability(
                    VncObservedCapability::H264,
                ));
                if let Some(bgra) = decoded {
                    events.push(VncServerEvent::RawImage(rect, bgra));
                }
            }
            VNC_ENCODING_DESKTOP_SIZE => {
                events.push(VncServerEvent::SetResolution {
                    width: rect.width,
                    height: rect.height,
                });
            }
            VNC_ENCODING_EXTENDED_DESKTOP_SIZE => {
                events.push(VncServerEvent::ExtendedDesktopSize(
                    read_extended_desktop_size(reader, rect)?,
                ));
            }
            VNC_ENCODING_CURSOR => {
                events.push(read_rich_cursor(reader, rect)?);
            }
            VNC_ENCODING_X_CURSOR => {
                events.push(read_x_cursor(reader, rect)?);
            }
            VNC_ENCODING_QEMU_AUDIO => {
                require_empty_vnc_extension_rect(rect, "QEMU Audio")?;
                events.push(VncServerEvent::QemuAudioCapability);
            }
            VNC_ENCODING_QEMU_EXTENDED_KEY_EVENT => {
                require_empty_vnc_extension_rect(rect, "QEMU extended key event")?;
                events.push(VncServerEvent::QemuExtendedKeyEvents);
            }
            VNC_ENCODING_QEMU_LED_STATE => {
                let state = read_u8(reader)
                    .map_err(|error| format!("VNC QEMU LED state read failed: {error}"))?;
                events.push(VncServerEvent::LockKeys(vnc_lock_keys_from_bits(state)));
            }
            VNC_ENCODING_VMWARE_LED_STATE => {
                let state = read_be_u32(reader)
                    .map_err(|error| format!("VNC VMware LED state read failed: {error}"))?;
                events.push(VncServerEvent::LockKeys(vnc_lock_keys_from_bits(
                    state as u8,
                )));
            }
            VNC_ENCODING_EXTENDED_MOUSE_BUTTONS => {
                require_empty_vnc_extension_rect(rect, "extended mouse buttons")?;
                events.push(VncServerEvent::ExtendedMouseButtons);
            }
            VNC_ENCODING_LAST_RECT => {
                events.push(VncServerEvent::ObservedCapability(
                    VncObservedCapability::LastRect,
                ));
                found_last_rect = true;
                break;
            }
            other => return Err(format!("Unsupported VNC rectangle encoding {other}.")),
        }
    }
    if count_is_unknown && !found_last_rect {
        return Err(format!(
            "VNC framebuffer update with an unknown rectangle count did not send LastRect within {MAX_VNC_RECTS_PER_UPDATE} rectangles."
        ));
    }

    Ok(VncServerEvent::Batch(events))
}

pub(super) fn require_empty_vnc_extension_rect(
    rect: RfbRect,
    extension_name: &str,
) -> Result<(), String> {
    if rect.width == 0 && rect.height == 0 {
        Ok(())
    } else {
        Err(format!(
            "VNC {extension_name} capability rectangle is not empty."
        ))
    }
}

pub(super) fn skip_color_map_entries(reader: &mut impl Read) -> Result<(), String> {
    let _padding =
        read_u8(reader).map_err(|error| format!("VNC color-map padding read failed: {error}"))?;
    let _first = read_be_u16(reader)
        .map_err(|error| format!("VNC color-map first index read failed: {error}"))?;
    let count =
        read_be_u16(reader).map_err(|error| format!("VNC color-map count read failed: {error}"))?;
    read_exact_vec(reader, count as usize * 6)
        .map(|_| ())
        .map_err(|error| format!("VNC color-map entries read failed: {error}"))
}

#[derive(Default)]
pub(super) struct HextileState {
    background: Option<[u8; 4]>,
    foreground: Option<[u8; 4]>,
}

pub(super) fn read_hextile_rect(reader: &mut impl Read, rect: RfbRect) -> Result<Vec<u8>, String> {
    let mut bytes = vec![0; rect_byte_len(rect)?];
    let mut state = HextileState::default();

    for tile_y in (0..rect.height).step_by(VNC_HEXTILE_TILE_SIZE as usize) {
        for tile_x in (0..rect.width).step_by(VNC_HEXTILE_TILE_SIZE as usize) {
            let tile_width = (rect.width - tile_x).min(VNC_HEXTILE_TILE_SIZE);
            let tile_height = (rect.height - tile_y).min(VNC_HEXTILE_TILE_SIZE);
            read_hextile_tile(
                reader,
                &mut bytes,
                rect.width,
                RfbRect {
                    x: tile_x,
                    y: tile_y,
                    width: tile_width,
                    height: tile_height,
                },
                &mut state,
            )?;
        }
    }

    Ok(bytes)
}

pub(super) fn read_hextile_tile(
    reader: &mut impl Read,
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
    state: &mut HextileState,
) -> Result<(), String> {
    let subencoding =
        read_u8(reader).map_err(|error| format!("VNC hextile subencoding read failed: {error}"))?;
    if subencoding & VNC_HEXTILE_RAW != 0 {
        let raw = read_exact_vec(reader, rect_byte_len(tile)?)
            .map_err(|error| format!("VNC hextile raw tile read failed: {error}"))?;
        copy_hextile_tile(target, target_width, tile, &raw)?;
        state.background = None;
        state.foreground = None;
        return Ok(());
    }

    if subencoding & VNC_HEXTILE_BACKGROUND_SPECIFIED != 0 {
        state.background = Some(read_hextile_pixel(reader)?);
    }
    let Some(background) = state.background else {
        return Err("VNC hextile background color is missing.".to_string());
    };
    fill_hextile_area(target, target_width, tile, background)?;

    if subencoding & VNC_HEXTILE_FOREGROUND_SPECIFIED != 0 {
        state.foreground = Some(read_hextile_pixel(reader)?);
    }
    let subrect_count = if subencoding & VNC_HEXTILE_ANY_SUBRECTS != 0 {
        read_u8(reader)
            .map_err(|error| format!("VNC hextile subrect count read failed: {error}"))?
    } else {
        0
    };

    let colored_subrects = subencoding & VNC_HEXTILE_SUBRECTS_COLORED != 0;
    if subrect_count > 0 && !colored_subrects && state.foreground.is_none() {
        return Err("VNC hextile foreground color is missing.".to_string());
    }

    for _ in 0..subrect_count {
        let color = if colored_subrects {
            read_hextile_pixel(reader)?
        } else {
            state
                .foreground
                .ok_or_else(|| "VNC hextile foreground color is missing.".to_string())?
        };
        let position =
            read_u8(reader).map_err(|error| format!("VNC hextile subrect read failed: {error}"))?;
        let size =
            read_u8(reader).map_err(|error| format!("VNC hextile subrect read failed: {error}"))?;
        let subrect = hextile_subrect(tile, position, size)?;
        fill_hextile_area(target, target_width, subrect, color)?;
    }

    if colored_subrects {
        // The foreground color is not meaningful after a colored-subrect tile
        // because every subrect carried its own color.
        state.foreground = None;
    }

    Ok(())
}

pub(super) fn read_hextile_pixel(reader: &mut impl Read) -> Result<[u8; 4], String> {
    read_exact_array::<4, _>(reader)
        .map_err(|error| format!("VNC hextile color read failed: {error}"))
}

pub(super) fn hextile_subrect(tile: RfbRect, position: u8, size: u8) -> Result<RfbRect, String> {
    let local_x = u16::from(position >> 4);
    let local_y = u16::from(position & 0x0f);
    let width = u16::from(size >> 4) + 1;
    let height = u16::from(size & 0x0f) + 1;
    if local_x + width > tile.width || local_y + height > tile.height {
        return Err("VNC hextile subrect exceeds its tile.".to_string());
    }
    Ok(RfbRect {
        x: tile.x + local_x,
        y: tile.y + local_y,
        width,
        height,
    })
}

pub(super) fn copy_hextile_tile(
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
    raw: &[u8],
) -> Result<(), String> {
    let tile_width = usize::from(tile.width);
    let tile_height = usize::from(tile.height);
    if raw.len() < tile_width * tile_height * 4 {
        return Err("VNC hextile raw tile is incomplete.".to_string());
    }
    for row in 0..tile_height {
        let src_start = row * tile_width * 4;
        let src_end = src_start + tile_width * 4;
        let dst_start =
            ((usize::from(tile.y) + row) * usize::from(target_width) + usize::from(tile.x)) * 4;
        let dst_end = dst_start + tile_width * 4;
        let Some(dst) = target.get_mut(dst_start..dst_end) else {
            return Err("VNC hextile raw tile exceeds its target rectangle.".to_string());
        };
        dst.copy_from_slice(&raw[src_start..src_end]);
    }
    Ok(())
}

pub(super) fn fill_hextile_area(
    target: &mut [u8],
    target_width: u16,
    area: RfbRect,
    color: [u8; 4],
) -> Result<(), String> {
    let width = usize::from(area.width);
    let target_width = usize::from(target_width);
    for row in 0..usize::from(area.height) {
        let start = ((usize::from(area.y) + row) * target_width + usize::from(area.x)) * 4;
        let end = start + width * 4;
        let Some(dst_row) = target.get_mut(start..end) else {
            return Err("VNC hextile fill exceeds its target rectangle.".to_string());
        };
        for pixel in dst_row.chunks_exact_mut(4) {
            pixel.copy_from_slice(&color);
        }
    }
    Ok(())
}

pub(super) fn read_zrle_rect(
    reader: &mut impl Read,
    rect: RfbRect,
    decode_state: &mut VncDecodeState,
) -> Result<Vec<u8>, String> {
    let compressed_len = read_be_u32(reader)
        .map_err(|error| format!("VNC ZRLE length read failed: {error}"))?
        as usize;
    if compressed_len > MAX_VNC_FRAME_BYTES {
        return Err("VNC ZRLE rectangle is larger than the helper limit.".to_string());
    }
    let compressed = read_exact_vec(reader, compressed_len)
        .map_err(|error| format!("VNC ZRLE payload read failed: {error}"))?;
    let decompressed = inflate_zrle_payload(
        &mut decode_state.zrle_decompressor,
        &compressed,
        zrle_uncompressed_limit(rect)?,
    )?;
    decode_trle_rect(&decompressed, rect)
}

pub(super) fn inflate_zrle_payload(
    decompressor: &mut Decompress,
    compressed: &[u8],
    output_limit: usize,
) -> Result<Vec<u8>, String> {
    let input_start = decompressor.total_in();
    let mut input_offset = 0usize;
    let mut output = Vec::with_capacity(output_limit.min(64 * 1024));

    while input_offset < compressed.len() {
        let total_in_before = decompressor.total_in();
        let total_out_before = decompressor.total_out();
        let status = decompressor
            .decompress_vec(
                &compressed[input_offset..],
                &mut output,
                FlushDecompress::Sync,
            )
            .map_err(|error| format!("VNC ZRLE inflate failed: {error}"))?;
        input_offset = (decompressor.total_in() - input_start) as usize;
        if output.len() > output_limit {
            return Err("VNC ZRLE rectangle expanded beyond the helper limit.".to_string());
        }
        let consumed = decompressor.total_in() != total_in_before;
        let produced = decompressor.total_out() != total_out_before;
        if input_offset >= compressed.len() {
            break;
        }
        if matches!(status, Status::StreamEnd) {
            return Err(
                "VNC ZRLE stream ended before the rectangle payload was consumed.".to_string(),
            );
        }
        if !consumed && !produced {
            return Err("VNC ZRLE inflater made no progress.".to_string());
        }
    }

    Ok(output)
}

pub(super) fn zrle_uncompressed_limit(rect: RfbRect) -> Result<usize, String> {
    let pixels = usize::from(rect.width)
        .checked_mul(usize::from(rect.height))
        .ok_or_else(|| "VNC ZRLE rectangle dimensions overflowed.".to_string())?;
    let tile_columns = usize::from(rect.width).div_ceil(usize::from(VNC_ZRLE_TILE_SIZE));
    let tile_rows = usize::from(rect.height).div_ceil(usize::from(VNC_ZRLE_TILE_SIZE));
    let tile_count = tile_columns
        .checked_mul(tile_rows)
        .ok_or_else(|| "VNC ZRLE tile count overflowed.".to_string())?;
    pixels
        .checked_mul(4)
        .and_then(|bytes| bytes.checked_add(tile_count * 1024))
        .ok_or_else(|| "VNC ZRLE expanded byte count overflowed.".to_string())
}

pub(super) fn decode_trle_rect(data: &[u8], rect: RfbRect) -> Result<Vec<u8>, String> {
    let mut reader = io::Cursor::new(data);
    let mut bytes = vec![0; rect_byte_len(rect)?];

    for tile_y in (0..rect.height).step_by(VNC_ZRLE_TILE_SIZE as usize) {
        for tile_x in (0..rect.width).step_by(VNC_ZRLE_TILE_SIZE as usize) {
            let tile_width = (rect.width - tile_x).min(VNC_ZRLE_TILE_SIZE);
            let tile_height = (rect.height - tile_y).min(VNC_ZRLE_TILE_SIZE);
            decode_trle_tile(
                &mut reader,
                &mut bytes,
                rect.width,
                RfbRect {
                    x: tile_x,
                    y: tile_y,
                    width: tile_width,
                    height: tile_height,
                },
            )?;
        }
    }

    if reader.position() != data.len() as u64 {
        return Err("VNC ZRLE rectangle has trailing tile bytes.".to_string());
    }

    Ok(bytes)
}

pub(super) fn decode_trle_tile(
    reader: &mut impl Read,
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
) -> Result<(), String> {
    let subencoding =
        read_u8(reader).map_err(|error| format!("VNC ZRLE tile type read failed: {error}"))?;
    // ZRLE wraps the TRLE tile grammar, but unlike TRLE it never allows palette
    // reuse between tiles.
    match subencoding {
        VNC_TRLE_RAW => decode_trle_raw_tile(reader, target, target_width, tile),
        VNC_TRLE_SOLID => {
            let color = read_zrle_cpixel(reader)?;
            fill_hextile_area(target, target_width, tile, color)
        }
        2..=16 => decode_trle_packed_palette(reader, target, target_width, tile, subencoding),
        17..=126 => Err(format!("Unsupported VNC ZRLE tile type {subencoding}.")),
        127 | 129 => Err("VNC ZRLE palette reuse is not valid for ZRLE.".to_string()),
        VNC_TRLE_PLAIN_RLE => decode_trle_plain_rle(reader, target, target_width, tile),
        130..=255 => decode_trle_palette_rle(reader, target, target_width, tile, subencoding - 128),
    }
}

pub(super) fn decode_trle_raw_tile(
    reader: &mut impl Read,
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
) -> Result<(), String> {
    for index in 0..tile_pixel_count(tile) {
        write_trle_tile_pixel(target, target_width, tile, index, read_zrle_cpixel(reader)?)?;
    }
    Ok(())
}

pub(super) fn decode_trle_packed_palette(
    reader: &mut impl Read,
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
    palette_size: u8,
) -> Result<(), String> {
    let palette = read_zrle_palette(reader, palette_size)?;
    let bits_per_index = match palette_size {
        2 => 1,
        3..=4 => 2,
        5..=16 => 4,
        _ => return Err("VNC ZRLE packed palette size is invalid.".to_string()),
    };
    let bytes_per_row = (usize::from(tile.width) * bits_per_index as usize).div_ceil(8);

    for y in 0..tile.height {
        let row = read_exact_vec(reader, bytes_per_row)
            .map_err(|error| format!("VNC ZRLE packed row read failed: {error}"))?;
        for x in 0..tile.width {
            let bit_index = usize::from(x) * bits_per_index as usize;
            let byte = row[bit_index / 8];
            let shift = 8 - bits_per_index - (bit_index % 8) as u8;
            let palette_index = ((byte >> shift) & ((1u8 << bits_per_index) - 1)) as usize;
            let Some(color) = palette.get(palette_index).copied() else {
                return Err("VNC ZRLE packed palette index is invalid.".to_string());
            };
            write_trle_tile_pixel(
                target,
                target_width,
                tile,
                usize::from(y) * usize::from(tile.width) + usize::from(x),
                color,
            )?;
        }
    }

    Ok(())
}

pub(super) fn decode_trle_plain_rle(
    reader: &mut impl Read,
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
) -> Result<(), String> {
    let mut written = 0usize;
    let total = tile_pixel_count(tile);
    while written < total {
        let color = read_zrle_cpixel(reader)?;
        let run_length = read_zrle_run_length(reader)?;
        write_trle_run(target, target_width, tile, written, run_length, color)?;
        written += run_length;
    }
    Ok(())
}

pub(super) fn decode_trle_palette_rle(
    reader: &mut impl Read,
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
    palette_size: u8,
) -> Result<(), String> {
    let palette = read_zrle_palette(reader, palette_size)?;
    let mut written = 0usize;
    let total = tile_pixel_count(tile);
    while written < total {
        let index = read_u8(reader)
            .map_err(|error| format!("VNC ZRLE palette RLE index read failed: {error}"))?;
        let palette_index = usize::from(index & 0x7f);
        let Some(color) = palette.get(palette_index).copied() else {
            return Err("VNC ZRLE palette RLE index is invalid.".to_string());
        };
        let run_length = if index & 0x80 != 0 {
            read_zrle_run_length(reader)?
        } else {
            1
        };
        write_trle_run(target, target_width, tile, written, run_length, color)?;
        written += run_length;
    }
    Ok(())
}

pub(super) fn read_zrle_palette(
    reader: &mut impl Read,
    palette_size: u8,
) -> Result<Vec<[u8; 4]>, String> {
    let mut palette = Vec::with_capacity(usize::from(palette_size));
    for _ in 0..palette_size {
        palette.push(read_zrle_cpixel(reader)?);
    }
    Ok(palette)
}

pub(super) fn read_zrle_cpixel(reader: &mut impl Read) -> Result<[u8; 4], String> {
    let pixel = read_exact_array::<3, _>(reader)
        .map_err(|error| format!("VNC ZRLE compact pixel read failed: {error}"))?;
    // With our negotiated 32-bit little-endian true-color format, CPIXEL omits
    // the unused fourth transport byte and leaves B, G, R in wire order.
    Ok([pixel[0], pixel[1], pixel[2], 0])
}

pub(super) fn read_zrle_run_length(reader: &mut impl Read) -> Result<usize, String> {
    let mut run_length = 1usize;
    loop {
        let byte =
            read_u8(reader).map_err(|error| format!("VNC ZRLE run length read failed: {error}"))?;
        run_length = run_length
            .checked_add(usize::from(byte))
            .ok_or_else(|| "VNC ZRLE run length overflowed.".to_string())?;
        if byte != u8::MAX {
            return Ok(run_length);
        }
    }
}

pub(super) fn tile_pixel_count(tile: RfbRect) -> usize {
    usize::from(tile.width) * usize::from(tile.height)
}

pub(super) fn write_trle_run(
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
    start: usize,
    run_length: usize,
    color: [u8; 4],
) -> Result<(), String> {
    let total = tile_pixel_count(tile);
    let Some(end) = start.checked_add(run_length) else {
        return Err("VNC ZRLE run exceeds its tile.".to_string());
    };
    if end > total {
        return Err("VNC ZRLE run exceeds its tile.".to_string());
    }
    for index in start..end {
        write_trle_tile_pixel(target, target_width, tile, index, color)?;
    }
    Ok(())
}

pub(super) fn write_trle_tile_pixel(
    target: &mut [u8],
    target_width: u16,
    tile: RfbRect,
    index: usize,
    color: [u8; 4],
) -> Result<(), String> {
    let tile_width = usize::from(tile.width);
    let x = usize::from(tile.x) + index % tile_width;
    let y = usize::from(tile.y) + index / tile_width;
    let offset = (y * usize::from(target_width) + x) * 4;
    let Some(pixel) = target.get_mut(offset..offset + 4) else {
        return Err("VNC ZRLE tile pixel exceeds its target rectangle.".to_string());
    };
    pixel.copy_from_slice(&color);
    Ok(())
}
