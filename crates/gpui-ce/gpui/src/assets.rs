// OxideTerm modification: adds mutable texture handles for remote desktop framebuffers.

use crate::{DevicePixels, Pixels, Result, SharedString, Size, size};
use smallvec::SmallVec;
use std::sync::Arc;

use image::{Delay, Frame, ImageEncoder};
use std::{
    borrow::Cow,
    fmt,
    hash::Hash,
    sync::atomic::{AtomicUsize, Ordering::SeqCst},
};

/// A source of assets for this app to use.
pub trait AssetSource: 'static + Send + Sync {
    /// Load the given asset from the source path.
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>>;

    /// List the assets at the given path.
    fn list(&self, path: &str) -> Result<Vec<SharedString>>;
}

impl AssetSource for () {
    fn load(&self, _path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(None)
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(vec![])
    }
}

/// A unique identifier for the image cache
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ImageId(pub usize);

/// A unique identifier for a mutable dynamic texture.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct DynamicTextureId(pub usize);

#[derive(PartialEq, Eq, Hash, Clone)]
#[expect(missing_docs)]
pub struct RenderImageParams {
    pub image_id: ImageId,
    pub frame_index: usize,
}

/// The atlas lookup parameters for an OxideTerm dynamic texture.
#[derive(PartialEq, Eq, Hash, Clone)]
#[doc(hidden)]
pub struct DynamicTextureParams {
    /// The stable identity of the dynamic texture.
    pub texture_id: DynamicTextureId,
}

/// A mutable BGRA texture with a stable identity and fixed device-pixel size.
pub struct DynamicTexture {
    /// The stable ID associated with this texture.
    pub id: DynamicTextureId,
    size: Size<DevicePixels>,
}

impl PartialEq for DynamicTexture {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for DynamicTexture {}

impl DynamicTexture {
    /// Creates a dynamic texture with the given device-pixel size.
    pub fn new(size: Size<DevicePixels>) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

        Self {
            id: DynamicTextureId(NEXT_ID.fetch_add(1, SeqCst)),
            size,
        }
    }

    /// Returns the fixed size of this texture in device pixels.
    pub fn size(&self) -> Size<DevicePixels> {
        self.size
    }
}

impl fmt::Debug for DynamicTexture {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DynamicTexture")
            .field("id", &self.id)
            .field("size", &self.size)
            .finish()
    }
}

/// A cached and processed image, in BGRA format
pub struct RenderImage {
    /// The ID associated with this image
    pub id: ImageId,
    /// The scale factor of this image on render.
    pub(crate) scale_factor: f32,
    intrinsic_size: Option<Size<DevicePixels>>,
    data: SmallVec<[ImageFrameData; 1]>,
    retained: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

enum ImageFrameData {
    Rgba {
        pixels: Arc<[u8]>,
        width: u32,
        crop: crate::Bounds<u32>,
        delay: Delay,
        decoded: std::sync::OnceLock<Vec<u8>>,
    },
    Pixels(Frame),
    Compact {
        bytes: Vec<u8>,
        size: Size<DevicePixels>,
        delay: Delay,
        // Explicit borrowed reads retain their result; painting uses a temporary decode.
        decoded: std::sync::OnceLock<Vec<u8>>,
    },
}

/// Keeps pixels alive through upload, borrowing fully decoded animation frames.
pub(crate) enum ImageFrameBytes<'a> {
    Borrowed(&'a [u8]),
    Owned(Vec<u8>),
}

impl std::ops::Deref for ImageFrameBytes<'_> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        match self {
            Self::Borrowed(bytes) => bytes,
            Self::Owned(bytes) => bytes,
        }
    }
}

impl AsRef<[u8]> for ImageFrameBytes<'_> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl PartialEq for RenderImage {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for RenderImage {}

