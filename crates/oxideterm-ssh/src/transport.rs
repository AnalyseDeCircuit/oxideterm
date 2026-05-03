// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{net::ToSocketAddrs, path::PathBuf, sync::Arc, time::Duration};

use russh::{
    ChannelMsg, Pty, client,
    keys::{
        HashAlg, PrivateKeyWithHashAlg,
        agent::{AgentIdentity, client::AgentClient},
        load_openssh_certificate, load_secret_key,
    },
};
use tokio::sync::{broadcast, mpsc};

use crate::{
    AuthMethod, ConnectionConsumer, ConnectionState, SshConfig, SshConnectionRegistry,
    host_key::{HostKeyVerification, learn_host_key, public_key_fingerprint, verify_host_key},
};

pub const DEFAULT_PTY_MODES: &[(Pty, u32)] = &[
    (Pty::VINTR, 0x03),
    (Pty::VQUIT, 0x1c),
    (Pty::VERASE, 0x7f),
    (Pty::VKILL, 0x15),
    (Pty::VEOF, 0x04),
    (Pty::VEOL, 0x00),
    (Pty::VEOL2, 0x00),
    (Pty::VSTART, 0x11),
    (Pty::VSTOP, 0x13),
    (Pty::VSUSP, 0x1a),
    (Pty::VREPRINT, 0x12),
    (Pty::VWERASE, 0x17),
    (Pty::VLNEXT, 0x16),
    (Pty::VDISCARD, 0x0f),
    (Pty::ICRNL, 1),
    (Pty::IXON, 1),
    (Pty::IMAXBEL, 1),
    (Pty::IUTF8, 1),
    (Pty::ISIG, 1),
    (Pty::ICANON, 1),
    (Pty::ECHO, 1),
    (Pty::ECHOE, 1),
    (Pty::ECHOK, 1),
    (Pty::IEXTEN, 1),
    (Pty::ECHOCTL, 1),
    (Pty::ECHOKE, 1),
    (Pty::OPOST, 1),
    (Pty::ONLCR, 1),
    (Pty::CS8, 1),
    (Pty::TTY_OP_ISPEED, 38400),
    (Pty::TTY_OP_OSPEED, 38400),
];

#[derive(Debug, thiserror::Error)]
pub enum SshTransportError {
    #[error("DNS resolution failed for {address}: {message}")]
    DnsResolution { address: String, message: String },
    #[error("SSH connection timed out")]
    Timeout,
    #[error("SSH connection failed: {0}")]
    ConnectionFailed(String),
    #[error("SSH authentication failed: {0}")]
    AuthenticationFailed(String),
    #[error("SSH authentication method is not implemented in native yet: {0}")]
    UnsupportedAuth(&'static str),
    #[error("SSH host key is unknown for {host}:{port}: {fingerprint}")]
    HostKeyUnknown {
        host: String,
        port: u16,
        fingerprint: String,
    },
    #[error(
        "SSH host key changed for {host}:{port}: expected {expected_fingerprint}, got {actual_fingerprint}"
    )]
    HostKeyChanged {
        host: String,
        port: u16,
        expected_fingerprint: String,
        actual_fingerprint: String,
    },
    #[error("SSH host key check failed: {0}")]
    HostKeyCheckFailed(String),
    #[error("SSH preflight complete")]
    PreflightComplete,
    #[error("SSH channel error: {0}")]
    Channel(String),
}

impl From<russh::Error> for SshTransportError {
    fn from(error: russh::Error) -> Self {
        Self::ConnectionFailed(error.to_string())
    }
}

#[derive(Debug)]
pub enum SshTransportCommand {
    Data(Vec<u8>),
    Resize { cols: u16, rows: u16 },
    Close,
}

pub struct SshPtyHandle {
    pub session_id: String,
    pub command_tx: mpsc::Sender<SshTransportCommand>,
    pub output_rx: broadcast::Receiver<Vec<u8>>,
    registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
}

