
#[derive(Clone, Debug, Default)]
pub struct NodeRuntimeStore {
    nodes: Arc<DashMap<NodeId, NodeRuntimeEntry>>,
    root_ids: Arc<parking_lot::RwLock<Vec<NodeId>>>,
    connection_nodes: Arc<DashMap<String, NodeId>>,
}

impl NodeRuntimeStore {
    pub fn upsert_node(&self, node_id: NodeId, config: SshConfig) {
        self.upsert_node_with_origin(node_id, config, NodeOrigin::Direct);
    }

    pub fn upsert_node_with_origin(&self, node_id: NodeId, config: SshConfig, origin: NodeOrigin) {
        let is_new = !self.nodes.contains_key(&node_id);
        self.nodes
            .entry(node_id.clone())
            .and_modify(|route| {
                route.config = config.clone();
                route.origin = origin.clone();
                route.generation += 1;
            })
            .or_insert_with(|| NodeRuntimeEntry {
                config,
                parent_id: None,
                children_ids: Vec::new(),
                depth: 0,
                origin,
                connection_id: None,
                terminal_session_id: None,
                sftp_session_id: None,
                state: NodeState::default(),
                created_at_ms: now_ms(),
                generation: 0,
            });
        if is_new {
            let mut root_ids = self.root_ids.write();
            if !root_ids.contains(&node_id) {
                root_ids.push(node_id);
            }
        }
    }

    pub fn snapshot(&self, node_id: &NodeId) -> Option<NodeRuntimeSnapshot> {
        let route = self.nodes.get(node_id)?;
        Some(NodeRuntimeSnapshot {
            config: route.config.clone(),
            parent_id: route.parent_id.clone(),
            children_ids: route.children_ids.clone(),
            depth: route.depth,
            origin: route.origin.clone(),
            connection_id: route.connection_id.clone(),
            terminal_session_id: route.terminal_session_id.clone(),
            sftp_session_id: route.sftp_session_id.clone(),
            state: route.state.clone(),
            created_at_ms: route.created_at_ms,
            generation: route.generation,
        })
    }

    pub fn upsert_child_node(
        &self,
        parent_id: NodeId,
        node_id: NodeId,
        config: SshConfig,
    ) -> Result<(), RouteError> {
        self.upsert_child_node_with_origin(parent_id, node_id, config, NodeOrigin::Direct)
    }

    pub fn upsert_child_node_with_origin(
        &self,
        parent_id: NodeId,
        node_id: NodeId,
        config: SshConfig,
        origin: NodeOrigin,
    ) -> Result<(), RouteError> {
        let parent_depth = {
            let mut parent = self
                .nodes
                .get_mut(&parent_id)
                .ok_or_else(|| RouteError::NodeNotFound(parent_id.0.clone()))?;
            if !parent.children_ids.contains(&node_id) {
                parent.children_ids.push(node_id.clone());
                parent.generation += 1;
            }
            parent.depth
        };

        self.nodes
            .entry(node_id.clone())
            .and_modify(|route| {
                route.config = config.clone();
                route.parent_id = Some(parent_id.clone());
                route.depth = parent_depth + 1;
                route.origin = origin.clone();
                route.generation += 1;
            })
            .or_insert_with(|| NodeRuntimeEntry {
                config,
                parent_id: Some(parent_id),
                children_ids: Vec::new(),
                depth: parent_depth + 1,
                origin,
                connection_id: None,
                terminal_session_id: None,
                sftp_session_id: None,
                state: NodeState::default(),
                created_at_ms: now_ms(),
                generation: 0,
            });
        self.root_ids.write().retain(|id| id != &node_id);
        Ok(())
    }

