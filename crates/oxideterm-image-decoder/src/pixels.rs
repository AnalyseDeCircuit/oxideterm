use anyhow::Result;
use image::RgbaImage;

/// Convert owned RGBA pixels to the byte order used by the native image atlas.
pub fn to_bgra(pixels: &mut [u8]) {
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
}
pub fn rotate(pixels: RgbaImage, quarter_turns: u8) -> RgbaImage {
    match quarter_turns % 4 {
        1 => image::imageops::rotate90(&pixels),
        2 => {
            let mut pixels = pixels;
            image::imageops::rotate180_in_place(&mut pixels);
            pixels
        }
        3 => image::imageops::rotate270(&pixels),
        _ => pixels,
    }
}

pub fn resize_frame(
    mut pixels: RgbaImage,
    target: (u32, u32),
    resizer: &mut fast_image_resize::Resizer,
) -> Result<RgbaImage> {
    if pixels.dimensions() == target {
        return Ok(pixels);
    }
    let mut output = RgbaImage::new(target.0, target.1);
    // The composed frame is owned: premultiply it in place instead of letting the
    // resizer retain another full-size alpha buffer across every animation frame.
    let alpha = fast_image_resize::MulDiv::default();
    alpha.multiply_alpha_inplace(&mut pixels)?;
    resizer.resize(
        &pixels,
        &mut output,
        &fast_image_resize::ResizeOptions::new()
            .use_alpha(false)
            .resize_alg(fast_image_resize::ResizeAlg::Convolution(
                fast_image_resize::FilterType::Bilinear,
            )),
    )?;
    alpha.divide_alpha_inplace(&mut output)?;
    Ok(output)
}
