fn entered_control_sequence(before: &ParserState, after: &ParserState) -> bool {
    matches!(before, ParserState::Ground | ParserState::Esc)
        && !matches!(after, ParserState::Ground | ParserState::Esc)
}

#[derive(Clone, Debug)]
struct DecodedPixels {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    frames: Vec<DecodedImageFrame>,
}

#[derive(Clone, Debug)]
struct DecodedImageFrame {
    rgba: Vec<u8>,
    delay_ms_numerator: u32,
    delay_ms_denominator: u32,
}

fn looks_like_sixel(data: &[u8]) -> bool {
    data.iter()
        .position(|byte| *byte == b'q')
        .is_some_and(|index| {
            data[..=index]
                .iter()
                .all(|byte| byte.is_ascii() && !byte.is_ascii_alphabetic() || *byte == b'q')
        })
}

fn decode_image_bytes(bytes: &[u8], pixel_limit: u32) -> Result<DecodedPixels, GraphicsError> {
    let format = image::guess_format(bytes).map_err(|_| GraphicsError::UnsupportedImage)?;
    if format == image::ImageFormat::Gif {
        let decoder = GifDecoder::new(std::io::Cursor::new(bytes))
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let mut frames = decoder.into_frames();
        let first_frame = frames
            .next()
            .ok_or(GraphicsError::UnsupportedImage)?
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let (delay_ms_numerator, delay_ms_denominator) = first_frame.delay().numer_denom_ms();
        let first_image = first_frame.into_buffer();
        let (width, height) = first_image.dimensions();
        enforce_pixel_limit(width, height, pixel_limit)?;
        let first_rgba = first_image.into_raw();
        let mut decoded_frames = vec![DecodedImageFrame {
            rgba: first_rgba.clone(),
            // GIF zero-delay frames are common in the wild. Normalize them
            // before rendering so protocol-level zero gaps can still mean
            // "skip this frame" for Kitty animation frames.
            delay_ms_numerator: normalize_gif_frame_delay_ms(delay_ms_numerator),
            delay_ms_denominator,
        }];
        for frame in frames {
            let frame = frame.map_err(|error| GraphicsError::Decode(error.to_string()))?;
            let (delay_ms_numerator, delay_ms_denominator) = frame.delay().numer_denom_ms();
            let image = frame.into_buffer();
            if image.dimensions() != (width, height) {
                return Err(GraphicsError::UnsupportedImage);
            }
            decoded_frames.push(DecodedImageFrame {
                rgba: image.into_raw(),
                delay_ms_numerator: normalize_gif_frame_delay_ms(delay_ms_numerator),
                delay_ms_denominator,
            });
        }
        let frames = (decoded_frames.len() > 1)
            .then_some(decoded_frames)
            .unwrap_or_default();
        return Ok(DecodedPixels {
            width,
            height,
            rgba: first_rgba,
            frames,
        });
    }

    let image = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| GraphicsError::Decode(error.to_string()))?
        .decode()
        .map_err(|error| GraphicsError::Decode(error.to_string()))?
        .to_rgba8();
    let (width, height) = image.dimensions();
    enforce_pixel_limit(width, height, pixel_limit)?;
    Ok(DecodedPixels {
        width,
        height,
        rgba: image.into_raw(),
        frames: Vec::new(),
    })
}

fn decode_kitty_payload(
    params: &HashMap<String, String>,
    encoded: &[u8],
    storage_limit_mb: u32,
) -> Result<Vec<u8>, GraphicsError> {
    let payload = BASE64
        .decode(encoded)
        .map_err(|_| GraphicsError::InvalidBase64)?;
    enforce_storage_limit(payload.len(), storage_limit_mb)?;

    let transmission = params.get("t").map(String::as_str).unwrap_or("d");
    match transmission {
        "d" => Ok(payload),
        "f" | "t" => {
            let path = String::from_utf8(payload).map_err(|_| GraphicsError::InvalidPath)?;
            let path = path.trim_end_matches('\0');
            let metadata =
                fs::metadata(path).map_err(|error| GraphicsError::Io(error.to_string()))?;
            enforce_storage_limit(metadata.len() as usize, storage_limit_mb)?;
            let bytes = fs::read(path).map_err(|error| GraphicsError::Io(error.to_string()))?;
            if transmission == "t" {
                let _ = fs::remove_file(path);
            }
            Ok(bytes)
        }
        _ => Err(GraphicsError::UnsupportedImage),
    }
}

fn decode_raw_rgb(
    bytes: &[u8],
    params: &HashMap<String, String>,
) -> Result<DecodedPixels, GraphicsError> {
    let (width, height) = raw_dimensions(params)?;
    enforce_raw_len(bytes, width, height, 3)?;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for chunk in bytes.chunks_exact(3) {
        rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 0xff]);
    }
    Ok(DecodedPixels {
        width,
        height,
        rgba,
        frames: Vec::new(),
    })
}

fn normalize_gif_frame_delay_ms(delay_ms: u32) -> u32 {
    if delay_ms == 0 {
        100
    } else {
        delay_ms
    }
}

fn decode_raw_rgba(
    bytes: &[u8],
    params: &HashMap<String, String>,
) -> Result<DecodedPixels, GraphicsError> {
    let (width, height) = raw_dimensions(params)?;
    enforce_raw_len(bytes, width, height, 4)?;
    Ok(DecodedPixels {
        width,
        height,
        rgba: bytes.to_vec(),
        frames: Vec::new(),
    })
}

fn raw_dimensions(params: &HashMap<String, String>) -> Result<(u32, u32), GraphicsError> {
    let width = params
        .get("s")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(GraphicsError::UnsupportedImage)?;
    let height = params
        .get("v")
        .and_then(|value| value.parse::<u32>().ok())
        .ok_or(GraphicsError::UnsupportedImage)?;
    Ok((width, height))
}

fn enforce_raw_len(
    bytes: &[u8],
    width: u32,
    height: u32,
    channels: usize,
) -> Result<(), GraphicsError> {
    let expected = width as usize * height as usize * channels;
    if bytes.len() == expected {
        Ok(())
    } else {
        Err(GraphicsError::UnsupportedImage)
    }
}

fn enforce_pixel_limit(width: u32, height: u32, pixel_limit: u32) -> Result<(), GraphicsError> {
    if width.saturating_mul(height) <= pixel_limit {
        Ok(())
    } else {
        Err(GraphicsError::PixelLimitExceeded)
    }
}

fn enforce_storage_limit(bytes: usize, storage_limit_mb: u32) -> Result<(), GraphicsError> {
    let limit = storage_limit_mb.max(1) as usize * 1024 * 1024;
    if bytes <= limit {
        Ok(())
    } else {
        Err(GraphicsError::StorageLimitExceeded)
    }
}

fn encoded_storage_limit(storage_limit_mb: u32) -> usize {
    // Base64 payloads are roughly 4/3 of decoded data. Keep a small allowance
    // for protocol parameters while still bounding incomplete graphics control
    // sequences before they can stall the PTY reader.
    storage_limit_mb.max(1) as usize * 1024 * 1024 * 2
}
