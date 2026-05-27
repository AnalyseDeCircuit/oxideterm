// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    future::Future,
    net::{SocketAddr, ToSocketAddrs},
    path::PathBuf,
    pin::Pin,
    sync::Arc,
    time::Duration,
};

use oxideterm_sftp::{SftpChannelOpener, SftpError, SftpExecChannelOpener};
use parking_lot::RwLock;
use russh::{
    AgentAuthError, Channel, ChannelMsg, MethodKind, Pty, Signer as RusshSigner, client,
    keys::{
        Algorithm, Certificate, HashAlg, PrivateKey, PrivateKeyWithHashAlg,
        agent::{
            AgentIdentity,
            client::{AgentClient, AgentStream},
        },
        load_openssh_certificate, load_secret_key,
        ssh_key::private::KeypairData,
    },
};
use signature::Signer as SignatureSigner;
use ssh_encoding::Encode;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Semaphore,
    sync::{Mutex, mpsc},
    time::{Instant, sleep_until, timeout},
};
use zeroize::Zeroizing;

use crate::{
    AuthMethod, ConnectionConsumer, ConnectionState, ConnectionTransportStatus,
    KeepaliveProbeResult, ProxyHopConfig, SshConfig, SshConnectionHandle, SshConnectionRegistry,
    host_key::{
        HostKeyStatus, HostKeyVerification, accept_host_key_for_session, check_host_key,
        check_host_key_via_stream, learn_host_key, public_key_fingerprint, verify_host_key,
    },
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

const NONE_AUTH_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PASSWORD_RETRY_DELAY: Duration = Duration::from_millis(500);
const PASSWORD_AUTH_TIMEOUT: Duration = Duration::from_secs(30);
const KBI_USER_PROMPT_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_PASSWORD_KBI_FALLBACK_ROUNDS: usize = 5;
const RSA_AUTH_ALGORITHMS: [Option<HashAlg>; 3] =
    [Some(HashAlg::Sha512), Some(HashAlg::Sha256), None];
const SSH_COMMAND_CHANNEL_CAPACITY: usize = 1024;
const SSH_OUTPUT_CHANNEL_CAPACITY: usize = 1024;
const SSH_OUTPUT_BATCH_MAX_BYTES: usize = 64 * 1024;
const SSH_OUTPUT_FLUSH_MS: u64 = 4;
const SSH_OUTPUT_INTERACTIVE_FLUSH_MS: u64 = 1;
const SSH_OUTPUT_INTERACTIVE_WINDOW_MS: u64 = 120;
const UTF8_RESIDUAL_MAX_BYTES: usize = 4;
const MAX_PROXY_CHAIN_DEPTH: usize = 32;

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

fn ssh_channel_error_is_transport_lost(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    [
        "connection is closed",
        "connection closed",
        "connection reset",
        "reset by peer",
        "broken pipe",
        "not connected",
        "disconnected",
        "eof",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

pub trait SshForwardStream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> SshForwardStream for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

pub type BoxedSshForwardStream = Box<dyn SshForwardStream>;

pub struct RemoteForwardedTcpIp {
    pub connection_id: String,
    pub connected_address: String,
    pub connected_port: u16,
    pub originator_address: String,
    pub originator_port: u16,
    pub stream: BoxedSshForwardStream,
}

pub trait RemoteForwardHandler: Send + Sync {
    fn handle_remote_forward(
        &self,
        event: RemoteForwardedTcpIp,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

#[derive(Clone)]
struct RemoteForwardRegistration {
    connection_id: String,
    handler: Arc<dyn RemoteForwardHandler>,
}

type RemoteForwardHandlerSlot = Arc<RwLock<Option<RemoteForwardRegistration>>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyChainPreflightChallenge {
    pub step_index: usize,
    pub host: String,
    pub port: u16,
    pub status: HostKeyStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardInteractivePrompt {
    pub prompt: String,
    pub echo: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardInteractivePromptRequest {
    pub flow_id: String,
    pub name: String,
    pub instructions: String,
    pub prompts: Vec<KeyboardInteractivePrompt>,
    pub chained: bool,
}

pub type KeyboardInteractiveResponses = Zeroizing<Vec<String>>;

#[derive(Clone, Debug, thiserror::Error)]
pub enum SshPromptError {
    #[error("keyboard-interactive authentication cancelled")]
    Cancelled,
    #[error("keyboard-interactive authentication timed out")]
    Timeout,
    #[error("keyboard-interactive prompt failed: {0}")]
    Failed(String),
}

pub trait SshPromptHandler: Send + Sync {
    fn keyboard_interactive(
        &self,
        request: KeyboardInteractivePromptRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<KeyboardInteractiveResponses, SshPromptError>> + Send + '_>,
    >;
}

pub struct SshPtyHandle {
    pub session_id: String,
    pub command_tx: mpsc::Sender<SshTransportCommand>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,
    ssh_connection: Option<SshConnectionHandle>,
    registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
}

pub struct SshShellChannel {
    channel: Channel<client::Msg>,
}

impl SshShellChannel {
    pub async fn sample_until(
        &mut self,
        command: &str,
        end_marker: &str,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<String, SshTransportError> {
        self.channel
            .data(command.as_bytes())
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;

        let mut output = Vec::new();
        tokio::time::timeout(timeout, async {
            loop {
                match self.channel.wait().await {
                    Some(ChannelMsg::Data { data }) => {
                        output.extend_from_slice(&data);
                        if output.len() > max_output_size {
                            output.truncate(max_output_size);
                            break;
                        }
                        if let Ok(text) = std::str::from_utf8(&output)
                            && text.contains(end_marker)
                        {
                            break;
                        }
                    }
                    Some(ChannelMsg::ExtendedData { .. }) => {}
                    Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) => {
                        return Err(SshTransportError::Channel(
                            "persistent shell channel closed".to_string(),
                        ));
                    }
                    Some(_) => {}
                    None => {
                        return Err(SshTransportError::Channel(
                            "persistent shell channel ended".to_string(),
                        ));
                    }
                }
            }
            Ok(())
        })
        .await
        .map_err(|_| SshTransportError::Timeout)??;

        String::from_utf8(output).map_err(|error| {
            SshTransportError::Channel(format!("remote shell output was not UTF-8: {error}"))
        })
    }

    pub async fn close(&mut self) -> Result<(), SshTransportError> {
        self.channel
            .close()
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))
    }
}

impl SshPtyHandle {
    pub fn ssh_connection_handle(&self) -> Option<SshConnectionHandle> {
        self.ssh_connection.clone()
    }
}

impl Drop for SshPtyHandle {
    fn drop(&mut self) {
        if let Some((registry, connection_id, consumer)) = self.registry_release.take() {
            registry.release(&connection_id, &consumer);
        }
    }
}

#[derive(Clone)]
pub struct SshTransportClient {
    config: SshConfig,
    prompt_handler: Option<Arc<dyn SshPromptHandler>>,
}

include!("transport/connection.rs");
include!("transport/signers.rs");
include!("transport/output.rs");
include!("transport/client.rs");
include!("transport/handler.rs");
include!("transport/auth.rs");
include!("transport/paths.rs");

#[cfg(test)]
mod transport_lost_tests {
    use super::ssh_channel_error_is_transport_lost;

    #[test]
    fn channel_error_classifier_matches_idle_closed_transport() {
        assert!(ssh_channel_error_is_transport_lost(
            "SSH channel error: Connection is closed"
        ));
        assert!(ssh_channel_error_is_transport_lost(
            "write failed: broken pipe"
        ));
        assert!(ssh_channel_error_is_transport_lost("client disconnected"));
        assert!(!ssh_channel_error_is_transport_lost(
            "server refused PTY allocation"
        ));
    }
}
