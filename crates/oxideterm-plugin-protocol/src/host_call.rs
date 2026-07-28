// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::sensitive::{PluginHostCallSensitivity, zeroize_json_value};

#[derive(PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginHostCall {
    pub request_id: String,
    pub namespace: String,
    pub method: String,
    #[serde(default)]
    pub args: Value,
}

impl PluginHostCall {
    pub fn sensitivity(&self) -> PluginHostCallSensitivity {
        PluginHostCallSensitivity::classify(&self.namespace, &self.method)
    }

    pub fn zeroize_args(&mut self) {
        // Sensitive handlers call this at their ownership boundary after moving
        // any value needed by the backend out of the JSON object.
        zeroize_json_value(&mut self.args);
    }
}

impl fmt::Debug for PluginHostCall {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("PluginHostCall");
        debug
            .field("request_id", &self.request_id)
            .field("namespace", &self.namespace)
            .field("method", &self.method);
        if self.sensitivity().is_sensitive() {
            debug.field("args", &"<redacted>");
        } else {
            debug.field("args", &self.args);
        }
        debug.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_host_call_debug_redacts_arguments() {
        let call = PluginHostCall {
            request_id: "secret-1".to_string(),
            namespace: "secrets".to_string(),
            method: "set".to_string(),
            args: serde_json::json!({ "key": "token", "value": "sensitive-value" }),
        };

        let rendered = format!("{call:?}");

        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("sensitive-value"));
    }

    #[test]
    fn sync_password_host_call_debug_redacts_arguments() {
        let call = PluginHostCall {
            request_id: "sync-1".to_string(),
            namespace: "sync".to_string(),
            method: "importOxide".to_string(),
            args: serde_json::json!({ "password": "sensitive-value" }),
        };

        let rendered = format!("{call:?}");

        assert!(rendered.contains("<redacted>"));
        assert!(!rendered.contains("sensitive-value"));
    }

    #[test]
    fn zeroize_args_clears_nested_secret_strings() {
        let mut call = PluginHostCall {
            request_id: "secret-2".to_string(),
            namespace: "secrets".to_string(),
            method: "set".to_string(),
            args: serde_json::json!({
                "value": "sensitive-value",
                "nested": ["another-sensitive-value"],
            }),
        };

        call.zeroize_args();

        assert!(call.args.is_null());
    }
}
