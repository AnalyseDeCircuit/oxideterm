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
    time::{Instant, sleep_until},
};
use zeroize::Zeroizing;

use crate::{
    AuthMethod, ConnectionConsumer, ConnectionState, ProxyHopConfig, SshConfig,
    SshConnectionHandle, SshConnectionRegistry,
    host_key::{
        HostKeyStatus, HostKeyVerification, check_host_key, check_host_key_via_stream,
        learn_host_key, public_key_fingerprint, verify_host_key,
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

pub trait SshForwardStream: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> SshForwardStream for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

pub type BoxedSshForwardStream = Box<dyn SshForwardStream>;

pub struct RemoteForwardedTcpIp {
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

type RemoteForwardHandlerSlot = Arc<RwLock<Option<Arc<dyn RemoteForwardHandler>>>>;

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
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, SshPromptError>> + Send + '_>>;
}

pub struct SshPtyHandle {
    pub session_id: String,
    pub command_tx: mpsc::Sender<SshTransportCommand>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,
    ssh_connection: Option<SshConnectionHandle>,
    registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
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

struct PooledSshConnection {
    target: Mutex<client::Handle<NativeClientHandler>>,
    _jump_handles: Vec<client::Handle<NativeClientHandler>>,
    remote_forward_handler: RemoteForwardHandlerSlot,
}

impl PooledSshConnection {
    fn direct(
        handle: client::Handle<NativeClientHandler>,
        remote_forward_handler: RemoteForwardHandlerSlot,
    ) -> Self {
        Self {
            target: Mutex::new(handle),
            _jump_handles: Vec::new(),
            remote_forward_handler,
        }
    }

    fn tunneled(
        target: client::Handle<NativeClientHandler>,
        jump_handles: Vec<client::Handle<NativeClientHandler>>,
        remote_forward_handler: RemoteForwardHandlerSlot,
    ) -> Self {
        Self {
            target: Mutex::new(target),
            _jump_handles: jump_handles,
            remote_forward_handler,
        }
    }

    async fn is_closed(&self) -> bool {
        self.target.lock().await.is_closed()
    }
}

impl SshConnectionHandle {
    pub async fn open_direct_tcpip(
        &self,
        host: &str,
        port: u16,
        origin_host: &str,
        origin_port: u16,
    ) -> Result<BoxedSshForwardStream, SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for port forwarding".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot open a port forward".to_string(),
            ));
        }

        let handle = pooled.target.lock().await;
        let stream =
            open_direct_tcpip_stream_with_origin(&handle, host, port, origin_host, origin_port)
                .await?;
        Ok(Box::new(stream))
    }

    pub async fn request_remote_tcpip_forward(
        &self,
        bind_address: &str,
        bind_port: u16,
    ) -> Result<u16, SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote port forwarding".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot request remote port forwarding".to_string(),
            ));
        }

        let handle = pooled.target.lock().await;
        handle
            .tcpip_forward(bind_address, bind_port as u32)
            .await
            .map(|port| port as u16)
            .map_err(|error| {
                SshTransportError::ConnectionFailed(format!(
                    "failed to request remote port forward {bind_address}:{bind_port}: {error}"
                ))
            })
    }

    pub async fn cancel_remote_tcpip_forward(
        &self,
        bind_address: &str,
        bind_port: u16,
    ) -> Result<(), SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote port forwarding".to_string(),
            ));
        };
        let handle = pooled.target.lock().await;
        handle
            .cancel_tcpip_forward(bind_address, bind_port as u32)
            .await
            .map_err(|error| {
                SshTransportError::ConnectionFailed(format!(
                    "failed to cancel remote port forward {bind_address}:{bind_port}: {error}"
                ))
            })
    }

    pub async fn run_command(
        &self,
        command: &str,
        timeout: Duration,
        max_output_size: usize,
    ) -> Result<String, SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote command execution".to_string(),
            ));
        };
        if pooled.is_closed().await {
            return Err(SshTransportError::ConnectionFailed(
                "SSH connection is closed and cannot execute remote commands".to_string(),
            ));
        }

        let mut channel = {
            let handle = pooled.target.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|error| SshTransportError::Channel(error.to_string()))?
        };
        channel
            .exec(true, command)
            .await
            .map_err(|error| SshTransportError::Channel(error.to_string()))?;

        let mut output = Vec::new();
        let mut exit_status = None;
        tokio::time::timeout(timeout, async {
            while let Some(message) = channel.wait().await {
                match message {
                    ChannelMsg::Data { data } => {
                        output.extend_from_slice(&data);
                    }
                    ChannelMsg::ExtendedData { data, ext } if ext == 1 => {
                        output.extend_from_slice(&data);
                    }
                    ChannelMsg::ExitStatus {
                        exit_status: status,
                    } => {
                        exit_status = Some(status);
                    }
                    ChannelMsg::Eof | ChannelMsg::Close => break,
                    _ => {}
                }
                if output.len() > max_output_size {
                    output.truncate(max_output_size);
                    break;
                }
            }
        })
        .await
        .map_err(|_| SshTransportError::Timeout)?;
        let _ = channel.close().await;

        if let Some(status) = exit_status
            && status != 0
        {
            return Err(SshTransportError::Channel(format!(
                "remote command exited with status {status}"
            )));
        }

        String::from_utf8(output).map_err(|error| {
            SshTransportError::Channel(format!("remote command output was not UTF-8: {error}"))
        })
    }

    pub fn set_remote_forward_handler(
        &self,
        handler: Arc<dyn RemoteForwardHandler>,
    ) -> Result<(), SshTransportError> {
        let Some(pooled) = self.physical::<PooledSshConnection>() else {
            return Err(SshTransportError::ConnectionFailed(
                "no active SSH connection is available for remote port forwarding".to_string(),
            ));
        };
        *pooled.remote_forward_handler.write() = Some(handler);
        Ok(())
    }

    pub fn clear_remote_forward_handler(&self) {
        if let Some(pooled) = self.physical::<PooledSshConnection>() {
            *pooled.remote_forward_handler.write() = None;
        }
    }
}

type NativeAgentClient = AgentClient<Box<dyn AgentStream + Send + Unpin + 'static>>;

