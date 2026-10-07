// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};
use zeroize::Zeroizing;

pub const SECURITY_KEY_PROTOCOL_VERSION: u32 = 1;
pub const SECURITY_KEY_MAX_FRAME_BYTES: usize = 64 * 1024;
pub const SECURITY_KEY_PLUGIN_ID: &str = "com.oxideterm.auth.fido2";

// Do not derive Debug: handles and PINs are credential material, including in failures.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SecurityKeyRequest {
    Sign {
        algorithm: String,
        application: String,
        key_handle: Zeroizing<Vec<u8>>,
        flags: u8,
        challenge: Vec<u8>,
    },
    Pin {
        value: Zeroizing<String>,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum SecurityKeyResponse {
    Ready {
        protocol_version: u32,
    },
    Touch,
    PinRequired {
        retry: bool,
    },
    Signature {
        signature: Vec<u8>,
        flags: u8,
        counter: u32,
    },
    Failure {
        code: SecurityKeyError,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SecurityKeyError {
    Unavailable,
    Cancelled,
    Timeout,
    InvalidRequest,
    Incompatible,
    NoDevice,
    CredentialNotFound,
    PinBlocked,
    DeviceFailure,
    InvalidSignature,
}

impl std::fmt::Display for SecurityKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Security-key plugin is unavailable",
            Self::Cancelled => "Security-key authentication cancelled",
            Self::Timeout => "Security-key authentication timed out",
            Self::InvalidRequest => "Invalid security-key signing request",
            Self::Incompatible => "Unsupported security-key plugin protocol",
            Self::NoDevice => "No FIDO security key found",
            Self::CredentialNotFound => "SSH credential was not found on the security key",
            Self::PinBlocked => "Security-key PIN is blocked",
            Self::DeviceFailure => "Security-key operation failed",
            Self::InvalidSignature => "Invalid security-key signature",
        })
    }
}
impl std::error::Error for SecurityKeyError {}

impl SecurityKeyError {
    pub fn message_key(self) -> &'static str {
        match self {
            Self::Unavailable => "ssh.fido.errors.unavailable",
            Self::Cancelled => "ssh.fido.errors.cancelled",
            Self::Timeout => "ssh.fido.errors.timeout",
            Self::InvalidRequest => "ssh.fido.errors.invalid_request",
            Self::Incompatible => "ssh.fido.errors.incompatible",
            Self::NoDevice => "ssh.fido.errors.no_device",
            Self::CredentialNotFound => "ssh.fido.errors.credential_not_found",
            Self::PinBlocked => "ssh.fido.errors.pin_blocked",
            Self::DeviceFailure => "ssh.fido.errors.device_failure",
            Self::InvalidSignature => "ssh.fido.errors.invalid_signature",
        }
    }
}

pub fn encode_message(value: &impl Serialize) -> io::Result<Zeroizing<Vec<u8>>> {
    let bytes = Zeroizing::new(
        serde_json::to_vec(value).map_err(|_| io::Error::other("Invalid signing message"))?,
    );
    if bytes.len() > SECURITY_KEY_MAX_FRAME_BYTES {
        return Err(io::Error::other("Signing message exceeds size limit"));
    }
    Ok(bytes)
}

pub fn read_message<T: serde::de::DeserializeOwned>(reader: &mut impl Read) -> io::Result<T> {
    let mut prefix = [0; 4];
    reader.read_exact(&mut prefix)?;
    let length = u32::from_be_bytes(prefix) as usize;
    if length == 0 || length > SECURITY_KEY_MAX_FRAME_BYTES {
        return Err(io::Error::other("Invalid signing message size"));
    }
    let mut bytes = Zeroizing::new(vec![0; length]);
    reader.read_exact(&mut bytes)?;
    serde_json::from_slice(&bytes).map_err(|_| io::Error::other("Invalid signing message"))
}

pub fn write_message(writer: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    let bytes = encode_message(value)?;
    writer.write_all(&(bytes.len() as u32).to_be_bytes())?;
    writer.write_all(&bytes)?;
    writer.flush()
}
