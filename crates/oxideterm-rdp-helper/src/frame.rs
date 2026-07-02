use ironrdp::{
    graphics::image_processing::PixelFormat,
    pdu::geometry::{InclusiveRectangle, Rectangle as _},
    session::{SessionResult, image::DecodedImage},
};
use std::time::{Duration, Instant};

use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopHelperEvent, RemoteDesktopRect, RemoteDesktopSize,
};

const RDP_GRAPHICS_ACCUMULATOR_QUIET_WINDOW: Duration = Duration::from_millis(2);
const RDP_GRAPHICS_ACCUMULATOR_MAX_WINDOW: Duration = Duration::from_millis(8);
const RDP_GRAPHICS_ACCUMULATOR_BASE_AREA_DIVISOR: u64 = 3;

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
    pending_rect: Option<RemoteDesktopRect>,
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
        if self.pending_rect.is_none() {
            self.first_update_at = Some(now);
        }
        self.quiet_until = Some(now + RDP_GRAPHICS_ACCUMULATOR_QUIET_WINDOW);
        self.regions = self.regions.saturating_add(1);
        self.pending_rect = Some(match self.pending_rect {
            Some(existing) => existing.union(rect).unwrap_or(rect),
            None => rect,
        });
    }

    pub(crate) fn take_ready_rect(&mut self) -> Option<RemoteDesktopRect> {
        if !self.ready_to_flush() {
            return None;
        }
        self.take_rect()
    }

    pub(crate) fn take_rect(&mut self) -> Option<RemoteDesktopRect> {
        let rect = self.pending_rect.take();
        self.first_update_at = None;
        self.quiet_until = None;
        self.regions = 0;
        rect
    }

    pub(crate) fn next_flush_delay(&self) -> Option<Duration> {
        self.pending_rect?;
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
        let Some(rect) = self.pending_rect else {
            return false;
        };
        rect_covers_image(rect, image)
            || rect_pixels(rect).saturating_mul(RDP_GRAPHICS_ACCUMULATOR_BASE_AREA_DIVISOR)
                >= image_pixels(image)
    }

    fn ready_to_flush(&self) -> bool {
        self.next_flush_delay().is_some_and(|delay| delay.is_zero())
    }
}

pub(crate) fn graphics_update_event(
    image: &DecodedImage,
    region: InclusiveRectangle,
    sync_state: &mut RdpGraphicsSyncState,
) -> SessionResult<Option<RemoteDesktopHelperEvent>> {
    let Some(rect) = normalized_update_rect(image, region)? else {
        return Ok(None);
    };

    if sync_state.needs_base() || rect_covers_image(rect, image) {
        // A full decoded image is the only recovery boundary. Dirty rectangles
        // are only safe after this helper has published a complete base frame.
        sync_state.mark_synced();
        return Ok(Some(base_frame_event(image)));
    }

    Ok(Some(graphics_update_rect_event(image, rect)))
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
    rect: RemoteDesktopRect,
) -> RemoteDesktopHelperEvent {
    if rect_covers_image(rect, image) {
        base_frame_event(image)
    } else {
        graphics_update_rect_event(image, rect)
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
