use alacritty_terminal::term::graphics::{
    SIXEL_MAX_COLORS as SIXEL_COLORS, SIXEL_MAX_DIMENSION as SIXEL_DIMENSION,
};

struct SixelState {
    colors: [Option<[usize; 4]>; SIXEL_COLORS],
}

impl Default for SixelState {
    fn default() -> Self {
        Self {
            colors: [None; SIXEL_COLORS],
        }
    }
}

struct SixelScan {
    width: u32,
    height: u32,
    cursor_y: usize,
    aspect: [usize; 2],
    transparent: bool,
    payload: usize,
    colors: [Option<[usize; 4]>; SIXEL_COLORS],
}

fn sixel_parameters(data: &[u8], index: &mut usize, values: &mut [usize]) -> usize {
    let mut field = 0usize;
    while let Some(byte) = data.get(*index) {
        match byte {
            b'0'..=b'9' => {
                if let Some(value) = values.get_mut(field) {
                    *value = value
                        .saturating_mul(10)
                        .saturating_add((byte - b'0') as usize);
                }
            }
            b';' => field = field.saturating_add(1),
            _ => break,
        }
        *index += 1;
    }
    field.saturating_add(1)
}

// Bound the complete canvas before the codec allocates pixels; record only final color definitions.
fn scan_sixel(
    sequence: &[u8],
    pixel_limit: u32,
    cache_mb: u32,
    color_count: usize,
) -> Result<SixelScan, GraphicsError> {
    let payload = sequence
        .iter()
        .position(|byte| *byte == b'q')
        .ok_or(GraphicsError::UnsupportedImage)?
        + 1;
    let mut params = [0usize; 3];
    let mut header = 2;
    sixel_parameters(sequence, &mut header, &mut params);
    let mut scan = SixelScan {
        width: 1,
        height: 1,
        cursor_y: 0,
        payload,
        transparent: params[1] == 1,
        aspect: [
            match params[0] {
                2 => 5,
                3 | 4 => 3,
                7..=9 => 1,
                _ => 2,
            },
            1,
        ],
        colors: [None; SIXEL_COLORS],
    };
    let (mut x, mut y, mut width, mut height, mut repeat) =
        (0usize, 0usize, 1usize, 1usize, 1usize);
    let mut index = payload;
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
                let mut values = [0usize; 5];
                let count = sixel_parameters(sequence, &mut index, &mut values);
                if values[0] >= color_count {
                    return Err(GraphicsError::InvalidCommand);
                }
                if count >= 5 && matches!(values[1], 1 | 2) {
                    scan.colors[values[0]] = Some([
                        values[1],
                        values[2].min(if values[1] == 1 { 360 } else { 100 }),
                        values[3].min(100),
                        values[4].min(100),
                    ]);
                }
            }
            b'"' => {
                let mut values = [0usize; 4];
                sixel_parameters(sequence, &mut index, &mut values);
                scan.aspect = [values[0].max(1), values[1].max(1)];
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
    scan.width = width as u32;
    scan.height = height as u32;
    scan.cursor_y = y;
    Ok(scan)
}
