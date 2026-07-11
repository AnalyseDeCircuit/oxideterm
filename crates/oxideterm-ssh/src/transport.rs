// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{future::Future, path::PathBuf, pin::Pin, sync::Arc, time::Duration};

use oxideterm_sftp::{SftpChannelOpener, SftpError, SftpExecChannelOpener};
use oxideterm_x11_forwarding::{X11RemoteDisplayAllocator, X11RemoteXauthUpdate, X11SshRequest};
use parking_lot::RwLock;
use russh::{
    AgentAuthError, Channel, ChannelMsg, MethodKind, Pty, Signer as RusshSigner, client,
    keys::{
        Algorithm, Certificate, HashAlg, PrivateKey, PrivateKeyWithHashAlg,
        agent::{
            AgentIdentity,
            client::{AgentClient, AgentStream},
        },
        load_openssh_certificate,
        ssh_key::private::KeypairData,
    },
};
use signature::Signer as SignatureSigner;
use ssh_encoding::Encode;
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::Semaphore,
    sync::mpsc,
    time::{Instant, sleep_until, timeout},
};
use zeroize::Zeroizing;

use crate::{
    AuthMethod, ConnectionConsumer, ConnectionState, ConnectionTransportStatus,
    KeepaliveProbeResult, ProxyHopConfig, SshConfig, SshConnectionHandle, SshConnectionRegistry,
    host_key::{
        HostKeyStatus, HostKeyVerification, accept_host_key_for_session, check_host_key_via_stream,
        learn_host_key, public_key_fingerprint, verify_host_key,
    },
    upstream_proxy::{UpstreamProxyConfig, UpstreamProxyProtocol, dial_initial_tcp},
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

fn log_upstream_proxy_path(
    target_host: &str,
    target_port: u16,
    proxy: Option<&UpstreamProxyConfig>,
) {
    if let Some(proxy) = proxy {
        tracing::info!(
            target_host,
            target_port,
            proxy_protocol = upstream_proxy_protocol_label(proxy.protocol),
            proxy_host = proxy.host.as_str(),
            proxy_port = proxy.port,
            proxy_remote_dns = proxy.remote_dns,
            proxy_no_proxy_configured = !proxy.no_proxy.trim().is_empty(),
            "Connecting through upstream proxy"
        );
    }
}

fn upstream_proxy_protocol_label(protocol: UpstreamProxyProtocol) -> &'static str {
    match protocol {
        UpstreamProxyProtocol::Socks5 => "socks5",
        UpstreamProxyProtocol::HttpConnect => "http_connect",
    }
}
const SSH_COMMAND_CHANNEL_CAPACITY: usize = 1024;
const SSH_OUTPUT_CHANNEL_CAPACITY: usize = 1024;
const SSH_OUTPUT_BATCH_MAX_BYTES: usize = 64 * 1024;
const SSH_OUTPUT_FLUSH_MS: u64 = 4;
const SSH_OUTPUT_INTERACTIVE_FLUSH_MS: u64 = 1;
const SSH_OUTPUT_INTERACTIVE_WINDOW_MS: u64 = 120;
const UTF8_RESIDUAL_MAX_BYTES: usize = 4;
const MAX_PROXY_CHAIN_DEPTH: usize = 32;
const MAX_AUTH_BANNER_BYTES: usize = 16 * 1024;

type AuthBannerSink = Arc<parking_lot::Mutex<Vec<String>>>;

fn new_auth_banner_sink() -> AuthBannerSink {
    Arc::new(parking_lot::Mutex::new(Vec::new()))
}

fn sanitize_auth_banner(banner: &str) -> Option<String> {
    let mut out = String::with_capacity(banner.len().min(MAX_AUTH_BANNER_BYTES));
    for ch in banner.chars() {
        if out.len() >= MAX_AUTH_BANNER_BYTES {
            break;
        }
        match ch {
            '\r' | '\n' | '\t' => out.push(ch),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    let trimmed = out.trim_matches(['\r', '\n']).to_string();
    if trimmed.trim().is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn take_auth_banners(sink: &AuthBannerSink) -> Vec<String> {
    std::mem::take(&mut *sink.lock())
}

fn auth_banner_prelude_bytes(banners: Vec<String>) -> Vec<u8> {
    if banners.is_empty() {
        return Vec::new();
    }

    let mut prelude = Vec::new();
    for banner in banners {
        if !prelude.is_empty() {
            prelude.extend_from_slice(b"\r\n");
        }
        let normalized = banner.replace("\r\n", "\n").replace('\r', "\n");
        let normalized = normalized.replace('\n', "\r\n");
        prelude.extend_from_slice(normalized.as_bytes());
    }
    prelude.extend_from_slice(b"\r\n");
    prelude
}

#[derive(Debug, thiserror::Error)]
pub enum SshTransportError {
    #[error("DNS resolution failed for {address}: {message}")]
    DnsResolution { address: String, message: String },
    #[error("SSH connection timed out")]
    Timeout,
    #[error("SSH connection failed: {0}")]
    ConnectionFailed(String),
    #[error(
        "SSH algorithm negotiation failed: no common {kind} algorithm. Client offered: {client_algorithms:?}; server offered: {server_algorithms:?}"
    )]
    AlgorithmNegotiationFailed {
        kind: SshAlgorithmKind,
        client_algorithms: Vec<String>,
        server_algorithms: Vec<String>,
    },
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

pub type ManagedKeyResolver =
    Arc<dyn Fn(&str) -> Result<Zeroizing<String>, SshTransportError> + Send + Sync>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SshAlgorithmKind {
    KeyExchange,
    HostKey,
    Cipher,
    Mac,
    Compression,
}

impl std::fmt::Display for SshAlgorithmKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::KeyExchange => "key exchange",
            Self::HostKey => "host key",
            Self::Cipher => "cipher",
            Self::Mac => "MAC",
            Self::Compression => "compression",
        })
    }
}