impl Drop for SshPtyHandle {
    fn drop(&mut self) {
        if let Some((registry, connection_id, consumer)) = self.registry_release.take() {
            registry.release(&connection_id, &consumer);
        }
    }
}

#[derive(Clone, Debug)]
pub struct SshTransportClient {
    config: SshConfig,
}

impl SshTransportClient {
    pub fn new(config: SshConfig) -> Self {
        Self { config }
    }

    pub async fn connect_shell(self) -> Result<SshPtyHandle, SshTransportError> {
        self.connect_shell_inner(None).await
    }

    pub async fn connect_shell_with_registry(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let connection = registry.acquire(self.config.clone(), consumer.clone());
        let connection_id = connection.connection_id().to_string();
        let result = self
            .connect_shell_inner(Some((
                registry.clone(),
                connection_id.clone(),
                consumer.clone(),
            )))
            .await;

        match &result {
            Ok(_) => {
                let _ = registry.mark_state(&connection_id, ConnectionState::Active);
            }
            Err(error) => {
                let _ =
                    registry.mark_state(&connection_id, ConnectionState::Error(error.to_string()));
                registry.release(&connection_id, &consumer);
            }
        }

        result
    }

    async fn connect_shell_inner(
        self,
        registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|error| SshTransportError::DnsResolution {
                address: addr.clone(),
                message: error.to_string(),
            })?
            .next()
            .ok_or_else(|| SshTransportError::DnsResolution {
                address: addr.clone(),
                message: "no address found".to_string(),
            })?;

        let client_config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(30)),
            keepalive_interval: Some(Duration::from_secs(15)),
            keepalive_max: 3,
            ..client::Config::default()
        };
        let handler = NativeClientHandler::new(
            self.config.host.clone(),
            self.config.port,
            self.config.strict_host_key_checking,
            self.config.trust_host_key,
            self.config.expected_host_key_fingerprint.clone(),
        );
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect(Arc::new(client_config), socket_addr, handler),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?;

        authenticate(&mut handle, &self.config).await?;

        let mut channel = handle
            .channel_open_session()
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        channel
            .request_pty(
                false,
                "xterm-256color",
                self.config.cols,
                self.config.rows,
                0,
                0,
                DEFAULT_PTY_MODES,
            )
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        if self.config.agent_forwarding {
            let _ = channel.agent_forward(true).await;
        }
        channel
            .request_shell(false)
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;

        let session_id = uuid::Uuid::new_v4().to_string();
        let (command_tx, mut command_rx) = mpsc::channel::<SshTransportCommand>(1024);
        let (output_tx, output_rx) = broadcast::channel::<Vec<u8>>(1024);
        let task_session_id = session_id.clone();

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    Some(command) = command_rx.recv() => {
                        match command {
                            SshTransportCommand::Data(data) => {
                                if channel.data(data.as_slice()).await.is_err() {
                                    break;
                                }
                            }
                            SshTransportCommand::Resize { cols, rows } => {
                                let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                            }
                            SshTransportCommand::Close => {
                                let _ = channel.eof().await;
                                break;
                            }
                        }
                    }
                    Some(message) = channel.wait() => {
                        match message {
                            ChannelMsg::Data { data } => {
                                let _ = output_tx.send(data.to_vec());
                            }
                            ChannelMsg::ExtendedData { data, ext } if ext == 1 => {
                                let _ = output_tx.send(data.to_vec());
                            }
                            ChannelMsg::Eof | ChannelMsg::Close => break,
                            ChannelMsg::ExitStatus { .. } | ChannelMsg::ExitSignal { .. } => {}
                            _ => {}
                        }
                    }
                    else => break,
                }
            }
            let _ = output_tx
                .send(format!("\r\n[ssh session {task_session_id} closed]\r\n").into_bytes());
        });

        Ok(SshPtyHandle {
            session_id,
            command_tx,
            output_rx,
            registry_release,
        })
    }

    pub async fn test_connection(self) -> Result<(), SshTransportError> {
        let addr = format!("{}:{}", self.config.host, self.config.port);
        let socket_addr = addr
            .to_socket_addrs()
            .map_err(|error| SshTransportError::DnsResolution {
                address: addr.clone(),
                message: error.to_string(),
            })?
            .next()
            .ok_or_else(|| SshTransportError::DnsResolution {
                address: addr.clone(),
                message: "no address found".to_string(),
            })?;

        let client_config = client::Config {
            inactivity_timeout: Some(Duration::from_secs(30)),
            keepalive_interval: Some(Duration::from_secs(15)),
            keepalive_max: 3,
            ..client::Config::default()
        };
        let handler = NativeClientHandler::new(
            self.config.host.clone(),
            self.config.port,
            self.config.strict_host_key_checking,
            self.config.trust_host_key,
            self.config.expected_host_key_fingerprint.clone(),
        );
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect(Arc::new(client_config), socket_addr, handler),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?;

        authenticate(&mut handle, &self.config).await
    }
}