struct AgentSigner<'a> {
    agent: &'a mut NativeAgentClient,
}

impl RusshSigner for AgentSigner<'_> {
    type Error = AgentAuthError;

    fn auth_sign(
        &mut self,
        key: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        to_sign: Vec<u8>,
    ) -> impl std::future::Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let key_owned = key.clone();
        async move {
            self.agent
                .sign_request(&key_owned, hash_alg, to_sign)
                .await
                .map_err(Into::into)
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum LocalSignerError {
    #[error(transparent)]
    Send(#[from] russh::SendError),
    #[error("{0}")]
    Sign(String),
}

struct LocalKeySigner {
    key: Arc<PrivateKey>,
}

impl LocalKeySigner {
    fn new(key: Arc<PrivateKey>) -> Self {
        Self { key }
    }
}

impl RusshSigner for LocalKeySigner {
    type Error = LocalSignerError;

    fn auth_sign(
        &mut self,
        _key: &AgentIdentity,
        hash_alg: Option<HashAlg>,
        to_sign: Vec<u8>,
    ) -> impl Future<Output = Result<Vec<u8>, Self::Error>> + Send {
        let key = Arc::clone(&self.key);
        async move { sign_auth_payload_with_hash_alg(key.as_ref(), hash_alg, to_sign) }
    }
}

struct SshOutputBatcher {
    pending: Vec<u8>,
    utf8_guard: RawUtf8ResidualGuard,
    flush_deadline: Option<Instant>,
    interactive_until: Option<Instant>,
}

impl SshOutputBatcher {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
            utf8_guard: RawUtf8ResidualGuard::default(),
            flush_deadline: None,
            interactive_until: None,
        }
    }

    fn note_interaction(&mut self) {
        self.interactive_until =
            Some(Instant::now() + Duration::from_millis(SSH_OUTPUT_INTERACTIVE_WINDOW_MS));
        self.refresh_deadline();
    }

    fn push(&mut self, bytes: &[u8]) -> bool {
        if let Some(guarded) = self.utf8_guard.push(bytes) {
            self.pending.extend_from_slice(&guarded);
        }
        self.refresh_deadline();
        self.pending.len() >= SSH_OUTPUT_BATCH_MAX_BYTES
    }

    fn flush_due(&self) -> Option<Instant> {
        (!self.pending.is_empty())
            .then_some(self.flush_deadline?)
            .or(None)
    }

    fn take_flush(&mut self) -> Option<Vec<u8>> {
        if self.pending.is_empty() {
            self.flush_deadline = None;
            return None;
        }
        self.flush_deadline = None;
        Some(std::mem::take(&mut self.pending))
    }

    fn take_final_flush(&mut self) -> Option<Vec<u8>> {
        if let Some(residual) = self.utf8_guard.flush() {
            self.pending.extend_from_slice(&residual);
        }
        self.take_flush()
    }

    fn refresh_deadline(&mut self) {
        if self.pending.is_empty() {
            self.flush_deadline = None;
            return;
        }

        let now = Instant::now();
        let interactive = self
            .interactive_until
            .is_some_and(|deadline| deadline > now);
        let delay = if interactive {
            SSH_OUTPUT_INTERACTIVE_FLUSH_MS
        } else {
            SSH_OUTPUT_FLUSH_MS
        };
        self.flush_deadline = Some(now + Duration::from_millis(delay));
    }
}

#[derive(Default)]
struct RawUtf8ResidualGuard {
    residual: Vec<u8>,
}

impl RawUtf8ResidualGuard {
    fn push(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        if bytes.is_empty() && self.residual.is_empty() {
            return None;
        }

        let mut combined = Vec::with_capacity(self.residual.len() + bytes.len());
        combined.extend_from_slice(&self.residual);
        combined.extend_from_slice(bytes);
        self.residual.clear();

        let split = split_before_incomplete_utf8_tail(&combined);
        if split < combined.len() {
            self.residual.extend_from_slice(&combined[split..]);
            combined.truncate(split);
        }

        if self.residual.len() >= UTF8_RESIDUAL_MAX_BYTES {
            combined.extend_from_slice(&self.residual);
            self.residual.clear();
        }

        (!combined.is_empty()).then_some(combined)
    }

    fn flush(&mut self) -> Option<Vec<u8>> {
        (!self.residual.is_empty()).then(|| std::mem::take(&mut self.residual))
    }
}

fn split_before_incomplete_utf8_tail(bytes: &[u8]) -> usize {
    let len = bytes.len();
    let max_tail = len.min(UTF8_RESIDUAL_MAX_BYTES - 1);

    for tail_len in 1..=max_tail {
        let start = len - tail_len;
        let first = bytes[start];
        let width = utf8_char_width(first);
        if width == 0 {
            continue;
        }

        if width > tail_len
            && bytes[start + 1..]
                .iter()
                .all(|byte| is_utf8_continuation(*byte))
        {
            return start;
        }

        break;
    }

    len
}

fn utf8_char_width(byte: u8) -> usize {
    match byte {
        0x00..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 0,
    }
}

