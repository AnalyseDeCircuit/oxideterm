// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::io::{Cursor, Read};

use flate2::{Decompress, FlushDecompress, Status};
use image::{ImageFormat, ImageReader, Limits};

use super::*;

const VNC_TIGHT_STREAM_COUNT: usize = 4;
const VNC_TIGHT_RESET_MASK: u8 = 0x0f;
const VNC_TIGHT_FILL: u8 = 8;
const VNC_TIGHT_JPEG: u8 = 9;
const VNC_TIGHT_EXPLICIT_FILTER: u8 = 4;
const VNC_TIGHT_FILTER_COPY: u8 = 0;
const VNC_TIGHT_FILTER_PALETTE: u8 = 1;
const VNC_TIGHT_FILTER_GRADIENT: u8 = 2;
const VNC_TIGHT_MIN_TO_COMPRESS: usize = 12;
const VNC_TIGHT_MAX_WIDTH: u16 = 2048;
const VNC_TIGHT_MAX_COMPACT_LENGTH: usize = 0x3f_ffff;

const VNC_CLIENT_FENCE_MESSAGE_TYPE: u8 = 248;
const VNC_ENABLE_CONTINUOUS_UPDATES_MESSAGE_TYPE: u8 = 150;
const VNC_FENCE_FLAG_BLOCK_BEFORE: u32 = 1;
const VNC_FENCE_FLAG_BLOCK_AFTER: u32 = 2;
#[cfg(test)]
const VNC_FENCE_FLAG_SYNC_NEXT: u32 = 4;
const VNC_FENCE_FLAG_REQUEST: u32 = 1 << 31;
const VNC_FENCE_SUPPORTED_RESPONSE_FLAGS: u32 =
    VNC_FENCE_FLAG_BLOCK_BEFORE | VNC_FENCE_FLAG_BLOCK_AFTER;
const VNC_FENCE_MAX_PAYLOAD_BYTES: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VncObservedCapability {
    Tight,
    Jpeg,
    H264,
    LastRect,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct VncObservedCapabilities {
    tight: bool,
    jpeg: bool,
    h264: bool,
    last_rect: bool,
    fence: bool,
    continuous_updates: bool,
}

/// Promotes only capabilities proven by server traffic and preserves any
/// explicit rejection recorded by another protocol path.
pub(super) fn observe_vnc_performance_capabilities(
    event: &VncServerEvent,
    capabilities: &SharedVncCapabilities,
) -> Result<Option<RemoteDesktopHelperEvent>, String> {
    let mut observed = VncObservedCapabilities::default();
    collect_vnc_performance_capabilities(event, &mut observed);
    update_vnc_capabilities(capabilities, |snapshot| {
        promote_supported(&mut snapshot.tight, observed.tight);
        promote_supported(&mut snapshot.jpeg, observed.jpeg);
        promote_supported(&mut snapshot.h264, observed.h264);
        promote_supported(&mut snapshot.last_rect, observed.last_rect);
        promote_supported(&mut snapshot.fence, observed.fence);
        promote_supported(
            &mut snapshot.continuous_updates,
            observed.continuous_updates,
        );
    })
}

fn collect_vnc_performance_capabilities(
    event: &VncServerEvent,
    observed: &mut VncObservedCapabilities,
) {
    match event {
        VncServerEvent::ObservedCapability(capability) => match capability {
            VncObservedCapability::Tight => observed.tight = true,
            VncObservedCapability::Jpeg => observed.jpeg = true,
            VncObservedCapability::H264 => observed.h264 = true,
            VncObservedCapability::LastRect => observed.last_rect = true,
        },
        VncServerEvent::ServerFence(_) => observed.fence = true,
        VncServerEvent::EndOfContinuousUpdates => observed.continuous_updates = true,
        VncServerEvent::Batch(events) => {
            for event in events {
                collect_vnc_performance_capabilities(event, observed);
            }
        }
        _ => {}
    }
}

fn promote_supported(status: &mut NegotiatedCapabilityStatus, observed: bool) {
    if observed && *status == NegotiatedCapabilityStatus::Unknown {
        *status = NegotiatedCapabilityStatus::Supported;
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) struct VncTightRectangle {
    pub(super) bgra: Vec<u8>,
    pub(super) used_jpeg: bool,
}

pub(super) struct VncTightState {
    streams: [Decompress; VNC_TIGHT_STREAM_COUNT],
}

impl Default for VncTightState {
    fn default() -> Self {
        Self {
            // Tight keeps four independent zlib streams for the entire RFB
            // connection. Rectangle reset bits are the only reset boundary.
            streams: std::array::from_fn(|_| Decompress::new(true)),
        }
    }
}

pub(super) fn read_tight_rect(
    reader: &mut impl Read,
    rect: RfbRect,
    state: &mut VncTightState,
) -> Result<VncTightRectangle, String> {
    if rect.width == 0 || rect.height == 0 {
        return Err("VNC Tight rectangle has an empty area.".to_string());
    }
    if rect.width > VNC_TIGHT_MAX_WIDTH {
        return Err(format!(
            "VNC Tight rectangle width exceeds {VNC_TIGHT_MAX_WIDTH} pixels."
        ));
    }

    let control = read_u8(reader)
        .map_err(|error| format!("VNC Tight compression control read failed: {error}"))?;
    let reset_flags = control & VNC_TIGHT_RESET_MASK;
    for stream_index in 0..VNC_TIGHT_STREAM_COUNT {
        if reset_flags & (1 << stream_index) != 0 {
            state.streams[stream_index].reset(true);
        }
    }

    let subencoding = control >> 4;
    match subencoding {
        VNC_TIGHT_FILL => read_tight_fill(reader, rect),
        VNC_TIGHT_JPEG => read_tight_jpeg(reader, rect),
        0..=7 => read_tight_basic(reader, rect, state, subencoding),
        other => Err(format!("Unsupported VNC Tight subencoding {other}.")),
    }
}

fn read_tight_fill(reader: &mut impl Read, rect: RfbRect) -> Result<VncTightRectangle, String> {
    let rgb = read_exact_array::<3, _>(reader)
        .map_err(|error| format!("VNC Tight fill color read failed: {error}"))?;
    let mut bgra = vec![0; rect_byte_len(rect)?];
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[rgb[2], rgb[1], rgb[0], u8::MAX]);
    }
    Ok(VncTightRectangle {
        bgra,
        used_jpeg: false,
    })
}

