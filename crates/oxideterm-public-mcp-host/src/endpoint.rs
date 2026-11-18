use super::*;
fn audit_public_mcp_client_change(
    client_ref: &ClientRef,
    action: &str,
    detail: Option<&str>,
    succeeded: bool,
) {
    if let Some(mut context) = oxideterm_audit::AuditContext::current_request()
        .or_else(oxideterm_audit::AuditContext::current)
    {
        if context.source == oxideterm_audit::AuditSource::Application {
            context.source = oxideterm_audit::AuditSource::User;
        }
        context.target = Some(oxideterm_audit::redact(client_ref.as_str()));
        let audit = context.operation(oxideterm_audit::AuditCategory::Security, action, detail);
        audit.finish(
            if succeeded {
                oxideterm_audit::AuditOutcome::Succeeded
            } else {
                oxideterm_audit::AuditOutcome::Failed
            },
            oxideterm_audit::AuditEvidence::Protocol,
            None,
            None,
        );
    }
}

impl PublicMcpHost {
    pub fn start(settings_path: &Path, runtime: &tokio::runtime::Handle) -> Self {
        let clients_path = settings_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(PUBLIC_MCP_CLIENTS_FILE);
        let endpoint_state_path = public_mcp_endpoint_state_path(settings_path);
        let (clients, registry_error) = match ClientRegistry::open(clients_path) {
            Ok(clients) => (Arc::new(clients), None),
            Err(error) => (Arc::new(ClientRegistry::default()), Some(error.to_string())),
        };
        let client_registry_ready = registry_error.is_none();
        let (broker, receiver) = DomainBroker::channel(PUBLIC_MCP_BROKER_CAPACITY);
        let state = Arc::new(PublicMcpState {
            clients,
            approvals: Arc::default(),
            audit: Arc::new(oxideterm_public_mcp::AuditStore::new(2_048)),
            artifacts: Arc::default(),
            broker,
        });
        let endpoint_state = read_endpoint_state(&endpoint_state_path);
        let preferred_port = endpoint_state
            .as_ref()
            .map(|state| state.preferred_port)
            .unwrap_or(0);
        let initial_port = if preferred_port == 0 {
            endpoint_state.as_ref().map(|state| state.port).unwrap_or(0)
        } else {
            preferred_port
        };
        let (server, endpoint_url, server_error) = if client_registry_ready {
            let started =
                start_http_server(runtime, state.clone(), initial_port).or_else(|first_error| {
                    // Only automatic mode may move away from the previous discovery port.
                    if preferred_port != 0 || initial_port == 0 {
                        Err(first_error)
                    } else {
                        start_http_server(runtime, state.clone(), 0)
                    }
                });
            match started {
                Ok(server) => {
                    let endpoint_url = Some(server.endpoint_url());
                    // A persistence failure must not hide a healthy live endpoint.
                    let _ =
                        persist_endpoint_state(&endpoint_state_path, server.port(), preferred_port);
                    (Some(server), endpoint_url, None)
                }
                Err(error) => (None, None, Some(error.to_string())),
            }
        } else {
            (None, None, None)
        };
        let mut host = Self {
            endpoint_url,
            startup_error: registry_error.or(server_error),
            server,
            port_draft: preferred_port.to_string(),
            client_registry_ready,
            state,
            settings_path: settings_path.to_path_buf(),
            receiver: Some(receiver),
            admission: Arc::new(Mutex::new(true)),
            revealed_credential: None,
            connection_refs: HashMap::new(),
            connection_ids: HashMap::new(),
            quick_commands: Arc::default(),
            addon_refs: HashMap::new(),
            addon_ids: HashMap::new(),
            sync_handles: Arc::default(),
            terminals: Arc::default(),
            pending_terminal_opens: HashMap::new(),
            recordings: Arc::default(),
            history: Arc::default(),
            handle_cleanup: None,
            desktops: Arc::default(),
            runtime_handles: Arc::default(),
        };
        host.start_handle_cleanup(runtime);
        host
    }

    pub fn endpoint_url(&self) -> Option<&str> {
        self.endpoint_url.as_deref()
    }

