use crate::{
    LoopCount, MediaError, MediaInfo, MemoryBudget, PixelLease, gif_metadata, pixels::pixel_bytes,
};
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat};
use std::{
    fs::File,
    io::BufReader,
    path::{Path, PathBuf},
    time::Duration,
};

pub(crate) struct Decoder {
    pub info: MediaInfo,
    path: PathBuf,
    source: Source,
    _lease: PixelLease,
    software_video: bool,
    native_device: Option<crate::NativeVideoDevice>,
}

enum Source {
    Gif(image::Frames<'static>),
    WebP {
        decoder: image_webp::WebPDecoder<BufReader<File>>,
        remaining: u32,
    },
    Still(Option<DynamicImage>),
    Video(crate::video::VideoDecoder),
}

pub(crate) enum DecodedImage {
    Pixels(DynamicImage),
    Native(crate::NativeVideoFrame),
}

impl Decoder {
    pub fn next_for_stream(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
        native_video: bool,
    ) -> Result<Option<(DecodedImage, Duration, Option<Duration>)>, MediaError> {
        if native_video && let Source::Video(video) = &mut self.source {
            return Ok(video.next_native(cancelled)?.map(|frame| {
                let duration = frame.duration;
                let timestamp = frame.timestamp;
                (DecodedImage::Native(frame), duration, Some(timestamp))
            }));
        }
        Ok(self
            .next(cancelled)?
            .map(|(image, duration, timestamp)| (DecodedImage::Pixels(image), duration, timestamp)))
    }

    pub fn is_bgra(&self) -> bool {
        matches!(self.source, Source::Video(_))
    }

    pub fn open(
        path: &Path,
        budget: &MemoryBudget,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Self, MediaError> {
        Self::open_with_video_mode(path, budget, cancelled, false, None)
    }

    pub fn open_with_video_mode(
        path: &Path,
        budget: &MemoryBudget,
        cancelled: &std::sync::atomic::AtomicBool,
        software_video: bool,
        native_device: Option<crate::NativeVideoDevice>,
    ) -> Result<Self, MediaError> {
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(MediaError::Cancelled);
        }
        if !path.metadata()?.is_file() {
            return Err(MediaError::Unsupported);
        }
        let reader = image::ImageReader::open(path)?.with_guessed_format()?;
        let Some(format) = reader.format() else {
            let video = crate::video::VideoDecoder::open(
                path,
                budget,
                cancelled,
                software_video,
                native_device.as_ref(),
            )?;
            return Ok(Self {
                info: video.info.clone(),
                path: path.into(),
                source: Source::Video(video),
                _lease: budget.reserve(0)?,
                software_video,
                native_device,
            });
        };
        let (source, info, lease) = match format {
            ImageFormat::Gif => {
                let info = gif_metadata::inspect(&mut BufReader::new(File::open(path)?))?;
                let bytes = pixel_bytes(info.width, info.height)?;
                let lease = budget.reserve(
                    bytes
                        .checked_mul(4)
                        .and_then(|n| n.checked_add(65536))
                        .ok_or(MediaError::ResourceExhausted)?,
                )?;
                let mut decoder =
                    image::codecs::gif::GifDecoder::new(BufReader::new(File::open(path)?))?;
                let mut limits = image::Limits::default();
                limits.max_alloc = Some((bytes * 4) as u64);
                decoder.set_limits(limits)?;
                (Source::Gif(decoder.into_frames()), info, lease)
            }
            ImageFormat::WebP => {
                let mut decoder = image_webp::WebPDecoder::new(BufReader::new(File::open(path)?))?;
                let (width, height) = decoder.dimensions();
                let bytes = pixel_bytes(width, height)?;
                // Includes the composite canvas, patch, output and codec working storage.
                let work_bytes = bytes.checked_mul(12).ok_or(MediaError::ResourceExhausted)?;
                let lease = budget.reserve(work_bytes)?;
                decoder.set_memory_limit(work_bytes);
                if decoder.is_animated() {
                    decoder.set_background_color([0, 0, 0, 0])?;
                }
                let loops = match decoder.loop_count() {
                    image_webp::LoopCount::Forever => LoopCount::Infinite,
                    image_webp::LoopCount::Times(n) => LoopCount::Finite(n.get() as u32),
                };
                let info = MediaInfo {
                    width,
                    height,
                    animated: decoder.num_frames() > 1,
                    frame_count: Some(decoder.num_frames().max(1)),
                    loops,
                };
                let remaining = decoder.num_frames().max(1);
                (Source::WebP { decoder, remaining }, info, lease)
            }
            _ => {
                let (width, height) = image::ImageReader::open(path)?
                    .with_guessed_format()?
                    .into_dimensions()?;
                let bytes = pixel_bytes(width, height)?;
                let lease =
                    budget.reserve(bytes.checked_mul(4).ok_or(MediaError::ResourceExhausted)?)?;
                let mut reader = reader;
                let mut limits = image::Limits::default();
                limits.max_alloc = Some((bytes * 4) as u64);
                reader.limits(limits);
                let info = MediaInfo {
                    width,
                    height,
                    animated: false,
                    frame_count: Some(1),
                    loops: LoopCount::Finite(1),
                };
                (Source::Still(Some(reader.decode()?)), info, lease)
            }
        };
        Ok(Self {
            info,
            path: path.into(),
            source,
            _lease: lease,
            software_video,
            native_device,
        })
    }

