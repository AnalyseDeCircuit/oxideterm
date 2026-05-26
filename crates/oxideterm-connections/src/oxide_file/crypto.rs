use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{ChaCha20Poly1305, KeyInit, Nonce, aead::Aead};
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::{
    EncryptedPayload, NONCE_LEN, OxideFile, OxideFileError, OxideMetadata, SALT_LEN, TAG_LEN,
    kdf_flags,
};

struct KdfParams {
    memory_cost: u32,
    iterations: u32,
    parallelism: u32,
}

impl KdfParams {
    fn for_version(version: u32) -> Result<Self, OxideFileError> {
        match version {
            kdf_flags::KDF_V1 | 0 => Ok(Self {
                memory_cost: 262_144,
                iterations: 4,
                parallelism: 4,
            }),
            kdf_flags::KDF_V2 => Ok(Self {
                memory_cost: 524_288,
                iterations: 6,
                parallelism: 4,
            }),
            other => Err(OxideFileError::UnsupportedKdfVersion(other)),
        }
    }
}

pub fn derive_key(
    password: &str,
    salt: &[u8],
    kdf_version: u32,
) -> Result<Zeroizing<[u8; 32]>, OxideFileError> {
    let kdf = KdfParams::for_version(kdf_version)?;
    let params = Params::new(kdf.memory_cost, kdf.iterations, kdf.parallelism, Some(32))
        .map_err(|_| OxideFileError::CryptoError)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|_| OxideFileError::CryptoError)?;
    Ok(key)
}

pub fn encrypt_oxide_file(
    payload: &EncryptedPayload,
    password: &str,
    metadata: OxideMetadata,
) -> Result<OxideFile, OxideFileError> {
    encrypt_oxide_file_with_progress(payload, password, metadata, |_| {})
}

pub fn encrypt_oxide_file_with_progress<F>(
    payload: &EncryptedPayload,
    password: &str,
    metadata: OxideMetadata,
    mut on_progress: F,
) -> Result<OxideFile, OxideFileError>
where
    F: FnMut(&'static str),
{
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce);
    on_progress("generating_salt_nonce");

    let key = derive_key(password, &salt, kdf_flags::CURRENT_KDF)?;
    on_progress("deriving_key");

    let plaintext = Zeroizing::new(rmp_serde::to_vec_named(payload)?);
    on_progress("serializing_payload");

    let cipher =
        ChaCha20Poly1305::new_from_slice(&*key).map_err(|_| OxideFileError::CryptoError)?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext.as_ref())
        .map_err(|_| OxideFileError::EncryptionFailed)?;
    on_progress("encrypting_payload");

    if ciphertext.len() < TAG_LEN {
        return Err(OxideFileError::CryptoError);
    }
    let (encrypted_data, tag_slice) = ciphertext.split_at(ciphertext.len() - TAG_LEN);
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(tag_slice);
    on_progress("finalizing_file");

    Ok(OxideFile {
        metadata,
        salt,
        nonce,
        encrypted_data: encrypted_data.to_vec(),
        tag,
        kdf_version: kdf_flags::CURRENT_KDF,
    })
}

pub fn decrypt_oxide_file(
    oxide_file: &OxideFile,
    password: &str,
) -> Result<EncryptedPayload, OxideFileError> {
    decrypt_oxide_file_with_progress(oxide_file, password, |_| {})
}

pub fn decrypt_oxide_file_with_progress<F>(
    oxide_file: &OxideFile,
    password: &str,
    mut on_progress: F,
) -> Result<EncryptedPayload, OxideFileError>
where
    F: FnMut(&'static str),
{
    let key = derive_key(password, &oxide_file.salt, oxide_file.kdf_version)?;
    on_progress("deriving_key");

    let cipher =
        ChaCha20Poly1305::new_from_slice(&*key).map_err(|_| OxideFileError::CryptoError)?;
    let mut ciphertext_with_tag = oxide_file.encrypted_data.clone();
    ciphertext_with_tag.extend_from_slice(&oxide_file.tag);

    let plaintext = Zeroizing::new(
        cipher
            .decrypt(
                Nonce::from_slice(&oxide_file.nonce),
                ciphertext_with_tag.as_ref(),
            )
            .map_err(|_| OxideFileError::DecryptionFailed)?,
    );
    on_progress("decrypting_payload");

    let payload: EncryptedPayload = rmp_serde::from_slice(&plaintext)?;
    on_progress("deserializing_payload");
    verify_checksum(&payload)?;
    on_progress("verifying_checksum");
    Ok(payload)
}

pub fn compute_checksum(payload: &EncryptedPayload) -> Result<String, OxideFileError> {
    if payload.version <= 1
        && payload.app_settings_json.is_none()
        && payload.plugin_settings.is_empty()
        && payload.portable_secrets.is_empty()
    {
        return compute_legacy_checksum(payload);
    }

    let mut hasher = Sha256::new();
    hasher.update(payload.version.to_le_bytes());
    hasher.update((payload.connections.len() as u64).to_le_bytes());
    for conn in &payload.connections {
        // Serialized connections may include decrypted auth material; wipe the
        // checksum staging buffer after it is absorbed by SHA-256.
        let encoded = Zeroizing::new(rmp_serde::to_vec_named(conn)?);
        hasher.update(encoded.as_slice());
    }

    match &payload.app_settings_json {
        Some(json) => {
            hasher.update([1]);
            hasher.update(json.as_bytes());
        }
        None => hasher.update([0]),
    }

    hasher.update((payload.plugin_settings.len() as u64).to_le_bytes());
    for plugin_setting in &payload.plugin_settings {
        let encoded = Zeroizing::new(rmp_serde::to_vec_named(plugin_setting)?);
        hasher.update(encoded.as_slice());
    }

    hasher.update((payload.portable_secrets.len() as u64).to_le_bytes());
    for portable_secret in &payload.portable_secrets {
        let encoded = Zeroizing::new(rmp_serde::to_vec_named(portable_secret)?);
        hasher.update(encoded.as_slice());
    }

    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn compute_legacy_checksum(payload: &EncryptedPayload) -> Result<String, OxideFileError> {
    let mut hasher = Sha256::new();
    for conn in &payload.connections {
        // Legacy payloads still serialize auth data for checksum compatibility;
        // keep the temporary serialized form zeroized.
        let encoded = Zeroizing::new(rmp_serde::to_vec_named(conn)?);
        hasher.update(encoded.as_slice());
    }
    Ok(format!("sha256:{:x}", hasher.finalize()))
}

fn verify_checksum(payload: &EncryptedPayload) -> Result<(), OxideFileError> {
    if compute_checksum(payload)? == payload.checksum {
        Ok(())
    } else {
        Err(OxideFileError::ChecksumMismatch)
    }
}
