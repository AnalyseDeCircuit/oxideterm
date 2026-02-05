//! Cryptographic operations for .oxide file encryption/decryption

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{aead::Aead, ChaCha20Poly1305, KeyInit, Nonce};
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use super::error::OxideFileError;
use super::format::{
    kdf_flags, EncryptedConnection, EncryptedPayload, OxideFile, OxideMetadata, NONCE_LEN,
    SALT_LEN, TAG_LEN,
};

/// KDF parameters for different versions
struct KdfParams {
    memory_cost: u32,  // in KB
    iterations: u32,
    parallelism: u32,
}

impl KdfParams {
    /// Get KDF parameters for a specific version
    fn for_version(version: u32) -> Result<Self, OxideFileError> {
        match version {
            kdf_flags::KDF_V1 | 0 => {
                // v1 (default): 256MB, 4 iterations, parallelism=4
                // Also handle legacy files with flags=0
                Ok(KdfParams {
                    memory_cost: 262144,  // 256 MB
                    iterations: 4,
                    parallelism: 4,
                })
            }
            kdf_flags::KDF_V2 => {
                // v2 (future): 512MB, 6 iterations, parallelism=4
                Ok(KdfParams {
                    memory_cost: 524288,  // 512 MB
                    iterations: 6,
                    parallelism: 4,
                })
            }
            _ => Err(OxideFileError::UnsupportedKdfVersion(version)),
        }
    }
}

/// Derive encryption key from password using Argon2id with specified KDF version
///
/// Default (v1): 4 iterations, 256MB memory, parallelism=4 (~2 seconds on modern CPU)
/// Provides strong protection against GPU brute-force attacks
pub fn derive_key(
    password: &str,
    salt: &[u8],
    kdf_version: u32,
) -> Result<Zeroizing<[u8; 32]>, OxideFileError> {
    let kdf_params = KdfParams::for_version(kdf_version)?;

    let params = Params::new(
        kdf_params.memory_cost,
        kdf_params.iterations,
        kdf_params.parallelism,
        Some(32), // 32 byte output
    )
    .map_err(|_| OxideFileError::CryptoError)?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut key = Zeroizing::new([0u8; 32]);
    argon2
        .hash_password_into(password.as_bytes(), salt, &mut *key)
        .map_err(|_| OxideFileError::CryptoError)?;

    Ok(key)
}

/// Encrypt payload and create .oxide file structure
pub fn encrypt_oxide_file(
    payload: &EncryptedPayload,
    password: &str,
    metadata: OxideMetadata,
) -> Result<OxideFile, OxideFileError> {
    // 1. Generate random salt and nonce using cryptographically secure RNG
    let mut salt = [0u8; SALT_LEN];
    let mut nonce = [0u8; NONCE_LEN];
    rand::rngs::OsRng.fill_bytes(&mut salt);
    rand::rngs::OsRng.fill_bytes(&mut nonce);

    // 2. Derive encryption key from password using current KDF version
    let key = derive_key(password, &salt, kdf_flags::CURRENT_KDF)?;

    // 3. Serialize payload with MessagePack (supports tagged enums)
    let plaintext = rmp_serde::to_vec_named(payload)?;

    // 4. Encrypt with ChaCha20-Poly1305
    let cipher =
        ChaCha20Poly1305::new_from_slice(&*key).map_err(|_| OxideFileError::CryptoError)?;

    let nonce_obj = Nonce::from_slice(&nonce);

    let ciphertext = cipher
        .encrypt(nonce_obj, plaintext.as_ref())
        .map_err(|_| OxideFileError::EncryptionFailed)?;

    // 5. Split ciphertext and authentication tag
    // ChaCha20Poly1305 appends the 16-byte tag to the ciphertext
    if ciphertext.len() < TAG_LEN {
        return Err(OxideFileError::CryptoError);
    }

    let (encrypted_data, tag_slice) = ciphertext.split_at(ciphertext.len() - TAG_LEN);
    let mut tag = [0u8; TAG_LEN];
    tag.copy_from_slice(tag_slice);

    Ok(OxideFile {
        metadata,
        salt,
        nonce,
        encrypted_data: encrypted_data.to_vec(),
        tag,
        kdf_version: kdf_flags::CURRENT_KDF,
    })
}

/// Decrypt .oxide file and extract payload
///
/// Returns error if password is wrong or data is corrupted/tampered
pub fn decrypt_oxide_file(
    oxide_file: &OxideFile,
    password: &str,
) -> Result<EncryptedPayload, OxideFileError> {
    // 1. Derive key from password and salt using file's KDF version
    let key = derive_key(password, &oxide_file.salt, oxide_file.kdf_version)?;

    // 2. Prepare cipher
    let cipher =
        ChaCha20Poly1305::new_from_slice(&*key).map_err(|_| OxideFileError::CryptoError)?;

    let nonce_obj = Nonce::from_slice(&oxide_file.nonce);

    // 3. Reconstruct ciphertext with tag
    let mut ciphertext_with_tag = oxide_file.encrypted_data.clone();
    ciphertext_with_tag.extend_from_slice(&oxide_file.tag);

    // 4. Decrypt and authenticate
    let plaintext = cipher
        .decrypt(nonce_obj, ciphertext_with_tag.as_ref())
        .map_err(|_| OxideFileError::DecryptionFailed)?;

    // 5. Deserialize payload with MessagePack
    let payload: EncryptedPayload =
        rmp_serde::from_slice(&plaintext)?;

    // 6. Verify internal checksum
    verify_checksum(&payload)?;

    Ok(payload)
}

