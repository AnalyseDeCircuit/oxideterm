use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

use gpui::{RenderImage, Size};
use image::Frame;

use crate::{
    BackgroundFit as TerminalBackgroundFit, BackgroundPreferences as TerminalBackgroundPreferences,
};

const DEFAULT_BACKGROUND_IMAGE_CACHE_BYTES: usize = 64 * 1024 * 1024;
const BACKGROUND_METADATA_RECHECK_INTERVAL: Duration = Duration::from_secs(2);
// Quantizes the display target so small viewport changes reuse the cached
// texture instead of re-decoding the background on every intermediate resize.
const BACKGROUND_TARGET_ALIGN: u32 = 64;
// Serialize source decodes across panes; cache admission only accounts for
// final pixels, not the temporary full-resolution image.
static BACKGROUND_DECODE_LOCK: Mutex<()> = Mutex::new(());

/// The device-pixel size the background image should be downscaled to.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BackgroundImageTargetSize {
    width: u32,
    height: u32,
}

/// Quantizes a logical display size to an aligned device-pixel target so the
/// background cache key stays stable during incremental window resizes.
pub fn background_display_target(
    size: Size<gpui::Pixels>,
    scale_factor: f32,
) -> BackgroundImageTargetSize {
    let aligned = |logical: gpui::Pixels| {
        let pixels = (logical.as_f32() * scale_factor).ceil().max(1.0) as u32;
        pixels
            .div_ceil(BACKGROUND_TARGET_ALIGN)
            .saturating_mul(BACKGROUND_TARGET_ALIGN)
    };
    BackgroundImageTargetSize {
        width: aligned(size.width),
        height: aligned(size.height),
    }
}

pub struct BackgroundImageRenderCache {
    entries: HashMap<BackgroundImageCacheKey, CachedBackgroundImage>,
    key_cache: HashMap<BackgroundImageRequestKey, CachedBackgroundImageKey>,
    order: VecDeque<BackgroundImageCacheKey>,
    pending: HashSet<BackgroundImageCacheKey>,
    cancelled: Arc<AtomicBool>,
    failed: Option<BackgroundImageCacheKey>,
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
    target: BackgroundImageTargetSize,
    fit: TerminalBackgroundFit,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct BackgroundImageCacheKey {
    path: PathBuf,
    blur_millis: u32,
    target: BackgroundImageTargetSize,
    fit: TerminalBackgroundFit,
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

    pub(crate) fn take_all_images(&mut self) -> Vec<Arc<RenderImage>> {
        self.cancelled.store(true, Ordering::Release);
        let mut images = self.take_retired_images();
        images.extend(self.entries.drain().map(|(_, entry)| entry.image));
        images
    }

    pub fn render_background_image(
        &mut self,
        background: &TerminalBackgroundPreferences,
        target: BackgroundImageTargetSize,
    ) -> Option<Arc<RenderImage>> {
        if background.path.as_os_str().is_empty() || background.opacity <= 0.0 {
            return None;
        }
        self.drain_completed();

        // Native-size backgrounds retain GPUI's original loading path; reducing
        // their dimensions would alter ObjectFit::None's scale and crop.
        if background.fit == TerminalBackgroundFit::Tile && background.blur <= 0.01 {
            return None;
        }

        let key = self.cached_key_for_background(background, target);
        if self.entries.contains_key(&key) {
            self.touch(&key);
            return self.entries.get(&key).map(|entry| entry.image.clone());
        }

        // Keep at most one request per cache. New resize targets are picked up
        // by the completion-triggered render, without queuing intermediate sizes.
        if self.pending.is_empty() && self.failed.as_ref() != Some(&key) {
            self.pending.insert(key.clone());
            let sender = self.sender.clone();
            let background = background.clone();
            let cancelled = self.cancelled.clone();
            let load_key = key.clone();
            std::thread::spawn(move || {
                let key = load_key;
                let _decode = BACKGROUND_DECODE_LOCK
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if cancelled.load(Ordering::Acquire) {
                    return;
                }
                let result = match load_background_image(key.clone(), &background, target) {
                    Some((image, bytes)) => BackgroundImageLoadResult::Loaded { key, image, bytes },
                    None => BackgroundImageLoadResult::Failed { key },
                };
                let _ = sender.send(result);
            });
        }

        // Keep the previous resolution visible while a resized image loads.
        self.order.iter().rev().find_map(|cached| {
            (cached.path == background.path
                && cached.blur_millis == key.blur_millis
                && cached.fit == background.fit)
                .then(|| self.entries.get(cached).map(|entry| entry.image.clone()))
                .flatten()
        })
    }

    fn cached_key_for_background(
        &mut self,
        background: &TerminalBackgroundPreferences,
        target: BackgroundImageTargetSize,
    ) -> BackgroundImageCacheKey {
        let request = BackgroundImageRequestKey::new(background, target);
        if let Some(cached) = self.key_cache.get(&request)
            && cached.checked_at.elapsed() < BACKGROUND_METADATA_RECHECK_INTERVAL
        {
            return cached.key.clone();
        }

        // The cache key includes file metadata so a changed image is eventually
        // reloaded, but metadata() must not run on every render/scroll frame.
        let key = BackgroundImageCacheKey::new(background, target);
        // Key entries are keyed by the quantized target size, so they would
        // grow without bound across every distinct resize step. Entries older
        // than the recheck interval are useless anyway, so drop them on insert.
        self.key_cache
            .retain(|_, cached| cached.checked_at.elapsed() < BACKGROUND_METADATA_RECHECK_INTERVAL);
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
                    self.evict_for_admission(bytes);
                    if bytes > self.byte_limit {
                        self.failed = Some(key);
                        self.retired_images.push(image);
                        changed = true;
                        continue;
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
                    self.failed = Some(key);
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

    fn evict_for_admission(&mut self, bytes: usize) {
        while self.bytes.saturating_add(bytes) > self.byte_limit {
            let Some(key) = self.order.pop_front() else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.bytes = self.bytes.saturating_sub(entry.bytes);
                self.retired_images.push(entry.image);
            }
        }
    }
}

impl Drop for BackgroundImageRenderCache {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.bytes = 0;
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
            cancelled: Arc::new(AtomicBool::new(false)),
            failed: None,
            retired_images: Vec::new(),
            sender,
            receiver,
            bytes: 0,
            byte_limit: DEFAULT_BACKGROUND_IMAGE_CACHE_BYTES,
        }
    }
}

