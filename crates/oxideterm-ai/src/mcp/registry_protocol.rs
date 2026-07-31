impl McpRegistry {
    async fn discover_modern_stdio(
        &self,
        runtime_id: &str,
    ) -> Result<(McpProtocol, McpServerCapabilities), McpError> {
        let preferred = McpProtocol::modern_preferred();
        let discover_params = request_params_for_protocol(&preferred, None, None, None)?;
        match self
            .processes
            .send_probe_request_with_framing(
                runtime_id,
                "server/discover",
                discover_params,
                MCP_DISCOVERY_TIMEOUT,
                McpStdioFraming::LineDelimited,
            )
            .await
        {
            Ok(result) => {
                let McpResultEnvelope::Complete(result) =
                    parse_result_for_protocol(&preferred, result)?
                else {
                    return Err(McpError::Message(
                        "MCP discovery did not return a complete result".to_string(),
                    ));
                };
                let discover = serde_json::from_value::<McpDiscoverResult>(result)
                    .map_err(|error| McpError::Message(error.to_string()))?;
                let protocol = select_supported_modern_version(&discover.supported_versions)
                    .ok_or_else(|| {
                        McpError::Message(format!(
                            "MCP server does not support {MODERN_PROTOCOL_VERSION}"
                        ))
                    })?;
                Ok((protocol, discover.capabilities))
            }
            Err(error) if is_recognized_modern_error(&error) => {
                let supported = supported_versions_from_error(&error);
                if select_supported_modern_version(&supported).is_some() {
                    return Err(McpError::Message(
                        "MCP discovery rejected a protocol version it advertises".to_string(),
                    ));
                }
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    async fn initialize_legacy_stdio(
        &self,
        runtime_id: &str,
        protocol: McpProtocol,
    ) -> Result<(McpProtocol, McpServerCapabilities), McpError> {
        let init = self
            .processes
            .send_protocol_request(
                runtime_id,
                &protocol,
                "initialize",
                serde_json::json!({
                    "protocolVersion": protocol.version,
                    "capabilities": {},
                    "clientInfo": { "name": MCP_CLIENT_NAME, "version": MCP_CLIENT_VERSION },
                }),
            )
            .await?;
        let negotiated_version = init
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
        let capabilities = init
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        self.processes
            .send_protocol_request(
                runtime_id,
                &protocol,
                "notifications/initialized",
                serde_json::json!({}),
            )
            .await?;
        let capabilities = serde_json::from_value(capabilities)
            .map_err(|error| McpError::Message(error.to_string()))?;
        Ok((protocol, capabilities))
    }

    async fn stdio_rpc(
        &self,
        server: &McpServerState,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value, McpError> {
        self.stdio_rpc_round(server, method, params, None, None)
            .await
    }

    async fn stdio_rpc_round(
        &self,
        server: &McpServerState,
        method: &str,
        params: Option<Value>,
        input_responses: Option<Value>,
        request_state: Option<&str>,
    ) -> Result<Value, McpError> {
        let runtime_id = server
            .runtime_id
            .as_deref()
            .ok_or_else(|| McpError::NotConnected(server.config.id.clone()))?;
        let protocol = server
            .protocol
            .as_ref()
            .ok_or_else(|| McpError::Message("MCP protocol was not negotiated".to_string()))?;
        let params = request_params_for_protocol(protocol, params, input_responses, request_state)?;
        self.processes
            .send_protocol_request(runtime_id, protocol, method, params)
            .await
    }

    async fn stdio_rpc_until_completion(
        &self,
        server: &McpServerState,
        method: &str,
        params: Option<Value>,
    ) -> Result<McpPendingStdioRequest, McpError> {
        let runtime_id = server
            .runtime_id
            .as_deref()
            .ok_or_else(|| McpError::NotConnected(server.config.id.clone()))?;
        let protocol = server
            .protocol
            .as_ref()
            .ok_or_else(|| McpError::Message("MCP protocol was not negotiated".to_string()))?;
        let params = request_params_for_protocol(protocol, params, None, None)?;
        self.processes
            .begin_protocol_request_until_completion(runtime_id, protocol, method, params)
            .await
    }

    async fn list_tools(
        &self,
        server: &McpServerState,
    ) -> Result<McpCachedResult<Vec<McpToolSchema>>, McpError> {
        if let Some(cache) = &server.tools_cache
            && cache.is_fresh()
        {
            return Ok(cache.clone());
        }
        let mut tools = Vec::new();
        let mut cursor = None;
        let mut seen_cursors = std::collections::HashSet::new();
        let mut cache_hint = None;
        let mut session_id = None;
        loop {
            let params = cursor
                .as_ref()
                .map(|cursor| serde_json::json!({ "cursor": cursor }));
            let result = if server.runtime_id.is_some() {
                self.stdio_rpc(server, "tools/list", params).await?
            } else {
                let response = self.http_rpc(server, "tools/list", params, true).await?;
                session_id = response.session_id.or(session_id);
                extract_result(response.response)?
            };
            let result = complete_result(server, result)?;
            cache_hint = Some(merge_cache_hint(cache_hint, parse_cache_hint(&result)));
            tools.extend(parse_tools(result.clone())?);
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            let Some(next_cursor) = cursor.as_ref() else {
                break;
            };
            if !seen_cursors.insert(next_cursor.clone()) {
                return Err(McpError::Message(
                    "MCP tools/list repeated a pagination cursor".to_string(),
                ));
            }
        }
        let tools = tools
            .into_iter()
            .filter(|tool| {
                if server.resolved_transport == Some(McpEffectiveTransport::StreamableHttp)
                    && server.protocol.as_ref().is_some_and(McpProtocol::is_modern)
                    && let Err(error) = mcp_tool_header_bindings(&tool.input_schema)
                {
                    tracing::warn!(
                        "Ignoring MCP tool {} with invalid x-mcp-header: {}",
                        tool.name,
                        error
                    );
                    return false;
                }
                true
            })
            .collect();
        Ok(McpCachedResult::new(tools, cache_hint.unwrap_or_default()).with_session_id(session_id))
    }

    async fn list_resources(
        &self,
        server: &McpServerState,
    ) -> Result<McpCachedResult<Vec<McpResource>>, McpError> {
        if let Some(cache) = &server.resources_cache
            && cache.is_fresh()
        {
            return Ok(cache.clone());
        }
        let mut resources = Vec::new();
        let mut cursor = None;
        let mut seen_cursors = std::collections::HashSet::new();
        let mut cache_hint = None;
        let mut session_id = None;
        loop {
            let params = cursor
                .as_ref()
                .map(|cursor| serde_json::json!({ "cursor": cursor }));
            let result = if server.runtime_id.is_some() {
                self.stdio_rpc(server, "resources/list", params).await?
            } else {
                let response = self
                    .http_rpc(server, "resources/list", params, true)
                    .await?;
                session_id = response.session_id.or(session_id);
                extract_result(response.response)?
            };
            let result = complete_result(server, result)?;
            cache_hint = Some(merge_cache_hint(cache_hint, parse_cache_hint(&result)));
            resources.extend(parse_resources(result.clone())?);
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            let Some(next_cursor) = cursor.as_ref() else {
                break;
            };
            if !seen_cursors.insert(next_cursor.clone()) {
                return Err(McpError::Message(
                    "MCP resources/list repeated a pagination cursor".to_string(),
                ));
            }
        }
        Ok(
            McpCachedResult::new(resources, cache_hint.unwrap_or_default())
                .with_session_id(session_id),
        )
    }

    async fn call_tool(
        &self,
        server: &McpServerState,
        tool_name: &str,
        args: Value,
    ) -> Result<McpCallToolResult, McpError> {
        let params = serde_json::json!({ "name": tool_name, "arguments": args });
        let result = if server.runtime_id.is_some() {
            self.stdio_rpc(server, "tools/call", Some(params.clone()))
                .await?
        } else {
            extract_result(
                self.http_rpc(server, "tools/call", Some(params.clone()), true)
                    .await?
                    .response,
            )?
        };
        let result = self
            .drive_modern_result(server, "tools/call", Some(params), result)
            .await?;
        serde_json::from_value(result).map_err(|error| McpError::Message(error.to_string()))
    }

    async fn read_resource_inner(
        &self,
        server: &McpServerState,
        uri: &str,
    ) -> Result<Vec<McpResourceContent>, McpError> {
        if server.protocol.as_ref().is_some_and(McpProtocol::is_modern)
            && let Some(cache) = server.resource_content_cache.get(uri)
            && cache.is_fresh()
        {
            return Ok(cache.value.clone());
        }
        let params = serde_json::json!({ "uri": uri });
        let result = if server.runtime_id.is_some() {
            self.stdio_rpc(server, "resources/read", Some(params.clone()))
                .await?
        } else {
            extract_result(
                self.http_rpc(server, "resources/read", Some(params.clone()), true)
                    .await?
                    .response,
            )?
        };
        let result = self
            .drive_modern_result(server, "resources/read", Some(params), result)
            .await?;
        let hint = parse_cache_hint(&result);
        let contents = result
            .get("contents")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if contents.is_empty() {
            return Err(McpError::Message(format!(
                "Empty resource response for {uri}"
            )));
        }
        let contents: Vec<McpResourceContent> = serde_json::from_value(Value::Array(contents))
            .map_err(|error| McpError::Message(error.to_string()))?;
        if server.protocol.as_ref().is_some_and(McpProtocol::is_modern) {
            {
                let mut state = self.state.write();
                if let Some(current) = state.servers.get_mut(&server.config.id)
                    && current.generation == server.generation
                {
                    current.resource_content_cache.insert(
                        uri.to_string(),
                        McpCachedResult::new(contents.clone(), hint),
                    );
                }
            }
            self.subscribe_to_resource_updates(&server.config.id, server.generation, uri);
        }
        Ok(contents)
    }

    async fn http_rpc(
        &self,
        server: &McpServerState,
        method: &str,
        params: Option<Value>,
        expect_json: bool,
    ) -> Result<HttpRequestResult, McpError> {
        self.http_rpc_round(server, method, params, None, None, expect_json)
            .await
    }

    async fn http_rpc_round(
        &self,
        server: &McpServerState,
        method: &str,
        params: Option<Value>,
        input_responses: Option<Value>,
        request_state: Option<&str>,
        expect_json: bool,
    ) -> Result<HttpRequestResult, McpError> {
        let protocol = server
            .protocol
            .as_ref()
            .ok_or_else(|| McpError::Message("MCP protocol was not negotiated".to_string()))?;
        let endpoint = server
            .endpoint_url
            .as_deref()
            .or(server.config.url.as_deref())
            .ok_or_else(|| McpError::Message("MCP HTTP server requires url".to_string()))?;
        let token = self.mcp_auth_token(&server.config);
        let request = if method.starts_with("notifications/") {
            json_rpc_notification(method, params)
        } else {
            json_rpc_request(
                method,
                Some(request_params_for_protocol(
                    protocol,
                    params,
                    input_responses,
                    request_state,
                )?),
            )
        };
        self.http_json_rpc_request(
            endpoint,
            request,
            &server.config,
            token.as_ref().map(|token| token.as_str()),
            server.session_id.as_deref(),
            protocol,
            expect_json,
        )
        .await
    }

    async fn rpc_result_round(
        &self,
        server: &McpServerState,
        method: &str,
        params: Option<Value>,
        input_responses: Option<Value>,
        request_state: Option<&str>,
    ) -> Result<Value, McpError> {
        if server.runtime_id.is_some() {
            self.stdio_rpc_round(server, method, params, input_responses, request_state)
                .await
        } else {
            extract_result(
                self.http_rpc_round(server, method, params, input_responses, request_state, true)
                    .await?
                    .response,
            )
        }
    }

    async fn drive_modern_result(
        &self,
        server: &McpServerState,
        method: &str,
        original_params: Option<Value>,
        mut result: Value,
    ) -> Result<Value, McpError> {
        let protocol = server
            .protocol
            .as_ref()
            .ok_or_else(|| McpError::Message("MCP protocol was not negotiated".to_string()))?;
        if !protocol.is_modern() {
            return Ok(result);
        }
        for _ in 0..MCP_MAX_MULTI_ROUND_TRIPS {
            match parse_result_for_protocol(protocol, result)? {
                McpResultEnvelope::Complete(result) => return Ok(result),
                McpResultEnvelope::InputRequired(input) => {
                    if !input.input_requests.is_empty() {
                        return Err(McpError::Message(
                            "MCP server requested a client capability that OxideTerm did not advertise"
                                .to_string(),
                        ));
                    }
                    // A state-only round is a load-shedding retry and needs no user interaction.
                    result = self
                        .rpc_result_round(
                            server,
                            method,
                            original_params.clone(),
                            None,
                            input.request_state.as_deref(),
                        )
                        .await?;
                }
                McpResultEnvelope::Task(task) => {
                    if !server_supports_tasks(server) {
                        return Err(McpError::Message(
                            "MCP server returned a task without advertising the Tasks extension"
                                .to_string(),
                        ));
                    }
                    return self.poll_task(server, *task).await;
                }
            }
        }
        Err(McpError::Message(
            "MCP request exceeded the multi-round trip limit".to_string(),
        ))
    }

    async fn poll_task(
        &self,
        server: &McpServerState,
        mut task: McpTask,
    ) -> Result<Value, McpError> {
        let started_at = std::time::Instant::now();
        let server_ttl = task.ttl_ms.map(Duration::from_millis);
        // Dropping the caller's future cancels a still-running server task.
        let mut cancellation = McpTaskCancellationGuard::new(self.clone(), server, &task.task_id);
        loop {
            match task.status {
                McpTaskStatus::Completed => {
                    cancellation.disarm();
                    return task.result.ok_or_else(|| {
                        McpError::Message(format!(
                            "MCP task {} completed without a result",
                            task.task_id
                        ))
                    });
                }
                McpTaskStatus::Failed => {
                    cancellation.disarm();
                    if let Some(error) = task.error.as_ref() {
                        return Err(rpc_error_from_value(error));
                    }
                    return Err(McpError::Message(
                        task.status_message
                            .unwrap_or_else(|| format!("MCP task {} failed", task.task_id)),
                    ));
                }
                McpTaskStatus::Cancelled => {
                    cancellation.disarm();
                    return Err(McpError::Message(format!(
                        "MCP task {} was cancelled",
                        task.task_id
                    )));
                }
                McpTaskStatus::InputRequired => {
                    return Err(McpError::Message(format!(
                        "MCP task {} requires {} unsupported client input request(s)",
                        task.task_id,
                        task.input_requests.len()
                    )));
                }
                McpTaskStatus::Working => {}
            }
            let wait_limit = server_ttl
                .map(|ttl| ttl.min(MCP_TASK_MAX_WAIT))
                .unwrap_or(MCP_TASK_MAX_WAIT);
            if started_at.elapsed() >= wait_limit {
                return Err(McpError::Timeout(format!("task {}", task.task_id)));
            }
            let interval = Duration::from_millis(
                task.poll_interval_ms
                    .unwrap_or(MCP_TASK_DEFAULT_POLL_INTERVAL.as_millis() as u64),
            )
            .clamp(MCP_TASK_MIN_POLL_INTERVAL, MCP_TASK_MAX_POLL_INTERVAL);
            tokio::time::sleep(interval).await;
            let result = self
                .rpc_result_round(
                    server,
                    "tasks/get",
                    Some(serde_json::json!({ "taskId": task.task_id })),
                    None,
                    None,
                )
                .await?;
            let result = complete_result(server, result)?;
            task = serde_json::from_value(result)
                .map_err(|error| McpError::Message(error.to_string()))?;
        }
    }

    async fn subscription_loop(&self, server: McpServerState) -> Result<(), McpError> {
        let notifications = subscription_filters(&server).ok_or_else(|| {
            McpError::Message("MCP server has no subscribable notifications".to_string())
        })?;
        if let Some(runtime_id) = server.runtime_id.as_deref() {
            let mut receiver = self.processes.subscribe_notifications(runtime_id).await?;
            let pending = self
                .stdio_rpc_until_completion(
                    &server,
                    "subscriptions/listen",
                    Some(serde_json::json!({ "notifications": notifications })),
                )
                .await?;
            let subscription_id = Value::from(pending.request_id);
            let listen = pending.wait();
            let mut acknowledged_filter = None;
            tokio::pin!(listen);
            loop {
                tokio::select! {
                    result = &mut listen => {
                        complete_result(&server, result?)?;
                        return Ok(());
                    }
                    notification = receiver.recv() => {
                        match notification {
                            Ok(notification) => {
                                if !notification_matches_subscription(
                                    &notification,
                                    &subscription_id,
                                ) {
                                    continue;
                                }
                                if acknowledged_filter.is_none() {
                                    if notification.get("method").and_then(Value::as_str)
                                        != Some("notifications/subscriptions/acknowledged")
                                    {
                                        return Err(McpError::Message(
                                            "MCP subscription sent a notification before acknowledgment"
                                                .to_string(),
                                        ));
                                    }
                                    acknowledged_filter =
                                        Some(acknowledged_subscription_filter(&notification)?);
                                    continue;
                                }
                                if subscription_filter_allows_notification(
                                    acknowledged_filter.as_ref().expect("filter was acknowledged"),
                                    &notification,
                                ) {
                                    self.handle_subscription_notification(&server, notification).await;
                                }
                            }
                            Err(broadcast::error::RecvError::Lagged(_)) => {
                                self.invalidate_server_lists(&server.config.id, server.generation);
                                let _ = self.refresh_tools(&server.config.id).await;
                                let _ = self.refresh_resources(&server.config.id).await;
                            }
                            Err(broadcast::error::RecvError::Closed) => {
                                return Err(McpError::Message("MCP notification stream closed".to_string()));
                            }
                        }
                    }
                }
            }
        }
        self.http_subscription_loop(&server, notifications).await
    }

    async fn http_subscription_loop(
        &self,
        server: &McpServerState,
        notifications: Value,
    ) -> Result<(), McpError> {
        let endpoint = server
            .endpoint_url
            .as_deref()
            .ok_or_else(|| McpError::NotConnected(server.config.id.clone()))?;
        let protocol = server
            .protocol
            .as_ref()
            .ok_or_else(|| McpError::Message("MCP protocol was not negotiated".to_string()))?;
        let request = json_rpc_request_for_protocol(
            protocol,
            "subscriptions/listen",
            Some(serde_json::json!({ "notifications": notifications })),
        )?;
        let subscription_id = request.get("id").cloned().ok_or_else(|| {
            McpError::Message("MCP subscription request is missing id".to_string())
        })?;
        let token = self.mcp_auth_token(&server.config);
        let mut headers = build_http_headers(
            &server.config,
            token.as_ref().map(|token| token.as_str()),
            None,
            protocol,
            true,
            "application/json, text/event-stream",
        )?;
        insert_modern_request_headers(&mut headers, &request, None)?;
        let client = oxideterm_network_proxy::application_http_client()
            .map_err(|error| McpError::Message(error.to_string()))?;
        let response = tokio::time::timeout(
            MCP_REQUEST_TIMEOUT,
            client.post(endpoint).headers(headers).json(&request).send(),
        )
        .await
        .map_err(|_| McpError::Timeout(server.config.name.clone()))?
        .map_err(|error| McpError::Message(error.without_url().to_string()))?;
        if !response.status().is_success() {
            return Err(McpError::HttpStatus(
                response.status(),
                response
                    .status()
                    .canonical_reason()
                    .unwrap_or("")
                    .to_string(),
            ));
        }
        let is_sse = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.contains("text/event-stream"));
        if !is_sse {
            return Err(McpError::Message(
                "MCP subscription did not return an SSE stream".to_string(),
            ));
        }
        let mut stream = response.bytes_stream();
        let mut parser = SseEventParser::default();
        let mut acknowledged_filter = None;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|error| McpError::Message(error.to_string()))?;
            parser.push_str(&String::from_utf8_lossy(&chunk));
            for event in parser.drain_events() {
                if let Ok(message) = serde_json::from_str::<Value>(&event.data) {
                    if message.get("method").is_some() {
                        if !notification_matches_subscription(&message, &subscription_id) {
                            continue;
                        }
                        if acknowledged_filter.is_none() {
                            if message.get("method").and_then(Value::as_str)
                                != Some("notifications/subscriptions/acknowledged")
                            {
                                return Err(McpError::Message(
                                    "MCP subscription sent a notification before acknowledgment"
                                        .to_string(),
                                ));
                            }
                            acknowledged_filter = Some(acknowledged_subscription_filter(&message)?);
                            continue;
                        }
                        if subscription_filter_allows_notification(
                            acknowledged_filter
                                .as_ref()
                                .expect("filter was acknowledged"),
                            &message,
                        ) {
                            self.handle_subscription_notification(server, message).await;
                        }
                    } else if message.get("id") == Some(&subscription_id) {
                        return extract_result(Some(message)).map(|_| ());
                    }
                }
            }
        }
        Err(McpError::Message(
            "MCP subscription stream ended unexpectedly".to_string(),
        ))
    }

    async fn handle_subscription_notification(&self, server: &McpServerState, notification: Value) {
        match notification.get("method").and_then(Value::as_str) {
            Some("notifications/tools/list_changed") => {
                self.invalidate_tools(&server.config.id, server.generation);
                let _ = self.refresh_tools(&server.config.id).await;
            }
            Some("notifications/resources/list_changed") => {
                self.invalidate_resources(&server.config.id, server.generation);
                let _ = self.refresh_resources(&server.config.id).await;
            }
            Some("notifications/resources/updated") => {
                if let Some(uri) = notification.pointer("/params/uri").and_then(Value::as_str) {
                    let mut state = self.state.write();
                    if let Some(current) = state.servers.get_mut(&server.config.id)
                        && current.generation == server.generation
                    {
                        current.resource_content_cache.remove(uri);
                    }
                }
            }
            _ => {}
        }
    }

    fn invalidate_server_lists(&self, server_id: &str, generation: u64) {
        self.invalidate_tools(server_id, generation);
        self.invalidate_resources(server_id, generation);
    }

    fn invalidate_tools(&self, server_id: &str, generation: u64) {
        let mut state = self.state.write();
        if let Some(server) = state.servers.get_mut(server_id)
            && server.generation == generation
        {
            server.tools_cache = None;
        }
    }

    fn invalidate_resources(&self, server_id: &str, generation: u64) {
        let mut state = self.state.write();
        if let Some(server) = state.servers.get_mut(server_id)
            && server.generation == generation
        {
            server.resources_cache = None;
        }
    }

    fn mcp_auth_token(&self, config: &McpServerConfig) -> Option<Zeroizing<String>> {
        if config.auth_header_mode == Some(McpAuthHeaderMode::None) {
            return None;
        }
        // Tauri stores MCP auth tokens in the same OS keychain namespace under
        // `mcp:{id}` and only falls back to legacy config.authToken for
        // migration. Keep both values out of Debug/log paths and return an
        // owned Zeroizing clone with request-scoped lifetime.
        self.key_store
            .get_provider_key(&format!("mcp:{}", config.id))
            .ok()
            .flatten()
            .or_else(|| {
                config
                    .auth_token
                    .as_ref()
                    .map(|token| Zeroizing::new(token.clone()))
            })
    }

    async fn discover_legacy_sse_endpoint(
        &self,
        base_url: &str,
        config: &McpServerConfig,
        auth_token: Option<&str>,
    ) -> Result<String, McpError> {
        let url = validate_mcp_http_url(base_url)?;
        let headers = build_http_headers(
            config,
            auth_token,
            None,
            &McpProtocol::legacy_sse(),
            false,
            "text/event-stream",
        )?;
        let client = oxideterm_network_proxy::application_http_client()
            .map_err(|error| McpError::Message(error.to_string()))?;
        let response = tokio::time::timeout(
            MCP_REQUEST_TIMEOUT,
            client.get(&url).headers(headers).send(),
        )
        .await
        .map_err(|_| McpError::Timeout(config.name.clone()))?
        .map_err(|error| McpError::Message(error.without_url().to_string()))?;
        if !response.status().is_success() {
            return Err(McpError::HttpStatus(
                response.status(),
                response
                    .status()
                    .canonical_reason()
                    .unwrap_or("")
                    .to_string(),
            ));
        }
        let endpoint = tokio::time::timeout(MCP_REQUEST_TIMEOUT, read_sse_until_endpoint(response))
            .await
            .map_err(|_| McpError::Timeout(url.clone()))??;
        let base =
            reqwest::Url::parse(&url).map_err(|error| McpError::Message(error.to_string()))?;
        base.join(&endpoint)
            .map(|url| url.to_string())
            .map_err(|error| McpError::Message(error.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    async fn http_json_rpc_request(
        &self,
        endpoint_url: &str,
        request: Value,
        config: &McpServerConfig,
        auth_token: Option<&str>,
        session_id: Option<&str>,
        protocol: &McpProtocol,
        expect_json: bool,
    ) -> Result<HttpRequestResult, McpError> {
        let url = validate_mcp_http_url(endpoint_url)?;
        let request_id = request.get("id").and_then(Value::as_u64);
        let mut headers = build_http_headers(
            config,
            auth_token,
            session_id,
            protocol,
            true,
            "application/json, text/event-stream",
        )?;
        if protocol.is_modern() {
            let tool_schema = request
                .get("params")
                .and_then(|params| params.get("name"))
                .and_then(Value::as_str)
                .and_then(|tool_name| {
                    self.state
                        .read()
                        .servers
                        .get(&config.id)
                        .and_then(|server| server.tools.iter().find(|tool| tool.name == tool_name))
                        .cloned()
                });
            insert_modern_request_headers(&mut headers, &request, tool_schema.as_ref())?;
        }
        let client = oxideterm_network_proxy::application_http_client()
            .map_err(|error| McpError::Message(error.to_string()))?;
        tokio::time::timeout(MCP_REQUEST_TIMEOUT, async {
            let response = client
                .post(&url)
                .headers(headers)
                .json(&request)
                .send()
                .await
                .map_err(|error| McpError::Message(error.without_url().to_string()))?;
            let status = response.status();
            let session_id = response
                .headers()
                .get("MCP-Session-Id")
                .or_else(|| response.headers().get("Mcp-Session-Id"))
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
                .or_else(|| session_id.map(str::to_string));
            let response = parse_http_response(response, request_id, expect_json).await?;
            if !status.is_success() {
                if let Some(error) = response.as_ref().and_then(|response| response.get("error")) {
                    return Err(rpc_error_from_value_with_status(error, Some(status)));
                }
                return Err(McpError::HttpStatus(
                    status,
                    status.canonical_reason().unwrap_or("").to_string(),
                ));
            }
            Ok(HttpRequestResult {
                endpoint_url: url,
                session_id,
                response,
            })
        })
        .await
        .map_err(|_| McpError::Timeout(endpoint_url.to_string()))?
    }
}
