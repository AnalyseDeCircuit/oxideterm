use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Instant,
};

use gpui::RenderImage;
use image::Delay;
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
    desired: HashSet<ImageCacheKey>,
    suspended_clocks: HashMap<ImageCacheKey, Instant>,
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
    source: Arc<oxideterm_terminal::TerminalImageData>,
}

pub(crate) struct PreparedRenderImage {
    key: ImageCacheKey,
    image: Arc<RenderImage>,
    source: Arc<oxideterm_terminal::TerminalImageData>,
    bytes: usize,
}

struct ImageUploadReservation(usize);

impl Drop for ImageUploadReservation {
    fn drop(&mut self) {
        release_image_bytes(self.0);
    }
}

impl Default for ImageRenderCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            pending: HashSet::new(),
            desired: HashSet::new(),
            suspended_clocks: HashMap::new(),
            retry_after: HashMap::new(),
            retired_images: Vec::new(),
            bytes: 0,
            byte_limit: DEFAULT_RENDER_IMAGE_CACHE_BYTES,
            usage_clock: 0,
        }
    }
}

impl ImageRenderCache {
    pub(crate) fn suspend(&mut self) -> Vec<Arc<RenderImage>> {
        self.suspended_clocks.clear();
        for (key, entry) in self.entries.drain() {
            if let Some(started) = entry.animation_started_at {
                self.suspended_clocks.insert(key, started);
            }
            self.retired_images.push(entry.image);
        }
        self.bytes = 0;
        self.pending.clear();
        self.desired.clear();
        self.retry_after.clear();
        self.take_retired_images()
    }

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
        self.desired = if decode_images {
            images.iter().map(ImageCacheKey::from_snapshot).collect()
        } else {
            HashSet::new()
        };
        self.retry_after.retain(|key, _| self.desired.contains(key));
        if !decode_images {
            return Vec::new();
        }
        let mut requests = Vec::new();
        for snapshot in images.iter().filter(|snapshot| snapshot.data.is_some()) {
            let key = ImageCacheKey::from_snapshot(snapshot);
            // Animation controls revise the snapshot without changing its pixels.
            // Reuse the uploaded frames, but keep the existing restart-clock semantics.
            if !self.entries.contains_key(&key) {
                let reusable = self.entries.iter().find_map(|(old_key, entry)| {
                    (old_key.id == key.id
                        && old_key.source_x == key.source_x
                        && old_key.source_y == key.source_y
                        && old_key.source_width == key.source_width
                        && old_key.source_height == key.source_height
                        && same_render_frames(&entry.source, snapshot.data.as_deref().unwrap()))
                    .then_some(*old_key)
                });
                if let Some(old_key) = reusable {
                    let mut entry = self.entries.remove(&old_key).unwrap();
                    entry.source = snapshot.data.as_ref().unwrap().clone();
                    entry.animation_started_at = (entry.image.frame_count() > 1).then(Instant::now);
                    self.entries.insert(key, entry);
                }
                // A newer revision supersedes every crop of the older image.
                let obsolete = self
                    .entries
                    .keys()
                    .copied()
                    .filter(|old| old.id == key.id && old.version != key.version)
                    .collect::<Vec<_>>();
                for old in obsolete {
                    self.retire(old);
                }
            }
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
        let source = snapshot.data.as_ref()?.clone();
        let upload_bytes = (snapshot.source_width as usize)
            .checked_mul(snapshot.source_height as usize)?
            .checked_mul(4)?
            .checked_mul(source.frames.len().max(1))?;
        // Reserve the full animation or static conversion workspace before allocating. The lease follows
        // the image through retirement and upload, not just the cache entry.
        if !try_reserve_image_bytes(upload_bytes) {
            return None;
        }
        let reservation = ImageUploadReservation(upload_bytes);
        let image = render_image_for_snapshot(&source, &snapshot)?.retaining(reservation);
        let bytes = if source.frames.is_empty() {
            source.rgba.len()
        } else {
            source.frames.iter().map(|frame| frame.rgba.len()).sum()
        };
        Some(PreparedRenderImage {
            key,
            image: Arc::new(image),
            source,
            bytes,
        })
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
            if self.desired.contains(&key) && !prepared_keys.contains(&key) {
                self.retry_after
                    .insert(key, Instant::now() + IMAGE_PREPARATION_RETRY_DELAY);
            }
        }
        for prepared in prepared {
            if !self.desired.contains(&prepared.key) || self.entries.contains_key(&prepared.key) {
                continue;
            }
            self.evict_for_admission(prepared.bytes);
            self.retry_after.remove(&prepared.key);
            self.usage_clock = self.usage_clock.wrapping_add(1);
            let render_image = prepared.image;
            let animation_started_at = (render_image.frame_count() > 1).then(|| {
                self.suspended_clocks
                    .remove(&prepared.key)
                    .unwrap_or_else(Instant::now)
            });
            self.entries.insert(
                prepared.key,
                CachedRenderImage {
                    image: render_image,
                    bytes: prepared.bytes,
                    animation_started_at,
                    last_used: self.usage_clock,
                    source: prepared.source,
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
        self.retire(key)
    }

    fn retire(&mut self, key: ImageCacheKey) -> bool {
        let Some(entry) = self.entries.remove(&key) else {
            return false;
        };
        self.bytes = self.bytes.saturating_sub(entry.bytes);
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

fn same_render_frames(
    left: &oxideterm_terminal::TerminalImageData,
    right: &oxideterm_terminal::TerminalImageData,
) -> bool {
    left.width == right.width
        && left.height == right.height
        && Arc::ptr_eq(&left.rgba, &right.rgba)
        && left.frames.len() == right.frames.len()
        && left.frames.iter().zip(&right.frames).all(|(left, right)| {
            Arc::ptr_eq(&left.rgba, &right.rgba)
                && left.delay_ms_numerator == right.delay_ms_numerator
                && left.delay_ms_denominator == right.delay_ms_denominator
        })
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

fn render_image_for_snapshot(
    data: &oxideterm_terminal::TerminalImageData,
    snapshot: &TerminalImageSnapshot,
) -> Option<RenderImage> {
    let dimensions = gpui::size(data.width, data.height);
    let crop = gpui::Bounds::new(
        gpui::point(snapshot.source_x, snapshot.source_y),
        gpui::size(snapshot.source_width, snapshot.source_height),
    );
    if data.frames.is_empty() {
        RenderImage::from_rgba_frames(
            [(data.rgba.clone(), Delay::from_numer_denom_ms(0, 1))],
            dimensions,
            crop,
        )
    } else {
        RenderImage::from_rgba_frames(
            data.frames.iter().map(|frame| {
                (
                    frame.rgba.clone(),
                    Delay::from_numer_denom_ms(
                        frame.delay_ms_numerator,
                        frame.delay_ms_denominator.max(1),
                    ),
                )
            }),
            dimensions,
            crop,
        )
    }
}

#[cfg(test)]
mod tests {
    use oxideterm_terminal::{
        TerminalImageAnimationState, TerminalImageData, TerminalImageFrame, TerminalImageProtocol,
        TerminalImageSnapshot,
    };

    use super::*;

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
                rgba: vec![255, 0, 0, 255, 0, 0, 255, 255].into(),
                frames: Vec::new(),
                animation: TerminalImageAnimationState::default(),
                name: None,
            })),
        };

        let rendered = cache.render_images(&[snapshot], true);
        let image = rendered[0].render_image.as_ref().unwrap();

        assert_eq!(image.as_bytes(0), Some([255, 0, 0, 255].as_slice()));
    }

