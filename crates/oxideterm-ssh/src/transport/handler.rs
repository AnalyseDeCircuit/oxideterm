fn ssh_client_config(legacy_ssh_compatibility: bool) -> client::Config {
    let mut config = client::Config {
        inactivity_timeout: None,
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        window_size: 32 * 1024 * 1024,
        maximum_packet_size: 256 * 1024,
        ..client::Config::default()
    };
    if legacy_ssh_compatibility {
        // This is a per-connection opt-in so the default SSH security posture
        // never offers SHA-1 DH, CBC ciphers, or SHA-1 MACs automatically.
        config.preferred = russh::Preferred::legacy_compatibility();
    }
    config
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
    prompt_handler: Option<&dyn SshPromptHandler>,
    managed_key_resolver: Option<&ManagedKeyResolver>,
) -> Result<(), SshTransportError> {
    let config = SshConfig {
        host: hop.host.clone(),
        port: hop.port,
        username: hop.username.clone(),
        auth: hop.auth.clone(),
        strict_host_key_checking: hop.strict_host_key_checking,
        trust_host_key: hop.trust_host_key,
        expected_host_key_fingerprint: hop.expected_host_key_fingerprint.clone(),
        agent_forwarding: hop.agent_forwarding,
        legacy_ssh_compatibility: hop.legacy_ssh_compatibility,
        ..SshConfig::default()
    };
    authenticate_with_options(
        handle,
        &config,
        prompt_handler,
        managed_key_resolver,
        // Proxy hops use the same KBI prompt and fallback rules as target
        // hosts so bastions and MFA jump boxes do not become a special case.
        AuthenticationOptions::default(),
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

    fn kex_done(
        &mut self,
        _shared_secret: Option<&[u8]>,
        names: &russh::Names,
        _session: &mut client::Session,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send {
        tracing::debug!(
            kex = names.kex.as_ref(),
            host_key_algorithm = names.key.as_str(),
            cipher = names.cipher.as_ref(),
            client_mac = names.client_mac.as_ref(),
            server_mac = names.server_mac.as_ref(),
            client_compression = compression_algorithm_label(&names.client_compression),
            server_compression = compression_algorithm_label(&names.server_compression),
            strict_kex = names.strict_kex(),
            "SSH key exchange completed"
        );
        async { Ok(()) }
    }

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
        tracing::debug!(
            host = self.host.as_str(),
            port = self.port,
            host_key_algorithm = server_public_key.algorithm().as_str(),
            host_key_fingerprint = actual_fingerprint.as_str(),
            "SSH server host key received"
        );
        if let Some(expected_fingerprint) = self.expected_host_key_fingerprint.as_deref() {
            if expected_fingerprint != actual_fingerprint {
                tracing::debug!(
                    host = self.host.as_str(),
                    port = self.port,
                    "SSH server host key fingerprint mismatch"
                );
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
                tracing::debug!(
                    host = self.host.as_str(),
                    port = self.port,
                    persisted = trust_host_key,
                    "SSH server host key matched expected fingerprint"
                );
                return Ok(true);
            }
        }

        match verify_host_key(&self.host, self.port, server_public_key)? {
            HostKeyVerification::Verified => {
                tracing::debug!(
                    host = self.host.as_str(),
                    port = self.port,
                    "SSH server host key verified"
                );
                Ok(true)
            }
            HostKeyVerification::Unknown { fingerprint, .. } => {
                if let Some(trust_host_key) = self.trust_host_key {
                    accept_host_key_for_session(&self.host, self.port, fingerprint);
                    if trust_host_key {
                        learn_host_key(&self.host, self.port, server_public_key)?;
                    }
                    tracing::debug!(
                        host = self.host.as_str(),
                        port = self.port,
                        persisted = trust_host_key,
                        "SSH unknown server host key accepted by policy"
                    );
                    return Ok(true);
                }

                if self.strict {
                    tracing::debug!(
                        host = self.host.as_str(),
                        port = self.port,
                        "SSH unknown server host key rejected by strict checking"
                    );
                    Err(SshTransportError::HostKeyUnknown {
                        host: self.host.clone(),
                        port: self.port,
                        fingerprint,
                    })
                } else {
                    learn_host_key(&self.host, self.port, server_public_key)?;
                    tracing::debug!(
                        host = self.host.as_str(),
                        port = self.port,
                        "SSH unknown server host key learned"
                    );
                    Ok(true)
                }
            }
            HostKeyVerification::Changed {
                expected_fingerprint,
                actual_fingerprint,
                ..
            } => {
                tracing::debug!(
                    host = self.host.as_str(),
                    port = self.port,
                    "SSH server host key changed"
                );
                Err(SshTransportError::HostKeyChanged {
                    host: self.host.clone(),
                    port: self.port,
                    expected_fingerprint,
                    actual_fingerprint,
                })
            }
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

fn compression_algorithm_label(compression: &russh::compression::Compression) -> &'static str {
    match compression {
        russh::compression::Compression::None => "none",
        russh::compression::Compression::Zlib => "zlib",
        russh::compression::Compression::ZlibOpenSSH => "zlib@openssh.com",
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
    tracing::debug!(
        auth_method = auth_method_label(&config.auth),
        "SSH authentication flow starting"
    );
    if let Some(result) = try_none_auth_probe(handle, &config.username).await
        && result.success()
    {
        tracing::debug!("SSH none-auth probe accepted by server");
        return Ok(());
    }

    let result = match &config.auth {
        AuthMethod::Password { password } => {
            tracing::debug!("SSH password authentication starting");
            let result = authenticate_password(handle, config, password).await?;
            log_auth_result("password", &result);
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
                tracing::debug!("SSH password keyboard-interactive fallback succeeded");
                return Ok(());
            }
            result
        }
        AuthMethod::Key {
            key_path,
            passphrase,
        } => {
            tracing::debug!(
                key_source = if key_path.trim().is_empty() {
                    "default"
                } else {
                    "file"
                },
                passphrase_supplied = passphrase.is_some(),
                "SSH public-key authentication preparing key"
            );
            let key = load_private_key_material(
                key_path,
                passphrase.as_ref().map(|passphrase| passphrase.as_str()),
            )?;
            let result = authenticate_publickey_best_algo(handle, &config.username, key).await?;
            log_auth_result("publickey", &result);
            result
        }
        AuthMethod::Certificate {
            key_path,
            cert_path,
            passphrase,
        } => {
            tracing::debug!(
                key_source = if key_path.trim().is_empty() {
                    "default"
                } else {
                    "file"
                },
                certificate_source = if cert_path.trim().is_empty() {
                    "empty"
                } else {
                    "file"
                },
                passphrase_supplied = passphrase.is_some(),
                "SSH certificate authentication preparing key"
            );
            let (key, cert) = load_certificate_auth_material(
                key_path,
                cert_path,
                passphrase.as_ref().map(|passphrase| passphrase.as_str()),
            )?;
            let result =
                authenticate_certificate_best_algo(handle, &config.username, key, cert).await?;
            log_auth_result("certificate", &result);
            result
        }
        AuthMethod::Agent => {
            tracing::debug!("SSH agent authentication starting");
            let result = authenticate_agent(handle, config).await?;
            log_auth_result("agent", &result);
            result
        }
        AuthMethod::ManagedKey { key_id, passphrase } => {
            let Some(resolve_managed_key) = managed_key_resolver else {
                return Err(SshTransportError::AuthenticationFailed(
                    "Managed key authentication requires a key resolver".to_string(),
                ));
            };
            tracing::debug!(
                managed_key_configured = !key_id.trim().is_empty(),
                passphrase_supplied = passphrase.is_some(),
                "SSH managed-key authentication preparing key"
            );
            // SshConfig stores only the managed key id. The resolver exposes
            // keychain material for this auth attempt and drops it after decode.
            let private_key = resolve_managed_key(key_id)?;
            let key = load_private_key_from_memory(
                private_key.as_str(),
                passphrase.as_ref().map(|passphrase| passphrase.as_str()),
            )?;
            let result = authenticate_publickey_best_algo(handle, &config.username, key).await?;
            log_auth_result("managed-key", &result);
            result
        }
        AuthMethod::KeyboardInteractive => {
            tracing::debug!("SSH keyboard-interactive authentication starting");
            let result =
                authenticate_keyboard_interactive(handle, &config.username, prompt_handler).await?;
            log_auth_result("keyboard-interactive", &result);
            result
        }
    };

    if result.success() {
        tracing::debug!("SSH authentication flow succeeded");
        Ok(())
    } else if options.interactive_kbi_chain
        && try_keyboard_interactive_chain(handle, &config.username, &result, prompt_handler)
        .await?
    {
        tracing::debug!("SSH chained keyboard-interactive authentication succeeded");
        Ok(())
    } else {
        tracing::debug!("SSH authentication flow failed");
        Err(SshTransportError::AuthenticationFailed(
            authentication_failure_message(&result),
        ))
    }
}

fn auth_method_label(auth: &AuthMethod) -> &'static str {
    match auth {
        AuthMethod::Password { .. } => "password",
        AuthMethod::Key { .. } => "publickey",
        AuthMethod::Agent => "agent",
        AuthMethod::ManagedKey { .. } => "managed-key",
        AuthMethod::Certificate { .. } => "certificate",
        AuthMethod::KeyboardInteractive => "keyboard-interactive",
    }
}

fn auth_result_remaining_methods(result: &client::AuthResult) -> String {
    match result {
        client::AuthResult::Success => String::new(),
        client::AuthResult::Failure {
            remaining_methods, ..
        } => remaining_methods
            .iter()
            .map(|method| String::from(<&str>::from(method)))
            .collect::<Vec<_>>()
            .join(","),
    }
}

fn log_auth_result(method: &'static str, result: &client::AuthResult) {
    match result {
        client::AuthResult::Success => {
            tracing::debug!(auth_method = method, "SSH authentication method accepted");
        }
        client::AuthResult::Failure {
            partial_success, ..
        } => {
            tracing::debug!(
                auth_method = method,
                partial_success,
                remaining_methods = auth_result_remaining_methods(result),
                "SSH authentication method rejected"
            );
        }
    }
}

async fn try_none_auth_probe(
    handle: &mut client::Handle<NativeClientHandler>,
    username: &str,
) -> Option<client::AuthResult> {
    tracing::debug!("SSH none-auth probe starting");
    match tokio::time::timeout(NONE_AUTH_PROBE_TIMEOUT, handle.authenticate_none(username)).await {
        Ok(Ok(result)) => {
            log_auth_result("none", &result);
            Some(result)
        }
        Ok(Err(_)) | Err(_) => {
            tracing::debug!("SSH none-auth probe unavailable");
            None
        }
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
        tracing::debug!("SSH password authentication retry starting");
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
