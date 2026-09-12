use super::{records::*, *};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

pub(super) fn store(
    tx: &redb::WriteTransaction,
    conversation: &str,
    text: String,
) -> Result<StoredValue> {
    let text = Zeroizing::new(text);
    let id = uuid::Uuid::new_v4().to_string();
    write_chunks(tx, conversation, &id, 0, &text)?;
    Ok(StoredValue::Text {
        id,
        bytes: text.len() as u64,
    })
}

pub(super) fn load(
    tx: &redb::ReadTransaction,
    conversation: &str,
    id: &str,
    expected_bytes: u64,
    cache: &parking_lot::Mutex<super::cache::HistoryCache>,
) -> Result<String> {
    let chunks = tx.open_table(TEXT_CHUNKS)?;
    let mut text = Zeroizing::new(Vec::new());
    for row in chunks.range((conversation, id, 0)..=(conversation, id, u64::MAX))? {
        let (offset, key) = row?;
        if offset.value().2 != text.len() as u64 {
            return Err(anyhow!("History content index is invalid"));
        }
        text.extend_from_slice(&chunk(tx, conversation, key.value(), cache)?);
    }
    if text.len() as u64 != expected_bytes {
        return Err(anyhow!("History content length is invalid"));
    }
    Ok(String::from_utf8(std::mem::take(&mut *text))?)
}

pub(super) fn chunk(
    tx: &redb::ReadTransaction,
    conversation: &str,
    key: &str,
    cache: &parking_lot::Mutex<super::cache::HistoryCache>,
) -> Result<Arc<[u8]>> {
    if let Some(bytes) = cache.lock().get(conversation, key) {
        return Ok(bytes);
    }
    let table = tx.open_table(BLOBS)?;
    let row = table
        .get((conversation, key))?
        .ok_or_else(|| anyhow!("History content is missing"))?;
    let decoded: Arc<[u8]> = Arc::from(decode(row.value())?);
    cache.lock().insert(conversation, key, decoded.clone());
    Ok(decoded)
}

fn decode(bytes: &[u8]) -> Result<Vec<u8>> {
    use std::io::Read;
    let decoded = match bytes.split_first() {
        Some((0, bytes)) if bytes.len() <= CONTENT_CHUNK_BYTES => bytes.to_vec(),
        Some((1, bytes)) => {
            let mut output = Zeroizing::new(Vec::new());
            zstd::stream::read::Decoder::new(bytes)?
                .take(CONTENT_CHUNK_BYTES as u64 + 1)
                .read_to_end(&mut output)?;
            if output.len() > CONTENT_CHUNK_BYTES {
                return Err(anyhow!("History content chunk exceeds its bound"));
            }
            std::mem::take(&mut *output)
        }
        _ => return Err(anyhow!("Invalid history content encoding")),
    };
    Ok(decoded)
}

pub(super) fn preview(tx: &redb::WriteTransaction, conversation: &str, id: &str) -> Result<String> {
    let key = tx
        .open_table(TEXT_CHUNKS)?
        .get((conversation, id, 0))?
        .ok_or_else(|| anyhow!("History text is missing"))?
        .value()
        .to_owned();
    let table = tx.open_table(BLOBS)?;
    let row = table
        .get((conversation, key.as_str()))?
        .ok_or_else(|| anyhow!("History content is missing"))?;
    let text = Zeroizing::new(decode(row.value())?);
    Ok(std::str::from_utf8(&text)?.chars().take(256).collect())
}