fn is_utf8_continuation(byte: u8) -> bool {
    (0x80..=0xbf).contains(&byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_utf8_guard_keeps_incomplete_scalar_tail() {
        let mut guard = RawUtf8ResidualGuard::default();

        assert_eq!(guard.push(&[0xe4, 0xbd]), None);
        assert_eq!(guard.push(&[0xa0]), Some("你".as_bytes().to_vec()));
    }

    #[test]
    fn raw_utf8_guard_flushes_invalid_bytes_unchanged() {
        let mut guard = RawUtf8ResidualGuard::default();

        assert_eq!(guard.push(&[0xff, b'a']), Some(vec![0xff, b'a']));
    }

    #[test]
    fn output_batcher_holds_utf8_tail_until_final_flush() {
        let mut batcher = SshOutputBatcher::new();

        assert!(!batcher.push(&[0xe4, 0xbd]));
        assert_eq!(batcher.take_flush(), None);
        assert_eq!(batcher.take_final_flush(), Some(vec![0xe4, 0xbd]));
    }

    #[test]
    fn output_batcher_flushes_complete_text() {
        let mut batcher = SshOutputBatcher::new();

        assert!(!batcher.push(b"abc"));
        assert_eq!(batcher.take_flush(), Some(b"abc".to_vec()));
    }
}

impl SshTransportClient {
    pub fn new(config: SshConfig) -> Self {
        Self {
            config,
            prompt_handler: None,
        }
    }

    pub fn with_prompt_handler(mut self, prompt_handler: Arc<dyn SshPromptHandler>) -> Self {
        self.prompt_handler = Some(prompt_handler);
        self
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

        let pooled = if let Some(existing) = connection.physical::<PooledSshConnection>() {
            if existing.is_closed().await {
                connection.clear_physical();
                match self.connect_authenticated_connection().await {
                    Ok(pooled) => {
                        connection.set_physical(pooled.clone());
                        pooled
                    }
                    Err(error) => {
                        let _ = registry
                            .mark_state(&connection_id, ConnectionState::Error(error.to_string()));
                        registry.release(&connection_id, &consumer);
                        return Err(error);
                    }
                }
            } else {
                existing
            }
        } else {
            match self.connect_authenticated_connection().await {
                Ok(pooled) => {
                    connection.set_physical(pooled.clone());
                    pooled
                }
                Err(error) => {
                    let _ = registry
                        .mark_state(&connection_id, ConnectionState::Error(error.to_string()));
                    registry.release(&connection_id, &consumer);
                    return Err(error);
                }
            }
        };

        let result = self
            .open_shell_from_pooled(
                pooled,
                Some((registry.clone(), connection_id.clone(), consumer.clone())),
                Some(connection.clone()),
            )
            .await;

        match &result {
            Ok(_) => {
                let _ = registry.mark_state(&connection_id, ConnectionState::Active);
            }
            Err(error) => {
                connection.clear_physical();
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
        let pooled = self.connect_authenticated_connection().await?;
        self.open_shell_from_pooled(pooled, registry_release, None)
            .await
    }

    async fn connect_authenticated_connection(
        &self,
    ) -> Result<Arc<PooledSshConnection>, SshTransportError> {
        let remote_forward_handler = Arc::new(RwLock::new(None));
        if self
            .config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            return self
                .connect_authenticated_proxy_connection(remote_forward_handler)
                .await;
        }

        self.connect_direct_authenticated_handle(&self.config, remote_forward_handler.clone())
            .await
            .map(|handle| PooledSshConnection::direct(handle, remote_forward_handler))
            .map(Arc::new)
    }

    async fn connect_direct_authenticated_handle(
        &self,
        config: &SshConfig,
        remote_forward_handler: RemoteForwardHandlerSlot,
    ) -> Result<client::Handle<NativeClientHandler>, SshTransportError> {
        let socket_addr = resolve_socket_addr(&config.host, config.port)?;

        let client_config = ssh_client_config();
        let handler = NativeClientHandler::new(
            config.host.clone(),
            config.port,
            config.strict_host_key_checking,
            config.trust_host_key,
            config.expected_host_key_fingerprint.clone(),
            config.agent_forwarding,
            remote_forward_handler,
        );
        let mut handle = tokio::time::timeout(
            Duration::from_secs(config.timeout_secs),
            client::connect(Arc::new(client_config), socket_addr, handler),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?;

        authenticate(&mut handle, config, self.prompt_handler.as_deref()).await?;
        Ok(handle)
    }

    async fn connect_authenticated_proxy_connection(
        &self,
        remote_forward_handler: RemoteForwardHandlerSlot,
    ) -> Result<Arc<PooledSshConnection>, SshTransportError> {
        let chain = self.config.proxy_chain.as_deref().unwrap_or_default();
        if chain.is_empty() {
            return Err(SshTransportError::ConnectionFailed(
                "proxy chain is empty".to_string(),
            ));
        }

        let mut current_stream: Option<russh::ChannelStream<client::Msg>> = None;
        let mut jump_handles = Vec::with_capacity(chain.len());

        for (index, hop) in chain.iter().enumerate() {
            let handle = if let Some(stream) = current_stream.take() {
                self.connect_proxy_hop_via_stream(hop, stream).await?
            } else {
                self.connect_proxy_hop_direct(hop).await?
            };

            let (next_host, next_port) = if let Some(next_hop) = chain.get(index + 1) {
                (next_hop.host.as_str(), next_hop.port)
            } else {
                (self.config.host.as_str(), self.config.port)
            };
            let channel = handle
                .channel_open_direct_tcpip(next_host, next_port as u32, "127.0.0.1", 0)
                .await
                .map_err(|error| {
                    SshTransportError::ConnectionFailed(format!(
                        "failed to open proxy tunnel to {next_host}:{next_port}: {error}"
                    ))
                })?;
            current_stream = Some(channel.into_stream());
            jump_handles.push(handle);
        }

        let stream = current_stream.ok_or_else(|| {
            SshTransportError::ConnectionFailed(
                "no proxy stream available for target connection".to_string(),
            )
        })?;
        let target = self
            .connect_target_via_proxy_stream(
                stream,
                self.config.timeout_secs,
                remote_forward_handler.clone(),
            )
            .await?;
        Ok(Arc::new(PooledSshConnection::tunneled(
            target,
            jump_handles,
            remote_forward_handler,
        )))
    }

    async fn connect_proxy_hop_direct(
        &self,
        hop: &ProxyHopConfig,
    ) -> Result<client::Handle<NativeClientHandler>, SshTransportError> {
        let socket_addr = resolve_socket_addr(&hop.host, hop.port)?;
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect(
                Arc::new(ssh_client_config()),
                socket_addr,
                proxy_hop_handler(hop),
            ),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| SshTransportError::ConnectionFailed(error.to_string()))?;

        authenticate_proxy_hop(&mut handle, hop).await?;
        Ok(handle)
    }

    async fn connect_proxy_hop_via_stream(
        &self,
        hop: &ProxyHopConfig,
        stream: russh::ChannelStream<client::Msg>,
    ) -> Result<client::Handle<NativeClientHandler>, SshTransportError> {
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect_stream(
                Arc::new(ssh_client_config()),
                stream,
                proxy_hop_handler(hop),
            ),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| {
            SshTransportError::ConnectionFailed(format!(
                "failed to connect via proxy stream to {}:{}: {error}",
                hop.host, hop.port
            ))
        })?;

        authenticate_proxy_hop(&mut handle, hop).await?;
        Ok(handle)
    }

    async fn connect_target_via_proxy_stream(
        &self,
        stream: russh::ChannelStream<client::Msg>,
        timeout_secs: u64,
        remote_forward_handler: RemoteForwardHandlerSlot,
    ) -> Result<client::Handle<NativeClientHandler>, SshTransportError> {
        let handler = NativeClientHandler::new(
            self.config.host.clone(),
            self.config.port,
            self.config.strict_host_key_checking,
            self.config.trust_host_key,
            self.config.expected_host_key_fingerprint.clone(),
            self.config.agent_forwarding,
            remote_forward_handler,
        );
        let mut handle = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            client::connect_stream(Arc::new(ssh_client_config()), stream, handler),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| {
            SshTransportError::ConnectionFailed(format!(
                "failed to connect to target via proxy stream: {error}"
            ))
        })?;

        authenticate(&mut handle, &self.config, self.prompt_handler.as_deref()).await?;
        Ok(handle)
    }

    async fn open_shell_from_pooled(
        self,
        pooled: Arc<PooledSshConnection>,
        registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
        ssh_connection: Option<SshConnectionHandle>,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let mut channel = {
            let handle = pooled.target.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|error| SshTransportError::Channel(error.to_string()))?
        };
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
        let (command_tx, mut command_rx) =
            mpsc::channel::<SshTransportCommand>(SSH_COMMAND_CHANNEL_CAPACITY);
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(SSH_OUTPUT_CHANNEL_CAPACITY);
        let task_session_id = session_id.clone();

        tokio::spawn(async move {
            let mut output_batcher = SshOutputBatcher::new();
            loop {
                let flush_deadline = output_batcher.flush_due();
                tokio::select! {
                    _ = async move {
                        if let Some(deadline) = flush_deadline {
                            sleep_until(deadline).await;
                        } else {
                            std::future::pending::<()>().await;
                        }
                    } => {
                        if let Some(bytes) = output_batcher.take_flush()
                            && output_tx.send(bytes).await.is_err()
                        {
                            break;
                        }
                    }
                    Some(command) = command_rx.recv() => {
                        match command {
                            SshTransportCommand::Data(data) => {
                                output_batcher.note_interaction();
                                if channel.data(data.as_slice()).await.is_err() {
                                    break;
                                }
                            }
                            SshTransportCommand::Resize { cols, rows } => {
                                output_batcher.note_interaction();
                                let _ = channel.window_change(cols as u32, rows as u32, 0, 0).await;
                            }
                            SshTransportCommand::Close => {
                                if let Some(bytes) = output_batcher.take_final_flush() {
                                    let _ = output_tx.send(bytes).await;
                                }
                                let _ = channel.eof().await;
                                break;
                            }
                        }
                    }
                    Some(message) = channel.wait() => {
                        match message {
                            ChannelMsg::Data { data } => {
                                if output_batcher.push(&data)
                                    && let Some(bytes) = output_batcher.take_flush()
                                    && output_tx.send(bytes).await.is_err()
                                {
                                    break;
                                }
                            }
                            ChannelMsg::ExtendedData { data, ext } if ext == 1 => {
                                if output_batcher.push(&data)
                                    && let Some(bytes) = output_batcher.take_flush()
                                    && output_tx.send(bytes).await.is_err()
                                {
                                    break;
                                }
                            }
                            ChannelMsg::Eof | ChannelMsg::Close => {
                                if let Some(bytes) = output_batcher.take_final_flush() {
                                    let _ = output_tx.send(bytes).await;
                                }
                                break;
                            }
                            ChannelMsg::ExitStatus { .. } | ChannelMsg::ExitSignal { .. } => {}
                            _ => {}
                        }
                    }
                    else => break,
                }
            }
            if let Some(bytes) = output_batcher.take_final_flush() {
                let _ = output_tx.send(bytes).await;
            }
            let _ = output_tx
                .send(format!("\r\n[ssh session {task_session_id} closed]\r\n").into_bytes())
                .await;
        });

        Ok(SshPtyHandle {
            session_id,
            command_tx,
            output_rx,
            ssh_connection,
            registry_release,
        })
    }

    pub async fn test_connection(self) -> Result<(), SshTransportError> {
        self.connect_authenticated_connection().await.map(|_| ())
    }

    pub async fn preflight_proxy_chain(
        self,
    ) -> Result<Option<ProxyChainPreflightChallenge>, SshTransportError> {
        let chain = self.config.proxy_chain.as_deref().unwrap_or_default();
        if chain.is_empty() {
            return Ok(None);
        }

        let mut current_handle: Option<client::Handle<NativeClientHandler>> = None;
        let mut jump_handles = Vec::with_capacity(chain.len());

        for (index, hop) in chain.iter().enumerate() {
            if !proxy_step_has_accepted_fingerprint(hop) {
                let status = if let Some(parent) = current_handle.as_ref() {
                    let stream = open_direct_tcpip_stream(parent, &hop.host, hop.port).await?;
                    check_host_key_via_stream(&hop.host, hop.port, stream, self.config.timeout_secs)
                        .await
                } else {
                    check_host_key(&hop.host, hop.port, self.config.timeout_secs).await
                };
                if status != HostKeyStatus::Verified {
                    return Ok(Some(ProxyChainPreflightChallenge {
                        step_index: index,
                        host: hop.host.clone(),
                        port: hop.port,
                        status,
                    }));
                }
            }

            let handle = if let Some(parent) = current_handle.take() {
                let stream = open_direct_tcpip_stream(&parent, &hop.host, hop.port).await?;
                jump_handles.push(parent);
                self.connect_proxy_hop_via_stream(hop, stream).await?
            } else {
                self.connect_proxy_hop_direct(hop).await?
            };
            current_handle = Some(handle);
        }

        let target_index = chain.len();
        if !target_step_has_accepted_fingerprint(&self.config) {
            let status = if let Some(parent) = current_handle.as_ref() {
                let stream =
                    open_direct_tcpip_stream(parent, &self.config.host, self.config.port).await?;
                check_host_key_via_stream(
                    &self.config.host,
                    self.config.port,
                    stream,
                    self.config.timeout_secs,
                )
                .await
            } else {
                check_host_key(
                    &self.config.host,
                    self.config.port,
                    self.config.timeout_secs,
                )
                .await
            };
            if status != HostKeyStatus::Verified {
                return Ok(Some(ProxyChainPreflightChallenge {
                    step_index: target_index,
                    host: self.config.host.clone(),
                    port: self.config.port,
                    status,
                }));
            }
        }

        if let Some(handle) = current_handle {
            jump_handles.push(handle);
        }
        drop(jump_handles);
        Ok(None)
    }
}

