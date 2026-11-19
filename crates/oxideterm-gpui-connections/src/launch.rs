// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{future::Future, pin::Pin, result::Result as StdResult, sync::Arc};

use oxideterm_connections::{
    ConnectionTerminalOptions, MoshIpFamily as SavedMoshIpFamily, MoshPredictionMode,
    MoshUdpPortSelection as SavedMoshUdpPortSelection, SshChannelStrategy,
};
use oxideterm_ssh::{
    ConnectionConsumer, HostKeyStatus, KeyboardInteractivePromptRequest,
    KeyboardInteractiveResponses, NodeId, SshConfig, SshPromptError, SshPromptHandler,
    UpstreamProxyConfig,
};
use oxideterm_workspace::delivery::ActiveDeliverySender;
use tokio::sync::oneshot;

use crate::NewConnectionForm;

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct SshTerminalConnectionOptions {
    pub terminal: ConnectionTerminalOptions,
    pub dedicated_new_terminal_connection: bool,
    /// One-shot/per-connection intent to open SFTP alongside the first terminal.
    pub open_sftp_on_connect: bool,
    pub ssh_channel_strategy: SshChannelStrategy,
}

impl SshTerminalConnectionOptions {
    pub fn from_form(form: &NewConnectionForm) -> Self {
        // Keep the SSH session ownership policy separate from terminal protocol overrides.
        Self {
            terminal: form.terminal.clone(),
            dedicated_new_terminal_connection: form.dedicated_new_terminal_connection,
            open_sftp_on_connect: form.open_sftp_on_connect,
            ssh_channel_strategy: form.ssh_channel_strategy,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SshConnectionIntent {
    Test,
    TestStandaloneSftp,
    Connect(SshTerminalConnectionOptions),
    ConnectTemporary,
    ConnectSaved {
        id: String,
        auth_save_target: Option<super::saved_auth::SavedAuthSaveTarget>,
        open_sftp_on_connect: bool,
    },
    DrillDown {
        parent_id: NodeId,
        saved_connection_id: Option<String>,
        terminal_options: SshTerminalConnectionOptions,
    },
    Mosh(MoshConnectionOptions),
    StandaloneSftp {
        saved_profile_id: Option<String>,
        initial_remote_path: Option<String>,
        pair_launch_token: Option<String>,
    },
    StandaloneSftpSecondary {
        pair_launch_token: String,
    },
}

impl SshConnectionIntent {
    pub fn standalone_sftp_pair_launch_token(&self) -> Option<&str> {
        match self {
            Self::StandaloneSftp {
                pair_launch_token: Some(token),
                ..
            }
            | Self::StandaloneSftpSecondary {
                pair_launch_token: token,
            } => Some(token),
            _ => None,
        }
    }
}

/// Owns both secret-bearing endpoint configs only until host-key checks finish.
pub struct PendingStandaloneSftpPairLaunch {
    pub saved_profile_id: String,
    pub title: String,
    pub primary_initial_remote_path: Option<String>,
    pub secondary_initial_remote_path: Option<String>,
    pub primary_config: Option<SshConfig>,
    pub secondary_config: Option<SshConfig>,
}

/// Non-secret Mosh launch settings travel through SSH host-key preflight.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MoshConnectionOptions {
    pub saved_profile_id: Option<String>,
    pub server_executable: String,
    pub udp_host_override: Option<String>,
    pub udp_port: SavedMoshUdpPortSelection,
    pub ip_family: SavedMoshIpFamily,
    pub prediction: MoshPredictionMode,
    pub locale: Option<String>,
    pub terminal: ConnectionTerminalOptions,
    // Correlates an asynchronous verified Mosh launch without exposing a GPUI identity.
    pub public_mcp_open_token: Option<String>,
    // Reconnect binds the new terminal surface to an existing logical connection record.
    pub runtime_connection_attempt_id: Option<String>,
}

impl MoshConnectionOptions {
    pub fn into_verified_terminal(self, ssh: SshConfig) -> VerifiedMoshTerminal {
        let mut bootstrap = oxideterm_mosh::MoshBootstrapConfig::new("pending", ssh);
        bootstrap.server_executable = self.server_executable;
        bootstrap.udp_host_override = self.udp_host_override;
        bootstrap.udp_port = match self.udp_port {
            SavedMoshUdpPortSelection::Automatic => {
                oxideterm_mosh::MoshUdpPortSelection::Automatic
            }
            SavedMoshUdpPortSelection::Fixed { port } => {
                oxideterm_mosh::MoshUdpPortSelection::Fixed(port)
            }
            SavedMoshUdpPortSelection::Range { start, end } => {
                oxideterm_mosh::MoshUdpPortSelection::Range { start, end }
            }
        };
        bootstrap.ip_family = match self.ip_family {
            SavedMoshIpFamily::Auto => oxideterm_mosh::MoshIpFamily::Auto,
            SavedMoshIpFamily::Ipv4 => oxideterm_mosh::MoshIpFamily::Ipv4,
            SavedMoshIpFamily::Ipv6 => oxideterm_mosh::MoshIpFamily::Ipv6,
        };
        bootstrap.locale_assignments = self
            .locale
            .filter(|locale| !locale.trim().is_empty())
            .map(|locale| vec![("LANG".to_string(), locale)])
            .unwrap_or_default();
        let prediction = match self.prediction {
            MoshPredictionMode::Adaptive => {
                oxideterm_terminal::MoshPredictionDisplay::Adaptive
            }
            MoshPredictionMode::Always => {
                oxideterm_terminal::MoshPredictionDisplay::Always
            }
            MoshPredictionMode::Never => {
                oxideterm_terminal::MoshPredictionDisplay::Never
            }
        };
        VerifiedMoshTerminal {
            bootstrap,
            prediction,
            terminal: self.terminal,
            saved_profile_id: self.saved_profile_id,
        }
    }
}

pub struct VerifiedMoshTerminal {
    pub bootstrap: oxideterm_mosh::MoshBootstrapConfig,
    pub prediction: oxideterm_terminal::MoshPredictionDisplay,
    pub terminal: ConnectionTerminalOptions,
    pub saved_profile_id: Option<String>,
}

pub enum SshConnectionWorkerResult {
    Preflight {
        config: SshConfig,
        upstream_proxy: Option<UpstreamProxyConfig>,
        title: String,
        intent: SshConnectionIntent,
        host: String,
        port: u16,
        status: HostKeyStatus,
    },
    SessionTreePreflight {
        generation: u64,
        step_index: usize,
        upstream_proxy: Option<UpstreamProxyConfig>,
        status: HostKeyStatus,
    },
    Test {
        result: StdResult<(), String>,
    },
    StandaloneSftpConnected {
        endpoint_id: String,
        saved_profile_id: Option<String>,
        title: String,
        initial_remote_path: Option<String>,
        consumer: ConnectionConsumer,
        result: StdResult<oxideterm_ssh::SshConnectionHandle, String>,
    },
    StandaloneSftpPairConnected {
        saved_profile_id: String,
        title: String,
        primary_endpoint_id: String,
        secondary_endpoint_id: String,
        primary_initial_remote_path: Option<String>,
        secondary_initial_remote_path: Option<String>,
        primary_consumer: ConnectionConsumer,
        secondary_consumer: ConnectionConsumer,
        result: StdResult<
            (
                oxideterm_ssh::SshConnectionHandle,
                oxideterm_ssh::SshConnectionHandle,
            ),
            String,
        >,
    },
    KeyboardInteractivePrompt {
        request: KeyboardInteractivePromptRequest,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
    },
    SecurityKeyProvider {
        response_tx: oneshot::Sender<Result<Arc<oxideterm_ssh::SecurityKeyProvider>, String>>,
    },
    SecurityKeyPrompt {
        flow_id: String,
        touch: bool,
        retry: bool,
        response_tx: oneshot::Sender<Result<KeyboardInteractiveResponses, SshPromptError>>,
    },
    SecurityKeyError {
        error: oxideterm_ssh::SecurityKeyError,
        response_tx: oneshot::Sender<String>,
    },
    PasswordPrompt {
        node_id: Option<NodeId>,
        prompt: oxideterm_ssh::SshPasswordPrompt,
        response_tx: oneshot::Sender<Result<oxideterm_ssh::SshPasswordResponse, SshPromptError>>,
    },
    PasswordAuthenticated {
        target: super::saved_auth::SavedPasswordTarget,
        password: zeroize::Zeroizing<String>,
    },
    AuthenticationCompleted {
        node_id: NodeId,
        connection_id: String,
        configured_credentials_confirmed: bool,
    },
}

#[derive(Clone)]
pub struct NativeSshPromptHandler {
    tx: ActiveDeliverySender<SshConnectionWorkerResult>,
    node_id: Option<NodeId>,
    connection_id: Option<String>,
}

impl NativeSshPromptHandler {
    pub fn new(tx: ActiveDeliverySender<SshConnectionWorkerResult>) -> Self {
        Self {
            tx,
            node_id: None,
            connection_id: None,
        }
    }

    pub fn for_node(mut self, node_id: NodeId) -> Self {
        self.node_id = Some(node_id);
        self
    }

    pub fn for_connection(mut self, connection_id: String) -> Self {
        self.connection_id = Some(connection_id);
        self
    }
}

impl SshPromptHandler for NativeSshPromptHandler {
    fn security_key_provider(
        &self,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Arc<oxideterm_ssh::SecurityKeyProvider>, String>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let (response_tx, response_rx) = oneshot::channel();
            self.tx
                .send(SshConnectionWorkerResult::SecurityKeyProvider { response_tx })
                .map_err(|_| "Security-key UI is unavailable".to_string())?;
            response_rx
                .await
                .map_err(|_| "Security-key UI was closed".to_string())?
        })
    }

