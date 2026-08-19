use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use gpui::RenderImage;
use image::{Delay, Frame, RgbaImage};
use oxideterm_terminal::{TerminalImageId, TerminalImageSnapshot};

use crate::image_budget::{release_image_bytes, try_reserve_image_bytes};

const DEFAULT_RENDER_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;
const IMAGE_PREPARATION_RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Clone)]
pub(crate) struct TerminalRenderedImage {
    pub(crate) snapshot: TerminalImageSnapshot,
    pub(crate) render_image: Option<Arc<RenderImage>>,
    pub(crate) animation_started_at: Option<Instant>,
}

pub(crate) struct ImageRenderCache {
    entries: HashMap<ImageCacheKey, CachedRenderImage>,
    pending: HashSet<ImageCacheKey>,
    retry_after: HashMap<ImageCacheKey, Instant>,
    retired_images: Vec<Arc<RenderImage>>,
    bytes: usize,
    byte_limit: usize,
    usage_clock: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ImageCacheKey {
    id: TerminalImageId,
    version: u64,
    source_x: u32,
    source_y: u32,
    source_width: u32,
    source_height: u32,
}

struct CachedRenderImage {
    image: Arc<RenderImage>,
    bytes: usize,
    animation_started_at: Option<Instant>,
    last_used: u64,
}

pub(crate) struct PreparedRenderImage {
    key: ImageCacheKey,
    frames: Vec<Frame>,
    bytes: usize,
}

impl Default for ImageRenderCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            pending: HashSet::new(),
            retry_after: HashMap::new(),
            retired_images: Vec::new(),
            bytes: 0,
            byte_limit: DEFAULT_RENDER_IMAGE_CACHE_BYTES,
            usage_clock: 0,
        }
    }
}

impl ImageRenderCache {
    pub(crate) fn set_byte_limit(&mut self, byte_limit: usize) {
        self.byte_limit = byte_limit;
        self.evict_over_budget(None);
    }

