use std::io::{Cursor, Read};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::ConnectionOptions;

use super::OxideFileError;

pub const MAGIC: &[u8; 5] = b"OXIDE";
pub const VERSION: u32 = 1;
pub const SALT_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;
pub const TAG_LEN: usize = 16;

pub mod kdf_flags {
    pub const KDF_V1: u32 = 0x0001;
    pub const KDF_V2: u32 = 0x0002;
    pub const KDF_VERSION_MASK: u32 = 0x00FF;
    pub const CURRENT_KDF: u32 = KDF_V1;
}

#[derive(Debug, Clone, Eq, PartialEq)]
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
            flags: kdf_flags::CURRENT_KDF,
            metadata_length,
            encrypted_data_length,
        }
    }

    pub fn kdf_version(&self) -> u32 {
        self.flags & kdf_flags::KDF_VERSION_MASK
    }

    pub fn to_bytes(&self) -> [u8; 21] {
        let mut bytes = [0u8; 21];
        bytes[0..5].copy_from_slice(&self.magic);
        bytes[5..9].copy_from_slice(&self.version.to_le_bytes());
        bytes[9..13].copy_from_slice(&self.flags.to_le_bytes());
        bytes[13..17].copy_from_slice(&self.metadata_length.to_le_bytes());
        bytes[17..21].copy_from_slice(&self.encrypted_data_length.to_le_bytes());
        bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, OxideFileError> {
        if data.len() < 21 {
            return Err(OxideFileError::InvalidFormat("Header too short".into()));
        }

        let mut magic = [0u8; 5];
        magic.copy_from_slice(&data[0..5]);
        if &magic != MAGIC {
            return Err(OxideFileError::InvalidMagic);
        }

        let version = u32::from_le_bytes(
            data[5..9]
                .try_into()
                .map_err(|_| OxideFileError::InvalidFormat("Failed to read version".into()))?,
        );
        if version != VERSION {
            return Err(OxideFileError::UnsupportedVersion(version));
        }

        Ok(Self {
            magic,
            version,
            flags: u32::from_le_bytes(
                data[9..13]
                    .try_into()
                    .map_err(|_| OxideFileError::InvalidFormat("Failed to read flags".into()))?,
            ),
            metadata_length: u32::from_le_bytes(data[13..17].try_into().map_err(|_| {
                OxideFileError::InvalidFormat("Failed to read metadata length".into())
            })?),
            encrypted_data_length: u32::from_le_bytes(data[17..21].try_into().map_err(|_| {
                OxideFileError::InvalidFormat("Failed to read encrypted data length".into())
            })?),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OxideMetadata {
    pub exported_at: DateTime<Utc>,
    pub exported_by: String,
    pub description: Option<String>,
    pub num_connections: usize,
    pub connection_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_app_settings: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_quick_commands: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick_commands_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick_command_categories_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin_settings_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub portable_secret_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedPayload {
    pub version: u32,
    pub connections: Vec<EncryptedConnection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_settings_json: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quick_commands_json: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub plugin_settings: Vec<EncryptedPluginSetting>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub portable_secrets: Vec<EncryptedPortableSecret>,
    pub checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedPluginSetting {
    pub storage_key: String,
    pub serialized_value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedPortableSecret {
    pub kind: String,
    pub id: String,
    pub secret: Zeroizing<String>,
}

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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<EncryptedProxyHop>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub forwards: Vec<EncryptedForward>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedForward {
    pub forward_type: String,
    pub bind_address: String,
    pub bind_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub description: Option<String>,
    pub auto_start: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedProxyHop {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: EncryptedAuth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EncryptedAuth {
    Password {
        password: Zeroizing<String>,
    },
    Key {
        key_path: String,
        passphrase: Option<Zeroizing<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedded_key: Option<Zeroizing<String>>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        passphrase: Option<Zeroizing<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedded_key: Option<Zeroizing<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        embedded_cert: Option<Zeroizing<String>>,
    },
    Agent,
}

#[derive(Debug)]
pub struct OxideFile {
    pub metadata: OxideMetadata,
    pub salt: [u8; SALT_LEN],
    pub nonce: [u8; NONCE_LEN],
    pub encrypted_data: Vec<u8>,
    pub tag: [u8; TAG_LEN],
    pub kdf_version: u32,
}

impl OxideFile {
    pub fn to_bytes(&self) -> Result<Vec<u8>, OxideFileError> {
        let metadata_json = serde_json::to_vec(&self.metadata)?;
        let header = FileHeader::new(metadata_json.len() as u32, self.encrypted_data.len() as u32);

        let mut bytes = Vec::with_capacity(
            21 + SALT_LEN + NONCE_LEN + metadata_json.len() + self.encrypted_data.len() + TAG_LEN,
        );
        bytes.extend_from_slice(&header.to_bytes());
        bytes.extend_from_slice(&self.salt);
        bytes.extend_from_slice(&self.nonce);
        bytes.extend_from_slice(&metadata_json);
        bytes.extend_from_slice(&self.encrypted_data);
        bytes.extend_from_slice(&self.tag);
        Ok(bytes)
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, OxideFileError> {
        let mut cursor = Cursor::new(data);
        let mut header_bytes = [0u8; 21];
        cursor
            .read_exact(&mut header_bytes)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read header".into()))?;
        let header = FileHeader::from_bytes(&header_bytes)?;

        let expected_len = 21usize
            .saturating_add(SALT_LEN)
            .saturating_add(NONCE_LEN)
            .saturating_add(header.metadata_length as usize)
            .saturating_add(header.encrypted_data_length as usize)
            .saturating_add(TAG_LEN);
        if data.len() < expected_len {
            return Err(OxideFileError::InvalidFormat(
                "File is shorter than header lengths".into(),
            ));
        }

        let mut salt = [0u8; SALT_LEN];
        cursor
            .read_exact(&mut salt)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read salt".into()))?;
        let mut nonce = [0u8; NONCE_LEN];
        cursor
            .read_exact(&mut nonce)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read nonce".into()))?;

        let mut metadata_bytes = vec![0u8; header.metadata_length as usize];
        cursor
            .read_exact(&mut metadata_bytes)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read metadata".into()))?;
        let metadata = serde_json::from_slice(&metadata_bytes)?;

        let mut encrypted_data = vec![0u8; header.encrypted_data_length as usize];
        cursor
            .read_exact(&mut encrypted_data)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read encrypted data".into()))?;
        let mut tag = [0u8; TAG_LEN];
        cursor
            .read_exact(&mut tag)
            .map_err(|_| OxideFileError::InvalidFormat("Failed to read tag".into()))?;

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
    fn file_header_matches_tauri_binary_layout() {
        let header = FileHeader::new(1234, 5678);
        let bytes = header.to_bytes();
        let parsed = FileHeader::from_bytes(&bytes).unwrap();

        assert_eq!(bytes.len(), 21);
        assert_eq!(parsed.magic, *MAGIC);
        assert_eq!(parsed.version, VERSION);
        assert_eq!(parsed.flags, kdf_flags::CURRENT_KDF);
        assert_eq!(parsed.metadata_length, 1234);
        assert_eq!(parsed.encrypted_data_length, 5678);
    }
}