#[derive(Clone)]
struct NativeClientHandler {
    host: String,
    port: u16,
    strict: bool,
    trust_host_key: Option<bool>,
    expected_host_key_fingerprint: Option<String>,
}

impl NativeClientHandler {
    fn new(
        host: String,
        port: u16,
        strict: bool,
        trust_host_key: Option<bool>,
        expected_host_key_fingerprint: Option<String>,
    ) -> Self {
        Self {
            host,
            port,
            strict,
            trust_host_key,
            expected_host_key_fingerprint,
        }
    }
}

impl client::Handler for NativeClientHandler {
    type Error = SshTransportError;

    async fn check_server_key(
        &mut self,
        server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        let actual_fingerprint = public_key_fingerprint(server_public_key);
        if let Some(expected_fingerprint) = self.expected_host_key_fingerprint.as_deref() {
            if expected_fingerprint != actual_fingerprint {
                return Err(SshTransportError::HostKeyChanged {
                    host: self.host.clone(),
                    port: self.port,
                    expected_fingerprint: expected_fingerprint.to_string(),
                    actual_fingerprint,
                });
            }
        }

        match verify_host_key(&self.host, self.port, server_public_key)? {
            HostKeyVerification::Verified => Ok(true),
            HostKeyVerification::Unknown { fingerprint, .. } => {
                if let Some(trust_host_key) = self.trust_host_key {
                    if trust_host_key {
                        learn_host_key(&self.host, self.port, server_public_key)?;
                    }
                    return Ok(true);
                }

                if self.strict {
                    Err(SshTransportError::HostKeyUnknown {
                        host: self.host.clone(),
                        port: self.port,
                        fingerprint,
                    })
                } else {
                    Err(SshTransportError::HostKeyUnknown {
                        host: self.host.clone(),
                        port: self.port,
                        fingerprint,
                    })
                }
            }
            HostKeyVerification::Changed {
                expected_fingerprint,
                actual_fingerprint,
                ..
            } => Err(SshTransportError::HostKeyChanged {
                host: self.host.clone(),
                port: self.port,
                expected_fingerprint,
                actual_fingerprint,
            }),
        }
    }
}

