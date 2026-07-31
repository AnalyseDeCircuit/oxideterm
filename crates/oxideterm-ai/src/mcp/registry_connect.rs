impl McpRegistry {
    async fn connect(&self, config: McpServerConfig) {
        let generation = {
            let mut state = self.state.write();
            if state.servers.get(&config.id).is_some_and(|server| {
                matches!(
                    server.status,
                    McpServerStatus::Connecting | McpServerStatus::Connected
                )
            }) {
                return;
            }
            let generation = state
                .generations
                .entry(config.id.clone())
                .and_modify(|value| *value = value.saturating_add(1))
                .or_insert(1);
            let generation = *generation;
            if !state.server_order.iter().any(|id| id == &config.id) {
                state.server_order.push(config.id.clone());
            }
            state.servers.insert(
                config.id.clone(),
                McpServerState {
                    status: McpServerStatus::Connecting,
                    ..McpServerState::disconnected(config.clone(), generation)
                },
            );
            generation
        };

        let result = self.connect_inner(config.clone(), generation).await;
        match result {
            Ok(server) => {
                let subscription_server = server.clone();
                let mut start_subscription = false;
                let mut state = self.state.write();
                if current_generation(&state, &config.id) == generation {
                    state.retry_counters.remove(&config.id);
                    state.servers.insert(config.id.clone(), server);
                    rebuild_tool_index(&mut state);
                    start_subscription = subscription_server
                        .protocol
                        .as_ref()
                        .is_some_and(McpProtocol::is_modern)
                        && subscription_filters(&subscription_server).is_some();
                } else if let Some(runtime_id) = server.runtime_id {
                    let processes = self.processes.clone();
                    tokio::spawn(async move {
                        let _ = processes.close(&runtime_id).await;
                    });
                }
                drop(state);
                if start_subscription {
                    self.start_subscription(subscription_server);
                }
            }
            Err(error) => {
                let mut state = self.state.write();
                if current_generation(&state, &config.id) == generation {
                    state.servers.insert(
                        config.id.clone(),
                        McpServerState {
                            status: McpServerStatus::Error,
                            error: Some(error.to_string()),
                            ..McpServerState::disconnected(config.clone(), generation)
                        },
                    );
                    rebuild_tool_index(&mut state);
                    if should_retry_mcp_server(&config) {
                        drop(state);
                        self.schedule_retry(config.id.clone(), generation);
                    }
                }
            }
        }
    }

    async fn disconnect(&self, server_id: &str) {
        let current = {
            let mut state = self.state.write();
            let generation = state
                .generations
                .entry(server_id.to_string())
                .and_modify(|value| *value = value.saturating_add(1))
                .or_insert(1);
            let generation = *generation;
            state.retry_counters.remove(server_id);
            let current = state.servers.get(server_id).cloned();
            if let Some(existing) = state.servers.get_mut(server_id) {
                if let Some(abort) = existing.subscription_abort.take() {
                    abort.abort();
                }
                existing.status = McpServerStatus::Disconnected;
                existing.runtime_id = None;
                existing.endpoint_url = None;
                existing.session_id = None;
                existing.resolved_transport = None;
                existing.protocol = None;
                existing.tools_cache = None;
                existing.resources_cache = None;
                existing.resource_content_cache.clear();
                existing.tools.clear();
                existing.resources.clear();
                existing.error = None;
                existing.generation = generation;
            }
            rebuild_tool_index(&mut state);
            current
        };
        if let Some(runtime_id) = current.and_then(|server| server.runtime_id) {
            let _ = self.processes.close(&runtime_id).await;
        }
    }

    async fn disconnect_and_remove(&self, server_id: &str) {
        self.disconnect(server_id).await;
        let mut state = self.state.write();
        state.servers.remove(server_id);
        state.server_order.retain(|id| id != server_id);
        state
            .tool_index
            .retain(|_, (owner_id, _)| owner_id.as_str() != server_id);
        state.retry_counters.remove(server_id);
    }

    async fn connect_inner(
        &self,
        config: McpServerConfig,
        generation: u64,
    ) -> Result<McpServerState, McpError> {
        match config.transport.effective() {
            McpEffectiveTransport::Stdio => self.connect_stdio(config, generation).await,
            McpEffectiveTransport::StreamableHttp | McpEffectiveTransport::LegacySse => {
                self.connect_http(config, generation).await
            }
        }
    }

    async fn connect_stdio(
        &self,
        config: McpServerConfig,
        generation: u64,
    ) -> Result<McpServerState, McpError> {
        let modern_runtime_id = self.spawn_stdio_process(&config).await?;
        let negotiation = self.discover_modern_stdio(&modern_runtime_id).await;
        let (runtime_id, protocol, capabilities) = match negotiation {
            Ok((protocol, capabilities)) => {
                self.processes
                    .set_eof_shutdown(&modern_runtime_id, true)
                    .await;
                (modern_runtime_id, protocol, capabilities)
            }
            Err(error) if is_recognized_modern_error(&error) => {
                self.processes
                    .terminate_unnegotiated(&modern_runtime_id)
                    .await;
                return Err(error);
            }
            Err(_) => {
                // Restart before the legacy handshake because a probe can
                // leave an older parser in an implementation-defined state.
                self.processes
                    .terminate_unnegotiated(&modern_runtime_id)
                    .await;
                let legacy_runtime_id = self.spawn_stdio_process(&config).await?;
                let standard_protocol = McpProtocol::legacy_streamable_http();
                match self
                    .initialize_legacy_stdio(&legacy_runtime_id, standard_protocol)
                    .await
                {
                    Ok((protocol, capabilities)) => {
                        self.processes
                            .set_eof_shutdown(&legacy_runtime_id, true)
                            .await;
                        (legacy_runtime_id, protocol, capabilities)
                    }
                    Err(error) if error.is_connection_failure() => {
                        self.processes
                            .terminate_unnegotiated(&legacy_runtime_id)
                            .await;
                        let compatibility_runtime_id = self.spawn_stdio_process(&config).await?;
                        let compatibility_protocol = McpProtocol::legacy_content_length_stdio();
                        match self
                            .initialize_legacy_stdio(
                                &compatibility_runtime_id,
                                compatibility_protocol,
                            )
                            .await
                        {
                            Ok((protocol, capabilities)) => {
                                (compatibility_runtime_id, protocol, capabilities)
                            }
                            Err(error) => {
                                self.processes
                                    .terminate_unnegotiated(&compatibility_runtime_id)
                                    .await;
                                return Err(error);
                            }
                        }
                    }
                    Err(error) => {
                        self.processes
                            .terminate_unnegotiated(&legacy_runtime_id)
                            .await;
                        return Err(error);
                    }
                }
            }
        };
        let connected = async {
            let mut server = McpServerState {
                config: config.clone(),
                status: McpServerStatus::Connected,
                error: None,
                capabilities: Some(capabilities),
                tools: Vec::new(),
                resources: Vec::new(),
                runtime_id: Some(runtime_id.clone()),
                endpoint_url: None,
                resolved_transport: Some(McpEffectiveTransport::Stdio),
                session_id: None,
                protocol: Some(protocol),
                tools_cache: None,
                resources_cache: None,
                resource_content_cache: HashMap::new(),
                resource_subscriptions: std::collections::HashSet::new(),
                subscription_abort: None,
                generation,
            };
            if server
                .capabilities
                .as_ref()
                .and_then(|capabilities| capabilities.tools.as_ref())
                .is_some()
            {
                let cache = self.list_tools(&server).await?;
                server.tools = cache.value.clone();
                server.tools_cache = Some(cache);
            }
            if server
                .capabilities
                .as_ref()
                .and_then(|capabilities| capabilities.resources.as_ref())
                .is_some()
            {
                let cache = self.list_resources(&server).await?;
                server.resources = cache.value.clone();
                server.resources_cache = Some(cache);
            }
            Ok::<_, McpError>(server)
        }
        .await;
        if connected.is_err() {
            let _ = self.processes.close(&runtime_id).await;
        }
        connected
    }

    async fn spawn_stdio_process(&self, config: &McpServerConfig) -> Result<String, McpError> {
        self.processes
            .spawn(
                config.command.as_deref().unwrap_or_default(),
                &config.args,
                &config.env,
            )
            .await
    }

    async fn connect_http(
        &self,
        config: McpServerConfig,
        generation: u64,
    ) -> Result<McpServerState, McpError> {
        let token = self.mcp_auth_token(&config);
        let endpoint_url = config
            .url
            .clone()
            .ok_or_else(|| McpError::Message("MCP HTTP server requires url".to_string()))?;
        let auth_token = token.as_ref().map(|token| token.as_str());
        let negotiated = if config.transport.effective() == McpEffectiveTransport::LegacySse {
            self.initialize_http_legacy_sse(&config, &endpoint_url, auth_token)
                .await?
        } else {
            match self
                .discover_modern_http(&config, &endpoint_url, auth_token)
                .await
            {
                Ok(discovered) => discovered,
                Err(error) if is_recognized_modern_http_error(&error) => return Err(error),
                Err(error) if is_legacy_http_probe_error(&error) => match self
                    .initialize_http_legacy_streamable(&config, &endpoint_url, auth_token)
                    .await
                {
                    Ok(initialized) => initialized,
                    Err(error) if is_legacy_http_probe_error(&error) => {
                        self.initialize_http_legacy_sse(&config, &endpoint_url, auth_token)
                            .await?
                    }
                    Err(error) => return Err(error),
                },
                Err(error) => return Err(error),
            }
        };
        let (protocol, resolved_transport, endpoint_url, session_id, capabilities) = negotiated;
        let mut server = McpServerState {
            config: config.clone(),
            status: McpServerStatus::Connected,
            error: None,
            capabilities: Some(capabilities),
            tools: Vec::new(),
            resources: Vec::new(),
            runtime_id: None,
            endpoint_url: Some(endpoint_url),
            resolved_transport: Some(resolved_transport),
            session_id,
            protocol: Some(protocol),
            tools_cache: None,
            resources_cache: None,
            resource_content_cache: HashMap::new(),
            resource_subscriptions: std::collections::HashSet::new(),
            subscription_abort: None,
            generation,
        };
        if server
            .capabilities
            .as_ref()
            .and_then(|cap| cap.tools.as_ref())
            .is_some()
        {
            let cache = self.list_tools(&server).await?;
            server.session_id = cache.session_id.clone().or(server.session_id);
            server.tools = cache.value.clone();
            server.tools_cache = Some(cache);
        }
        if server
            .capabilities
            .as_ref()
            .and_then(|cap| cap.resources.as_ref())
            .is_some()
        {
            let cache = self.list_resources(&server).await?;
            server.session_id = cache.session_id.clone().or(server.session_id);
            server.resources = cache.value.clone();
            server.resources_cache = Some(cache);
        }
        Ok(server)
    }

    async fn discover_modern_http(
        &self,
        config: &McpServerConfig,
        endpoint_url: &str,
        auth_token: Option<&str>,
    ) -> Result<McpHttpNegotiation, McpError> {
        let preferred = McpProtocol::modern_preferred();
        let request = json_rpc_request_for_protocol(&preferred, "server/discover", None)?;
        let response = self
            .http_json_rpc_request(
                endpoint_url,
                request,
                config,
                auth_token,
                None,
                &preferred,
                true,
            )
            .await?;
        let result = extract_result(response.response)?;
        let McpResultEnvelope::Complete(result) = parse_result_for_protocol(&preferred, result)?
        else {
            return Err(McpError::Message(
                "MCP discovery did not return a complete result".to_string(),
            ));
        };
        let discover = serde_json::from_value::<McpDiscoverResult>(result)
            .map_err(|error| McpError::Message(error.to_string()))?;
        let protocol =
            select_supported_modern_version(&discover.supported_versions).ok_or_else(|| {
                McpError::Message(format!(
                    "MCP server does not support {MODERN_PROTOCOL_VERSION}"
                ))
            })?;
        Ok((
            protocol,
            McpEffectiveTransport::StreamableHttp,
            response.endpoint_url,
            None,
            discover.capabilities,
        ))
    }

    async fn initialize_http_legacy_streamable(
        &self,
        config: &McpServerConfig,
        endpoint_url: &str,
        auth_token: Option<&str>,
    ) -> Result<McpHttpNegotiation, McpError> {
        self.initialize_http_legacy(
            config,
            endpoint_url,
            auth_token,
            McpEffectiveTransport::StreamableHttp,
            McpProtocol::legacy_streamable_http(),
        )
        .await
    }

    async fn initialize_http_legacy_sse(
        &self,
        config: &McpServerConfig,
        base_url: &str,
        auth_token: Option<&str>,
    ) -> Result<McpHttpNegotiation, McpError> {
        let endpoint_url = self
            .discover_legacy_sse_endpoint(base_url, config, auth_token)
            .await?;
        self.initialize_http_legacy(
            config,
            &endpoint_url,
            auth_token,
            McpEffectiveTransport::LegacySse,
            McpProtocol::legacy_sse(),
        )
        .await
    }

    async fn initialize_http_legacy(
        &self,
        config: &McpServerConfig,
        endpoint_url: &str,
        auth_token: Option<&str>,
        transport: McpEffectiveTransport,
        protocol: McpProtocol,
    ) -> Result<McpHttpNegotiation, McpError> {
        let request = json_rpc_request(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": protocol.version,
                "capabilities": {},
                "clientInfo": { "name": MCP_CLIENT_NAME, "version": MCP_CLIENT_VERSION },
            })),
        );
        let mut initialized = self
            .http_json_rpc_request(
                endpoint_url,
                request,
                config,
                auth_token,
                None,
                &protocol,
                true,
            )
            .await?;
        let initialize_result = extract_result(initialized.response.take())?;
        let negotiated_version = initialize_result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                McpError::Message(
                    "Legacy MCP initialize response is missing protocolVersion".to_string(),
                )
            })?;
        if negotiated_version != protocol.version {
            return Err(McpError::Message(format!(
                "Legacy MCP server selected unsupported protocol version {negotiated_version}"
            )));
        }
        let capabilities = initialize_result
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let capabilities = serde_json::from_value::<McpServerCapabilities>(capabilities)
            .map_err(|error| McpError::Message(error.to_string()))?;
        let notification = self
            .http_json_rpc_request(
                &initialized.endpoint_url,
                json_rpc_notification("notifications/initialized", None),
                config,
                auth_token,
                initialized.session_id.as_deref(),
                &protocol,
                false,
            )
            .await?;
        Ok((
            protocol,
            transport,
            initialized.endpoint_url,
            notification.session_id.or(initialized.session_id),
            capabilities,
        ))
    }

    fn connected_server(&self, server_id: &str) -> Result<McpServerState, McpError> {
        let state = self.state.read();
        let server = state
            .servers
            .get(server_id)
            .cloned()
            .ok_or_else(|| McpError::NotConnected(server_id.to_string()))?;
        if server.status != McpServerStatus::Connected {
            return Err(McpError::NotConnected(server_id.to_string()));
        }
        Ok(server)
    }

    async fn apply_runtime_error(&self, server_id: &str, generation: u64, message: String) {
        let (runtime_id, config, generation) = {
            let mut state = self.state.write();
            let Some(server) = state.servers.get_mut(server_id) else {
                return;
            };
            if server.generation != generation {
                return;
            }
            let config = server.config.clone();
            server.status = McpServerStatus::Error;
            server.error = Some(message);
            server.tools.clear();
            server.resources.clear();
            server.tools_cache = None;
            server.resources_cache = None;
            server.resource_content_cache.clear();
            if let Some(abort) = server.subscription_abort.take() {
                abort.abort();
            }
            (server.runtime_id.take(), config, generation)
        };
        if let Some(runtime_id) = runtime_id {
            let _ = self.processes.close(&runtime_id).await;
        }
        rebuild_tool_index(&mut self.state.write());
        if should_retry_mcp_server(&config) {
            self.schedule_retry(server_id.to_string(), generation);
        }
    }

    fn start_subscription(&self, server: McpServerState) {
        let registry = self.clone();
        let server_id = server.config.id.clone();
        let generation = server.generation;
        let task = tokio::spawn(async move {
            if let Err(error) = registry.subscription_loop(server).await {
                tracing::debug!("MCP subscription ended: {error}");
            }
        });
        let mut state = self.state.write();
        if let Some(current) = state.servers.get_mut(&server_id)
            && current.generation == generation
            && current.status == McpServerStatus::Connected
        {
            current.subscription_abort = Some(task.abort_handle());
        } else {
            task.abort();
        }
    }

    fn subscribe_to_resource_updates(&self, server_id: &str, generation: u64, uri: &str) {
        let restart = {
            let mut state = self.state.write();
            let Some(server) = state.servers.get_mut(server_id) else {
                return;
            };
            if server.generation != generation
                || server.status != McpServerStatus::Connected
                || !server.protocol.as_ref().is_some_and(McpProtocol::is_modern)
                || !server
                    .capabilities
                    .as_ref()
                    .and_then(|capabilities| capabilities.resources.as_ref())
                    .and_then(|resources| resources.get("subscribe"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                || !server.resource_subscriptions.insert(uri.to_string())
            {
                return;
            }
            // A listen request has immutable filters, so replace it after the
            // first read of a resource that now needs update invalidation.
            if let Some(abort) = server.subscription_abort.take() {
                abort.abort();
            }
            server.clone()
        };
        self.start_subscription(restart);
    }

    fn schedule_retry(&self, server_id: String, generation: u64) {
        let (config, attempt) = {
            let mut state = self.state.write();
            if current_generation(&state, &server_id) != generation {
                return;
            }
            let Some(config) = state
                .servers
                .get(&server_id)
                .map(|server| server.config.clone())
            else {
                return;
            };
            if !should_retry_mcp_server(&config) {
                state.retry_counters.remove(&server_id);
                return;
            }
            let attempt = state
                .retry_counters
                .entry(server_id.clone())
                .and_modify(|value| *value = value.saturating_add(1))
                .or_insert(1);
            let attempt = *attempt;
            if attempt > MCP_MAX_RETRIES {
                tracing::warn!(
                    "[MCP:{}] giving up retry after {} attempts",
                    server_id,
                    MCP_MAX_RETRIES
                );
                state.retry_counters.remove(&server_id);
                return;
            }
            (config, attempt)
        };
        let delay = MCP_RETRY_BASE_DELAY * 2_u32.saturating_pow(attempt.saturating_sub(1));
        let registry = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            let should_connect = {
                let state = registry.state.read();
                current_generation(&state, &server_id) == generation
                    && state.servers.get(&server_id).is_some_and(|server| {
                        should_retry_mcp_server(&server.config)
                            && !matches!(
                                server.status,
                                McpServerStatus::Connected | McpServerStatus::Connecting
                            )
                    })
            };
            if should_connect {
                registry.connect(config).await;
            }
        });
    }
}
