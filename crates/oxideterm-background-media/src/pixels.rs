use crate::{BackgroundFit, MediaError, MemoryBudget, OutputParams, PixelLease};
use image::{DynamicImage, GenericImageView};
use std::sync::{Arc, Mutex};

#[derive(Default)]
pub(crate) struct RecycledPixels {
    pub bytes: usize,
    pub buffers: Vec<(Vec<u8>, crate::PixelLease)>,
}

#[derive(Default)]
pub(crate) struct Processor {
    resizer: fast_image_resize::Resizer,
    scratch: Option<crate::PixelLease>,
    shape: Option<((u32, u32), (u32, u32))>,
    pub pool: Arc<Mutex<RecycledPixels>>,
}

pub(crate) fn pixel_bytes(width: u32, height: u32) -> Result<usize, MediaError> {
    if width == 0 || height == 0 {
        return Err(MediaError::Decode("empty canvas".into()));
    }
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(MediaError::ResourceExhausted)
}

pub fn output_dimensions(
    source: (u32, u32),
    output: OutputParams,
) -> Result<(u32, u32), MediaError> {
    output.limits.validate()?;
    pixel_bytes(source.0, source.1)?;
    if output.width == 0 || output.height == 0 || !output.blur.is_finite() || output.blur < 0.0 {
        return Err(MediaError::InvalidOutput);
    }
    let (width, height) = source;
    let dimensions = if output.fit == BackgroundFit::Tile {
        source
    } else if output.fit == BackgroundFit::Fill {
        (width.min(output.width), height.min(output.height))
    } else {
        let width_limited = u64::from(output.width) * u64::from(height)
            <= u64::from(output.height) * u64::from(width);
        let (numerator, denominator) = if width_limited == (output.fit == BackgroundFit::Contain) {
            (output.width.min(width), width)
        } else {
            (output.height.min(height), height)
        };
        let scale = |edge: u32| {
            (u64::from(edge) * u64::from(numerator)).div_ceil(u64::from(denominator)) as u32
        };
        (scale(width), scale(height))
    };
    let max_width = output
        .limits
        .max_width
        .unwrap_or(dimensions.0)
        .min(dimensions.0);
    let max_height = output
        .limits
        .max_height
        .unwrap_or(dimensions.1)
        .min(dimensions.1);
    let scale =
        (max_width as f64 / dimensions.0 as f64).min(max_height as f64 / dimensions.1 as f64);
    Ok((
        (dimensions.0 as f64 * scale).floor().max(1.0) as u32,
        (dimensions.1 as f64 * scale).floor().max(1.0) as u32,
    ))
}

pub(crate) fn process(
    image: DynamicImage,
    output: OutputParams,
    budget: &MemoryBudget,
    bgra: bool,
) -> Result<(Vec<u8>, PixelLease, (u32, u32)), MediaError> {
    Processor::default().process(image, output, budget, bgra)
}

impl Processor {
    pub fn process(
        &mut self,
        image: DynamicImage,
        output: OutputParams,
        budget: &MemoryBudget,
        bgra: bool,
    ) -> Result<(Vec<u8>, PixelLease, (u32, u32)), MediaError> {
        let dimensions = output_dimensions(image.dimensions(), output)?;
        let bytes = pixel_bytes(dimensions.0, dimensions.1)?;
        let shape = (image.dimensions(), dimensions);
        if self.shape != Some(shape) {
            self.resizer.reset_internal_buffers();
            self.scratch = None;
            let mut pool = self.pool.lock().unwrap_or_else(|error| error.into_inner());
            pool.buffers.clear();
            pool.bytes = if shape.0 != dimensions { bytes } else { 0 };
            self.shape = Some(shape);
        }
        let resizing = dimensions != image.dimensions();
        let reused = if resizing {
            self.pool
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .buffers
                .pop()
        } else {
            None
        };
        let (mut pixels, result_lease) = match reused {
            Some(buffer) => buffer,
            None => {
                let lease = budget.reserve(bytes)?;
                (if resizing { vec![0; bytes] } else { Vec::new() }, lease)
            }
        };
        let _blur_scratch = budget.reserve(if output.blur > 0.01 { bytes * 4 } else { 0 })?;
        let image = if resizing {
            if self.scratch.is_none() {
                // Retained SIMD work buffers include alpha and convolution intermediates.
                self.scratch = Some(budget.reserve(pixel_bytes(shape.0.0, shape.0.1)? * 3)?);
            }
            let image = DynamicImage::ImageRgba8(image.into_rgba8());
            let mut target = fast_image_resize::images::Image::from_slice_u8(
                dimensions.0,
                dimensions.1,
                &mut pixels,
                fast_image_resize::PixelType::U8x4,
            )
            .map_err(|error| MediaError::Decode(error.to_string()))?;
            self.resizer
                .resize(
                    &image,
                    &mut target,
                    &fast_image_resize::ResizeOptions::new()
                        .use_alpha(!bgra)
                        .resize_alg(fast_image_resize::ResizeAlg::Convolution(
                            fast_image_resize::FilterType::Bilinear,
                        )),
                )
                .map_err(|error| MediaError::Decode(error.to_string()))?;
            DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(dimensions.0, dimensions.1, pixels)
                    .expect("validated frame dimensions"),
            )
        } else {
            // No processing needs a copy when the decoder already produced the requested size.
            drop(pixels);
            image
        };
        let image = if output.blur > 0.01 && bgra {
            // Video is opaque; the box-filter approximation avoids a radius-dependent convolution cost.
            image.fast_blur(output.blur)
        } else if output.blur > 0.01 {
            image.blur(output.blur)
        } else {
            image
        };
        let mut pixels = image.into_rgba8().into_raw();
        if !bgra {
            for pixel in pixels.chunks_exact_mut(4) {
                pixel.swap(0, 2);
            }
        }
        Ok((pixels, result_lease, dimensions))
    }
}