    #[test]
    fn render_cache_preserves_animation_frames_and_delays() {
        let mut cache = ImageRenderCache::default();
        let mut snapshot = TerminalImageSnapshot {
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

        let rendered = cache.render_images(std::slice::from_ref(&snapshot), true);
        let image = rendered[0].render_image.as_ref().unwrap();

        assert_eq!(image.frame_count(), 2);
        assert_eq!(image.delay(0).numer_denom_ms(), (50, 1));
        assert_eq!(image.delay(1).numer_denom_ms(), (75, 1));
        assert_eq!(image.as_bytes(0), Some([0, 0, 255, 255].as_slice()));
        assert_eq!(image.as_bytes(1), Some([0, 255, 0, 255].as_slice()));
        assert!(rendered[0].animation_started_at.is_some());

        snapshot.version = 2;
        let data = Arc::make_mut(snapshot.data.as_mut().unwrap());
        data.version = 2;
        data.animation.running = false;
        let controlled = cache.render_images(std::slice::from_ref(&snapshot), true);
        assert_eq!(controlled[0].render_image.as_ref().unwrap().id, image.id);
        assert!(
            !controlled[0]
                .snapshot
                .data
                .as_ref()
                .unwrap()
                .animation
                .running
        );
        let clock = controlled[0].animation_started_at;
        let retired = cache.suspend();
        assert_eq!(
            retired.iter().map(|image| image.id).collect::<Vec<_>>(),
            [image.id]
        );
        let restored = cache.render_images(std::slice::from_ref(&snapshot), true);
        assert_eq!(restored[0].animation_started_at, clock);
        assert_eq!(
            restored[0].render_image.as_ref().unwrap().as_bytes(1),
            Some([0, 255, 0, 255].as_slice())
        );

        snapshot.version = 3;
        Arc::make_mut(snapshot.data.as_mut().unwrap()).frames[1].rgba = vec![0, 0, 255, 255].into();
        let requested = cache.take_preparation_requests(std::slice::from_ref(&snapshot), true);
        let prepared = requested
            .iter()
            .cloned()
            .filter_map(ImageRenderCache::prepare_snapshot)
            .collect();
        drop(cache.suspend());
        cache.finish_preparations(&requested, prepared);
        assert!(
            cache.cached_images(std::slice::from_ref(&snapshot), true)[0]
                .render_image
                .is_none()
        );
        let latest = cache.render_images(&[snapshot], true);
        assert_eq!(
            latest[0].render_image.as_ref().unwrap().as_bytes(1),
            Some([255, 0, 0, 255].as_slice())
        );
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