fn read_tight_jpeg(reader: &mut impl Read, rect: RfbRect) -> Result<VncTightRectangle, String> {
    let payload_len = read_tight_compact_length(reader)?;
    if payload_len == 0 || payload_len > MAX_VNC_FRAME_BYTES {
        return Err("VNC Tight JPEG payload length is invalid.".to_string());
    }
    let payload = read_exact_vec(reader, payload_len)
        .map_err(|error| format!("VNC Tight JPEG payload read failed: {error}"))?;
    let mut image_reader = ImageReader::with_format(Cursor::new(payload), ImageFormat::Jpeg);
    let mut limits = Limits::default();
    limits.max_image_width = Some(u32::from(rect.width));
    limits.max_image_height = Some(u32::from(rect.height));
    limits.max_alloc = Some(MAX_VNC_FRAME_BYTES as u64);
    image_reader.limits(limits);
    let rgba = image_reader
        .decode()
        .map_err(|error| format!("VNC Tight JPEG decode failed: {error}"))?
        .into_rgba8();
    if rgba.dimensions() != (u32::from(rect.width), u32::from(rect.height)) {
        return Err("VNC Tight JPEG dimensions do not match its rectangle.".to_string());
    }
    let mut bgra = rgba.into_raw();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        pixel[3] = u8::MAX;
    }
    Ok(VncTightRectangle {
        bgra,
        used_jpeg: true,
    })
}