    fn security_key_interaction(&self) -> Option<Arc<dyn oxideterm_ssh::SecurityKeyInteraction>> {
        Some(Arc::new(NativeSecurityKeyInteraction {
            handler: self.clone(),
            flow_id: uuid::Uuid::new_v4().to_string(),
        }))
    }
    fn authentication_completed(&self, configured_credentials_confirmed: bool) {
        if let (Some(node_id), Some(connection_id)) = (&self.node_id, &self.connection_id) {
            let _ = self
                .tx
                .send(SshConnectionWorkerResult::AuthenticationCompleted {
                    node_id: node_id.clone(),
                    connection_id: connection_id.clone(),
                    configured_credentials_confirmed,
                });
        }
    }

    fn password(
        &self,
        prompt: oxideterm_ssh::SshPasswordPrompt,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<oxideterm_ssh::SshPasswordResponse, SshPromptError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let (response_tx, response_rx) = oneshot::channel();
            self.tx
                .send(SshConnectionWorkerResult::PasswordPrompt {
                    node_id: self.node_id.clone(),
                    prompt,
                    response_tx,
                })
                .map_err(|_| {
                    SshPromptError::Failed("native SSH prompt UI is unavailable".into())
                })?;
            response_rx
                .await
                .map_err(|_| SshPromptError::Failed("native SSH prompt UI was closed".into()))?
        })
    }

    fn keyboard_interactive(
        &self,
        request: KeyboardInteractivePromptRequest,
    ) -> Pin<
        Box<dyn Future<Output = Result<KeyboardInteractiveResponses, SshPromptError>> + Send + '_>,
    > {
        Box::pin(async move {
            // The application has one protected SSH prompt surface. Keep other
            // authentication attempts waiting instead of cancelling their challenges.
            let _prompt_guard = NATIVE_SSH_PROMPT.lock().await;
            let (response_tx, response_rx) = oneshot::channel();
            self.tx
                .send(SshConnectionWorkerResult::KeyboardInteractivePrompt {
                    request,
                    response_tx,
                })
                .map_err(|_| {
                    SshPromptError::Failed("native SSH prompt UI is unavailable".into())
                })?;
            response_rx
                .await
                .map_err(|_| SshPromptError::Failed("native SSH prompt UI was closed".into()))?
        })
    }
}

