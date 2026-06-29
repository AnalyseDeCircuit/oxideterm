use ironrdp::{
    pdu::geometry::{InclusiveRectangle, Rectangle as _},
    session::{SessionResult, image::DecodedImage},
};
use oxideterm_remote_desktop::{
    RemoteDesktopFrame, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopHelperEvent, RemoteDesktopRect, RemoteDesktopSize,
};

pub(crate) fn graphics_update_event(
    image: &DecodedImage,
    region: InclusiveRectangle,
    sent_initial_frame: &mut bool,
) -> SessionResult<Option<RemoteDesktopHelperEvent>> {
    let Some(rect) = normalized_update_rect(image, region)? else {
        return Ok(None);
    };
    if !*sent_initial_frame {
        if !rect_covers_image(rect, image) && !image_has_visible_rgb(image.data()) {
            // A zero-filled partial update is not a valid desktop base. Keep
            // waiting until IronRDP has decoded a real backing image.
            return Ok(None);
        }
        *sent_initial_frame = true;
        return Ok(Some(base_frame_event(image)));
    }
    if rect_covers_image(rect, image) {
        // A full-screen dirty region is already a complete backing buffer. Send
        // it as a base frame so the UI can recover after logon reactivation or
        // any missed incremental rectangles.
        return Ok(Some(base_frame_event(image)));
    }

    Ok(Some(RemoteDesktopHelperEvent::FrameUpdate {
        update: RemoteDesktopFrameUpdate::new(
            remote_size_for_image(image),
            rect,
            RemoteDesktopFrameFormat::Rgba8,
            copy_image_rect(image.data(), image.width(), rect),
        ),
    }))
}

pub(crate) fn base_frame_event(image: &DecodedImage) -> RemoteDesktopHelperEvent {
    RemoteDesktopHelperEvent::Frame {
        frame: RemoteDesktopFrame::new(
            remote_size_for_image(image),
            RemoteDesktopFrameFormat::Rgba8,
            opaque_rgba_bytes(image.data()),
        ),
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
    rgba_bytes: &[u8],
    image_width: u16,
    rect: RemoteDesktopRect,
) -> Vec<u8> {
    let pixel_size = RemoteDesktopFrameFormat::Rgba8.bytes_per_pixel();
    let image_width = usize::from(image_width);
    let rect_x = usize::try_from(rect.x).unwrap_or(usize::MAX);
    let rect_y = usize::try_from(rect.y).unwrap_or(usize::MAX);
    let rect_width = usize::try_from(rect.width).unwrap_or(0);
    let rect_height = usize::try_from(rect.height).unwrap_or(0);
    let mut bytes = Vec::with_capacity(rect_width * rect_height * pixel_size);
    for row in 0..rect_height {
        let start = ((rect_y + row) * image_width + rect_x) * pixel_size;
        let end = start + rect_width * pixel_size;
        bytes.extend_from_slice(&rgba_bytes[start..end]);
    }
    set_rgba_alpha_opaque(&mut bytes);
    bytes
}

fn rect_covers_image(rect: RemoteDesktopRect, image: &DecodedImage) -> bool {
    rect.x == 0
        && rect.y == 0
        && rect.width == u32::from(image.width())
        && rect.height == u32::from(image.height())
}

fn image_has_visible_rgb(bytes: &[u8]) -> bool {
    bytes
        .chunks_exact(RemoteDesktopFrameFormat::Rgba8.bytes_per_pixel())
        .any(|pixel| pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0)
}

pub(crate) fn opaque_rgba_bytes(bytes: &[u8]) -> Vec<u8> {
    let mut bytes = bytes.to_vec();
    set_rgba_alpha_opaque(&mut bytes);
    bytes
}

fn set_rgba_alpha_opaque(bytes: &mut [u8]) {
    for pixel in bytes.chunks_exact_mut(RemoteDesktopFrameFormat::Rgba8.bytes_per_pixel()) {
        pixel[3] = 0xff;
    }
}
