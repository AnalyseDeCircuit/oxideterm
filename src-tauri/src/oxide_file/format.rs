//! .oxide file format specification and binary serialization

use super::error::OxideFileError;
use crate::config::types::ConnectionOptions;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::{Cursor, Read};

/// Magic number identifying .oxide files
pub const MAGIC: &[u8; 5] = b"OXIDE";

/// Current file format version
pub const VERSION: u32 = 1;

/// Lengths of fixed-size fields
pub const SALT_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;
pub const TAG_LEN: usize = 16;

/// KDF (Key Derivation Function) version flags stored in header.flags
/// Lower 8 bits are reserved for KDF version selector
pub mod kdf_flags {
    /// KDF v1: Argon2id with 256MB memory, 4 iterations, parallelism=4
    /// This is the default and current version
    pub const KDF_V1: u32 = 0x0001;

    /// KDF v2: Reserved for future use (e.g., higher memory cost)
    pub const KDF_V2: u32 = 0x0002;

    /// Mask to extract KDF version from flags
    pub const KDF_VERSION_MASK: u32 = 0x00FF;

    /// Current KDF version used for new files
    pub const CURRENT_KDF: u32 = KDF_V1;
}

/// File header structure (21 bytes fixed)
#[derive(Debug)]
pub struct FileHeader {
    pub magic: [u8; 5],
    pub version: u32,
    pub flags: u32,
    pub metadata_length: u32,
    pub encrypted_data_length: u32,
}

impl FileHeader {
    pub fn new(metadata_length: u32, encrypted_data_length: u32) -> Self {
        Self {
            magic: *MAGIC,
            version: VERSION,
            flags: kdf_flags::CURRENT_KDF, // Set current KDF version in flags
            metadata_length,
            encrypted_data_length,
        }
    }

    /// Get the KDF version from flags
    pub fn kdf_version(&self) -> u32 {
        self.flags & kdf_flags::KDF_VERSION_MASK
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(21);
        bytes.extend_from_slice(&self.magic);
        bytes.extend_from_slice(&self.version.to_le_bytes());
        bytes.extend_from_slice(&self.flags.to_le_bytes());
        bytes.extend_from_slice(&self.metadata_length.to_le_bytes());
        bytes.extend_from_slice(&self.encrypted_data_length.to_le_bytes());
        bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, OxideFileError> {
        if data.len() < 21 {
            return Err(OxideFileError::InvalidFormat(
                "Header too short".to_string(),
            ));
        }

        let mut magic = [0u8; 5];
        magic.copy_from_slice(&data[0..5]);

        if &magic != MAGIC {
            return Err(OxideFileError::InvalidMagic);
        }

        let version = u32::from_le_bytes(data[5..9].try_into().unwrap());
        if version != VERSION {
            return Err(OxideFileError::UnsupportedVersion(version));
        }

        let flags = u32::from_le_bytes(data[9..13].try_into().unwrap());
        let metadata_length = u32::from_le_bytes(data[13..17].try_into().unwrap());
        let encrypted_data_length = u32::from_le_bytes(data[17..21].try_into().unwrap());

        Ok(Self {
            magic,
            version,
            flags,
            metadata_length,
            encrypted_data_length,
        })
    }
}

/// Metadata section (unencrypted, JSON)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OxideMetadata {
    pub exported_at: DateTime<Utc>,
    pub exported_by: String,
    pub description: Option<String>,
    pub num_connections: usize,
    pub connection_names: Vec<String>,
}

/// Encrypted payload structure
#[derive(Debug, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub version: u32,
    pub connections: Vec<EncryptedConnection>,
    pub checksum: String,
}

/// Connection data stored in encrypted payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedConnection {
    pub name: String,
    pub group: Option<String>,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: EncryptedAuth,
    pub color: Option<String>,
    pub tags: Vec<String>,
    pub options: ConnectionOptions,
    /// Proxy chain for multi-hop connections (intermediate jump hosts)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<EncryptedProxyHop>,
}

/// Encrypted proxy hop for multi-hop connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedProxyHop {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: EncryptedAuth,
}

