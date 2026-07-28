// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use crate::workspace::plugin_runtime;

struct SecretHostCallOwner(plugin_runtime::PluginHostCall);

impl Drop for SecretHostCallOwner {
    fn drop(&mut self) {
        // The protocol DTO cannot zeroize every JSON argument because most host
        // APIs carry ordinary data. This owner is the secret-specific drop
        // boundary for the one moved call received from the process transport.
        self.0.zeroize_args();
    }
}

// Secrets are scoped by plugin id before they touch the shared key store, so a
// plugin can never address another plugin's persisted account id by raw key.
pub(super) fn native_plugin_secret_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    key_store: &oxideterm_ai::AiProviderKeyStore,
) -> plugin_runtime::PluginResponse {
    let mut call = SecretHostCallOwner(call);
    let request_id = std::mem::take(&mut call.0.request_id);
    let method = std::mem::take(&mut call.0.method);
    let returns_secret = matches!(method.as_str(), "get" | "getMany");
    match native_plugin_secret_result(plugin_id, &method, &mut call.0.args, key_store) {
        Ok(value) if returns_secret => {
            plugin_runtime::PluginResponse::sensitive_ok(request_id, value)
        }
        Ok(value) => plugin_runtime::PluginResponse::ok(request_id, value),
        Err(error) => plugin_runtime::PluginResponse::error(
            request_id,
            plugin_runtime::PluginError::runtime("plugin_secret_error", error),
        ),
    }
}

fn native_plugin_secret_result(
    plugin_id: &str,
    method: &str,
    args: &mut Value,
    key_store: &oxideterm_ai::AiProviderKeyStore,
) -> Result<Value, String> {
    match method {
        "get" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id =
                oxideterm_plugin_host_api::secrets::plugin_secret_account_id(plugin_id, key)?;
            let secret = key_store
                .get_provider_key(&account_id)
                .map_err(|error| format!("Failed to read plugin secret: {error}"))?;
            Ok(secret
                .map(|secret| json!(secret.as_str()))
                .unwrap_or(Value::Null))
        }
        "getMany" => {
            let keys = native_plugin_secret_keys_arg(args)?;
            let mut account_ids = Vec::with_capacity(keys.len());
            for key in &keys {
                account_ids.push(
                    oxideterm_plugin_host_api::secrets::plugin_secret_account_id(plugin_id, key)?,
                );
            }
            let secrets = key_store
                .get_provider_keys(&account_ids)
                .map_err(|error| format!("Failed to read plugin secrets: {error}"))?;
            let secret_by_account = secrets.into_iter().collect::<HashMap<_, _>>();
            let mut values = Map::new();
            for (key, account_id) in keys.iter().zip(account_ids.iter()) {
                let value = secret_by_account
                    .get(account_id)
                    .map(|secret| json!(secret.as_str()))
                    .unwrap_or(Value::Null);
                values.insert(key.clone(), value);
            }
            Ok(Value::Object(values))
        }
        "set" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id =
                oxideterm_plugin_host_api::secrets::plugin_secret_account_id(plugin_id, key)?;
            let value = take_native_plugin_secret_value(args)?;
            let deletes_secret = value.is_empty();
            // Move the JSON string into the zeroizing owner instead of cloning
            // it for the keychain handoff.
            key_store
                .store_provider_key(&account_id, value)
                .map_err(|error| {
                    if deletes_secret {
                        format!("Failed to delete plugin secret: {error}")
                    } else {
                        format!("Failed to save plugin secret: {error}")
                    }
                })?;
            Ok(Value::Null)
        }
        "has" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id =
                oxideterm_plugin_host_api::secrets::plugin_secret_account_id(plugin_id, key)?;
            Ok(json!(key_store.has_provider_key(&account_id)))
        }
        "delete" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id =
                oxideterm_plugin_host_api::secrets::plugin_secret_account_id(plugin_id, key)?;
            key_store
                .delete_provider_key(&account_id)
                .map_err(|error| format!("Failed to delete plugin secret: {error}"))?;
            Ok(Value::Null)
        }
        method => Err(format!("Unsupported secrets host call: {method}")),
    }
}

fn native_plugin_secret_key_arg(args: &Value) -> Result<&str, String> {
    args.get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| "secrets host call requires args.key".to_string())
}

fn native_plugin_secret_keys_arg(args: &Value) -> Result<Vec<String>, String> {
    let keys = args
        .get("keys")
        .and_then(Value::as_array)
        .ok_or_else(|| "secrets.getMany requires args.keys".to_string())?;
    keys.iter()
        .map(|key| {
            key.as_str()
                .map(str::to_string)
                .ok_or_else(|| "secrets.getMany keys must be strings".to_string())
        })
        .collect()
}

fn take_native_plugin_secret_value(args: &mut Value) -> Result<Zeroizing<String>, String> {
    let value = args
        .get_mut("value")
        .ok_or_else(|| "secrets.set requires args.value".to_string())?;
    let owned_value = std::mem::take(value);
    match owned_value {
        Value::String(secret) => Ok(Zeroizing::new(secret)),
        other => {
            // Restore invalid input so the call owner still clears any nested
            // strings when it leaves the secret boundary.
            *value = other;
            Err("secrets.set requires args.value".to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_set_moves_value_out_of_json_owner() {
        let mut call = plugin_runtime::PluginHostCall {
            request_id: "secret-1".to_string(),
            namespace: "secrets".to_string(),
            method: "set".to_string(),
            args: serde_json::json!({
                "key": "token",
                "value": "sensitive-value",
            }),
        };

        let secret = take_native_plugin_secret_value(&mut call.args).unwrap();

        assert_eq!(secret.as_str(), "sensitive-value");
        assert!(call.args["value"].is_null());
        call.zeroize_args();
    }

    #[test]
    fn invalid_secret_set_value_remains_owned_for_zeroization() {
        let mut call = plugin_runtime::PluginHostCall {
            request_id: "secret-2".to_string(),
            namespace: "secrets".to_string(),
            method: "set".to_string(),
            args: serde_json::json!({
                "key": "token",
                "value": { "nested": "sensitive-value" },
            }),
        };

        assert!(take_native_plugin_secret_value(&mut call.args).is_err());
        assert_eq!(call.args["value"]["nested"], "sensitive-value");
        call.zeroize_args();
        assert!(call.args.is_null());
    }
}
