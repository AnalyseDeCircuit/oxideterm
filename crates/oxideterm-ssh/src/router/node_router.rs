#[derive(Clone, Debug)]
pub struct NodeRouter {
    registry: SshConnectionRegistry,
    runtime: NodeRuntimeStore,
    emitter: NodeEventEmitter,
}

impl NodeRouter {
    pub fn new(registry: SshConnectionRegistry) -> Self {
        Self::with_runtime_store(registry, NodeRuntimeStore::default())
    }

    pub fn with_runtime_store(registry: SshConnectionRegistry, runtime: NodeRuntimeStore) -> Self {
        Self::with_runtime_store_and_emitter(registry, runtime, NodeEventEmitter::default())
    }

    pub fn with_runtime_store_and_emitter(
        registry: SshConnectionRegistry,
        runtime: NodeRuntimeStore,
        emitter: NodeEventEmitter,
    ) -> Self {
        registry.set_node_event_emitter(emitter.clone());
        Self {
            registry,
            runtime,
            emitter,
        }
    }

    pub fn runtime_store(&self) -> NodeRuntimeStore {
        self.runtime.clone()
    }

    pub fn emitter(&self) -> &NodeEventEmitter {
        &self.emitter
    }

    pub fn upsert_node(&self, node_id: NodeId, config: SshConfig) {
        self.runtime.upsert_node(node_id, config);
    }

    pub fn upsert_node_with_origin(&self, node_id: NodeId, config: SshConfig, origin: NodeOrigin) {
        self.runtime
            .upsert_node_with_origin(node_id, config, origin);
    }

    pub fn export_tree_snapshot(&self) -> NodeTreeSnapshot {
        self.runtime.export_snapshot()
    }

    pub fn apply_tree_snapshot(&self, snapshot: NodeTreeSnapshot) -> Result<(), RouteError> {
        self.runtime.apply_snapshot(snapshot)
    }

    pub fn flatten_tree(&self) -> Vec<FlatNode> {
        self.runtime.flatten()
    }

    pub fn tree_summary(&self) -> SessionTreeSummary {
        self.runtime.summary()
    }

    pub fn drill_down_node(
        &self,
        parent_id: NodeId,
        config: SshConfig,
    ) -> Result<NodeId, RouteError> {
        self.runtime.drill_down(parent_id, config)
    }

    pub fn expand_manual_preset(
        &self,
        saved_connection_id: &str,
        hops: Vec<SshConfig>,
        target: SshConfig,
    ) -> Result<NodeTreeExpansion, RouteError> {
        self.runtime
            .expand_manual_preset(saved_connection_id, hops, target)
    }

    pub fn expand_auto_route(
        &self,
        target_host: &str,
        route_id: &str,
        hops: Vec<SshConfig>,
        target: SshConfig,
    ) -> Result<NodeTreeExpansion, RouteError> {
        self.runtime
            .expand_auto_route(target_host, route_id, hops, target)
    }

    pub fn reconcile_runtime_tree(&self) {
        let connections = self
            .registry
            .list()
            .into_iter()
            .map(|info| (info.connection_id, info.state))
            .collect::<HashMap<_, _>>();
        self.runtime.reconcile_with_connections(&connections);
    }

    pub async fn resolve_connection(
        &self,
        node_id: &NodeId,
    ) -> Result<ResolvedConnection, RouteError> {
        self.resolve_connection_wait(node_id, Duration::from_secs(15))
            .await
    }