fn ssh_client_config() -> client::Config {
    client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        ..client::Config::default()
    }
}

async fn open_direct_tcpip_stream(
    handle: &client::Handle<NativeClientHandler>,
    host: &str,
    port: u16,
) -> Result<russh::ChannelStream<client::Msg>, SshTransportError> {
    open_direct_tcpip_stream_with_origin(handle, host, port, "127.0.0.1", 0).await
}

async fn open_direct_tcpip_stream_with_origin(
    handle: &client::Handle<NativeClientHandler>,
    host: &str,
    port: u16,
    origin_host: &str,
    origin_port: u16,
) -> Result<russh::ChannelStream<client::Msg>, SshTransportError> {
    handle
        .channel_open_direct_tcpip(host, port as u32, origin_host, origin_port as u32)
        .await
        .map(|channel| channel.into_stream())
        .map_err(|error| {
            SshTransportError::ConnectionFailed(format!(
                "failed to open proxy tunnel to {host}:{port}: {error}"
            ))
        })
}

fn proxy_step_has_accepted_fingerprint(hop: &ProxyHopConfig) -> bool {
    hop.trust_host_key.is_some() && hop.expected_host_key_fingerprint.is_some()
}

fn target_step_has_accepted_fingerprint(config: &SshConfig) -> bool {
    config.trust_host_key.is_some() && config.expected_host_key_fingerprint.is_some()
}