    pub fn next(
        &mut self,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<Option<(DynamicImage, Duration, Option<Duration>)>, MediaError> {
        if let Source::Video(video) = &mut self.source {
            return Ok(video
                .next(cancelled)?
                .map(|frame| (frame.image, frame.duration, Some(frame.timestamp))));
        }
        let frame = match &mut self.source {
            Source::Gif(frames) => match frames.next().transpose()? {
                Some(frame) => {
                    let (numerator, denominator) = frame.delay().numer_denom_ms();
                    Some((
                        DynamicImage::ImageRgba8(frame.into_buffer()),
                        Duration::from_secs_f64(numerator as f64 / denominator as f64 / 1000.0),
                    ))
                }
                None => None,
            },
            Source::WebP { decoder, remaining } => {
                if *remaining == 0 {
                    return Ok(None);
                }
                let bytes = decoder
                    .output_buffer_size()
                    .ok_or(MediaError::ResourceExhausted)?;
                let mut pixels = vec![0; bytes];
                let duration = if decoder.is_animated() {
                    match decoder.read_frame(&mut pixels) {
                        Ok(millis) => Duration::from_millis(millis as u64),
                        Err(image_webp::DecodingError::NoMoreFrames) => return Ok(None),
                        Err(error) => return Err(error.into()),
                    }
                } else {
                    decoder.read_image(&mut pixels)?;
                    Duration::ZERO
                };
                let (width, height) = decoder.dimensions();
                let image = if decoder.has_alpha() {
                    DynamicImage::ImageRgba8(
                        image::RgbaImage::from_raw(width, height, pixels)
                            .ok_or_else(|| MediaError::Decode("invalid WebP pixels".into()))?,
                    )
                } else {
                    DynamicImage::ImageRgb8(
                        image::RgbImage::from_raw(width, height, pixels)
                            .ok_or_else(|| MediaError::Decode("invalid WebP pixels".into()))?,
                    )
                };
                *remaining -= 1;
                Some((image, duration))
            }
            Source::Still(image) => image.take().map(|image| (image, Duration::ZERO)),
            Source::Video(_) => unreachable!("video handled before image decoding"),
        };
        Ok(frame.map(|(image, duration)| {
            (
                image,
                if duration.is_zero() && self.info.animated {
                    Duration::from_millis(100)
                } else {
                    duration
                },
                None,
            )
        }))
    }

    pub fn restart(
        &mut self,
        budget: &MemoryBudget,
        cancelled: &std::sync::atomic::AtomicBool,
    ) -> Result<(), MediaError> {
        // Release the old codec before admitting a replacement, without overlapping canvases.
        self.source = Source::Still(None);
        let empty = budget.reserve(0)?;
        self._lease = empty;
        *self = Self::open_with_video_mode(
            &self.path,
            budget,
            cancelled,
            self.software_video,
            self.native_device.clone(),
        )?;
        Ok(())
    }
}