pub(super) fn update(
    tx: &redb::WriteTransaction,
    conversation: &str,
    id: String,
    text: String,
) -> Result<StoredValue> {
    let text = Zeroizing::new(text);
    let retained: HashSet<_> = write_chunks(tx, conversation, &id, 0, &text)?
        .into_iter()
        .collect();
    let obsolete = tx
        .open_table(TEXT_CHUNKS)?
        .range((conversation, id.as_str(), 0)..=(conversation, id.as_str(), u64::MAX))?
        .filter_map(|row| match row {
            Ok((offset, key)) if !retained.contains(&offset.value().2) => {
                Some(Ok((offset.value().2, key.value().to_owned())))
            }
            Ok(_) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (offset, key) in obsolete {
        tx.open_table(TEXT_CHUNKS)?
            .remove((conversation, id.as_str(), offset))?;
        release_chunk(tx, conversation, &key)?;
    }
    Ok(StoredValue::Text {
        id,
        bytes: text.len() as u64,
    })
}

pub(super) fn replace_tail(
    tx: &redb::WriteTransaction,
    conversation: &str,
    id: String,
    bytes: u64,
    keep: u64,
    append: &str,
) -> Result<StoredValue> {
    if keep > bytes {
        return Err(anyhow!("History text boundary is invalid"));
    }
    let mut prefix = Zeroizing::new(String::new());
    let mut start = keep;
    if keep > 0 {
        let entry = {
            let table = tx.open_table(TEXT_CHUNKS)?;
            table
                .range((conversation, id.as_str(), 0)..=(conversation, id.as_str(), keep))?
                .next_back()
                .transpose()?
                .map(|(offset, key)| (offset.value().2, key.value().to_owned()))
                .ok_or_else(|| anyhow!("History text boundary is missing"))?
        };
        if entry.0 < keep {
            let data = {
                let table = tx.open_table(BLOBS)?;
                let row = table
                    .get((conversation, entry.1.as_str()))?
                    .ok_or_else(|| anyhow!("History content is missing"))?;
                Zeroizing::new(decode(row.value())?)
            };
            let text = std::str::from_utf8(&data)?;
            let retained = usize::try_from(keep - entry.0)?;
            if retained > text.len() || !text.is_char_boundary(retained) {
                return Err(anyhow!("History text boundary is invalid"));
            }
            if retained != CONTENT_CHUNK_BYTES {
                start = entry.0;
                prefix.push_str(&text[..retained]);
            }
        }
    }
    remove_tail(tx, conversation, &id, start)?;
    prefix.push_str(append);
    write_chunks(tx, conversation, &id, start, &prefix)?;
    Ok(StoredValue::Text {
        id,
        bytes: keep + append.len() as u64,
    })
}

pub(super) fn remove_tail(
    tx: &redb::WriteTransaction,
    conversation: &str,
    id: &str,
    start: u64,
) -> Result<()> {
    let entries = tx
        .open_table(TEXT_CHUNKS)?
        .range((conversation, id, start)..=(conversation, id, u64::MAX))?
        .map(|row| row.map(|(offset, key)| (offset.value().2, key.value().to_owned())))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (offset, key) in entries {
        tx.open_table(TEXT_CHUNKS)?
            .remove((conversation, id, offset))?;
        release_chunk(tx, conversation, &key)?;
    }
    Ok(())
}

fn write_chunks(
    tx: &redb::WriteTransaction,
    conversation: &str,
    id: &str,
    mut offset: u64,
    mut text: &str,
) -> Result<Vec<u64>> {
    let mut offsets = Vec::new();
    while !text.is_empty() {
        offsets.push(offset);
        let mut length = text.len().min(CONTENT_CHUNK_BYTES);
        while !text.is_char_boundary(length) {
            length -= 1;
        }
        let part = &text[..length];
        let key = format!("{:x}", Sha256::digest(part.as_bytes()));
        let old = tx
            .open_table(TEXT_CHUNKS)?
            .get((conversation, id, offset))?
            .map(|row| row.value().to_owned());
        if old.as_deref() != Some(&key) {
            retain_chunk(tx, conversation, part, &key)?;
            tx.open_table(TEXT_CHUNKS)?
                .insert((conversation, id, offset), key.as_str())?;
            if let Some(old) = old {
                release_chunk(tx, conversation, &old)?;
            }
        }
        text = &text[length..];
        offset += length as u64;
    }
    Ok(offsets)
}

fn retain_chunk(
    tx: &redb::WriteTransaction,
    conversation: &str,
    chunk: &str,
    key: &str,
) -> Result<()> {
    let mut blobs = tx.open_table(BLOBS)?;
    if blobs.get((conversation, key))?.is_none() {
        let compressed = Zeroizing::new(zstd::encode_all(chunk.as_bytes(), 1)?);
        let mut bytes = Zeroizing::new(Vec::new());
        if compressed.len() < chunk.len() {
            bytes.push(1);
            bytes.extend_from_slice(&compressed);
        } else {
            bytes.push(0);
            bytes.extend_from_slice(chunk.as_bytes());
        }
        blobs.insert((conversation, key), bytes.as_slice())?;
    }
    let mut refs = tx.open_table(REFS)?;
    let count = refs
        .get((conversation, key))?
        .map(|row| row.value())
        .unwrap_or(0);
    refs.insert((conversation, key), count + 1)?;
    Ok(())
}

fn release_chunk(tx: &redb::WriteTransaction, conversation: &str, key: &str) -> Result<()> {
    let mut refs = tx.open_table(REFS)?;
    let count = refs
        .get((conversation, key))?
        .map(|row| row.value())
        .ok_or_else(|| anyhow!("History reference is missing"))?;
    if count == 1 {
        refs.remove((conversation, key))?;
        tx.open_table(BLOBS)?.remove((conversation, key))?;
    } else {
        refs.insert((conversation, key), count - 1)?;
    }
    Ok(())
}
