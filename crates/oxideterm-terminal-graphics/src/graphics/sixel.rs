// Bound the decoder's growing canvas without allocating or interpreting its colors.
fn sixel_canvas_bytes(
    sequence: &[u8],
    pixel_limit: u32,
    cache_mb: u32,
) -> Result<usize, GraphicsError> {
    let start = sequence
        .iter()
        .position(|byte| *byte == b'q')
        .ok_or(GraphicsError::UnsupportedImage)?
        + 1;
    let (mut x, mut y, mut width, mut height, mut repeat) =
        (0usize, 0usize, 1usize, 1usize, 1usize);
    let mut index = start;
    while index < sequence.len() {
        let byte = sequence[index];
        index += 1;
        match byte {
            0x1b | 0x9c => break,
            b'$' => x = 0,
            b'-' => {
                x = 0;
                y = y.saturating_add(6);
            }
            b'!' => {
                repeat = 0;
                while let Some(byte @ b'0'..=b'9') = sequence.get(index) {
                    repeat = repeat
                        .saturating_mul(10)
                        .saturating_add((byte - b'0') as usize);
                    index += 1;
                }
                repeat = repeat.max(1);
                if repeat > 0xffff {
                    return Err(GraphicsError::UnsupportedImage);
                }
            }
            b'#' => {
                while sequence
                    .get(index)
                    .is_some_and(|b| b.is_ascii_digit() || *b == b';')
                {
                    index += 1;
                }
            }
            b'"' => {
                let mut values = [0usize; 4];
                let mut field = 0;
                while let Some(byte) = sequence.get(index) {
                    match byte {
                        b'0'..=b'9' => {
                            if let Some(value) = values.get_mut(field) {
                                *value = value
                                    .saturating_mul(10)
                                    .saturating_add((byte - b'0') as usize);
                            }
                        }
                        b';' => field += 1,
                        _ => break,
                    }
                    index += 1;
                }
                width = width.max(values[2]);
                height = height.max(values[3]);
            }
            b'?'..=b'~' => {
                x = x.saturating_add(repeat);
                repeat = 1;
                width = width.max(x);
                height = height.max(y.saturating_add(6));
            }
            _ => {}
        }
        decoded_rgba_bytes(
            u32::try_from(width).map_err(|_| GraphicsError::PixelLimitExceeded)?,
            u32::try_from(height).map_err(|_| GraphicsError::PixelLimitExceeded)?,
            pixel_limit,
            cache_mb,
        )?;
    }
    decoded_rgba_bytes(width as u32, height as u32, pixel_limit, cache_mb)
}
