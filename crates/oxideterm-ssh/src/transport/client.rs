async fn request_x11_forwarding_for_shell(
    channel: &russh::Channel<client::Msg>,
    request: &X11SshRequest,
) -> Result<(), russh::Error> {
    channel
        .request_x11(
            true,
            request.single_connection,
            request.auth_protocol_name(),
            request.auth_cookie_hex.clone(),
            request.screen_number,
        )
        .await
}

const SHELL_BOOTSTRAP_STAGE_TIMEOUT: Duration = Duration::from_secs(10);
const SHELL_BOOTSTRAP_CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const SHELL_BOOTSTRAP_ECHO_RECOVERY_INPUT: &str = "stty echo\r";

fn pty_modes_for_shell_bootstrap(staged_bootstrap: bool) -> Vec<(Pty, u32)> {
    if !staged_bootstrap {
        return DEFAULT_PTY_MODES.to_vec();
    }

    DEFAULT_PTY_MODES
        .iter()
        .map(|(mode, value)| {
            // Only the short launcher enters the PTY. The staged wrapper restores
            // echo before replacing the login shell with the integrated shell.
            (*mode, if *mode == Pty::ECHO { 0 } else { *value })
        })
        .collect()
}

async fn send_shell_bootstrap_launcher(
    channel: &russh::Channel<client::Msg>,
    launch_input: Option<&str>,
) -> Result<(), russh::Error> {
    if let Some(input) = launch_input {
        channel.data(input.as_bytes()).await?;
    }
    Ok(())
}

async fn run_hidden_shell_bootstrap_command(
    pooled: &Arc<PooledSshConnection>,
    command: &str,
    command_timeout: Duration,
) -> Result<(), &'static str> {
    let mut channel = pooled
        .target
        .channel_open_session()
        .await
        .map_err(|_| "open-channel")?;
    channel
        .exec(true, command)
        .await
        .map_err(|_| "start-command")?;

    let mut exit_status = None;
    tokio::time::timeout(command_timeout, async {
        while let Some(message) = channel.wait().await {
            match message {
                // Setup output is never forwarded into the user terminal.
                ChannelMsg::ExitStatus {
                    exit_status: status,
                } => exit_status = Some(status),
                // Some SSH servers send EOF before the exit status. Keep
                // draining until Close so a successful staging command is not
                // mistaken for a command without an exit status.
                ChannelMsg::Eof => {}
                ChannelMsg::Close => break,
                _ => {}
            }
        }
    })
    .await
    .map_err(|_| "timeout")?;
    let _ = channel.close().await;

    match exit_status {
        Some(0) => Ok(()),
        Some(_) => Err("non-zero-exit"),
        None => Err("missing-exit-status"),
    }
}

async fn open_interactive_shell_channel(
    pooled: &Arc<PooledSshConnection>,
    cols: u32,
    rows: u32,
    pty_modes: &[(Pty, u32)],
    agent_forwarding: bool,
    x11_forwarding: Option<&X11SshRequest>,
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
    if agent_forwarding {
        let _ = channel.agent_forward(true).await;
    }
    if let Some(request) = x11_forwarding {
        request_x11_forwarding_for_shell(&channel, request)
            .await
            .map_err(|error| ("request-x11", SshTransportError::Channel(error.to_string())))?;
    }
    channel
        .request_shell(false)
        .await
        .map_err(|error| ("request-shell", SshTransportError::Channel(error.to_string())))?;
    Ok(channel)
}

async fn open_shell_with_bootstrap_fallback(
    pooled: &Arc<PooledSshConnection>,
    cols: u32,
    rows: u32,
    agent_forwarding: bool,
    x11_forwarding: Option<&X11SshRequest>,
    bootstrap: Option<&SshShellBootstrap>,
) -> Result<russh::Channel<client::Msg>, SshTransportError> {
    let bootstrap_modes = pty_modes_for_shell_bootstrap(bootstrap.is_some());
    let channel = open_interactive_shell_channel(
        pooled,
        cols,
        rows,
        &bootstrap_modes,
        agent_forwarding,
        x11_forwarding,
    )
    .await
    .map_err(|(_, error)| error)?;
    let Some(bootstrap) = bootstrap else {
        return Ok(channel);
    };

    // The visible request_shell must be the first remote session so PAM MOTD,
    // last-login text, and dynamic login banners are not consumed by staging.
    match run_hidden_shell_bootstrap_command(
        pooled,
        bootstrap.stage_command(),
        SHELL_BOOTSTRAP_STAGE_TIMEOUT,
    )
    .await
    {
        Ok(()) => {
            if let Err(error) =
                send_shell_bootstrap_launcher(&channel, Some(bootstrap.launch_input())).await
            {
                let _ = run_hidden_shell_bootstrap_command(
                    pooled,
                    bootstrap.cleanup_command(),
                    SHELL_BOOTSTRAP_CLEANUP_TIMEOUT,
                )
                .await;
                return Err(SshTransportError::Channel(error.to_string()));
            }
        }
        Err(stage) => {
            tracing::warn!(stage, "SSH shell metadata bootstrap staging failed; using plain shell");
            let _ = run_hidden_shell_bootstrap_command(
                pooled,
                bootstrap.cleanup_command(),
                SHELL_BOOTSTRAP_CLEANUP_TIMEOUT,
            )
            .await;
            channel
                .data(SHELL_BOOTSTRAP_ECHO_RECOVERY_INPUT.as_bytes())
                .await
                .map_err(|error| SshTransportError::Channel(error.to_string()))?;
        }
    }
    Ok(channel)
}

