// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{fs, io::Read};

use oxideterm_connections::{
    SaveConnectionRequest, SavedAuth, SavedConnection, SavedProxyHop, SecretString,
};
use serde::Deserialize;
use zeroize::Zeroizing;

use crate::{
    args::{ConnectionAuthArg, ConnectionDirectArgs},
    error::{CliError, CliResult},
};

#[derive(Default, Deserialize)]
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
        password: Option<SecretString>,
        password_env: Option<String>,
        save_password: Option<bool>,
    },
    Key {
        key_path: String,
        passphrase: Option<SecretString>,
        passphrase_env: Option<String>,
    },
    Certificate {
        key_path: String,
        cert_path: String,
        passphrase: Option<SecretString>,
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

pub(super) fn connection_spec_from_direct_args(
    args: ConnectionDirectArgs,
    json: bool,
) -> CliResult<Option<ConnectionSpec>> {
    if !args.has_values() {
        return Ok(None);
    }
    let auth = direct_auth_spec(&args, json)?;
    Ok(Some(ConnectionSpec {
        name: args.name,
        host: args.host,
        port: args.port,
        username: args.username,
        group: args.group.map(Some),
        color: args.color.map(Some),
        tags: (!args.tags.is_empty()).then_some(args.tags),
        auth,
        proxy_chain: None,
        agent_forwarding: args.agent_forwarding,
        post_connect_command: args.post_connect_command.map(Some),
    }))
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
        upstream_proxy: existing
            .map(|connection| connection.upstream_proxy.clone())
            .unwrap_or_default(),
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
            existing.and_then(|connection| connection.post_connect_command().map(ToOwned::to_owned))
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

fn direct_auth_spec(
    args: &ConnectionDirectArgs,
    json: bool,
) -> CliResult<Option<ConnectionAuthSpec>> {
    Ok(match args.auth {
        Some(ConnectionAuthArg::Agent) => Some(ConnectionAuthSpec::Agent),
        Some(ConnectionAuthArg::Password) => Some(ConnectionAuthSpec::Password {
            password: read_direct_secret(
                args.password_stdin,
                args.password_env.as_deref(),
                "password",
                json,
            )?,
            password_env: None,
            save_password: args.save_password,
        }),
        Some(ConnectionAuthArg::Key) => Some(ConnectionAuthSpec::Key {
            key_path: required_direct_value(args.key_path.as_ref(), "key-path", json)?,
            passphrase: read_direct_secret(
                args.passphrase_stdin,
                args.passphrase_env.as_deref(),
                "passphrase",
                json,
            )?,
            passphrase_env: None,
        }),
        Some(ConnectionAuthArg::Certificate) => Some(ConnectionAuthSpec::Certificate {
            key_path: required_direct_value(args.key_path.as_ref(), "key-path", json)?,
            cert_path: required_direct_value(args.cert_path.as_ref(), "cert-path", json)?,
            passphrase: read_direct_secret(
                args.passphrase_stdin,
                args.passphrase_env.as_deref(),
                "passphrase",
                json,
            )?,
            passphrase_env: None,
        }),
        None => None,
    })
}

fn required_direct_value(value: Option<&String>, field: &str, json: bool) -> CliResult<String> {
    value
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CliError::new(
                "connection_direct_args_invalid",
                format!("--{field} is required for the selected connection auth type"),
                json,
            )
        })
}

fn read_direct_secret(
    stdin: bool,
    env_var: Option<&str>,
    field: &str,
    json: bool,
) -> CliResult<Option<SecretString>> {
    if stdin && env_var.is_some() {
        return Err(CliError::new(
            "connection_direct_args_invalid",
            format!("--{field}-stdin and --{field}-env are mutually exclusive"),
            json,
        ));
    }
    if stdin {
        let mut value = Zeroizing::new(String::new());
        std::io::stdin()
            .read_to_string(&mut value)
            .map_err(|error| {
                CliError::new(
                    "connection_secret_read_failed",
                    format!("failed to read {field} from stdin: {error}"),
                    json,
                )
            })?;
        while value.ends_with('\n') || value.ends_with('\r') {
            value.pop();
        }
        return Ok(Some(SecretString::from(value)));
    }
    if let Some(env_var) = env_var {
        let value = std::env::var(env_var).map_err(|error| {
            CliError::new(
                "connection_spec_secret_missing",
                format!("failed to read {field} from env var {env_var}: {error}"),
                json,
            )
        })?;
        return Ok(Some(SecretString::from(Zeroizing::new(value))));
    }
    Ok(None)
}

fn secret_from_value_or_env(
    value: Option<SecretString>,
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
        return Ok(Some(value));
    }
    if let Some(env_var) = env_var {
        let value = std::env::var(&env_var).map_err(|error| {
            CliError::new(
                "connection_spec_secret_missing",
                format!("failed to read {field} from env var {env_var}: {error}"),
                json,
            )
        })?;
        return Ok(Some(SecretString::from(Zeroizing::new(value))));
    }
    Ok(None)
}

fn default_connection_port() -> u16 {
    22
}

impl ConnectionDirectArgs {
    fn has_values(&self) -> bool {
        self.name.is_some()
            || self.host.is_some()
            || self.username.is_some()
            || self.port.is_some()
            || self.group.is_some()
            || self.color.is_some()
            || !self.tags.is_empty()
            || self.auth.is_some()
            || self.password_stdin
            || self.password_env.is_some()
            || self.save_password.is_some()
            || self.key_path.is_some()
            || self.cert_path.is_some()
            || self.passphrase_stdin
            || self.passphrase_env.is_some()
            || self.agent_forwarding.is_some()
            || self.post_connect_command.is_some()
    }
}
