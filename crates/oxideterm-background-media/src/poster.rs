use crate::{
    MediaError, MediaFrame, MediaInfo, MemoryBudget, OutputParams, decoder::Decoder, pixels,
};
use std::path::Path;

// Galleries request many covers together. Admit one full-resolution poster decode at a time.
static POSTER_DECODE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Inspect playback capability without decoding pixels. Call on a background thread.
pub fn is_animated_media(path: &Path) -> Result<bool, MediaError> {
    let reader = image::ImageReader::open(path)?.with_guessed_format()?;
    match reader.format() {
        Some(image::ImageFormat::Gif) => Ok(crate::gif_metadata::inspect(
            &mut std::io::BufReader::new(std::fs::File::open(path)?),
        )?
        .animated),
        Some(image::ImageFormat::WebP) => Ok(image_webp::WebPDecoder::new(
            std::io::BufReader::new(std::fs::File::open(path)?),
        )?
        .num_frames()
            > 1),
        Some(_) => Ok(false),
        None => {
            if path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| {
                    value.eq_ignore_ascii_case("mp4") || value.eq_ignore_ascii_case("m4v")
                })
            {
                Ok(true)
            } else {
                Err(MediaError::Unsupported)
            }
        }
    }
}

/// Decode exactly one composed frame. Call on a background thread; video objects stay local.
pub fn decode_poster(
    path: &Path,
    output: OutputParams,
) -> Result<(MediaInfo, MediaFrame), MediaError> {
    let _decode = POSTER_DECODE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let budget = MemoryBudget::default();
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    let mut decoder = Decoder::open(path, &budget, &cancelled)?;
    let (image, _, _) = decoder
        .next(&cancelled)?
        .ok_or_else(|| MediaError::Decode("media contains no frame".into()))?;
    let (pixels, lease, (width, height)) =
        pixels::process(image, output, &budget, decoder.is_bgra())?;
    Ok((
        decoder.info.clone(),
        MediaFrame {
            generation: 0,
            sequence: 1,
            timestamp: std::time::Duration::ZERO,
            duration: std::time::Duration::ZERO,
            processing_time: std::time::Duration::ZERO,
            width,
            height,
            pixels,
            native: None,
            _lease: Some(lease),
            recycle: None,
        },
    ))
}