    pub fn drill_down(&self, parent_id: NodeId, config: SshConfig) -> Result<NodeId, RouteError> {
        let parent = self
            .nodes
            .get(&parent_id)
            .ok_or_else(|| RouteError::NodeNotFound(parent_id.0.clone()))?;
        if !matches!(parent.state.readiness, NodeReadiness::Ready) {
            return Err(RouteError::ParentNotConnected(parent_id.0.clone()));
        }
        let depth = parent.depth + 1;
        if depth > MAX_SESSION_TREE_DEPTH {
            return Err(RouteError::MaxDepthExceeded(MAX_SESSION_TREE_DEPTH));
        }
        drop(parent);

        let node_id = generated_tree_node_id("drill");
        self.upsert_child_node_with_origin(
            parent_id,
            node_id.clone(),
            config,
            NodeOrigin::DrillDown {
                timestamp: now_ms() as i64 / 1000,
            },
        )?;
        Ok(node_id)
    }

    pub fn expand_manual_preset(
        &self,
        saved_connection_id: &str,
        hops: Vec<SshConfig>,
        target: SshConfig,
    ) -> Result<NodeTreeExpansion, RouteError> {
        let saved_connection_id = saved_connection_id.to_string();
        self.expand_preset_chain_internal(hops, target, |hop_index| NodeOrigin::ManualPreset {
            saved_connection_id: saved_connection_id.clone(),
            hop_index,
        })
    }

    pub fn expand_auto_route(
        &self,
        target_host: &str,
        route_id: &str,
        hops: Vec<SshConfig>,
        target: SshConfig,
    ) -> Result<NodeTreeExpansion, RouteError> {
        let target_host = target_host.to_string();
        let route_id = route_id.to_string();
        self.expand_preset_chain_internal(hops, target, |hop_index| NodeOrigin::AutoRoute {
            target_host: target_host.clone(),
            route_id: route_id.clone(),
            hop_index,
        })
    }

    fn expand_preset_chain_internal(
        &self,
        hops: Vec<SshConfig>,
        target: SshConfig,
        origin_for_hop: impl Fn(u32) -> NodeOrigin,
    ) -> Result<NodeTreeExpansion, RouteError> {
        if hops.is_empty() {
            let target_node_id = generated_tree_node_id("direct");
            self.upsert_node_with_origin(target_node_id.clone(), target, NodeOrigin::Direct);
            return Ok(NodeTreeExpansion {
                target_node_id: target_node_id.clone(),
                path_node_ids: vec![target_node_id],
                chain_depth: 1,
            });
        }

        let chain_depth = hops.len() as u32 + 1;
        if chain_depth > MAX_SESSION_TREE_DEPTH {
            return Err(RouteError::MaxDepthExceeded(MAX_SESSION_TREE_DEPTH));
        }

        let root_id = generated_tree_node_id("hop");
        self.upsert_node_with_origin(root_id.clone(), hops[0].clone(), origin_for_hop(0));
        let mut path_node_ids = vec![root_id.clone()];
        let mut current_id = root_id;

        for (index, hop) in hops.into_iter().enumerate().skip(1) {
            let hop_index = index as u32;
            let node_id = generated_tree_node_id("hop");
            self.upsert_child_node_with_origin(
                current_id.clone(),
                node_id.clone(),
                hop,
                origin_for_hop(hop_index),
            )?;
            path_node_ids.push(node_id.clone());
            current_id = node_id;
        }

        let target_node_id = generated_tree_node_id("target");
        self.upsert_child_node_with_origin(
            current_id,
            target_node_id.clone(),
            target,
            origin_for_hop(chain_depth - 1),
        )?;
        path_node_ids.push(target_node_id.clone());

        // Tauri returns the path from root to target so the frontend can call
        // connect_tree_node linearly. Native keeps the same shape even though
        // GPUI can let `ensure_node_connection_started` walk ancestors itself.
        Ok(NodeTreeExpansion {
            target_node_id,
            path_node_ids,
            chain_depth,
        })
    }

