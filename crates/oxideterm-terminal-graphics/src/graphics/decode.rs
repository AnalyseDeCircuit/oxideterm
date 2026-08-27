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
    let Some(final_byte) = data.iter().position(|byte| *byte == b'q') else {
        return false;
    };

    // Sixel accepts only numeric Ps parameters before its `q` final byte. DCS queries such as
    // DECRQSS (`$q`) and XTGETTCAP (`+q`) must continue to the terminal parser unchanged.
    data[..final_byte]
        .iter()
        .all(|byte| byte.is_ascii_digit() || *byte == b';')
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
    file_transmission: &KittyFileTransmissionControl,
) -> Result<Vec<u8>, GraphicsError> {
    let transmission = params.get("t").map(String::as_str).unwrap_or("d");
    let payload = BASE64
        .decode(encoded)
        .map_err(|_| GraphicsError::InvalidBase64)?;
    enforce_storage_limit(payload.len(), storage_limit_mb)?;

    match transmission {
        "d" => Ok(payload),
        "f" | "t" => {
            let path = String::from_utf8(payload)
                .map_err(|_| GraphicsError::InvalidLocalFileTransmissionPath)?;
            let path = Path::new(path.trim_end_matches('\0'));
            read_kitty_file_from_session_sandbox(path, storage_limit_mb, file_transmission)
        }
        _ => Err(GraphicsError::UnsupportedImage),
    }
}

fn read_kitty_file_from_session_sandbox(
    requested_path: &Path,
    storage_limit_mb: u32,
    file_transmission: &KittyFileTransmissionControl,
) -> Result<Vec<u8>, GraphicsError> {
    let Some(sandbox_root) = file_transmission.authorized_root() else {
        file_transmission.note_authorization_request();
        return Err(GraphicsError::LocalFileTransmissionDisabled);
    };
    if !requested_path.is_absolute() {
        return Err(GraphicsError::InvalidLocalFileTransmissionPath);
    }

    let requested_metadata = fs::symlink_metadata(requested_path)
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?;
    if requested_metadata.file_type().is_symlink() || !requested_metadata.is_file() {
        return Err(GraphicsError::InvalidLocalFileTransmissionPath);
    }

    let canonical_root = fs::canonicalize(&sandbox_root)
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?;
    let canonical_path = fs::canonicalize(requested_path)
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?;
    let is_direct_sandbox_file = canonical_path.parent() == Some(canonical_root.as_path());
    let has_protocol_marker = canonical_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.contains("tty-graphics-protocol"));
    if !is_direct_sandbox_file || !has_protocol_marker {
        return Err(GraphicsError::InvalidLocalFileTransmissionPath);
    }

    enforce_storage_limit(requested_metadata.len() as usize, storage_limit_mb)?;
    let file = fs::File::open(&canonical_path)
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?;
    if !file
        .metadata()
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?
        .is_file()
    {
        return Err(GraphicsError::InvalidLocalFileTransmissionPath);
    }
    let limit = storage_limit_mb.max(1) as u64 * 1024 * 1024;
    let mut bytes = Vec::with_capacity(requested_metadata.len() as usize);
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?;
    enforce_storage_limit(bytes.len(), storage_limit_mb)?;

    // `t=t` is intentionally not unlinked here. The private TempDir owner
    // performs bounded cleanup when the session is denied, revoked, or dropped.
    Ok(bytes)
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