    pub fn startup_error(&self) -> Option<&str> {
        self.startup_error.as_deref()
    }

    pub fn port_draft(&self) -> &str {
        &self.port_draft
    }

    pub fn set_port_draft(&mut self, draft: String) {
        self.port_draft = draft;
    }

    pub fn apply_preferred_port(
        &mut self,
        runtime: &tokio::runtime::Handle,
        preferred_port: u16,
    ) -> std::io::Result<()> {
        if !self.client_registry_ready {
            return Err(std::io::Error::other(
                "The Public MCP client registry is unavailable",
            ));
        }
        let current_port = self.server.as_ref().map(PublicMcpHttpServer::port);
        let endpoint_state_path = public_mcp_endpoint_state_path(&self.settings_path);

        if preferred_port == 0 && current_port.is_some() {
            // Automatic mode can keep the healthy listener and choose again on a later startup.
            persist_endpoint_state(
                &endpoint_state_path,
                current_port.unwrap_or_default(),
                preferred_port,
            )?;
        } else if current_port == Some(preferred_port) {
            persist_endpoint_state(&endpoint_state_path, preferred_port, preferred_port)?;
        } else {
            // Bind the replacement before dropping the current listener so a failed choice
            // never takes a working endpoint offline.
            let replacement = start_http_server(runtime, self.state.clone(), preferred_port)?;
            let replacement_url = replacement.endpoint_url();
            persist_endpoint_state(&endpoint_state_path, replacement.port(), preferred_port)?;
            self.server = Some(replacement);
            self.endpoint_url = Some(replacement_url);
        }

        self.port_draft = preferred_port.to_string();
        self.startup_error = None;
        Ok(())
    }

    pub fn record_action_error(&mut self, error: String) {
        self.startup_error = Some(error);
    }

    pub fn clients(&self) -> Vec<ClientProjection> {
        self.state.clients.list()
    }

    pub fn approvals(&self) -> Vec<oxideterm_public_mcp::ApprovalProjection> {
        self.state.approvals.list()
    }

    pub fn revealed_credential(&self) -> Option<&str> {
        self.revealed_credential
            .as_ref()
            .map(ClientCredential::expose)
    }

    pub fn create_client(
        &mut self,
        label: String,
        approval_mode: ClientApprovalMode,
    ) -> Result<(), String> {
        let registered = self
            .state
            .clients
            .register(label, approval_mode, all_tool_groups())
            .map_err(|error| error.to_string());
        let registered = match registered {
            Ok(registered) => registered,
            Err(error) => {
                if let Some(mut context) = oxideterm_audit::AuditContext::current_request()
                    .or_else(oxideterm_audit::AuditContext::current)
                {
                    if context.source == oxideterm_audit::AuditSource::Application {
                        context.source = oxideterm_audit::AuditSource::User;
                    }
                    context.observe(
                        oxideterm_audit::AuditCategory::Security,
                        "mcp_client_create",
                        None,
                        oxideterm_audit::AuditOutcome::Failed,
                        oxideterm_audit::AuditEvidence::Protocol,
                        oxideterm_audit::AuditAuthorization::NotRequired,
                    );
                }
                return Err(error);
            }
        };
        audit_public_mcp_client_change(
            &registered.projection.client_ref,
            "mcp_client_create",
            None,
            true,
        );
        self.revealed_credential = Some(registered.credential);
        self.startup_error = None;
        Ok(())
    }

    pub fn dismiss_revealed_credential(&mut self) {
        self.revealed_credential.take();
    }

    pub fn set_client_enabled(&self, client_ref: &ClientRef, enabled: bool) -> Result<(), String> {
        let result = self
            .state
            .clients
            .set_enabled(client_ref, enabled)
            .map_err(|error| error.to_string());
        audit_public_mcp_client_change(
            client_ref,
            "mcp_client_authorization_change",
            Some(if enabled { "enabled" } else { "disabled" }),
            result.is_ok(),
        );
        result
    }