impl RenderImage {
    /// Shares static protocol RGBA canvases and prepares complete BGRA animation frames.
    pub fn from_rgba_frames(
        frames: impl IntoIterator<Item = (Arc<[u8]>, Delay)>,
        dimensions: Size<u32>,
        crop: crate::Bounds<u32>,
    ) -> Option<Self> {
        if crop.size.width == 0
            || crop.size.height == 0
            || crop.origin.x.checked_add(crop.size.width)? > dimensions.width
            || crop.origin.y.checked_add(crop.size.height)? > dimensions.height
        {
            return None;
        }
        let bytes = (dimensions.width as usize)
            .checked_mul(dimensions.height as usize)?
            .checked_mul(4)?;
        let mut image = Self::new(SmallVec::<[Frame; 1]>::new());
        for (pixels, delay) in frames {
            if pixels.len() != bytes {
                return None;
            }
            image.data.push(ImageFrameData::Rgba {
                pixels,
                width: dimensions.width,
                crop,
                delay,
                decoded: Default::default(),
            });
        }
        if image.frame_count() > 1 {
            image.data = image
                .data
                .into_iter()
                .map(|frame| {
                    let ImageFrameData::Rgba {
                        pixels,
                        width,
                        crop,
                        delay,
                        ..
                    } = frame
                    else {
                        unreachable!();
                    };
                    let pixels = rgba_frame_pixels(&pixels, width, crop);
                    let buffer =
                        image::RgbaImage::from_raw(crop.size.width, crop.size.height, pixels)
                            .expect("validated RGBA crop dimensions");
                    ImageFrameData::Pixels(Frame::from_parts(buffer, 0, 0, delay))
                })
                .collect();
        }
        Some(image)
    }

    /// Create a new image from the given data.
    pub fn new(data: impl Into<SmallVec<[Frame; 1]>>) -> Self {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

        Self {
            id: ImageId(NEXT_ID.fetch_add(1, SeqCst)),
            scale_factor: 1.0,
            intrinsic_size: None,
            data: data
                .into()
                .into_iter()
                .map(ImageFrameData::Pixels)
                .collect(),
            retained: Vec::new(),
        }
    }

    pub(crate) fn push_decoded_frame(&mut self, frame: Frame) -> Result<()> {
        let pixels = frame.buffer();
        if pixels.len() < 64 * 1024 {
            self.data.push(ImageFrameData::Pixels(frame));
            return Ok(());
        }
        let mut encoded = Vec::new();
        image::codecs::qoi::QoiEncoder::new(&mut encoded).write_image(
            pixels.as_raw(),
            pixels.width(),
            pixels.height(),
            image::ExtendedColorType::Rgba8,
        )?;
        if encoded.len() < pixels.len() {
            encoded.shrink_to_fit();
            self.data.push(ImageFrameData::Compact {
                bytes: encoded,
                size: size(pixels.width().into(), pixels.height().into()),
                delay: frame.delay(),
                decoded: Default::default(),
            });
        } else {
            self.data.push(ImageFrameData::Pixels(frame));
        }
        Ok(())
    }

    pub(crate) fn push_animation_frame(&mut self, frame: Frame) {
        // Animation loops reuse decoded pixels rather than decompressing every cycle.
        self.data.push(ImageFrameData::Pixels(frame));
    }

    pub(crate) fn from_decoded_frames(frames: impl IntoIterator<Item = Frame>) -> Result<Self> {
        let mut image = Self::new(SmallVec::<[Frame; 1]>::new());
        let mut frames = frames.into_iter();
        if let Some(first) = frames.next() {
            if let Some(second) = frames.next() {
                image.push_animation_frame(first);
                image.push_animation_frame(second);
                for frame in frames {
                    image.push_animation_frame(frame);
                }
            } else {
                image.push_decoded_frame(first)?;
            }
        }
        Ok(image)
    }

    /// Retained CPU pixel/encoded storage, including explicitly materialized borrowed reads.
    pub fn retained_bytes(&self) -> usize {
        self.data
            .iter()
            .map(|frame| match frame {
                ImageFrameData::Rgba {
                    pixels, decoded, ..
                } => pixels.len() + decoded.get().map_or(0, Vec::capacity),
                ImageFrameData::Pixels(frame) => frame.buffer().as_raw().capacity(),
                ImageFrameData::Compact { bytes, decoded, .. } => {
                    bytes.capacity() + decoded.get().map_or(0, Vec::capacity)
                }
            })
            .sum()
    }