fn resolve_socket_addr(host: &str, port: u16) -> Result<SocketAddr, SshTransportError> {
    let addr = format!("{host}:{port}");
    addr.to_socket_addrs()
        .map_err(|error| SshTransportError::DnsResolution {
            address: addr.clone(),
            message: error.to_string(),
        })?
        .next()
        .ok_or_else(|| SshTransportError::DnsResolution {
            address: addr,
            message: "no address found".to_string(),
        })
}

fn proxy_hop_handler(hop: &ProxyHopConfig) -> NativeClientHandler {
    NativeClientHandler::new(
        hop.host.clone(),
        hop.port,
        hop.strict_host_key_checking,
        hop.trust_host_key,
        hop.expected_host_key_fingerprint.clone(),
        hop.agent_forwarding,
        Arc::new(RwLock::new(None)),
    )
}

async fn authenticate_proxy_hop(
    handle: &mut client::Handle<NativeClientHandler>,
    hop: &ProxyHopConfig,
) -> Result<(), SshTransportError> {
    if matches!(hop.auth, AuthMethod::KeyboardInteractive) {
        return Err(SshTransportError::UnsupportedAuth(
            "keyboard-interactive authentication is not supported for proxy chain hops",
        ));
    }

    let config = SshConfig {
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        auth: hop.auth.clone(),
        strict_host_key_checking: hop.strict_host_key_checking,
        trust_host_key: hop.trust_host_key,
        expected_host_key_fingerprint: hop.expected_host_key_fingerprint.clone(),
        agent_forwarding: hop.agent_forwarding,
        ..SshConfig::default()
    };
    authenticate(handle, &config, None).await
}

#[derive(Clone)]
struct NativeClientHandler {
    host: String,
    port: u16,
    strict: bool,
    trust_host_key: Option<bool>,
    expected_host_key_fingerprint: Option<String>,
    agent_forwarding_requested: bool,
    agent_forward_semaphore: Arc<Semaphore>,
    remote_forward_handler: RemoteForwardHandlerSlot,
}

