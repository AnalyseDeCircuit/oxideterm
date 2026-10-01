// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use chacha20poly1305::{
    ChaCha20Poly1305, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use rand::RngCore;
use zeroize::Zeroizing;

use super::{
    FileHeader, MAGIC, NONCE_LEN, OxideBatchDecryptionContext, OxideBatchEncryptionContext,
    OxideFileError, SALT_LEN, TAG_LEN, kdf_flags,
};

const CONTAINER_VERSION: u32 = 2;
const HEADER_LEN: usize = 21;
const PREFIX_LEN: usize = HEADER_LEN + SALT_LEN + NONCE_LEN;
const LOCAL_KEY_FLAG: u32 = 1 << 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OxideDocumentKind {
    Archive = 1,
    SyncSnapshot = 2,
    LocalReplica = 3,
    Recovery = 4,
}

impl OxideDocumentKind {
    fn is_local(self) -> bool {
        matches!(self, Self::LocalReplica | Self::Recovery)
    }
}

pub fn seal_oxide_document(
    kind: OxideDocumentKind,
    payload: &[u8],
    context: &OxideBatchEncryptionContext,
) -> Result<Vec<u8>, OxideFileError> {
    if kind.is_local() {
        return Err(invalid_container());
    }
    seal(
        kind,
        payload,
        &context.key,
        context.salt,
        context.kdf_version,
    )
}

pub fn open_oxide_document(
    bytes: &[u8],
    expected: OxideDocumentKind,
    context: &mut OxideBatchDecryptionContext,
) -> Result<Zeroizing<Vec<u8>>, OxideFileError> {
    let flags = validate_container(bytes)?;
    if expected.is_local() || flags & LOCAL_KEY_FLAG != 0 {
        return Err(invalid_container());
    }
    let salt = bytes[HEADER_LEN..HEADER_LEN + SALT_LEN]
        .try_into()
        .map_err(|_| invalid_container())?;
    let key = context.key_for(salt, flags & kdf_flags::KDF_VERSION_MASK)?;
    open(bytes, expected, key)
}

/// Local replica and recovery keys belong to the platform secret store, never to an archive.
pub fn seal_local_oxide_document(
    kind: OxideDocumentKind,
    payload: &[u8],
    key: &[u8; 32],
) -> Result<Vec<u8>, OxideFileError> {
    if !kind.is_local() {
        return Err(invalid_container());
    }
    seal(kind, payload, key, [0; SALT_LEN], LOCAL_KEY_FLAG)
}

pub fn open_local_oxide_document(
    bytes: &[u8],
    expected: OxideDocumentKind,
    key: &[u8; 32],
) -> Result<Zeroizing<Vec<u8>>, OxideFileError> {
    if !expected.is_local() || validate_container(bytes)? != LOCAL_KEY_FLAG {
        return Err(invalid_container());
    }
    open(bytes, expected, key)
}

pub(super) fn seal(
    kind: OxideDocumentKind,
    payload: &[u8],
    key: &[u8; 32],
    salt: [u8; SALT_LEN],
    flags: u32,
) -> Result<Vec<u8>, OxideFileError> {
    let plaintext_len = payload.len().checked_add(1).ok_or_else(invalid_container)?;
    let header = FileHeader {
        magic: *MAGIC,
        version: CONTAINER_VERSION,
        flags,
        metadata_length: 0,
        encrypted_data_length: plaintext_len.try_into().map_err(|_| invalid_container())?,
    };
    let mut nonce = [0; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    let mut bytes = Vec::with_capacity(PREFIX_LEN + plaintext_len + TAG_LEN);
    bytes.extend_from_slice(&header.to_bytes());
    bytes.extend_from_slice(&salt);
    bytes.extend_from_slice(&nonce);
    // Both metadata and document purpose are authenticated ciphertext. No caller
    // receives plaintext until the complete prefix and purpose have been checked.
    let mut plaintext = Zeroizing::new(Vec::with_capacity(plaintext_len));
    plaintext.push(kind as u8);
    plaintext.extend_from_slice(payload);
    let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|_| OxideFileError::CryptoError)?;
    let encrypted = cipher
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext.as_slice(),
                aad: &bytes,
            },
        )
        .map_err(|_| OxideFileError::EncryptionFailed)?;
    bytes.extend_from_slice(&encrypted);
    Ok(bytes)
}

