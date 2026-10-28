use anyhow::Result;
use image::{DynamicImage, RgbaImage};
use std::io::Cursor;
pub(crate) fn thumbnail(bytes: &[u8], target: (u32, u32)) -> Result<Option<RgbaImage>> {
    let mut decoder = jpeg_decoder::Decoder::new(Cursor::new(bytes));
    if let Some(limit) = image::Limits::default().max_alloc {
        decoder.set_max_decoding_buffer_size(limit.try_into().unwrap_or(usize::MAX));
    }
    decoder.read_info()?;
    let info = decoder.info().expect("JPEG header was read");
    if !matches!(
        info.pixel_format,
        jpeg_decoder::PixelFormat::L8 | jpeg_decoder::PixelFormat::RGB24
    ) {
        return Ok(None);
    }
    let (width, height) = decoder.scale(
        target.0.min(u16::MAX as u32) as u16,
        target.1.min(u16::MAX as u32) as u16,
    )?;
    let pixels = decoder.decode()?;
    let image = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(width as u32, height as u32, pixels)
                .expect("JPEG dimensions match output"),
        ),
        _ => DynamicImage::ImageLuma8(
            image::GrayImage::from_raw(width as u32, height as u32, pixels)
                .expect("JPEG dimensions match output"),
        ),
    };
    Ok(Some(
        image
            .resize_exact(target.0, target.1, image::imageops::FilterType::Triangle)
            .into_rgba8(),
    ))
}
