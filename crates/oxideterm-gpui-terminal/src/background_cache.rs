use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::UNIX_EPOCH,
};

use gpui::RenderImage;
use image::{Frame, RgbaImage};

use crate::terminal_ui::TerminalBackgroundPreferences;

const DEFAULT_BACKGROUND_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;

pub struct BackgroundImageRenderCache {
    entries: HashMap<BackgroundImageCacheKey, CachedBackgroundImage>,
    order: VecDeque<BackgroundImageCacheKey>,
    bytes: usize,
    byte_limit: usize,
}

struct CachedBackgroundImage {
    image: Arc<RenderImage>,
    bytes: usize,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct BackgroundImageCacheKey {
    path: PathBuf,
    blur_millis: u32,
    modified_millis: Option<u128>,
    len: Option<u64>,
}

impl BackgroundImageRenderCache {
    pub fn set_byte_limit(&mut self, byte_limit: usize) {
        self.byte_limit = byte_limit;
        self.evict_over_budget();
    }

    pub fn render_blurred_image(
        &mut self,
        background: &TerminalBackgroundPreferences,
    ) -> Option<Arc<RenderImage>> {
        if background.blur <= 0.01 {
            return None;
        }

        let key = BackgroundImageCacheKey::new(background);
        if self.entries.contains_key(&key) {
            self.touch(&key);
            return self.entries.get(&key).map(|entry| entry.image.clone());
        }

        let pixels = image::open(&background.path)
            .ok()?
            .blur(background.blur)
            .into_rgba8();
        let width = pixels.width();
        let height = pixels.height();
        let bytes = pixels.len();
        let mut pixels = pixels.into_raw();
        convert_rgba_pixels_to_gpui_bgra(&mut pixels);
        let buffer = RgbaImage::from_raw(width, height, pixels)?;
        let image = Arc::new(RenderImage::new(vec![Frame::new(buffer)]));

        self.entries.insert(
            key.clone(),
            CachedBackgroundImage {
                image: image.clone(),
                bytes,
            },
        );
        self.order.push_back(key);
        self.bytes += bytes;
        self.evict_over_budget();
        Some(image)
    }

    fn touch(&mut self, key: &BackgroundImageCacheKey) {
        self.order.retain(|existing| existing != key);
        self.order.push_back(key.clone());
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

impl Default for BackgroundImageRenderCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            byte_limit: DEFAULT_BACKGROUND_IMAGE_CACHE_BYTES,
        }
    }
}

impl BackgroundImageCacheKey {
    fn new(background: &TerminalBackgroundPreferences) -> Self {
        let metadata = background.path.metadata().ok();
        let modified_millis = metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis());

        Self {
            path: background.path.clone(),
            blur_millis: (background.blur.max(0.0) * 1000.0).round() as u32,
            modified_millis,
            len: metadata.map(|metadata| metadata.len()),
        }
    }
}

fn convert_rgba_pixels_to_gpui_bgra(pixels: &mut [u8]) {
    // GPUI 0.2.2 RenderImage consumes BGRA bytes. Keep the app-facing image
    // data in normal RGBA and isolate the texture contract here.
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}