impl From<russh::Error> for SshTransportError {
    fn from(error: russh::Error) -> Self {
        match error {
            russh::Error::NoCommonAlgo { kind, ours, theirs } => {
                // Keep both sides' advertised lists structured so the UI can
                // explain the exact negotiation gap instead of showing a flat
                // "connection failed" string.
                Self::AlgorithmNegotiationFailed {
                    kind: SshAlgorithmKind::from(kind),
                    client_algorithms: ours,
                    server_algorithms: theirs,
                }
            }
            error => Self::ConnectionFailed(error.to_string()),
        }
    }
}

impl SshTransportError {
    pub(crate) fn with_context(self, context: impl Into<String>) -> Self {
        match self {
            Self::ConnectionFailed(message) => {
                Self::ConnectionFailed(format!("{}: {message}", context.into()))
            }
            error => error,
        }
    }
}

impl From<russh::AlgorithmKind> for SshAlgorithmKind {
    fn from(kind: russh::AlgorithmKind) -> Self {
        match kind {
            russh::AlgorithmKind::Kex => Self::KeyExchange,
            russh::AlgorithmKind::Key => Self::HostKey,
            russh::AlgorithmKind::Cipher => Self::Cipher,
            russh::AlgorithmKind::Compression => Self::Compression,
            russh::AlgorithmKind::Mac => Self::Mac,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SshCommandOutput {
    // Keep direct SSH command execution aligned with Tauri nodeIdeExecCommand:
    // exit status is structured output, not a transport-level failure.
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub truncated: bool,
}

#[derive(Clone, Eq, PartialEq)]
pub struct SshShellBootstrap {
    stage_command: String,
    launch_input: String,
    cleanup_command: String,
}

impl SshShellBootstrap {
    pub fn new(stage_command: String, launch_input: String, cleanup_command: String) -> Self {
        Self {
            stage_command,
            launch_input,
            cleanup_command,
        }
    }

    pub fn stage_command(&self) -> &str {
        &self.stage_command
    }

    pub fn launch_input(&self) -> &str {
        &self.launch_input
    }

    pub fn cleanup_command(&self) -> &str {
        &self.cleanup_command
    }
}

impl std::fmt::Debug for SshShellBootstrap {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Bootstrap commands contain the private OSC session token and must stay redacted.
        formatter
            .debug_struct("SshShellBootstrap")
            .field("stage_command", &"<redacted>")
            .field("launch_input_bytes", &self.launch_input.len())
            .field("cleanup_command", &"<redacted>")
            .finish()
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

pub struct X11ForwardedChannel {
    pub connection_id: String,
    pub originator_address: String,
    pub originator_port: u16,
    pub stream: BoxedSshForwardStream,
}

pub trait X11ForwardHandler: Send + Sync {
    fn handle_x11_forward(
        &self,
        event: X11ForwardedChannel,
    ) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

#[derive(Clone)]
struct X11ForwardRegistration {
    connection_id: String,
    handler: Arc<dyn X11ForwardHandler>,
}

type X11ForwardHandlerSlot = Arc<RwLock<Option<X11ForwardRegistration>>>;

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
    auth_banners: AuthBannerSink,
    ssh_connection: Option<SshConnectionHandle>,
    registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
}

struct RegistryConsumerGuard {
    release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
}

impl RegistryConsumerGuard {
    fn new(
        registry: SshConnectionRegistry,
        connection_id: String,
        consumer: ConnectionConsumer,
    ) -> Self {
        Self {
            release: Some((registry, connection_id, consumer)),
        }
    }

    fn release_tuple(&self) -> Option<(SshConnectionRegistry, String, ConnectionConsumer)> {
        self.release.clone()
    }

    fn release_now(&mut self) {
        if let Some((registry, connection_id, consumer)) = self.release.take() {
            registry.release(&connection_id, &consumer);
        }
    }

    fn disarm(&mut self) {
        self.release = None;
    }
}

impl Drop for RegistryConsumerGuard {
    fn drop(&mut self) {
        // Terminal setup can be cancelled after the pool consumer is acquired
        // but before an SshPtyHandle exists. Tauri close_terminal releases the
        // terminal's connection ref in that in-flight window, so native keeps
        // the same ownership invariant with a short-lived guard.
        self.release_now();
    }
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

    pub fn take_auth_banner_prelude(&self) -> Vec<u8> {
        auth_banner_prelude_bytes(take_auth_banners(&self.auth_banners))
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
    managed_key_resolver: Option<ManagedKeyResolver>,
    shell_bootstrap: Option<SshShellBootstrap>,
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
    use super::{RegistryConsumerGuard, ssh_channel_error_is_transport_lost};
    use crate::{ConnectionConsumer, SshConfig, SshConnectionRegistry};

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

    #[test]
    fn registry_consumer_guard_releases_cancelled_terminal_setup() {
        let registry = SshConnectionRegistry::default();
        let consumer = ConnectionConsumer::Terminal("term-1".to_string());
        let handle = registry.acquire(
            SshConfig::password("host", 22, "me", "pw"),
            consumer.clone(),
        );
        let connection_id = handle.connection_id().to_string();

        let guard = RegistryConsumerGuard::new(registry.clone(), connection_id, consumer);
        drop(guard);

        assert_eq!(handle.info().ref_count, 0);
    }
}

#[cfg(test)]
mod transport_error_tests {
    use super::{SshAlgorithmKind, SshTransportError};

    #[test]
    fn no_common_kex_error_keeps_algorithm_lists_structured() {
        let error = SshTransportError::from(russh::Error::NoCommonAlgo {
            kind: russh::AlgorithmKind::Kex,
            ours: vec!["curve25519-sha256".to_string()],
            theirs: vec!["diffie-hellman-group1-sha1".to_string()],
        });

        match error {
            SshTransportError::AlgorithmNegotiationFailed {
                kind,
                client_algorithms,
                server_algorithms,
            } => {
                assert_eq!(kind, SshAlgorithmKind::KeyExchange);
                assert_eq!(client_algorithms, ["curve25519-sha256"]);
                assert_eq!(server_algorithms, ["diffie-hellman-group1-sha1"]);
            }
            other => panic!("unexpected error mapping: {other:?}"),
        }
    }

    #[test]
    fn no_common_mac_error_keeps_display_actionable() {
        let error = SshTransportError::from(russh::Error::NoCommonAlgo {
            kind: russh::AlgorithmKind::Mac,
            ours: vec!["hmac-sha2-256-etm@openssh.com".to_string()],
            theirs: vec!["umac-64-etm@openssh.com".to_string()],
        });

        let message = error.to_string();

        assert!(message.contains("no common MAC algorithm"));
        assert!(message.contains("hmac-sha2-256-etm@openssh.com"));
        assert!(message.contains("umac-64-etm@openssh.com"));
    }

    #[test]
    fn contextual_prefix_keeps_negotiation_errors_structured() {
        let error = SshTransportError::from(russh::Error::NoCommonAlgo {
            kind: russh::AlgorithmKind::Cipher,
            ours: vec!["aes256-gcm@openssh.com".to_string()],
            theirs: vec!["aes128-cbc".to_string()],
        })
        .with_context("proxy stream");

        assert!(matches!(
            error,
            SshTransportError::AlgorithmNegotiationFailed {
                kind: SshAlgorithmKind::Cipher,
                ..
            }
        ));
    }

    #[test]
    fn contextual_prefix_keeps_plain_connection_errors_actionable() {
        let error =
            SshTransportError::ConnectionFailed("unexpected eof".to_string()).with_context("proxy");

        assert_eq!(
            error.to_string(),
            "SSH connection failed: proxy: unexpected eof"
        );
    }
}

#[cfg(test)]
mod auth_banner_tests {
    use super::{
        auth_banner_prelude_bytes, new_auth_banner_sink, sanitize_auth_banner, take_auth_banners,
    };

    #[test]
    fn sanitize_auth_banner_strips_control_chars() {
        assert_eq!(
            sanitize_auth_banner("hello\u{0007}\nworld"),
            Some("hello\nworld".to_string())
        );
    }

    #[test]
    fn sanitize_auth_banner_drops_empty_banner() {
        assert_eq!(sanitize_auth_banner("\u{0007}\r\n"), None);
    }

    #[test]
    fn auth_banners_are_consumed_once() {
        let sink = new_auth_banner_sink();
        sink.lock().push("Banner A".to_string());
        sink.lock().push("Banner B".to_string());

        assert_eq!(
            take_auth_banners(&sink),
            vec!["Banner A".to_string(), "Banner B".to_string()]
        );
        assert!(take_auth_banners(&sink).is_empty());
    }

    #[test]
    fn auth_banner_prelude_matches_terminal_line_endings() {
        assert_eq!(
            auth_banner_prelude_bytes(vec!["one\ntwo".to_string(), "three".to_string()]),
            b"one\r\ntwo\r\nthree\r\n".to_vec()
        );
    }
}
