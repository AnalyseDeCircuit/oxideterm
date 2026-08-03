
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
        let is_new = match self.nodes.entry(node_id.clone()) {
            Entry::Occupied(mut entry) => {
                let route = entry.get_mut();
                // Move auth material into the existing runtime owner without a transient clone.
                route.config = config;
                route.origin = origin;
                route.generation += 1;
                false
            }
            Entry::Vacant(entry) => {
                entry.insert(NodeRuntimeEntry {
                    config,
                    parent_id: None,
                    children_ids: Vec::new(),
                    depth: 0,
                    origin,
                    connection_id: None,
                    terminal_session_id: None,
                    terminal_endpoints: Vec::new(),
                    sftp_session_id: None,
                    state: NodeState::default(),
                    created_at_ms: now_ms(),
                    generation: 0,
                });
                true
            }
        };
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

    fn connection_runtime(&self, node_id: &NodeId) -> Result<NodeConnectionRuntime, RouteError> {
        let route = self
            .nodes
            .get(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let connection_id = route
            .connection_id
            .clone()
            .ok_or_else(|| RouteError::NotConnected(node_id.0.clone()))?;
        Ok(NodeConnectionRuntime {
            connection_id,
            terminal_session_id: route.terminal_session_id.clone(),
            sftp_session_id: route.sftp_session_id.clone(),
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

        match self.nodes.entry(node_id.clone()) {
            Entry::Occupied(mut entry) => {
                let route = entry.get_mut();
                // Existing child nodes receive the new config by value so secrets are not copied.
                route.config = config;
                route.parent_id = Some(parent_id.clone());
                route.depth = parent_depth + 1;
                route.origin = origin;
                route.generation += 1;
            }
            Entry::Vacant(entry) => {
                entry.insert(NodeRuntimeEntry {
                    config,
                    parent_id: Some(parent_id),
                    children_ids: Vec::new(),
                    depth: parent_depth + 1,
                    origin,
                    connection_id: None,
                    terminal_session_id: None,
                    terminal_endpoints: Vec::new(),
                    sftp_session_id: None,
                    state: NodeState::default(),
                    created_at_ms: now_ms(),
                    generation: 0,
                });
            }
        }
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

    pub fn expand_manual_preset_under_parent(
        &self,
        parent_id: NodeId,
        saved_connection_id: &str,
        hops: Vec<SshConfig>,
        target: SshConfig,
    ) -> Result<NodeTreeExpansion, RouteError> {
        let saved_connection_id = saved_connection_id.to_string();
        self.expand_preset_chain_under_parent_internal(parent_id, hops, target, |hop_index| {
            NodeOrigin::ManualPreset {
                saved_connection_id: saved_connection_id.clone(),
                hop_index,
            }
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

    fn expand_preset_chain_under_parent_internal(
        &self,
        parent_id: NodeId,
        hops: Vec<SshConfig>,
        target: SshConfig,
        origin_for_hop: impl Fn(u32) -> NodeOrigin,
    ) -> Result<NodeTreeExpansion, RouteError> {
        let parent_depth = {
            let parent = self
                .nodes
                .get(&parent_id)
                .ok_or_else(|| RouteError::NodeNotFound(parent_id.0.clone()))?;
            if !matches!(parent.state.readiness, NodeReadiness::Ready) {
                return Err(RouteError::ParentNotConnected(parent_id.0.clone()));
            }
            parent.depth
        };
        let chain_depth = hops.len() as u32 + 1;
        if parent_depth + chain_depth > MAX_SESSION_TREE_DEPTH {
            return Err(RouteError::MaxDepthExceeded(MAX_SESSION_TREE_DEPTH));
        }

        let mut path_node_ids = Vec::with_capacity(chain_depth as usize);
        let mut current_parent_id = parent_id;
        for (index, hop) in hops.into_iter().enumerate() {
            let node_id = generated_tree_node_id("hop");
            self.upsert_child_node_with_origin(
                current_parent_id,
                node_id.clone(),
                hop,
                origin_for_hop(index as u32),
            )?;
            path_node_ids.push(node_id.clone());
            current_parent_id = node_id;
        }

        let target_node_id = generated_tree_node_id(if path_node_ids.is_empty() {
            "direct"
        } else {
            "target"
        });
        // Native extends Tauri's manual-preset expansion so a saved connection
        // can be consumed as a next hop beneath an already connected node.
        self.upsert_child_node_with_origin(
            current_parent_id,
            target_node_id.clone(),
            target,
            origin_for_hop(chain_depth - 1),
        )?;
        path_node_ids.push(target_node_id.clone());

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
                    terminal_endpoints: route.terminal_endpoints.clone(),
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

    pub fn export_persistence_snapshot(&self) -> NodeTreePersistenceSnapshot {
        let mut nodes = self
            .nodes
            .iter()
            .filter_map(|entry| {
                let route = entry.value();
                let config = if route.origin.saved_connection_id().is_some() {
                    None
                } else if route.config.has_runtime_auth_secret() {
                    return None;
                } else {
                    // Only restart-safe configurations cross the persistence boundary.
                    Some(route.config.clone())
                };
                Some(NodeTreePersistenceNode {
                    id: entry.key().clone(),
                    parent_id: route.parent_id.clone(),
                    children_ids: route.children_ids.clone(),
                    depth: route.depth,
                    origin: route.origin.clone(),
                    config,
                    created_at_ms: route.created_at_ms,
                    generation: route.generation,
                })
            })
            .collect::<Vec<_>>();
        // A child cannot be restored when a secret-bearing unsaved ancestor
        // was excluded, so prune such descendants before writing the tree.
        loop {
            let candidate_ids = nodes
                .iter()
                .map(|node| node.id.clone())
                .collect::<HashSet<_>>();
            let previous_len = nodes.len();
            nodes.retain(|node| {
                node.parent_id
                    .as_ref()
                    .is_none_or(|parent_id| candidate_ids.contains(parent_id))
            });
            if nodes.len() == previous_len {
                break;
            }
        }
        let retained_ids = nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        for node in &mut nodes {
            node.children_ids
                .retain(|child_id| retained_ids.contains(child_id));
        }
        nodes.sort_by_key(|node| (node.depth, node.created_at_ms, node.id.0.clone()));

        NodeTreePersistenceSnapshot {
            version: 1,
            exported_at_ms: now_ms(),
            root_ids: self
                .root_ids
                .read()
                .iter()
                .filter(|node_id| retained_ids.contains(*node_id))
                .cloned()
                .collect(),
            nodes,
        }
    }

    pub fn root_node_ids(&self) -> Vec<NodeId> {
        self.root_ids.read().clone()
    }

    pub fn contains_node(&self, node_id: &NodeId) -> bool {
        self.nodes.contains_key(node_id)
    }

    /// Reads only the non-secret X11 policy needed when opening a new shell.
    pub fn x11_forwarding(
        &self,
        node_id: &NodeId,
    ) -> Result<Option<X11ForwardPolicy>, RouteError> {
        self.nodes
            .get(node_id)
            .map(|route| route.config.x11_forwarding)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))
    }

    /// Updates materialized targets without replacing their secret-bearing configs.
    pub fn update_saved_connection_x11_forwarding(
        &self,
        saved_connection_id: &str,
        x11_forwarding: Option<X11ForwardPolicy>,
    ) -> usize {
        let materialized_nodes = self
            .nodes
            .iter()
            .filter_map(|entry| {
                let route = entry.value();
                match &route.origin {
                    NodeOrigin::Restored {
                        saved_connection_id: route_saved_connection_id,
                    } if route_saved_connection_id == saved_connection_id => {
                        Some((entry.key().clone(), None, route.children_ids.clone()))
                    }
                    NodeOrigin::ManualPreset {
                        saved_connection_id: route_saved_connection_id,
                        hop_index,
                    } if route_saved_connection_id == saved_connection_id => Some((
                        entry.key().clone(),
                        Some(*hop_index),
                        route.children_ids.clone(),
                    )),
                    NodeOrigin::Restored { .. }
                    | NodeOrigin::ManualPreset { .. }
                    | NodeOrigin::AutoRoute { .. }
                    | NodeOrigin::DrillDown { .. }
                    | NodeOrigin::Direct => None,
                }
            })
            .collect::<Vec<_>>();
        let manual_hop_indexes = materialized_nodes
            .iter()
            .filter_map(|(node_id, hop_index, _)| hop_index.map(|index| (node_id.clone(), index)))
            .collect::<HashMap<_, _>>();
        let target_node_ids = materialized_nodes
            .into_iter()
            .filter_map(|(node_id, hop_index, children_ids)| {
                let is_target = hop_index.is_none_or(|index| {
                    let next_hop_index = index.saturating_add(1);
                    // Every hop and target share the same origin id. Only a
                    // route without the next indexed child is a target.
                    !children_ids.iter().any(|child_id| {
                        manual_hop_indexes.get(child_id) == Some(&next_hop_index)
                    })
                });
                is_target.then_some(node_id)
            })
            .collect::<Vec<_>>();

        let mut updated = 0;
        for node_id in target_node_ids {
            let Some(mut route) = self.nodes.get_mut(&node_id) else {
                continue;
            };
            if route.config.x11_forwarding == x11_forwarding {
                continue;
            }
            route.config.x11_forwarding = x11_forwarding;
            route.generation = route.generation.saturating_add(1);
            updated += 1;
        }
        updated
    }

    /// Returns the non-secret projection used by UI and plugin readers.
    pub fn metadata_snapshot(&self, node_id: &NodeId) -> Option<NodeMetadataSnapshot> {
        let route = self.nodes.get(node_id)?;
        Some(node_metadata_snapshot(node_id.clone(), &route))
    }

    /// Changes persistence ownership without copying the node's SSH config.
    pub fn update_origin(
        &self,
        node_id: &NodeId,
        origin: NodeOrigin,
    ) -> Result<(), RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.origin = origin;
        route.generation = route.generation.saturating_add(1);
        Ok(())
    }

    /// Selects non-overlapping existing roots without cloning node configs.
    pub fn minimal_subtree_roots(
        &self,
        candidate_node_ids: impl IntoIterator<Item = NodeId>,
    ) -> Vec<NodeId> {
        let mut candidates = candidate_node_ids
            .into_iter()
            .filter(|node_id| self.nodes.contains_key(node_id))
            .collect::<Vec<_>>();
        candidates.sort_by_key(|node_id| {
            self.nodes
                .get(node_id)
                .map(|node| (node.depth, node_id.0.clone()))
                .unwrap_or((u32::MAX, node_id.0.clone()))
        });

        let mut roots = Vec::new();
        for node_id in candidates {
            if roots
                .iter()
                .any(|root_id| self.node_is_descendant_or_same(&node_id, root_id))
            {
                continue;
            }
            roots.push(node_id);
        }
        roots
    }

    fn node_is_descendant_or_same(&self, node_id: &NodeId, ancestor_id: &NodeId) -> bool {
        let mut current_id = Some(node_id.clone());
        while let Some(candidate_id) = current_id {
            if &candidate_id == ancestor_id {
                return true;
            }
            current_id = self
                .nodes
                .get(&candidate_id)
                .and_then(|node| node.parent_id.clone());
        }
        false
    }

    pub fn metadata_snapshots(&self) -> Vec<NodeMetadataSnapshot> {
        let mut nodes = self
            .nodes
            .iter()
            .map(|entry| node_metadata_snapshot(entry.key().clone(), entry.value()))
            .collect::<Vec<_>>();
        nodes.sort_by_key(|node| (node.depth, node.created_at_ms, node.id.0.clone()));
        nodes
    }

    pub fn apply_snapshot(&self, snapshot: NodeTreeSnapshot) -> Result<(), RouteError> {
        let node_ids = snapshot
            .nodes
            .iter()
            .map(|node| node.id.clone())
            .collect::<HashSet<_>>();
        if node_ids.len() != snapshot.nodes.len() {
            return Err(RouteError::ConnectionError(
                "invalid node tree snapshot: duplicate node identifiers".to_string(),
            ));
        }
        for node in &snapshot.nodes {
            if let Some(parent_id) = &node.parent_id
                && !node_ids.contains(parent_id)
            {
                return Err(RouteError::NodeNotFound(parent_id.0.clone()));
            }
        }

        let parent_by_id = snapshot
            .nodes
            .iter()
            .map(|node| (node.id.clone(), node.parent_id.clone()))
            .collect::<HashMap<_, _>>();
        validate_snapshot_parent_graph(&parent_by_id)?;
        if parent_by_id
            .keys()
            .any(|node_id| snapshot_node_depth(node_id, &parent_by_id) > MAX_SESSION_TREE_DEPTH)
        {
            return Err(RouteError::MaxDepthExceeded(MAX_SESSION_TREE_DEPTH));
        }
        let mut children_by_parent: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for node in &snapshot.nodes {
            if let Some(parent_id) = &node.parent_id {
                children_by_parent
                    .entry(parent_id.clone())
                    .or_default()
                    .push(node.id.clone());
            }
        }
        for node in &snapshot.nodes {
            if let Some(derived_children) = children_by_parent.get_mut(&node.id) {
                *derived_children = ordered_snapshot_ids(&node.children_ids, derived_children);
            }
        }
        let ordered_root_ids =
            ordered_snapshot_ids(&snapshot.root_ids, &root_ids_from_nodes(&snapshot.nodes));
        self.nodes.clear();
        self.connection_nodes.clear();
        {
            let mut root_ids = self.root_ids.write();
            root_ids.clear();
            root_ids.extend(ordered_root_ids);
        }

        for node in snapshot.nodes {
            let node_id = node.id.clone();
            if let Some(connection_id) = node.connection_id.as_ref() {
                self.connection_nodes
                    .insert(connection_id.clone(), node_id.clone());
            }
            let mut state = node.state;
            let terminal_endpoints =
                restored_terminal_endpoints(node.terminal_endpoints, state.ws_endpoint.take());
            self.nodes.insert(
                node_id.clone(),
                NodeRuntimeEntry {
                    config: node.config,
                    parent_id: node.parent_id.clone(),
                    children_ids: children_by_parent.remove(&node_id).unwrap_or_default(),
                    depth: snapshot_node_depth(&node_id, &parent_by_id),
                    origin: node.origin,
                    connection_id: node.connection_id,
                    terminal_session_id: node.terminal_session_id,
                    terminal_endpoints,
                    sftp_session_id: node.sftp_session_id,
                    state,
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
                    route.terminal_endpoints.clear();
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

    pub fn apply_node_readiness(
        &self,
        node_id: &NodeId,
        readiness: NodeReadiness,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.generation += 1;
        route.state.readiness = readiness.clone();
        route.state.error = match readiness {
            NodeReadiness::Error => {
                let reason = reason.into();
                (!reason.is_empty()).then_some(reason)
            }
            NodeReadiness::Disconnected | NodeReadiness::Ready | NodeReadiness::Connecting => None,
        };
        Ok(NodeStateEvent::ConnectionStateChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            state: route.state.readiness.clone(),
            reason: route.state.error.clone().unwrap_or_default(),
        })
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

    pub fn remove_subtree(&self, node_id: &NodeId) -> Vec<NodeId> {
        let nodes = self.subtree_postorder(node_id);
        if nodes.is_empty() {
            return nodes;
        }

        if let Some(parent_id) = self.nodes.get(node_id).and_then(|node| node.parent_id.clone()) {
            if let Some(mut parent) = self.nodes.get_mut(&parent_id) {
                parent.children_ids.retain(|child_id| child_id != node_id);
                parent.generation += 1;
            }
        } else {
            self.root_ids
                .write()
                .retain(|root_id| root_id != node_id);
        }

        for removed_id in &nodes {
            if let Some((_id, removed)) = self.nodes.remove(removed_id)
                && let Some(connection_id) = removed.connection_id
            {
                self.connection_nodes.remove(&connection_id);
            }
        }
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

    fn disconnect_node(
        &self,
        node_id: &NodeId,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let reason = reason.into();
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if let Some(connection_id) = route.connection_id.take() {
            self.connection_nodes.remove(&connection_id);
        }
        route.terminal_session_id = None;
        route.terminal_endpoints.clear();
        route.sftp_session_id = None;
        route.state.readiness = NodeReadiness::Disconnected;
        route.state.error = None;
        route.state.sftp_ready = false;
        route.state.sftp_cwd = None;
        route.state.ws_endpoint = None;
        route.generation += 1;
        Ok(NodeStateEvent::ConnectionStateChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            state: NodeReadiness::Disconnected,
            reason,
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
        if route.terminal_session_id.is_none() {
            route.terminal_session_id = Some(session_id);
            route.state.ws_endpoint = None;
        }
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
        let endpoint_session_id = endpoint.session_id.clone();
        if let Some(existing) = route
            .terminal_endpoints
            .iter_mut()
            .find(|existing| existing.session_id == endpoint.session_id)
        {
            *existing = endpoint;
        } else {
            route.terminal_endpoints.push(endpoint);
        }
        if route.terminal_session_id.is_none()
            || route.terminal_session_id.as_deref() == Some(endpoint_session_id.as_str())
        {
            route.terminal_session_id = Some(endpoint_session_id);
            // The legacy state slot is accepted on import only. Runtime keeps
            // one endpoint owner in terminal_endpoints.
            route.state.ws_endpoint = None;
        }
        route.generation += 1;
        Ok(NodeStateEvent::TerminalEndpointChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            // Event fan-out carries only availability; the token stays in the
            // runtime endpoint owner and is resolved explicitly by consumers.
            available: true,
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
        let before = route.terminal_endpoints.len();
        route
            .terminal_endpoints
            .retain(|endpoint| endpoint.session_id != session_id);
        if route.terminal_session_id.as_deref() == Some(session_id) {
            route.terminal_session_id = route
                .terminal_endpoints
                .first()
                .map(|endpoint| endpoint.session_id.clone());
            route.state.ws_endpoint = None;
            route.generation += 1;
        } else if route.terminal_endpoints.len() != before {
            route.generation += 1;
        }
        Ok(())
    }

    pub(super) fn primary_terminal_endpoint(
        &self,
        node_id: &NodeId,
    ) -> Result<Option<TerminalEndpoint>, RouteError> {
        let route = self
            .nodes
            .get(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        let Some(primary_session_id) = route.terminal_session_id.as_deref() else {
            return Ok(None);
        };
        Ok(route
            .terminal_endpoints
            .iter()
            .find(|endpoint| endpoint.session_id == primary_session_id)
            .cloned())
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
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        if !ready {
            route.sftp_session_id = None;
        }
        route.state.sftp_ready = ready;
        route.state.sftp_cwd = if ready { cwd } else { None };
        route.generation += 1;
        Ok(NodeStateEvent::SftpReady {
            node_id: node_id.0.clone(),
            generation: route.generation,
            ready: route.state.sftp_ready,
            cwd: route.state.sftp_cwd.clone(),
        })
    }

    pub(super) fn shared_sftp_session_changed(
        &self,
        node_id: &NodeId,
        connection_id: String,
        session_generation: Option<u64>,
        ready: bool,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.generation += 1;
        Ok(NodeStateEvent::SharedSftpSessionChanged {
            node_id: node_id.0.clone(),
            generation: route.generation,
            connection_id,
            session_generation,
            ready,
        })
    }

    fn update_connection_state(
        &self,
        node_id: &NodeId,
        connection: &ConnectionInfo,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        self.update_connection_state_from_parts(node_id, &connection.state, reason)
    }

    fn update_connection_state_from_parts(
        &self,
        node_id: &NodeId,
        state: &ConnectionState,
        reason: impl Into<String>,
    ) -> Result<NodeStateEvent, RouteError> {
        let mut route = self
            .nodes
            .get_mut(node_id)
            .ok_or_else(|| RouteError::NodeNotFound(node_id.0.clone()))?;
        route.generation += 1;
        route.state.readiness = readiness_for_connection_state(state);
        route.state.error = match state {
            ConnectionState::Error(error) => Some(error.clone()),
            ConnectionState::LinkDown => Some("Link down".to_string()),
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

fn node_metadata_snapshot(node_id: NodeId, route: &NodeRuntimeEntry) -> NodeMetadataSnapshot {
    NodeMetadataSnapshot {
        id: node_id,
        parent_id: route.parent_id.clone(),
        children_ids: route.children_ids.clone(),
        depth: route.depth,
        origin: route.origin.clone(),
        host: route.config.host.clone(),
        port: route.config.port,
        username: route.config.username.clone(),
        readiness: route.state.readiness.clone(),
        error: route.state.error.clone(),
        connection_id: route.connection_id.clone(),
        terminal_session_id: route.terminal_session_id.clone(),
        sftp_session_id: route.sftp_session_id.clone(),
        created_at_ms: route.created_at_ms,
    }
}

fn restored_terminal_endpoints(
    endpoints: Vec<TerminalEndpoint>,
    legacy_primary: Option<TerminalEndpoint>,
) -> Vec<TerminalEndpoint> {
    if !endpoints.is_empty() {
        return endpoints;
    }
    legacy_primary.into_iter().collect()
}

fn root_ids_from_nodes(nodes: &[NodeTreeSnapshotNode]) -> Vec<NodeId> {
    nodes
        .iter()
        .filter(|node| node.parent_id.is_none())
        .map(|node| node.id.clone())
        .collect()
}

fn ordered_snapshot_ids(preferred: &[NodeId], derived: &[NodeId]) -> Vec<NodeId> {
    let derived_ids = derived.iter().cloned().collect::<HashSet<_>>();
    let mut seen = HashSet::new();
    let mut ordered = preferred
        .iter()
        .filter(|node_id| derived_ids.contains(*node_id) && seen.insert((*node_id).clone()))
        .cloned()
        .collect::<Vec<_>>();
    ordered.extend(
        derived
            .iter()
            .filter(|node_id| seen.insert((*node_id).clone()))
            .cloned(),
    );
    ordered
}

fn validate_snapshot_parent_graph(
    parent_by_id: &HashMap<NodeId, Option<NodeId>>,
) -> Result<(), RouteError> {
    for start in parent_by_id.keys() {
        let mut path = HashSet::new();
        let mut current = Some(start.clone());
        while let Some(node_id) = current {
            if !path.insert(node_id.clone()) {
                return Err(RouteError::ConnectionError(format!(
                    "invalid node tree snapshot: parent cycle contains {}",
                    node_id.0
                )));
            }
            current = parent_by_id.get(&node_id).cloned().flatten();
        }
    }
    Ok(())
}

fn snapshot_node_depth(
    node_id: &NodeId,
    parent_by_id: &HashMap<NodeId, Option<NodeId>>,
) -> u32 {
    let mut depth = 0_u32;
    let mut current = parent_by_id.get(node_id).cloned().flatten();
    while let Some(parent_id) = current {
        depth = depth.saturating_add(1);
        current = parent_by_id.get(&parent_id).cloned().flatten();
    }
    depth
}
