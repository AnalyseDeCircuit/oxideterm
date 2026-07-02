// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashSet,
    fmt,
    sync::{Arc, Mutex},
};

use gpui::{DevicePixels, DynamicTexture, RenderImage, size};
use image::{Frame as ImageFrame, RgbaImage};
use oxideterm_remote_desktop::{
    RemoteDesktopCursorShape, RemoteDesktopErrorCategory, RemoteDesktopFrame,
    RemoteDesktopFrameCompression, RemoteDesktopFrameFormat, RemoteDesktopFrameUpdate,
    RemoteDesktopHelperEvent, RemoteDesktopProtocol, RemoteDesktopRect, RemoteDesktopSessionStatus,
    RemoteDesktopSize,
};

const REMOTE_DESKTOP_MAX_TEXTURE_UPLOAD_RECTS: usize = 64;
const REMOTE_DESKTOP_TEXTURE_REGION_SIMPLIFY_TRIGGER: usize =
    REMOTE_DESKTOP_MAX_TEXTURE_UPLOAD_RECTS * 2;
const REMOTE_DESKTOP_TEXTURE_MERGE_AREA_INFLATION_LIMIT: u64 = 2;

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
    frame_image: Option<CachedRemoteDesktopFrameImage>,
    texture_generation: u64,
    corrupted_frame: Option<CorruptedRemoteDesktopFrame>,
    cursor: RemoteDesktopCursorState,
    cursor_image: Option<Arc<RenderImage>>,
    retired_images: Vec<Arc<RenderImage>>,
    retired_textures: Vec<Arc<DynamicTexture>>,
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
            && self.texture_generation == other.texture_generation
            && self.frame_image.as_ref().map(|frame| frame.size)
                == other.frame_image.as_ref().map(|frame| frame.size)
            && self.corrupted_frame == other.corrupted_frame
            && self.cursor == other.cursor
            && self.cursor_image.as_ref().map(|image| image.id)
                == other.cursor_image.as_ref().map(|image| image.id)
            && self.read_only == other.read_only
            && self.pending_resize == other.pending_resize
    }
}