    pub fn path_to_node(&self, node_id: &NodeId) -> Result<Vec<NodeId>, RouteError> {
        let mut path = Vec::new();
        let mut current_id = Some(node_id.clone());
        while let Some(id) = current_id {
            let node = self
                .nodes
                .get(&id)
                .ok_or_else(|| RouteError::NodeNotFound(id.0.clone()))?;
            path.push(id.clone());
            current_id = node.parent_id.clone();
        }
        path.reverse();
        Ok(path)
    }

    pub fn export_snapshot(&self) -> NodeTreeSnapshot {
        let mut nodes = self
            .nodes
            .iter()
            .map(|entry| {
                let route = entry.value();
                NodeTreeSnapshotNode {
                    id: entry.key().clone(),
                    parent_id: route.parent_id.clone(),
                    children_ids: route.children_ids.clone(),
                    depth: route.depth,
                    config: route.config.clone(),
                    origin: route.origin.clone(),
                    state: route.state.clone(),
                    connection_id: route.connection_id.clone(),
                    terminal_session_id: route.terminal_session_id.clone(),
                    sftp_session_id: route.sftp_session_id.clone(),
                    created_at_ms: route.created_at_ms,
                    generation: route.generation,
                }
            })
            .collect::<Vec<_>>();
        nodes.sort_by_key(|node| (node.depth, node.created_at_ms, node.id.0.clone()));

        NodeTreeSnapshot {
            version: 1,
            exported_at_ms: now_ms(),
            root_ids: self.root_ids.read().clone(),
            nodes,
        }
    }

    pub fn apply_snapshot(&self, snapshot: NodeTreeSnapshot) -> Result<(), RouteError> {
        let node_ids = snapshot
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        for node in &snapshot.nodes {
            if let Some(parent_id) = &node.parent_id
                && !node_ids.contains(parent_id)
            {
                return Err(RouteError::NodeNotFound(parent_id.0.clone()));
            }
        }

        self.nodes.clear();
        self.connection_nodes.clear();
        {
            let mut root_ids = self.root_ids.write();
            root_ids.clear();
            root_ids.extend(snapshot.root_ids);
        }

        for node in snapshot.nodes {
            if let Some(connection_id) = node.connection_id.as_ref() {
                self.connection_nodes
                    .insert(connection_id.clone(), node.id.clone());
            }
            self.nodes.insert(
                node.id,
                NodeRuntimeEntry {
                    config: node.config,
                    parent_id: node.parent_id,
                    children_ids: node.children_ids,
                    depth: node.depth,
                    origin: node.origin,
                    connection_id: node.connection_id,
                    terminal_session_id: node.terminal_session_id,
                    sftp_session_id: node.sftp_session_id,
                    state: node.state,
                    created_at_ms: node.created_at_ms,
                    generation: node.generation,
                },
            );
        }
        self.reconcile_topology();
        Ok(())
    }

    pub fn clear(&self) {
        self.nodes.clear();
        self.connection_nodes.clear();
        self.root_ids.write().clear();
    }

    pub fn flatten(&self) -> Vec<FlatNode> {
        fn collect(store: &NodeRuntimeStore, node_id: &NodeId, output: &mut Vec<FlatNode>) {
            let Some(route) = store.nodes.get(node_id) else {
                return;
            };
            let route = route.value().clone();
            output.push(store.flat_node(node_id, &route));
            for child_id in route.children_ids {
                collect(store, &child_id, output);
            }
        }

        let mut output = Vec::new();
        for root_id in self.root_ids.read().iter() {
            collect(self, root_id, &mut output);
        }
        output
    }

    pub fn summary(&self) -> SessionTreeSummary {
        let nodes = self
            .nodes
            .iter()
            .map(|entry| entry.value().clone())
            .collect::<Vec<_>>();
        SessionTreeSummary {
            total_nodes: nodes.len(),
            root_count: self.root_ids.read().len(),
            connected_count: nodes
                .iter()
                .filter(|node| matches!(node.state.readiness, NodeReadiness::Ready))
                .count(),
            max_depth: nodes.iter().map(|node| node.depth).max().unwrap_or(0),
        }
    }

