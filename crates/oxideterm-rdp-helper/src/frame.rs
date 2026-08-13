use ironrdp::{
    graphics::image_processing::PixelFormat,
    pdu::geometry::{InclusiveRectangle, Rectangle as _},
    session::{SessionResult, image::DecodedImage},
};
use std::time::{Duration, Instant};

use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopFrameUpdateBatch, RemoteDesktopHelperEvent, RemoteDesktopRect, RemoteDesktopSize,
};

const RDP_GRAPHICS_ACCUMULATOR_QUIET_WINDOW: Duration = Duration::from_millis(2);
const RDP_GRAPHICS_ACCUMULATOR_MAX_WINDOW: Duration = Duration::from_millis(8);
const RDP_GRAPHICS_ACCUMULATOR_BASE_AREA_DIVISOR: u64 = 3;
pub(crate) const RDP_GRAPHICS_MAX_DIRTY_RECTS: usize = 16;
const RDP_GRAPHICS_ACCUMULATOR_MERGE_INFLATION_LIMIT: u64 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RdpGraphicsSyncState {
    NeedsBase,
    Synced,
}

impl Default for RdpGraphicsSyncState {
    fn default() -> Self {
        Self::NeedsBase
    }
}

impl RdpGraphicsSyncState {
    pub(crate) fn needs_base(self) -> bool {
        self == Self::NeedsBase
    }

    pub(crate) fn mark_needs_base(&mut self) {
        *self = Self::NeedsBase;
    }

    pub(crate) fn mark_synced(&mut self) {
        *self = Self::Synced;
    }
}

#[derive(Debug, Default)]
pub(crate) struct RdpGraphicsFrameAccumulator {
    pending_rects: Vec<RemoteDesktopRect>,
    first_update_at: Option<Instant>,
    quiet_until: Option<Instant>,
    regions: usize,
}

impl RdpGraphicsFrameAccumulator {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn queue_rect(&mut self, rect: RemoteDesktopRect) {
        let now = Instant::now();
        if self.pending_rects.is_empty() {
            self.first_update_at = Some(now);
        }
        self.quiet_until = Some(now + RDP_GRAPHICS_ACCUMULATOR_QUIET_WINDOW);
        self.regions = self.regions.saturating_add(1);
        queue_bounded_dirty_rect(&mut self.pending_rects, rect, RDP_GRAPHICS_MAX_DIRTY_RECTS);
    }

    pub(crate) fn take_ready_rects(&mut self) -> Option<Vec<RemoteDesktopRect>> {
        if !self.ready_to_flush() {
            return None;
        }
        self.take_rects()
    }

    pub(crate) fn take_rects(&mut self) -> Option<Vec<RemoteDesktopRect>> {
        if self.pending_rects.is_empty() {
            return None;
        }
        let rects = std::mem::take(&mut self.pending_rects);
        self.first_update_at = None;
        self.quiet_until = None;
        self.regions = 0;
        Some(rects)
    }

    pub(crate) fn next_flush_delay(&self) -> Option<Duration> {
        if self.pending_rects.is_empty() {
            return None;
        }
        let now = Instant::now();
        let mut deadline = self.quiet_until?;
        if let Some(first_update_at) = self.first_update_at {
            deadline = deadline.min(first_update_at + RDP_GRAPHICS_ACCUMULATOR_MAX_WINDOW);
        }
        Some(deadline.saturating_duration_since(now))
    }

    pub(crate) fn pending_regions(&self) -> usize {
        self.regions
    }

    pub(crate) fn should_promote_to_base(&self, image: &DecodedImage) -> bool {
        if self.pending_rects.is_empty() {
            return false;
        }
        let dirty_pixels = self
            .pending_rects
            .iter()
            .copied()
            .map(rect_pixels)
            .fold(0_u64, u64::saturating_add);
        self.pending_rects
            .iter()
            .copied()
            .any(|rect| rect_covers_image(rect, image))
            || dirty_pixels.saturating_mul(RDP_GRAPHICS_ACCUMULATOR_BASE_AREA_DIVISOR)
                >= image_pixels(image)
    }

    fn ready_to_flush(&self) -> bool {
        self.next_flush_delay().is_some_and(|delay| delay.is_zero())
    }
}

