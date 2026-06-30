// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    fs::File,
    io::{self, Read},
};

use oxideterm_ai::AiProviderKeyStore;
use oxideterm_cloud_sync::{secret_keys, secrets::CloudSyncKeychainSecretProvider};
use oxideterm_connections::{ConnectionStore, SaveConnectionRequest, SavedAuth, SecretString};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    args::{
        SecretScopeArg, SecretsAction, SecretsClearArgs, SecretsCommand, SecretsImportArgs,
        SecretsSetArgs, SecretsStatusArgs,
    },
    cloud_sync_preview,
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::{default_cloud_sync_path, default_connections_path},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretStatus {
    scope: &'static str,
    id: Option<String>,
    key: Option<String>,
    configured: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretsStatusResponse {
    count: usize,
    secrets: Vec<SecretStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SecretsWriteResponse {
    scope: &'static str,
    id: Option<String>,
    key: Option<String>,
    imported: usize,
    cleared: bool,
    configured: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecretImportDocument {
    secrets: Vec<SecretImportEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SecretImportEntry {
    scope: SecretScopeArg,
    id: Option<String>,
    plugin_id: Option<String>,
    key: Option<String>,
    value: Option<Zeroizing<String>>,
    env: Option<String>,
}

pub fn run(command: SecretsCommand) -> CliResult<i32> {
    match command.action {
        SecretsAction::Status(args) => {
            status(args)?;
            Ok(0)
        }
        SecretsAction::Set(args) => {
            set(args)?;
            Ok(0)
        }
        SecretsAction::Clear(args) => {
            clear(args)?;
            Ok(0)
        }
        SecretsAction::Import(args) => {
            import(args)?;
            Ok(0)
        }
    }
}

fn status(args: SecretsStatusArgs) -> CliResult<()> {
    let mut statuses = Vec::new();
    match args.scope {
        Some(SecretScopeArg::Ai) => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            let store = AiProviderKeyStore::new();
            statuses.push(SecretStatus {
                scope: "ai",
                id: Some(id.clone()),
                key: None,
                configured: store.has_provider_key(&id),
            });
        }
        Some(SecretScopeArg::Plugin) => {
            let plugin_id = required_arg(args.plugin_id.as_deref(), "--plugin-id", args.json)?;
            let key = required_arg(args.key.as_deref(), "--key", args.json)?;
            let account_id = plugin_secret_account_id(&plugin_id, &key, args.json)?;
            let store = AiProviderKeyStore::new();
            statuses.push(SecretStatus {
                scope: "plugin",
                id: Some(plugin_id),
                key: Some(key),
                configured: store.has_provider_key(&account_id),
            });
        }
        Some(SecretScopeArg::CloudSync) => {
            statuses.extend(cloud_sync_statuses(args.key.as_deref(), args.json)?);
        }
        Some(SecretScopeArg::Connection) => {
            statuses.extend(connection_statuses(args.id.as_deref(), args.json)?);
        }
        Some(SecretScopeArg::Portable) => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            let store = AiProviderKeyStore::new();
            statuses.push(SecretStatus {
                scope: "portable",
                id: Some(id.clone()),
                key: Some(args.key.unwrap_or_else(|| "ai_provider_key".to_string())),
                configured: store.has_provider_key(&id),
            });
        }
        None => {
            statuses.extend(cloud_sync_statuses(None, args.json)?);
            statuses.extend(connection_statuses(None, args.json)?);
        }
    }
    write_status(args.json, statuses)
}

fn set(args: SecretsSetArgs) -> CliResult<()> {
    let value = read_secret_value(args.stdin, args.env.as_deref(), args.json)?;
    match args.scope {
        SecretScopeArg::Ai => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            // The provider key enters the OS keychain as Zeroizing<String> and is never echoed.
            AiProviderKeyStore::new()
                .store_provider_key(&id, value)
                .map_err(|error| runtime_error(error, args.json))?;
            write_secret_response(args.json, "ai", Some(id), None, false, true)
        }
        SecretScopeArg::Plugin => {
            let plugin_id = required_arg(args.plugin_id.as_deref(), "--plugin-id", args.json)?;
            let key = required_arg(args.key.as_deref(), "--key", args.json)?;
            let account_id = plugin_secret_account_id(&plugin_id, &key, args.json)?;
            AiProviderKeyStore::new()
                .store_provider_key(&account_id, value)
                .map_err(|error| runtime_error(error, args.json))?;
            write_secret_response(args.json, "plugin", Some(plugin_id), Some(key), false, true)
        }
        SecretScopeArg::CloudSync => {
            let key = required_arg(args.key.as_deref(), "--key", args.json)?;
            write_cloud_sync_secret(&key, Some(value.as_str()), args.json)?;
            write_secret_response(args.json, "cloud-sync", None, Some(key), false, true)
        }
        SecretScopeArg::Connection => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            let key = args.key.unwrap_or_else(|| "password".to_string());
            write_connection_secret(&id, &key, Some(value), args.json)?;
            write_secret_response(args.json, "connection", Some(id), Some(key), false, true)
        }
        SecretScopeArg::Portable => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            validate_portable_secret_kind(args.key.as_deref(), args.json)?;
            // Portable AI-provider secrets use the same keychain entry that .oxide export encrypts.
            AiProviderKeyStore::new()
                .store_provider_key(&id, value)
                .map_err(|error| runtime_error(error, args.json))?;
            write_secret_response(
                args.json,
                "portable",
                Some(id),
                Some("ai_provider_key".to_string()),
                false,
                true,
            )
        }
    }
}

