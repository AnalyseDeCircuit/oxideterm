// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use serde_json::{Map, Value, json};
use zeroize::Zeroizing;

use crate::workspace::plugin_runtime;

// Secrets are scoped by plugin id before they touch the shared key store, so a
// plugin can never address another plugin's persisted account id by raw key.
pub(super) fn native_plugin_secret_response(
    plugin_id: &str,
    call: plugin_runtime::PluginHostCall,
    key_store: &oxideterm_ai::AiProviderKeyStore,
) -> plugin_runtime::PluginResponse {
    let request_id = call.request_id.clone();
    match native_plugin_secret_result(plugin_id, &call.method, &call.args, key_store) {
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
    args: &Value,
    key_store: &oxideterm_ai::AiProviderKeyStore,
) -> Result<Value, String> {
    match method {
        "get" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id = native_plugin_secret_account_id(plugin_id, key)?;
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
                account_ids.push(native_plugin_secret_account_id(plugin_id, key)?);
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
            let value = args
                .get("value")
                .and_then(Value::as_str)
                .ok_or_else(|| "secrets.set requires args.value".to_string())?;
            let account_id = native_plugin_secret_account_id(plugin_id, key)?;
            // The JSON bridge gives us a borrowed string; wrap the owned copy at
            // the keychain boundary so the temporary is zeroized after storage,
            // matching Tauri's rule that plugin secrets live only in keychain
            // and the runtime response.
            key_store
                .store_provider_key(&account_id, Zeroizing::new(value.to_string()))
                .map_err(|error| {
                    if value.is_empty() {
                        format!("Failed to delete plugin secret: {error}")
                    } else {
                        format!("Failed to save plugin secret: {error}")
                    }
                })?;
            Ok(Value::Null)
        }
        "has" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id = native_plugin_secret_account_id(plugin_id, key)?;
            Ok(json!(key_store.has_provider_key(&account_id)))
        }
        "delete" => {
            let key = native_plugin_secret_key_arg(args)?;
            let account_id = native_plugin_secret_account_id(plugin_id, key)?;
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

pub(super) fn native_plugin_secret_account_id(
    plugin_id: &str,
    key: &str,
) -> Result<String, String> {
    native_plugin_validate_secret_plugin_id(plugin_id)?;
    native_plugin_validate_secret_key(key)?;
    Ok(format!(
        "plugin-secret:{}:{}:{}:{}",
        plugin_id.len(),
        plugin_id,
        key.len(),
        key
    ))
}

fn native_plugin_validate_secret_plugin_id(plugin_id: &str) -> Result<(), String> {
    if plugin_id.is_empty() {
        return Err("Plugin ID cannot be empty".to_string());
    }
    if plugin_id.contains('/') || plugin_id.contains('\\') || plugin_id.contains("..") {
        return Err("Plugin ID contains invalid path characters".to_string());
    }
    if plugin_id.bytes().any(|byte| byte < 0x20) {
        return Err("Plugin ID contains invalid characters".to_string());
    }
    Ok(())
}

fn native_plugin_validate_secret_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("Plugin secret key cannot be empty".to_string());
    }
    if key.bytes().any(|byte| byte < 0x20) {
        return Err("Plugin secret key contains invalid characters".to_string());
    }
    Ok(())
}
