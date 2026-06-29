// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fmt, sync::Arc};

use gpui::RenderImage;
use image::{Frame as ImageFrame, RgbaImage};
use oxideterm_remote_desktop::{
    RemoteDesktopCursorShape, RemoteDesktopErrorCategory, RemoteDesktopFrame,
    RemoteDesktopFrameCompression, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopHelperEvent, RemoteDesktopProtocol, RemoteDesktopRect, RemoteDesktopSessionStatus,
    RemoteDesktopSize,
};

const REMOTE_DESKTOP_TILE_SIZE: u32 = 256;

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteDesktopCursorState {
    pub x: u32,
    pub y: u32,
    pub visible: bool,
    pub shape: Option<RemoteDesktopCursorShape>,
}

impl Default for RemoteDesktopCursorState {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            visible: true,
            shape: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RemoteDesktopViewSnapshot {
    pub title: String,
    pub protocol: RemoteDesktopProtocol,
    pub status: RemoteDesktopSessionStatus,
    pub size: Option<RemoteDesktopSize>,
    pub message: Option<String>,
    pub error_category: Option<RemoteDesktopErrorCategory>,
    pub has_frame: bool,
    pub read_only: bool,
    pub pending_resize: Option<RemoteDesktopSize>,
}

#[derive(Clone, Debug)]
pub struct RemoteDesktopViewState {
    title: String,
    protocol: RemoteDesktopProtocol,
    status: RemoteDesktopSessionStatus,
    size: Option<RemoteDesktopSize>,
    message: Option<String>,
    error_category: Option<RemoteDesktopErrorCategory>,
    frame: Option<RemoteDesktopFrame>,
    frame_image: Option<CachedRemoteDesktopFrameImage>,
    cursor: RemoteDesktopCursorState,
    read_only: bool,
    pending_resize: Option<RemoteDesktopSize>,
}

impl PartialEq for RemoteDesktopViewState {
    fn eq(&self, other: &Self) -> bool {
        self.title == other.title
            && self.protocol == other.protocol
            && self.status == other.status
            && self.size == other.size
            && self.message == other.message
            && self.error_category == other.error_category
            && self.frame == other.frame
            && self.cursor == other.cursor
            && self.read_only == other.read_only
            && self.pending_resize == other.pending_resize
    }
}

#[derive(Clone)]
struct CachedRemoteDesktopFrameImage {
    size: RemoteDesktopSize,
    bytes: Vec<u8>,
    tiles: Vec<RemoteDesktopFrameTile>,
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteDesktopFrameTile {
    pub(crate) rect: RemoteDesktopRect,
    pub(crate) image: Arc<RenderImage>,
}

impl fmt::Debug for CachedRemoteDesktopFrameImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CachedRemoteDesktopFrameImage")
            .field("size", &self.size)
            .field("tile_count", &self.tiles.len())
            .finish()
    }
}

impl CachedRemoteDesktopFrameImage {
    fn from_frame(frame: &RemoteDesktopFrame) -> Option<Self> {
        if !frame.is_complete() {
            return None;
        }

        let mut bytes = frame.bytes.clone();
        convert_framebuffer_bytes_to_gpui_bgra(&mut bytes, frame.format);
        let tiles = render_tiles_for_bgra_bytes(frame.size, &bytes)?;
        Some(Self {
            size: frame.size,
            bytes,
            tiles,
        })
    }

    fn apply_update(&mut self, update: &RemoteDesktopFrameUpdate) -> bool {
        if update.size != self.size
            || update.compression != RemoteDesktopFrameCompression::None
            || !update.is_complete()
        {
            return false;
        }

        // Keep the CPU-side backing buffer in GPUI's BGRA order by touching
        // only the dirty rectangle. GPUI still needs a fresh RenderImage today,
        // but ordinary dirty updates no longer re-convert the full frame.
        if !copy_update_to_bgra_backing(&mut self.bytes, self.size.width, update) {
            return false;
        }
        self.refresh_tiles_in_rect(update.rect)
    }

    fn refresh_tiles_in_rect(&mut self, rect: RemoteDesktopRect) -> bool {
        for tile in &mut self.tiles {
            if !rects_intersect(tile.rect, rect) {
                continue;
            }
            let Some(image) = render_tile_for_bgra_bytes(self.size.width, &self.bytes, tile.rect)
            else {
                return false;
            };
            tile.image = image;
        }
        true
    }
}