    pub fn set_client_approval_mode(
        &self,
        client_ref: &ClientRef,
        approval_mode: ClientApprovalMode,
    ) -> Result<(), String> {
        let result = self
            .state
            .clients
            .set_approval_mode(client_ref, approval_mode)
            .map_err(|error| error.to_string());
        audit_public_mcp_client_change(
            client_ref,
            "mcp_client_authorization_change",
            Some(match approval_mode {
                ClientApprovalMode::Standard => "standard_approval",
                ClientApprovalMode::Unattended => "unattended",
            }),
            result.is_ok(),
        );
        result
    }

    pub fn set_client_tool_group(
        &self,
        client_ref: &ClientRef,
        tool_group: ToolGroup,
        enabled: bool,
    ) -> Result<(), String> {
        let Some(client) = self.state.clients.get(client_ref) else {
            audit_public_mcp_client_change(
                client_ref,
                "mcp_client_authorization_change",
                Some("client_missing"),
                false,
            );
            return Err("The external MCP client no longer exists".to_owned());
        };
        let mut tool_groups = client.tool_groups;
        if enabled {
            tool_groups.insert(tool_group);
        } else if tool_group != ToolGroup::Basic {
            tool_groups.remove(&tool_group);
        }
        let result = self
            .state
            .clients
            .set_groups(client_ref, tool_groups)
            .map_err(|error| error.to_string());
        audit_public_mcp_client_change(
            client_ref,
            "mcp_client_authorization_change",
            Some(&format!("group={} enabled={enabled}", tool_group.as_str())),
            result.is_ok(),
        );
        result
    }

    pub fn set_client_groups(
        &self,
        client_ref: &ClientRef,
        tool_groups: BTreeSet<ToolGroup>,
    ) -> Result<(), String> {
        let group_count = tool_groups.len();
        let result = self
            .state
            .clients
            .set_groups(client_ref, tool_groups)
            .map_err(|error| error.to_string());
        audit_public_mcp_client_change(
            client_ref,
            "mcp_client_authorization_change",
            Some(&format!("groups={group_count}")),
            result.is_ok(),
        );
        result
    }

    pub fn remove_client(&self, client_ref: &ClientRef) -> Result<(), String> {
        let result = self
            .state
            .clients
            .remove(client_ref)
            .map_err(|error| error.to_string());
        audit_public_mcp_client_change(client_ref, "mcp_client_remove", None, result.is_ok());
        result
    }

    pub fn set_approval_status(
        &mut self,
        approval_ref: &ApprovalRef,
        status: ApprovalStatus,
    ) -> Result<(), String> {
        let result = self
            .state
            .approvals
            .set_status(approval_ref, status)
            .map_err(|error| error.to_string());
        if result.is_ok() {
            self.startup_error = None;
        }
        result
    }

    pub fn take_receiver(&mut self) -> Option<DomainRequestReceiver> {
        self.receiver.take()
    }
}

impl Drop for PublicMcpHost {
    fn drop(&mut self) {
        self.set_suspended(true);
        for client in self.clients() {
            self.state.broker.cancel_client(&client.client_ref);
        }
        let workspaces = self
            .runtime_handles
            .lock()
            .workspaces
            .drain()
            .map(|(_, record)| record)
            .collect::<Vec<_>>();
        for record in workspaces {
            record.revoke();
        }
        if let Some(task) = self.handle_cleanup.take() {
            task.abort();
        }
        self.revealed_credential.take();
        self.server.take();
    }
}

fn read_endpoint_state(path: &Path) -> Option<PublicMcpEndpointState> {
    let bytes = std::fs::read(path).ok()?;
    let state: PublicMcpEndpointState = serde_json::from_slice(&bytes).ok()?;
    (state.version == 1 && state.port != 0).then_some(state)
}

fn public_mcp_endpoint_state_path(settings_path: &Path) -> PathBuf {
    settings_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(PUBLIC_MCP_ENDPOINT_FILE)
}

fn persist_endpoint_state(path: &Path, port: u16, preferred_port: u16) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(&PublicMcpEndpointState {
        version: 1,
        port,
        preferred_port,
    })
    .map_err(std::io::Error::other)?;
    oxideterm_atomic_file::durable_write(path, &bytes)
}
