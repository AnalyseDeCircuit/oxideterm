use super::*;
use crate::{DevicePixels, Size, SvgRenderer, asset_cache::CachedLoad, size};
use std::collections::VecDeque;
use std::rc::Rc;

pub(super) struct SourceInfo {
    source: Resource,
    bytes: Option<Arc<[u8]>>,
    info: Option<super::decoded_size::ImageInfo>,
    layout: Arc<RenderImage>,
    animated: bool,
}

impl SourceInfo {
    pub(super) fn new(source: Resource, bytes: Option<Vec<u8>>) -> Result<Self, ImageCacheError> {
        let (data, declared) = match &source {
            Resource::Image(image) => (image.bytes.as_slice(), Some(image.format)),
            _ => (bytes.as_deref().expect("resource bytes loaded"), None),
        };
        let format = match declared {
            Some(crate::ImageFormat::Png) => Some(image::ImageFormat::Png),
            Some(crate::ImageFormat::Jpeg) => Some(image::ImageFormat::Jpeg),
            Some(crate::ImageFormat::Gif) => Some(image::ImageFormat::Gif),
            Some(crate::ImageFormat::Webp) => Some(image::ImageFormat::WebP),
            _ => None,
        };
        let info = if declared.is_some() && format.is_none() {
            None
        } else {
            super::decoded_size::ImageInfo::read(data, format)?
        };
        let animated = info.is_some_and(|info| info.format == image::ImageFormat::Gif)
            || (info.is_some_and(|info| info.format == image::ImageFormat::WebP)
                && declared.is_none()
                && image::codecs::webp::WebPDecoder::new(Cursor::new(data))?.has_animation());
        let mut result = Self {
            source,
            bytes: bytes.map(Arc::from),
            info,
            animated,
            layout: Arc::new(RenderImage::new(SmallVec::<[image::Frame; 1]>::new())),
        };
        if let Some(info) = info {
            let dimensions = result.rotated_size(info.size);
            result.layout = Arc::new(
                RenderImage::new(SmallVec::<[image::Frame; 1]>::new())
                    .with_intrinsic_size(size(dimensions.0.into(), dimensions.1.into())),
            );
        }
        Ok(result)
    }

    fn quarter_turns(&self) -> u8 {
        match self.source {
            Resource::RotatedImage { quarter_turns, .. } => quarter_turns % 4,
            _ => 0,
        }
    }

    fn rotated_size(&self, size: (u32, u32)) -> (u32, u32) {
        if self.quarter_turns() % 2 == 1 {
            (size.1, size.0)
        } else {
            size
        }
    }

    fn target(&self, bounds: (u32, u32)) -> (u32, u32) {
        self.rotated_size(self.info.unwrap().target(self.rotated_size(bounds)))
    }

    fn bytes(&self) -> &[u8] {
        match &self.source {
            Resource::Image(image) => &image.bytes,
            _ => self.bytes.as_deref().expect("resource bytes loaded"),
        }
    }