impl RemoteDesktopViewState {
    pub fn new(title: impl Into<String>, protocol: RemoteDesktopProtocol) -> Self {
        Self {
            title: title.into(),
            protocol,
            status: RemoteDesktopSessionStatus::Idle,
            size: None,
            message: None,
            error_category: None,
            frame: None,
            frame_image: None,
            cursor: RemoteDesktopCursorState::default(),
            read_only: false,
            pending_resize: None,
        }
    }

    pub fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    pub fn apply_event(&mut self, event: RemoteDesktopHelperEvent) {
        match event {
            RemoteDesktopHelperEvent::Status { status, message } => {
                self.status = status;
                self.message = message;
                self.error_category = None;
            }
            RemoteDesktopHelperEvent::Connected { size } => {
                self.status = RemoteDesktopSessionStatus::Connected;
                self.size = Some(size);
                self.message = None;
                self.error_category = None;
                self.pending_resize = None;
            }
            RemoteDesktopHelperEvent::Frame { frame } => {
                self.status = RemoteDesktopSessionStatus::Connected;
                self.size = Some(frame.size);
                self.frame_image = CachedRemoteDesktopFrameImage::from_frame(&frame);
                self.frame = Some(frame);
                self.message = None;
                self.error_category = None;
                self.pending_resize = None;
            }
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                if let Some(frame) = self.frame.as_mut()
                    && frame.apply_update(&update)
                {
                    self.status = RemoteDesktopSessionStatus::Connected;
                    self.size = Some(update.size);
                    if let Some(frame_image) = self.frame_image.as_mut()
                        && !frame_image.apply_update(&update)
                    {
                        self.frame_image = CachedRemoteDesktopFrameImage::from_frame(frame);
                    }
                    self.message = None;
                    self.error_category = None;
                    self.pending_resize = None;
                } else if let Some(frame) = frame_from_full_update(&update) {
                    // Full-frame updates carry a complete backing buffer. Use
                    // them to recover if the previous base frame was missing or
                    // belonged to an earlier desktop activation.
                    self.apply_event(RemoteDesktopHelperEvent::Frame { frame });
                }
            }
            RemoteDesktopHelperEvent::ConnectionFailure { message, category } => {
                self.status = RemoteDesktopSessionStatus::Failed;
                self.message = Some(message);
                self.error_category = category;
            }
            RemoteDesktopHelperEvent::Disconnected { reason } => {
                self.status = RemoteDesktopSessionStatus::Disconnected;
                self.message = reason;
                self.error_category = None;
                self.frame = None;
                self.frame_image = None;
            }
            RemoteDesktopHelperEvent::Terminated { exit_code } => {
                if self.status == RemoteDesktopSessionStatus::Disconnected && self.message.is_some()
                {
                    return;
                }
                self.status = RemoteDesktopSessionStatus::Disconnected;
                self.message = exit_code.map(|code| format!("Helper exited with code {code}."));
                self.error_category = None;
                self.frame = None;
                self.frame_image = None;
            }
            RemoteDesktopHelperEvent::Cursor { x, y, .. } => {
                self.cursor.x = x;
                self.cursor.y = y;
            }
            RemoteDesktopHelperEvent::CursorShape { shape } => {
                if shape.is_complete() {
                    self.cursor.shape = Some(shape);
                    self.cursor.visible = true;
                }
            }
            RemoteDesktopHelperEvent::CursorDefault => {
                self.cursor.shape = None;
                self.cursor.visible = true;
            }
            RemoteDesktopHelperEvent::CursorHidden => {
                self.cursor.visible = false;
            }
            RemoteDesktopHelperEvent::ClipboardText { .. }
            | RemoteDesktopHelperEvent::ClipboardData { .. } => {
                // Clipboard changes are handled by the app surface that owns
                // focus, clipboard, and pointer capture.
            }
        }
    }

    pub fn mark_resize_requested(&mut self, size: RemoteDesktopSize) {
        self.pending_resize = Some(RemoteDesktopSize::clamped(size.width, size.height));
    }

    pub fn snapshot(&self) -> RemoteDesktopViewSnapshot {
        RemoteDesktopViewSnapshot {
            title: self.title.clone(),
            protocol: self.protocol,
            status: self.status,
            size: self.size,
            message: self.message.clone(),
            error_category: self.error_category,
            has_frame: self.frame.is_some(),
            read_only: self.read_only,
            pending_resize: self.pending_resize,
        }
    }

    pub fn frame(&self) -> Option<&RemoteDesktopFrame> {
        self.frame.as_ref()
    }

    pub(crate) fn frame_tiles(&self) -> Option<Vec<RemoteDesktopFrameTile>> {
        self.frame_image.as_ref().map(|cached| cached.tiles.clone())
    }

    pub fn cursor(&self) -> &RemoteDesktopCursorState {
        &self.cursor
    }
}