    pub fn resolve_connection_now(
        &self,
        node_id: &NodeId,
    ) -> Result<ResolvedConnection, RouteError> {
        let runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = runtime
            .connection_id
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;

        let handle = self
            .registry
            .get(&connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        self.require_resolvable_state(node_id, &handle.info())?;
        self.require_physical_transport(node_id, &connection_id, &handle)?;
        Ok(ResolvedConnection {
            connection_id,
            handle,
            terminal_session_id: runtime.terminal_session_id,
            sftp_session_id: runtime.sftp_session_id,
        })
    }

    pub fn acquire_connection(
        &self,
        node_id: &NodeId,
        consumer: ConnectionConsumer,
    ) -> Result<ResolvedConnection, RouteError> {
        let runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = runtime
            .connection_id
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        let handle = self
            .registry
            .get(&connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        self.require_resolvable_state(node_id, &handle.info())?;
        self.require_physical_transport(node_id, &connection_id, &handle)?;
        let handle = self
            .registry
            .acquire_consumer_for_connection(&connection_id, consumer)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        let _ =
            self.runtime
                .update_connection_state(node_id, &handle.info(), "connection acquired");

        self.require_resolvable_state(node_id, &handle.info())?;
        self.require_physical_transport(node_id, &connection_id, &handle)?;
        Ok(ResolvedConnection {
            connection_id,
            handle,
            terminal_session_id: runtime.terminal_session_id,
            sftp_session_id: runtime.sftp_session_id,
        })
    }

    pub async fn acquire_connection_wait(
        &self,
        node_id: &NodeId,
        consumer: ConnectionConsumer,
        max_wait: Duration,
    ) -> Result<ResolvedConnection, RouteError> {
        self.resolve_connection_wait_inner(node_id, max_wait, Some(consumer))
            .await
    }

    pub fn release_consumer(&self, connection_id: &str, consumer: &ConnectionConsumer) {
        self.registry.release(connection_id, consumer);
    }

    pub fn bind_connection(
        &self,
        node_id: &NodeId,
        connection_id: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let connection_id = connection_id.into();
        let handle = self
            .registry
            .get(&connection_id)
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        let connection = handle.info();
        let event = self
            .runtime
            .bind_connection(node_id, connection_id.clone(), &connection)?;
        // Tauri registers connectionId -> nodeId when the runtime tree binds a
        // connection. Native keeps the same translation point so lower-level
        // connection events can be consumed as node events without consulting
        // terminal panes.
        self.emitter
            .register(connection_id.clone(), node_id.clone());
        Ok(self
            .emitter
            .emit_state_from_connection(&connection_id, &connection.state, "connection bound")
            .unwrap_or(event))
    }

    pub fn disconnect_node_runtime(
        &self,
        node_id: &NodeId,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        // Tauri emits SftpReady(false) as part of connection teardown even
        // though link-down keeps the SFTP owner around for reconnect. Keep the
        // same split here: explicit disconnect clears SFTP before the node
        // state moves to Disconnected, while link-down callers do not come
        // through this path.
        if let Ok(event) = self.runtime.set_sftp_ready(node_id, false, None) {
            self.emitter.dispatch(&event);
        }
        let event = self.runtime.disconnect_node(node_id, reason)?;
        self.emitter.dispatch(&event);
        Ok(event)
    }

    pub fn bind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: impl Into<String>,
    ) -> Result<(), RouteError> {
        self.runtime
            .bind_terminal_session(node_id, session_id.into())
    }

    pub fn bind_terminal_endpoint(
        &self,
        node_id: &NodeId,
        endpoint: TerminalEndpoint,
    ) -> Result<NodeStateEvent, RouteError> {
        let event = self
            .runtime
            .bind_terminal_endpoint(node_id, endpoint.clone())?;
        self.emitter.dispatch(&event);
        Ok(event)
    }

    pub fn unbind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: &str,
    ) -> Result<(), RouteError> {
        self.runtime.unbind_terminal_session(node_id, session_id)
    }

    pub fn terminal_url(&self, node_id: &NodeId) -> Result<TerminalEndpoint, RouteError> {
        let runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        runtime.state.ws_endpoint.ok_or_else(|| {
            RouteError::NotConnected(format!("No active terminal session for node {}", node_id.0))
        })
    }

    pub fn node_id_for_connection(&self, connection_id: &str) -> Option<NodeId> {
        self.runtime.node_id_for_connection(connection_id)
    }

