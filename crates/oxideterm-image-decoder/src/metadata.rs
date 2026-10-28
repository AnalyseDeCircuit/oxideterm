use crate::{
    jpeg::thumbnail as jpeg_thumbnail, pixels::resize_frame, png::thumbnail as png_thumbnail,
};
use anyhow::Result;
use image::{DynamicImage, ImageDecoder, ImageFormat, RgbaImage, metadata::Orientation};
use std::io::Cursor;

#[derive(Clone, Copy)]
pub struct ImageInfo {
    pub size: (u32, u32),
    orientation: Orientation,
    pub format: ImageFormat,
}

impl ImageInfo {
    pub fn read(bytes: &[u8], declared: Option<ImageFormat>) -> Result<Option<Self>> {
        let format = match declared {
            Some(ImageFormat::Png) => ImageFormat::Png,
            Some(ImageFormat::Jpeg) => ImageFormat::Jpeg,
            Some(ImageFormat::Gif) => ImageFormat::Gif,
            Some(ImageFormat::WebP) => ImageFormat::WebP,
            Some(_) => return Ok(None),
            None => match image::guess_format(bytes) {
                Ok(
                    format @ (ImageFormat::Png
                    | ImageFormat::Jpeg
                    | ImageFormat::Gif
                    | ImageFormat::WebP),
                ) => format,
                _ => return Ok(None),
            },
        };
        match format {
            ImageFormat::Png => {
                let reader = png::Decoder::new(Cursor::new(bytes)).read_info()?;
                let info = reader.info();
                if info.interlaced
                    || info.bit_depth == png::BitDepth::Sixteen
                    || info.animation_control.is_some()
                {
                    return Ok(None);
                }
            }
            ImageFormat::Jpeg => {
                let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
                decoder.read_info()?;
                if !matches!(
                    decoder.info().unwrap().pixel_format,
                    jpeg_decoder::PixelFormat::L8 | jpeg_decoder::PixelFormat::RGB24
                ) || decoder.info().unwrap().coding_process
                    == jpeg_decoder::CodingProcess::Lossless
                {
                    return Ok(None);
                }
            }
            _ => {}
        }
        let mut decoder =
            image::ImageReader::with_format(Cursor::new(bytes), format).into_decoder()?;
        let mut size = decoder.dimensions();
        // The existing animated WebP path uses the codec canvas as-is; EXIF is
        // applied only to static images (including declared WebP Image sources).
        let orientation = if format == ImageFormat::WebP
            && declared.is_none()
            && image::codecs::webp::WebPDecoder::new(Cursor::new(bytes))?.has_animation()
        {
            Orientation::NoTransforms
        } else {
            decoder.orientation()?
        };
        if swaps_axes(orientation) {
            size = (size.1, size.0);
        }
        Ok(Some(Self {
            size,
            orientation,
            format,
        }))
    }

    /// Keep the background renderer's existing raw-pixel orientation contract.
    pub fn without_orientation(mut self) -> Self {
        if swaps_axes(self.orientation) {
            self.size = (self.size.1, self.size.0);
        }
        self.orientation = Orientation::NoTransforms;
        self
    }

    pub fn target(&self, bounds: (u32, u32)) -> (u32, u32) {
        let ratio = (bounds.0 as f64 / self.size.0 as f64)
            .min(bounds.1 as f64 / self.size.1 as f64)
            .min(1.0);
        (
            ((self.size.0 as f64 * ratio).ceil() as u32).max(1),
            ((self.size.1 as f64 * ratio).ceil() as u32).max(1),
        )
    }