    pub(crate) fn frame_bytes(&self, index: usize) -> Result<Option<ImageFrameBytes<'_>>> {
        match self.data.get(index) {
            Some(ImageFrameData::Rgba {
                pixels,
                width,
                crop,
                decoded,
                ..
            }) => Ok(Some(if let Some(pixels) = decoded.get() {
                ImageFrameBytes::Borrowed(pixels)
            } else {
                ImageFrameBytes::Owned(rgba_frame_pixels(pixels, *width, *crop))
            })),
            Some(ImageFrameData::Pixels(frame)) => {
                Ok(Some(ImageFrameBytes::Borrowed(frame.buffer().as_raw())))
            }
            Some(ImageFrameData::Compact { bytes, decoded, .. }) => {
                if let Some(pixels) = decoded.get() {
                    Ok(Some(ImageFrameBytes::Borrowed(pixels)))
                } else {
                    Ok(Some(ImageFrameBytes::Owned(
                        image::load_from_memory_with_format(bytes, image::ImageFormat::Qoi)?
                            .into_rgba8()
                            .into_raw(),
                    )))
                }
            }
            None => Ok(None),
        }
    }

    /// Retain a memory lease for as long as this image's pixels are owned by any consumer.
    pub fn retaining(mut self, resource: impl std::any::Any + Send + Sync) -> Self {
        self.retained.push(Box::new(resource));
        self
    }

    /// Convert this image into a byte slice.
    pub fn as_bytes(&self, frame_index: usize) -> Option<&[u8]> {
        self.data.get(frame_index).map(|frame| match frame {
            ImageFrameData::Rgba {
                pixels,
                width,
                crop,
                decoded,
                ..
            } => decoded
                .get_or_init(|| rgba_frame_pixels(pixels, *width, *crop))
                .as_slice(),
            ImageFrameData::Pixels(frame) => frame.buffer().as_raw().as_slice(),
            ImageFrameData::Compact { bytes, decoded, .. } => decoded
                .get_or_init(|| {
                    image::load_from_memory_with_format(bytes, image::ImageFormat::Qoi)
                        .expect("internally encoded QOI frame remains valid")
                        .into_rgba8()
                        .into_raw()
                })
                .as_slice(),
        })
    }

    /// Get the size of this image, in pixels.
    pub fn size(&self, frame_index: usize) -> Size<DevicePixels> {
        self.data
            .get(frame_index)
            .map(|frame| match frame {
                ImageFrameData::Rgba { crop, .. } => {
                    size(crop.size.width.into(), crop.size.height.into())
                }
                ImageFrameData::Pixels(frame) => {
                    let (width, height) = frame.buffer().dimensions();
                    size(width.into(), height.into())
                }
                ImageFrameData::Compact { size, .. } => *size,
            })
            .unwrap_or_default()
    }

    /// Get the size of this image, in pixels for display, adjusted for the scale factor.
    pub(crate) fn render_size(&self, frame_index: usize) -> Size<Pixels> {
        self.intrinsic_size(frame_index)
            .map(|v| (v.0 as f32 / self.scale_factor).into())
    }

    pub(crate) fn with_intrinsic_size(mut self, size: Size<DevicePixels>) -> Self {
        self.intrinsic_size = Some(size);
        self
    }

    pub(crate) fn intrinsic_size(&self, frame_index: usize) -> Size<DevicePixels> {
        self.intrinsic_size
            .unwrap_or_else(|| self.size(frame_index))
    }

    /// Get the delay of this frame from the previous
    pub fn delay(&self, frame_index: usize) -> Delay {
        self.data
            .get(frame_index)
            .map(|frame| match frame {
                ImageFrameData::Rgba { delay, .. } => *delay,
                ImageFrameData::Pixels(frame) => frame.delay(),
                ImageFrameData::Compact { delay, .. } => *delay,
            })
            .unwrap_or(Delay::from_numer_denom_ms(100, 1))
    }

    /// Get the number of frames for this image.
    pub fn frame_count(&self) -> usize {
        self.data.len()
    }
}

impl fmt::Debug for RenderImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageData")
            .field("id", &self.id)
            .field("size", &self.size(0))
            .finish()
    }
}

fn rgba_frame_pixels(pixels: &[u8], width: u32, crop: crate::Bounds<u32>) -> Vec<u8> {
    let row_bytes = crop.size.width as usize * 4;
    let mut converted = Vec::with_capacity(row_bytes * crop.size.height as usize);
    for row in crop.origin.y..crop.origin.y + crop.size.height {
        let start = (row as usize * width as usize + crop.origin.x as usize) * 4;
        converted.extend_from_slice(&pixels[start..start + row_bytes]);
    }
    for pixel in converted.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    converted
}

#[cfg(test)]
mod tests {
    use super::*;
    use smallvec::SmallVec;