    pub fn connection_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        self.runtime.connection_id_for_node(node_id)
    }

    pub fn bind_sftp_session(
        &self,
        node_id: &NodeId,
        session_id: impl Into<String>,
        cwd: Option<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        self.runtime
            .bind_sftp_session(node_id, session_id.into(), cwd)
    }

    pub async fn acquire_sftp(
        &self,
        node_id: &NodeId,
    ) -> Result<Arc<Mutex<SftpSession>>, RouteError> {
        let resolved = self
            .resolve_connection_wait(node_id, Duration::from_secs(15))
            .await?;
        let AcquiredSftpMeta {
            session,
            was_new,
            cwd,
        } = resolved
            .handle
            .acquire_sftp_with_meta()
            .await
            .map_err(|error| sftp_route_error("SFTP init failed", error))?;

        if was_new {
            let _ = self
                .registry
                .mark_sftp_session(&resolved.connection_id, true, cwd.clone());
        }
        let event = self.runtime.set_sftp_ready(node_id, true, cwd)?;
        if was_new {
            self.emitter.dispatch(&event);
        }
        Ok(session)
    }

    pub async fn acquire_transfer_sftp(&self, node_id: &NodeId) -> Result<SftpSession, RouteError> {
        let resolved = self
            .resolve_connection_wait(node_id, Duration::from_secs(15))
            .await?;
        resolved
            .handle
            .acquire_transfer_sftp()
            .await
            .map_err(|error| sftp_route_error("Transfer SFTP init failed", error))
    }

    pub async fn invalidate_and_reacquire_sftp(
        &self,
        node_id: &NodeId,
    ) -> Result<Arc<Mutex<SftpSession>>, RouteError> {
        let resolved = self
            .resolve_connection_wait(node_id, Duration::from_secs(15))
            .await?;
        let had_sftp = resolved.handle.invalidate_sftp().await;
        if had_sftp {
            let _ = self
                .registry
                .mark_sftp_session(&resolved.connection_id, false, None);
            let event = self.runtime.set_sftp_ready(node_id, false, None)?;
            self.emitter.dispatch(&event);
        }

        let AcquiredSftpMeta { session, cwd, .. } = resolved
            .handle
            .acquire_sftp_with_meta()
            .await
            .map_err(|error| sftp_route_error("SFTP rebuild failed", error))?;
        let _ = self
            .registry
            .mark_sftp_session(&resolved.connection_id, true, cwd.clone());
        let event = self.runtime.set_sftp_ready(node_id, true, cwd)?;
        self.emitter.dispatch(&event);
        Ok(session)
    }

    pub fn node_state(&self, node_id: &NodeId) -> Result<NodeStateSnapshot, RouteError> {
        let mut runtime = self
            .runtime
            .snapshot(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if let Some(connection_id) = runtime.connection_id.clone() {
            if let Some(handle) = self.registry.get(&connection_id) {
                let info = handle.info();
                runtime.state.readiness = readiness_for_connection(&info);
                runtime.state.error = match &info.state {
                    ConnectionState::Error(error) => Some(error.clone()),
                    ConnectionState::LinkDown => Some("Link down".to_string()),
                    _ => None,
                };
                if let Some(sftp_state) = self.registry.sftp_session_state(&connection_id) {
                    runtime.state.sftp_ready = sftp_state.ready;
                    runtime.state.sftp_cwd = sftp_state.cwd;
                }
            } else {
                runtime.state.readiness = NodeReadiness::Disconnected;
                runtime.state.error = None;
                runtime.state.sftp_ready = false;
                runtime.state.sftp_cwd = None;
            }
        }
        Ok(NodeStateSnapshot {
            state: runtime.state,
            generation: self
                .emitter
                .sequencer()
                .current(node_id)
                .max(runtime.generation),
        })
    }

    pub fn sync_connection_state(
        &self,
        node_id: &NodeId,
        connection: &ConnectionInfo,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let reason = reason.into();
        let event = self
            .runtime
            .update_connection_state(node_id, connection, reason.clone())?;
        Ok(self
            .emitter
            .emit_state_from_connection(&connection.connection_id, &connection.state, reason)
            .unwrap_or(event))
    }

    pub fn sync_connection_state_by_connection_id(
        &self,
        connection: &ConnectionInfo,
        reason: impl Into<String>,
    ) -> Option<NodeStateEvent> {
        let node_id = self
            .emitter
            .node_id_for_connection(&connection.connection_id)
            .or_else(|| self.node_id_for_connection(&connection.connection_id))?;
        self.sync_connection_state(&node_id, connection, reason)
            .ok()
    }

    pub fn sync_node_readiness_event(
        &self,
        node_id: &NodeId,
        readiness: NodeReadiness,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        self.runtime
            .apply_node_readiness(node_id, readiness, reason)
    }

    async fn resolve_connection_wait(
        &self,
        node_id: &NodeId,
        max_wait: Duration,
    ) -> Result<ResolvedConnection, RouteError> {
        self.resolve_connection_wait_inner(node_id, max_wait, None)
            .await
    }

    async fn resolve_connection_wait_inner(
        &self,
        node_id: &NodeId,
        max_wait: Duration,
        consumer: Option<ConnectionConsumer>,
    ) -> Result<ResolvedConnection, RouteError> {
        let started_at = Instant::now();
        loop {
            let runtime = self
                .runtime
                .snapshot(node_id)
                .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
            let connection_id = runtime
                .connection_id
                .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;

            if let Some(handle) = self.registry.get(&connection_id) {
                match handle.state() {
                    ConnectionState::Active | ConnectionState::Idle => {
                        let transport_status = handle.transport_status().await;
                        match transport_status {
                            ConnectionTransportStatus::Open => {
                                let handle = if let Some(consumer) = consumer.clone() {
                                    self.registry
                                        .acquire_consumer_for_connection(&connection_id, consumer)
                                        .ok_or_else(|| {
                                            RouteError::NotConnected(node_id.0.clone())
                                        })?
                                } else {
                                    handle
                                };
                                let _ = self.runtime.update_connection_state(
                                    node_id,
                                    &handle.info(),
                                    "connection acquired",
                                );
                                return Ok(ResolvedConnection {
                                    connection_id,
                                    handle,
                                    terminal_session_id: runtime.terminal_session_id,
                                    sftp_session_id: runtime.sftp_session_id,
                                });
                            }
                            ConnectionTransportStatus::Closed
                            | ConnectionTransportStatus::Missing => {
                                let detail = match transport_status {
                                    ConnectionTransportStatus::Closed => "transport is closed",
                                    ConnectionTransportStatus::Missing => "transport is missing",
                                    ConnectionTransportStatus::Open => unreachable!(),
                                };
                                let _ = self
                                    .registry
                                    .mark_state(&connection_id, ConnectionState::LinkDown);
                                return Err(RouteError::NotConnected(format!(
                                    "Connection {connection_id} is stale: {detail}"
                                )));
                            }
                        }
                    }
                    ConnectionState::Error(error) => {
                        return Err(RouteError::ConnectionError(error));
                    }
                    ConnectionState::Disconnecting | ConnectionState::Disconnected => {
                        return Err(RouteError::NotConnected(connection_id));
                    }
                    ConnectionState::Connecting
                    | ConnectionState::Reconnecting
                    | ConnectionState::LinkDown => {}
                }
            }

            if started_at.elapsed() >= max_wait {
                return Err(RouteError::ConnectionTimeout(format!(
                    "Timed out waiting for node {} connection {connection_id} to become active ({max_wait:?})",
                    node_id.0
                )));
            }
            // Re-read the node runtime each lap. Tauri reconnect restores child
            // forwards after connectNodeWithAncestors has rebound the node to a
            // fresh connection id; native must not keep waiting on the old
            // link-down child id captured at the start of the restore phase.
            sleep(Duration::from_millis(200)).await;
        }
    }

    fn require_resolvable_state(
        &self,
        node_id: &NodeId,
        connection: &ConnectionInfo,
    ) -> Result<(), RouteError> {
        match &connection.state {
            ConnectionState::Active | ConnectionState::Idle => Ok(()),
            ConnectionState::Connecting | ConnectionState::Reconnecting => {
                Err(RouteError::ConnectionTimeout(format!(
                    "Connection {} for node {} is still {:?}",
                    connection.connection_id, node_id.0, connection.state
                )))
            }
            ConnectionState::Error(error) => Err(RouteError::ConnectionError(error.clone())),
            ConnectionState::LinkDown => Err(RouteError::NotConnected(format!(
                "Node {} connection {} is link_down",
                node_id.0, connection.connection_id
            ))),
            ConnectionState::Disconnecting | ConnectionState::Disconnected => {
                Err(RouteError::NotConnected(node_id.0.clone()))
            }
        }
    }

    fn require_physical_transport(
        &self,
        node_id: &NodeId,
        connection_id: &str,
        handle: &SshConnectionHandle,
    ) -> Result<(), RouteError> {
        if handle.has_physical() {
            return Ok(());
        }
        let _ = self
            .registry
            .mark_state(connection_id, ConnectionState::LinkDown);
        Err(RouteError::NotConnected(format!(
            "Connection {connection_id} for node {} has no active transport",
            node_id.0
        )))
    }
}