#[derive(Clone)]
struct CachedRemoteDesktopFrameImage {
    size: RemoteDesktopSize,
    generation: u64,
    bytes: Arc<Mutex<Vec<u8>>>,
    texture: Arc<DynamicTexture>,
    next_texture_update_sequence: u64,
    pending_texture_updates: Arc<Mutex<Vec<RemoteDesktopTextureUpdate>>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CorruptedRemoteDesktopFrame {
    pub size: RemoteDesktopSize,
    pub format: RemoteDesktopFrameFormat,
    pub byte_len: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RemoteDesktopFrameApplyStats {
    pub events: usize,
    pub full_frames: usize,
    pub frame_updates: usize,
    pub first_trace_id: Option<u64>,
    pub last_trace_id: Option<u64>,
    pub dirty_updates_applied: usize,
    pub dirty_updates_rejected: usize,
    pub full_update_recoveries: usize,
    pub corrupted_frames: usize,
    pub dirty_rect_pixels: u64,
    pub dirty_frame_pixels: u64,
    pub dirty_tiles_refreshed: usize,
    pub frame_tiles_created: usize,
    pub pending_texture_updates: usize,
    pub pending_texture_upload_bytes: usize,
}

#[derive(Clone)]
pub(crate) struct RemoteDesktopFrameSurface {
    pub(crate) size: RemoteDesktopSize,
    pub(crate) generation: u64,
    pub(crate) texture: Arc<DynamicTexture>,
    backing_bytes: Arc<Mutex<Vec<u8>>>,
    pub(crate) pending_texture_updates: Arc<Mutex<Vec<RemoteDesktopTextureUpdate>>>,
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteDesktopTextureUpdate {
    sequence_ids: Vec<u64>,
    pub(crate) rect: RemoteDesktopRect,
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteDesktopTextureUpload {
    pub(crate) sequence_ids: Vec<u64>,
    pub(crate) rect: RemoteDesktopRect,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CachedRemoteDesktopTextureApplyStats {
    pending_texture_updates: usize,
    pending_texture_upload_bytes: usize,
}

impl RemoteDesktopFrameSurface {
    pub(crate) fn pending_texture_uploads(&self) -> Vec<RemoteDesktopTextureUpload> {
        // Match the state update lock order to avoid deadlocks while the UI
        // thread snapshots upload bytes from the latest backing buffer.
        let Ok(backing_bytes) = self.backing_bytes.lock() else {
            return Vec::new();
        };
        let Ok(mut pending_updates) = self.pending_texture_updates.lock() else {
            return Vec::new();
        };
        simplify_pending_texture_regions(&mut pending_updates);
        pending_updates
            .iter()
            .filter_map(|update| {
                copy_bgra_rect_from_backing(&backing_bytes, self.size, update.rect).map(|bytes| {
                    RemoteDesktopTextureUpload {
                        sequence_ids: update.sequence_ids.clone(),
                        rect: update.rect,
                        bytes,
                    }
                })
            })
            .collect()
    }

    pub(crate) fn acknowledge_texture_uploads(&self, sequence_ids: impl IntoIterator<Item = u64>) {
        let uploaded_sequences = sequence_ids.into_iter().collect::<HashSet<_>>();
        if uploaded_sequences.is_empty() {
            return;
        }
        let Ok(mut pending_updates) = self.pending_texture_updates.lock() else {
            return;
        };
        for update in pending_updates.iter_mut() {
            update
                .sequence_ids
                .retain(|sequence_id| !uploaded_sequences.contains(sequence_id));
        }
        pending_updates.retain(|update| !update.sequence_ids.is_empty());
    }
}

impl fmt::Debug for CachedRemoteDesktopFrameImage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CachedRemoteDesktopFrameImage")
            .field("size", &self.size)
            .field("generation", &self.generation)
            .field(
                "pending_texture_updates",
                &self
                    .pending_texture_updates
                    .lock()
                    .map(|updates| updates.len())
                    .ok(),
            )
            .finish()
    }
}

impl CachedRemoteDesktopFrameImage {
    fn from_frame(frame: &RemoteDesktopFrame, generation: u64) -> Option<Self> {
        if !frame.is_complete() {
            return None;
        }

        let mut bytes = frame.bytes.clone();
        convert_framebuffer_bytes_to_gpui_bgra(&mut bytes, frame.format);
        let texture = Arc::new(DynamicTexture::new(size(
            DevicePixels(i32::try_from(frame.size.width).ok()?),
            DevicePixels(i32::try_from(frame.size.height).ok()?),
        )));
        let pending_texture_updates = Arc::new(Mutex::new(vec![RemoteDesktopTextureUpdate {
            sequence_ids: vec![0],
            rect: RemoteDesktopRect::new(0, 0, frame.size.width, frame.size.height),
        }]));
        Some(Self {
            size: frame.size,
            generation,
            bytes: Arc::new(Mutex::new(bytes)),
            texture,
            next_texture_update_sequence: 1,
            pending_texture_updates,
        })
    }

    fn apply_update(&mut self, update: &RemoteDesktopFrameUpdate) -> bool {
        self.apply_update_with_stats(update).is_some()
    }

    fn apply_update_with_stats(
        &mut self,
        update: &RemoteDesktopFrameUpdate,
    ) -> Option<CachedRemoteDesktopTextureApplyStats> {
        if update.size != self.size
            || update.compression != RemoteDesktopFrameCompression::None
            || !update.is_complete()
        {
            return None;
        }

        // Keep the CPU-side backing buffer in GPUI's BGRA order. The paint
        // phase drains the pending queue into one stable GPU texture.
        {
            let Ok(mut bytes) = self.bytes.lock() else {
                return None;
            };
            if !copy_update_to_bgra_backing(&mut bytes, self.size.width, update) {
                return None;
            }
        }

        let Ok(mut pending_updates) = self.pending_texture_updates.lock() else {
            return None;
        };
        if pending_updates
            .iter()
            .any(|pending_update| is_full_frame_rect(self.size, pending_update.rect))
        {
            // Dirty updates that arrive before the initial texture upload can
            // be folded into that upload. This avoids bursty login/animation
            // periods from turning one pending base frame into many GPU writes.
            return Some(pending_texture_stats(&pending_updates));
        }

        let sequence_id = self.next_texture_update_sequence;
        self.next_texture_update_sequence = self.next_texture_update_sequence.saturating_add(1);
        push_pending_texture_update(&mut pending_updates, sequence_id, update.rect);
        Some(pending_texture_stats(&pending_updates))
    }

    fn surface(&self) -> RemoteDesktopFrameSurface {
        RemoteDesktopFrameSurface {
            size: self.size,
            generation: self.generation,
            texture: Arc::clone(&self.texture),
            backing_bytes: Arc::clone(&self.bytes),
            pending_texture_updates: Arc::clone(&self.pending_texture_updates),
        }
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
            frame_image: None,
            texture_generation: 0,
            corrupted_frame: None,
            cursor: RemoteDesktopCursorState::default(),
            cursor_image: None,
            retired_images: Vec::new(),
            retired_textures: Vec::new(),
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
                self.retire_frame_image();
                let generation = self.advance_texture_generation();
                self.frame_image = CachedRemoteDesktopFrameImage::from_frame(&frame, generation);
                self.corrupted_frame =
                    self.frame_image
                        .is_none()
                        .then(|| CorruptedRemoteDesktopFrame {
                            size: frame.size,
                            format: frame.format,
                            byte_len: frame.bytes.len(),
                        });
                self.message = None;
                self.error_category = None;
                self.pending_resize = None;
            }
            RemoteDesktopHelperEvent::FrameUpdate { update } => {
                if let Some(frame_image) = self.frame_image.as_mut()
                    && frame_image.apply_update(&update)
                {
                    self.status = RemoteDesktopSessionStatus::Connected;
                    self.size = Some(update.size);
                    self.message = None;
                    self.error_category = None;
                    self.corrupted_frame = None;
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
                self.retire_frame_image();
                self.retire_cursor_image();
                self.corrupted_frame = None;
            }
            RemoteDesktopHelperEvent::Disconnected { reason } => {
                self.status = RemoteDesktopSessionStatus::Disconnected;
                self.message = reason;
                self.error_category = None;
                self.retire_frame_image();
                self.retire_cursor_image();
                self.corrupted_frame = None;
            }
            RemoteDesktopHelperEvent::Terminated { exit_code } => {
                if matches!(
                    self.status,
                    RemoteDesktopSessionStatus::Disconnected | RemoteDesktopSessionStatus::Failed
                ) && self.message.is_some()
                {
                    return;
                }
                self.status = RemoteDesktopSessionStatus::Disconnected;
                self.message = exit_code.map(|code| format!("Helper exited with code {code}."));
                self.error_category = None;
                self.retire_frame_image();
                self.retire_cursor_image();
                self.corrupted_frame = None;
            }
            RemoteDesktopHelperEvent::Cursor { x, y, .. } => {
                self.cursor.x = x;
                self.cursor.y = y;
            }
            RemoteDesktopHelperEvent::CursorShape { shape } => {
                if shape.is_complete() {
                    self.retire_cursor_image();
                    self.cursor_image = render_image_for_cursor_shape(&shape);
                    self.cursor.shape = Some(shape);
                    self.cursor.visible = true;
                }
            }
            RemoteDesktopHelperEvent::CursorDefault => {
                self.retire_cursor_image();
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

    pub fn apply_frame_events(
        &mut self,
        events: impl IntoIterator<Item = RemoteDesktopHelperEvent>,
    ) -> RemoteDesktopFrameApplyStats {
        let mut stats = RemoteDesktopFrameApplyStats::default();
        for event in events {
            stats.events += 1;
            match event {
                RemoteDesktopHelperEvent::Frame { frame } => {
                    record_frame_trace(&mut stats, frame.trace_id);
                    self.apply_event(RemoteDesktopHelperEvent::Frame { frame });
                    stats.full_frames += 1;
                    if self.frame_image.is_some() {
                        stats.frame_tiles_created += 1;
                        if let Some(frame_image) = self.frame_image.as_ref()
                            && let Ok(updates) = frame_image.pending_texture_updates.lock()
                        {
                            let pending = pending_texture_stats(&updates);
                            stats.pending_texture_updates = pending.pending_texture_updates;
                            stats.pending_texture_upload_bytes =
                                pending.pending_texture_upload_bytes;
                        }
                    } else {
                        stats.corrupted_frames += 1;
                    }
                }
                RemoteDesktopHelperEvent::FrameUpdate { update } => {
                    stats.frame_updates += 1;
                    record_frame_trace(&mut stats, update.trace_id);
                    if let Some(frame_image) = self.frame_image.as_mut()
                        && let Some(texture_stats) = frame_image.apply_update_with_stats(&update)
                    {
                        stats.dirty_updates_applied += 1;
                        stats.dirty_rect_pixels += frame_rect_pixels(update.rect);
                        stats.dirty_frame_pixels += frame_size_pixels(update.size);
                        stats.dirty_tiles_refreshed += 1;
                        stats.pending_texture_updates = texture_stats.pending_texture_updates;
                        stats.pending_texture_upload_bytes =
                            texture_stats.pending_texture_upload_bytes;
                        self.status = RemoteDesktopSessionStatus::Connected;
                        self.size = Some(update.size);
                        self.message = None;
                        self.error_category = None;
                        self.corrupted_frame = None;
                        self.pending_resize = None;
                    } else if let Some(frame) = frame_from_full_update(&update) {
                        stats.full_update_recoveries += 1;
                        record_frame_trace(&mut stats, frame.trace_id);
                        self.apply_event(RemoteDesktopHelperEvent::Frame { frame });
                        stats.full_frames += 1;
                        if self.frame_image.is_some() {
                            stats.frame_tiles_created += 1;
                            if let Some(frame_image) = self.frame_image.as_ref()
                                && let Ok(updates) = frame_image.pending_texture_updates.lock()
                            {
                                let pending = pending_texture_stats(&updates);
                                stats.pending_texture_updates = pending.pending_texture_updates;
                                stats.pending_texture_upload_bytes =
                                    pending.pending_texture_upload_bytes;
                            }
                        } else {
                            stats.corrupted_frames += 1;
                        }
                    } else {
                        stats.dirty_updates_rejected += 1;
                    }
                }
                event => {
                    self.apply_event(event);
                }
            }
        }
        stats
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
            has_frame: self.frame_image.is_some(),
            read_only: self.read_only,
            pending_resize: self.pending_resize,
        }
    }

    pub fn frame_size(&self) -> Option<RemoteDesktopSize> {
        self.frame_image.as_ref().map(|frame| frame.size)
    }

    pub fn corrupted_frame(&self) -> Option<CorruptedRemoteDesktopFrame> {
        self.corrupted_frame
    }

    pub fn cursor_image(&self) -> Option<Arc<RenderImage>> {
        self.cursor_image.clone()
    }

    pub fn take_retired_images(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.retired_images)
    }

    pub fn take_retired_textures(&mut self) -> Vec<Arc<DynamicTexture>> {
        std::mem::take(&mut self.retired_textures)
    }

    pub fn take_all_images(&mut self) -> Vec<Arc<RenderImage>> {
        self.retire_frame_image();
        self.retire_cursor_image();
        self.take_retired_images()
    }

    pub fn take_all_textures(&mut self) -> Vec<Arc<DynamicTexture>> {
        self.retire_frame_image();
        self.take_retired_textures()
    }

    pub(crate) fn frame_surface(&self) -> Option<RemoteDesktopFrameSurface> {
        self.frame_image.as_ref().map(|cached| cached.surface())
    }

    pub fn texture_generation(&self) -> u64 {
        self.texture_generation
    }

    pub fn cursor(&self) -> &RemoteDesktopCursorState {
        &self.cursor
    }

    fn retire_frame_image(&mut self) {
        if let Some(frame_image) = self.frame_image.take() {
            self.retired_textures.push(frame_image.texture);
        }
    }

    fn retire_cursor_image(&mut self) {
        if let Some(image) = self.cursor_image.take() {
            self.retired_images.push(image);
        }
    }

    fn advance_texture_generation(&mut self) -> u64 {
        // Texture generations are diagnostic and synchronization boundaries,
        // so wrapping would make stale and current surfaces indistinguishable.
        self.texture_generation = self.texture_generation.saturating_add(1);
        self.texture_generation
    }
}

fn render_image_for_cursor_shape(shape: &RemoteDesktopCursorShape) -> Option<Arc<RenderImage>> {
    if !shape.is_complete() {
        return None;
    }

    let mut bytes = shape.bytes.clone();
    if shape.format == RemoteDesktopFrameFormat::Rgba8 {
        // Cursor images carry real transparency, so preserve the alpha channel
        // unlike opaque framebuffer padding.
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    let buffer = RgbaImage::from_raw(shape.size.width, shape.size.height, bytes)?;
    Some(Arc::new(RenderImage::new(vec![ImageFrame::new(buffer)])))
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

    let frame = RemoteDesktopFrame::new(update.size, update.format, update.bytes.clone());
    Some(match update.trace_id {
        Some(trace_id) => frame.with_trace_id(trace_id),
        None => frame,
    })
}

fn pending_texture_stats(
    updates: &[RemoteDesktopTextureUpdate],
) -> CachedRemoteDesktopTextureApplyStats {
    CachedRemoteDesktopTextureApplyStats {
        pending_texture_updates: updates.len(),
        pending_texture_upload_bytes: updates
            .iter()
            .map(|update| {
                usize::try_from(frame_rect_pixels(update.rect))
                    .ok()
                    .and_then(|pixels| pixels.checked_mul(4))
                    .unwrap_or(0)
            })
            .sum::<usize>(),
    }
}

fn push_pending_texture_update(
    pending_updates: &mut Vec<RemoteDesktopTextureUpdate>,
    sequence_id: u64,
    rect: RemoteDesktopRect,
) {
    if rect.width == 0 || rect.height == 0 {
        return;
    }
    pending_updates.push(RemoteDesktopTextureUpdate {
        sequence_ids: vec![sequence_id],
        rect,
    });
    if pending_updates.len() > REMOTE_DESKTOP_TEXTURE_REGION_SIMPLIFY_TRIGGER {
        simplify_pending_texture_regions(pending_updates);
    }
}

fn simplify_pending_texture_regions(pending_updates: &mut Vec<RemoteDesktopTextureUpdate>) {
    if pending_updates.len() <= 1 {
        return;
    }
    merge_touching_texture_rects(pending_updates);
    if pending_updates.len() > REMOTE_DESKTOP_MAX_TEXTURE_UPLOAD_RECTS {
        merge_texture_rects_to_limit(pending_updates, REMOTE_DESKTOP_MAX_TEXTURE_UPLOAD_RECTS);
    }
}

fn merge_touching_texture_rects(updates: &mut Vec<RemoteDesktopTextureUpdate>) {
    sort_texture_updates(updates);
    let mut index = 0;
    while index < updates.len() {
        let mut next_index = index + 1;
        while next_index < updates.len() {
            if can_merge_texture_rects(updates[index].rect, updates[next_index].rect) {
                updates[index] =
                    merged_texture_update(updates[index].clone(), updates[next_index].clone());
                updates.remove(next_index);
                sort_texture_updates(updates);
                index = 0;
                next_index = 1;
            } else {
                next_index += 1;
            }
        }
        index += 1;
    }
}

fn merge_texture_rects_to_limit(updates: &mut Vec<RemoteDesktopTextureUpdate>, limit: usize) {
    if limit == 0 {
        updates.clear();
        return;
    }
    sort_texture_updates(updates);
    while updates.len() > limit {
        let Some(best_index) = best_neighbor_merge_index(updates) else {
            break;
        };
        updates[best_index] =
            merged_texture_update(updates[best_index].clone(), updates[best_index + 1].clone());
        updates.remove(best_index + 1);
        merge_touching_texture_rects(updates);
    }
}

fn best_neighbor_merge_index(updates: &[RemoteDesktopTextureUpdate]) -> Option<usize> {
    updates
        .windows(2)
        .enumerate()
        .min_by_key(|(_, pair)| {
            let merged = bounding_texture_rect(pair[0].rect, pair[1].rect);
            let merged_area = frame_rect_pixels(merged);
            let pair_area =
                frame_rect_pixels(pair[0].rect).saturating_add(frame_rect_pixels(pair[1].rect));
            (
                merged_area.saturating_sub(pair_area),
                merged_area,
                u64::from(merged.y),
                u64::from(merged.x),
            )
        })
        .map(|(index, _)| index)
}

fn sort_texture_updates(updates: &mut [RemoteDesktopTextureUpdate]) {
    updates.sort_by_key(|update| {
        (
            update.rect.y,
            update.rect.x,
            update.rect.height,
            update.rect.width,
        )
    });
}

fn merged_texture_update(
    mut a: RemoteDesktopTextureUpdate,
    b: RemoteDesktopTextureUpdate,
) -> RemoteDesktopTextureUpdate {
    a.rect = bounding_texture_rect(a.rect, b.rect);
    a.sequence_ids.extend(b.sequence_ids);
    a.sequence_ids.sort_unstable();
    a.sequence_ids.dedup();
    a
}

fn can_merge_texture_rects(a: RemoteDesktopRect, b: RemoteDesktopRect) -> bool {
    if !texture_rects_touch_or_overlap(a, b) {
        return false;
    }
    let merged = bounding_texture_rect(a, b);
    let merged_area = frame_rect_pixels(merged);
    let source_area = frame_rect_pixels(a).saturating_add(frame_rect_pixels(b));
    merged_area <= source_area.saturating_mul(REMOTE_DESKTOP_TEXTURE_MERGE_AREA_INFLATION_LIMIT)
}

fn texture_rects_touch_or_overlap(a: RemoteDesktopRect, b: RemoteDesktopRect) -> bool {
    let a_right = u64::from(a.x) + u64::from(a.width);
    let b_right = u64::from(b.x) + u64::from(b.width);
    let a_bottom = u64::from(a.y) + u64::from(a.height);
    let b_bottom = u64::from(b.y) + u64::from(b.height);
    u64::from(a.x) <= b_right
        && u64::from(b.x) <= a_right
        && u64::from(a.y) <= b_bottom
        && u64::from(b.y) <= a_bottom
}

fn bounding_texture_rect(a: RemoteDesktopRect, b: RemoteDesktopRect) -> RemoteDesktopRect {
    let x = a.x.min(b.x);
    let y = a.y.min(b.y);
    let right = a.x.saturating_add(a.width).max(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .max(b.y.saturating_add(b.height));
    RemoteDesktopRect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y))
}

fn record_frame_trace(stats: &mut RemoteDesktopFrameApplyStats, trace_id: Option<u64>) {
    let Some(trace_id) = trace_id else {
        return;
    };
    if stats.first_trace_id.is_none() {
        stats.first_trace_id = Some(trace_id);
    }
    stats.last_trace_id = Some(trace_id);
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
            // Some desktop buffers use the fourth byte
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

fn copy_bgra_rect_from_backing(
    backing: &[u8],
    size: RemoteDesktopSize,
    rect: RemoteDesktopRect,
) -> Option<Vec<u8>> {
    if rect.x.checked_add(rect.width)? > size.width
        || rect.y.checked_add(rect.height)? > size.height
    {
        return None;
    }
    let dst_width = usize::try_from(size.width).ok()?;
    let rect_x = usize::try_from(rect.x).ok()?;
    let rect_y = usize::try_from(rect.y).ok()?;
    let rect_width = usize::try_from(rect.width).ok()?;
    let rect_height = usize::try_from(rect.height).ok()?;
    let backing_stride = dst_width.checked_mul(4)?;
    let row_len = rect_width.checked_mul(4)?;
    let mut bytes = Vec::with_capacity(row_len.checked_mul(rect_height)?);

    for row in 0..rect_height {
        let row_start = rect_y
            .checked_add(row)?
            .checked_mul(backing_stride)?
            .checked_add(rect_x.checked_mul(4)?)?;
        let row_end = row_start.checked_add(row_len)?;
        bytes.extend_from_slice(backing.get(row_start..row_end)?);
    }
    Some(bytes)
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

fn frame_rect_pixels(rect: RemoteDesktopRect) -> u64 {
    u64::from(rect.width) * u64::from(rect.height)
}

fn frame_size_pixels(size: RemoteDesktopSize) -> u64 {
    u64::from(size.width) * u64::from(size.height)
}

fn is_full_frame_rect(size: RemoteDesktopSize, rect: RemoteDesktopRect) -> bool {
    rect.x == 0 && rect.y == 0 && rect.width == size.width && rect.height == size.height
}

#[cfg(test)]
mod tests {
    use oxideterm_remote_desktop::{
        RemoteDesktopCursorShape, RemoteDesktopFrame, RemoteDesktopFrameFormat,
        RemoteDesktopFrameUpdate, RemoteDesktopRect,
    };

    use super::*;

    fn frame_bgra_bytes(state: &RemoteDesktopViewState) -> Vec<u8> {
        state
            .frame_image
            .as_ref()
            .expect("frame should be cached")
            .bytes
            .lock()
            .expect("frame bytes should not be poisoned")
            .clone()
    }

    fn frame_texture(state: &RemoteDesktopViewState) -> Arc<DynamicTexture> {
        Arc::clone(
            &state
                .frame_image
                .as_ref()
                .expect("frame should be cached")
                .texture,
        )
    }

    fn frame_generation(state: &RemoteDesktopViewState) -> u64 {
        state
            .frame_image
            .as_ref()
            .expect("frame should be cached")
            .generation
    }

    fn pending_texture_update_count(state: &RemoteDesktopViewState) -> usize {
        state
            .frame_image
            .as_ref()
            .expect("frame should be cached")
            .pending_texture_updates
            .lock()
            .expect("pending texture updates should not be poisoned")
            .len()
    }

    fn drain_pending_texture_updates(
        state: &RemoteDesktopViewState,
    ) -> Vec<RemoteDesktopTextureUpload> {
        let surface = state.frame_surface().expect("frame should be cached");
        let updates = surface.pending_texture_uploads();
        surface.acknowledge_texture_uploads(
            updates
                .iter()
                .flat_map(|update| update.sequence_ids.iter().copied()),
        );
        updates
    }

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
        assert_eq!(state.frame_size(), Some(size));
        assert!(state.frame_surface().is_some());
        assert_eq!(state.texture_generation(), 1);
        assert_eq!(frame_generation(&state), 1);
    }

    #[test]
    fn base_frame_replacement_advances_texture_generation() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 1,
            height: 1,
        };

        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0, 0, 0, 0xff],
            ),
        });
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![1, 1, 1, 0xff],
            ),
        });

        let surface = state.frame_surface().expect("frame should be cached");
        assert_eq!(state.texture_generation(), 2);
        assert_eq!(frame_generation(&state), 2);
        assert_eq!(surface.generation, 2);
    }

    #[test]
    fn corrupted_frame_advances_texture_generation_before_recovery() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 1,
            height: 1,
        };

        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0, 0, 0, 0xff],
            ),
        });
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(size, RemoteDesktopFrameFormat::Rgba8, vec![0]),
        });
        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(0, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![2, 2, 2, 0xff],
            ),
        });

        assert_eq!(state.texture_generation(), 3);
        assert_eq!(frame_generation(&state), 3);
        assert_eq!(state.corrupted_frame(), None);
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

        assert_eq!(state.frame_size(), Some(size));
        assert_eq!(
            frame_bgra_bytes(&state),
            [1, 1, 1, 1, 9, 9, 9, 9].as_slice()
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

        assert_eq!(state.frame_size(), Some(size));
        assert_eq!(
            frame_bgra_bytes(&state),
            [0x10, 0x20, 0x30, 0xff, 0x40, 0x50, 0x60, 0xff].as_slice()
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
            frame_bgra_bytes(&state),
            [2, 2, 2, 0xff, 3, 3, 3, 0xff].as_slice()
        );
    }

    #[test]
    fn frame_update_patches_cached_bgra_backing_locally() {
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
        let before = frame_texture(&state);
        drain_pending_texture_updates(&state);

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(1, 1, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0xaa, 0xbb, 0xcc, 0xdd],
            ),
        });

        let after = frame_texture(&state);
        assert!(Arc::ptr_eq(&before, &after));
        assert_eq!(
            frame_bgra_bytes(&state),
            [
                0x10, 0x20, 0x30, 0xff, 0x40, 0x50, 0x60, 0xff, 0x70, 0x80, 0x90, 0xff, 0xcc, 0xbb,
                0xaa, 0xdd,
            ]
            .as_slice()
        );
    }

    #[test]
    fn dirty_update_reuses_dynamic_texture() {
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
        let before = frame_texture(&state);

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(10, 10, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0xaa, 0xbb, 0xcc, 0xdd],
            ),
        });

        let after = frame_texture(&state);
        assert!(Arc::ptr_eq(&before, &after));
        assert_eq!(pending_texture_update_count(&state), 1);
    }

    #[test]
    fn batched_dirty_updates_queue_dynamic_texture_uploads() {
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
        let before = frame_texture(&state);
        drain_pending_texture_updates(&state);

        let stats = state.apply_frame_events([
            RemoteDesktopHelperEvent::FrameUpdate {
                update: RemoteDesktopFrameUpdate::new(
                    size,
                    RemoteDesktopRect::new(10, 10, 1, 1),
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![0xaa, 0xbb, 0xcc, 0xdd],
                ),
            },
            RemoteDesktopHelperEvent::FrameUpdate {
                update: RemoteDesktopFrameUpdate::new(
                    size,
                    RemoteDesktopRect::new(20, 20, 1, 1),
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![0x11, 0x22, 0x33, 0x44],
                ),
            },
        ]);

        let retired = state.take_retired_images();
        let after = frame_texture(&state);
        assert_eq!(stats.events, 2);
        assert_eq!(stats.frame_updates, 2);
        assert_eq!(stats.dirty_updates_applied, 2);
        assert_eq!(stats.dirty_rect_pixels, 2);
        assert_eq!(stats.dirty_frame_pixels, 180_000);
        assert_eq!(stats.dirty_tiles_refreshed, 2);
        assert_eq!(retired.len(), 0);
        assert!(Arc::ptr_eq(&before, &after));
        assert_eq!(pending_texture_update_count(&state), 2);
    }

    #[test]
    fn dirty_updates_merge_into_pending_full_texture_upload() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 2,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0, 0, 0, 0xff, 1, 1, 1, 0xff],
            ),
        });

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(1, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0xaa, 0xbb, 0xcc, 0xdd],
            ),
        });

        let updates = drain_pending_texture_updates(&state);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].rect, RemoteDesktopRect::new(0, 0, 2, 1));
        assert_eq!(updates[0].bytes, frame_bgra_bytes(&state));
    }

    #[test]
    fn pending_texture_regions_clear_only_after_upload_acknowledgement() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 2,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0; RemoteDesktopFrame::expected_len(size).unwrap()],
            ),
        });
        drain_pending_texture_updates(&state);

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(1, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0xaa, 0xbb, 0xcc, 0xdd],
            ),
        });

        let surface = state.frame_surface().expect("frame should be cached");
        let updates = surface.pending_texture_uploads();
        assert_eq!(updates.len(), 1);
        assert_eq!(pending_texture_update_count(&state), 1);

        surface.acknowledge_texture_uploads(updates[0].sequence_ids.clone());
        assert_eq!(pending_texture_update_count(&state), 0);
    }

    #[test]
    fn upload_acknowledgement_does_not_drop_new_dirty_regions() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 4,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0; RemoteDesktopFrame::expected_len(size).unwrap()],
            ),
        });
        drain_pending_texture_updates(&state);

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(0, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0x30, 0x20, 0x10, 0xff],
            ),
        });
        let surface = state.frame_surface().expect("frame should be cached");
        let first_upload = surface.pending_texture_uploads();

        state.apply_event(RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new(2, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![0x60, 0x50, 0x40, 0xff],
            ),
        });
        surface.acknowledge_texture_uploads(first_upload[0].sequence_ids.clone());

        let remaining = surface.pending_texture_uploads();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].rect, RemoteDesktopRect::new(2, 0, 1, 1));
        assert_eq!(remaining[0].bytes, vec![0x40, 0x50, 0x60, 0xff]);
    }

    #[test]
    fn touching_dirty_regions_merge_before_texture_upload() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 4,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0; RemoteDesktopFrame::expected_len(size).unwrap()],
            ),
        });
        drain_pending_texture_updates(&state);

        let stats = state.apply_frame_events([
            RemoteDesktopHelperEvent::FrameUpdate {
                update: RemoteDesktopFrameUpdate::new(
                    size,
                    RemoteDesktopRect::new(0, 0, 1, 1),
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![0x30, 0x20, 0x10, 0xff],
                ),
            },
            RemoteDesktopHelperEvent::FrameUpdate {
                update: RemoteDesktopFrameUpdate::new(
                    size,
                    RemoteDesktopRect::new(1, 0, 1, 1),
                    RemoteDesktopFrameFormat::Rgba8,
                    vec![0x60, 0x50, 0x40, 0xff],
                ),
            },
        ]);

        let updates = drain_pending_texture_updates(&state);
        assert_eq!(stats.full_update_recoveries, 0);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].rect, RemoteDesktopRect::new(0, 0, 2, 1));
        assert_eq!(
            updates[0].bytes,
            vec![0x10, 0x20, 0x30, 0xff, 0x40, 0x50, 0x60, 0xff]
        );
    }

    #[test]
    fn dirty_update_backlog_keeps_dirty_regions_without_full_texture_promotion() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 128,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0; RemoteDesktopFrame::expected_len(size).unwrap()],
            ),
        });
        drain_pending_texture_updates(&state);

        let update_count = 32_usize;
        let events = (0..update_count).map(|index| RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new((index * 2) as u32, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![index as u8, index as u8, index as u8, 0xff],
            ),
        });
        let stats = state.apply_frame_events(events);

        let updates = drain_pending_texture_updates(&state);
        assert_eq!(stats.dirty_updates_applied, update_count);
        assert_eq!(stats.full_update_recoveries, 0);
        assert_eq!(stats.pending_texture_updates, update_count);
        assert_eq!(stats.pending_texture_upload_bytes, update_count * 4);
        assert_eq!(updates.len(), update_count);
        assert_eq!(updates[0].rect, RemoteDesktopRect::new(0, 0, 1, 1));
        assert_eq!(
            updates[update_count - 1].rect,
            RemoteDesktopRect::new(((update_count - 1) * 2) as u32, 0, 1, 1)
        );
        assert_eq!(updates[update_count - 1].bytes, vec![31, 31, 31, 0xff]);
    }

    #[test]
    fn high_fragmentation_dirty_regions_are_bounded_without_full_texture_promotion() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let size = RemoteDesktopSize {
            width: 256,
            height: 1,
        };
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                size,
                RemoteDesktopFrameFormat::Rgba8,
                vec![0; RemoteDesktopFrame::expected_len(size).unwrap()],
            ),
        });
        drain_pending_texture_updates(&state);

        let update_count = REMOTE_DESKTOP_MAX_TEXTURE_UPLOAD_RECTS + 20;
        let events = (0..update_count).map(|index| RemoteDesktopHelperEvent::FrameUpdate {
            update: RemoteDesktopFrameUpdate::new(
                size,
                RemoteDesktopRect::new((index * 2) as u32, 0, 1, 1),
                RemoteDesktopFrameFormat::Rgba8,
                vec![index as u8, index as u8, index as u8, 0xff],
            ),
        });
        let stats = state.apply_frame_events(events);

        let updates = drain_pending_texture_updates(&state);
        assert_eq!(stats.dirty_updates_applied, update_count);
        assert_eq!(stats.full_update_recoveries, 0);
        assert_eq!(stats.pending_texture_updates, update_count);
        assert!(updates.len() <= REMOTE_DESKTOP_MAX_TEXTURE_UPLOAD_RECTS);
        assert!(
            !updates
                .iter()
                .any(|update| is_full_frame_rect(size, update.rect))
        );
    }

    #[test]
    fn cursor_event_does_not_replace_cached_dynamic_texture() {
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
        let texture = frame_texture(&state);

        state.apply_event(RemoteDesktopHelperEvent::Cursor {
            x: 10,
            y: 20,
            width: 1,
            height: 1,
        });

        let cached_texture = frame_texture(&state);
        assert!(Arc::ptr_eq(&texture, &cached_texture));
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
            frame_bgra_bytes(&state),
            [0x10, 0x20, 0x30, 0xff].as_slice()
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
            frame_bgra_bytes(&state),
            [0x10, 0x20, 0x30, 0xff].as_slice()
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
    fn failure_event_retires_existing_dynamic_texture() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        state.apply_event(RemoteDesktopHelperEvent::Frame {
            frame: RemoteDesktopFrame::new(
                RemoteDesktopSize {
                    width: 1,
                    height: 1,
                },
                RemoteDesktopFrameFormat::Rgba8,
                vec![0, 0, 0, 255],
            ),
        });
        let frame_texture = frame_texture(&state);

        state.apply_event(RemoteDesktopHelperEvent::ConnectionFailure {
            message: "transport failed".to_string(),
            category: Some(RemoteDesktopErrorCategory::Unknown),
        });

        let retired = state.take_retired_textures();
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &frame_texture));
        assert!(!state.snapshot().has_frame);
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
    fn terminated_event_does_not_hide_failure_reason() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);

        state.apply_event(RemoteDesktopHelperEvent::ConnectionFailure {
            message: "RDP transport failed".to_string(),
            category: Some(RemoteDesktopErrorCategory::Unknown),
        });
        state.apply_event(RemoteDesktopHelperEvent::Terminated { exit_code: Some(0) });

        let snapshot = state.snapshot();
        assert_eq!(snapshot.status, RemoteDesktopSessionStatus::Failed);
        assert_eq!(snapshot.message.as_deref(), Some("RDP transport failed"));
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

    #[test]
    fn cursor_shape_caches_image_and_retires_replaced_images() {
        let mut state = RemoteDesktopViewState::new("Server", RemoteDesktopProtocol::Rdp);
        let first_shape = RemoteDesktopCursorShape::new(
            RemoteDesktopSize {
                width: 1,
                height: 1,
            },
            0,
            0,
            RemoteDesktopFrameFormat::Rgba8,
            vec![0x30, 0x20, 0x10, 0x40],
        );
        let second_shape = RemoteDesktopCursorShape::new(
            RemoteDesktopSize {
                width: 1,
                height: 1,
            },
            0,
            0,
            RemoteDesktopFrameFormat::Rgba8,
            vec![0x60, 0x50, 0x40, 0x70],
        );

        state.apply_event(RemoteDesktopHelperEvent::CursorShape { shape: first_shape });
        let first_image = state
            .cursor_image()
            .expect("cursor shape should create a cached image");
        assert_eq!(
            first_image.as_bytes(0),
            Some([0x10, 0x20, 0x30, 0x40].as_slice())
        );

        state.apply_event(RemoteDesktopHelperEvent::CursorShape {
            shape: second_shape,
        });
        let retired = state.take_retired_images();
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &first_image));

        let second_image = state
            .cursor_image()
            .expect("replacement cursor should stay cached");
        state.apply_event(RemoteDesktopHelperEvent::CursorDefault);
        let retired = state.take_retired_images();
        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &second_image));
        assert!(state.cursor_image().is_none());
    }
}
