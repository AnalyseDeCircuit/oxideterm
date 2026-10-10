use crate::{LoopCount, MediaError, MediaInfo};
use std::io::{Read, Seek, SeekFrom};

/// Scan sub-block lengths without retaining metadata, before image's decoder can allocate it.
pub(crate) fn inspect(reader: &mut (impl Read + Seek)) -> Result<MediaInfo, MediaError> {
    let end = reader.seek(SeekFrom::End(0))?;
    reader.seek(SeekFrom::Start(0))?;
    let mut header = [0; 13];
    reader.read_exact(&mut header)?;
    if &header[..6] != b"GIF87a" && &header[..6] != b"GIF89a" {
        return Err(MediaError::Unsupported);
    }
    let width = u16::from_le_bytes([header[6], header[7]]) as u32;
    let height = u16::from_le_bytes([header[8], header[9]]) as u32;
    skip_palette(reader, header[10], end)?;
    let mut frames = 0u32;
    let mut loops = LoopCount::Finite(1);
    loop {
        match byte(reader)? {
            0x3b => break,
            0x2c => {
                let mut descriptor = [0; 9];
                reader.read_exact(&mut descriptor)?;
                skip_palette(reader, descriptor[8], end)?;
                byte(reader)?;
                blocks(reader, end, false)?;
                frames = frames.checked_add(1).ok_or(MediaError::ResourceExhausted)?;
            }
            0x21 => {
                let label = byte(reader)?;
                let prefix = blocks(reader, end, true)?;
                if label == 0xff
                    && (&prefix[..11] == b"NETSCAPE2.0" || &prefix[..11] == b"ANIMEXTS1.0")
                    && prefix[11] == 1
                {
                    let repeat = u16::from_le_bytes([prefix[12], prefix[13]]) as u32;
                    loops = if repeat == 0 {
                        LoopCount::Infinite
                    } else {
                        LoopCount::Finite(repeat + 1)
                    };
                }
            }
            _ => return Err(MediaError::Decode("invalid GIF block".into())),
        }
    }
    if frames == 0 {
        return Err(MediaError::Decode("GIF contains no frames".into()));
    }
    Ok(MediaInfo {
        width,
        height,
        animated: frames > 1,
        frame_count: Some(frames),
        loops,
    })
}

fn byte(reader: &mut impl Read) -> Result<u8, MediaError> {
    let mut byte = [0];
    reader.read_exact(&mut byte)?;
    Ok(byte[0])
}

fn skip(reader: &mut impl Seek, count: u64, end: u64) -> Result<(), MediaError> {
    let next = reader
        .stream_position()?
        .checked_add(count)
        .filter(|next| *next <= end)
        .ok_or_else(|| MediaError::Decode("truncated GIF block".into()))?;
    reader.seek(SeekFrom::Start(next))?;
    Ok(())
}

fn skip_palette(reader: &mut impl Seek, packed: u8, end: u64) -> Result<(), MediaError> {
    if packed & 0x80 != 0 {
        skip(reader, 3 * (1u64 << ((packed & 7) + 1)), end)?;
    }
    Ok(())
}

fn blocks(
    reader: &mut (impl Read + Seek),
    end: u64,
    metadata: bool,
) -> Result<[u8; 14], MediaError> {
    let mut prefix = [0; 14];
    let mut retained = 0;
    let mut total = 0usize;
    loop {
        let count = byte(reader)? as usize;
        if count == 0 {
            return Ok(prefix);
        }
        total += count;
        if metadata && total > 64 * 1024 {
            return Err(MediaError::ResourceExhausted);
        }
        let take = (prefix.len() - retained).min(count);
        reader.read_exact(&mut prefix[retained..retained + take])?;
        retained += take;
        skip(reader, (count - take) as u64, end)?;
    }
}