pub(crate) fn queue_bounded_dirty_rect(
    rects: &mut Vec<RemoteDesktopRect>,
    rect: RemoteDesktopRect,
    limit: usize,
) {
    rects.push(rect);
    merge_touching_accumulator_rects(rects);
    merge_accumulator_rects_to_limit(rects, limit);
}

fn merge_touching_accumulator_rects(rects: &mut Vec<RemoteDesktopRect>) {
    let mut index = 0;
    while index < rects.len() {
        let mut candidate = index + 1;
        while candidate < rects.len() {
            if let Some(union) = rects[index].union(rects[candidate])
                && accumulator_rects_touch(rects[index], rects[candidate])
                && rect_pixels(union)
                    <= rect_pixels(rects[index])
                        .saturating_add(rect_pixels(rects[candidate]))
                        .saturating_mul(RDP_GRAPHICS_ACCUMULATOR_MERGE_INFLATION_LIMIT)
            {
                rects[index] = union;
                rects.swap_remove(candidate);
                index = 0;
                candidate = 1;
                continue;
            }
            candidate += 1;
        }
        index += 1;
    }
}

fn merge_accumulator_rects_to_limit(rects: &mut Vec<RemoteDesktopRect>, limit: usize) {
    while rects.len() > limit {
        let mut best_pair = (0, 1);
        let mut best_inflation = u64::MAX;
        for first in 0..rects.len() {
            for second in (first + 1)..rects.len() {
                let Some(union) = rects[first].union(rects[second]) else {
                    continue;
                };
                let inflation = rect_pixels(union).saturating_sub(
                    rect_pixels(rects[first]).saturating_add(rect_pixels(rects[second])),
                );
                if inflation < best_inflation {
                    best_pair = (first, second);
                    best_inflation = inflation;
                }
            }
        }
        let (first, second) = best_pair;
        rects[first] = rects[first].union(rects[second]).unwrap_or(rects[first]);
        rects.swap_remove(second);
    }
}

fn accumulator_rects_touch(first: RemoteDesktopRect, second: RemoteDesktopRect) -> bool {
    first.x <= second.x.saturating_add(second.width)
        && second.x <= first.x.saturating_add(first.width)
        && first.y <= second.y.saturating_add(second.height)
        && second.y <= first.y.saturating_add(first.height)
}

pub(crate) fn graphics_update_rect_event(
    image: &DecodedImage,
    rect: RemoteDesktopRect,
) -> RemoteDesktopHelperEvent {
    let frame_format = frame_format_for_image(image);
    RemoteDesktopHelperEvent::FrameUpdate {
        update: RemoteDesktopFrameUpdate::new(
            remote_size_for_image(image),
            rect,
            frame_format,
            copy_image_rect(image.data(), image.width(), rect, frame_format),
        ),
    }
}

pub(crate) fn graphics_update_rect_for_accumulator(
    image: &DecodedImage,
    region: InclusiveRectangle,
    sync_state: RdpGraphicsSyncState,
) -> SessionResult<Option<RemoteDesktopRect>> {
    let Some(rect) = normalized_update_rect(image, region)? else {
        return Ok(None);
    };
    if sync_state.needs_base() || rect_covers_image(rect, image) {
        return Ok(Some(RemoteDesktopRect::new(
            0,
            0,
            u32::from(image.width()),
            u32::from(image.height()),
        )));
    }
    Ok(Some(rect))
}

pub(crate) fn accumulated_graphics_event(
    image: &DecodedImage,
    rects: Vec<RemoteDesktopRect>,
) -> RemoteDesktopHelperEvent {
    if rects
        .iter()
        .copied()
        .any(|rect| rect_covers_image(rect, image))
    {
        base_frame_event(image)
    } else if rects.len() == 1 {
        graphics_update_rect_event(image, rects[0])
    } else {
        let updates = rects
            .into_iter()
            .map(|rect| {
                let frame_format = frame_format_for_image(image);
                RemoteDesktopFrameUpdate::new(
                    remote_size_for_image(image),
                    rect,
                    frame_format,
                    copy_image_rect(image.data(), image.width(), rect, frame_format),
                )
            })
            .collect();
        RemoteDesktopHelperEvent::FrameUpdateBatch {
            batch: RemoteDesktopFrameUpdateBatch::new(updates),
        }
    }
}