fn read_tight_basic(
    reader: &mut impl Read,
    rect: RfbRect,
    state: &mut VncTightState,
    subencoding: u8,
) -> Result<VncTightRectangle, String> {
    let stream_index = usize::from(subencoding & 0x03);
    let filter = if subencoding & VNC_TIGHT_EXPLICIT_FILTER != 0 {
        read_u8(reader)
            .map_err(|error| format!("VNC Tight filter identifier read failed: {error}"))?
    } else {
        VNC_TIGHT_FILTER_COPY
    };

    let (palette, filtered_len) = match filter {
        VNC_TIGHT_FILTER_COPY | VNC_TIGHT_FILTER_GRADIENT => {
            (Vec::new(), tight_rgb_byte_len(rect)?)
        }
        VNC_TIGHT_FILTER_PALETTE => {
            let color_count = usize::from(
                read_u8(reader)
                    .map_err(|error| format!("VNC Tight palette size read failed: {error}"))?,
            ) + 1;
            let palette_len = color_count
                .checked_mul(3)
                .ok_or_else(|| "VNC Tight palette length overflowed.".to_string())?;
            let palette = read_exact_vec(reader, palette_len)
                .map_err(|error| format!("VNC Tight palette read failed: {error}"))?;
            let index_len = tight_palette_index_len(rect, color_count)?;
            (palette, index_len)
        }
        other => return Err(format!("Unsupported VNC Tight filter {other}.")),
    };

    let filtered = if filtered_len < VNC_TIGHT_MIN_TO_COMPRESS {
        read_exact_vec(reader, filtered_len)
            .map_err(|error| format!("VNC Tight uncompressed data read failed: {error}"))?
    } else {
        let compressed_len = read_tight_compact_length(reader)?;
        if compressed_len == 0 || compressed_len > MAX_VNC_FRAME_BYTES {
            return Err("VNC Tight compressed payload length is invalid.".to_string());
        }
        let compressed = read_exact_vec(reader, compressed_len)
            .map_err(|error| format!("VNC Tight compressed data read failed: {error}"))?;
        inflate_tight_segment(&mut state.streams[stream_index], &compressed, filtered_len)?
    };

    let bgra = match filter {
        VNC_TIGHT_FILTER_COPY => tight_copy_to_bgra(rect, &filtered)?,
        VNC_TIGHT_FILTER_PALETTE => tight_palette_to_bgra(rect, &palette, &filtered)?,
        VNC_TIGHT_FILTER_GRADIENT => tight_gradient_to_bgra(rect, &filtered)?,
        _ => unreachable!("validated Tight filter"),
    };
    Ok(VncTightRectangle {
        bgra,
        used_jpeg: false,
    })
}

pub(super) fn read_tight_compact_length(reader: &mut impl Read) -> Result<usize, String> {
    let first = read_u8(reader)
        .map_err(|error| format!("VNC Tight compact length read failed: {error}"))?;
    let mut value = usize::from(first & 0x7f);
    if first & 0x80 == 0 {
        return Ok(value);
    }

    let second = read_u8(reader)
        .map_err(|error| format!("VNC Tight compact length read failed: {error}"))?;
    value |= usize::from(second & 0x7f) << 7;
    if second & 0x80 == 0 {
        return Ok(value);
    }

    let third = read_u8(reader)
        .map_err(|error| format!("VNC Tight compact length read failed: {error}"))?;
    value |= usize::from(third) << 14;
    if value > VNC_TIGHT_MAX_COMPACT_LENGTH {
        return Err("VNC Tight compact length exceeds the protocol limit.".to_string());
    }
    Ok(value)
}

fn inflate_tight_segment(
    stream: &mut Decompress,
    compressed: &[u8],
    expected_len: usize,
) -> Result<Vec<u8>, String> {
    if expected_len > MAX_VNC_FRAME_BYTES {
        return Err("VNC Tight output exceeds the helper frame limit.".to_string());
    }
    let output_capacity = expected_len
        .checked_add(1)
        .ok_or_else(|| "VNC Tight zlib output length overflowed.".to_string())?;
    // Keep one spare byte so zlib can consume a trailing sync-flush marker
    // even when the expected image bytes exactly fill the output.
    let mut output = vec![0; output_capacity];
    let mut input_offset = 0;
    let mut output_offset = 0;

    while input_offset < compressed.len() || output_offset < expected_len {
        let input_before = stream.total_in();
        let output_before = stream.total_out();
        let status = stream
            .decompress(
                &compressed[input_offset..],
                &mut output[output_offset..],
                FlushDecompress::Sync,
            )
            .map_err(|error| format!("VNC Tight zlib decode failed: {error}"))?;
        let consumed = usize::try_from(stream.total_in() - input_before)
            .map_err(|_| "VNC Tight zlib input count overflowed.".to_string())?;
        let produced = usize::try_from(stream.total_out() - output_before)
            .map_err(|_| "VNC Tight zlib output count overflowed.".to_string())?;
        input_offset = input_offset
            .checked_add(consumed)
            .ok_or_else(|| "VNC Tight zlib input offset overflowed.".to_string())?;
        output_offset = output_offset
            .checked_add(produced)
            .ok_or_else(|| "VNC Tight zlib output offset overflowed.".to_string())?;

        if status == Status::StreamEnd {
            return Err("VNC Tight zlib stream ended unexpectedly.".to_string());
        }
        if output_offset > expected_len {
            return Err("VNC Tight zlib output exceeds the expected length.".to_string());
        }
        if consumed == 0 && produced == 0 {
            break;
        }
    }

    if input_offset != compressed.len() || output_offset != expected_len {
        return Err("VNC Tight zlib output length is invalid.".to_string());
    }
    output.truncate(expected_len);
    Ok(output)
}

