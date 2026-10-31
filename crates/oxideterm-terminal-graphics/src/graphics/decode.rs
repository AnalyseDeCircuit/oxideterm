#[derive(Clone, Debug)]
struct DecodedPixels {
    width: u32,
    height: u32,
    rgba: Arc<[u8]>,
    memory: ImageMemoryLease,
    frames: Vec<DecodedImageFrame>,
}

#[derive(Clone, Debug)]
struct DecodedImageFrame {
    rgba: Arc<[u8]>,
    memory: ImageMemoryLease,
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

fn decode_image_bytes(
    bytes: &[u8],
    pixel_limit: u32,
    storage_limit_mb: u32,
) -> Result<DecodedPixels, GraphicsError> {
    let format = image::guess_format(bytes).map_err(|_| GraphicsError::UnsupportedImage)?;
    if format == image::ImageFormat::Gif {
        let mut decoder = GifDecoder::new(std::io::Cursor::new(bytes))
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        decoder
            .set_limits(decoder_limits())
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let (width, height) = decoder.dimensions();
        enforce_pixel_limit(width, height, pixel_limit)?;
        let frame_bytes = decoded_rgba_bytes(width, height, pixel_limit, storage_limit_mb)?;
        let _workspace =
            reserve_decode_workspace(bytes.len().saturating_add(frame_bytes.saturating_mul(3)))?;
        let mut frames = decoder.into_frames();
        let first_memory = reserve_pixels(frame_bytes)?;
        let first_frame = frames
            .next()
            .ok_or(GraphicsError::UnsupportedImage)?
            .map_err(|error| GraphicsError::Decode(error.to_string()))?;
        let (delay_ms_numerator, delay_ms_denominator) = first_frame.delay().numer_denom_ms();
        let first_image = first_frame.into_buffer();
        let (width, height) = first_image.dimensions();
        enforce_pixel_limit(width, height, pixel_limit)?;
        let first_rgba: Arc<[u8]> = first_image.into_raw().into();
        let mut decoded_frames = vec![DecodedImageFrame {
            rgba: first_rgba.clone(),
            memory: first_memory.clone(),
            // GIF zero-delay frames are common in the wild. Normalize them
            // before rendering so protocol-level zero gaps can still mean
            // "skip this frame" for Kitty animation frames.
            delay_ms_numerator: normalize_gif_frame_delay_ms(delay_ms_numerator),
            delay_ms_denominator,
        }];
        loop {
            let Some(frame) = frames.next() else { break };
            let frame = frame.map_err(|error| GraphicsError::Decode(error.to_string()))?;
            enforce_storage_limit((decoded_frames.len() + 1) * frame_bytes, storage_limit_mb)?;
            let memory = reserve_pixels(frame_bytes)?;
            let (delay_ms_numerator, delay_ms_denominator) = frame.delay().numer_denom_ms();
            let image = frame.into_buffer();
            if image.dimensions() != (width, height) {
                return Err(GraphicsError::UnsupportedImage);
            }
            decoded_frames.push(DecodedImageFrame {
                rgba: image.into_raw().into(),
                memory,
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
            memory: first_memory,
            frames,
        });
    }

    let mut reader = ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|error| GraphicsError::Decode(error.to_string()))?;
    reader.limits(decoder_limits());
    let decoder = reader
        .into_decoder()
        .map_err(|error| GraphicsError::Decode(error.to_string()))?;
    let (width, height) = decoder.dimensions();
    let rgba_bytes = decoded_rgba_bytes(width, height, pixel_limit, storage_limit_mb)?;
    let raw_bytes =
        usize::try_from(decoder.total_bytes()).map_err(|_| GraphicsError::StorageLimitExceeded)?;
    let _workspace =
        reserve_decode_workspace(bytes.len().saturating_add(raw_bytes.max(rgba_bytes)))?;
    let memory = reserve_pixels(rgba_bytes)?;
    let image = image::DynamicImage::from_decoder(decoder)
        .map_err(|error| GraphicsError::Decode(error.to_string()))?
        .to_rgba8();
    Ok(DecodedPixels {
        width,
        height,
        rgba: image.into_raw().into(),
        memory,
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
    let payload = Zeroizing::new(
        BASE64
            .decode(encoded)
            .map_err(|_| GraphicsError::InvalidBase64)?,
    );
    enforce_storage_limit(payload.len(), storage_limit_mb)?;
    let offset = u64::from(parse_u32(params.get("O")).unwrap_or(0));
    let length = parse_u32(params.get("S")).filter(|n| *n > 0).map(u64::from);
    let bytes = match transmission {
        "d" => payload.to_vec(),
        "f" | "t" | "s" => {
            if !file_transmission.is_authorized() {
                file_transmission.note_authorization_request();
                return Err(GraphicsError::LocalFileTransmissionDisabled);
            }
            let name = Zeroizing::new(
                String::from_utf8(payload.to_vec())
                    .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?,
            );
            if name.contains('\0') {
                return Err(GraphicsError::LocalFileTransmissionAccessFailed);
            }
            if transmission == "s" {
                read_kitty_shared_memory(&name, offset, length, storage_limit_mb)?
            } else {
                read_kitty_file(
                    Path::new(name.as_str()),
                    transmission == "t",
                    offset,
                    length,
                    storage_limit_mb,
                )?
            }
        }
        _ => return Err(GraphicsError::UnsupportedImage),
    };
    match params.get("o").map(String::as_str) {
        None => Ok(bytes),
        Some("z") => {
            let input = Zeroizing::new(bytes);
            let decoder = flate2::read::ZlibDecoder::new(input.as_slice());
            let transfer_limit = storage_limit_mb.max(1) as u64 * 1024 * 1024;
            let limit = match params.get("f").map(String::as_str).unwrap_or("32") {
                "24" | "32" => {
                    let (width, height) = raw_dimensions(params)?;
                    u64::from(width)
                        .checked_mul(u64::from(height))
                        .and_then(|n| {
                            n.checked_mul(if params.get("f").is_some_and(|f| f == "24") {
                                3
                            } else {
                                4
                            })
                        })
                        .ok_or(GraphicsError::StorageLimitExceeded)?
                        .min(transfer_limit)
                }
                _ => transfer_limit,
            };
            let mut output = Vec::new();
            decoder
                .take(limit + 1)
                .read_to_end(&mut output)
                .map_err(|_| GraphicsError::InvalidCommand)?;
            if output.len() as u64 > limit {
                return Err(GraphicsError::StorageLimitExceeded);
            }
            Ok(output)
        }
        _ => Err(GraphicsError::UnsupportedImage),
    }
}

fn read_kitty_slice(
    mut file: fs::File,
    offset: u64,
    length: Option<u64>,
    storage_limit_mb: u32,
) -> Result<Vec<u8>, GraphicsError> {
    use std::io::{Seek, SeekFrom};
    let metadata = file
        .metadata()
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?;
    let available = metadata
        .len()
        .checked_sub(offset)
        .ok_or(GraphicsError::LocalFileTransmissionAccessFailed)?;
    let count = length.unwrap_or(available);
    if count > available {
        return Err(GraphicsError::LocalFileTransmissionAccessFailed);
    }
    enforce_storage_limit(
        usize::try_from(count).map_err(|_| GraphicsError::StorageLimitExceeded)?,
        storage_limit_mb,
    )?;
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?;
    let mut output = Vec::with_capacity(count as usize);
    file.take(count)
        .read_to_end(&mut output)
        .map_err(|_| GraphicsError::LocalFileTransmissionAccessFailed)?;
    if output.len() as u64 != count {
        return Err(GraphicsError::LocalFileTransmissionAccessFailed);
    }
    Ok(output)
}

fn read_kitty_file(
    path: &Path,
    temporary: bool,
    offset: u64,
    length: Option<u64>,
    storage_limit_mb: u32,
) -> Result<Vec<u8>, GraphicsError> {
    let failed = || GraphicsError::LocalFileTransmissionAccessFailed;
    if !path.is_absolute() {
        return Err(failed());
    }
    let canonical = fs::canonicalize(path).map_err(|_| failed())?;
    #[cfg(unix)]
    if ["/dev", "/proc", "/sys"]
        .iter()
        .any(|root| canonical.starts_with(root))
    {
        return Err(failed());
    }
    #[cfg(windows)]
    if canonical
        .to_string_lossy()
        .to_ascii_uppercase()
        .contains("GLOBALROOT")
    {
        return Err(failed());
    }
    if !fs::metadata(&canonical).map_err(|_| failed())?.is_file() {
        return Err(failed());
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A replaced FIFO must not block the terminal reader.
        options.custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW);
    }
    let file = options.open(&canonical).map_err(|_| failed())?;
    if !file.metadata().map_err(|_| failed())?.is_file() {
        return Err(failed());
    }
    let bytes = read_kitty_slice(file, offset, length, storage_limit_mb)?;
    if temporary
        && canonical
            .to_string_lossy()
            .contains("tty-graphics-protocol")
    {
        let temporary = std::env::temp_dir();
        let roots = std::iter::once(temporary.as_path());
        #[cfg(unix)]
        let roots = roots.chain(["/tmp", "/var/tmp", "/dev/shm"].map(Path::new));
        if roots
            .filter_map(|root| fs::canonicalize(root).ok())
            .any(|root| canonical.starts_with(root))
        {
            fs::remove_file(&canonical).map_err(|_| failed())?;
        }
    }
    Ok(bytes)
}

#[cfg(unix)]
fn read_kitty_shared_memory(
    name: &str,
    offset: u64,
    length: Option<u64>,
    storage_limit_mb: u32,
) -> Result<Vec<u8>, GraphicsError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let failed = || GraphicsError::LocalFileTransmissionAccessFailed;
    if !name.starts_with('/') || name.len() < 2 || name[1..].contains('/') {
        return Err(failed());
    }
    let name = std::ffi::CString::new(name).map_err(|_| failed())?;
    // The descriptor has one owner even when a bounded read fails.
    let fd = unsafe { libc::shm_open(name.as_ptr(), libc::O_RDONLY, 0) };
    if fd < 0 {
        return Err(failed());
    }
    let file = unsafe { fs::File::from_raw_fd(fd) };
    if unsafe { libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
        return Err(failed());
    }
    let available = file
        .metadata()
        .map_err(|_| failed())?
        .len()
        .checked_sub(offset)
        .ok_or_else(failed)?;
    let count = length.unwrap_or(available);
    if count == 0 || count > available {
        return Err(failed());
    }
    enforce_storage_limit(
        usize::try_from(count).map_err(|_| GraphicsError::StorageLimitExceeded)?,
        storage_limit_mb,
    )?;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page <= 0 {
        return Err(failed());
    }
    let aligned = offset / page as u64 * page as u64;
    let skip = (offset - aligned) as usize;
    let mapped = skip
        .checked_add(count as usize)
        .ok_or(GraphicsError::StorageLimitExceeded)?;
    let address = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            mapped,
            libc::PROT_READ,
            libc::MAP_SHARED,
            file.as_raw_fd(),
            aligned as libc::off_t,
        )
    };
    if address == libc::MAP_FAILED {
        return Err(failed());
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(address.cast::<u8>().add(skip), count as usize) }
            .to_vec();
    unsafe {
        libc::munmap(address, mapped);
    }
    if unsafe { libc::shm_unlink(name.as_ptr()) } != 0 {
        return Err(failed());
    }
    Ok(bytes)
}