pub(super) fn open(
    bytes: &[u8],
    expected: OxideDocumentKind,
    key: &[u8; 32],
) -> Result<Zeroizing<Vec<u8>>, OxideFileError> {
    let cipher = ChaCha20Poly1305::new_from_slice(key).map_err(|_| OxideFileError::CryptoError)?;
    let nonce = &bytes[HEADER_LEN + SALT_LEN..PREFIX_LEN];
    let mut plaintext = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: &bytes[PREFIX_LEN..],
                    aad: &bytes[..PREFIX_LEN],
                },
            )
            .map_err(|_| OxideFileError::DecryptionFailed)?,
    );
    if plaintext.first().copied() != Some(expected as u8) {
        return Err(invalid_container());
    }
    plaintext.remove(0);
    Ok(plaintext)
}

pub(super) fn validate_container(bytes: &[u8]) -> Result<u32, OxideFileError> {
    if bytes.len() < PREFIX_LEN + TAG_LEN + 1 || bytes.get(..MAGIC.len()) != Some(MAGIC) {
        return Err(invalid_container());
    }
    let read = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    let version = read(5);
    if version != CONTAINER_VERSION {
        return Err(OxideFileError::UnsupportedVersion(version));
    }
    let flags = read(9);
    if read(13) != 0 || flags & !(LOCAL_KEY_FLAG | kdf_flags::KDF_VERSION_MASK) != 0 {
        return Err(invalid_container());
    }
    let expected_len = (read(17) as usize)
        .checked_add(PREFIX_LEN + TAG_LEN)
        .ok_or_else(invalid_container)?;
    if expected_len != bytes.len() {
        return Err(invalid_container());
    }
    Ok(flags)
}

fn invalid_container() -> OxideFileError {
    OxideFileError::InvalidFormat("Invalid .oxide container or document purpose".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_container_authenticates_content_prefix_and_purpose() {
        let key = [0x57; 32];
        let bytes = seal_local_oxide_document(
            OxideDocumentKind::LocalReplica,
            b"synthetic private connection metadata",
            &key,
        )
        .unwrap();
        assert_eq!(&bytes[..9], b"OXIDE\x02\0\0\0");
        assert!(!bytes.windows(10).any(|window| window == b"connection"));
        assert_eq!(
            open_local_oxide_document(&bytes, OxideDocumentKind::LocalReplica, &key)
                .unwrap()
                .as_slice(),
            b"synthetic private connection metadata"
        );
        assert!(open_local_oxide_document(&bytes, OxideDocumentKind::Recovery, &key).is_err());
        assert!(
            open_local_oxide_document(&bytes, OxideDocumentKind::LocalReplica, &[0x58; 32])
                .is_err()
        );
        for offset in [
            0,
            5,
            9,
            13,
            17,
            HEADER_LEN,
            PREFIX_LEN - 1,
            PREFIX_LEN,
            bytes.len() - 1,
        ] {
            let mut changed = bytes.clone();
            changed[offset] ^= 1;
            assert!(
                open_local_oxide_document(&changed, OxideDocumentKind::LocalReplica, &key).is_err(),
                "accepted mutation at byte {offset}"
            );
        }
        let mut appended = bytes.clone();
        appended.push(0);
        assert!(
            open_local_oxide_document(&appended, OxideDocumentKind::LocalReplica, &key).is_err()
        );
        assert!(
            open_local_oxide_document(
                &bytes[..bytes.len() - 1],
                OxideDocumentKind::LocalReplica,
                &key
            )
            .is_err()
        );
    }
}