    pub(crate) fn take_retired_images(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.retired_images)
    }

    pub(crate) fn cached_images(
        &mut self,
        images: &[TerminalImageSnapshot],
        decode_images: bool,
    ) -> Vec<TerminalRenderedImage> {
        images
            .iter()
            .cloned()
            .map(|snapshot| {
                let (render_image, animation_started_at) = if decode_images {
                    match self.cached_image_for_snapshot(&snapshot) {
                        Some((image, animation_started_at)) => (Some(image), animation_started_at),
                        None => (None, None),
                    }
                } else {
                    (None, None)
                };
                TerminalRenderedImage {
                    snapshot,
                    render_image,
                    animation_started_at,
                }
            })
            .collect()
    }

    pub(crate) fn take_preparation_requests(
        &mut self,
        images: &[TerminalImageSnapshot],
        decode_images: bool,
    ) -> Vec<TerminalImageSnapshot> {
        if !decode_images {
            return Vec::new();
        }
        let mut requests = Vec::new();
        for snapshot in images.iter().filter(|snapshot| snapshot.data.is_some()) {
            let key = ImageCacheKey::from_snapshot(snapshot);
            if self.entries.contains_key(&key) || self.pending.contains(&key) {
                continue;
            }
            if self
                .retry_after
                .get(&key)
                .is_some_and(|retry_after| Instant::now() < *retry_after)
            {
                continue;
            }
            self.retry_after.remove(&key);
            self.pending.insert(key);
            requests.push(snapshot.clone());
        }
        requests
    }

    pub(crate) fn prepare_snapshot(snapshot: TerminalImageSnapshot) -> Option<PreparedRenderImage> {
        let key = ImageCacheKey::from_snapshot(&snapshot);
        let data = snapshot.data.as_deref()?;
        let (frames, bytes) = render_frames_for_snapshot(data, &snapshot)?;
        Some(PreparedRenderImage { key, frames, bytes })
    }

    pub(crate) fn finish_preparations(
        &mut self,
        requested: &[TerminalImageSnapshot],
        prepared: Vec<PreparedRenderImage>,
    ) {
        let prepared_keys = prepared
            .iter()
            .map(|prepared| prepared.key)
            .collect::<HashSet<_>>();
        for snapshot in requested {
            let key = ImageCacheKey::from_snapshot(snapshot);
            self.pending.remove(&key);
            if !prepared_keys.contains(&key) {
                self.retry_after
                    .insert(key, Instant::now() + IMAGE_PREPARATION_RETRY_DELAY);
            }
        }
        for prepared in prepared {
            if self.entries.contains_key(&prepared.key) {
                continue;
            }
            self.evict_for_admission(prepared.bytes);
            if !try_reserve_image_bytes(prepared.bytes) {
                self.retry_after
                    .insert(prepared.key, Instant::now() + IMAGE_PREPARATION_RETRY_DELAY);
                continue;
            }
            self.retry_after.remove(&prepared.key);
            self.usage_clock = self.usage_clock.wrapping_add(1);
            let render_image = Arc::new(RenderImage::new(prepared.frames));
            let animation_started_at = (render_image.frame_count() > 1).then(Instant::now);
            self.entries.insert(
                prepared.key,
                CachedRenderImage {
                    image: render_image,
                    bytes: prepared.bytes,
                    animation_started_at,
                    last_used: self.usage_clock,
                },
            );
            self.bytes += prepared.bytes;
            self.evict_over_budget(Some(prepared.key));
        }
    }

    fn cached_image_for_snapshot(
        &mut self,
        snapshot: &TerminalImageSnapshot,
    ) -> Option<(Arc<RenderImage>, Option<Instant>)> {
        let key = ImageCacheKey::from_snapshot(snapshot);
        self.usage_clock = self.usage_clock.wrapping_add(1);
        if let Some(cached) = self.entries.get_mut(&key) {
            // Cache hits remain O(1); eviction scans only when admitting new pixel data.
            cached.last_used = self.usage_clock;
            return Some((cached.image.clone(), cached.animation_started_at));
        }

        None
    }

    fn evict_for_admission(&mut self, bytes: usize) {
        while self.bytes.saturating_add(bytes) > self.byte_limit && !self.entries.is_empty() {
            if !self.evict_oldest(None) {
                break;
            }
        }
    }

    fn evict_over_budget(&mut self, protected: Option<ImageCacheKey>) {
        // Keep one oversized image resident. Re-decoding it every frame is more expensive than
        // temporarily exceeding the configured budget, while all competing entries are evicted.
        while self.bytes > self.byte_limit && self.entries.len() > 1 {
            if !self.evict_oldest(protected) {
                break;
            }
        }
    }

    fn evict_oldest(&mut self, protected: Option<ImageCacheKey>) -> bool {
        let Some(key) = self
            .entries
            .iter()
            .filter(|(key, _)| Some(**key) != protected)
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(key, _)| *key)
        else {
            return false;
        };
        let Some(entry) = self.entries.remove(&key) else {
            return false;
        };
        self.bytes = self.bytes.saturating_sub(entry.bytes);
        release_image_bytes(entry.bytes);
        self.retired_images.push(entry.image);
        true
    }

    #[cfg(test)]
    fn render_images(
        &mut self,
        images: &[TerminalImageSnapshot],
        decode_images: bool,
    ) -> Vec<TerminalRenderedImage> {
        let requested = self.take_preparation_requests(images, decode_images);
        let prepared = requested
            .iter()
            .cloned()
            .filter_map(Self::prepare_snapshot)
            .collect();
        self.finish_preparations(&requested, prepared);
        self.cached_images(images, decode_images)
    }
}

impl ImageCacheKey {
    fn from_snapshot(snapshot: &TerminalImageSnapshot) -> Self {
        Self {
            id: snapshot.id,
            version: snapshot.version,
            source_x: snapshot.source_x,
            source_y: snapshot.source_y,
            source_width: snapshot.source_width,
            source_height: snapshot.source_height,
        }
    }
}