/// Authentication data (stored encrypted)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EncryptedAuth {
    Password {
        password: String,
    },
    Key {
        /// Original path to the key file (for reference)
        key_path: String,
        passphrase: Option<String>,
        /// Embedded private key content (base64 encoded) for portable backups
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedded_key: Option<String>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        passphrase: Option<String>,
        /// Embedded private key content (base64 encoded)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedded_key: Option<String>,
        /// Embedded certificate content (base64 encoded)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedded_cert: Option<String>,
    },
    Agent,
}

/// Complete .oxide file structure
#[derive(Debug)]
pub struct OxideFile {
    pub metadata: OxideMetadata,
    pub salt: [u8; SALT_LEN],
    pub nonce: [u8; NONCE_LEN],
    pub encrypted_data: Vec<u8>,
    pub tag: [u8; TAG_LEN],
    /// KDF version used for key derivation (extracted from header flags)
    pub kdf_version: u32,
}

impl OxideFile {
    /// Serialize the entire file to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, OxideFileError> {
        // Serialize metadata to JSON
        let metadata_json = serde_json::to_vec(&self.metadata)?;
        let metadata_len = metadata_json.len() as u32;
        let encrypted_len = self.encrypted_data.len() as u32;

        // Build header
        let header = FileHeader::new(metadata_len, encrypted_len);
        let header_bytes = header.to_bytes();

        // Combine all parts
        let total_len =
            21 + SALT_LEN + NONCE_LEN + metadata_json.len() + self.encrypted_data.len() + TAG_LEN;
        let mut bytes = Vec::with_capacity(total_len);

        bytes.extend_from_slice(&header_bytes);
        bytes.extend_from_slice(&self.salt);
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&metadata_json);
        bytes.extend_from_slice(&self.encrypted_data);
        bytes.extend_from_slice(&self.tag);

        Ok(bytes)
    }

    /// Parse the file from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, OxideFileError> {
        let mut cursor = Cursor::new(data);

        // Read header (21 bytes)
        let mut header_bytes = [0u8; 21];
        cursor
            .read_exact(&mut header_bytes)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read header".to_string()))?;

        let header = FileHeader::from_bytes(&header_bytes)?;

        // Read salt (32 bytes)
        let mut salt = [0u8; SALT_LEN];
        cursor
            .read_exact(&mut salt)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read salt".to_string()))?;

        // Read nonce (12 bytes)
        let mut nonce = [0u8; NONCE_LEN];
        cursor
            .read_exact(&mut nonce)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read nonce".to_string()))?;

        // Read metadata (JSON)
        let mut metadata_bytes = vec![0u8; header.metadata_length as usize];
        cursor
            .read_exact(&mut metadata_bytes)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read metadata".to_string()))?;

        let metadata: OxideMetadata = serde_json::from_slice(&metadata_bytes)?;

        // Read encrypted data
        let mut encrypted_data = vec![0u8; header.encrypted_data_length as usize];
        cursor.read_exact(&mut encrypted_data).map_err(|_| {
            OxideFileError::InvalidFormat("Failed to read encrypted data".to_string())
        })?;

        // Read authentication tag (16 bytes)
        let mut tag = [0u8; TAG_LEN];
        cursor
            .read_exact(&mut tag)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read tag".to_string()))?;

        Ok(Self {
            metadata,
            salt,
            nonce,
            encrypted_data,
            tag,
            kdf_version: header.kdf_version(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_roundtrip() {
        let header = FileHeader::new(1234, 5678);
        let bytes = header.to_bytes();
        let parsed = FileHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed.magic, *MAGIC);
        assert_eq!(parsed.version, VERSION);
        assert_eq!(parsed.metadata_length, 1234);
        assert_eq!(parsed.encrypted_data_length, 5678);
    }

    #[test]
    fn test_invalid_magic() {
        let mut bytes = vec![0u8; 21]; // Header needs at least 21 bytes
        bytes[0..5].copy_from_slice(b"WRONG");

        let result = FileHeader::from_bytes(&bytes);
        assert!(matches!(result, Err(OxideFileError::InvalidMagic)));
    }
}
