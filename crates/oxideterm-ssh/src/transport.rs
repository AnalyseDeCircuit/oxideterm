// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{net::ToSocketAddrs, sync::Arc, time::Duration};

use russh::{ChannelMsg, Pty, client};
use tokio::sync::{broadcast, mpsc};

use crate::{AuthMethod, SshConfig};

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
    #[error("SSH authentication failed")]
    AuthenticationFailed,
    #[error("SSH authentication method is not implemented in native yet: {0}")]
    UnsupportedAuth(&'static str),
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
            ..client::Config::default()
        };
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect(Arc::new(client_config), socket_addr, NativeClientHandler),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?;

        let authenticated = match &self.config.auth {
            AuthMethod::Password { password } => handle
                .authenticate_password(self.config.username.clone(), password.as_str())
                .await
                .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?,
            AuthMethod::Agent => return Err(SshTransportError::UnsupportedAuth("agent")),
            AuthMethod::Key { .. } => return Err(SshTransportError::UnsupportedAuth("key")),
            AuthMethod::Certificate { .. } => {
                return Err(SshTransportError::UnsupportedAuth("certificate"));
            }
            AuthMethod::KeyboardInteractive => {
                return Err(SshTransportError::UnsupportedAuth("keyboard-interactive"));
            }
        };
        if !authenticated.success() {
            return Err(SshTransportError::AuthenticationFailed);
        }

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
        })
    }
}

#[derive(Clone)]
struct NativeClientHandler;

impl client::Handler for NativeClientHandler {
    type Error = SshTransportError;

    async fn check_server_key(
        &mut self,
        _server_public_key: &russh::keys::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }
}