#[cfg(windows)]
fn read_kitty_shared_memory(
    name: &str,
    offset: u64,
    length: Option<u64>,
    storage_limit_mb: u32,
) -> Result<Vec<u8>, GraphicsError> {
    use std::ffi::c_void;
    #[repr(C)]
    struct MemoryInfo {
        base: *mut c_void,
        allocation_base: *mut c_void,
        protection: u32,
        partition: u16,
        region_size: usize,
        state: u32,
        protect: u32,
        kind: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenFileMappingW(access: u32, inherit: i32, name: *const u16) -> *mut c_void;
        fn MapViewOfFile(
            handle: *mut c_void,
            access: u32,
            high: u32,
            low: u32,
            count: usize,
        ) -> *mut c_void;
        fn UnmapViewOfFile(address: *const c_void) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn VirtualQuery(address: *const c_void, info: *mut MemoryInfo, count: usize) -> usize;
    }
    let failed = || GraphicsError::LocalFileTransmissionAccessFailed;
    let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let base_offset = offset & !65535;
    let skip = (offset - base_offset) as usize;
    let count = match length {
        Some(n) => {
            enforce_storage_limit(
                usize::try_from(n).map_err(|_| GraphicsError::StorageLimitExceeded)?,
                storage_limit_mb,
            )?;
            skip.checked_add(n as usize)
                .ok_or(GraphicsError::StorageLimitExceeded)?
        }
        None => 0,
    };
    let handle = unsafe { OpenFileMappingW(4, 0, name.as_ptr()) };
    if handle.is_null() {
        return Err(failed());
    }
    let address = unsafe {
        MapViewOfFile(
            handle,
            4,
            (base_offset >> 32) as u32,
            base_offset as u32,
            count,
        )
    };
    if address.is_null() {
        unsafe {
            CloseHandle(handle);
        }
        return Err(failed());
    }
    let result = (|| {
        let mut info: MemoryInfo = unsafe { std::mem::zeroed() };
        if unsafe { VirtualQuery(address, &mut info, std::mem::size_of::<MemoryInfo>()) } == 0 {
            return Err(failed());
        }
        let available = info.region_size.checked_sub(skip).ok_or_else(failed)?;
        let length = length.map(|n| n as usize).unwrap_or(available);
        if length > available {
            return Err(failed());
        }
        enforce_storage_limit(length, storage_limit_mb)?;
        Ok(unsafe { std::slice::from_raw_parts(address.cast::<u8>().add(skip), length) }.to_vec())
    })();
    unsafe {
        UnmapViewOfFile(address);
        CloseHandle(handle);
    }
    result
}