impl Drop for ImageRenderCache {
    fn drop(&mut self) {
        release_image_bytes(self.bytes);
        self.bytes = 0;
    }
}

fn render_frames_for_snapshot(
    data: &oxideterm_terminal::TerminalImageData,
    snapshot: &TerminalImageSnapshot,
) -> Option<(Vec<Frame>, usize)> {
    if data.frames.is_empty() {
        let pixels = cropped_protocol_rgba_pixels(&data.rgba, data.width, data.height, snapshot);
        let byte_len = pixels.len();
        let pixels = gpui_render_image_pixels_from_protocol_rgba(pixels);
        let buffer = RgbaImage::from_raw(snapshot.source_width, snapshot.source_height, pixels)?;
        return Some((vec![Frame::new(buffer)], byte_len));
    }

    let mut byte_len = 0;
    let mut frames = Vec::with_capacity(data.frames.len());
    for frame in &data.frames {
        let pixels = cropped_protocol_rgba_pixels(&frame.rgba, data.width, data.height, snapshot);
        byte_len += pixels.len();
        let pixels = gpui_render_image_pixels_from_protocol_rgba(pixels);
        let buffer = RgbaImage::from_raw(snapshot.source_width, snapshot.source_height, pixels)?;
        let delay =
            Delay::from_numer_denom_ms(frame.delay_ms_numerator, frame.delay_ms_denominator.max(1));
        frames.push(Frame::from_parts(buffer, 0, 0, delay));
    }
    Some((frames, byte_len))
}

fn cropped_protocol_rgba_pixels(
    rgba: &[u8],
    image_width: u32,
    image_height: u32,
    snapshot: &TerminalImageSnapshot,
) -> Vec<u8> {
    let source_x = snapshot.source_x.min(image_width);
    let source_y = snapshot.source_y.min(image_height);
    let source_width = snapshot
        .source_width
        .min(image_width.saturating_sub(source_x));
    let source_height = snapshot
        .source_height
        .min(image_height.saturating_sub(source_y));

    if source_x == 0
        && source_y == 0
        && source_width == image_width
        && source_height == image_height
    {
        return rgba.to_vec();
    }

    let row_bytes = source_width as usize * 4;
    let mut cropped = Vec::with_capacity(row_bytes * source_height as usize);
    let stride = image_width as usize * 4;
    for row in source_y..source_y + source_height {
        let start = row as usize * stride + source_x as usize * 4;
        let end = start + row_bytes;
        cropped.extend_from_slice(&rgba[start..end]);
    }
    cropped
}

fn gpui_render_image_pixels_from_protocol_rgba(mut pixels: Vec<u8>) -> Vec<u8> {
    // GPUI 0.2.2 documents RenderImage as BGRA and its own img element performs
    // this same conversion before constructing RenderImage. Keep the protocol
    // state RGBA and isolate the GPUI texture contract at this boundary.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    pixels
}

#[cfg(test)]
mod tests {
    use oxideterm_terminal::{
        TerminalImageAnimationState, TerminalImageData, TerminalImageFrame, TerminalImageProtocol,
        TerminalImageSnapshot,
    };

    use super::*;

    #[test]
    fn render_cache_converts_protocol_rgba_to_gpui_bgra() {
        let mut cache = ImageRenderCache::default();
        let snapshot = TerminalImageSnapshot {
            id: TerminalImageId(9),
            protocol: TerminalImageProtocol::Kitty,
            row: 0,
            col: 0,
            cols: 1,
            rows: 1,
            pixel_width: 1,
            pixel_height: 1,
            source_x: 0,
            source_y: 0,
            source_width: 1,
            source_height: 1,
            z_index: 0,
            placeholder: true,
            version: 1,
            data: Some(Arc::new(TerminalImageData {
                id: TerminalImageId(9),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255].into(),
                frames: Vec::new(),
                animation: TerminalImageAnimationState::default(),
                name: None,
            })),
        };

        let rendered = cache.render_images(&[snapshot], true);
        let image = rendered[0].render_image.as_ref().unwrap();

