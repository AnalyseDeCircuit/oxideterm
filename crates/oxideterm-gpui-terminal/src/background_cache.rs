use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{Arc, mpsc},
    time::{Duration, Instant, UNIX_EPOCH},
};

use gpui::RenderImage;
use image::{Frame, RgbaImage};

use crate::terminal_ui::TerminalBackgroundPreferences;

const DEFAULT_BACKGROUND_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;
const BACKGROUND_METADATA_RECHECK_INTERVAL: Duration = Duration::from_secs(2);

pub struct BackgroundImageRenderCache {
    entries: HashMap<BackgroundImageCacheKey, CachedBackgroundImage>,
    key_cache: HashMap<BackgroundImageRequestKey, CachedBackgroundImageKey>,
    order: VecDeque<BackgroundImageCacheKey>,
    pending: HashSet<BackgroundImageCacheKey>,
    retired_images: Vec<Arc<RenderImage>>,
    sender: mpsc::Sender<BackgroundImageLoadResult>,
    receiver: mpsc::Receiver<BackgroundImageLoadResult>,
    bytes: usize,
    byte_limit: usize,
}

struct CachedBackgroundImage {
    image: Arc<RenderImage>,
    bytes: usize,
}

struct CachedBackgroundImageKey {
    key: BackgroundImageCacheKey,
    checked_at: Instant,
}

enum BackgroundImageLoadResult {
    Loaded {
        key: BackgroundImageCacheKey,
        image: Arc<RenderImage>,
        bytes: usize,
    },
    Failed {
        key: BackgroundImageCacheKey,
    },
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct BackgroundImageRequestKey {
    path: PathBuf,
    blur_millis: u32,
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

    pub fn take_retired_images(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.retired_images)
    }

    pub fn render_blurred_image(
        &mut self,
        background: &TerminalBackgroundPreferences,
    ) -> Option<Arc<RenderImage>> {
        self.drain_completed();

        if background.blur <= 0.01 {
            return None;
        }

        let key = self.cached_key_for_background(background);
        if self.entries.contains_key(&key) {
            self.touch(&key);
            return self.entries.get(&key).map(|entry| entry.image.clone());
        }

        if self.pending.insert(key.clone()) {
            let sender = self.sender.clone();
            let background = background.clone();
            std::thread::spawn(move || {
                let result = match load_blurred_background_image(key.clone(), &background) {
                    Some((image, bytes)) => BackgroundImageLoadResult::Loaded { key, image, bytes },
                    None => BackgroundImageLoadResult::Failed { key },
                };
                let _ = sender.send(result);
            });
        }

        None
    }

    fn cached_key_for_background(
        &mut self,
        background: &TerminalBackgroundPreferences,
    ) -> BackgroundImageCacheKey {
        let request = BackgroundImageRequestKey::new(background);
        if let Some(cached) = self.key_cache.get(&request)
            && cached.checked_at.elapsed() < BACKGROUND_METADATA_RECHECK_INTERVAL
        {
            return cached.key.clone();
        }

        // The cache key includes file metadata so a changed image is eventually
        // reloaded, but metadata() must not run on every render/scroll frame.
        let key = BackgroundImageCacheKey::new(background);
        self.key_cache.insert(
            request,
            CachedBackgroundImageKey {
                key: key.clone(),
                checked_at: Instant::now(),
            },
        );
        key
    }

    pub fn drain_completed(&mut self) -> bool {
        let mut changed = false;
        while let Ok(result) = self.receiver.try_recv() {
            match result {
                BackgroundImageLoadResult::Loaded { key, image, bytes } => {
                    self.pending.remove(&key);
                    if let Some(existing) = self.entries.remove(&key) {
                        self.bytes = self.bytes.saturating_sub(existing.bytes);
                        self.retired_images.push(existing.image);
                    }
                    self.entries.insert(
                        key.clone(),
                        CachedBackgroundImage {
                            image: image.clone(),
                            bytes,
                        },
                    );
                    self.touch(&key);
                    self.bytes += bytes;
                    self.evict_over_budget();
                    changed = true;
                }
                BackgroundImageLoadResult::Failed { key } => {
                    self.pending.remove(&key);
                    self.key_cache.retain(|_, cached| cached.key != key);
                    changed = true;
                }
            }
        }
        changed
    }

    pub fn has_pending(&self) -> bool {
        !self.pending.is_empty()
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
                self.retired_images.push(entry.image);
            }
        }
    }
}

impl Default for BackgroundImageRenderCache {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            entries: HashMap::new(),
            key_cache: HashMap::new(),
            order: VecDeque::new(),
            pending: HashSet::new(),
            retired_images: Vec::new(),
            sender,
            receiver,
            bytes: 0,
            byte_limit: DEFAULT_BACKGROUND_IMAGE_CACHE_BYTES,
        }
    }
}

impl BackgroundImageRequestKey {
    fn new(background: &TerminalBackgroundPreferences) -> Self {
        Self {
            path: background.path.clone(),
            blur_millis: (background.blur.max(0.0) * 1000.0).round() as u32,
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

fn load_blurred_background_image(
    key: BackgroundImageCacheKey,
    background: &TerminalBackgroundPreferences,
) -> Option<(Arc<RenderImage>, usize)> {
    if key != BackgroundImageCacheKey::new(background) {
        return None;
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
    Some((Arc::new(RenderImage::new(vec![Frame::new(buffer)])), bytes))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn background_cache_tracks_evicted_images_for_gpui_drop() {
        let mut cache = BackgroundImageRenderCache::default();
        let key = BackgroundImageCacheKey {
            path: PathBuf::from("/tmp/background.png"),
            blur_millis: 1000,
            modified_millis: Some(1),
            len: Some(8),
        };
        let buffer = RgbaImage::from_raw(2, 1, vec![0, 0, 0, 255, 255, 255, 255, 255])
            .expect("test image should be valid");
        let image = Arc::new(RenderImage::new(vec![Frame::new(buffer)]));
        cache.entries.insert(
            key.clone(),
            CachedBackgroundImage {
                image: image.clone(),
                bytes: 8,
            },
        );
        cache.order.push_back(key);
        cache.bytes = 8;

        cache.set_byte_limit(4);
        let retired = cache.take_retired_images();

        assert_eq!(retired.len(), 1);
        assert!(Arc::ptr_eq(&retired[0], &image));
    }
}