fn clear(args: SecretsClearArgs) -> CliResult<()> {
    match args.scope {
        SecretScopeArg::Ai => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            AiProviderKeyStore::new()
                .delete_provider_key(&id)
                .map_err(|error| runtime_error(error, args.json))?;
            write_secret_response(args.json, "ai", Some(id), None, true, false)
        }
        SecretScopeArg::Plugin => {
            let plugin_id = required_arg(args.plugin_id.as_deref(), "--plugin-id", args.json)?;
            let key = required_arg(args.key.as_deref(), "--key", args.json)?;
            let account_id = plugin_secret_account_id(&plugin_id, &key, args.json)?;
            AiProviderKeyStore::new()
                .delete_provider_key(&account_id)
                .map_err(|error| runtime_error(error, args.json))?;
            write_secret_response(args.json, "plugin", Some(plugin_id), Some(key), true, false)
        }
        SecretScopeArg::CloudSync => {
            let key = required_arg(args.key.as_deref(), "--key", args.json)?;
            write_cloud_sync_secret(&key, None, args.json)?;
            write_secret_response(args.json, "cloud-sync", None, Some(key), true, false)
        }
        SecretScopeArg::Connection => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            let key = args.key.unwrap_or_else(|| "password".to_string());
            write_connection_secret(&id, &key, None, args.json)?;
            write_secret_response(args.json, "connection", Some(id), Some(key), true, false)
        }
        SecretScopeArg::Portable => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            validate_portable_secret_kind(args.key.as_deref(), args.json)?;
            AiProviderKeyStore::new()
                .delete_provider_key(&id)
                .map_err(|error| runtime_error(error, args.json))?;
            write_secret_response(
                args.json,
                "portable",
                Some(id),
                Some("ai_provider_key".to_string()),
                true,
                false,
            )
        }
    }
}

