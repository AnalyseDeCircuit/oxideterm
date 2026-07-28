// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use serde_json::Value;
use zeroize::Zeroize;

pub(crate) fn zeroize_json_value(value: &mut Value) {
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
