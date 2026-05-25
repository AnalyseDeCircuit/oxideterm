// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde::{Deserialize, Serialize};

use crate::error::PluginError;

pub const NATIVE_PLUGIN_PROTOCOL_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginProtocolEnvelope<T> {
    pub protocol_version: u32,
    pub request_id: Option<String>,
    pub payload: T,
}

impl<T> PluginProtocolEnvelope<T> {
    pub fn new(request_id: Option<String>, payload: T) -> Self {
        Self {
            protocol_version: NATIVE_PLUGIN_PROTOCOL_VERSION,
            request_id,
            payload,
        }
    }

    pub fn validate_version(&self) -> Result<(), PluginError> {
        validate_protocol_version(self.protocol_version)
    }
}

pub fn validate_protocol_version(protocol_version: u32) -> Result<(), PluginError> {
    if protocol_version == NATIVE_PLUGIN_PROTOCOL_VERSION {
        return Ok(());
    }
    Err(PluginError::protocol(
        "unsupported_protocol_version",
        format!(
            "Unsupported native plugin protocol version {protocol_version}; expected {NATIVE_PLUGIN_PROTOCOL_VERSION}"
        ),
    ))
}
