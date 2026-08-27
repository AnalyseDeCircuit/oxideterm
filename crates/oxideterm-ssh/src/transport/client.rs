const AGENT_FORWARDING_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const X11_FORWARDING_RESPONSE_TIMEOUT: Duration = Duration::from_secs(5);
const OXIDETERM_PRIVATE_OSC_SESSION_ENV: &str = "LC_OXIDETERM_SESSION";
const OXIDETERM_PRIVATE_OSC_SESSION_VALUE: &str = "1";
const CHILD_CONNECTION_RETIRED_DURING_CONNECT: &str =
    "child connection was retired while its SSH transport was connecting";

enum ShellRequestConfig {
    Owned(SshConfig),
    Registry {
        connection: SshConnectionHandle,
        // None inherits the shared entry; Some(None) explicitly disables X11.
        x11_forwarding_override: Option<Option<X11ForwardPolicy>>,
    },
}

#[derive(Clone, Copy)]
enum ShellRegistryAcquisition {
    Shared,
    Dedicated,
}

impl ShellRequestConfig {
    fn registry(
        connection: SshConnectionHandle,
        x11_forwarding_override: Option<Option<X11ForwardPolicy>>,
    ) -> Self {
        Self::Registry {
            connection,
            x11_forwarding_override,
        }
    }

    fn config(&self) -> &SshConfig {
        match self {
            Self::Owned(config) => config,
            Self::Registry { connection, .. } => connection.config(),
        }
    }

    fn x11_forwarding(&self) -> Option<X11ForwardPolicy> {
        match self {
            Self::Registry {
                x11_forwarding_override: Some(x11_forwarding),
                ..
            } => *x11_forwarding,
            Self::Owned(_) | Self::Registry { .. } => self.config().x11_forwarding,
        }
    }
}

fn validate_agent_forwarding_response(
    response: Option<ChannelMsg>,
) -> Result<(), SshTransportError> {
    match response {
        Some(ChannelMsg::Success) => Ok(()),
        Some(ChannelMsg::Failure) => Err(SshTransportError::Channel(
            "SSH server rejected agent forwarding".to_string(),
        )),
        Some(_) => Err(SshTransportError::Channel(
            "SSH server returned an unexpected response while enabling agent forwarding"
                .to_string(),
        )),
        None => Err(SshTransportError::Channel(
            "SSH channel closed while enabling agent forwarding".to_string(),
        )),
    }
}

fn commit_child_registry_ownership(
    registry: &SshConnectionRegistry,
    connection_id: &str,
    parent_connection_id: &str,
    parent_consumer: &ConnectionConsumer,
    child_release_guard: &mut RegistryConsumerGuard,
    parent_release_guard: &mut RegistryConsumerGuard,
) -> Result<(), SshTransportError> {
    let child_still_registered = registry
        .set_parent_connection_ownership(
            connection_id,
            parent_connection_id.to_string(),
            parent_consumer.clone(),
        )
        .is_some()
        && registry
            .mark_state(connection_id, ConnectionState::Active)
            .is_some();
    if !child_still_registered {
        // A retired child cannot own the ancestor consumer acquired for its tunnel.
        parent_release_guard.release_now();
        child_release_guard.release_now();
        return Err(SshTransportError::ConnectionFailed(
            CHILD_CONNECTION_RETIRED_DURING_CONNECT.to_string(),
        ));
    }

    child_release_guard.disarm();
    parent_release_guard.disarm();
    Ok(())
}

async fn request_agent_forwarding_for_shell(
    channel: &mut russh::Channel<client::Msg>,
) -> Result<(), SshTransportError> {
    channel
        .agent_forward(true)
        .await
        .map_err(|error| SshTransportError::Channel(error.to_string()))?;
    let response = timeout(AGENT_FORWARDING_RESPONSE_TIMEOUT, channel.wait())
        .await
        .map_err(|_| {
            SshTransportError::Channel(
                "SSH server did not respond to the agent forwarding request".to_string(),
            )
        })?;
    validate_agent_forwarding_response(response)
}

fn validate_x11_forwarding_response(
    response: Option<ChannelMsg>,
) -> Result<(), SshTransportError> {
    match response {
        Some(ChannelMsg::Success) => Ok(()),
        Some(ChannelMsg::Failure) => Err(SshTransportError::Channel(
            "SSH server rejected X11 forwarding".to_string(),
        )),
        Some(_) => Err(SshTransportError::Channel(
            "SSH server returned an unexpected response while enabling X11 forwarding"
                .to_string(),
        )),
        None => Err(SshTransportError::Channel(
            "SSH channel closed while enabling X11 forwarding".to_string(),
        )),
    }
}

#[cfg(test)]
mod x11_request_response_tests {
    use super::*;

    #[test]
    fn x11_request_requires_explicit_success() {
        assert!(validate_x11_forwarding_response(Some(ChannelMsg::Success)).is_ok());
        assert!(validate_x11_forwarding_response(Some(ChannelMsg::Failure)).is_err());
        assert!(validate_x11_forwarding_response(None).is_err());
    }

    #[test]
    fn per_shell_x11_override_does_not_mutate_the_shared_connection() {
        let registry = SshConnectionRegistry::default();
        let mut config = SshConfig::password("host", 22, "me", "pw");
        let shared_policy = Some(X11ForwardPolicy::untrusted());
        config.x11_forwarding = shared_policy;
        let connection = registry.acquire(
            config,
            ConnectionConsumer::NodeRouter("node".to_string()),
        );

        let inherited = ShellRequestConfig::registry(connection.clone(), None);
        let disabled = ShellRequestConfig::registry(connection.clone(), Some(None));
        let overridden_policy = Some(X11ForwardPolicy::trusted());
        let overridden =
            ShellRequestConfig::registry(connection.clone(), Some(overridden_policy));

        assert_eq!(inherited.x11_forwarding(), shared_policy);
        assert_eq!(disabled.x11_forwarding(), None);
        assert_eq!(overridden.x11_forwarding(), overridden_policy);
        assert_eq!(connection.config().x11_forwarding, shared_policy);
    }
}