    /// None means this format/variant needs the original full-resolution decoder.
    pub fn decode(&self, bytes: &[u8], bounds: (u32, u32)) -> Result<Option<RgbaImage>> {
        let mut target = self.target(bounds);
        if target == self.size {
            return Ok(None);
        }
        if swaps_axes(self.orientation) {
            target = (target.1, target.0);
        }
        let pixels = match self.format {
            ImageFormat::Png => png_thumbnail(bytes, target)?,
            ImageFormat::Jpeg => jpeg_thumbnail(bytes, target)?,
            ImageFormat::WebP => Some(resize_frame(
                image::load_from_memory_with_format(bytes, ImageFormat::WebP)?.into_rgba8(),
                target,
                &mut fast_image_resize::Resizer::new(),
            )?),
            _ => return Ok(None),
        };
        Ok(pixels.map(|pixels| {
            let mut image = DynamicImage::ImageRgba8(pixels);
            image.apply_orientation(self.orientation);
            image.into_rgba8()
        }))
    }
}

fn swaps_axes(orientation: Orientation) -> bool {
    matches!(
        orientation,
        Orientation::Rotate90
            | Orientation::Rotate270
            | Orientation::Rotate90FlipH
            | Orientation::Rotate270FlipH
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::ImageEncoder;

    fn png_bytes(image: &RgbaImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(
                image,
                image.width(),
                image.height(),
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn png_rows_preserve_transparency_and_non_integer_resize_coverage() {
        let pixels = RgbaImage::from_fn(7, 5, |x, _| {
            if x < 3 {
                image::Rgba([255, 0, 0, 255])
            } else {
                image::Rgba([0, 0, 255, 0])
            }
        });
        let bytes = png_bytes(&pixels);
        let info = ImageInfo::read(&bytes, None).unwrap().unwrap();
        assert!(ImageInfo::read(&bytes, Some(ImageFormat::Jpeg)).is_err());
        let resized = info.decode(&bytes, (3, 3)).unwrap().unwrap();
        assert_eq!(resized.dimensions(), (3, 3));
        for y in 0..3 {
            assert_eq!(resized.get_pixel(0, y).0, [255, 0, 0, 255]);
            assert_eq!(resized.get_pixel(1, y).0, [255, 0, 0, 73]);
            assert_eq!(resized.get_pixel(2, y).0, [0, 0, 0, 0]);
        }
        assert!(
            info.decode(&bytes, (7, 5)).unwrap().is_none(),
            "native resolution uses the unchanged decoder"
        );
    }

    #[test]
    fn jpeg_scaled_decode_respects_orientation() {
        let bytes = include_bytes!(
            "../../gpui-ce/gpui/examples/legacy/image/exif-orientation-rotate-180.jpg"
        );
        let info = ImageInfo::read(bytes, None).unwrap().unwrap();
        let pixels = info.decode(bytes, (8, 16)).unwrap().unwrap();
        assert_eq!(pixels.dimensions(), (8, 16));
        assert!(pixels.get_pixel(0, 0).0[..3].iter().all(|&c| c >= 245));
        assert!(pixels.get_pixel(7, 15).0[..3].iter().all(|&c| c <= 10));

        let pixels = image::RgbImage::from_fn(600, 300, |x, _| {
            image::Rgb([if x < 300 { 255 } else { 0 }; 3])
        });
        let mut bytes = Vec::new();
        let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut bytes);
        // Little-endian TIFF IFD with one Orientation=6 (clockwise 90 degrees) entry.
        encoder
            .set_exif_metadata(vec![
                73, 73, 42, 0, 8, 0, 0, 0, 1, 0, 18, 1, 3, 0, 1, 0, 0, 0, 6, 0, 0, 0, 0, 0, 0, 0,
            ])
            .unwrap();
        encoder
            .encode(&pixels, 600, 300, image::ExtendedColorType::Rgb8)
            .unwrap();
        let info = ImageInfo::read(&bytes, None).unwrap().unwrap();
        assert_eq!(info.size, (300, 600));
        let pixels = info.decode(&bytes, (75, 150)).unwrap().unwrap();
        assert_eq!(pixels.dimensions(), (75, 150));
        assert!(pixels.get_pixel(30, 20).0[..3].iter().all(|&c| c >= 245));
        assert!(pixels.get_pixel(30, 120).0[..3].iter().all(|&c| c <= 10));
    }
}
