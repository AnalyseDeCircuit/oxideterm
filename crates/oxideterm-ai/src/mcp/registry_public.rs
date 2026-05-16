impl McpRegistry {
    pub fn new(key_store: AiProviderKeyStore) -> Self {
        Self {
            state: Arc::new(RwLock::new(McpRuntimeState::default())),
            processes: Arc::new(McpProcessRegistry::default()),
            http: Client::new(),
            key_store,
        }
    }

    pub async fn connect_all_values(&self, configs: &[Value]) {
        self.synchronize_values(configs).await;
    }

    pub async fn synchronize_values(&self, configs: &[Value]) {
        let parsed = configs
            .iter()
            .filter_map(|value| serde_json::from_value::<McpServerConfig>(value.clone()).ok())
            .collect::<Vec<_>>();
        self.synchronize_configs(parsed).await;
    }

    pub async fn synchronize_configs(&self, configs: Vec<McpServerConfig>) {
        let desired_ids = configs
            .iter()
            .map(|config| config.id.clone())
            .collect::<std::collections::HashSet<_>>();
        {
            let mut state = self.state.write();
            state.server_order = configs.iter().map(|config| config.id.clone()).collect();
        }
        let existing_ids = self
            .state
            .read()
            .servers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in existing_ids {
            let should_remove = !desired_ids.contains(&id)
                || configs
                    .iter()
                    .find(|config| config.id == id)
                    .is_some_and(|config| !config.enabled);
            if should_remove {
                self.disconnect_and_remove(&id).await;
            }
        }

        let connect_tasks = configs
            .into_iter()
            .filter(|config| config.enabled)
            .map(|config| {
                let needs_reconnect = {
                    let state = self.state.read();
                    state.servers.get(&config.id).is_some_and(|current| {
                        current.config != config
                            && matches!(
                                current.status,
                                McpServerStatus::Connecting | McpServerStatus::Connected
                            )
                    })
                };
                let registry = self.clone();
                let task: BoxFuture<'static, ()> = async move {
                    if needs_reconnect {
                        registry.disconnect(&config.id).await;
                    }
                    registry.connect(config).await;
                }
                .boxed();
                task
            })
            .collect::<Vec<_>>();
        futures_util::future::join_all(connect_tasks).await;
    }

    pub async fn disconnect_all(&self) {
        let ids = self
            .state
            .read()
            .servers
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            self.disconnect(&id).await;
        }
    }

    pub fn tool_definitions(&self) -> Vec<AiToolDefinition> {
        let state = self.state.read();
        let mut definitions = vec![
            AiToolDefinition {
                name: "list_mcp_resources".to_string(),
                description: "List all resources available from connected MCP servers. Returns URI, name, description, and MIME type for each resource.".to_string(),
                parameters: serde_json::json!({ "type": "object", "properties": {} }),
            },
            AiToolDefinition {
                name: "read_mcp_resource".to_string(),
                description: "Read the content of a specific MCP resource by its URI. Returns text or base64-encoded binary data.".to_string(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "server_id": { "type": "string", "description": "The MCP server ID that owns this resource." },
                        "uri": { "type": "string", "description": "The resource URI to read (as returned by list_mcp_resources)." },
                    },
                    "required": ["server_id", "uri"],
                }),
            },
        ];
        definitions.extend(
            ordered_servers(&state)
                .into_iter()
                .filter(|server| server.status == McpServerStatus::Connected)
                .flat_map(|server| {
                    let namespace = server_namespace(server, &state.servers);
                    server.tools.iter().map(move |tool| AiToolDefinition {
                        name: format!("mcp::{namespace}::{}", tool.name),
                        description: format!(
                            "[MCP: {}] {}",
                            server.config.name,
                            tool.description.as_deref().unwrap_or(&tool.name)
                        ),
                        parameters: tool.input_schema.clone(),
                    })
                }),
        );
        definitions
    }

    pub fn resources(&self) -> Vec<(McpResource, String, String)> {
        let state = self.state.read();
        ordered_servers(&state)
            .into_iter()
            .filter(|server| server.status == McpServerStatus::Connected)
            .flat_map(|server| {
                server.resources.iter().cloned().map(|resource| {
                    (
                        resource,
                        server.config.id.clone(),
                        server.config.name.clone(),
                    )
                })
            })
            .collect()
    }

    pub fn snapshots(&self) -> Vec<McpServerStateSnapshot> {
        let state = self.state.read();
        ordered_servers(&state)
            .into_iter()
            .map(McpServerState::snapshot)
            .collect()
    }

    pub async fn connect_config(&self, config: McpServerConfig) {
        self.connect(config).await;
    }

    pub async fn disconnect_server(&self, server_id: &str) {
        self.disconnect(server_id).await;
    }

    pub fn has_auth_token(&self, server_id: &str) -> bool {
        self.key_store.has_provider_key(&format!("mcp:{server_id}"))
    }

    pub fn store_auth_token(
        &self,
        server_id: &str,
        token: Zeroizing<String>,
    ) -> anyhow::Result<()> {
        self.key_store
            .store_provider_key(&format!("mcp:{server_id}"), token)
    }

    pub fn delete_auth_token(&self, server_id: &str) -> anyhow::Result<()> {
        self.key_store
            .delete_provider_key(&format!("mcp:{server_id}"))
    }

    pub async fn call_prefixed_tool(
        &self,
        prefixed_name: &str,
        args: Value,
    ) -> Result<McpCallToolResult, McpError> {
        let (server_id, original_name) = self
            .state
            .read()
            .tool_index
            .get(prefixed_name)
            .cloned()
            .ok_or_else(|| {
            McpError::Message(format!("No MCP server found for tool: {prefixed_name}"))
        })?;
        let args = args.as_object().cloned().unwrap_or_default();
        let server = self.connected_server(&server_id)?;
        let generation = server.generation;
        let result = self
            .call_tool(&server, &original_name, Value::Object(args))
            .await;
        if result.is_err() {
            self.apply_runtime_error(
                &server_id,
                generation,
                result.as_ref().err().unwrap().to_string(),
            )
            .await;
        }
        result
    }

    pub async fn read_resource(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<McpResourceContent, McpError> {
        let server = self.connected_server(server_id)?;
        let generation = server.generation;
        let result = self.read_resource_inner(&server, uri).await;
        if result.is_err() {
            self.apply_runtime_error(
                server_id,
                generation,
                result.as_ref().err().unwrap().to_string(),
            )
            .await;
        }
        result
    }

    pub async fn refresh_tools(&self, server_id: &str) -> Result<(), McpError> {
        let server = self.connected_server(server_id)?;
        let generation = server.generation;
        if server
            .capabilities
            .as_ref()
            .and_then(|cap| cap.tools.as_ref())
            .is_none()
        {
            return Ok(());
        }
        let tools = match self.list_tools(&server).await {
            Ok(tools) => tools,
            Err(error) => {
                self.apply_runtime_error(server_id, generation, error.to_string())
                    .await;
                return Err(error);
            }
        };
        let mut state = self.state.write();
        if let Some(current) = state.servers.get_mut(server_id)
            && current.status == McpServerStatus::Connected
            && current.generation == generation
        {
            current.tools = tools;
        }
        rebuild_tool_index(&mut state);
        Ok(())
    }

}