impl NativeClientHandler {
    fn new(
        host: String,
        port: u16,
        strict: bool,
        trust_host_key: Option<bool>,
        expected_host_key_fingerprint: Option<String>,
        agent_forwarding_requested: bool,
        remote_forward_handler: RemoteForwardHandlerSlot,
    ) -> Self {
        Self {
            host,
            port,
            strict,
            trust_host_key,
            expected_host_key_fingerprint,
            agent_forwarding_requested,
            agent_forward_semaphore: Arc::new(Semaphore::new(16)),
            remote_forward_handler,
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
                    learn_host_key(&self.host, self.port, server_public_key)?;
                    Ok(true)
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

    async fn server_channel_open_agent_forward(
        &mut self,
        channel: Channel<client::Msg>,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        if !self.agent_forwarding_requested {
            let _ = channel.eof().await;
            return Ok(());
        }

        let Ok(permit) = self.agent_forward_semaphore.clone().try_acquire_owned() else {
            let _ = channel.eof().await;
            return Ok(());
        };

        tokio::spawn(async move {
            handle_agent_forward_channel(channel).await;
            drop(permit);
        });
        Ok(())
    }

    async fn server_channel_open_forwarded_tcpip(
        &mut self,
        channel: Channel<client::Msg>,
        connected_address: &str,
        connected_port: u32,
        originator_address: &str,
        originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let Some(handler) = self.remote_forward_handler.read().clone() else {
            let _ = channel.eof().await;
            return Ok(());
        };

        let event = RemoteForwardedTcpIp {
            connected_address: connected_address.to_string(),
            connected_port: connected_port as u16,
            originator_address: originator_address.to_string(),
            originator_port: originator_port as u16,
            stream: Box::new(channel.into_stream()),
        };
        tokio::spawn(async move {
            handler.handle_remote_forward(event).await;
        });
        Ok(())
    }
}

async fn authenticate(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
    prompt_handler: Option<&dyn SshPromptHandler>,
) -> Result<(), SshTransportError> {
    if let Some(result) = try_none_auth_probe(handle, &config.username).await
        && result.success()
    {
        return Ok(());
    }

    let result = match &config.auth {
        AuthMethod::Password { password } => {
            if password.trim().is_empty() {
                return Err(SshTransportError::AuthenticationFailed(
                    "password is empty".to_string(),
                ));
            }
            let result = authenticate_password(handle, config, password).await?;
            if try_password_as_keyboard_interactive(
                handle,
                config,
                password,
                &result,
                prompt_handler,
            )
            .await?
            {
                return Ok(());
            }
            result
        }
        AuthMethod::Key {
            key_path,
            passphrase,
        } => {
            let key = load_private_key_material(
                key_path,
                passphrase.as_ref().map(|passphrase| passphrase.as_str()),
            )?;
            authenticate_publickey_best_algo(handle, &config.username, key).await?
        }
        AuthMethod::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => {
            let (key, cert) = load_certificate_auth_material(
                key_path,
                cert_path,
                passphrase.as_ref().map(|passphrase| passphrase.as_str()),
            )?;
            authenticate_certificate_best_algo(handle, &config.username, key, cert).await?
        }
        AuthMethod::Agent => authenticate_agent(handle, config).await?,
        AuthMethod::KeyboardInteractive => {
            authenticate_keyboard_interactive(handle, &config.username, prompt_handler).await?
        }
    };

    if result.success() {
        Ok(())
    } else if try_keyboard_interactive_chain(handle, &config.username, &result, prompt_handler)
        .await?
    {
        Ok(())
    } else {
        Err(SshTransportError::AuthenticationFailed(
            authentication_failure_message(&result),
        ))
    }
}

async fn try_none_auth_probe(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
) -> Option<client::AuthResult> {
    match tokio::time::timeout(NONE_AUTH_PROBE_TIMEOUT, handle.authenticate_none(username)).await {
        Ok(Ok(result)) => Some(result),
        Ok(Err(_)) | Err(_) => None,
    }
}

async fn authenticate_password(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
    password: &str,
) -> Result<client::AuthResult, SshTransportError> {
    let result = tokio::time::timeout(
        PASSWORD_AUTH_TIMEOUT,
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

    if should_retry_password_auth(&result) {
        tokio::time::sleep(PASSWORD_RETRY_DELAY).await;
        tokio::time::timeout(
            PASSWORD_AUTH_TIMEOUT,
            handle.authenticate_password(config.username.clone(), password),
        )
        .await
        .map_err(|_| {
            SshTransportError::AuthenticationFailed(
                "password authentication retry timed out".to_string(),
            )
        })?
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))
    } else {
        Ok(result)
    }
}

fn should_retry_password_auth(result: &client::AuthResult) -> bool {
    matches!(
        result,
        client::AuthResult::Failure {
            partial_success: false,
            ..
        }
    )
}

async fn try_password_as_keyboard_interactive(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
    password: &str,
    password_result: &client::AuthResult,
    prompt_handler: Option<&dyn SshPromptHandler>,
) -> Result<bool, SshTransportError> {
    let client::AuthResult::Failure {
        partial_success: false,
        remaining_methods,
    } = password_result
    else {
        return Ok(false);
    };
    if !remaining_methods.contains(&MethodKind::KeyboardInteractive)
        || remaining_methods.contains(&MethodKind::Password)
    {
        return Ok(false);
    }

    let mut password_prompt_consumed = false;
    let mut response = tokio::time::timeout(
        PASSWORD_AUTH_TIMEOUT,
        handle.authenticate_keyboard_interactive_start(config.username.clone(), None::<String>),
    )
    .await
    .map_err(|_| {
        SshTransportError::AuthenticationFailed(
            "keyboard-interactive password fallback timed out".to_string(),
        )
    })?
    .map_err(|error| {
        SshTransportError::AuthenticationFailed(format!(
            "keyboard-interactive password fallback failed: {error}"
        ))
    })?;

    loop {
        match response {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                let replies = if prompts.is_empty() {
                    Vec::new()
                } else if !password_prompt_consumed
                    && prompts.len() == 1
                    && !prompts[0].echo
                    && prompt_looks_like_password(&prompts[0].prompt)
                {
                    password_prompt_consumed = true;
                    vec![password.to_string()]
                } else {
                    let Some(prompt_handler) = prompt_handler else {
                        return Ok(false);
                    };
                    return continue_keyboard_interactive_flow(
                        handle,
                        prompt_handler,
                        client::KeyboardInteractiveAuthResponse::InfoRequest {
                            name,
                            instructions,
                            prompts,
                        },
                        false,
                    )
                    .await;
                };
                response = tokio::time::timeout(
                    PASSWORD_AUTH_TIMEOUT,
                    handle.authenticate_keyboard_interactive_respond(replies),
                )
                .await
                .map_err(|_| {
                    SshTransportError::AuthenticationFailed(
                        "keyboard-interactive password fallback response timed out".to_string(),
                    )
                })?
                .map_err(|error| {
                    SshTransportError::AuthenticationFailed(format!(
                        "keyboard-interactive password fallback response failed: {error}"
                    ))
                })?;
            }
        }
    }
}

async fn authenticate_keyboard_interactive(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
    prompt_handler: Option<&dyn SshPromptHandler>,
) -> Result<client::AuthResult, SshTransportError> {
    let Some(prompt_handler) = prompt_handler else {
        return Err(SshTransportError::UnsupportedAuth(
            "keyboard-interactive requires a native prompt flow",
        ));
    };
    let response = tokio::time::timeout(
        PASSWORD_AUTH_TIMEOUT,
        handle.authenticate_keyboard_interactive_start(username, None::<String>),
    )
    .await
    .map_err(|_| {
        SshTransportError::AuthenticationFailed(
            "keyboard-interactive authentication timed out".to_string(),
        )
    })?
    .map_err(|error| {
        SshTransportError::AuthenticationFailed(format!(
            "keyboard-interactive authentication start failed: {error}"
        ))
    })?;
    let success =
        continue_keyboard_interactive_flow(handle, prompt_handler, response, false).await?;
    Ok(if success {
        client::AuthResult::Success
    } else {
        client::AuthResult::Failure {
            remaining_methods: russh::MethodSet::empty(),
            partial_success: false,
        }
    })
}

