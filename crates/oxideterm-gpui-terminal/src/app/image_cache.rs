use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use gpui::RenderImage;
use image::{Frame, RgbaImage};
use oxideterm_terminal::{TerminalImageId, TerminalImageSnapshot};

const DEFAULT_RENDER_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone)]
pub(crate) struct TerminalRenderedImage {
    pub(crate) snapshot: TerminalImageSnapshot,
    pub(crate) render_image: Option<Arc<RenderImage>>,
}

pub(crate) struct ImageRenderCache {
    entries: HashMap<(TerminalImageId, u64), CachedRenderImage>,
    order: VecDeque<(TerminalImageId, u64)>,
    bytes: usize,
    byte_limit: usize,
}

struct CachedRenderImage {
    image: Arc<RenderImage>,
    bytes: usize,
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
                let render_image = if decode_images {
                    snapshot
                        .data
                        .as_ref()
                        .and_then(|data| self.image_for_snapshot(&snapshot, data.rgba.len()))
                } else {
                    None
                };
                TerminalRenderedImage {
                    snapshot,
                    render_image,
                }
            })
            .collect()
    }

    fn image_for_snapshot(
        &mut self,
        snapshot: &TerminalImageSnapshot,
        byte_len: usize,
    ) -> Option<Arc<RenderImage>> {
        let key = (snapshot.id, snapshot.version);
        if self.entries.contains_key(&key) {
            self.touch(key);
            return self.entries.get(&key).map(|cached| cached.image.clone());
        }

        let data = snapshot.data.as_ref()?;
        let pixels = gpui_render_image_pixels_from_protocol_rgba(data.rgba.to_vec());
        let buffer = RgbaImage::from_raw(data.width, data.height, pixels)?;
        let render_image = Arc::new(RenderImage::new(vec![Frame::new(buffer)]));
        self.entries.insert(
            key,
            CachedRenderImage {
                image: render_image.clone(),
                bytes: byte_len,
            },
        );
        self.order.push_back(key);
        self.bytes += byte_len;
        self.evict_over_budget();
        Some(render_image)
    }

    fn touch(&mut self, key: (TerminalImageId, u64)) {
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
    use oxideterm_terminal::{TerminalImageData, TerminalImageProtocol, TerminalImageSnapshot};

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
            placeholder: true,
            version: 1,
            data: Some(TerminalImageData {
                id: TerminalImageId(7),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![0, 0, 0, 255].into(),
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
            placeholder: true,
            version: 1,
            data: Some(TerminalImageData {
                id: TerminalImageId(9),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255].into(),
                name: None,
            }),
        };

        let rendered = cache.render_images(&[snapshot], true);
        let image = rendered[0].render_image.as_ref().unwrap();

        assert_eq!(image.as_bytes(0), Some([0, 0, 255, 255].as_slice()));
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
            placeholder: true,
            version: 1,
            data: Some(TerminalImageData {
                id: TerminalImageId(11),
                protocol: TerminalImageProtocol::Kitty,
                version: 1,
                width: 1,
                height: 1,
                rgba: vec![255, 0, 0, 255].into(),
                name: None,
            }),
        };

        let rendered = cache.render_images(&[snapshot], false);

        assert!(rendered[0].render_image.is_none());
        assert!(cache.entries.is_empty());
    }
}