async fn request_x11_forwarding_for_shell(
    channel: &mut russh::Channel<client::Msg>,
    request: &X11SshRequest,
) -> Result<(), SshTransportError> {
    channel
        .request_x11(
            true,
            request.single_connection,
            request.auth_protocol_name(),
            request.auth_cookie_hex.as_str(),
            request.screen_number,
        )
        .await
        .map_err(|error| SshTransportError::Channel(error.to_string()))?;
    let response = timeout(X11_FORWARDING_RESPONSE_TIMEOUT, channel.wait())
        .await
        .map_err(|_| {
            SshTransportError::Channel(
                "SSH server did not respond to the X11 forwarding request".to_string(),
            )
        })?;
    validate_x11_forwarding_response(response)
}

async fn open_pty_channel(
    pooled: &Arc<PooledSshConnection>,
    cols: u32,
    rows: u32,
    pty_modes: &[(Pty, u32)],
) -> Result<russh::Channel<client::Msg>, (&'static str, SshTransportError)> {
    let channel = pooled
        .target
        .channel_open_session()
        .await
        .map_err(|error| {
            (
                "open-channel",
                SshTransportError::Channel(error.to_string()),
            )
        })?;
    channel
        .request_pty(
            false,
            "xterm-256color",
            cols,
            rows,
            0,
            0,
            pty_modes,
        )
        .await
        .map_err(|error| ("request-pty", SshTransportError::Channel(error.to_string())))?;
    Ok(channel)
}