fn import(args: SecretsImportArgs) -> CliResult<()> {
    let mut contents = Zeroizing::new(String::new());
    File::open(&args.path)
        .and_then(|mut file| file.read_to_string(&mut contents))
        .map_err(|error| {
            CliError::new(
                "secrets_import_failed",
                format!("failed to read secrets import file {}: {error}", args.path),
                args.json,
            )
        })?;
    let document = serde_json::from_str::<SecretImportDocument>(&contents).map_err(|error| {
        CliError::new(
            "secrets_import_failed",
            format!("failed to parse secrets import file {}: {error}", args.path),
            args.json,
        )
    })?;
    let mut imported = 0;
    for entry in document.secrets {
        let value = import_entry_value(&entry, args.json)?;
        let set_args = SecretsSetArgs {
            scope: entry.scope,
            id: entry.id,
            plugin_id: entry.plugin_id,
            key: entry.key,
            stdin: false,
            env: None,
            json: args.json,
        };
        set_imported_secret(set_args, value)?;
        imported += 1;
    }
    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&serde_json::json!({ "imported": imported })),
        OutputFormat::Text => {
            output::write_text(format!("imported={imported}"));
            Ok(())
        }
    }
}

fn set_imported_secret(args: SecretsSetArgs, value: Zeroizing<String>) -> CliResult<()> {
    match args.scope {
        SecretScopeArg::Ai => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            AiProviderKeyStore::new()
                .store_provider_key(&id, value)
                .map_err(|error| runtime_error(error, args.json))
        }
        SecretScopeArg::Plugin => {
            let plugin_id = required_arg(args.plugin_id.as_deref(), "--plugin-id", args.json)?;
            let key = required_arg(args.key.as_deref(), "--key", args.json)?;
            let account_id = plugin_secret_account_id(&plugin_id, &key, args.json)?;
            AiProviderKeyStore::new()
                .store_provider_key(&account_id, value)
                .map_err(|error| runtime_error(error, args.json))
        }
        SecretScopeArg::CloudSync => {
            let key = required_arg(args.key.as_deref(), "--key", args.json)?;
            write_cloud_sync_secret(&key, Some(value.as_str()), args.json)
        }
        SecretScopeArg::Connection => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            let key = args.key.unwrap_or_else(|| "password".to_string());
            write_connection_secret(&id, &key, Some(value), args.json)
        }
        SecretScopeArg::Portable => {
            let id = required_arg(args.id.as_deref(), "--id", args.json)?;
            validate_portable_secret_kind(args.key.as_deref(), args.json)?;
            // Portable secrets decrypt into AI provider keychain entries, matching .oxide import.
            AiProviderKeyStore::new()
                .store_provider_key(&id, value)
                .map_err(|error| runtime_error(error, args.json))
        }
    }
}

fn validate_portable_secret_kind(key: Option<&str>, json: bool) -> CliResult<()> {
    let key = key.unwrap_or("ai_provider_key");
    if key == "ai_provider_key" {
        Ok(())
    } else {
        Err(CliError::new(
            "portable_secret_kind_invalid",
            "only portable ai_provider_key secrets are supported",
            json,
        ))
    }
}

fn write_status(json: bool, statuses: Vec<SecretStatus>) -> CliResult<()> {
    match output::format_from_flag(json) {
        OutputFormat::Json => output::write_json(&SecretsStatusResponse {
            count: statuses.len(),
            secrets: statuses,
        }),
        OutputFormat::Text => {
            if statuses.is_empty() {
                output::write_text("No secret hints");
            } else {
                for status in statuses {
                    output::write_text(format!(
                        "{}\t{}\t{}\tconfigured={}",
                        status.scope,
                        status.id.as_deref().unwrap_or("-"),
                        status.key.as_deref().unwrap_or("-"),
                        status.configured
                    ));
                }
            }
            Ok(())
        }
    }
}

fn write_secret_response(
    json: bool,
    scope: &'static str,
    id: Option<String>,
    key: Option<String>,
    cleared: bool,
    configured: bool,
) -> CliResult<()> {
    let response = SecretsWriteResponse {
        scope,
        id,
        key,
        imported: usize::from(configured),
        cleared,
        configured,
    };
    match output::format_from_flag(json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format!(
                "{}\t{}\t{}\tconfigured={} cleared={}",
                response.scope,
                response.id.as_deref().unwrap_or("-"),
                response.key.as_deref().unwrap_or("-"),
                response.configured,
                response.cleared
            ));
            Ok(())
        }
    }
}

