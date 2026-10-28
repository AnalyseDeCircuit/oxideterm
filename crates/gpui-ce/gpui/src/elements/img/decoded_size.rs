use crate::RenderImage;
use anyhow::Result;
use image::ImageFormat;
pub(super) use oxideterm_image_decoder::{ImageInfo, resize_frame, rotate};
use std::io::Cursor;

pub(super) fn animation(
    info: ImageInfo,
    input: super::AnimationInput,
    target: (u32, u32),
    turns: u8,
    animate_webp: bool,
) -> Result<Option<RenderImage>> {
    if info.format == ImageFormat::Gif
        || (info.format == ImageFormat::WebP
            && animate_webp
            && image::codecs::webp::WebPDecoder::new(Cursor::new(input.as_ref()))?.has_animation())
    {
        return super::AnimationStream::open(input, info.format, info.size, target, turns);
    }
    Ok(None)
}