impl BackgroundImageRequestKey {
    fn new(background: &TerminalBackgroundPreferences, target: BackgroundImageTargetSize) -> Self {
        Self {
            path: background.path.clone(),
            blur_millis: (background.blur.max(0.0) * 1000.0).round() as u32,
            target,
            fit: background.fit,
        }
    }
}

impl BackgroundImageCacheKey {
    fn new(background: &TerminalBackgroundPreferences, target: BackgroundImageTargetSize) -> Self {
        let metadata = background.path.metadata().ok();
        let modified_millis = metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok())
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis());

        Self {
            path: background.path.clone(),
            blur_millis: (background.blur.max(0.0) * 1000.0).round() as u32,
            target,
            fit: background.fit,
            modified_millis,
            len: metadata.map(|metadata| metadata.len()),
        }
    }
}

fn load_background_image(
    key: BackgroundImageCacheKey,
    background: &TerminalBackgroundPreferences,
    target: BackgroundImageTargetSize,
) -> Option<(Arc<RenderImage>, usize)> {
    if key != BackgroundImageCacheKey::new(background, target) {
        return None;
    }
    let (_, mut frame) = oxideterm_background_media::decode_poster(
        &background.path,
        oxideterm_background_media::OutputParams {
            width: target.width,
            height: target.height,
            fit: background.fit,
            blur: background.blur,
            limits: background.limits,
        },
    )
    .ok()?;
    let bytes = frame.pixels.len();
    let pixels =
        image::RgbaImage::from_raw(frame.width, frame.height, std::mem::take(&mut frame.pixels))?;
    let image = RenderImage::new(vec![Frame::new(pixels)]).retaining(frame);
    Some((Arc::new(image), bytes))
}

#[cfg(test)]
mod performance_probe {
    use super::*;
    use crate::BackgroundFit as TerminalBackgroundFit;
    use image::RgbaImage;
    use std::fs::File;

    #[test]
    fn display_target_accounts_for_dpi_and_quantizes_resize_steps() {
        assert_eq!(
            background_display_target(Size::new(gpui::px(950.0), gpui::px(530.0)), 2.0),
            BackgroundImageTargetSize {
                width: 1920,
                height: 1088
            },
        );
    }