fn frame_from_full_update(update: &RemoteDesktopFrameUpdate) -> Option<RemoteDesktopFrame> {
    if update.compression != RemoteDesktopFrameCompression::None
        || !update.is_complete()
        || update.rect.x != 0
        || update.rect.y != 0
        || update.rect.width != update.size.width
        || update.rect.height != update.size.height
    {
        return None;
    }

    Some(RemoteDesktopFrame::new(
        update.size,
        update.format,
        update.bytes.clone(),
    ))
}

fn convert_framebuffer_bytes_to_gpui_bgra(bytes: &mut [u8], format: RemoteDesktopFrameFormat) {
    match format {
        RemoteDesktopFrameFormat::Rgba8 => {
            // GPUI's RenderImage cache expects BGRA bytes, while the helper
            // protocol keeps RGBA explicit for engines that already produce it.
            for pixel in bytes.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        RemoteDesktopFrameFormat::Bgra8 => {
            // FreeRDP and VNC-style desktop buffers often use the fourth byte
            // as unused padding rather than alpha. Remote desktop framebuffers
            // are opaque, so make that explicit before uploading to GPUI.
            for pixel in bytes.chunks_exact_mut(4) {
                pixel[3] = 0xff;
            }
        }
    }
}

fn copy_update_to_bgra_backing(
    dst: &mut [u8],
    dst_width: u32,
    update: &RemoteDesktopFrameUpdate,
) -> bool {
    let Ok(dst_width) = usize::try_from(dst_width) else {
        return false;
    };
    let Ok(dst_x) = usize::try_from(update.rect.x) else {
        return false;
    };
    let Ok(dst_y) = usize::try_from(update.rect.y) else {
        return false;
    };
    let Ok(rect_width) = usize::try_from(update.rect.width) else {
        return false;
    };
    let Ok(rect_height) = usize::try_from(update.rect.height) else {
        return false;
    };
    let Some(dst_stride) = dst_width.checked_mul(4) else {
        return false;
    };
    let Some(src_stride) = rect_width.checked_mul(4) else {
        return false;
    };
    let Some(row_len) = rect_width.checked_mul(4) else {
        return false;
    };

    for row in 0..rect_height {
        let Some(dst_offset) = dst_y
            .checked_add(row)
            .and_then(|y| y.checked_mul(dst_stride))
            .and_then(|offset| offset.checked_add(dst_x.checked_mul(4)?))
        else {
            return false;
        };
        let Some(src_offset) = row.checked_mul(src_stride) else {
            return false;
        };
        let Some(dst_end) = dst_offset.checked_add(row_len) else {
            return false;
        };
        let Some(src_end) = src_offset.checked_add(row_len) else {
            return false;
        };
        let Some(dst_row) = dst.get_mut(dst_offset..dst_end) else {
            return false;
        };
        let Some(src_row) = update.bytes.get(src_offset..src_end) else {
            return false;
        };
        copy_update_row_to_bgra(dst_row, src_row, update.format);
    }
    true
}

fn copy_update_row_to_bgra(dst_row: &mut [u8], src_row: &[u8], format: RemoteDesktopFrameFormat) {
    match format {
        RemoteDesktopFrameFormat::Rgba8 => {
            for (dst, src) in dst_row.chunks_exact_mut(4).zip(src_row.chunks_exact(4)) {
                dst[0] = src[2];
                dst[1] = src[1];
                dst[2] = src[0];
                dst[3] = src[3];
            }
        }
        RemoteDesktopFrameFormat::Bgra8 => {
            for (dst, src) in dst_row.chunks_exact_mut(4).zip(src_row.chunks_exact(4)) {
                dst[0] = src[0];
                dst[1] = src[1];
                dst[2] = src[2];
                dst[3] = 0xff;
            }
        }
    }
}

fn render_image_for_bgra_bytes(
    size: RemoteDesktopSize,
    bytes: Vec<u8>,
) -> Option<Arc<RenderImage>> {
    let buffer = RgbaImage::from_raw(size.width, size.height, bytes)?;
    Some(Arc::new(RenderImage::new(vec![ImageFrame::new(buffer)])))
}

fn render_tiles_for_bgra_bytes(
    size: RemoteDesktopSize,
    bytes: &[u8],
) -> Option<Vec<RemoteDesktopFrameTile>> {
    let mut tiles = Vec::new();
    let mut y = 0;
    while y < size.height {
        let tile_height = REMOTE_DESKTOP_TILE_SIZE.min(size.height - y);
        let mut x = 0;
        while x < size.width {
            let tile_width = REMOTE_DESKTOP_TILE_SIZE.min(size.width - x);
            let rect = RemoteDesktopRect::new(x, y, tile_width, tile_height);
            let image = render_tile_for_bgra_bytes(size.width, bytes, rect)?;
            tiles.push(RemoteDesktopFrameTile { rect, image });
            x += tile_width;
        }
        y += tile_height;
    }
    Some(tiles)
}

fn render_tile_for_bgra_bytes(
    backing_width: u32,
    bytes: &[u8],
    rect: RemoteDesktopRect,
) -> Option<Arc<RenderImage>> {
    let tile_bytes = copy_bgra_rect_from_backing(bytes, backing_width, rect)?;
    render_image_for_bgra_bytes(
        RemoteDesktopSize {
            width: rect.width,
            height: rect.height,
        },
        tile_bytes,
    )
}

fn copy_bgra_rect_from_backing(
    src: &[u8],
    src_width: u32,
    rect: RemoteDesktopRect,
) -> Option<Vec<u8>> {
    let src_width = usize::try_from(src_width).ok()?;
    let src_x = usize::try_from(rect.x).ok()?;
    let src_y = usize::try_from(rect.y).ok()?;
    let rect_width = usize::try_from(rect.width).ok()?;
    let rect_height = usize::try_from(rect.height).ok()?;
    let src_stride = src_width.checked_mul(4)?;
    let row_len = rect_width.checked_mul(4)?;
    let mut bytes = vec![0; row_len.checked_mul(rect_height)?];

    for row in 0..rect_height {
        let src_offset = src_y
            .checked_add(row)?
            .checked_mul(src_stride)?
            .checked_add(src_x.checked_mul(4)?)?;
        let src_end = src_offset.checked_add(row_len)?;
        let dst_offset = row.checked_mul(row_len)?;
        let dst_end = dst_offset.checked_add(row_len)?;
        bytes
            .get_mut(dst_offset..dst_end)?
            .copy_from_slice(src.get(src_offset..src_end)?);
    }

    Some(bytes)
}

fn rects_intersect(a: RemoteDesktopRect, b: RemoteDesktopRect) -> bool {
    let Some(a_right) = a.x.checked_add(a.width) else {
        return false;
    };
    let Some(a_bottom) = a.y.checked_add(a.height) else {
        return false;
    };
    let Some(b_right) = b.x.checked_add(b.width) else {
        return false;
    };
    let Some(b_bottom) = b.y.checked_add(b.height) else {
        return false;
    };

    a.x < b_right && b.x < a_right && a.y < b_bottom && b.y < a_bottom
}

#[cfg(test)]
mod tests {
    use oxideterm_remote_desktop::{
        RemoteDesktopCursorShape, RemoteDesktopFrame, RemoteDesktopFrameFormat,
        RemoteDesktopFrameUpdate, RemoteDesktopRect,
    };

    use super::*;

    #[test]
    fn connected_event_sets_size_and_status() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::Connected {
            size: RemoteDesktopSize {
                width: 1280,
                height: 720,
            },
        });

        let snapshot = state.snapshot();
        assert_eq!(snapshot.status, RemoteDesktopSessionStatus::Connected);
        assert_eq!(
            snapshot.size,
            Some(RemoteDesktopSize {
                width: 1280,
                height: 720,
            })
        );
        assert!(!snapshot.has_frame);
    }

    #[test]
    fn frame_event_keeps_latest_frame_for_rendering() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Vnc);
        let size = RemoteDesktopSize {
            width: 2,
            height: 2,
        };

        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(size, RemoteDesktopFrameFormat::Rgba8, vec![0; 16]),
        });

        assert!(state.snapshot().has_frame);
        assert!(state.frame().unwrap().is_complete());
        assert!(state.frame_tiles().is_some());
    }

    #[test]
    fn frame_update_patches_existing_frame() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 2,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![1, 1, 1, 1, 2, 2, 2, 2],
            ),
        });

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(1, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![9, 9, 9, 9],
            ),
        });

        assert_eq!(state.frame().unwrap().bytes, vec![1, 1, 1, 1, 9, 9, 9, 9]);
        let tiles = state.frame_tiles().unwrap();
        assert_eq!(
            tiles[0].image.as_bytes(0),
            Some([1, 1, 1, 1, 9, 9, 9, 9].as_slice())
        );
    }

    #[test]
    fn full_frame_update_without_base_establishes_frame() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 2,
            height: 1,
        };

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(0, 0, 2, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0x30, 0x20, 0x10, 0xff, 0x60, 0x50, 0x40, 0xff],
            ),
        });

        assert_eq!(
            state.frame().map(|frame| frame.bytes.as_slice()),
            Some([0x30, 0x20, 0x10, 0xff, 0x60, 0x50, 0x40, 0xff].as_slice())
        );
        assert_eq!(
            state.frame_tiles().unwrap()[0].image.as_bytes(0),
            Some([0x10, 0x20, 0x30, 0xff, 0x40, 0x50, 0x60, 0xff].as_slice())
        );
    }

    #[test]
    fn full_frame_update_replaces_mismatched_base_frame() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                RemoteDesktopSize {
                    width: 1,
                    height: 1,
                },
                RemoteDesktopFrameFormat::Rgba8,
                vec![1, 1, 1, 0xff],
            ),
        });
        let new_size = RemoteDesktopSize {
            width: 2,
            height: 1,
        };

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                new_size,
                RemoteDesktopRect::new(0, 0, 2, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![2, 2, 2, 0xff, 3, 3, 3, 0xff],
            ),
        });

        assert_eq!(state.snapshot().size, Some(new_size));
        assert_eq!(
            state.frame().map(|frame| frame.bytes.as_slice()),
            Some([2, 2, 2, 0xff, 3, 3, 3, 0xff].as_slice())
        );
    }

    #[test]
    fn frame_update_patches_cached_tile_bgra_backing_locally() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 2,
            height: 2,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![
                    0x30, 0x20, 0x10, 0xff, 0x60, 0x50, 0x40, 0xff, 0x90, 0x80, 0x70, 0xff, 0xc0,
                    0xb0, 0xa0, 0xff,
                ],
            ),
        });
        let before = state.frame_tiles().expect("base frame should create tiles");

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(1, 1, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0xaa, 0xbb, 0xcc, 0xdd],
            ),
        });

        let after = state
            .frame_tiles()
            .expect("dirty update should keep image tiles");
        assert!(!Arc::ptr_eq(&before[0].image, &after[0].image));
        assert_eq!(
            after[0].image.as_bytes(0),
            Some(
                [
                    0x10, 0x20, 0x30, 0xff, 0x40, 0x50, 0x60, 0xff, 0x70, 0x80, 0x90, 0xff, 0xcc,
                    0xbb, 0xaa, 0xdd,
                ]
                .as_slice()
            )
        );
    }

    #[test]
    fn dirty_update_only_rebuilds_intersecting_tiles() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 300,
            height: 300,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0; RemoteDesktopFrame::expected_len(size).unwrap()],
            ),
        });
        let before = state.frame_tiles().expect("base frame should create tiles");
        assert_eq!(before.len(), 4);

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(10, 10, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0xaa, 0xbb, 0xcc, 0xdd],
            ),
        });

        let after = state
            .frame_tiles()
            .expect("dirty update should keep image tiles");
        assert!(!Arc::ptr_eq(&before[0].image, &after[0].image));
        assert!(Arc::ptr_eq(&before[1].image, &after[1].image));
        assert!(Arc::ptr_eq(&before[2].image, &after[2].image));
        assert!(Arc::ptr_eq(&before[3].image, &after[3].image));
    }

    #[test]
    fn cursor_event_does_not_rebuild_cached_frame_tiles() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 1,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0x30, 0x20, 0x10, 0xff],
            ),
        });
        let tiles = state
            .frame_tiles()
            .expect("frame should create image tiles");

        state.apply_event(RemoteDesktopHelperEvent::Cursor {
            x: 10,
            y: 20,
            width: 1,
            height: 1,
        });

        let cached_tiles = state
            .frame_tiles()
            .expect("cursor updates should keep image tiles");
        assert!(Arc::ptr_eq(&tiles[0].image, &cached_tiles[0].image));
    }

    #[test]
    fn bgra_frame_padding_is_cached_as_opaque_alpha() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                RemoteDesktopSize {
                    width: 1,
                    height: 1,
                },
                RemoteDesktopFrameFormat::Bgra8,
                vec![0x10, 0x20, 0x30, 0x00],
            ),
        });

        assert_eq!(
            state.frame_tiles().unwrap()[0].image.as_bytes(0),
            Some([0x10, 0x20, 0x30, 0xff].as_slice())
        );
    }

    #[test]
    fn rgba_frame_is_cached_in_gpui_bgra_order() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                RemoteDesktopSize {
                    width: 1,
                    height: 1,
                },
                RemoteDesktopFrameFormat::Rgba8,
                vec![0x30, 0x20, 0x10, 0xff],
            ),
        });

        assert_eq!(
            state.frame_tiles().unwrap()[0].image.as_bytes(0),
            Some([0x10, 0x20, 0x30, 0xff].as_slice())
        );
    }

    #[test]
    fn connected_event_clears_pending_resize() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Vnc);
        state.mark_resize_requested(RemoteDesktopSize {
            width: 1200,
            height: 900,
        });

        state.apply_event(RemoteDesktopHelperEvent::Connected {
            size: RemoteDesktopSize {
                width: 1200,
                height: 900,
            },
        });

        assert_eq!(state.snapshot().pending_resize, None);
    }

    #[test]
    fn failure_event_exposes_user_safe_message() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::ConnectionFailure {
            message: "authentication failed".to_string(),
            category: Some(RemoteDesktopErrorCategory::Authentication),
        });

        let snapshot = state.snapshot();
        assert_eq!(snapshot.status, RemoteDesktopSessionStatus::Failed);
        assert_eq!(snapshot.message.as_deref(), Some("authentication failed"));
        assert_eq!(
            snapshot.error_category,
            Some(RemoteDesktopErrorCategory::Authentication)
        );
    }

    #[test]
    fn terminated_event_does_not_hide_disconnect_reason() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::Disconnected {
            reason: Some("RDP session closed.".to_string()),
        });
        state.apply_event(RemoteDesktopHelperEvent::Terminated { exit_code: Some(0) });

        let snapshot = state.snapshot();
        assert_eq!(snapshot.status, RemoteDesktopSessionStatus::Disconnected);
        assert_eq!(snapshot.message.as_deref(), Some("RDP session closed."));
    }

    #[test]
    fn cursor_events_update_remote_cursor_state() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let shape = RemoteDesktopCursorShape::new(
            RemoteDesktopSize {
                width: 1,
                height: 1,
            },
            0,
            0,
            RemoteDesktopFrameFormat::Rgba8,
            vec![1, 2, 3, 4],
        );

        state.apply_event(RemoteDesktopHelperEvent::CursorShape {
            shape: shape.clone(),
        });
        state.apply_event(RemoteDesktopHelperEvent::Cursor {
            x: 12,
            y: 34,
            width: 1,
            height: 1,
        });

        assert_eq!(state.cursor().x, 12);
        assert_eq!(state.cursor().y, 34);
        assert!(state.cursor().visible);
        assert_eq!(state.cursor().shape.as_ref(), Some(&shape));

        state.apply_event(RemoteDesktopHelperEvent::CursorHidden);
        assert!(!state.cursor().visible);

        state.apply_event(RemoteDesktopHelperEvent::CursorDefault);
        assert!(state.cursor().visible);
        assert!(state.cursor().shape.is_none());
    }
}