fn cloud_sync_statuses(key: Option<&str>, json: bool) -> CliResult<Vec<SecretStatus>> {
    let path = default_cloud_sync_path();
    let state = cloud_sync_preview::load_persisted_state(&path, json)?;
    Ok(state
        .secret_hints
        .into_iter()
        .filter(|(hint_key, _)| key.is_none_or(|key| key == hint_key))
        .map(|(key, configured)| SecretStatus {
            scope: "cloud-sync",
            id: None,
            key: Some(key),
            configured,
        })
        .collect())
}

fn connection_statuses(id: Option<&str>, json: bool) -> CliResult<Vec<SecretStatus>> {
    let store = ConnectionStore::load_read_only(default_connections_path())
        .map_err(|error| runtime_error(error, json))?;
    Ok(store
        .connections()
        .iter()
        .filter(|connection| id.is_none_or(|id| connection.id == id || connection.name == id))
        .map(|connection| SecretStatus {
            scope: "connection",
            id: Some(connection.id.clone()),
            key: Some(connection.auth.auth_type().as_str().to_string()),
            configured: !matches!(connection.auth, SavedAuth::Agent),
        })
        .collect())
}

fn write_cloud_sync_secret(key: &str, value: Option<&str>, json: bool) -> CliResult<()> {
    validate_cloud_sync_secret_key(key, json)?;
    let path = default_cloud_sync_path();
    let mut state = cloud_sync_preview::load_persisted_state(&path, json)?;
    let mut provider = CloudSyncKeychainSecretProvider::new(state.secret_hints.clone());
    provider
        .store_secret(key, value)
        .map_err(|error| runtime_error(error, json))?;
    state.secret_hints = provider.hints().clone();
    let mut store = oxideterm_cloud_sync::state::CloudSyncStateStore::load(path)
        .map_err(|error| runtime_error(error, json))?;
    store.state_mut().secret_hints = state.secret_hints;
    store.save().map_err(|error| runtime_error(error, json))
}

fn write_connection_secret(
    query: &str,
    key: &str,
    value: Option<Zeroizing<String>>,
    json: bool,
) -> CliResult<()> {
    let mut store = ConnectionStore::load(default_connections_path())
        .map_err(|error| runtime_error(error, json))?;
    let connection = store
        .connections()
        .iter()
        .find(|connection| connection.id == query || connection.name == query)
        .cloned()
        .ok_or_else(|| {
            CliError::new(
                "connection_not_found",
                format!("connection '{query}' was not found"),
                json,
            )
        })?;
    let secret = value.map(SecretString::from);
    let auth = match (key, connection.auth.clone(), secret) {
        ("password", _, Some(secret)) => SavedAuth::Password {
            keychain_id: None,
            plaintext_password: Some(secret),
        },
        ("password", _, None) => SavedAuth::Agent,
        ("passphrase", SavedAuth::Key { key_path, .. }, secret) => SavedAuth::Key {
            key_path,
            has_passphrase: secret.is_some(),
            passphrase_keychain_id: None,
            plaintext_passphrase: secret,
        },
        (
            "passphrase",
            SavedAuth::Certificate {
                key_path,
                cert_path,
                ..
            },
            secret,
        ) => SavedAuth::Certificate {
            key_path,
            cert_path,
            has_passphrase: secret.is_some(),
            passphrase_keychain_id: None,
            plaintext_passphrase: secret,
        },
        _ => {
            return Err(CliError::new(
                "connection_secret_key_invalid",
                "connection secrets support key=password or key=passphrase",
                json,
            ));
        }
    };
    let post_connect_command = connection.post_connect_command().map(ToOwned::to_owned);
    store
        .upsert(SaveConnectionRequest {
            id: Some(connection.id),
            name: connection.name,
            group: connection.group,
            host: connection.host,
            port: connection.port,
            username: connection.username,
            auth,
            proxy_chain: connection.proxy_chain,
            upstream_proxy: connection.upstream_proxy,
            color: connection.color,
            tags: connection.tags,
            agent_forwarding: connection.options.agent_forwarding,
            legacy_ssh_compatibility: connection.options.legacy_ssh_compatibility,
            post_connect_command,
        })
        .map_err(|error| runtime_error(error, json))?;
    Ok(())
}