fn decode_raw_rgb(
    bytes: &[u8],
    params: &HashMap<String, String>,
) -> Result<DecodedPixels, GraphicsError> {
    let (width, height) = raw_dimensions(params)?;
    enforce_raw_len(bytes, width, height, 3)?;
    let rgba_bytes = width as usize * height as usize * 4;
    let memory = reserve_pixels(rgba_bytes)?;
    let _workspace = reserve_decode_workspace(bytes.len().saturating_add(rgba_bytes))?;
    let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
    for chunk in bytes.chunks_exact(3) {
        rgba.extend_from_slice(&[chunk[0], chunk[1], chunk[2], 0xff]);
    }
    Ok(DecodedPixels {
        width,
        height,
        rgba: rgba.into(),
        memory,
        frames: Vec::new(),
    })
}

fn normalize_gif_frame_delay_ms(delay_ms: u32) -> u32 {
    if delay_ms == 0 { 100 } else { delay_ms }
}

fn decode_raw_rgba(
    bytes: &[u8],
    params: &HashMap<String, String>,
) -> Result<DecodedPixels, GraphicsError> {
    let (width, height) = raw_dimensions(params)?;
    enforce_raw_len(bytes, width, height, 4)?;
    let memory = reserve_pixels(bytes.len())?;
    let _workspace = reserve_decode_workspace(bytes.len())?;
    Ok(DecodedPixels {
        width,
        height,
        rgba: bytes.into(),
        memory,
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
    if width > 0
        && height > 0
        && width
            .checked_mul(height)
            .is_some_and(|pixels| pixels <= pixel_limit)
    {
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

fn encoded_storage_limit() -> usize {
    // Base64 payloads are roughly 4/3 of decoded data. Keep a small allowance
    // for protocol parameters while still bounding incomplete graphics control
    // sequences before they can stall the PTY reader.
    MAX_TRANSFER_MB as usize * 1024 * 1024 * 2
}

fn reserve_pixels(bytes: usize) -> Result<ImageMemoryLease, GraphicsError> {
    reserve_image_memory(bytes).ok_or(GraphicsError::StorageLimitExceeded)
}

fn decoder_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_WORKSPACE_BYTES as u64);
    limits
}

fn reserve_decode_workspace(bytes: usize) -> Result<ImageMemoryLease, GraphicsError> {
    if bytes > MAX_DECODE_WORKSPACE_BYTES {
        return Err(GraphicsError::StorageLimitExceeded);
    }
    reserve_pixels(bytes)
}

fn decoded_rgba_bytes(
    width: u32,
    height: u32,
    pixel_limit: u32,
    cache_mb: u32,
) -> Result<usize, GraphicsError> {
    enforce_pixel_limit(width, height, pixel_limit)?;
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(GraphicsError::StorageLimitExceeded)?;
    if bytes > MAX_DECODE_BYTES {
        return Err(GraphicsError::StorageLimitExceeded);
    }
    enforce_storage_limit(bytes, cache_mb)?;
    Ok(bytes)
}
