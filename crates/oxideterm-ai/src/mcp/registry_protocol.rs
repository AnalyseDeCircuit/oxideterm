impl McpRegistry {
    async fn initialize_stdio(&self, runtime_id: &str) -> Result<McpServerCapabilities, McpError> {
        let init = self
            .processes
            .send_request(
                runtime_id,
                "initialize",
                serde_json::json!({
                    "protocolVersion": LEGACY_SSE_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": { "name": MCP_CLIENT_NAME, "version": MCP_CLIENT_VERSION },
                }),
            )
            .await?;
        let capabilities = init
            .get("capabilities")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        self.processes
            .send_request(
                runtime_id,
                "notifications/initialized",
                serde_json::json!({}),
            )
            .await?;
        serde_json::from_value(capabilities).map_err(|error| McpError::Message(error.to_string()))
    }

    async fn stdio_list_tools(&self, runtime_id: &str) -> Result<Vec<McpToolSchema>, McpError> {
        let result = self
            .processes
            .send_request(runtime_id, "tools/list", serde_json::json!({}))
            .await?;
        parse_tools(result)
    }

    async fn stdio_list_resources(&self, runtime_id: &str) -> Result<Vec<McpResource>, McpError> {
        let result = self
            .processes
            .send_request(runtime_id, "resources/list", serde_json::json!({}))
            .await?;
        parse_resources(result)
    }

    async fn list_tools(&self, server: &McpServerState) -> Result<Vec<McpToolSchema>, McpError> {
        if let Some(runtime_id) = &server.runtime_id {
            return self.stdio_list_tools(runtime_id).await;
        }
        let response = self.http_rpc(server, "tools/list", None, true).await?;
        parse_tools(extract_result(response.response)?)
    }

    async fn call_tool(
        &self,
        server: &McpServerState,
        tool_name: &str,
        args: Value,
    ) -> Result<McpCallToolResult, McpError> {
        let params = serde_json::json!({ "name": tool_name, "arguments": args });
        let result = if let Some(runtime_id) = &server.runtime_id {
            self.processes
                .send_request(runtime_id, "tools/call", params)
                .await?
        } else {
            extract_result(
                self.http_rpc(server, "tools/call", Some(params), true)
                    .await?
                    .response,
            )?
        };
        serde_json::from_value(result).map_err(|error| McpError::Message(error.to_string()))
    }

    async fn read_resource_inner(
        &self,
        server: &McpServerState,
        uri: &str,
    ) -> Result<McpResourceContent, McpError> {
        let params = serde_json::json!({ "uri": uri });
        let result = if let Some(runtime_id) = &server.runtime_id {
            self.processes
                .send_request(runtime_id, "resources/read", params)
                .await?
        } else {
            extract_result(
                self.http_rpc(server, "resources/read", Some(params), true)
                    .await?
                    .response,
            )?
        };
        let contents = result
            .get("contents")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(first) = contents.into_iter().next() else {
            return Err(McpError::Message(format!(
                "Empty resource response for {uri}"
            )));
        };
        serde_json::from_value(first).map_err(|error| McpError::Message(error.to_string()))
    }

    async fn http_rpc(
        &self,
        server: &McpServerState,
        method: &str,
        params: Option<Value>,
        expect_json: bool,
    ) -> Result<HttpRequestResult, McpError> {
        let transport = server
            .resolved_transport
            .unwrap_or_else(|| server.config.transport.effective());
        let endpoint = server
            .endpoint_url
            .as_deref()
            .or(server.config.url.as_deref())
            .ok_or_else(|| McpError::Message("MCP HTTP server requires url".to_string()))?;
        let token = self.mcp_auth_token(&server.config);
        self.http_json_rpc_request(
            endpoint,
            if method.starts_with("notifications/") {
                json_rpc_notification(method, params)
            } else {
                json_rpc_request(method, params)
            },
            &server.config,
            token.as_ref().map(|token| token.as_str()),
            server.session_id.as_deref(),
            transport.protocol_version(),
            expect_json,
        )
        .await
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
            LEGACY_SSE_PROTOCOL_VERSION,
            false,
            "text/event-stream",
        )?;
        let response = tokio::time::timeout(
            MCP_REQUEST_TIMEOUT,
            self.http.get(&url).headers(headers).send(),
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
        protocol_version: &str,
        expect_json: bool,
    ) -> Result<HttpRequestResult, McpError> {
        let url = validate_mcp_http_url(endpoint_url)?;
        let request_id = request.get("id").and_then(Value::as_u64);
        let headers = build_http_headers(
            config,
            auth_token,
            session_id,
            protocol_version,
            true,
            "application/json, text/event-stream",
        )?;
        tokio::time::timeout(MCP_REQUEST_TIMEOUT, async {
            let response = self
                .http
                .post(&url)
                .headers(headers)
                .json(&request)
                .send()
                .await
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
            let session_id = response
                .headers()
                .get("MCP-Session-Id")
                .or_else(|| response.headers().get("Mcp-Session-Id"))
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
                .or_else(|| session_id.map(str::to_string));
            let response = parse_http_response(response, request_id, expect_json).await?;
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
