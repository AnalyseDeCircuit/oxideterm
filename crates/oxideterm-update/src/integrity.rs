// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};

use crate::NativeUpdateError;

pub const OXIDETERM_UPDATER_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDM2RTE5RDY5OTJCNTdFQkIKUldTN2ZyV1NhWjNoTnJFZ3p6T2s0WEtNaTVTWUhpUW1LdnRjTlpEaGZsTTAzaTJOSll1bVhPem4K";

pub fn verify_minisign_signature(
    data: &[u8],
    release_signature: &str,
) -> Result<(), NativeUpdateError> {
    let pub_key_decoded = base64_to_string(OXIDETERM_UPDATER_PUBKEY)?;
    let public_key = PublicKey::decode(&pub_key_decoded).map_err(|error| {
        NativeUpdateError::Integrity(format!("decode public key failed: {error}"))
    })?;
    let signature_decoded = base64_to_string(release_signature)?;
    let signature = Signature::decode(&signature_decoded).map_err(|error| {
        NativeUpdateError::Integrity(format!("decode release signature failed: {error}"))
    })?;
    public_key.verify(data, &signature, true).map_err(|error| {
        NativeUpdateError::Integrity(format!("signature verification failed: {error}"))
    })?;
    Ok(())
}

fn base64_to_string(value: &str) -> Result<String, NativeUpdateError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(value)
        .map_err(|error| NativeUpdateError::Integrity(format!("base64 decode failed: {error}")))?;
    std::str::from_utf8(&decoded)
        .map(str::to_string)
        .map_err(|_| NativeUpdateError::Integrity("invalid utf8 in signature".to_string()))
}