    pub fn reconcile_with_connections(&self, connections: &HashMap<String, ConnectionState>) {
        for mut route in self.nodes.iter_mut() {
            let Some(connection_id) = route.connection_id.clone() else {
                route.state.readiness = NodeReadiness::Disconnected;
                route.state.error = None;
                continue;
            };
            match connections.get(&connection_id) {
                Some(state) => {
                    route.state.readiness = readiness_for_connection_state(state);
                    route.state.error = match state {
                        ConnectionState::Error(error) => Some(error.clone()),
                        ConnectionState::LinkDown => Some("Link down".to_string()),
                        _ => None,
                    };
                }
                None => {
                    route.connection_id = None;
                    route.terminal_session_id = None;
                    route.sftp_session_id = None;
                    route.state.ws_endpoint = None;
                    route.state.sftp_ready = false;
                    route.state.sftp_cwd = None;
                    route.state.readiness = NodeReadiness::Disconnected;
                    route.state.error = None;
                }
            }
            route.generation += 1;
        }
        self.rebuild_connection_index();
    }

    pub fn subtree_postorder(&self, node_id: &NodeId) -> Vec<NodeId> {
        fn collect(store: &NodeRuntimeStore, node_id: &NodeId, output: &mut Vec<NodeId>) {
            let children = store
                .nodes
                .get(node_id)
                .map(|node| node.children_ids.clone())
                .unwrap_or_default();
            for child_id in children {
                collect(store, &child_id, output);
            }
            output.push(node_id.clone());
        }

        let mut nodes = Vec::new();
        collect(self, node_id, &mut nodes);
        nodes
    }

