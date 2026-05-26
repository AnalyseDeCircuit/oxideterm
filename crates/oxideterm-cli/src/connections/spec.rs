// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use oxideterm_connections::{
    SaveConnectionRequest, SavedAuth, SavedConnection, SavedProxyHop, SecretString,
};
use serde::Deserialize;
use std::fs;

use crate::error::{CliError, CliResult};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ConnectionSpec {
    name: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    username: Option<String>,
    group: Option<Option<String>>,
    color: Option<Option<String>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    auth: Option<ConnectionAuthSpec>,
    #[serde(default)]
    proxy_chain: Option<Vec<ConnectionProxyHopSpec>>,
    agent_forwarding: Option<bool>,
    post_connect_command: Option<Option<String>>,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ConnectionAuthSpec {
    Password {
        password: Option<String>,
        password_env: Option<String>,
        save_password: Option<bool>,
    },
    Key {
        key_path: String,
        passphrase: Option<String>,
        passphrase_env: Option<String>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        passphrase: Option<String>,
        passphrase_env: Option<String>,
    },
    Agent,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionProxyHopSpec {
    host: String,
    #[serde(default = "default_connection_port")]
    port: u16,
    username: String,
    auth: ConnectionAuthSpec,
    #[serde(default)]
    agent_forwarding: bool,
}

pub(super) fn read_connection_spec(path: &str, json: bool) -> CliResult<ConnectionSpec> {
    let contents = fs::read_to_string(path).map_err(|error| {
        CliError::new(
            "connection_spec_read_failed",
            format!("failed to read connection spec {path}: {error}"),
            json,
        )
    })?;
    serde_json::from_str::<ConnectionSpec>(&contents).map_err(|error| {
        CliError::new(
            "connection_spec_parse_failed",
            format!("failed to parse connection spec {path}: {error}"),
            json,
        )
    })
}

pub(super) fn connection_request_from_spec(
    spec: ConnectionSpec,
    existing: Option<&SavedConnection>,
    json: bool,
) -> CliResult<SaveConnectionRequest> {
    let name = required_or_existing(
        spec.name,
        existing.map(|connection| &connection.name),
        "name",
        json,
    )?;
    let host = required_or_existing(
        spec.host,
        existing.map(|connection| &connection.host),
        "host",
        json,
    )?;
    let username = required_or_existing(
        spec.username,
        existing.map(|connection| &connection.username),
        "username",
        json,
    )?;
    let auth = match spec.auth {
        Some(auth) => saved_auth_from_connection_spec(
            auth,
            existing.map(|connection| &connection.auth),
            json,
        )?,
        None => existing
            .map(|connection| connection.auth.clone())
            .unwrap_or(SavedAuth::Agent),
    };
    let proxy_chain = match spec.proxy_chain {
        Some(proxy_chain) => proxy_chain
            .into_iter()
            .map(|hop| saved_proxy_hop_from_spec(hop, json))
            .collect::<CliResult<Vec<_>>>()?,
        None => existing
            .map(|connection| connection.proxy_chain.clone())
            .unwrap_or_default(),
    };
    Ok(SaveConnectionRequest {
        id: existing.map(|connection| connection.id.clone()),
        name,
        group: spec
            .group
            .unwrap_or_else(|| existing.and_then(|connection| connection.group.clone())),
        host,
        port: spec.port.unwrap_or_else(|| {
            existing
                .map(|connection| connection.port)
                .unwrap_or(default_connection_port())
        }),
        username,
        auth,
        proxy_chain,
        color: spec
            .color
            .unwrap_or_else(|| existing.and_then(|connection| connection.color.clone())),
        tags: spec.tags.unwrap_or_else(|| {
            existing
                .map(|connection| connection.tags.clone())
                .unwrap_or_default()
        }),
        agent_forwarding: spec.agent_forwarding.unwrap_or_else(|| {
            existing
                .map(|connection| connection.options.agent_forwarding)
                .unwrap_or(false)
        }),
        post_connect_command: spec.post_connect_command.unwrap_or_else(|| {
            existing.and_then(|connection| connection.post_connect_command.clone())
        }),
    })
}

fn required_or_existing(
    value: Option<String>,
    existing: Option<&String>,
    field: &str,
    json: bool,
) -> CliResult<String> {
    let value = value.or_else(|| existing.cloned()).unwrap_or_default();
    if value.trim().is_empty() {
        return Err(CliError::new(
            "connection_spec_invalid",
            format!("connection spec requires non-empty {field}"),
            json,
        ));
    }
    Ok(value)
}

fn saved_auth_from_connection_spec(
    spec: ConnectionAuthSpec,
    existing_auth: Option<&SavedAuth>,
    json: bool,
) -> CliResult<SavedAuth> {
    Ok(match spec {
        ConnectionAuthSpec::Password {
            password,
            password_env,
            save_password,
        } => {
            let secret = secret_from_value_or_env(password, password_env, "password", json)?;
            if secret.is_none() {
                if let Some(existing @ SavedAuth::Password { .. }) = existing_auth {
                    return Ok(existing.clone());
                }
            }
            SavedAuth::Password {
                keychain_id: None,
                // Inline CLI secrets are wrapped immediately, then moved into the
                // connection store/keychain path instead of being formatted for output.
                plaintext_password: save_password
                    .unwrap_or(secret.is_some())
                    .then_some(secret)
                    .flatten(),
            }
        }
        ConnectionAuthSpec::Key {
            key_path,
            passphrase,
            passphrase_env,
        } => {
            let secret = secret_from_value_or_env(passphrase, passphrase_env, "passphrase", json)?;
            SavedAuth::Key {
                key_path,
                has_passphrase: secret.is_some(),
                passphrase_keychain_id: None,
                plaintext_passphrase: secret,
            }
        }
        ConnectionAuthSpec::Certificate {
            key_path,
            cert_path,
            passphrase,
            passphrase_env,
        } => {
            let secret = secret_from_value_or_env(passphrase, passphrase_env, "passphrase", json)?;
            SavedAuth::Certificate {
                key_path,
                cert_path,
                has_passphrase: secret.is_some(),
                passphrase_keychain_id: None,
                plaintext_passphrase: secret,
            }
        }
        ConnectionAuthSpec::Agent => SavedAuth::Agent,
    })
}

fn saved_proxy_hop_from_spec(spec: ConnectionProxyHopSpec, json: bool) -> CliResult<SavedProxyHop> {
    Ok(SavedProxyHop {
        host: spec.host,
        port: spec.port,
        username: spec.username,
        auth: saved_auth_from_connection_spec(spec.auth, None, json)?,
        agent_forwarding: spec.agent_forwarding,
    })
}

fn secret_from_value_or_env(
    value: Option<String>,
    env_var: Option<String>,
    field: &str,
    json: bool,
) -> CliResult<Option<SecretString>> {
    if value.is_some() && env_var.is_some() {
        return Err(CliError::new(
            "connection_spec_invalid",
            format!("{field} and {field}Env/passwordEnv style references are mutually exclusive"),
            json,
        ));
    }
    if let Some(value) = value {
        return Ok(Some(SecretString::from(value)));
    }
    if let Some(env_var) = env_var {
        let value = std::env::var(&env_var).map_err(|error| {
            CliError::new(
                "connection_spec_secret_missing",
                format!("failed to read {field} from env var {env_var}: {error}"),
                json,
            )
        })?;
        return Ok(Some(SecretString::from(value)));
    }
    Ok(None)
}

fn default_connection_port() -> u16 {
    22
}
