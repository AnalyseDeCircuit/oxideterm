use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Instant,
};

use gpui::RenderImage;
use image::{Delay, Frame, RgbaImage};
use oxideterm_terminal::{TerminalImageId, TerminalImageSnapshot};

const DEFAULT_RENDER_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct TerminalRenderedImage {
    pub(crate) snapshot: TerminalImageSnapshot,
    pub(crate) render_image: Option<Arc<RenderImage>>,
    pub(crate) animation_started_at: Option<Instant>,
}

pub(crate) struct ImageRenderCache {
    entries: HashMap<ImageCacheKey, CachedRenderImage>,
    order: VecDeque<ImageCacheKey>,
    bytes: usize,
    byte_limit: usize,
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
}

impl Default for ImageRenderCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            byte_limit: DEFAULT_RENDER_IMAGE_CACHE_BYTES,
        }
    }
}

impl ImageRenderCache {
    pub(crate) fn set_byte_limit(&mut self, byte_limit: usize) {
        self.byte_limit = byte_limit;
        self.evict_over_budget();
    }

    pub(crate) fn render_images(
        &mut self,
        images: &[TerminalImageSnapshot],
        decode_images: bool,
    ) -> Vec<TerminalRenderedImage> {
        images
            .iter()
            .cloned()
            .map(|snapshot| {
                let (render_image, animation_started_at) = if decode_images {
                    match snapshot
                        .data
                        .as_ref()
                        .and_then(|_| self.image_for_snapshot(&snapshot))
                    {
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

    fn image_for_snapshot(
        &mut self,
        snapshot: &TerminalImageSnapshot,
    ) -> Option<(Arc<RenderImage>, Option<Instant>)> {
        let key = ImageCacheKey {
            id: snapshot.id,
            version: snapshot.version,
            source_x: snapshot.source_x,
            source_y: snapshot.source_y,
            source_width: snapshot.source_width,
            source_height: snapshot.source_height,
        };
        if self.entries.contains_key(&key) {
            self.touch(key);
            return self
                .entries
                .get(&key)
                .map(|cached| (cached.image.clone(), cached.animation_started_at));
        }

        let data = snapshot.data.as_ref()?;
        let (frames, byte_len) = render_frames_for_snapshot(data, snapshot)?;
        let render_image = Arc::new(RenderImage::new(frames));
        let animation_started_at = (render_image.frame_count() > 1).then(Instant::now);
        self.entries.insert(
            key,
            CachedRenderImage {
                image: render_image.clone(),
                bytes: byte_len,
                animation_started_at,
            },
        );
        self.order.push_back(key);
        self.bytes += byte_len;
        self.evict_over_budget();
        Some((render_image, animation_started_at))
    }

    fn touch(&mut self, key: ImageCacheKey) {
        self.order.retain(|existing| *existing != key);
        self.order.push_back(key);
    }

    fn evict_over_budget(&mut self) {
        while self.bytes > self.byte_limit {
            let Some(key) = self.order.pop_front() else {
                self.bytes = 0;
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
            }
        }
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
    fn render_cache_reuses_same_image_version() {
        let mut cache = ImageRenderCache::default();
        let snapshot = TerminalImageSnapshot {
            id: TerminalImageId(7),
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
            data: Some(TerminalImageData {
                id: TerminalImageId(7),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![0, 0, 0, 255].into(),
                frames: Vec::new(),
                animation: TerminalImageAnimationState::default(),
                name: None,
            }),
        };

        let first = cache.render_images(std::slice::from_ref(&snapshot), true);
        let second = cache.render_images(std::slice::from_ref(&snapshot), true);

        let first = first[0].render_image.as_ref().unwrap();
        let second = second[0].render_image.as_ref().unwrap();
        assert!(Arc::ptr_eq(first, second));
    }

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
            data: Some(TerminalImageData {
                id: TerminalImageId(9),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255].into(),
                frames: Vec::new(),
                animation: TerminalImageAnimationState::default(),
                name: None,
            }),
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
            data: Some(TerminalImageData {
                id: TerminalImageId(10),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 2,
                height: 1,
                rgba: vec![255, 0, 0, 255, 0, 255, 0, 255].into(),
                frames: Vec::new(),
                animation: TerminalImageAnimationState::default(),
                name: None,
            }),
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
            data: Some(TerminalImageData {
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
            }),
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
    fn gpui_pixel_adapter_leaves_alpha_and_green_unchanged() {
        let pixels = gpui_render_image_pixels_from_protocol_rgba(vec![1, 2, 3, 4, 5, 6, 7, 8]);

        assert_eq!(pixels, vec![3, 2, 1, 4, 7, 6, 5, 8]);
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
            data: Some(TerminalImageData {
                id: TerminalImageId(11),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255].into(),
                frames: Vec::new(),
                animation: TerminalImageAnimationState::default(),
                name: None,
            }),
        };

        let rendered = cache.render_images(&[snapshot], false);

        assert!(rendered[0].render_image.is_none());
        assert!(cache.entries.is_empty());
    }
}