async fn try_keyboard_interactive_chain(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
    auth_result: &client::AuthResult,
    prompt_handler: Option<&dyn SshPromptHandler>,
) -> Result<bool, SshTransportError> {
    let client::AuthResult::Failure {
        partial_success: true,
        remaining_methods,
    } = auth_result
    else {
        return Ok(false);
    };
    if !remaining_methods.contains(&MethodKind::KeyboardInteractive) {
        return Ok(false);
    }
    let Some(prompt_handler) = prompt_handler else {
        return Ok(false);
    };
    let response = handle
        .authenticate_keyboard_interactive_start(username, None::<String>)
        .await
        .map_err(|error| {
            SshTransportError::AuthenticationFailed(format!(
                "keyboard-interactive chained authentication start failed: {error}"
            ))
        })?;
    continue_keyboard_interactive_flow(handle, prompt_handler, response, true).await
}

async fn continue_keyboard_interactive_flow(
    handle: &mut client::Handle<NativeClientHandler>,
    prompt_handler: &dyn SshPromptHandler,
    mut response: client::KeyboardInteractiveAuthResponse,
    chained: bool,
) -> Result<bool, SshTransportError> {
    for _ in 0..MAX_PASSWORD_KBI_FALLBACK_ROUNDS {
        match response {
            client::KeyboardInteractiveAuthResponse::Success => return Ok(true),
            client::KeyboardInteractiveAuthResponse::Failure { .. } => return Ok(false),
            client::KeyboardInteractiveAuthResponse::InfoRequest {
                name,
                instructions,
                prompts,
            } => {
                let request = KeyboardInteractivePromptRequest {
                    flow_id: uuid::Uuid::new_v4().to_string(),
                    name,
                    instructions,
                    prompts: prompts
                        .into_iter()
                        .map(|prompt| KeyboardInteractivePrompt {
                            prompt: prompt.prompt,
                            echo: prompt.echo,
                        })
                        .collect(),
                    chained,
                };
                let replies = tokio::time::timeout(
                    KBI_USER_PROMPT_TIMEOUT,
                    prompt_handler.keyboard_interactive(request),
                )
                .await
                .map_err(|_| {
                    SshTransportError::AuthenticationFailed(SshPromptError::Timeout.to_string())
                })?
                .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?
                .into_iter()
                .map(Zeroizing::new)
                .map(|reply| (*reply).clone())
                .collect::<Vec<_>>();
                response = tokio::time::timeout(
                    PASSWORD_AUTH_TIMEOUT,
                    handle.authenticate_keyboard_interactive_respond(replies),
                )
                .await
                .map_err(|_| {
                    SshTransportError::AuthenticationFailed(
                        "keyboard-interactive response timed out".to_string(),
                    )
                })?
                .map_err(|error| {
                    SshTransportError::AuthenticationFailed(format!(
                        "keyboard-interactive response failed: {error}"
                    ))
                })?;
            }
        }
    }
    Ok(false)
}

fn prompt_looks_like_password(prompt: &str) -> bool {
    let normalized = prompt.trim().to_ascii_lowercase();
    normalized.contains("password") || prompt.contains("密码")
}

fn authentication_failure_message(result: &client::AuthResult) -> String {
    match result {
        client::AuthResult::Success => "authentication succeeded".to_string(),
        client::AuthResult::Failure {
            remaining_methods,
            partial_success,
        } => {
            let methods = remaining_methods
                .iter()
                .map(|method| String::from(<&str>::from(method)))
                .collect::<Vec<_>>()
                .join(", ");
            if methods.is_empty() {
                format!("rejected by server; partial_success={partial_success}")
            } else {
                format!(
                    "rejected by server; remaining methods: {methods}; partial_success={partial_success}"
                )
            }
        }
    }
}

fn load_private_key_material(
    key_path: &str,
    passphrase: Option<&str>,
) -> Result<Arc<PrivateKey>, SshTransportError> {
    let key_path = resolve_key_path(key_path)?;
    let key = load_secret_key(&key_path, passphrase)
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
    Ok(Arc::new(key))
}

fn load_certificate_auth_material(
    key_path: &str,
    cert_path: &str,
    passphrase: Option<&str>,
) -> Result<(Arc<PrivateKey>, Certificate), SshTransportError> {
    let key = load_private_key_material(key_path, passphrase)?;
    let cert_path = expand_tilde_path(cert_path);
    let cert = load_openssh_certificate(&cert_path)
        .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
    Ok((key, cert))
}

async fn resolve_server_rsa_preference(
    handle: &client::Handle<NativeClientHandler>,
) -> Option<Option<HashAlg>> {
    handle.best_supported_rsa_hash().await.ok().flatten()
}

fn auth_algorithm_attempt_order(
    is_rsa: bool,
    server_preference: Option<Option<HashAlg>>,
) -> Vec<Option<HashAlg>> {
    if !is_rsa {
        return vec![None];
    }

    match server_preference {
        Some(None) => vec![None],
        Some(Some(preferred_hash)) => {
            let mut algorithms = vec![Some(preferred_hash)];
            algorithms.extend(
                RSA_AUTH_ALGORITHMS
                    .iter()
                    .copied()
                    .filter(|candidate| *candidate != Some(preferred_hash)),
            );
            algorithms
        }
        None => RSA_AUTH_ALGORITHMS.to_vec(),
    }
}

fn server_allows_more_publickey_attempts(result: &client::AuthResult) -> bool {
    matches!(
        result,
        client::AuthResult::Failure {
            remaining_methods,
            ..
        } if remaining_methods.contains(&MethodKind::PublicKey)
    )
}

async fn authenticate_publickey_best_algo(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
    key: Arc<PrivateKey>,
) -> Result<client::AuthResult, SshTransportError> {
    let algorithms = auth_algorithm_attempt_order(
        matches!(key.algorithm(), Algorithm::Rsa { .. }),
        resolve_server_rsa_preference(handle).await,
    );
    let mut last_result = None;

    for hash_alg in algorithms {
        let result = handle
            .authenticate_publickey(
                username,
                PrivateKeyWithHashAlg::new(Arc::clone(&key), hash_alg),
            )
            .await
            .map_err(|error| SshTransportError::AuthenticationFailed(error.to_string()))?;
        if result.success() || !server_allows_more_publickey_attempts(&result) {
            return Ok(result);
        }
        last_result = Some(result);
    }

    Ok(last_result.unwrap_or_else(|| client::AuthResult::Failure {
        remaining_methods: russh::MethodSet::empty(),
        partial_success: false,
    }))
}