pub(crate) fn base_frame_event(image: &DecodedImage) -> RemoteDesktopHelperEvent {
    let frame_format = frame_format_for_image(image);
    RemoteDesktopHelperEvent::Frame {
        frame: RemoteDesktopFrame::new(
            remote_size_for_image(image),
            frame_format,
            opaque_frame_bytes(image.data(), frame_format),
        ),
    }
}

pub(crate) fn frame_format_for_image(image: &DecodedImage) -> RemoteDesktopFrameFormat {
    match image.pixel_format() {
        // IronRDP's BgrA32 byte order is BGRA, which matches GPUI's upload
        // path and avoids an extra channel swap on every RDP dirty update.
        PixelFormat::BgrA32 | PixelFormat::BgrX32 => RemoteDesktopFrameFormat::Bgra8,
        PixelFormat::RgbA32 | PixelFormat::RgbX32 => RemoteDesktopFrameFormat::Rgba8,
        format => {
            debug_assert!(
                matches!(
                    format,
                    PixelFormat::BgrA32
                        | PixelFormat::BgrX32
                        | PixelFormat::RgbA32
                        | PixelFormat::RgbX32
                ),
                "unexpected RDP decoded image format: {format:?}"
            );
            RemoteDesktopFrameFormat::Rgba8
        }
    }
}

pub(crate) fn remote_size_for_image(image: &DecodedImage) -> RemoteDesktopSize {
    RemoteDesktopSize {
        width: u32::from(image.width()),
        height: u32::from(image.height()),
    }
}

pub(crate) fn normalized_update_rect(
    image: &DecodedImage,
    region: InclusiveRectangle,
) -> SessionResult<Option<RemoteDesktopRect>> {
    if region.right >= image.width()
        || region.bottom >= image.height()
        || region.left > region.right
        || region.top > region.bottom
    {
        // IronRDP can surface a stale region while the desktop size is being
        // renegotiated. Treat it as a dropped dirty update instead of tearing
        // down an otherwise healthy session.
        return Ok(None);
    }
    Ok(Some(RemoteDesktopRect::new(
        u32::from(region.left),
        u32::from(region.top),
        u32::from(region.width()),
        u32::from(region.height()),
    )))
}

pub(crate) fn copy_image_rect(
    frame_bytes: &[u8],
    image_width: u16,
    rect: RemoteDesktopRect,
    format: RemoteDesktopFrameFormat,
) -> Vec<u8> {
    let pixel_size = format.bytes_per_pixel();
    let image_width = usize::from(image_width);
    let rect_x = usize::try_from(rect.x).unwrap_or(usize::MAX);
    let rect_y = usize::try_from(rect.y).unwrap_or(usize::MAX);
    let rect_width = usize::try_from(rect.width).unwrap_or(0);
    let rect_height = usize::try_from(rect.height).unwrap_or(0);
    let mut bytes = Vec::with_capacity(rect_width * rect_height * pixel_size);
    for row in 0..rect_height {
        let start = ((rect_y + row) * image_width + rect_x) * pixel_size;
        let end = start + rect_width * pixel_size;
        bytes.extend_from_slice(&frame_bytes[start..end]);
    }
    set_frame_alpha_opaque(&mut bytes, format);
    bytes
}

pub(crate) fn rect_covers_image(rect: RemoteDesktopRect, image: &DecodedImage) -> bool {
    rect.x == 0
        && rect.y == 0
        && rect.width == u32::from(image.width())
        && rect.height == u32::from(image.height())
}

fn image_pixels(image: &DecodedImage) -> u64 {
    u64::from(image.width()).saturating_mul(u64::from(image.height()))
}

fn rect_pixels(rect: RemoteDesktopRect) -> u64 {
    u64::from(rect.width).saturating_mul(u64::from(rect.height))
}

pub(crate) fn opaque_frame_bytes(bytes: &[u8], format: RemoteDesktopFrameFormat) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    set_frame_alpha_opaque(&mut bytes, format);
    bytes
}

fn set_frame_alpha_opaque(bytes: &mut [u8], format: RemoteDesktopFrameFormat) {
    for pixel in bytes.chunks_exact_mut(format.bytes_per_pixel()) {
        pixel[3] = 0xff;
    }
}
