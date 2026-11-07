//! Lossless path identities for String-based SFTP APIs.
//!
//! Valid UTF-8 stays unchanged. Each invalid byte is carried as NUL followed
//! by two hex digits; NUL cannot occur in a filesystem path. Only path fields
//! use this codec, never protocol handles, status text, or user file contents.

use serde::{de::Visitor, ser::Error as _, Deserializer, Serializer};
use std::{borrow::Cow, fmt::Write};

pub fn from_bytes(mut bytes: &[u8]) -> String {
    let mut path = String::new();
    loop {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                path.push_str(text);
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                path.push_str(std::str::from_utf8(&bytes[..valid]).expect("validated prefix"));
                bytes = &bytes[valid..];
                let length = error.error_len().unwrap_or(bytes.len());
                for byte in &bytes[..length] {
                    write!(path, "\0{byte:02x}").expect("writing a path");
                }
                bytes = &bytes[length..];
            }
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_wire_bytes_survive_identical_lossy_labels() {
        // Independent SFTP NAME payloads: request id, count, filename,
        // empty longname, empty attributes.
        for (byte, identity) in [(0xff, "\0ff.log"), (0xfe, "\0fe.log")] {
            let wire = vec![
                0, 0, 0, 7, 0, 0, 0, 1, 0, 0, 0, 5, byte, b'.', b'l', b'o', b'g', 0, 0, 0, 0, 0, 0,
                0, 0,
            ];
            let packet: crate::protocol::Name =
                crate::de::from_bytes(&mut wire.clone().into()).unwrap();
            assert_eq!(packet.files[0].filename, identity);
            assert_eq!(display(identity), "�.log");
            assert_eq!(
                crate::ser::to_bytes(&packet).unwrap().as_ref(),
                wire.as_slice()
            );
            let open = crate::protocol::Remove {
                id: 8,
                filename: format!("/{identity}"),
            };
            let encoded = crate::ser::to_bytes(&open).unwrap();
            assert_eq!(
                encoded.as_ref(),
                &[0, 0, 0, 8, 0, 0, 0, 6, b'/', byte, b'.', b'l', b'o', b'g']
            );
        }
    }
}

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

pub fn serialize<S: Serializer>(path: &str, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_bytes(&to_bytes(path).map_err(S::Error::custom)?)
}

pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    struct PathVisitor;
    impl<'de> Visitor<'de> for PathVisitor {
        type Value = String;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("SFTP path bytes")
        }
        fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<String, E> {
            if bytes.contains(&0) {
                return Err(E::custom("NUL in remote path"));
            }
            Ok(from_bytes(&bytes))
        }
    }
    deserializer.deserialize_byte_buf(PathVisitor)
}
