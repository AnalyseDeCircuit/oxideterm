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
                let mut state = self.state.write();
                if current_generation(&state, &config.id) == generation {
                    state.retry_counters.remove(&config.id);
                    state.servers.insert(config.id.clone(), server);
                    rebuild_tool_index(&mut state);
                } else if let Some(runtime_id) = server.runtime_id {
                    let processes = self.processes.clone();
                    tokio::spawn(async move {
                        let _ = processes.close(&runtime_id).await;
                    });
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
                existing.status = McpServerStatus::Disconnected;
                existing.runtime_id = None;
                existing.endpoint_url = None;
                existing.session_id = None;
                existing.resolved_transport = None;
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
        let runtime_id = self
            .processes
            .spawn(
                config.command.as_deref().unwrap_or_default(),
                &config.args,
                &config.env,
            )
            .await?;
        let connected = async {
            let capabilities = self.initialize_stdio(&runtime_id).await?;
            let tools = if capabilities.tools.is_some() {
                self.stdio_list_tools(&runtime_id).await?
            } else {
                Vec::new()
            };
            let resources = if capabilities.resources.is_some() {
                self.stdio_list_resources(&runtime_id).await?
            } else {
                Vec::new()
            };
            Ok::<_, McpError>(McpServerState {
                config: config.clone(),
                status: McpServerStatus::Connected,
                error: None,
                capabilities: Some(capabilities),
                tools,
                resources,
                runtime_id: Some(runtime_id.clone()),
                endpoint_url: None,
                resolved_transport: Some(McpEffectiveTransport::Stdio),
                session_id: None,
                generation,
            })
        }
        .await;
        if connected.is_err() {
            let _ = self.processes.close(&runtime_id).await;
        }
        connected
    }

    async fn connect_http(
        &self,
        config: McpServerConfig,
        generation: u64,
    ) -> Result<McpServerState, McpError> {
        let token = self.mcp_auth_token(&config);
        let mut endpoint_url = config
            .url
            .clone()
            .ok_or_else(|| McpError::Message("MCP HTTP server requires url".to_string()))?;
        let mut resolved_transport = config.transport.effective();
        let mut init_request = json_rpc_request(
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": resolved_transport.protocol_version(),
                "capabilities": {},
                "clientInfo": { "name": MCP_CLIENT_NAME, "version": MCP_CLIENT_VERSION },
            })),
        );
        let mut init_transport = if resolved_transport == McpEffectiveTransport::LegacySse {
            endpoint_url = self
                .discover_legacy_sse_endpoint(
                    &endpoint_url,
                    &config,
                    token.as_ref().map(|token| token.as_str()),
                )
                .await?;
            self.http_json_rpc_request(
                &endpoint_url,
                init_request.clone(),
                &config,
                token.as_ref().map(|token| token.as_str()),
                None,
                resolved_transport.protocol_version(),
                true,
            )
            .await?
        } else {
            match self
                .http_json_rpc_request(
                    &endpoint_url,
                    init_request.clone(),
                    &config,
                    token.as_ref().map(|token| token.as_str()),
                    None,
                    resolved_transport.protocol_version(),
                    true,
                )
                .await
            {
                Ok(response) => response,
                Err(McpError::HttpStatus(status, _))
                    if matches!(
                        status,
                        StatusCode::BAD_REQUEST
                            | StatusCode::NOT_FOUND
                            | StatusCode::METHOD_NOT_ALLOWED
                    ) =>
                {
                    resolved_transport = McpEffectiveTransport::LegacySse;
                    endpoint_url = self
                        .discover_legacy_sse_endpoint(
                            &endpoint_url,
                            &config,
                            token.as_ref().map(|token| token.as_str()),
                        )
                        .await?;
                    init_request = json_rpc_request(
                        "initialize",
                        Some(serde_json::json!({
                            "protocolVersion": resolved_transport.protocol_version(),
                            "capabilities": {},
                            "clientInfo": { "name": MCP_CLIENT_NAME, "version": MCP_CLIENT_VERSION },
                        })),
                    );
                    self.http_json_rpc_request(
                        &endpoint_url,
                        init_request,
                        &config,
                        token.as_ref().map(|token| token.as_str()),
                        None,
                        resolved_transport.protocol_version(),
                        true,
                    )
                    .await?
                }
                Err(error) => return Err(error),
            }
        };

        endpoint_url = init_transport.endpoint_url;
        let mut session_id = init_transport.session_id.take();
        let initialize_result = extract_result(init_transport.response.take())?;
        let capabilities = initialize_result
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let capabilities = serde_json::from_value::<McpServerCapabilities>(capabilities)
            .map_err(|error| McpError::Message(error.to_string()))?;

        let notify = json_rpc_notification("notifications/initialized", None);
        let notify_transport = self
            .http_json_rpc_request(
                &endpoint_url,
                notify,
                &config,
                token.as_ref().map(|token| token.as_str()),
                session_id.as_deref(),
                resolved_transport.protocol_version(),
                false,
            )
            .await?;
        session_id = notify_transport.session_id.or(session_id);

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
            generation,
        };
        if server
            .capabilities
            .as_ref()
            .and_then(|cap| cap.tools.as_ref())
            .is_some()
        {
            let response = self.http_rpc(&server, "tools/list", None, true).await?;
            server.session_id = response.session_id.or(server.session_id);
            server.tools = parse_tools(extract_result(response.response)?)?;
        }
        if server
            .capabilities
            .as_ref()
            .and_then(|cap| cap.resources.as_ref())
            .is_some()
        {
            let response = self.http_rpc(&server, "resources/list", None, true).await?;
            server.session_id = response.session_id.or(server.session_id);
            server.resources = parse_resources(extract_result(response.response)?)?;
        }
        Ok(server)
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
