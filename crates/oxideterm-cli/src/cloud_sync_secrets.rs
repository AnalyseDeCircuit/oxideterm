// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::BTreeMap,
    fs::File,
    io::{self, Read},
};

use oxideterm_cloud_sync::{secret_keys, secrets::CloudSyncKeychainSecretProvider};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{
    args::{
        CloudSyncSecretKeyArgs, CloudSyncSecretSetArgs, CloudSyncSecretsAction,
        CloudSyncSecretsCommand, CloudSyncSecretsImportArgs, JsonArgs,
    },
    cloud_sync_preview,
    error::{CliError, CliResult, runtime_error},
    output::{self, OutputFormat},
    paths::default_cloud_sync_path,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SecretHintStatus {
    pub(crate) key: String,
    pub(crate) configured: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncSecretsStatusResponse {
    path: String,
    count: usize,
    hints: Vec<SecretHintStatus>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncSecretWriteResponse {
    path: String,
    key: Option<String>,
    imported: usize,
    cleared: bool,
    hints: Vec<SecretHintStatus>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum CloudSyncSecretImportDocument {
    LegacyMap(BTreeMap<String, Zeroizing<String>>),
    Entries {
        secrets: Vec<CloudSyncSecretImportEntry>,
    },
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CloudSyncSecretImportEntry {
    key: String,
    value: Option<Zeroizing<String>>,
    env: Option<String>,
}

pub fn run(command: CloudSyncSecretsCommand) -> CliResult<()> {
    match command.action {
        CloudSyncSecretsAction::Status(args) => status(args),
        CloudSyncSecretsAction::Set(args) => set(args),
        CloudSyncSecretsAction::Clear(args) => clear(args),
        CloudSyncSecretsAction::Import(args) => import(args),
    }
}

fn status(args: JsonArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let state = cloud_sync_preview::load_persisted_state(&path, args.json)?;
    let hints = secret_hint_statuses(&state.secret_hints);
    let response = CloudSyncSecretsStatusResponse {
        path: path.display().to_string(),
        count: hints.len(),
        hints,
    };

    match output::format_from_flag(args.json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            if response.hints.is_empty() {
                output::write_text("No cloud sync secret hints");
            } else {
                for hint in &response.hints {
                    output::write_text(format_secret_hint(hint));
                }
            }
            Ok(())
        }
    }
}

fn set(args: CloudSyncSecretSetArgs) -> CliResult<()> {
    let value = read_secret_value(args.stdin, args.env.as_deref(), args.json)?;
    write_secret(args.key, Some(value.as_str()), args.json)
}

fn clear(args: CloudSyncSecretKeyArgs) -> CliResult<()> {
    write_secret(args.key, None, args.json)
}

fn import(args: CloudSyncSecretsImportArgs) -> CliResult<()> {
    let path = default_cloud_sync_path();
    let mut state = cloud_sync_preview::load_persisted_state(&path, args.json)?;
    let mut contents = Zeroizing::new(String::new());
    File::open(&args.path)
        .and_then(|mut file| file.read_to_string(&mut contents))
        .map_err(|error| {
            CliError::new(
                "cloud_sync_secret_import_failed",
                format!("failed to read secret import file {}: {error}", args.path),
                args.json,
            )
        })?;
    let document =
        serde_json::from_str::<CloudSyncSecretImportDocument>(&contents).map_err(|error| {
            CliError::new(
                "cloud_sync_secret_import_failed",
                format!("failed to parse secret import file {}: {error}", args.path),
                args.json,
            )
        })?;
    let values = imported_secret_values(document, args.json)?;
    let mut provider = CloudSyncKeychainSecretProvider::new(state.secret_hints.clone());
    for (key, value) in &values {
        validate_secret_key(key, args.json)?;
        // Values are handed directly to the provider and are never echoed in CLI output.
        provider
            .store_secret(key, Some(value.as_str()))
            .map_err(|error| runtime_error(error, args.json))?;
    }
    state.secret_hints = provider.hints().clone();
    save_secret_hints(&state, args.json)?;
    let response = CloudSyncSecretWriteResponse {
        path: path.display().to_string(),
        key: None,
        imported: values.len(),
        cleared: false,
        hints: secret_hint_statuses(&state.secret_hints),
    };
    write_secret_response(args.json, response)
}

fn imported_secret_values(
    document: CloudSyncSecretImportDocument,
    json: bool,
) -> CliResult<BTreeMap<String, Zeroizing<String>>> {
    match document {
        CloudSyncSecretImportDocument::LegacyMap(values) => Ok(values),
        CloudSyncSecretImportDocument::Entries { secrets } => {
            let mut values = BTreeMap::new();
            for entry in secrets {
                validate_secret_key(&entry.key, json)?;
                if entry.value.is_some() && entry.env.is_some() {
                    return Err(CliError::new(
                        "cloud_sync_secret_import_failed",
                        format!("secret '{}' must use only one of value or env", entry.key),
                        json,
                    ));
                }
                let value = if let Some(value) = entry.value {
                    value
                } else if let Some(env) = entry.env {
                    Zeroizing::new(std::env::var(&env).map_err(|error| {
                        CliError::new(
                            "cloud_sync_secret_missing",
                            format!(
                                "failed to read secret '{}' from env var {env}: {error}",
                                entry.key
                            ),
                            json,
                        )
                    })?)
                } else {
                    return Err(CliError::new(
                        "cloud_sync_secret_import_failed",
                        format!("secret '{}' must provide value or env", entry.key),
                        json,
                    ));
                };
                values.insert(entry.key, value);
            }
            Ok(values)
        }
    }
}

fn write_secret(key: String, value: Option<&str>, json: bool) -> CliResult<()> {
    validate_secret_key(&key, json)?;
    let path = default_cloud_sync_path();
    let mut state = cloud_sync_preview::load_persisted_state(&path, json)?;
    let mut provider = CloudSyncKeychainSecretProvider::new(state.secret_hints.clone());
    // Secret values cross the CLI boundary only through stdin/env/import and are not logged or serialized.
    provider
        .store_secret(&key, value)
        .map_err(|error| runtime_error(error, json))?;
    state.secret_hints = provider.hints().clone();
    save_secret_hints(&state, json)?;
    let response = CloudSyncSecretWriteResponse {
        path: path.display().to_string(),
        key: Some(key),
        imported: usize::from(value.is_some()),
        cleared: value.is_none(),
        hints: secret_hint_statuses(&state.secret_hints),
    };
    write_secret_response(json, response)
}

fn save_secret_hints(
    state: &oxideterm_cloud_sync::state::CloudSyncPersistedState,
    json: bool,
) -> CliResult<()> {
    let mut store =
        oxideterm_cloud_sync::state::CloudSyncStateStore::load(default_cloud_sync_path())
            .map_err(|error| runtime_error(error, json))?;
    store.state_mut().secret_hints = state.secret_hints.clone();
    store.save().map_err(|error| runtime_error(error, json))
}

fn read_secret_value(stdin: bool, env: Option<&str>, json: bool) -> CliResult<Zeroizing<String>> {
    if let Some(name) = env {
        let value = std::env::var(name).map_err(|error| {
            CliError::new(
                "cloud_sync_secret_missing",
                format!("failed to read secret from env var {name}: {error}"),
                json,
            )
        })?;
        return Ok(Zeroizing::new(value));
    }
    if stdin {
        let mut value = Zeroizing::new(String::new());
        io::stdin().read_to_string(&mut value).map_err(|error| {
            CliError::new(
                "cloud_sync_secret_read_failed",
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
        "cloud_sync_secret_required",
        "provide --stdin or --env VAR; secret values are not accepted as command arguments",
        json,
    ))
}

fn validate_secret_key(key: &str, json: bool) -> CliResult<()> {
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

fn write_secret_response(json: bool, response: CloudSyncSecretWriteResponse) -> CliResult<()> {
    match output::format_from_flag(json) {
        OutputFormat::Json => output::write_json(&response),
        OutputFormat::Text => {
            output::write_text(format!(
                "key={} imported={} cleared={}",
                response.key.as_deref().unwrap_or("-"),
                response.imported,
                response.cleared
            ));
            Ok(())
        }
    }
}

pub(crate) fn secret_hint_statuses(
    hints: &std::collections::BTreeMap<String, bool>,
) -> Vec<SecretHintStatus> {
    // Only persisted hint booleans are exposed here; secret values are never loaded or printed.
    hints
        .iter()
        .map(|(key, configured)| SecretHintStatus {
            key: key.clone(),
            configured: *configured,
        })
        .collect()
}

fn format_secret_hint(hint: &SecretHintStatus) -> String {
    format!("{}\tconfigured={}", hint.key, hint.configured)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn secret_statuses_preserve_only_hint_flags() {
        let mut hints = BTreeMap::new();
        hints.insert("webdav.password".to_string(), true);

        let statuses = secret_hint_statuses(&hints);

        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].key, "webdav.password");
        assert!(statuses[0].configured);
    }
}