    pub(super) fn decode(
        &self,
        target: (u32, u32),
        renderer: SvgRenderer,
    ) -> Result<Arc<RenderImage>, ImageCacheError> {
        let turns = self.quarter_turns();
        if let Some(info) = self.info {
            let target = info.target(self.rotated_size(target));
            let dimensions = self.rotated_size(info.size);
            let intrinsic = size(dimensions.0.into(), dimensions.1.into());
            let input = match &self.source {
                Resource::Image(image) => super::AnimationInput::Image(image.clone()),
                _ => super::AnimationInput::Bytes(self.bytes.as_ref().unwrap().clone()),
            };
            if let Some(frames) = super::decoded_size::animation(
                info,
                input,
                target,
                turns,
                !matches!(self.source, Resource::Image(_)),
            )? {
                return Ok(Arc::new(frames.with_intrinsic_size(intrinsic)));
            }
            if let Some(pixels) = info.decode(self.bytes(), target)? {
                let mut pixels = super::decoded_size::rotate(pixels, turns);
                for pixel in pixels.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }
                return Ok(Arc::new(
                    RenderImage::from_decoded_frames([image::Frame::new(pixels)])?
                        .with_intrinsic_size(intrinsic),
                ));
            }
        }
        if turns != 0
            && let Ok(format) = image::guess_format(self.bytes())
        {
            // At native zoom (or for unsupported thumbnails), rotate decoded frames
            // before compact storage, without an intermediate full RenderImage copy.
            let frames = decode_static_image(self.bytes(), format)?
                .into_iter()
                .map(|frame| {
                    let delay = frame.delay();
                    image::Frame::from_parts(
                        super::decoded_size::rotate(frame.into_buffer(), turns),
                        0,
                        0,
                        delay,
                    )
                });
            return Ok(Arc::new(RenderImage::from_decoded_frames(frames)?));
        }
        match &self.source {
            Resource::Image(image) => image.to_image_data(renderer).map_err(Into::into),
            _ => ImageAssetLoader::decode_bytes(self.bytes(), renderer),
        }
    }
}

struct Ready {
    target: (u32, u32),
    image: Arc<RenderImage>,
}

/// The cache entry owns encoded input, one worker and at most two display resolutions.
pub(crate) struct SizedImage {
    source: Rc<CachedLoad<Result<Arc<SourceInfo>, ImageCacheError>>>,
    ready: VecDeque<Ready>,
    pending: Option<(
        (u32, u32),
        CachedLoad<Result<Arc<RenderImage>, ImageCacheError>>,
    )>,
    failed: Option<((u32, u32), ImageCacheError)>,
    pending_frame: Option<CachedLoad<Result<(), ImageCacheError>>>,
}

impl SizedImage {
    pub(crate) fn is_animation(&self, window: &Window) -> Option<bool> {
        self.source
            .use_by(window.current_view())
            .map(|result| result.is_ok_and(|source| source.animated))
    }

    pub(crate) fn fork_player(&self) -> Self {
        Self {
            source: self.source.clone(),
            ready: VecDeque::new(),
            pending: None,
            failed: None,
            pending_frame: None,
        }
    }

    pub(crate) fn image_bytes(&self) -> usize {
        self.ready
            .iter()
            .map(|ready| ready.image.retained_bytes())
            .sum()
    }
    pub(crate) fn contains_image(
        &self,
        painted: &std::collections::HashSet<crate::ImageId>,
    ) -> bool {
        self.ready
            .iter()
            .any(|ready| painted.contains(&ready.image.id))
    }
    pub(crate) fn new(source: Resource, cx: &App) -> Self {
        let read = ImageAssetLoader::read_bytes(source.clone(), cx);
        let load = async move {
            let bytes = match &source {
                Resource::Image(_) => None,
                _ => Some(read.await?),
            };
            SourceInfo::new(source, bytes).map(Arc::new)
        };
        Self {
            source: Rc::new(CachedLoad::new(load, cx)),
            ready: VecDeque::new(),
            pending: None,
            failed: None,
            pending_frame: None,
        }
    }