async fn open_interactive_shell_channel(
    pooled: &Arc<PooledSshConnection>,
    cols: u32,
    rows: u32,
    pty_modes: &[(Pty, u32)],
    agent_forwarding: bool,
    x11_forwarding: Option<X11ForwardPolicy>,
    x11_route_id: &str,
    x11_connection_owner: Option<X11ConnectionOwner>,
) -> Result<
    (russh::Channel<client::Msg>, Option<X11ForwardRouteGuard>),
    (&'static str, SshTransportError),
> {
    // Resolve device-local X11 state before allocating a remote session. X11 is
    // optional, so local preparation failures must not block the regular shell.
    let prepared_x11 = match x11_forwarding {
        Some(policy) => match prepare_x11_material(policy).await {
            Ok(prepared) => Some(prepared),
            Err(_error) => {
                // Keep this log fixed because preparation errors can contain
                // device-local display or xauth details.
                tracing::warn!(
                    "X11 forwarding is unavailable; continuing with a regular SSH shell"
                );
                None
            }
        },
        None => None,
    };
    let mut channel = open_pty_channel(pooled, cols, rows, pty_modes).await?;
    let mut x11_route_guard = None;
    if let Some(prepared) = prepared_x11 {
        let (request, guard) = register_x11_route(
            &pooled.x11_dispatcher,
            x11_route_id.to_string(),
            prepared,
            x11_connection_owner,
        );
        if request_x11_forwarding_for_shell(&mut channel, &request)
            .await
            .is_err()
        {
            // Remove the route and its bearer cookie before awaiting channel
            // cleanup. A fresh PTY also isolates any delayed X11 reply from
            // later Agent forwarding requests.
            drop(request);
            drop(guard);
            tracing::warn!(
                "X11 forwarding is unavailable; continuing with a regular SSH shell"
            );
            let _ = channel.close().await;
            channel = open_pty_channel(pooled, cols, rows, pty_modes).await?;
        } else {
            x11_route_guard = Some(guard);
        }
    }
    if agent_forwarding {
        if let Err(error) = request_agent_forwarding_for_shell(&mut channel).await {
            // A rejected request leaves no usable shell channel. Close it
            // explicitly so pooled transports do not retain a failed channel.
            let _ = channel.close().await;
            return Err(("request-agent-forwarding", error));
        }
        // No later forwarding setup can fail after the connection-wide Agent
        // admission flag becomes visible to server-opened channels.
        pooled
            .agent_forwarding_accepted
            .store(true, Ordering::Release);
    }
    Ok((channel, x11_route_guard))
}

async fn open_plain_shell(
    pooled: &Arc<PooledSshConnection>,
    cols: u32,
    rows: u32,
    agent_forwarding: bool,
    x11_forwarding: Option<X11ForwardPolicy>,
    x11_route_id: &str,
    x11_connection_owner: Option<X11ConnectionOwner>,
) -> Result<
    (russh::Channel<client::Msg>, Option<X11ForwardRouteGuard>),
    SshTransportError,
> {
    let (channel, x11_route_guard) = open_interactive_shell_channel(
        pooled,
        cols,
        rows,
        DEFAULT_PTY_MODES,
        agent_forwarding,
        x11_forwarding,
        x11_route_id,
        x11_connection_owner,
    )
    .await
    .map_err(|(_, error)| error)?;
    // This marker belongs to this PTY channel, not the shared physical node.
    // Servers may reject optional environment requests; private editor OSC then
    // stays disabled while the regular shell and standard OSC 7 remain usable.
    if channel
        .set_env(
            false,
            OXIDETERM_PRIVATE_OSC_SESSION_ENV,
            OXIDETERM_PRIVATE_OSC_SESSION_VALUE,
        )
        .await
        .is_err()
    {
        tracing::debug!("optional SSH shell integration marker could not be requested");
    }
    channel
        .request_shell(false)
        .await
        .map_err(|error| SshTransportError::Channel(error.to_string()))?;
    Ok((channel, x11_route_guard))
}

impl SshTransportClient {
    pub fn new(config: SshConfig) -> Self {
        Self {
            config,
            prompt_handler: None,
            managed_key_resolver: None,
            connection_progress: None,
        }
    }

    pub fn with_prompt_handler(mut self, prompt_handler: Arc<dyn SshPromptHandler>) -> Self {
        self.prompt_handler = Some(prompt_handler);
        self
    }

    pub fn with_managed_key_resolver(mut self, resolver: ManagedKeyResolver) -> Self {
        self.managed_key_resolver = Some(resolver);
        self
    }

    pub fn with_connection_progress(mut self, reporter: ConnectionProgressReporter) -> Self {
        self.connection_progress = Some(reporter);
        self
    }

    fn report_connection_progress(&self, stage: ConnectionTraceStage) {
        if let Some(reporter) = self.connection_progress.as_ref() {
            reporter.report(stage);
        }
    }

    pub async fn connect_shell(self) -> Result<SshPtyHandle, SshTransportError> {
        self.connect_shell_inner(None).await
    }

    pub async fn connect_shell_with_registry(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
    ) -> Result<SshPtyHandle, SshTransportError> {
        self.connect_shell_with_registry_acquisition(
            registry,
            consumer,
            ShellRegistryAcquisition::Shared,
        )
        .await
    }

    pub async fn connect_shell_with_dedicated_registry(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
        parent_connection_id: Option<String>,
    ) -> Result<SshPtyHandle, SshTransportError> {
        if let Some(parent_connection_id) = parent_connection_id {
            return self
                .connect_shell_with_dedicated_parent(
                    registry,
                    consumer,
                    parent_connection_id,
                )
                .await;
        }
        self.connect_shell_with_registry_acquisition(
            registry,
            consumer,
            ShellRegistryAcquisition::Dedicated,
        )
        .await
    }

    async fn connect_shell_with_registry_acquisition(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
        acquisition: ShellRegistryAcquisition,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let connection = match acquisition {
            ShellRegistryAcquisition::Shared => {
                registry.acquire(self.config.clone(), consumer.clone())
            }
            ShellRegistryAcquisition::Dedicated => {
                registry.acquire_dedicated(self.config.clone(), consumer.clone())
            }
        };
        let connection_id = connection.connection_id().to_string();
        let mut release_guard =
            RegistryConsumerGuard::new(registry.clone(), connection_id.clone(), consumer.clone());

        let pooled = if let Some(existing) = connection.physical::<PooledSshConnection>() {
            if existing.is_closed().await {
                connection.clear_physical().await;
                match self.connect_authenticated_connection().await {
                    Ok(pooled) => {
                        connection.set_physical(pooled.clone());
                        pooled
                    }
                    Err(error) => {
                        let _ = registry
                            .mark_state(&connection_id, ConnectionState::Error(error.to_string()));
                        release_guard.release_now();
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
                    release_guard.release_now();
                    return Err(error);
                }
            }
        };

        let result = Self::open_shell_from_pooled(
            ShellRequestConfig::registry(connection.clone(), None),
            pooled,
            None,
            release_guard.release_tuple(),
            Some(connection.clone()),
        )
        .await;

        match &result {
            Ok(_) => {
                let _ = registry.mark_state(&connection_id, ConnectionState::Active);
                release_guard.disarm();
            }
            Err(error) => {
                if ssh_channel_error_is_transport_lost(&error.to_string()) {
                    let _ = registry
                        .mark_transport_lost_cascade(&connection_id, "channel open failed")
                        .await;
                }
                // A PTY, shell, or forwarding request failure belongs to this
                // terminal consumer. Preserve the node-owned physical transport
                // and let release select Active or Idle from remaining owners.
                release_guard.release_now();
            }
        }

        result
    }

    async fn connect_shell_with_dedicated_parent(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
        parent_connection_id: String,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let connection = registry.acquire_dedicated(self.config.clone(), consumer.clone());
        let connection_id = connection.connection_id().to_string();
        // The parent consumer is tied to the dedicated child entry. Retiring
        // that entry releases this ancestor ownership in the registry.
        let parent_consumer =
            ConnectionConsumer::NodeRouter(format!("{connection_id}:ancestor"));
        let Some(parent) = registry.acquire_consumer_for_connection(
            &parent_connection_id,
            parent_consumer.clone(),
        ) else {
            registry.release(&connection_id, &consumer);
            return Err(SshTransportError::ConnectionFailed(
                "parent SSH connection is unavailable for dedicated terminal".to_string(),
            ));
        };

        let connection = self
            .connect_child_node_via_parent_with_registry(
                registry.clone(),
                consumer.clone(),
                connection,
                parent,
                parent_consumer,
            )
            .await?;
        let Some(pooled) = connection.physical::<PooledSshConnection>() else {
            registry.release(&connection_id, &consumer);
            return Err(SshTransportError::ConnectionFailed(
                "dedicated terminal SSH transport is unavailable".to_string(),
            ));
        };
        let mut release_guard =
            RegistryConsumerGuard::new(registry.clone(), connection_id.clone(), consumer);
        let result = Self::open_shell_from_pooled(
            ShellRequestConfig::registry(connection.clone(), None),
            pooled,
            None,
            release_guard.release_tuple(),
            Some(connection),
        )
        .await;
        match &result {
            Ok(_) => release_guard.disarm(),
            Err(error) => {
                if ssh_channel_error_is_transport_lost(&error.to_string()) {
                    let _ = registry
                        .mark_transport_lost_cascade(
                            &connection_id,
                            "dedicated terminal channel open failed",
                        )
                        .await;
                }
                release_guard.release_now();
            }
        }
        result
    }

    pub async fn connect_shell_on_existing_connection(
        registry: SshConnectionRegistry,
        connection_id: String,
        consumer: ConnectionConsumer,
        cols: u32,
        rows: u32,
    ) -> Result<SshPtyHandle, SshTransportError> {
        Self::connect_shell_on_existing_connection_with_x11_override(
            registry,
            connection_id,
            consumer,
            cols,
            rows,
            None,
        )
        .await
    }

    /// Opens a new channel with the node's current non-secret X11 policy.
    pub async fn connect_shell_on_existing_connection_with_x11_forwarding(
        registry: SshConnectionRegistry,
        connection_id: String,
        consumer: ConnectionConsumer,
        cols: u32,
        rows: u32,
        x11_forwarding: Option<X11ForwardPolicy>,
    ) -> Result<SshPtyHandle, SshTransportError> {
        Self::connect_shell_on_existing_connection_with_x11_override(
            registry,
            connection_id,
            consumer,
            cols,
            rows,
            Some(x11_forwarding),
        )
        .await
    }

    async fn connect_shell_on_existing_connection_with_x11_override(
        registry: SshConnectionRegistry,
        connection_id: String,
        consumer: ConnectionConsumer,
        cols: u32,
        rows: u32,
        x11_forwarding_override: Option<Option<X11ForwardPolicy>>,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let Some(connection) =
            registry.acquire_consumer_for_connection(&connection_id, consumer.clone())
        else {
            return Err(SshTransportError::ConnectionFailed(
                "node SSH connection is unavailable".to_string(),
            ));
        };
        let mut release_guard =
            RegistryConsumerGuard::new(registry.clone(), connection_id.clone(), consumer);
        let Some(pooled) = connection.physical::<PooledSshConnection>() else {
            release_guard.release_now();
            return Err(SshTransportError::ConnectionFailed(
                "node SSH transport is unavailable".to_string(),
            ));
        };
        if pooled.is_closed().await {
            let _ = registry
                .mark_transport_lost_cascade(&connection_id, "terminal found closed transport")
                .await;
            release_guard.release_now();
            return Err(SshTransportError::ConnectionFailed(
                "node SSH transport is closed".to_string(),
            ));
        }

        // Existing terminals borrow only a new session channel. Authentication
        // remains owned by the node's physical connection.
        let result = Self::open_shell_from_pooled(
            ShellRequestConfig::registry(connection.clone(), x11_forwarding_override),
            pooled,
            Some((cols, rows)),
            release_guard.release_tuple(),
            Some(connection),
        )
        .await;

        match &result {
            Ok(_) => {
                let _ = registry.mark_state(&connection_id, ConnectionState::Active);
                release_guard.disarm();
            }
            Err(error) => {
                if ssh_channel_error_is_transport_lost(&error.to_string()) {
                    let _ = registry
                        .mark_transport_lost_cascade(&connection_id, "channel open failed")
                        .await;
                }
                release_guard.release_now();
            }
        }

        result
    }

    pub async fn connect_node_with_registry(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
    ) -> Result<SshConnectionHandle, SshTransportError> {
        let connection = registry.acquire(self.config.clone(), consumer.clone());
        self.connect_existing_node_with_registry(registry, consumer, connection)
            .await
    }

    /// Establish an isolated registry-owned SSH transport for short-lived work.
    ///
    /// Callers must release the consumer after the command completes. Dedicated
    /// entries retire immediately, so bootstrap traffic cannot join the saved
    /// SSH node pool or inherit its long-lived capabilities.
    pub async fn connect_dedicated_node_with_registry(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
    ) -> Result<SshConnectionHandle, SshTransportError> {
        let connection = registry.acquire_dedicated(self.config.clone(), consumer.clone());
        self.connect_existing_node_with_registry(registry, consumer, connection)
            .await
    }

    pub async fn connect_dedicated_consumer_with_registry(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
        parent_connection_id: Option<String>,
    ) -> Result<DedicatedConnectionLease, SshTransportError> {
        let handle = if let Some(parent_connection_id) = parent_connection_id {
            let connection = registry.acquire_dedicated(self.config.clone(), consumer.clone());
            let connection_id = connection.connection_id().to_string();
            let parent_consumer =
                ConnectionConsumer::NodeRouter(format!("{connection_id}:ancestor"));
            let Some(parent) = registry.acquire_consumer_for_connection(
                &parent_connection_id,
                parent_consumer.clone(),
            ) else {
                registry.release(&connection_id, &consumer);
                return Err(SshTransportError::ConnectionFailed(
                    "parent SSH connection is unavailable for dedicated consumer".to_string(),
                ));
            };
            self.connect_child_node_via_parent_with_registry(
                registry.clone(),
                consumer.clone(),
                connection,
                parent,
                parent_consumer,
            )
            .await?
        } else {
            self.connect_dedicated_node_with_registry(registry.clone(), consumer.clone())
                .await?
        };
        Ok(DedicatedConnectionLease::new(registry, handle, consumer))
    }

    pub async fn connect_existing_node_with_registry(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
        connection: SshConnectionHandle,
    ) -> Result<SshConnectionHandle, SshTransportError> {
        let connection_id = connection.connection_id().to_string();
        let mut release_guard =
            RegistryConsumerGuard::new(registry.clone(), connection_id.clone(), consumer.clone());

        // Tauri's connect_tree_node establishes the SSH transport before any
        // terminal is created. Native uses the same registry physical slot so
        // SFTP, forwarding, and later terminal panes all consume the node
        // connection instead of bootstrapping from a terminal shell.
        let pooled = if let Some(existing) = connection.physical::<PooledSshConnection>() {
            if existing.is_closed().await {
                connection.clear_physical().await;
                self.connect_authenticated_connection().await
            } else {
                Ok(existing)
            }
        } else {
            self.connect_authenticated_connection().await
        };

        match pooled {
            Ok(pooled) => {
                connection.set_physical(pooled);
                let _ = registry.set_parent_connection_id(&connection_id, None);
                let _ = registry.mark_state(&connection_id, ConnectionState::Active);
                release_guard.disarm();
                Ok(connection)
            }
            Err(error) => {
                let _ =
                    registry.mark_state(&connection_id, ConnectionState::Error(error.to_string()));
                release_guard.release_now();
                Err(error)
            }
        }
    }

    pub async fn connect_child_node_via_parent_with_registry(
        self,
        registry: SshConnectionRegistry,
        consumer: ConnectionConsumer,
        connection: SshConnectionHandle,
        parent: SshConnectionHandle,
        parent_consumer: ConnectionConsumer,
    ) -> Result<SshConnectionHandle, SshTransportError> {
        let connection_id = connection.connection_id().to_string();
        let parent_connection_id = parent.connection_id().to_string();
        let mut child_release_guard =
            RegistryConsumerGuard::new(registry.clone(), connection_id.clone(), consumer.clone());
        let mut parent_release_guard = RegistryConsumerGuard::new(
            registry.clone(),
            parent_connection_id.clone(),
            parent_consumer.clone(),
        );
        let remote_forward_handler = Arc::new(RwLock::new(None));
        let x11_forward_handler = Arc::new(RwLock::new(None));

        // This is the native equivalent of Tauri establish_tunneled_connection:
        // the child SSH transport is opened over the parent's direct-tcpip
        // channel, then stored in the child's registry entry. The child node
        // still gets its own physical target connection and is resolved through
        // NodeRouter afterwards.
        let pooled = async {
            let Some(parent_pooled) = parent.physical::<PooledSshConnection>() else {
                return Err(SshTransportError::ConnectionFailed(
                    "parent node has no active SSH transport for tunneled connect".to_string(),
                ));
            };
            if parent_pooled.is_closed().await {
                return Err(SshTransportError::ConnectionFailed(
                    "parent SSH transport is closed and cannot open child tunnel".to_string(),
                ));
            }

            self.report_connection_progress(ConnectionTraceStage::OpeningTransport);
            let stream = {
                let parent_handle = &parent_pooled.target;
                open_direct_tcpip_stream(parent_handle, &self.config.host, self.config.port)
                    .await?
            };
            let handler = NativeClientHandler::new(
                self.config.host.clone(),
                self.config.port,
                self.config.strict_host_key_checking,
                self.config.trust_host_key,
                self.config.expected_host_key_fingerprint.clone(),
                self.config.agent_forwarding,
                self.config.identity_agent.clone(),
                self.config.agent_forwarding_socket.clone(),
                remote_forward_handler.clone(),
                x11_forward_handler.clone(),
            )?
            .with_connection_progress(self.connection_progress.clone());
            let auth_banners = handler.auth_banners();
            let agent_forwarding_accepted = handler.agent_forwarding_acceptance();
            let x11_dispatcher = handler.x11_dispatcher();
            self.report_connection_progress(ConnectionTraceStage::SshHandshake);
            let mut target = tokio::time::timeout(
                Duration::from_secs(self.config.timeout_secs),
                client::connect_stream(
                    Arc::new(ssh_client_config(
                        self.config.legacy_ssh_compatibility,
                        &self.config.ssh_algorithms,
                    )?),
                    stream,
                    handler,
                ),
            )
            .await
            .map_err(|_| SshTransportError::Timeout)?
            .map_err(|error| {
                error.with_context("failed to connect child node via parent tunnel")
            })?;
            self.report_connection_progress(ConnectionTraceStage::Authentication);
            authenticate(
                &mut target,
                &self.config,
                self.prompt_handler.as_deref(),
                self.managed_key_resolver.as_ref(),
                self.connection_progress.as_ref(),
            )
            .await?;
            Ok(Arc::new(PooledSshConnection::tunneled(
                target,
                Vec::new(),
                remote_forward_handler,
                x11_forward_handler,
                x11_dispatcher,
                auth_banners,
                agent_forwarding_accepted,
            )))
        }
        .await;

        match pooled {
            Ok(pooled) => {
                connection.set_physical(pooled);
                if let Err(error) = commit_child_registry_ownership(
                    &registry,
                    &connection_id,
                    &parent_connection_id,
                    &parent_consumer,
                    &mut child_release_guard,
                    &mut parent_release_guard,
                ) {
                    // The unregistered authenticated transport has no remaining owner.
                    connection.clear_physical().await;
                    return Err(error);
                }
                Ok(connection)
            }
            Err(error) => {
                let _ =
                    registry.mark_state(&connection_id, ConnectionState::Error(error.to_string()));
                parent_release_guard.release_now();
                child_release_guard.release_now();
                Err(error)
            }
        }
    }

    async fn connect_shell_inner(
        self,
        registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let pooled = self.connect_authenticated_connection().await?;
        Self::open_shell_from_pooled(
            ShellRequestConfig::Owned(self.config),
            pooled,
            None,
            registry_release,
            None,
        )
        .await
    }

    async fn connect_authenticated_connection(
        &self,
    ) -> Result<Arc<PooledSshConnection>, SshTransportError> {
        let remote_forward_handler = Arc::new(RwLock::new(None));
        let x11_forward_handler = Arc::new(RwLock::new(None));
        if self
            .config
            .proxy_chain
            .as_ref()
            .is_some_and(|chain| !chain.is_empty())
        {
            return self
                .connect_authenticated_proxy_connection(remote_forward_handler, x11_forward_handler)
                .await;
        }

        self.connect_direct_authenticated_handle(
            &self.config,
            remote_forward_handler.clone(),
            x11_forward_handler.clone(),
        )
            .await
            .map(|(handle, auth_banners, agent_forwarding_accepted, x11_dispatcher)| {
                PooledSshConnection::direct(
                    handle,
                    remote_forward_handler,
                    x11_forward_handler,
                    x11_dispatcher,
                    auth_banners,
                    agent_forwarding_accepted,
                )
            })
            .map(Arc::new)
    }

    async fn connect_direct_authenticated_handle(
        &self,
        config: &SshConfig,
        remote_forward_handler: RemoteForwardHandlerSlot,
        x11_forward_handler: X11ForwardHandlerSlot,
    ) -> Result<
        (
            client::Handle<NativeClientHandler>,
            AuthBannerSink,
            Arc<AtomicBool>,
            X11ForwardDispatcher,
        ),
        SshTransportError,
    > {
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            timeout_secs = config.timeout_secs,
            upstream_proxy = config.upstream_proxy.is_some(),
            proxy_command = config.proxy_command.is_some(),
            legacy_ssh_compatibility = config.legacy_ssh_compatibility,
            "SSH direct connection starting"
        );
        self.report_connection_progress(ConnectionTraceStage::OpeningTransport);
        let stream: BoxedSshForwardStream = if let Some(proxy_command) = &config.proxy_command {
            if config.upstream_proxy.is_some() {
                return Err(SshTransportError::ConnectionFailed(
                    "ProxyCommand cannot be combined with an upstream proxy".to_string(),
                ));
            }
            Box::new(dial_proxy_command(proxy_command).await?)
        } else {
            log_upstream_proxy_path(&config.host, config.port, config.upstream_proxy.as_ref());
            Box::new(
                dial_initial_tcp(
                    &config.host,
                    config.port,
                    config.timeout_secs,
                    config.upstream_proxy.as_ref(),
                )
                .await?,
            )
        };
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            "SSH TCP stream established"
        );

        let client_config = ssh_client_config(
            config.legacy_ssh_compatibility,
            &config.ssh_algorithms,
        )?;
        let handler = NativeClientHandler::new(
            config.host.clone(),
            config.port,
            config.strict_host_key_checking,
            config.trust_host_key,
            config.expected_host_key_fingerprint.clone(),
            config.agent_forwarding,
            config.identity_agent.clone(),
            config.agent_forwarding_socket.clone(),
            remote_forward_handler,
            x11_forward_handler,
        )?
        .with_connection_progress(self.connection_progress.clone());
        let auth_banners = handler.auth_banners();
        let agent_forwarding_accepted = handler.agent_forwarding_acceptance();
        let x11_dispatcher = handler.x11_dispatcher();
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            "SSH protocol handshake starting"
        );
        self.report_connection_progress(ConnectionTraceStage::SshHandshake);
        let mut handle = tokio::time::timeout(
            Duration::from_secs(config.timeout_secs),
            client::connect_stream(Arc::new(client_config), stream, handler),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(SshTransportError::from)?;
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            "SSH protocol handshake established"
        );

        self.report_connection_progress(ConnectionTraceStage::Authentication);
        authenticate(
            &mut handle,
            config,
            self.prompt_handler.as_deref(),
            self.managed_key_resolver.as_ref(),
            self.connection_progress.as_ref(),
        )
        .await?;
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            "SSH authentication completed"
        );
        Ok((
            handle,
            auth_banners,
            agent_forwarding_accepted,
            x11_dispatcher,
        ))
    }

    async fn connect_authenticated_proxy_connection(
        &self,
        remote_forward_handler: RemoteForwardHandlerSlot,
        x11_forward_handler: X11ForwardHandlerSlot,
    ) -> Result<Arc<PooledSshConnection>, SshTransportError> {
        let chain = self.config.proxy_chain.as_deref().unwrap_or_default();
        if chain.is_empty() {
            return Err(SshTransportError::ConnectionFailed(
                "proxy chain is empty".to_string(),
            ));
        }
        validate_proxy_chain_depth(chain)?;
        tracing::debug!(
            target_host = self.config.host.as_str(),
            target_port = self.config.port,
            proxy_hops = chain.len(),
            "SSH proxy chain connection starting"
        );

        let mut current_stream: Option<russh::ChannelStream<client::Msg>> = None;
        let mut jump_handles = Vec::with_capacity(chain.len());

        for (index, hop) in chain.iter().enumerate() {
            tracing::debug!(
                proxy_hop_index = index + 1,
                proxy_hop_count = chain.len(),
                hop_host = hop.host.as_str(),
                hop_port = hop.port,
                via_existing_stream = current_stream.is_some(),
                "SSH proxy hop connection starting"
            );
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
            tracing::debug!(
                proxy_hop_index = index + 1,
                next_host,
                next_port,
                "SSH opening direct-tcpip tunnel through proxy hop"
            );
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
        let (target, auth_banners, agent_forwarding_accepted, x11_dispatcher) = self
            .connect_target_via_proxy_stream(
                stream,
                self.config.timeout_secs,
                remote_forward_handler.clone(),
                x11_forward_handler.clone(),
            )
            .await?;
        tracing::debug!(
            target_host = self.config.host.as_str(),
            target_port = self.config.port,
            proxy_hops = chain.len(),
            "SSH proxy chain connection established"
        );
        Ok(Arc::new(PooledSshConnection::tunneled(
            target,
            jump_handles,
            remote_forward_handler,
            x11_forward_handler,
            x11_dispatcher,
            auth_banners,
            agent_forwarding_accepted,
        )))
    }

    async fn connect_proxy_hop_direct(
        &self,
        hop: &ProxyHopConfig,
    ) -> Result<client::Handle<NativeClientHandler>, SshTransportError> {
        tracing::debug!(
            hop_host = hop.host.as_str(),
            hop_port = hop.port,
            upstream_proxy = self.config.upstream_proxy.is_some(),
            legacy_ssh_compatibility = hop.legacy_ssh_compatibility,
            "SSH proxy hop direct connection starting"
        );
        log_upstream_proxy_path(&hop.host, hop.port, self.config.upstream_proxy.as_ref());
        let stream = dial_initial_tcp(
            &hop.host,
            hop.port,
            self.config.timeout_secs,
            self.config.upstream_proxy.as_ref(),
        )
        .await?;
        tracing::debug!(
            hop_host = hop.host.as_str(),
            hop_port = hop.port,
            "SSH proxy hop TCP stream established"
        );
        let handler = proxy_hop_handler(hop)?;
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect_stream(
                Arc::new(ssh_client_config(
                    hop.legacy_ssh_compatibility,
                    &hop.ssh_algorithms,
                )?),
                stream,
                handler,
            ),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(SshTransportError::from)?;

        authenticate_proxy_hop(
            &mut handle,
            hop,
            self.prompt_handler.as_deref(),
            self.managed_key_resolver.as_ref(),
        )
        .await?;
        tracing::debug!(
            hop_host = hop.host.as_str(),
            hop_port = hop.port,
            "SSH proxy hop authenticated"
        );
        Ok(handle)
    }

    async fn connect_proxy_hop_via_stream(
        &self,
        hop: &ProxyHopConfig,
        stream: russh::ChannelStream<client::Msg>,
    ) -> Result<client::Handle<NativeClientHandler>, SshTransportError> {
        tracing::debug!(
            hop_host = hop.host.as_str(),
            hop_port = hop.port,
            legacy_ssh_compatibility = hop.legacy_ssh_compatibility,
            "SSH proxy hop tunneled connection starting"
        );
        let handler = proxy_hop_handler(hop)?;
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect_stream(
                Arc::new(ssh_client_config(
                    hop.legacy_ssh_compatibility,
                    &hop.ssh_algorithms,
                )?),
                stream,
                handler,
            ),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| {
            error.with_context(format!(
                "failed to connect via proxy stream to {}:{}",
                hop.host, hop.port
            ))
        })?;

        authenticate_proxy_hop(
            &mut handle,
            hop,
            self.prompt_handler.as_deref(),
            self.managed_key_resolver.as_ref(),
        )
        .await?;
        tracing::debug!(
            hop_host = hop.host.as_str(),
            hop_port = hop.port,
            "SSH proxy hop tunneled authentication completed"
        );
        Ok(handle)
    }

    async fn connect_target_via_proxy_stream(
        &self,
        stream: russh::ChannelStream<client::Msg>,
        timeout_secs: u64,
        remote_forward_handler: RemoteForwardHandlerSlot,
        x11_forward_handler: X11ForwardHandlerSlot,
    ) -> Result<
        (
            client::Handle<NativeClientHandler>,
            AuthBannerSink,
            Arc<AtomicBool>,
            X11ForwardDispatcher,
        ),
        SshTransportError,
    > {
        tracing::debug!(
            target_host = self.config.host.as_str(),
            target_port = self.config.port,
            legacy_ssh_compatibility = self.config.legacy_ssh_compatibility,
            "SSH target connection over proxy stream starting"
        );
        let handler = NativeClientHandler::new(
            self.config.host.clone(),
            self.config.port,
            self.config.strict_host_key_checking,
            self.config.trust_host_key,
            self.config.expected_host_key_fingerprint.clone(),
            self.config.agent_forwarding,
            self.config.identity_agent.clone(),
            self.config.agent_forwarding_socket.clone(),
            remote_forward_handler,
            x11_forward_handler,
        )?;
        let auth_banners = handler.auth_banners();
        let agent_forwarding_accepted = handler.agent_forwarding_acceptance();
        let x11_dispatcher = handler.x11_dispatcher();
        let mut handle = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            client::connect_stream(
                Arc::new(ssh_client_config(
                    self.config.legacy_ssh_compatibility,
                    &self.config.ssh_algorithms,
                )?),
                stream,
                handler,
            ),
        )
        .await
        .map_err(|_| SshTransportError::Timeout)?
        .map_err(|error| {
            error.with_context("failed to connect to target via proxy stream")
        })?;

        authenticate(
            &mut handle,
            &self.config,
            self.prompt_handler.as_deref(),
            self.managed_key_resolver.as_ref(),
            self.connection_progress.as_ref(),
        )
        .await?;
        tracing::debug!(
            target_host = self.config.host.as_str(),
            target_port = self.config.port,
            "SSH target over proxy stream authenticated"
        );
        Ok((
            handle,
            auth_banners,
            agent_forwarding_accepted,
            x11_dispatcher,
        ))
    }

    async fn open_shell_from_pooled(
        request_config: ShellRequestConfig,
        pooled: Arc<PooledSshConnection>,
        dimensions: Option<(u32, u32)>,
        registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
        ssh_connection: Option<SshConnectionHandle>,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let session_id = uuid::Uuid::new_v4().to_string();
        let (command_tx, mut command_rx) =
            mpsc::channel::<SshTransportCommand>(SSH_COMMAND_CHANNEL_CAPACITY);
        // Output is bounded by retained bytes rather than message count. The
        // permit stays attached until the terminal finishes processing a chunk,
        // so a slow or hidden pane cannot accumulate tens of MiB per session.
        let (output_tx, output_rx) = ssh_output_channel();
        let task_session_id = session_id.clone();
        let x11_forwarding = request_config.x11_forwarding();
        let shell_config = request_config.config();
        let (cols, rows) = dimensions.unwrap_or((shell_config.cols, shell_config.rows));
        let deferred_pty = cols == 0 || rows == 0;
        let initial_cols = cols.clamp(1, 500);
        let initial_rows = rows.clamp(1, 200);
        let transport_lost_registry = registry_release
            .as_ref()
            .map(|(registry, _, _)| registry.clone());
        let transport_lost_connection_id = ssh_connection
            .as_ref()
            .map(|connection| connection.connection_id().to_string());
        let visible_terminal_registry = registry_release
            .as_ref()
            .map(|(registry, _, _)| registry.clone());
        let visible_terminal_connection_id = ssh_connection
            .as_ref()
            .map(|connection| connection.connection_id().to_string());
        let auth_banners = pooled.auth_banners.clone();
        // Standalone shells have no registry consumer to retain their physical
        // connection. Keep one explicit owner for the terminal task, while the
        // route holds only a weak reference to avoid a dispatcher cycle.
        let standalone_x11_owner = (registry_release.is_none()
            && x11_forwarding.is_some())
        .then(|| {
            let owner: Arc<dyn Send + Sync> = pooled.clone();
            owner
        });
        let x11_connection_owner = registry_release
            .as_ref()
            .and_then(|(registry, _, _)| {
                ssh_connection.as_ref().map(|connection| {
                    X11ConnectionOwner::Registry {
                        registry: registry.clone(),
                        connection_id: connection.connection_id().to_string(),
                    }
                })
            })
            .or_else(|| {
                standalone_x11_owner
                    .as_ref()
                    .map(|owner| X11ConnectionOwner::Standalone(Arc::downgrade(owner)))
            });

        let opened_shell = if deferred_pty {
            None
        } else {
            Some(
                open_plain_shell(
                    &pooled,
                    initial_cols,
                    initial_rows,
                    shell_config.agent_forwarding,
                    x11_forwarding,
                    &session_id,
                    x11_connection_owner.clone(),
                )
                .await?,
            )
        };
        let mut deferred_request_config = deferred_pty.then_some(request_config);

        tokio::spawn(async move {
            // The bridge lease upgrades the route's weak owner before this guard
            // drops, allowing an established X11 client to outlive the terminal.
            let _standalone_x11_owner = standalone_x11_owner;
            let mut output_batcher = SshOutputBatcher::new();
            let mark_transport_lost = |detail: String| {
                let registry = transport_lost_registry.clone();
                let connection_id = transport_lost_connection_id.clone();
                async move {
                    if let (Some(registry), Some(connection_id)) = (registry, connection_id) {
                        let _ = registry
                            .mark_transport_lost_cascade(&connection_id, detail)
                            .await;
                    }
                }
            };
            let (mut channel, _x11_route_guard) = if let Some(opened_shell) = opened_shell {
                opened_shell
            } else {
                let (pty_cols, pty_rows) = tokio::select! {
                    command = command_rx.recv() => {
                        match command {
                            Some(SshTransportCommand::Resize { cols, rows }) => {
                                ((cols as u32).clamp(1, 500), (rows as u32).clamp(1, 200))
                            }
                            Some(SshTransportCommand::Close) => {
                                let _ = output_tx
                                    .send(format!("\r\n[ssh session {task_session_id} closed]\r\n").into_bytes())
                                    .await;
                                return;
                            }
                            Some(SshTransportCommand::Data(_)) => {
                                tracing::warn!(
                                    "data arrived before deferred SSH PTY resize for session {}, using fallback 120x40",
                                    task_session_id
                                );
                                (120, 40)
                            }
                            None => {
                                let _ = output_tx
                                    .send(format!("\r\n[ssh session {task_session_id} closed]\r\n").into_bytes())
                                    .await;
                                return;
                            }
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(15)) => {
                        tracing::warn!(
                            "deferred SSH PTY resize timed out for session {}, using fallback 120x40",
                            task_session_id
                        );
                        (120, 40)
                    }
                };
                let request_config = deferred_request_config
                    .take()
                    .expect("deferred SSH shell config must remain owned until channel open");
                let open_result = {
                    let shell_config = request_config.config();
                    open_plain_shell(
                        &pooled,
                        pty_cols,
                        pty_rows,
                        shell_config.agent_forwarding,
                        x11_forwarding,
                        &task_session_id,
                        x11_connection_owner,
                    )
                    .await
                };
                // New direct sessions may own authentication material here.
                // Drop it immediately after the deferred channel is opened.
                drop(request_config);
                match open_result {
                    Ok(opened_shell) => opened_shell,
                    Err(error) => {
                        if ssh_channel_error_is_transport_lost(&error.to_string()) {
                            mark_transport_lost(format!("deferred shell startup failed: {error}"))
                                .await;
                        }
                        let _ = output_tx
                            .send(format!("\r\nFailed to initialize shell: {error}\r\n").into_bytes())
                            .await;
                        return;
                    }
                }
            };
            if let (Some(registry), Some(connection_id)) = (
                visible_terminal_registry.as_ref(),
                visible_terminal_connection_id.as_deref(),
            ) {
                // Environment probes start only after the visible shell request
                // so they cannot consume first-login output before the terminal.
                let _ = registry.mark_visible_terminal_ready(connection_id);
            }
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
                                if let Err(error) = channel.data(data.as_slice()).await {
                                    mark_transport_lost(format!(
                                        "terminal input write failed: {error}"
                                    ))
                                    .await;
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
            auth_banners,
            ssh_connection,
            registry_release,
        })
    }

    pub async fn test_connection(self) -> Result<(), SshTransportError> {
        self.connect_authenticated_connection().await.map(|_| ())
    }

    /// Authenticates a routed connection and reports the first untrusted host key.
    pub async fn preflight_route_host_keys(&self) -> (String, u16, HostKeyStatus) {
        match self.connect_authenticated_connection().await {
            Ok(_) => (
                self.config.host.clone(),
                self.config.port,
                HostKeyStatus::Verified,
            ),
            Err(SshTransportError::HostKeyUnknown {
                host,
                port,
                fingerprint,
                key_type,
            }) => (
                host,
                port,
                HostKeyStatus::Unknown {
                    fingerprint,
                    key_type,
                },
            ),
            Err(SshTransportError::HostKeyChanged {
                host,
                port,
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            }) => (
                host,
                port,
                HostKeyStatus::Changed {
                    expected_fingerprint,
                    actual_fingerprint,
                    key_type,
                },
            ),
            Err(error) => (
                self.config.host.clone(),
                self.config.port,
                HostKeyStatus::Error {
                    message: error.to_string(),
                },
            ),
        }
    }
}

#[cfg(test)]
mod agent_forwarding_tests {
    use super::*;

    #[test]
    fn forwarding_response_distinguishes_server_success_and_rejection() {
        assert!(validate_agent_forwarding_response(Some(ChannelMsg::Success)).is_ok());

        let error = validate_agent_forwarding_response(Some(ChannelMsg::Failure)).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("server rejected agent forwarding")
        );
    }

    #[test]
    fn retired_child_releases_parent_ancestor_consumer_before_success_handoff() {
        let registry = SshConnectionRegistry::new(Default::default());
        let parent_consumer = ConnectionConsumer::NodeRouter("parent".to_string());
        let parent = registry.acquire(
            SshConfig {
                host: "parent.example.test".to_string(),
                ..SshConfig::default()
            },
            parent_consumer.clone(),
        );
        let child_consumer = ConnectionConsumer::NodeRouter("child".to_string());
        let child = registry.acquire(
            SshConfig {
                host: "child.example.test".to_string(),
                ..SshConfig::default()
            },
            child_consumer.clone(),
        );
        let parent_connection_id = parent.connection_id().to_string();
        let child_connection_id = child.connection_id().to_string();
        let ancestor_consumer = ConnectionConsumer::NodeRouter("child:ancestor".to_string());
        registry
            .acquire_consumer_for_connection(&parent_connection_id, ancestor_consumer.clone())
            .expect("parent ancestor consumer");
        let mut child_guard = RegistryConsumerGuard::new(
            registry.clone(),
            child_connection_id.clone(),
            child_consumer,
        );
        let mut parent_guard = RegistryConsumerGuard::new(
            registry.clone(),
            parent_connection_id.clone(),
            ancestor_consumer.clone(),
        );
        registry
            .retire_connection(&child_connection_id)
            .expect("retired child connection");

        let result = commit_child_registry_ownership(
            &registry,
            &child_connection_id,
            &parent_connection_id,
            &ancestor_consumer,
            &mut child_guard,
            &mut parent_guard,
        );

        assert!(result.is_err());
        let parent_info = registry
            .get(&parent_connection_id)
            .expect("parent remains registered")
            .info();
        assert!(parent_info.consumers.contains(&parent_consumer));
        assert!(!parent_info.consumers.contains(&ancestor_consumer));
    }

    #[test]
    fn retired_child_releases_parent_ancestor_consumer_after_success_handoff() {
        let registry = SshConnectionRegistry::new(Default::default());
        let parent = registry.acquire(
            SshConfig {
                host: "parent.example.test".to_string(),
                ..SshConfig::default()
            },
            ConnectionConsumer::NodeRouter("parent".to_string()),
        );
        let child_consumer = ConnectionConsumer::NodeRouter("child".to_string());
        let child = registry.acquire(
            SshConfig {
                host: "child.example.test".to_string(),
                ..SshConfig::default()
            },
            child_consumer.clone(),
        );
        let parent_connection_id = parent.connection_id().to_string();
        let child_connection_id = child.connection_id().to_string();
        let ancestor_consumer = ConnectionConsumer::NodeRouter("child:ancestor".to_string());
        registry
            .acquire_consumer_for_connection(&parent_connection_id, ancestor_consumer.clone())
            .expect("parent ancestor consumer");
        let mut child_guard = RegistryConsumerGuard::new(
            registry.clone(),
            child_connection_id.clone(),
            child_consumer,
        );
        let mut parent_guard = RegistryConsumerGuard::new(
            registry.clone(),
            parent_connection_id.clone(),
            ancestor_consumer.clone(),
        );

        commit_child_registry_ownership(
            &registry,
            &child_connection_id,
            &parent_connection_id,
            &ancestor_consumer,
            &mut child_guard,
            &mut parent_guard,
        )
        .expect("child ownership handoff");
        registry
            .retire_connection(&child_connection_id)
            .expect("retired child connection");

        let parent_info = registry
            .get(&parent_connection_id)
            .expect("parent remains registered")
            .info();
        assert!(!parent_info.consumers.contains(&ancestor_consumer));
    }
}
