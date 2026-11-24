use std::borrow::Cow;

pub fn to_bytes(path: &str) -> Result<Vec<u8>, &'static str> {
    let mut bytes = Vec::with_capacity(path.len());
    let mut rest = path.as_bytes();
    while let Some((&byte, tail)) = rest.split_first() {
        if byte == 0 {
            let hex = tail.get(..2).ok_or("Invalid escaped SFTP path")?;
            let text = std::str::from_utf8(hex).map_err(|_| "Invalid escaped SFTP path")?;
            bytes.push(u8::from_str_radix(text, 16).map_err(|_| "Invalid escaped SFTP path")?);
            rest = &tail[2..];
        } else {
            bytes.push(byte);
            rest = tail;
        }
    }
    Ok(bytes)
}

/// For labels only; never pass this value back to a filesystem operation.
pub fn display(path: &str) -> Cow<'_, str> {
    if !path.contains('\0') {
        return Cow::Borrowed(path);
    }
    match to_bytes(path) {
        Ok(bytes) => Cow::Owned(String::from_utf8_lossy(&bytes).into_owned()),
        Err(_) => Cow::Borrowed(path),
    }
}