async fn authenticate_certificate_best_algo(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
    key: Arc<PrivateKey>,
    cert: Certificate,
) -> Result<client::AuthResult, SshTransportError> {
    let algorithms = auth_algorithm_attempt_order(
        matches!(cert.algorithm(), Algorithm::Rsa { .. }),
        resolve_server_rsa_preference(handle).await,
    );
    let mut signer = LocalKeySigner::new(key);
    let mut last_result = None;

    for hash_alg in algorithms {
        let result = handle
            .authenticate_certificate_with(username, cert.clone(), hash_alg, &mut signer)
            .await
            .map_err(|error| {
                SshTransportError::AuthenticationFailed(format!(
                    "certificate authentication failed: {error}"
                ))
            })?;
        if result.success() || !server_allows_more_publickey_attempts(&result) {
            return Ok(result);
        }
        last_result = Some(result);
    }

    Ok(last_result.unwrap_or_else(|| client::AuthResult::Failure {
        remaining_methods: russh::MethodSet::empty(),
        partial_success: false,
    }))
}

fn sign_auth_payload_with_hash_alg(
    key: &PrivateKey,
    hash_alg: Option<HashAlg>,
    mut data: Vec<u8>,
) -> Result<Vec<u8>, LocalSignerError> {
    let signature = match key.key_data() {
        KeypairData::Rsa(rsa_keypair) => {
            SignatureSigner::try_sign(&(rsa_keypair, hash_alg), data.as_slice())
                .map_err(|error| LocalSignerError::Sign(error.to_string()))?
        }
        keypair => SignatureSigner::try_sign(keypair, data.as_slice())
            .map_err(|error| LocalSignerError::Sign(error.to_string()))?,
    };

    let mut encoded_signature = Vec::new();
    signature
        .encode(&mut encoded_signature)
        .map_err(|error| LocalSignerError::Sign(error.to_string()))?;
    encoded_signature
        .encode(&mut data)
        .map_err(|error| LocalSignerError::Sign(error.to_string()))?;
    Ok(data)
}

async fn authenticate_agent(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
) -> Result<client::AuthResult, SshTransportError> {
    let mut agent = connect_agent_client()
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

    let server_rsa_preference = resolve_server_rsa_preference(handle).await;
    let mut last_failure = None;
    for identity in identities {
        let public_key = identity.public_key().into_owned();
        let algorithms = auth_algorithm_attempt_order(
            matches!(public_key.algorithm(), Algorithm::Rsa { .. }),
            server_rsa_preference,
        );
        for hash_alg in algorithms {
            let result = handle
                .authenticate_publickey_with(
                    config.username.clone(),
                    public_key.clone(),
                    hash_alg,
                    &mut AgentSigner { agent: &mut agent },
                )
                .await
                .map_err(|error| match error {
                    AgentAuthError::Send(send) => {
                        SshTransportError::AuthenticationFailed(send.to_string())
                    }
                    AgentAuthError::Key(key_error) => {
                        SshTransportError::AuthenticationFailed(key_error.to_string())
                    }
                })?;
            if result.success() || !server_allows_more_publickey_attempts(&result) {
                return Ok(result);
            }
            last_failure = Some(result);
        }
    }

    Ok(last_failure.unwrap_or_else(|| client::AuthResult::Failure {
        remaining_methods: russh::MethodSet::empty(),
        partial_success: false,
    }))
}

async fn connect_agent_client() -> Result<NativeAgentClient, String> {
    #[cfg(unix)]
    {
        AgentClient::connect_env()
            .await
            .map(|agent| agent.dynamic())
            .map_err(|error| {
                format!(
                    "Failed to connect to SSH Agent: {error}. Make sure SSH_AUTH_SOCK is set and ssh-agent is running."
                )
            })
    }

    #[cfg(windows)]
    {
        AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
            .await
            .map(|agent| agent.dynamic())
            .map_err(|error| {
                format!(
                    "Failed to connect to SSH Agent via named pipe: {error}. Make sure the OpenSSH Authentication Agent service is running."
                )
            })
    }

    #[cfg(not(any(unix, windows)))]
    {
        Err("SSH Agent is not supported on this platform".to_string())
    }
}

async fn handle_agent_forward_channel(channel: Channel<client::Msg>) {
    let agent_stream = match connect_agent_stream().await {
        Ok(stream) => stream,
        Err(_) => {
            let _ = channel.eof().await;
            return;
        }
    };
    relay_agent_forward_channel(channel, agent_stream).await;
}

async fn relay_agent_forward_channel(
    channel: Channel<client::Msg>,
    mut agent_stream: impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
) {
    let mut channel_stream = channel.into_stream();
    let _ = tokio::io::copy_bidirectional(&mut channel_stream, &mut agent_stream).await;
}

#[cfg(unix)]
async fn connect_agent_stream() -> Result<tokio::net::UnixStream, String> {
    let socket_path =
        std::env::var("SSH_AUTH_SOCK").map_err(|_| "SSH_AUTH_SOCK is not set".to_string())?;
    tokio::net::UnixStream::connect(&socket_path)
        .await
        .map_err(|error| format!("failed to connect to SSH agent socket {socket_path}: {error}"))
}

#[cfg(windows)]
async fn connect_agent_stream() -> Result<tokio::net::windows::named_pipe::NamedPipeClient, String>
{
    use tokio::net::windows::named_pipe::ClientOptions;
    let pipe_name = r"\\.\pipe\openssh-ssh-agent";
    ClientOptions::new()
        .open(pipe_name)
        .map_err(|error| format!("failed to connect to SSH agent named pipe {pipe_name}: {error}"))
}

#[cfg(not(any(unix, windows)))]
async fn connect_agent_stream() -> Result<(), String> {
    Err("SSH agent forwarding is not supported on this platform".to_string())
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
