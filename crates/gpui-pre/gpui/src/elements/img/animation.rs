use super::*;

#[derive(Clone)]
pub(crate) enum AnimationInput {
    Image(Arc<Image>),
    Bytes(Arc<[u8]>),
}

impl AsRef<[u8]> for AnimationInput {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Image(image) => &image.bytes,
            Self::Bytes(bytes) => bytes,
        }
    }
}

pub(crate) fn decode_animation(
    input: AnimationInput,
    format: ImageFormat,
    target: (u32, u32),
    turns: u8,
) -> Result<RenderImage> {
    let frames = match format {
        ImageFormat::Gif => GifDecoder::new(Cursor::new(input))?.into_frames(),
        ImageFormat::WebP => {
            let mut decoder = WebPDecoder::new(Cursor::new(input))?;
            decoder.set_background_color(Rgba([0; 4]))?;
            decoder.into_frames()
        }
        _ => unreachable!("animation format checked by caller"),
    };
    let mut output = RenderImage::new(SmallVec::<[image::Frame; 1]>::new());
    let mut resizer = fast_image_resize::Resizer::new();
    for frame in frames {
        match frame {
            Ok(frame) => {
                let delay = frame.delay();
                let pixels =
                    super::decoded_size::resize_frame(frame.into_buffer(), target, &mut resizer)?;
                let mut pixels = super::decoded_size::rotate(pixels, turns);
                oxideterm_image_decoder::to_bgra(&mut pixels);
                output.push_animation_frame(image::Frame::from_parts(pixels, 0, 0, delay));
            }
            Err(error) => log::debug!("Skipping animated image frame due to decode error: {error}"),
        }
    }
    anyhow::ensure!(
        output.frame_count() > 0,
        "animated image contains no decodable frames"
    );
    Ok(output)
}