impl SshTransportClient {
    pub fn new(config: SshConfig) -> Self {
        Self {
            config,
            prompt_handler: None,
            managed_key_resolver: None,
            shell_bootstrap: None,
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

    pub fn with_shell_bootstrap(mut self, bootstrap: Option<SshShellBootstrap>) -> Self {
        self.shell_bootstrap = bootstrap.filter(|bootstrap| {
            !bootstrap.stage_command().trim().is_empty()
                && !bootstrap.launch_input().trim().is_empty()
                && !bootstrap.cleanup_command().trim().is_empty()
        });
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

        let result = self
            .open_shell_from_pooled(
                pooled,
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
                } else {
                    connection.clear_physical().await;
                    let _ = registry
                        .mark_state(&connection_id, ConnectionState::Error(error.to_string()));
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
                remote_forward_handler.clone(),
                x11_forward_handler.clone(),
            );
            let auth_banners = handler.auth_banners();
            let mut target = tokio::time::timeout(
                Duration::from_secs(self.config.timeout_secs),
                client::connect_stream(
                    Arc::new(ssh_client_config(self.config.legacy_ssh_compatibility)),
                    stream,
                    handler,
                ),
            )
            .await
            .map_err(|_| SshTransportError::Timeout)?
            .map_err(|error| {
                error.with_context("failed to connect child node via parent tunnel")
            })?;
            authenticate(
                &mut target,
                &self.config,
                self.prompt_handler.as_deref(),
                self.managed_key_resolver.as_ref(),
            )
            .await?;
            Ok(Arc::new(PooledSshConnection::tunneled(
                target,
                Vec::new(),
                remote_forward_handler,
                x11_forward_handler,
                auth_banners,
            )))
        }
        .await;

        match pooled {
            Ok(pooled) => {
                connection.set_physical(pooled);
                let _ = registry.set_parent_connection_id(
                    &connection_id,
                    Some(parent_connection_id),
                );
                let _ = registry.mark_state(&connection_id, ConnectionState::Active);
                child_release_guard.disarm();
                parent_release_guard.disarm();
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
        self.open_shell_from_pooled(pooled, registry_release, None)
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
            .map(|(handle, auth_banners)| {
                PooledSshConnection::direct(
                    handle,
                    remote_forward_handler,
                    x11_forward_handler,
                    auth_banners,
                )
            })
            .map(Arc::new)
    }

    async fn connect_direct_authenticated_handle(
        &self,
        config: &SshConfig,
        remote_forward_handler: RemoteForwardHandlerSlot,
        x11_forward_handler: X11ForwardHandlerSlot,
    ) -> Result<(client::Handle<NativeClientHandler>, AuthBannerSink), SshTransportError> {
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            timeout_secs = config.timeout_secs,
            upstream_proxy = config.upstream_proxy.is_some(),
            legacy_ssh_compatibility = config.legacy_ssh_compatibility,
            "SSH direct connection starting"
        );
        log_upstream_proxy_path(&config.host, config.port, config.upstream_proxy.as_ref());
        let stream = dial_initial_tcp(
            &config.host,
            config.port,
            config.timeout_secs,
            config.upstream_proxy.as_ref(),
        )
        .await?;
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            "SSH TCP stream established"
        );

        let client_config = ssh_client_config(config.legacy_ssh_compatibility);
        let handler = NativeClientHandler::new(
            config.host.clone(),
            config.port,
            config.strict_host_key_checking,
            config.trust_host_key,
            config.expected_host_key_fingerprint.clone(),
            config.agent_forwarding,
            remote_forward_handler,
            x11_forward_handler,
        );
        let auth_banners = handler.auth_banners();
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            "SSH protocol handshake starting"
        );
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

        authenticate(
            &mut handle,
            config,
            self.prompt_handler.as_deref(),
            self.managed_key_resolver.as_ref(),
        )
        .await?;
        tracing::debug!(
            target_host = config.host.as_str(),
            target_port = config.port,
            "SSH authentication completed"
        );
        Ok((handle, auth_banners))
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
        let (target, auth_banners) = self
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
            auth_banners,
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
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect_stream(
                Arc::new(ssh_client_config(hop.legacy_ssh_compatibility)),
                stream,
                proxy_hop_handler(hop),
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
        let mut handle = tokio::time::timeout(
            Duration::from_secs(self.config.timeout_secs),
            client::connect_stream(
                Arc::new(ssh_client_config(hop.legacy_ssh_compatibility)),
                stream,
                proxy_hop_handler(hop),
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
    ) -> Result<(client::Handle<NativeClientHandler>, AuthBannerSink), SshTransportError> {
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
            remote_forward_handler,
            x11_forward_handler,
        );
        let auth_banners = handler.auth_banners();
        let mut handle = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            client::connect_stream(
                Arc::new(ssh_client_config(self.config.legacy_ssh_compatibility)),
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
        )
        .await?;
        tracing::debug!(
            target_host = self.config.host.as_str(),
            target_port = self.config.port,
            "SSH target over proxy stream authenticated"
        );
        Ok((handle, auth_banners))
    }

    async fn open_shell_from_pooled(
        self,
        pooled: Arc<PooledSshConnection>,
        registry_release: Option<(SshConnectionRegistry, String, ConnectionConsumer)>,
        ssh_connection: Option<SshConnectionHandle>,
    ) -> Result<SshPtyHandle, SshTransportError> {
        let shell_bootstrap = self.shell_bootstrap.clone();
        let session_id = uuid::Uuid::new_v4().to_string();
        let (command_tx, mut command_rx) =
            mpsc::channel::<SshTransportCommand>(SSH_COMMAND_CHANNEL_CAPACITY);
        let (output_tx, output_rx) = mpsc::channel::<Vec<u8>>(SSH_OUTPUT_CHANNEL_CAPACITY);
        let task_session_id = session_id.clone();
        let agent_forwarding = self.config.agent_forwarding;
        let x11_forwarding = self.config.x11_forwarding.clone();
        let deferred_pty = self.config.cols == 0 || self.config.rows == 0;
        let initial_cols = self.config.cols.clamp(1, 500);
        let initial_rows = self.config.rows.clamp(1, 200);
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

        let channel = if deferred_pty {
            None
        } else {
            Some(
                open_shell_with_bootstrap_fallback(
                    &pooled,
                    initial_cols,
                    initial_rows,
                    agent_forwarding,
                    x11_forwarding.as_ref(),
                    shell_bootstrap.as_ref(),
                )
                .await?,
            )
        };

        tokio::spawn(async move {
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
            let mut channel = if let Some(channel) = channel {
                channel
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
                match open_shell_with_bootstrap_fallback(
                    &pooled,
                    pty_cols,
                    pty_rows,
                    agent_forwarding,
                    x11_forwarding.as_ref(),
                    shell_bootstrap.as_ref(),
                )
                .await
                {
                    Ok(channel) => channel,
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
                // Tauri starts remote environment detection only after the
                // first visible shell has been requested, so hidden probes do
                // not consume Ubuntu's first-login MOTD before the terminal.
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

}

#[cfg(test)]
mod shell_bootstrap_transport_tests {
    use super::{Pty, pty_modes_for_shell_bootstrap};

    #[test]
    fn staged_bootstrap_disables_only_initial_pty_echo() {
        let plain = pty_modes_for_shell_bootstrap(false);
        let staged = pty_modes_for_shell_bootstrap(true);

        assert_eq!(
            plain.iter().find(|(mode, _)| *mode == Pty::ECHO),
            Some(&(Pty::ECHO, 1))
        );
        assert_eq!(
            staged.iter().find(|(mode, _)| *mode == Pty::ECHO),
            Some(&(Pty::ECHO, 0))
        );
        assert!(plain.iter().all(|(mode, value)| {
            *mode == Pty::ECHO
                || staged
                    .iter()
                    .any(|(candidate, staged_value)| candidate == mode && staged_value == value)
        }));
    }
}