async fn authenticate(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
) -> Result<(), SshTransportError> {
    if let Some(result) = try_none_auth_probe(handle, &config.username).await? {
        if result.success() {
            return Ok(());
        }
    }

    let result = match &config.auth {
        AuthMethod::Password { password } => {
            authenticate_password(handle, config, password).await?
        }
        AuthMethod::Key {
            key_path,
            passphrase,
        } => {
            let key_path = resolve_key_path(key_path)?;
            let key = load_secret_key(
                &key_path,
                passphrase.as_ref().map(|passphrase| passphrase.as_str()),
            )
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
            let key = Arc::new(key);
            let hash_alg = best_rsa_hash(handle).await?;
            handle
                .authenticate_publickey(
                    config.username.clone(),
                    PrivateKeyWithHashAlg::new(key, hash_alg),
                )
                .await
                .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?
        }
        AuthMethod::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => {
            let key_path = resolve_key_path(key_path)?;
            let key = load_secret_key(
                &key_path,
                passphrase.as_ref().map(|passphrase| passphrase.as_str()),
            )
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
            let cert_path = expand_tilde_path(cert_path);
            let cert = load_openssh_certificate(&cert_path)
                .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
            handle
                .authenticate_openssh_cert(config.username.clone(), Arc::new(key), cert)
                .await
                .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?
        }
        AuthMethod::Agent => authenticate_agent(handle, config).await?,
        AuthMethod::KeyboardInteractive => {
            return Err(SshTransportError::UnsupportedAuth(
                "keyboard-interactive requires a native prompt flow",
            ));
        }
    };

    if result.success() {
        Ok(())
    } else {
        Err(SshTransportError::AuthenticationFailed(format!(
            "rejected by server: {result:?}"
        )))
    }
}

async fn try_none_auth_probe(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
) -> Result<Option<client::AuthResult>, SshTransportError> {
    match tokio::time::timeout(Duration::from_secs(5), handle.authenticate_none(username)).await {
        Ok(Ok(result)) => Ok(Some(result)),
        Ok(Err(_)) | Err(_) => Ok(None),
    }
}

async fn authenticate_password(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
    password: &str,
) -> Result<client::AuthResult, SshTransportError> {
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        handle.authenticate_password(config.username.clone(), password),
    )
    .await
    .map_err(|_| {
        SshTransportError::AuthenticationFailed("password authentication timed out".to_string())
    })?
    .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;

    if result.success() {
        return Ok(result);
    }

    Ok(result)
}

async fn best_rsa_hash(
    handle: &client::Handle<NativeClientHandler>,
) -> Result<Option<HashAlg>, SshTransportError> {
    handle
        .best_supported_rsa_hash()
        .await
        .map(|hash| hash.flatten())
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))
}

async fn authenticate_agent(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
) -> Result<client::AuthResult, SshTransportError> {
    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
    if identities.is_empty() {
        return Err(SshTransportError::AuthenticationFailed(
            "SSH agent has no identities".to_string(),
        ));
    }

    let hash_alg = best_rsa_hash(handle).await?;
    let mut last_failure = None;
    for identity in identities {
        let AgentIdentity::PublicKey { key, .. } = identity else {
            continue;
        };
        let result = handle
            .authenticate_publickey_with(config.username.clone(), key, hash_alg, &mut agent)
            .await
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
        if result.success() {
            return Ok(result);
        }
        last_failure = Some(result);
    }

    Ok(last_failure.unwrap_or_else(|| client::AuthResult::Failure {
        remaining_methods: russh::MethodSet::empty(),
        partial_success: false,
    }))
}

fn resolve_key_path(path: &str) -> Result<PathBuf, SshTransportError> {
    if !path.trim().is_empty() {
        return Ok(expand_tilde_path(path));
    }

    default_key_paths()
        .into_iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            SshTransportError::AuthenticationFailed(
                "No default SSH key found in ~/.ssh".to_string(),
            )
        })
}

fn default_key_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::home_dir() else {
        return Vec::new();
    };
    let ssh = home.join(".ssh");
    [
        "id_ed25519",
        "id_ecdsa",
        "id_rsa",
        "id_dsa",
        "id_ed25519_sk",
        "id_ecdsa_sk",
    ]
    .into_iter()
    .map(|name| ssh.join(name))
    .collect()
}

fn expand_tilde_path(path: &str) -> PathBuf {
    if path == "~" {
        return std::env::home_dir().unwrap_or_else(|| PathBuf::from(path));
    }
    if let Some(rest) = path.strip_prefix("~/") {
        if let Some(home) = std::env::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}
