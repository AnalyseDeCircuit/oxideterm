// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde_json::Value;
use zeroize::Zeroize;

/// Classifies host calls whose arguments may contain plaintext credentials.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginHostCallSensitivity {
    Public,
    Sensitive,
}

impl PluginHostCallSensitivity {
    pub fn classify(namespace: &str, method: &str) -> Self {
        match (namespace, method) {
            ("secrets", _) | ("sync", "exportOxide" | "previewImport" | "importOxide") => {
                Self::Sensitive
            }
            _ => Self::Public,
        }
    }

    pub fn is_sensitive(self) -> bool {
        matches!(self, Self::Sensitive)
    }
}

pub fn zeroize_json_value(value: &mut Value) {
    match value {
        Value::String(secret) => secret.zeroize(),
        Value::Array(values) => {
            for value in values {
                zeroize_json_value(value);
            }
        }
        Value::Object(values) => {
            // Take the map so even a malicious dynamic key is cleared before
            // the temporary JSON allocation is released.
            for (mut key, mut value) in std::mem::take(values) {
                key.zeroize();
                zeroize_json_value(&mut value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    *value = Value::Null;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_all_credential_bearing_host_calls() {
        for (namespace, method) in [
            ("secrets", "get"),
            ("secrets", "set"),
            ("sync", "exportOxide"),
            ("sync", "previewImport"),
            ("sync", "importOxide"),
        ] {
            assert!(
                PluginHostCallSensitivity::classify(namespace, method).is_sensitive(),
                "{namespace}.{method} must use sensitive transport ownership"
            );
        }
        assert_eq!(
            PluginHostCallSensitivity::classify("sync", "validateOxide"),
            PluginHostCallSensitivity::Public
        );
    }
}