/// Compute SHA-256 checksum of connections
pub fn compute_checksum(connections: &[EncryptedConnection]) -> Result<String, OxideFileError> {
    let mut hasher = Sha256::new();

    for conn in connections {
        let conn_bytes = rmp_serde::to_vec_named(conn)?;
        hasher.update(&conn_bytes);
    }

    Ok(format!("sha256:{:x}", hasher.finalize()))
}

/// Verify payload checksum matches
fn verify_checksum(payload: &EncryptedPayload) -> Result<(), OxideFileError> {
    let computed = compute_checksum(&payload.connections)?;

    if computed != payload.checksum {
        return Err(OxideFileError::ChecksumMismatch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::format::EncryptedAuth;
    use super::*;
    use crate::config::types::ConnectionOptions;
    use chrono::Utc;

    fn create_test_connection() -> EncryptedConnection {
        EncryptedConnection {
            name: "Test Server".to_string(),
            group: Some("Production".to_string()),
            host: "example.com".to_string(),
            port: 22,
            username: "admin".to_string(),
            auth: EncryptedAuth::Password {
                password: "secret123".to_string(),
            },
            color: None,
            tags: vec![],
            options: ConnectionOptions::default(),
            proxy_chain: vec![],
        }
    }

    fn create_test_payload() -> EncryptedPayload {
        let connections = vec![create_test_connection()];
        let checksum = compute_checksum(&connections).unwrap();

        EncryptedPayload {
            version: 1,
            connections,
            checksum,
        }
    }

    fn create_test_metadata() -> OxideMetadata {
        OxideMetadata {
            exported_at: Utc::now(),
            exported_by: "OxideTerm v0.1.0".to_string(),
            description: Some("Test export".to_string()),
            num_connections: 1,
            connection_names: vec!["Test Server".to_string()],
        }
    }

    #[test]
    fn test_key_derivation() {
        let password = "TestPassword123!";
        let salt = [0u8; 32];

        let key1 = derive_key(password, &salt, kdf_flags::KDF_V1).unwrap();
        let key2 = derive_key(password, &salt, kdf_flags::KDF_V1).unwrap();

        // Same password and salt should produce same key
        assert_eq!(&*key1, &*key2);

        // Different salt should produce different key
        let mut different_salt = [0u8; 32];
        different_salt[0] = 1;
        let key3 = derive_key(password, &different_salt, kdf_flags::KDF_V1).unwrap();
        assert_ne!(&*key1, &*key3);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let payload = create_test_payload();
        let metadata = create_test_metadata();
        let password = "TestPassword123!";

        // Encrypt
        let oxide_file = encrypt_oxide_file(&payload, password, metadata).unwrap();

        // Decrypt
        let decrypted = decrypt_oxide_file(&oxide_file, password).unwrap();

        // Verify
        assert_eq!(payload.connections.len(), decrypted.connections.len());
        assert_eq!(payload.connections[0].name, decrypted.connections[0].name);
        assert_eq!(payload.connections[0].host, decrypted.connections[0].host);
    }

    #[test]
    fn test_wrong_password_fails() {
        let payload = create_test_payload();
        let metadata = create_test_metadata();

        let oxide_file = encrypt_oxide_file(&payload, "correct123!", metadata).unwrap();

        let result = decrypt_oxide_file(&oxide_file, "wrong123!");
        assert!(matches!(result, Err(OxideFileError::DecryptionFailed)));
    }

    #[test]
    fn test_tamper_detection() {
        let payload = create_test_payload();
        let metadata = create_test_metadata();

        let mut oxide_file = encrypt_oxide_file(&payload, "test123!", metadata).unwrap();

        // Tamper with encrypted data
        if !oxide_file.encrypted_data.is_empty() {
            oxide_file.encrypted_data[0] ^= 0xFF;
        }

        let result = decrypt_oxide_file(&oxide_file, "test123!");
        assert!(result.is_err()); // Should fail AEAD verification
    }

    #[test]
    fn test_checksum_computation() {
        let conn = create_test_connection();
        let checksum1 = compute_checksum(&[conn.clone()]).unwrap();
        let checksum2 = compute_checksum(&[conn.clone()]).unwrap();

        // Same connection should produce same checksum
        assert_eq!(checksum1, checksum2);
        assert!(checksum1.starts_with("sha256:"));

        // Different connection should produce different checksum
        let mut conn2 = conn;
        conn2.name = "Different".to_string();
        let checksum3 = compute_checksum(&[conn2]).unwrap();
        assert_ne!(checksum1, checksum3);
    }
}
