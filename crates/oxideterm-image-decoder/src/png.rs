use anyhow::Result;
use image::RgbaImage;
use std::io::Cursor;
pub(crate) fn thumbnail(bytes: &[u8], target: (u32, u32)) -> Result<Option<RgbaImage>> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info()?;
    let info = reader.info();
    // Adam7 and 16-bit sources retain the existing pixel/precision contract.
    if info.interlaced
        || info.bit_depth == png::BitDepth::Sixteen
        || info.animation_control.is_some()
    {
        return Ok(None);
    }
    let (width, height) = (info.width, info.height);
    let (color, depth) = reader.output_color_type();
    anyhow::ensure!(
        depth == png::BitDepth::Eight,
        "PNG expansion must produce eight-bit samples"
    );
    let channels = color.samples();
    let sx = width as f64 / target.0 as f64;
    let sy = height as f64 / target.1 as f64;
    let mut output = RgbaImage::new(target.0, target.1);
    let mut row = vec![[0.0f64; 4]; target.0 as usize];
    let mut sums = row.clone();
    let mut out_y = 0u32;
    for y in 0..height {
        let input = reader
            .next_row()?
            .ok_or_else(|| anyhow::anyhow!("truncated PNG rows"))?;
        for (x, sum) in row.iter_mut().enumerate() {
            *sum = [0.0; 4];
            let left = x as f64 * sx;
            let right = ((x + 1) as f64 * sx).min(width as f64);
            for ix in left.floor() as usize..right.ceil() as usize {
                let p = &input.data()[ix * channels..(ix + 1) * channels];
                let rgba = match color {
                    png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                    png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                    png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                    png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                    png::ColorType::Indexed => unreachable!("palette expanded by decoder"),
                };
                let weight = (right.min((ix + 1) as f64) - left.max(ix as f64)).max(0.0);
                let alpha = rgba[3] as f64;
                for c in 0..3 {
                    sum[c] += rgba[c] as f64 * alpha * weight;
                }
                sum[3] += alpha * weight;
            }
        }
        let mut top = y as f64;
        let bottom = (y + 1) as f64;
        while top < bottom && out_y < target.1 {
            let end = bottom.min((out_y + 1) as f64 * sy);
            let weight = end - top;
            for (sum, row) in sums.iter_mut().zip(&row) {
                for c in 0..4 {
                    sum[c] += row[c] * weight;
                }
            }
            top = end;
            if end >= (out_y + 1) as f64 * sy || y + 1 == height {
                for (x, sum) in sums.iter_mut().enumerate() {
                    let mut pixel = [0u8; 4];
                    if sum[3] > 0.0 {
                        for c in 0..3 {
                            pixel[c] = (sum[c] / sum[3]).round().clamp(0.0, 255.0) as u8;
                        }
                    }
                    pixel[3] = (sum[3] / (sx * sy)).round().clamp(0.0, 255.0) as u8;
                    output.put_pixel(x as u32, out_y, image::Rgba(pixel));
                    *sum = [0.0; 4];
                }
                out_y += 1;
            }
        }
    }
    reader.finish()?;
    Ok(Some(output))
}