    pub(crate) fn get(&self) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        self.ready.back().map(|ready| Ok(ready.image.clone()))
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        let input = self
            .source
            .get()
            .and_then(Result::ok)
            .map_or(0, |source| source.bytes().len());
        input + self.image_bytes()
    }

    pub(crate) fn use_image(
        &mut self,
        bounds: Option<Size<DevicePixels>>,
        frame: usize,
        window: &Window,
        cx: &App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let source = match self.source.use_by(window.current_view())? {
            Ok(source) => source,
            Err(error) => return Some(Err(error)),
        };
        if let Some(pending) = &self.pending_frame
            && let Some(result) = pending.use_by(window.current_view())
        {
            self.pending_frame = None;
            if let Err(error) = result {
                return Some(Err(error));
            }
        }
        if let Some((target, pending)) = &self.pending
            && let Some(result) = pending.use_by(window.current_view())
        {
            let target = *target;
            self.pending = None;
            match result {
                Ok(image) => {
                    self.ready.push_back(Ready { target, image });
                    if self.ready.len() > 2 {
                        self.ready.pop_front();
                    }
                    self.failed = None;
                }
                Err(error) => self.failed = Some((target, error)),
            }
        }
        let target = if source.info.is_some() {
            let Some(bounds) = bounds else {
                if let Some((_, error)) = &self.failed {
                    return Some(Err(error.clone()));
                }
                let image = self
                    .ready
                    .back()
                    .map_or_else(|| source.layout.clone(), |ready| ready.image.clone());
                return self.display_frame(image, frame, window, cx);
            };
            // Bucket resize steps upward, never below the actual device-pixel request.
            source.target((
                (bounds.width.0.max(1) as u32)
                    .div_ceil(64)
                    .saturating_mul(64),
                (bounds.height.0.max(1) as u32)
                    .div_ceil(64)
                    .saturating_mul(64),
            ))
        } else {
            // Other formats keep the established full-resolution decoder.
            (u32::MAX, u32::MAX)
        };
        if let Some((failed, error)) = &self.failed
            && *failed == target
        {
            return Some(Err(error.clone()));
        }
        let adequate = self
            .ready
            .iter()
            .filter(|ready| ready.target.0 >= target.0 && ready.target.1 >= target.1)
            .min_by_key(|ready| ready.target.0 as u64 * ready.target.1 as u64);
        if let Some(ready) = adequate
            && ready.target.0 <= target.0.saturating_mul(2)
            && ready.target.1 <= target.1.saturating_mul(2)
        {
            return self.display_frame(ready.image.clone(), frame, window, cx);
        }
        let previous = adequate
            .or_else(|| self.ready.back())
            .map(|ready| ready.image.clone());
        if self.pending.is_none() {
            let renderer = cx.svg_renderer();
            let pending = CachedLoad::new(async move { source.decode(target, renderer) }, cx);
            pending.use_by(window.current_view());
            self.pending = Some((target, pending));
        }
        previous.and_then(|image| self.display_frame(image, frame, window, cx))
    }

    fn display_frame(
        &mut self,
        image: Arc<RenderImage>,
        frame: usize,
        window: &Window,
        cx: &App,
    ) -> Option<Result<Arc<RenderImage>, ImageCacheError>> {
        let frame = frame.min(image.frame_count().saturating_sub(1));
        if image.frame_ready(frame) {
            image.prefetch_frame(frame + 1);
            if image.frame_count() > 1
                && self
                    .ready
                    .back()
                    .is_some_and(|ready| ready.image.id == image.id)
            {
                self.ready.retain(|ready| ready.image.id == image.id);
            }
            return Some(Ok(image));
        }
        if self.pending_frame.is_none() {
            let pending = CachedLoad::new(async move { image.prepare_frame(frame).await }, cx);
            pending.use_by(window.current_view());
            self.pending_frame = Some(pending);
        }
        self.ready
            .iter()
            .rev()
            .find(|ready| ready.image.frame_ready(frame))
            .map(|ready| Ok(ready.image.clone()))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    fn measure(label: &str, decode: impl FnOnce() -> Arc<RenderImage>) -> Arc<RenderImage> {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        #[repr(C)]
        #[derive(Default)]
        struct Stats {
            blocks: u32,
            live: usize,
            peak: usize,
            allocated: usize,
        }
        unsafe extern "C" {
            fn malloc_zone_statistics(zone: *mut std::ffi::c_void, stats: *mut Stats);
        }
        fn live() -> usize {
            let mut stats = Stats::default();
            unsafe {
                malloc_zone_statistics(std::ptr::null_mut(), &mut stats);
            }
            stats.live
        }
        let baseline = live();
        let peak = Arc::new(AtomicUsize::new(baseline));
        let stop = Arc::new(AtomicBool::new(false));
        let sampler = std::thread::spawn({
            let peak = peak.clone();
            let stop = stop.clone();
            move || {
                while !stop.load(Ordering::Acquire) {
                    peak.fetch_max(live(), Ordering::Relaxed);
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
            }
        });
        let start = Instant::now();
        let output = decode();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        peak.fetch_max(live(), Ordering::Relaxed);
        stop.store(true, Ordering::Release);
        sampler.join().unwrap();
        eprintln!(
            "{label} retained={} peak_delta={} ms={ms:.3}",
            output.retained_bytes(),
            peak.load(Ordering::Relaxed).saturating_sub(baseline)
        );
        std::hint::black_box(output)
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "manual production decode/upload-storage peak and latency comparison"]
    fn sized_decode_memory() {
        for format in [image::ImageFormat::Png, image::ImageFormat::Jpeg] {
            let pixels = image::RgbImage::from_fn(6000, 4000, |x, y| {
                image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
            });
            let mut bytes = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(pixels)
                .write_to(&mut bytes, format)
                .unwrap();
            let bytes = bytes.into_inner();
            let info = super::super::decoded_size::ImageInfo::read(&bytes, None)
                .unwrap()
                .unwrap();
            let source = SourceInfo {
                animated: false,
                source: Resource::Image(Arc::new(Image::from_bytes(
                    if format == image::ImageFormat::Png {
                        crate::ImageFormat::Png
                    } else {
                        crate::ImageFormat::Jpeg
                    },
                    bytes,
                ))),
                bytes: None,
                info: Some(info),
                layout: Arc::new(RenderImage::new(SmallVec::<[image::Frame; 1]>::new())),
            };
            for sized in [false, true] {
                measure(
                    &format!("SIZED_DECODE format={format:?} sized={sized}"),
                    || {
                        source
                            .decode(
                                if sized { (960, 640) } else { info.size },
                                SvgRenderer::new(Arc::new(())),
                            )
                            .unwrap()
                    },
                );
            }
        }
    }

    pub(crate) fn animation_bytes(width: u16, height: u16, count: usize) -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let palette: Vec<u8> = (0..=255u8)
                .flat_map(|i| [i, i.wrapping_mul(3), i.wrapping_mul(7)])
                .collect();
            let mut encoder = gif::Encoder::new(&mut bytes, width, height, &palette).unwrap();
            for frame in 0..count {
                let buffer = (0..width as usize * height as usize)
                    .map(|i| {
                        let x = i % width as usize;
                        let y = i / width as usize;
                        ((x / 3 + y / 3 + frame * 7) % 256) as u8
                    })
                    .collect();
                encoder
                    .write_frame(&gif::Frame {
                        width,
                        height,
                        delay: 4,
                        buffer: std::borrow::Cow::Owned(buffer),
                        ..Default::default()
                    })
                    .unwrap();
            }
        }
        bytes
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "manual animated decode peak and latency comparison"]
    fn animated_decode_memory() {
        let bytes = animation_bytes(1920, 1080, 24);
        let source = SourceInfo {
            animated: true,
            info: super::super::decoded_size::ImageInfo::read(&bytes, None).unwrap(),
            source: Resource::Image(Arc::new(Image::from_bytes(crate::ImageFormat::Gif, bytes))),
            bytes: None,
            layout: Arc::new(RenderImage::new(SmallVec::<[image::Frame; 1]>::new())),
        };
        for target in [(1920, 1080), (640, 360)] {
            measure(&format!("ANIMATED_DECODE target={target:?}"), || {
                source
                    .decode(target, SvgRenderer::new(Arc::new(())))
                    .unwrap()
            });
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "manual long-animation memory and frame-preparation comparison"]
    fn long_animation_memory() {
        let input = Arc::new(Image::from_bytes(
            crate::ImageFormat::Gif,
            animation_bytes(640, 360, 240),
        ));
        let source = SourceInfo::new(Resource::Image(input.clone()), None).unwrap();
        for adaptive in [false, true] {
            let output = measure(&format!("LONG_ANIMATION adaptive={adaptive}"), || {
                if adaptive {
                    source
                        .decode((640, 360), SvgRenderer::new(Arc::new(())))
                        .unwrap()
                } else {
                    input.to_image_data(SvgRenderer::new(Arc::new(()))).unwrap()
                }
            });
            assert_eq!(output.frame_count(), 240);
            let mut times = Vec::new();
            for index in 0..240 {
                let start = Instant::now();
                let pixels = output.frame_bytes(index).unwrap().unwrap();
                let color = (index * 7 % 256) as u8;
                assert_eq!(
                    &pixels[..4],
                    &[color.wrapping_mul(7), color.wrapping_mul(3), color, 255]
                );
                times.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            times.sort_by(f64::total_cmp);
            eprintln!(
                "LONG_PLAYBACK adaptive={adaptive} retained={} frame_p95_ms={:.3} frame_max_ms={:.3}",
                output.retained_bytes(),
                times[227],
                times[239]
            );
            futures::executor::block_on(output.prepare_frame(0)).unwrap();
            let start = Instant::now();
            let mut late = 0;
            let mut reads = Vec::new();
            for index in 0..240 {
                let deadline = start + Duration::from_millis(index as u64 * 40);
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
                late += usize::from(!output.frame_ready(index));
                let read = Instant::now();
                let pixels = output.frame_bytes(index).unwrap().unwrap();
                let color = (index * 7 % 256) as u8;
                assert_eq!(
                    &pixels[..4],
                    &[color.wrapping_mul(7), color.wrapping_mul(3), color, 255]
                );
                output.prefetch_frame(index + 1);
                reads.push(read.elapsed().as_secs_f64() * 1000.0);
            }
            reads.sort_by(f64::total_cmp);
            eprintln!(
                "PACED_PLAYBACK adaptive={adaptive} late={late} read_p95_ms={:.3} read_max_ms={:.3}",
                reads[227], reads[239]
            );
            if adaptive {
                assert!(output.retained_bytes() <= 640 * 360 * 4 * 3);
            }
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    #[ignore = "manual file-manager rotated preview peak and latency comparison"]
    fn rotated_decode_memory() {
        let pixels = image::RgbImage::from_fn(6000, 4000, |x, y| {
            image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
        });
        let mut bytes = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(pixels)
            .write_to(&mut bytes, image::ImageFormat::Png)
            .unwrap();
        let bytes = bytes.into_inner();
        measure("ROTATED_DECODE original", || {
            // The former file-manager helper rotated a full decoded image, then copied RGBA.
            let image = image::load_from_memory(&bytes).unwrap();
            let image = image.rotate90();
            let mut rgba = image.to_rgba8();
            for pixel in rgba.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
            Arc::new(RenderImage::new(vec![image::Frame::new(rgba)]))
        });
        let source = SourceInfo::new(
            Resource::RotatedImage {
                path: std::path::Path::new("preview.png").into(),
                quarter_turns: 1,
            },
            Some(bytes),
        )
        .unwrap();
        measure("ROTATED_DECODE sized", || {
            source
                .decode((640, 960), SvgRenderer::new(Arc::new(())))
                .unwrap()
        });
    }

    #[test]
    fn gif_resize_preserves_partial_frames_disposal_transparency_and_delays() {
        let mut bytes = Vec::new();
        {
            let mut encoder = gif::Encoder::new(
                &mut bytes,
                8,
                4,
                &[0, 0, 0, 255, 0, 0, 0, 0, 255, 0, 255, 0],
            )
            .unwrap();
            for (left, width, color, dispose, delay) in [
                (0, 8, 1, gif::DisposalMethod::Keep, 7),
                (0, 4, 2, gif::DisposalMethod::Previous, 13),
                (4, 4, 3, gif::DisposalMethod::Background, 19),
                (0, 8, 0, gif::DisposalMethod::Keep, 0),
            ] {
                encoder
                    .write_frame(&gif::Frame {
                        left,
                        width,
                        height: 4,
                        delay,
                        dispose,
                        transparent: Some(0),
                        buffer: std::borrow::Cow::Owned(vec![color; width as usize * 4]),
                        ..Default::default()
                    })
                    .unwrap();
            }
        }
        let source = SourceInfo::new(
            Resource::Image(Arc::new(Image::from_bytes(crate::ImageFormat::Gif, bytes))),
            None,
        )
        .unwrap();
        for target in [(4, 2), (8, 4)] {
            let regular = source
                .decode(target, SvgRenderer::new(Arc::new(())))
                .unwrap();
            let Resource::Image(input) = &source.source else {
                unreachable!()
            };
            let streamed = Arc::new(
                super::super::AnimationStream::spawn(
                    super::super::AnimationInput::Image(input.clone()),
                    image::ImageFormat::Gif,
                    (8, 4),
                    target,
                    0,
                    [70, 130, 190, 0]
                        .into_iter()
                        .map(|ms| image::Delay::from_numer_denom_ms(ms, 1))
                        .collect(),
                )
                .unwrap(),
            );
            for frames in [regular, streamed] {
                assert_eq!(frames.frame_count(), 4);
                if frames.estimated_decode_working_bytes() != 0 {
                    let upload = frames.frame_bytes(0).unwrap().unwrap();
                    let same_frame = frames.frame_bytes(0).unwrap().unwrap();
                    assert_eq!(
                        upload.as_ptr(),
                        same_frame.as_ptr(),
                        "upload copied a cached frame"
                    );
                    futures::executor::block_on(frames.prepare_frame(3)).unwrap();
                    assert!(
                        upload
                            .chunks_exact(4)
                            .all(|pixel| pixel == [0, 0, 255, 255]),
                        "eviction changed pixels still held for upload"
                    );
                }
                let expected = [
                    ([0, 0, 255, 255], [0, 0, 255, 255], 70),
                    ([255, 0, 0, 255], [0, 0, 255, 255], 130),
                    ([0, 0, 255, 255], [0, 255, 0, 255], 190),
                    ([0, 0, 255, 255], [0, 0, 0, 0], 0),
                ];
                for index in [0, 1, 2, 3, 0, 3, 2, 1] {
                    let (left, right, delay) = expected[index];
                    assert_eq!(frames.size(index), size(target.0.into(), target.1.into()));
                    assert_eq!(
                        Duration::from(frames.delay(index)),
                        Duration::from_millis(delay)
                    );
                    let pixels = frames.frame_bytes(index).unwrap().unwrap();
                    assert_eq!(&pixels[..4], &left, "frame {index} left");
                    assert_eq!(
                        &pixels[(target.0 as usize - 1) * 4..target.0 as usize * 4],
                        &right,
                        "frame {index} right"
                    );
                    if index == 3 && target == (4, 2) {
                        // The bilinear edge has 7/8 red coverage, not darkened RGB.
                        assert_eq!(&pixels[4..8], &[0, 0, 255, 223]);
                    }
                }
            }
        }
    }

    #[test]
    fn rotated_preview_retains_orientation_and_native_zoom_detail() {
        use image::ImageEncoder;
        let pixels = image::RgbImage::from_fn(600, 300, |x, _| {
            image::Rgb([if x < 300 { 255 } else { 0 }; 3])
        });
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut bytes);
        encoder
            .set_exif_metadata(vec![
                73, 73, 42, 0, 8, 0, 0, 0, 1, 0, 18, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
            ])
            .unwrap();
        encoder
            .encode(&pixels, 600, 300, image::ExtendedColorType::Rgb8)
            .unwrap();
        for (turns, dimensions, white_corner) in
            [(1, (600, 300), 1), (2, (300, 600), 2), (3, (600, 300), 0)]
        {
            let source = SourceInfo::new(
                Resource::RotatedImage {
                    path: Path::new("preview.jpg").into(),
                    quarter_turns: turns,
                },
                Some(bytes.clone()),
            )
            .unwrap();
            for divisor in [4, 1] {
                let target = (dimensions.0 / divisor, dimensions.1 / divisor);
                let frame = source
                    .decode(target, SvgRenderer::new(Arc::new(())))
                    .unwrap();
                assert_eq!(frame.size(0), size(target.0.into(), target.1.into()));
                assert_eq!(
                    frame.render_size(0),
                    size(px(dimensions.0 as f32), px(dimensions.1 as f32))
                );
                let data = frame.frame_bytes(0).unwrap().unwrap();
                let offsets = [
                    0,
                    (target.0 as usize - 1) * 4,
                    (target.1 as usize - 1) * target.0 as usize * 4,
                ];
                assert!(
                    data[offsets[white_corner]..offsets[white_corner] + 3]
                        .iter()
                        .all(|&c| c >= 245)
                );
                let dark = if white_corner == 0 { 1 } else { 0 };
                assert!(
                    data[offsets[dark]..offsets[dark] + 3]
                        .iter()
                        .all(|&c| c <= 10)
                );
            }
        }
    }

    #[test]
    fn webp_resource_animation_and_declared_static_image_keep_their_frame_contracts() {
        // Assemble an animated RIFF from independently encoded lossless frames.
        fn chunk(output: &mut Vec<u8>, kind: &[u8; 4], bytes: &[u8]) {
            output.extend_from_slice(kind);
            output.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
            output.extend_from_slice(bytes);
            if bytes.len() % 2 == 1 {
                output.push(0);
            }
        }
        let mut riff = b"WEBP".to_vec();
        chunk(&mut riff, b"VP8X", &[2, 0, 0, 0, 7, 0, 0, 3, 0, 0]);
        chunk(&mut riff, b"ANIM", &[0, 0, 0, 0, 0, 0]);
        for (color, delay) in [([255, 0, 0, 255], 70u8), ([0, 0, 255, 255], 130)] {
            let mut encoded = Cursor::new(Vec::new());
            image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(8, 4, image::Rgba(color)))
                .write_to(&mut encoded, image::ImageFormat::WebP)
                .unwrap();
            let mut frame = vec![0, 0, 0, 0, 0, 0, 7, 0, 0, 3, 0, 0, delay, 0, 0, 2];
            frame.extend_from_slice(&encoded.into_inner()[12..]);
            chunk(&mut riff, b"ANMF", &frame);
        }
        let mut bytes = b"RIFF".to_vec();
        bytes.extend_from_slice(&(riff.len() as u32).to_le_bytes());
        bytes.extend(riff);
        let resource = SourceInfo::new(
            Resource::Path(Path::new("animation.webp").into()),
            Some(bytes.clone()),
        )
        .unwrap();
        let declared = SourceInfo::new(
            Resource::Image(Arc::new(Image::from_bytes(crate::ImageFormat::Webp, bytes))),
            None,
        )
        .unwrap();
        for target in [(4, 2), (8, 4)] {
            let frames = resource
                .decode(target, SvgRenderer::new(Arc::new(())))
                .unwrap();
            assert_eq!(frames.frame_count(), 2);
            for (index, color, delay) in [(0, [0, 0, 255, 255], 70), (1, [255, 0, 0, 255], 130)] {
                assert_eq!(frames.size(index), size(target.0.into(), target.1.into()));
                assert_eq!(
                    frames.frame_bytes(index).unwrap().unwrap().as_ref(),
                    color.repeat((target.0 * target.1) as usize)
                );
                assert_eq!(
                    Duration::from(frames.delay(index)),
                    Duration::from_millis(delay)
                );
            }
            let frame = declared
                .decode(target, SvgRenderer::new(Arc::new(())))
                .unwrap();
            assert_eq!(frame.frame_count(), 1);
            assert_eq!(
                frame.frame_bytes(0).unwrap().unwrap().as_ref(),
                [0, 0, 255, 255].repeat((target.0 * target.1) as usize)
            );
        }
    }
}
