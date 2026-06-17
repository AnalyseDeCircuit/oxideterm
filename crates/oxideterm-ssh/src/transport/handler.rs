fn ssh_client_config() -> client::Config {
    client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        window_size: 32 * 1024 * 1024,
        maximum_packet_size: 256 * 1024,
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

fn validate_proxy_chain_depth(chain: &[ProxyHopConfig]) -> Result<(), SshTransportError> {
    if chain.len() > MAX_PROXY_CHAIN_DEPTH {
        return Err(SshTransportError::ConnectionFailed(format!(
            "proxy chain too long: {} hops (max {})",
            chain.len(),
            MAX_PROXY_CHAIN_DEPTH
        )));
    }
    Ok(())
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
        Arc::new(RwLock::new(None)),
    )
}

async fn authenticate_proxy_hop(
    handle: &mut client::Handle<NativeClientHandler>,
    hop: &ProxyHopConfig,
    managed_key_resolver: Option<&ManagedKeyResolver>,
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
    authenticate_with_options(
        handle,
        &config,
        None,
        managed_key_resolver,
        AuthenticationOptions {
            password_kbi_fallback: false,
            interactive_kbi_chain: false,
        },
    )
    .await
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
    x11_forward_handler: X11ForwardHandlerSlot,
    auth_banners: AuthBannerSink,
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
        x11_forward_handler: X11ForwardHandlerSlot,
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
            x11_forward_handler,
            auth_banners: new_auth_banner_sink(),
        }
    }

    fn auth_banners(&self) -> AuthBannerSink {
        self.auth_banners.clone()
    }
}

impl client::Handler for NativeClientHandler {
    type Error = SshTransportError;

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        // Authentication banners are server-auth messages. They are stored
        // separately from shell output so the first visible terminal can show
        // them once, matching Tauri's pending-auth-banner boundary.
        if let Some(sanitized) = sanitize_auth_banner(banner) {
            self.auth_banners.lock().push(sanitized);
        }
        Ok(())
    }

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
            if let Some(trust_host_key) = self.trust_host_key {
                accept_host_key_for_session(&self.host, self.port, actual_fingerprint.clone());
                if trust_host_key {
                    learn_host_key(&self.host, self.port, server_public_key)?;
                }
                return Ok(true);
            }
        }

        match verify_host_key(&self.host, self.port, server_public_key)? {
            HostKeyVerification::Verified => Ok(true),
            HostKeyVerification::Unknown { fingerprint, .. } => {
                if let Some(trust_host_key) = self.trust_host_key {
                    accept_host_key_for_session(&self.host, self.port, fingerprint);
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
        let Some(registration) = self.remote_forward_handler.read().clone() else {
            let _ = channel.eof().await;
            return Ok(());
        };

        let event = RemoteForwardedTcpIp {
            connection_id: registration.connection_id.clone(),
            connected_address: connected_address.to_string(),
            connected_port: connected_port as u16,
            originator_address: originator_address.to_string(),
            originator_port: originator_port as u16,
            stream: Box::new(channel.into_stream()),
        };
        tokio::spawn(async move {
            registration.handler.handle_remote_forward(event).await;
        });
        Ok(())
    }

    async fn server_channel_open_x11(
        &mut self,
        channel: Channel<client::Msg>,
        originator_address: &str,
        originator_port: u32,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let Some(registration) = self.x11_forward_handler.read().clone() else {
            let _ = channel.eof().await;
            return Ok(());
        };

        let event = X11ForwardedChannel {
            connection_id: registration.connection_id.clone(),
            originator_address: originator_address.to_string(),
            originator_port: originator_port as u16,
            stream: Box::new(channel.into_stream()),
        };
        tokio::spawn(async move {
            registration.handler.handle_x11_forward(event).await;
        });
        Ok(())
    }
}

async fn authenticate(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
    prompt_handler: Option<&dyn SshPromptHandler>,
    managed_key_resolver: Option<&ManagedKeyResolver>,
) -> Result<(), SshTransportError> {
    authenticate_with_options(
        handle,
        config,
        prompt_handler,
        managed_key_resolver,
        AuthenticationOptions::default(),
    )
    .await
}

#[derive(Clone, Copy)]
struct AuthenticationOptions {
    password_kbi_fallback: bool,
    interactive_kbi_chain: bool,
}

impl Default for AuthenticationOptions {
    fn default() -> Self {
        Self {
            password_kbi_fallback: true,
            interactive_kbi_chain: true,
        }
    }
}

async fn authenticate_with_options(
    handle: &mut client::Handle<NativeClientHandler>,
    config: &SshConfig,
    prompt_handler: Option<&dyn SshPromptHandler>,
    managed_key_resolver: Option<&ManagedKeyResolver>,
    options: AuthenticationOptions,
) -> Result<(), SshTransportError> {
    if let Some(result) = try_none_auth_probe(handle, &config.username).await
        && result.success()
    {
        return Ok(());
    }

    let result = match &config.auth {
        AuthMethod::Password { password } => {
            let result = authenticate_password(handle, config, password).await?;
            if options.password_kbi_fallback
                && try_password_as_keyboard_interactive(
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
        AuthMethod::ManagedKey { key_id, passphrase } => {
            let Some(resolve_managed_key) = managed_key_resolver else {
                return Err(SshTransportError::AuthenticationFailed(
                    "Managed key authentication requires a key resolver".to_string(),
                ));
            };
            // SshConfig stores only the managed key id. The resolver exposes
            // keychain material for this auth attempt and drops it after decode.
            let private_key = resolve_managed_key(key_id)?;
            let key = load_private_key_from_memory(
                private_key.as_str(),
                passphrase.as_ref().map(|passphrase| passphrase.as_str()),
            )?;
            authenticate_publickey_best_algo(handle, &config.username, key).await?
        }
        AuthMethod::KeyboardInteractive => {
            authenticate_keyboard_interactive(handle, &config.username, prompt_handler).await?
        }
    };

    if result.success() {
        Ok(())
    } else if options.interactive_kbi_chain
        && try_keyboard_interactive_chain(handle, &config.username, &result, prompt_handler)
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