    fn bind_connection(
        &self,
        node_id: &NodeId,
        connection_id: String,
        connection: &ConnectionInfo,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if let Some(previous_id) = route.connection_id.as_ref()
            && previous_id != &connection_id
        {
            self.connection_nodes.remove(previous_id);
        }
        self.connection_nodes
            .insert(connection_id.clone(), node_id.clone());
        route.connection_id = Some(connection_id);
        route.generation += 1;
        route.state.readiness = readiness_for_connection(connection);
        route.state.error = match &connection.state {
            ConnectionState::Error(error) => Some(error.clone()),
            ConnectionState::LinkDown => Some("Link down".to_string()),
            _ => None,
        };
        Ok(NodeStateEvent::ConnectionStateChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            state: route.state.readiness.clone(),
            reason: "connection bound".to_string(),
        })
    }

    fn bind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: String,
    ) -> Result<(), RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.terminal_session_id = Some(session_id);
        route.state.ws_endpoint = None;
        route.generation += 1;
        Ok(())
    }

    fn bind_terminal_endpoint(
        &self,
        node_id: &NodeId,
        endpoint: TerminalEndpoint,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.terminal_session_id = Some(endpoint.session_id.clone());
        route.state.ws_endpoint = Some(endpoint.clone());
        route.generation += 1;
        Ok(NodeStateEvent::TerminalEndpointChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            ws_port: endpoint.ws_port,
            ws_token: endpoint.ws_token,
        })
    }

    fn unbind_terminal_session(
        &self,
        node_id: &NodeId,
        session_id: &str,
    ) -> Result<(), RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if route.terminal_session_id.as_deref() == Some(session_id) {
            route.terminal_session_id = None;
            route.state.ws_endpoint = None;
            route.generation += 1;
        }
        Ok(())
    }

    fn bind_sftp_session(
        &self,
        node_id: &NodeId,
        session_id: String,
        cwd: Option<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.sftp_session_id = Some(session_id);
        route.generation += 1;
        route.state.sftp_ready = true;
        route.state.sftp_cwd = cwd;
        Ok(NodeStateEvent::SftpReady {
            node_id: node_id.0.clone(),
            generation: route.generation,
            ready: route.state.sftp_ready,
            cwd: route.state.sftp_cwd.clone(),
        })
    }

    fn set_sftp_ready(
        &self,
        node_id: &NodeId,
        ready: bool,
        cwd: Option<String>,
    ) -> Result<(), RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.state.sftp_ready = ready;
        route.state.sftp_cwd = cwd;
        route.generation += 1;
        Ok(())
    }

    fn update_connection_state(
        &self,
        node_id: &NodeId,
        connection: &ConnectionInfo,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.generation += 1;
        route.state.readiness = readiness_for_connection(connection);
        route.state.error = match &connection.state {
            ConnectionState::Error(error) => Some(error.clone()),
            _ => None,
        };
        Ok(NodeStateEvent::ConnectionStateChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            state: route.state.readiness.clone(),
            reason: reason.into(),
        })
    }

    pub fn node_id_for_connection(&self, connection_id: &str) -> Option<NodeId> {
        self.connection_nodes
            .get(connection_id)
            .map(|entry| entry.value().clone())
    }

    pub fn connection_id_for_node(&self, node_id: &NodeId) -> Option<String> {
        self.nodes
            .get(node_id)
            .and_then(|route| route.connection_id.clone())
    }

    fn flat_node(&self, node_id: &NodeId, route: &NodeRuntimeEntry) -> FlatNode {
        let is_last_child = if let Some(parent_id) = &route.parent_id {
            self.nodes
                .get(parent_id)
                .is_none_or(|parent| parent.children_ids.last() == Some(node_id))
        } else {
            self.root_ids.read().last() == Some(node_id)
        };
        FlatNode {
            id: node_id.0.clone(),
            parent_id: route.parent_id.as_ref().map(|id| id.0.clone()),
            depth: route.depth,
            host: route.config.host.clone(),
            port: route.config.port,
            username: route.config.username.clone(),
            display_name: None,
            state: route.state.readiness.clone(),
            error: route.state.error.clone(),
            has_children: !route.children_ids.is_empty(),
            is_last_child,
            origin_type: route.origin.origin_type().to_string(),
            terminal_session_id: route.terminal_session_id.clone(),
            sftp_session_id: route.sftp_session_id.clone(),
            ssh_connection_id: route.connection_id.clone(),
        }
    }

    fn reconcile_topology(&self) {
        let node_ids = self
            .nodes
            .iter()
            .map(|entry| entry.key().clone())
            .collect::<HashSet<_>>();
        for mut route in self.nodes.iter_mut() {
            route.children_ids.retain(|id| node_ids.contains(id));
            if route
                .parent_id
                .as_ref()
                .is_some_and(|parent_id| !node_ids.contains(parent_id))
            {
                route.parent_id = None;
                route.depth = 0;
            }
        }

        let mut computed_roots = self
            .nodes
            .iter()
            .filter_map(|entry| {
                entry
                    .value()
                    .parent_id
                    .is_none()
                    .then_some(entry.key().clone())
            })
            .collect::<Vec<_>>();
        computed_roots.sort_by_key(|id| {
            self.nodes
                .get(id)
                .map(|node| node.created_at_ms)
                .unwrap_or_default()
        });

        let mut roots = self.root_ids.write();
        roots.retain(|id| node_ids.contains(id) && computed_roots.contains(id));
        for root_id in computed_roots {
            if !roots.contains(&root_id) {
                roots.push(root_id);
            }
        }
        drop(roots);
        self.rebuild_connection_index();
    }

    fn rebuild_connection_index(&self) {
        self.connection_nodes.clear();
        for entry in self.nodes.iter() {
            if let Some(connection_id) = entry.value().connection_id.as_ref() {
                self.connection_nodes
                    .insert(connection_id.clone(), entry.key().clone());
            }
        }
    }
}

