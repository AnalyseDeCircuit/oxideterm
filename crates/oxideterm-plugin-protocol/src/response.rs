// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{error::PluginError, sensitive::zeroize_json_value};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginResponse {
    pub request_id: String,
    pub result: PluginResponseResult,
    #[serde(skip)]
    sensitive: bool,
}

impl PluginResponse {
    pub fn ok(request_id: impl Into<String>, value: Value) -> Self {
        Self {
            request_id: request_id.into(),
            result: PluginResponseResult::Ok { value },
            sensitive: false,
        }
    }

    pub fn sensitive_ok(request_id: impl Into<String>, value: Value) -> Self {
        // Sensitivity is deliberately out-of-band so the public protocol JSON
        // remains compatible with existing process plugins.
        Self {
            request_id: request_id.into(),
            result: PluginResponseResult::Ok { value },
            sensitive: true,
        }
    }

    pub fn error(request_id: impl Into<String>, error: PluginError) -> Self {
        Self {
            request_id: request_id.into(),
            result: PluginResponseResult::Error { error },
            sensitive: false,
        }
    }

    pub fn zeroize_sensitive_payload(&mut self) {
        // The process transport invokes this from its response-owner `Drop`,
        // including serialization and IO error paths.
        if !self.sensitive {
            return;
        }
        if let PluginResponseResult::Ok { value } = &mut self.result {
            zeroize_json_value(value);
        }
        self.sensitive = false;
    }
}

impl fmt::Debug for PluginResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PluginResponse")
            .field("request_id", &self.request_id)
            .field("result", &self.result)
            .finish()
    }
}

impl PartialEq for PluginResponse {
    fn eq(&self, other: &Self) -> bool {
        // Sensitivity is an in-process lifetime marker, not protocol data.
        self.request_id == other.request_id && self.result == other.result
    }
}

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PluginResponseResult {
    Ok { value: Value },
    Error { error: PluginError },
}

impl fmt::Debug for PluginResponseResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ok { .. } => formatter
                .debug_struct("Ok")
                .field("value", &"<redacted>")
                .finish(),
            Self::Error { error } => formatter
                .debug_struct("Error")
                .field("error", error)
                .finish(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn successful_response_debug_never_prints_payload() {
        let response =
            PluginResponse::sensitive_ok("secret-1", serde_json::json!("sensitive-value"));

        let rendered = format!("{response:?}");

        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("sensitive-value"));
    }

    #[test]
    fn sensitive_response_payload_can_be_zeroized_after_transport() {
        let mut response = PluginResponse::sensitive_ok(
            "secret-2",
            serde_json::json!({ "token": "sensitive-value" }),
        );

        response.zeroize_sensitive_payload();

        assert_eq!(
            response.result,
            PluginResponseResult::Ok { value: Value::Null }
        );
    }

    #[test]
    fn sensitive_response_preserves_public_wire_shape() {
        let mut response =
            PluginResponse::sensitive_ok("secret-3", serde_json::json!("sensitive-value"));

        let mut encoded = serde_json::to_value(&response).unwrap();

        assert_eq!(encoded["requestId"], "secret-3");
        assert_eq!(encoded["result"]["status"], "ok");
        assert_eq!(encoded["result"]["value"], "sensitive-value");
        response.zeroize_sensitive_payload();
        zeroize_json_value(&mut encoded);
    }
}