fn read_secret_value(stdin: bool, env: Option<&str>, json: bool) -> CliResult<Zeroizing<String>> {
    if let Some(name) = env {
        return std::env::var(name).map(Zeroizing::new).map_err(|error| {
            CliError::new(
                "secret_missing",
                format!("failed to read secret from env var {name}: {error}"),
                json,
            )
        });
    }
    if stdin {
        let mut value = Zeroizing::new(String::new());
        io::stdin().read_to_string(&mut value).map_err(|error| {
            CliError::new(
                "secret_read_failed",
                format!("failed to read secret from stdin: {error}"),
                json,
            )
        })?;
        while value.ends_with('\n') || value.ends_with('\r') {
            value.pop();
        }
        return Ok(value);
    }
    Err(CliError::new(
        "secret_required",
        "provide --stdin or --env VAR; secret values are not accepted as command arguments",
        json,
    ))
}

fn import_entry_value(entry: &SecretImportEntry, json: bool) -> CliResult<Zeroizing<String>> {
    if entry.value.is_some() && entry.env.is_some() {
        return Err(CliError::new(
            "secrets_import_failed",
            "secret import entries must use only one of value or env",
            json,
        ));
    }
    if let Some(value) = &entry.value {
        return Ok(Zeroizing::new(value.to_string()));
    }
    if let Some(env) = &entry.env {
        return std::env::var(env).map(Zeroizing::new).map_err(|error| {
            CliError::new(
                "secret_missing",
                format!("failed to read secret from env var {env}: {error}"),
                json,
            )
        });
    }
    Err(CliError::new(
        "secrets_import_failed",
        "secret import entries must provide value or env",
        json,
    ))
}

fn validate_cloud_sync_secret_key(key: &str, json: bool) -> CliResult<()> {
    let allowed = [
        secret_keys::SYNC_PASSWORD,
        secret_keys::TOKEN,
        secret_keys::GIT_TOKEN,
        secret_keys::BASIC_USERNAME,
        secret_keys::BASIC_PASSWORD,
        secret_keys::ACCESS_KEY_ID,
        secret_keys::SECRET_ACCESS_KEY,
        secret_keys::SESSION_TOKEN,
    ];
    if allowed.contains(&key) {
        Ok(())
    } else {
        Err(CliError::new(
            "cloud_sync_secret_key_invalid",
            format!("unsupported cloud sync secret key: {key}"),
            json,
        ))
    }
}

fn plugin_secret_account_id(plugin_id: &str, key: &str, json: bool) -> CliResult<String> {
    if plugin_id.is_empty()
        || plugin_id.contains('/')
        || plugin_id.contains('\\')
        || plugin_id.contains("..")
        || plugin_id.bytes().any(|byte| byte < 0x20)
    {
        return Err(CliError::new(
            "plugin_secret_id_invalid",
            "plugin id is invalid for secret storage",
            json,
        ));
    }
    if key.is_empty() || key.bytes().any(|byte| byte < 0x20) {
        return Err(CliError::new(
            "plugin_secret_key_invalid",
            "plugin secret key is invalid",
            json,
        ));
    }
    Ok(format!(
        "plugin-secret:{}:{}:{}:{}",
        plugin_id.len(),
        plugin_id,
        key.len(),
        key
    ))
}

fn required_arg(value: Option<&str>, name: &str, json: bool) -> CliResult<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CliError::new(
                "secret_argument_missing",
                format!("{name} is required"),
                json,
            )
        })
}
