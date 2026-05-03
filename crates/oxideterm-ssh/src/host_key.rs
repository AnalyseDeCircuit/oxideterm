// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{net::ToSocketAddrs, sync::Arc, time::Duration};

use russh::{
    client,
    keys::{
        self, PublicKey,
        known_hosts::{check_known_hosts, known_host_keys, learn_known_hosts},
        ssh_key::HashAlg,
    },
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::SshTransportError;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HostKeyStatus {
    Verified,
    Unknown {
        fingerprint: String,
        key_type: String,
    },
    Changed {
        expected_fingerprint: String,
        actual_fingerprint: String,
        key_type: String,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostKeyVerification {
    Verified,
    Unknown {
        fingerprint: String,
        key_type: String,
    },
    Changed {
        expected_fingerprint: String,
        actual_fingerprint: String,
        key_type: String,
    },
}

pub fn public_key_fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub fn public_key_type(key: &PublicKey) -> String {
    key.algorithm().as_str().to_string()
}

pub fn verify_host_key(
    host: &str,
    port: u16,
    server_public_key: &PublicKey,
) -> Result<HostKeyVerification, SshTransportError> {
    let fingerprint = public_key_fingerprint(server_public_key);
    let key_type = public_key_type(server_public_key);

    match check_known_hosts(host, port, server_public_key) {
        Ok(true) => Ok(HostKeyVerification::Verified),
        Ok(false) => Ok(HostKeyVerification::Unknown {
            fingerprint,
            key_type,
        }),
        Err(keys::Error::KeyChanged { .. }) => {
            let expected_fingerprint = known_host_keys(host, port)
                .ok()
                .and_then(|keys| {
                    keys.into_iter()
                        .map(|(_, key)| key)
                        .find(|key| key.algorithm() == server_public_key.algorithm())
                })
                .map(|key| public_key_fingerprint(&key))
                .unwrap_or_else(|| "unknown".to_string());

            Ok(HostKeyVerification::Changed {
                expected_fingerprint,
                actual_fingerprint: fingerprint,
                key_type,
            })
        }
        Err(error) => Err(SshTransportError::HostKeyCheckFailed(error.to_string())),
    }
}

pub fn learn_host_key(
    host: &str,
    port: u16,
    server_public_key: &PublicKey,
) -> Result<(), SshTransportError> {
    learn_known_hosts(host, port, server_public_key)
        .map_err(|error| SshTransportError::HostKeyCheckFailed(error.to_string()))
}

struct PreflightHandler {
    host: String,
    port: u16,
    status: Arc<Mutex<Option<HostKeyStatus>>>,
}

impl PreflightHandler {
    fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            status: Arc::new(Mutex::new(None)),
        }
    }
}

impl client::Handler for PreflightHandler {
    type Error = SshTransportError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let status = match verify_host_key(&self.host, self.port, server_public_key)? {
            HostKeyVerification::Verified => HostKeyStatus::Verified,
            HostKeyVerification::Unknown {
                fingerprint,
                key_type,
            } => HostKeyStatus::Unknown {
                fingerprint,
                key_type,
            },
            HostKeyVerification::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            } => HostKeyStatus::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            },
        };
        *self.status.lock().await = Some(status);
        Err(SshTransportError::PreflightComplete)
    }
}

pub async fn check_host_key(host: &str, port: u16, timeout_secs: u64) -> HostKeyStatus {
    let addr = format!("{host}:{port}");
    let socket_addr = match addr.to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) => addr,
            None => {
                return HostKeyStatus::Error {
                    message: format!("Could not resolve address: {addr}"),
                };
            }
        },
        Err(error) => {
            return HostKeyStatus::Error {
                message: format!("DNS resolution failed: {error}"),
            };
        }
    };

    let handler = PreflightHandler::new(host.to_string(), port);
    let status = Arc::clone(&handler.status);
    let config = client::Config {
        inactivity_timeout: Some(Duration::from_secs(timeout_secs)),
        ..client::Config::default()
    };

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client::connect(Arc::new(config), socket_addr, handler),
    )
    .await;

    if let Some(status) = status.lock().await.take() {
        return status;
    }

    match result {
        Ok(Ok(_)) => HostKeyStatus::Error {
            message: "Unexpectedly completed SSH preflight".to_string(),
        },
        Ok(Err(SshTransportError::PreflightComplete)) => HostKeyStatus::Error {
            message: "SSH preflight completed without a captured host key".to_string(),
        },
        Ok(Err(error)) => HostKeyStatus::Error {
            message: error.to_string(),
        },
        Err(_) => HostKeyStatus::Error {
            message: format!("Connection timeout after {timeout_secs}s"),
        },
    }
}