fn tight_rgb_byte_len(rect: RfbRect) -> Result<usize, String> {
    usize::from(rect.width)
        .checked_mul(usize::from(rect.height))
        .and_then(|pixels| pixels.checked_mul(3))
        .filter(|bytes| *bytes <= MAX_VNC_FRAME_BYTES)
        .ok_or_else(|| "VNC Tight RGB byte count exceeds the helper limit.".to_string())
}

fn tight_palette_index_len(rect: RfbRect, color_count: usize) -> Result<usize, String> {
    let row_bytes = if color_count == 2 {
        usize::from(rect.width)
            .checked_add(7)
            .ok_or_else(|| "VNC Tight palette row length overflowed.".to_string())?
            / 8
    } else {
        usize::from(rect.width)
    };
    row_bytes
        .checked_mul(usize::from(rect.height))
        .filter(|bytes| *bytes <= MAX_VNC_FRAME_BYTES)
        .ok_or_else(|| "VNC Tight palette index length exceeds the helper limit.".to_string())
}

fn tight_copy_to_bgra(rect: RfbRect, rgb: &[u8]) -> Result<Vec<u8>, String> {
    if rgb.len() != tight_rgb_byte_len(rect)? {
        return Err("VNC Tight copy-filter output length is invalid.".to_string());
    }
    let mut bgra = vec![0; rect_byte_len(rect)?];
    for (source, target) in rgb.chunks_exact(3).zip(bgra.chunks_exact_mut(4)) {
        target.copy_from_slice(&[source[2], source[1], source[0], u8::MAX]);
    }
    Ok(bgra)
}

fn tight_palette_to_bgra(rect: RfbRect, palette: &[u8], indexes: &[u8]) -> Result<Vec<u8>, String> {
    let color_count = palette.len() / 3;
    if palette.is_empty() || palette.len() % 3 != 0 || color_count > 256 {
        return Err("VNC Tight palette is invalid.".to_string());
    }
    if indexes.len() != tight_palette_index_len(rect, color_count)? {
        return Err("VNC Tight palette index length is invalid.".to_string());
    }

    let mut bgra = vec![0; rect_byte_len(rect)?];
    let binary = color_count == 2;
    let row_bytes = if binary {
        (usize::from(rect.width) + 7) / 8
    } else {
        usize::from(rect.width)
    };
    for y in 0..usize::from(rect.height) {
        for x in 0..usize::from(rect.width) {
            let color_index = if binary {
                usize::from((indexes[y * row_bytes + x / 8] >> (7 - x % 8)) & 1)
            } else {
                usize::from(indexes[y * row_bytes + x])
            };
            let color_start = color_index
                .checked_mul(3)
                .filter(|offset| *offset + 2 < palette.len())
                .ok_or_else(|| "VNC Tight palette index is out of range.".to_string())?;
            let target = (y * usize::from(rect.width) + x) * 4;
            bgra[target] = palette[color_start + 2];
            bgra[target + 1] = palette[color_start + 1];
            bgra[target + 2] = palette[color_start];
            bgra[target + 3] = u8::MAX;
        }
    }
    Ok(bgra)
}