    #[test]
    fn shared_rgba_upload_crops_and_converts_without_retaining_bgra() {
        let pixels: Arc<[u8]> = vec![255, 0, 0, 255, 0, 0, 255, 128].into();
        let image = RenderImage::from_rgba_frames(
            [(pixels.clone(), Delay::from_numer_denom_ms(50, 1))],
            size(2, 1),
            crate::Bounds::new(crate::point(1, 0), size(1, 1)),
        )
        .unwrap();
        for _ in 0..3 {
            assert_eq!(
                image.frame_bytes(0).unwrap().unwrap().as_ref(),
                &[255, 0, 0, 128]
            );
            assert_eq!(image.retained_bytes(), 8);
        }
        assert_eq!(image.delay(0).numer_denom_ms(), (50, 1));
        assert_eq!(image.as_bytes(0), Some([255, 0, 0, 128].as_slice()));
        assert_eq!(
            image.frame_bytes(0).unwrap().unwrap().as_ref(),
            &[255, 0, 0, 128]
        );
        assert_eq!(pixels.as_ref(), &[255, 0, 0, 255, 0, 0, 255, 128]);
        assert!(
            RenderImage::from_rgba_frames(
                [(pixels, Delay::from_numer_denom_ms(0, 1))],
                size(2, 1),
                crate::Bounds::new(crate::point(1, 0), size(2, 1)),
            )
            .is_none()
        );
    }

    #[test]
    fn compact_frames_preserve_pixels_delays_and_borrowed_reads() {
        let mut image = RenderImage::new(SmallVec::<[Frame; 1]>::new());
        for index in 0..3 {
            let pixels = image::RgbaImage::from_fn(256, 128, |x, y| {
                image::Rgba([index, (x % 8) as u8, (y % 8) as u8, 255])
            });
            image
                .push_decoded_frame(Frame::from_parts(
                    pixels,
                    0,
                    0,
                    Delay::from_numer_denom_ms(20 + index as u32, 1),
                ))
                .unwrap();
        }
        assert!(image.retained_bytes() < 3 * 256 * 128 * 4);
        for index in [2, 0, 1, 2] {
            assert_eq!(image.size(index), size(256.into(), 128.into()));
            assert_eq!(
                image.delay(index),
                Delay::from_numer_denom_ms(20 + index as u32, 1)
            );
            let bytes = image.frame_bytes(index).unwrap().unwrap();
            for (position, pixel) in bytes.chunks_exact(4).enumerate() {
                assert_eq!(
                    pixel,
                    [
                        index as u8,
                        (position % 256 % 8) as u8,
                        (position / 256 % 8) as u8,
                        255
                    ]
                );
            }
        }
        let retained = image.retained_bytes();
        assert_eq!(&image.as_bytes(1).unwrap()[..4], &[1, 0, 0, 255]);
        assert!(image.retained_bytes() > retained);
        assert_eq!(
            &image.frame_bytes(1).unwrap().unwrap()[..4],
            &[1, 0, 0, 255]
        );
        assert_eq!(image.frame_count(), 3);
    }

    #[test]
    #[ignore = "manual lossless frame storage and upload-read benchmark"]
    fn image_memory_lifecycle() {
        let frames = || {
            (0..60).map(|index| {
                Frame::from_parts(
                    image::RgbaImage::from_fn(640, 360, |x, y| {
                        image::Rgba([((x / 16 + index) % 256) as u8, (y / 16) as u8, 80, 255])
                    }),
                    0,
                    0,
                    Delay::from_numer_denom_ms(40, 1),
                )
            })
        };
        for via_decode in [false, true] {
            let start = std::time::Instant::now();
            let image = if via_decode {
                RenderImage::from_decoded_frames(frames()).unwrap()
            } else {
                RenderImage::new(frames().collect::<SmallVec<[_; 1]>>())
            };
            let load_ms = start.elapsed().as_secs_f64() * 1000.0;
            let start = std::time::Instant::now();
            for index in 0..image.frame_count() {
                std::hint::black_box(image.frame_bytes(index).unwrap());
            }
            eprintln!(
                "IMAGE_MEMORY via_decode={via_decode} bytes={} build_ms={load_ms:.3} read_all_ms={:.3}",
                image.retained_bytes(),
                start.elapsed().as_secs_f64() * 1000.0
            );
        }
    }

    #[test]
    fn empty_render_image_does_not_panic() {
        let image = RenderImage::new(SmallVec::new());
        assert_eq!(image.frame_count(), 0);
        assert_eq!(image.size(0), Size::default());
        assert_eq!(image.as_bytes(0), None);
        assert_eq!(image.render_size(0), Size::default());
        assert_eq!(image.delay(0), Delay::from_numer_denom_ms(100, 1));
        let _ = format!("{image:?}");
    }

    #[test]
    fn dynamic_textures_keep_distinct_stable_identities() {
        let size = size(DevicePixels(640), DevicePixels(480));
        let first = DynamicTexture::new(size);
        let second = DynamicTexture::new(size);

        assert_eq!(first.size(), size);
        assert_ne!(first.id, second.id);
        assert_ne!(first, second);
    }
}