        assert_eq!(image.as_bytes(0), Some([0, 0, 255, 255].as_slice()));
    }

    #[test]
    fn render_cache_crops_protocol_rgba_from_snapshot_source_rect() {
        let mut cache = ImageRenderCache::default();
        let snapshot = TerminalImageSnapshot {
            id: TerminalImageId(10),
            protocol: TerminalImageProtocol::Kitty,
            row: 0,
            col: 0,
            cols: 1,
            rows: 1,
            pixel_width: 2,
            pixel_height: 1,
            source_x: 1,
            source_y: 0,
            source_width: 1,
            source_height: 1,
            z_index: 0,
            placeholder: true,
            version: 1,
            data: Some(Arc::new(TerminalImageData {
                id: TerminalImageId(10),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 0, 255, 0, 255].into(),
                frames: Vec::new(),
                animation: TerminalImageAnimationState::default(),
                name: None,
            })),
        };

        let rendered = cache.render_images(&[snapshot], true);
        let image = rendered[0].render_image.as_ref().unwrap();

        assert_eq!(image.as_bytes(0), Some([0, 255, 0, 255].as_slice()));
    }

    #[test]
    fn render_cache_preserves_animation_frames_and_delays() {
        let mut cache = ImageRenderCache::default();
        let snapshot = TerminalImageSnapshot {
            id: TerminalImageId(12),
            protocol: TerminalImageProtocol::Kitty,
            row: 0,
            col: 0,
            cols: 1,
            rows: 1,
            pixel_width: 1,
            pixel_height: 1,
            source_x: 0,
            source_y: 0,
            source_width: 1,
            source_height: 1,
            z_index: 0,
            placeholder: true,
            version: 1,
            data: Some(Arc::new(TerminalImageData {
                id: TerminalImageId(12),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255].into(),
                frames: vec![
                    TerminalImageFrame {
                        rgba: vec![255, 0, 0, 255].into(),
                        delay_ms_numerator: 50,
                        delay_ms_denominator: 1,
                        gapless: false,
                    },
                    TerminalImageFrame {
                        rgba: vec![0, 255, 0, 255].into(),
                        delay_ms_numerator: 75,
                        delay_ms_denominator: 1,
                        gapless: false,
                    },
                ],
                animation: TerminalImageAnimationState {
                    running: true,
                    loading: false,
                    current_frame: 0,
                    loop_limit: None,
                },
                name: None,
            })),
        };

        let rendered = cache.render_images(&[snapshot], true);
        let image = rendered[0].render_image.as_ref().unwrap();

        assert_eq!(image.frame_count(), 2);
        assert_eq!(image.delay(0).numer_denom_ms(), (50, 1));
        assert_eq!(image.delay(1).numer_denom_ms(), (75, 1));
        assert_eq!(image.as_bytes(0), Some([0, 0, 255, 255].as_slice()));
        assert_eq!(image.as_bytes(1), Some([0, 255, 0, 255].as_slice()));
        assert!(rendered[0].animation_started_at.is_some());
    }

    #[test]
    fn render_cache_can_suppress_decode_for_compatibility_mode() {
        let mut cache = ImageRenderCache::default();
        let snapshot = TerminalImageSnapshot {
            id: TerminalImageId(11),
            protocol: TerminalImageProtocol::Kitty,
            row: 0,
            col: 0,
            cols: 1,
            rows: 1,
            pixel_width: 1,
            pixel_height: 1,
            source_x: 0,
            source_y: 0,
            source_width: 1,
            source_height: 1,
            z_index: 0,
            placeholder: true,
            version: 1,
            data: Some(Arc::new(TerminalImageData {
                id: TerminalImageId(11),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255].into(),
                frames: Vec::new(),
                animation: TerminalImageAnimationState::default(),
                name: None,
            })),
        };

        let rendered = cache.render_images(&[snapshot], false);

        assert!(rendered[0].render_image.is_none());
        assert!(cache.entries.is_empty());
    }
}