fn tight_gradient_to_bgra(rect: RfbRect, differences: &[u8]) -> Result<Vec<u8>, String> {
    if differences.len() != tight_rgb_byte_len(rect)? {
        return Err("VNC Tight gradient-filter output length is invalid.".to_string());
    }
    let width = usize::from(rect.width);
    let mut previous_row = vec![[0u8; 3]; width];
    let mut current_row = vec![[0u8; 3]; width];
    let mut bgra = vec![0; rect_byte_len(rect)?];

    for y in 0..usize::from(rect.height) {
        for x in 0..width {
            let source = (y * width + x) * 3;
            for channel in 0..3 {
                let left = if x == 0 {
                    0
                } else {
                    current_row[x - 1][channel]
                };
                let upper = previous_row[x][channel];
                let upper_left = if x == 0 {
                    0
                } else {
                    previous_row[x - 1][channel]
                };
                let prediction = (i16::from(left) + i16::from(upper) - i16::from(upper_left))
                    .clamp(0, 255) as u8;
                current_row[x][channel] = differences[source + channel].wrapping_add(prediction);
            }
            let target = (y * width + x) * 4;
            bgra[target] = current_row[x][2];
            bgra[target + 1] = current_row[x][1];
            bgra[target + 2] = current_row[x][0];
            bgra[target + 3] = u8::MAX;
        }
        std::mem::swap(&mut previous_row, &mut current_row);
        current_row.fill([0; 3]);
    }
    Ok(bgra)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VncServerFence {
    pub(super) flags: u32,
    pub(super) payload: Vec<u8>,
}

pub(super) fn read_server_fence(reader: &mut impl Read) -> Result<VncServerFence, String> {
    let _padding = read_exact_array::<3, _>(reader)
        .map_err(|error| format!("VNC Fence padding read failed: {error}"))?;
    let flags =
        read_be_u32(reader).map_err(|error| format!("VNC Fence flags read failed: {error}"))?;
    let payload_len = usize::from(
        read_u8(reader).map_err(|error| format!("VNC Fence length read failed: {error}"))?,
    );
    if payload_len > VNC_FENCE_MAX_PAYLOAD_BYTES {
        return Err("VNC Fence payload exceeds the 64-byte protocol limit.".to_string());
    }
    let payload = read_exact_vec(reader, payload_len)
        .map_err(|error| format!("VNC Fence payload read failed: {error}"))?;
    Ok(VncServerFence { flags, payload })
}

pub(super) fn client_fence_response_message(fence: &VncServerFence) -> Option<Vec<u8>> {
    if fence.flags & VNC_FENCE_FLAG_REQUEST == 0 {
        return None;
    }
    let flags = fence.flags & VNC_FENCE_SUPPORTED_RESPONSE_FLAGS;
    let mut message = Vec::with_capacity(9 + fence.payload.len());
    message.push(VNC_CLIENT_FENCE_MESSAGE_TYPE);
    message.extend_from_slice(&[0, 0, 0]);
    push_be_u32(&mut message, flags);
    message.push(fence.payload.len() as u8);
    message.extend_from_slice(&fence.payload);
    Some(message)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct VncContinuousUpdatesState {
    supported: bool,
    active: bool,
}

impl VncContinuousUpdatesState {
    pub(super) fn observe_end_of_continuous_updates(&mut self) -> VncContinuousUpdatesAction {
        if !self.supported {
            self.supported = true;
            self.active = true;
            VncContinuousUpdatesAction::Enable
        } else if self.active {
            // An unsolicited end after enable means the server left continuous
            // mode, so resume ordinary incremental polling.
            self.active = false;
            VncContinuousUpdatesAction::ResumePolling
        } else {
            VncContinuousUpdatesAction::ResumePolling
        }
    }

    pub(super) fn is_active(self) -> bool {
        self.active
    }

    fn disable(&mut self) -> bool {
        std::mem::take(&mut self.active)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VncContinuousUpdatesAction {
    Enable,
    ResumePolling,
}

pub(super) fn enable_continuous_updates_message(enable: bool, width: u16, height: u16) -> Vec<u8> {
    let mut message = Vec::with_capacity(10);
    message.push(VNC_ENABLE_CONTINUOUS_UPDATES_MESSAGE_TYPE);
    message.push(u8::from(enable));
    push_be_u16(&mut message, 0);
    push_be_u16(&mut message, 0);
    push_be_u16(&mut message, width);
    push_be_u16(&mut message, height);
    message
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct VncPerformanceState {
    continuous_updates: VncContinuousUpdatesState,
}

impl VncPerformanceState {
    /// Produces ordered client control messages for one fully decoded server
    /// event. The single I/O owner writes them before reading more server data.
    pub(super) fn observe_server_event(
        &mut self,
        event: &VncServerEvent,
        width: u16,
        height: u16,
    ) -> Vec<Vec<u8>> {
        let mut messages = Vec::new();
        self.collect_control_messages(event, width, height, &mut messages);
        messages
    }

    fn collect_control_messages(
        &mut self,
        event: &VncServerEvent,
        width: u16,
        height: u16,
        messages: &mut Vec<Vec<u8>>,
    ) {
        match event {
            VncServerEvent::ServerFence(fence) => {
                if let Some(response) = client_fence_response_message(fence) {
                    messages.push(response);
                }
            }
            VncServerEvent::EndOfContinuousUpdates => {
                if self.continuous_updates.observe_end_of_continuous_updates()
                    == VncContinuousUpdatesAction::Enable
                {
                    messages.push(enable_continuous_updates_message(true, width, height));
                }
            }
            VncServerEvent::Batch(events) => {
                for event in events {
                    self.collect_control_messages(event, width, height, messages);
                }
            }
            _ => {}
        }
    }

    pub(super) fn continuous_updates_active(&self) -> bool {
        self.continuous_updates.is_active()
    }

    pub(super) fn framebuffer_resized_message(&self, width: u16, height: u16) -> Option<Vec<u8>> {
        // Re-sending EnableContinuousUpdates changes the active update region
        // without briefly falling back to request/response polling.
        self.continuous_updates_active()
            .then(|| enable_continuous_updates_message(true, width, height))
    }

    pub(super) fn disable_continuous_updates_message(
        &mut self,
        width: u16,
        height: u16,
    ) -> Option<Vec<u8>> {
        self.continuous_updates
            .disable()
            .then(|| enable_continuous_updates_message(false, width, height))
    }
}

#[cfg(test)]
pub(super) const TEST_VNC_FENCE_FLAG_REQUEST: u32 = VNC_FENCE_FLAG_REQUEST;
#[cfg(test)]
pub(super) const TEST_VNC_FENCE_FLAG_SYNC_NEXT: u32 = VNC_FENCE_FLAG_SYNC_NEXT;

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use flate2::{Compress, Compression, FlushCompress};
    use image::{ExtendedColorType, codecs::jpeg::JpegEncoder};

    use super::*;

    fn rect(width: u16, height: u16) -> RfbRect {
        RfbRect {
            x: 0,
            y: 0,
            width,
            height,
        }
    }

    fn compact_length(value: usize) -> Vec<u8> {
        let mut bytes = vec![(value & 0x7f) as u8];
        if value > 0x7f {
            bytes[0] |= 0x80;
            bytes.push(((value >> 7) & 0x7f) as u8);
        }
        if value > 0x3fff {
            bytes[1] |= 0x80;
            bytes.push((value >> 14) as u8);
        }
        bytes
    }

    fn tight_zlib_payload(compressor: &mut Compress, control: u8, filtered: &[u8]) -> Vec<u8> {
        let mut compressed = Vec::with_capacity(128);
        compressor
            .compress_vec(filtered, &mut compressed, FlushCompress::Sync)
            .unwrap();
        let mut payload = vec![control];
        payload.extend(compact_length(compressed.len()));
        payload.extend(compressed);
        payload
    }

    #[test]
    fn tight_compact_length_reads_all_protocol_widths() {
        assert_eq!(
            read_tight_compact_length(&mut Cursor::new(compact_length(100))).unwrap(),
            100
        );
        assert_eq!(
            read_tight_compact_length(&mut Cursor::new(compact_length(10_000))).unwrap(),
            10_000
        );
        assert_eq!(
            read_tight_compact_length(&mut Cursor::new(compact_length(1_000_000))).unwrap(),
            1_000_000
        );
    }

    #[test]
    fn tight_fill_converts_rgb_to_opaque_bgra() {
        let tight = read_tight_rect(
            &mut Cursor::new([VNC_TIGHT_FILL << 4, 0x10, 0x20, 0x30]),
            rect(2, 1),
            &mut VncTightState::default(),
        )
        .unwrap();

        assert_eq!(
            tight.bgra,
            vec![0x30, 0x20, 0x10, 0xff, 0x30, 0x20, 0x10, 0xff]
        );
        assert!(!tight.used_jpeg);
    }

    #[test]
    fn tight_copy_filter_accepts_small_uncompressed_payload() {
        let tight = read_tight_rect(
            &mut Cursor::new([0, 0x11, 0x22, 0x33]),
            rect(1, 1),
            &mut VncTightState::default(),
        )
        .unwrap();

        assert_eq!(tight.bgra, vec![0x33, 0x22, 0x11, 0xff]);
    }

    #[test]
    fn tight_copy_filter_inflates_a_persistent_zlib_segment() {
        let first_rgb = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
        let second_rgb = [12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1];
        let mut compressor = Compress::new(Compression::default(), true);
        let first_payload = tight_zlib_payload(&mut compressor, 1, &first_rgb);
        let second_payload = tight_zlib_payload(&mut compressor, 0, &second_rgb);
        let mut state = VncTightState::default();

        let first =
            read_tight_rect(&mut Cursor::new(first_payload), rect(2, 2), &mut state).unwrap();
        let second =
            read_tight_rect(&mut Cursor::new(second_payload), rect(2, 2), &mut state).unwrap();

        assert_eq!(
            first.bgra,
            vec![
                3, 2, 1, 0xff, 6, 5, 4, 0xff, 9, 8, 7, 0xff, 12, 11, 10, 0xff
            ]
        );
        assert_eq!(
            second.bgra,
            vec![
                10, 11, 12, 0xff, 7, 8, 9, 0xff, 4, 5, 6, 0xff, 1, 2, 3, 0xff
            ]
        );
    }

    #[test]
    fn tight_two_color_palette_uses_row_aligned_bits() {
        let payload = [
            VNC_TIGHT_EXPLICIT_FILTER << 4,
            VNC_TIGHT_FILTER_PALETTE,
            1,
            0x10,
            0x20,
            0x30,
            0x40,
            0x50,
            0x60,
            0b0100_0000,
        ];
        let tight = read_tight_rect(
            &mut Cursor::new(payload),
            rect(2, 1),
            &mut VncTightState::default(),
        )
        .unwrap();

        assert_eq!(
            tight.bgra,
            vec![0x30, 0x20, 0x10, 0xff, 0x60, 0x50, 0x40, 0xff]
        );
    }

    #[test]
    fn tight_gradient_reconstructs_neighbor_predictions() {
        let payload = [
            VNC_TIGHT_EXPLICIT_FILTER << 4,
            VNC_TIGHT_FILTER_GRADIENT,
            10,
            20,
            30,
            5,
            5,
            5,
        ];
        let tight = read_tight_rect(
            &mut Cursor::new(payload),
            rect(2, 1),
            &mut VncTightState::default(),
        )
        .unwrap();

        assert_eq!(tight.bgra, vec![30, 20, 10, 0xff, 35, 25, 15, 0xff]);
    }

    #[test]
    fn tight_jpeg_decodes_to_rectangle_dimensions() {
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 100)
            .encode(&[220, 30, 10, 10, 200, 40], 2, 1, ExtendedColorType::Rgb8)
            .unwrap();
        let mut payload = vec![VNC_TIGHT_JPEG << 4];
        payload.extend(compact_length(jpeg.len()));
        payload.extend(jpeg);

        let tight = read_tight_rect(
            &mut Cursor::new(payload),
            rect(2, 1),
            &mut VncTightState::default(),
        )
        .unwrap();

        assert_eq!(tight.bgra.len(), 8);
        assert!(tight.used_jpeg);
        assert_eq!(tight.bgra[3], 0xff);
        assert_eq!(tight.bgra[7], 0xff);
    }

    #[test]
    fn tight_jpeg_rejects_dimensions_outside_the_rectangle() {
        let mut jpeg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode(&[0; 12], 2, 2, ExtendedColorType::Rgb8)
            .unwrap();
        let mut payload = vec![VNC_TIGHT_JPEG << 4];
        payload.extend(compact_length(jpeg.len()));
        payload.extend(jpeg);

        assert!(
            read_tight_rect(
                &mut Cursor::new(payload),
                rect(1, 1),
                &mut VncTightState::default(),
            )
            .unwrap_err()
            .contains("decode failed")
        );
    }

    #[test]
    fn fence_request_response_clears_request_and_unsupported_flags() {
        let fence = VncServerFence {
            flags: TEST_VNC_FENCE_FLAG_REQUEST
                | VNC_FENCE_FLAG_BLOCK_BEFORE
                | TEST_VNC_FENCE_FLAG_SYNC_NEXT,
            payload: b"token".to_vec(),
        };

        let response = client_fence_response_message(&fence).unwrap();

        assert_eq!(response[0], VNC_CLIENT_FENCE_MESSAGE_TYPE);
        assert_eq!(be_u32(&response[4..8]), VNC_FENCE_FLAG_BLOCK_BEFORE);
        assert_eq!(&response[9..], b"token");
    }

    #[test]
    fn fence_rejects_payloads_beyond_the_protocol_limit() {
        let mut payload = vec![0; 7];
        payload.push(65);

        assert!(
            read_server_fence(&mut Cursor::new(payload))
                .unwrap_err()
                .contains("64-byte")
        );
    }

    #[test]
    fn continuous_updates_enable_once_then_resume_polling_on_end() {
        let mut state = VncContinuousUpdatesState::default();

        assert_eq!(
            state.observe_end_of_continuous_updates(),
            VncContinuousUpdatesAction::Enable
        );
        assert!(state.is_active());
        assert_eq!(
            state.observe_end_of_continuous_updates(),
            VncContinuousUpdatesAction::ResumePolling
        );
        assert!(!state.is_active());
    }

    #[test]
    fn continuous_updates_message_covers_the_current_framebuffer() {
        assert_eq!(
            enable_continuous_updates_message(true, 800, 600),
            vec![150, 1, 0, 0, 0, 0, 3, 32, 2, 88]
        );
    }

    #[test]
    fn performance_state_replies_to_fence_before_future_reads() {
        let event = VncServerEvent::ServerFence(VncServerFence {
            flags: TEST_VNC_FENCE_FLAG_REQUEST | VNC_FENCE_FLAG_BLOCK_AFTER,
            payload: b"ordered".to_vec(),
        });
        let mut state = VncPerformanceState::default();

        let messages = state.observe_server_event(&event, 800, 600);

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0][0], VNC_CLIENT_FENCE_MESSAGE_TYPE);
        assert_eq!(&messages[0][9..], b"ordered");
    }

    #[test]
    fn performance_state_stops_polling_only_after_server_confirmation() {
        let mut state = VncPerformanceState::default();
        assert!(!state.continuous_updates_active());

        let messages =
            state.observe_server_event(&VncServerEvent::EndOfContinuousUpdates, 800, 600);

        assert_eq!(
            messages,
            vec![enable_continuous_updates_message(true, 800, 600)]
        );
        assert!(state.continuous_updates_active());

        assert!(
            state
                .observe_server_event(&VncServerEvent::EndOfContinuousUpdates, 800, 600)
                .is_empty()
        );
        assert!(!state.continuous_updates_active());
    }

    #[test]
    fn performance_state_emits_disable_only_while_continuous_updates_are_active() {
        let mut state = VncPerformanceState::default();
        state.observe_server_event(&VncServerEvent::EndOfContinuousUpdates, 800, 600);

        assert_eq!(
            state.disable_continuous_updates_message(800, 600),
            Some(enable_continuous_updates_message(false, 800, 600))
        );
        assert_eq!(state.disable_continuous_updates_message(800, 600), None);
    }

    #[test]
    fn server_observations_promote_a_cumulative_capability_snapshot() {
        let capabilities = Arc::new(Mutex::new(NegotiatedCapabilities::default()));
        let event = VncServerEvent::Batch(vec![
            VncServerEvent::ObservedCapability(VncObservedCapability::Tight),
            VncServerEvent::ObservedCapability(VncObservedCapability::Jpeg),
            VncServerEvent::ObservedCapability(VncObservedCapability::LastRect),
        ]);

        let update = observe_vnc_performance_capabilities(&event, &capabilities)
            .unwrap()
            .unwrap();
        let RemoteDesktopHelperEvent::CapabilitiesNegotiated { capabilities } = update else {
            panic!("expected capability update");
        };
        assert_eq!(capabilities.tight, NegotiatedCapabilityStatus::Supported);
        assert_eq!(capabilities.jpeg, NegotiatedCapabilityStatus::Supported);
        assert_eq!(
            capabilities.last_rect,
            NegotiatedCapabilityStatus::Supported
        );
        assert_eq!(capabilities.h264, NegotiatedCapabilityStatus::Unknown);
    }

    #[test]
    fn server_observation_does_not_overwrite_explicit_rejection() {
        let capabilities = Arc::new(Mutex::new(NegotiatedCapabilities {
            h264: NegotiatedCapabilityStatus::Unsupported,
            ..NegotiatedCapabilities::default()
        }));
        let event = VncServerEvent::ObservedCapability(VncObservedCapability::H264);

        assert!(
            observe_vnc_performance_capabilities(&event, &capabilities)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            capabilities.lock().unwrap().h264,
            NegotiatedCapabilityStatus::Unsupported
        );
    }

    #[test]
    fn server_control_messages_prove_fence_and_continuous_updates() {
        let capabilities = Arc::new(Mutex::new(NegotiatedCapabilities::default()));
        let event = VncServerEvent::Batch(vec![
            VncServerEvent::ServerFence(VncServerFence {
                flags: 0,
                payload: Vec::new(),
            }),
            VncServerEvent::EndOfContinuousUpdates,
        ]);

        observe_vnc_performance_capabilities(&event, &capabilities)
            .unwrap()
            .unwrap();
        let capabilities = capabilities.lock().unwrap();
        assert_eq!(capabilities.fence, NegotiatedCapabilityStatus::Supported);
        assert_eq!(
            capabilities.continuous_updates,
            NegotiatedCapabilityStatus::Supported
        );
    }
}