static NATIVE_SSH_PROMPT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

struct NativeSecurityKeyInteraction {
    handler: NativeSshPromptHandler,
    flow_id: String,
}

impl NativeSecurityKeyInteraction {
    async fn prompt(
        &self,
        touch: bool,
        retry: bool,
    ) -> Result<KeyboardInteractiveResponses, oxideterm_ssh::SecurityKeyError> {
        let _guard = NATIVE_SSH_PROMPT.lock().await;
        let (response_tx, response_rx) = oneshot::channel();
        self.handler
            .tx
            .send(SshConnectionWorkerResult::SecurityKeyPrompt {
                flow_id: self.flow_id.clone(),
                touch,
                retry,
                response_tx,
            })
            .map_err(|_| oxideterm_ssh::SecurityKeyError::Unavailable)?;
        response_rx
            .await
            .map_err(|_| oxideterm_ssh::SecurityKeyError::Cancelled)?
            .map_err(|error| match error {
                SshPromptError::Timeout => oxideterm_ssh::SecurityKeyError::Timeout,
                _ => oxideterm_ssh::SecurityKeyError::Cancelled,
            })
    }
}

impl oxideterm_ssh::SecurityKeyInteraction for NativeSecurityKeyInteraction {
    fn pin(
        &self,
        retry: bool,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<zeroize::Zeroizing<String>, oxideterm_ssh::SecurityKeyError>>
                + Send
                + '_,
        >,
    > {
        Box::pin(async move {
            let mut answers = self.prompt(false, retry).await?;
            if answers.len() != 1 {
                return Err(oxideterm_ssh::SecurityKeyError::InvalidRequest);
            }
            Ok(zeroize::Zeroizing::new(std::mem::take(&mut answers[0])))
        })
    }
    fn touch(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<(), oxideterm_ssh::SecurityKeyError>> + Send + '_>>
    {
        Box::pin(async move { self.prompt(true, false).await.map(|_| ()) })
    }
    fn error_message(
        &self,
        error: oxideterm_ssh::SecurityKeyError,
    ) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(async move {
            let (response_tx, response_rx) = oneshot::channel();
            if self
                .handler
                .tx
                .send(SshConnectionWorkerResult::SecurityKeyError { error, response_tx })
                .is_err()
            {
                return error.to_string();
            }
            response_rx.await.unwrap_or_else(|_| error.to_string())
        })
    }
}