    #[test]
    fn changing_fit_loads_new_pixels_instead_of_reusing_previous_texture() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("background.png");
        RgbaImage::from_pixel(600, 400, image::Rgba([20, 40, 60, 255]))
            .save(&path)
            .unwrap();
        let mut background = TerminalBackgroundPreferences {
            path,
            opacity: 1.0,
            blur: 0.0,
            fit: TerminalBackgroundFit::Cover,
            alignment: (0.5, 0.5),
            effect: Some(crate::GeneratedEffectPreferences {
                kind: crate::GeneratedEffectKind::Fog,
                strength: 0.35,
                sheen: 0.0,
                max_fps: None,
                colors: [0x8090aa, 0x90aa80],
                speed: 1.0,
                size: 1.0,
                brightness: 0.6,
                roughness: 0.6,
                direction: 25.0,
                particle_count: 12,
            }),
            readability: None,
            limits: Default::default(),
            scene: Default::default(),
            on_failure: None,
        };
        let target = BackgroundImageTargetSize {
            width: 192,
            height: 64,
        };
        let mut cache = BackgroundImageRenderCache::default();
        for (fit, bytes) in [
            (TerminalBackgroundFit::Cover, 192 * 128 * 4),
            (TerminalBackgroundFit::Contain, 96 * 64 * 4),
        ] {
            background.fit = fit;
            assert!(cache.render_background_image(&background, target).is_none());
            // Await the actual worker result before allowing cache admission.
            let result = cache
                .receiver
                .recv_timeout(Duration::from_secs(10))
                .unwrap();
            match &result {
                BackgroundImageLoadResult::Loaded { bytes: actual, .. } => {
                    assert_eq!(*actual, bytes)
                }
                BackgroundImageLoadResult::Failed { .. } => panic!("background decode failed"),
            }
            cache.sender.send(result).unwrap();
            assert!(cache.render_background_image(&background, target).is_some());
        }
    }

    #[test]
    fn resize_requests_keep_one_worker_and_retain_the_visible_image() {
        let _decode = BACKGROUND_DECODE_LOCK.lock().unwrap();
        let mut cache = BackgroundImageRenderCache::default();
        let directory = tempfile::tempdir().unwrap();
        let background = TerminalBackgroundPreferences {
            path: directory.path().join("pending-background.png"),
            opacity: 1.0,
            blur: 0.0,
            fit: TerminalBackgroundFit::Cover,
            alignment: (0.5, 0.5),
            effect: None,
            readability: None,
            limits: Default::default(),
            scene: Default::default(),
            on_failure: None,
        };
        let first = BackgroundImageTargetSize {
            width: 192,
            height: 128,
        };
        let image = Arc::new(RenderImage::new(vec![Frame::new(RgbaImage::new(1, 1))]));
        cache
            .sender
            .send(BackgroundImageLoadResult::Loaded {
                key: BackgroundImageCacheKey::new(
                    &background,
                    BackgroundImageTargetSize {
                        width: 64,
                        height: 64,
                    },
                ),
                image: image.clone(),
                bytes: 4,
            })
            .unwrap();
        assert!(Arc::ptr_eq(
            &cache.render_background_image(&background, first).unwrap(),
            &image
        ));
        for width in [256, 320, 384] {
            assert!(Arc::ptr_eq(
                &cache
                    .render_background_image(
                        &background,
                        BackgroundImageTargetSize { width, ..first }
                    )
                    .unwrap(),
                &image
            ));
        }
        assert_eq!(cache.pending.len(), 1);
        assert_eq!(cache.pending.iter().next().unwrap().target, first);
        // Dropping the cache cancels this worker before it acquires the decode lock.
    }

    #[test]
    fn animation_poster_contains_only_the_processed_first_frame() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("background.gif");
        let mut encoder = image::codecs::gif::GifEncoder::new(File::create(&path).unwrap());
        for (color, millis) in [([255, 0, 0, 255], 100), ([0, 0, 255, 255], 200)] {
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(64, 64, image::Rgba(color)),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(millis, 1),
                ))
                .unwrap();
        }
        drop(encoder);
        let mut background = TerminalBackgroundPreferences {
            path,
            opacity: 1.0,
            blur: 0.0,
            fit: TerminalBackgroundFit::Cover,
            alignment: (0.5, 0.5),
            effect: None,
            readability: None,
            limits: Default::default(),
            scene: Default::default(),
            on_failure: None,
        };
        let target = BackgroundImageTargetSize {
            width: 32,
            height: 32,
        };
        for blur in [0.0, 4.0] {
            background.blur = blur;
            let (image, bytes) = load_background_image(
                BackgroundImageCacheKey::new(&background, target),
                &background,
                target,
            )
            .unwrap();
            assert_eq!(image.frame_count(), 1, "blur={blur}");
            assert_eq!(bytes, 32 * 32 * 4);
            assert_eq!(&image.as_bytes(0).unwrap()[..4], &[0, 0, 255, 255]);
            assert_eq!(image.delay(0).numer_denom_ms(), (0, 1));
        }
    }

    #[test]
    #[ignore = "manual large background comparison"]
    fn large_background_probe() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("background.png");
        RgbaImage::from_fn(6000, 4000, |x, y| {
            image::Rgba([(x % 251) as u8, (y % 251) as u8, 80, 255])
        })
        .save(&path)
        .unwrap();
        let background = TerminalBackgroundPreferences {
            path,
            opacity: 1.0,
            blur: 4.0,
            fit: TerminalBackgroundFit::Cover,
            alignment: (0.5, 0.5),
            effect: None,
            readability: None,
            limits: Default::default(),
            scene: Default::default(),
            on_failure: None,
        };
        let start = Instant::now();
        let target = BackgroundImageTargetSize {
            width: 1920,
            height: 1088,
        };
        let baseline = std::env::var_os("OXIDE_BACKGROUND_BASELINE").is_some();
        let bytes = if baseline {
            image::open(&background.path)
                .unwrap()
                .blur(background.blur)
                .into_rgba8()
                .len()
        } else {
            load_background_image(
                BackgroundImageCacheKey::new(&background, target),
                &background,
                target,
            )
            .unwrap()
            .1
        };
        eprintln!(
            "background: elapsed={:?}, retained_rgba_bytes={bytes}",
            start.elapsed()
        );
        assert_eq!(
            bytes,
            if baseline {
                6000 * 4000 * 4
            } else {
                1920 * 1280 * 4
            }
        );
    }
}
