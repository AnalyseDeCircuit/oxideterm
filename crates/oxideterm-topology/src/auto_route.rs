// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet},
};

use oxideterm_connections::{SavedAuth, SavedConnection, SavedProxyHop};

const AUTO_ROUTE_ROUTE_VERSION: &str = "2.0";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AutoRouteNodeConfig {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: AutoRouteTopologyAuthType,
    pub key_path: Option<String>,
    pub display_name: Option<String>,
    pub is_local: bool,
    pub tags: Vec<String>,
    pub saved_connection_id: Option<String>,
}

impl AutoRouteNodeConfig {
    pub fn display_title(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", self.username, self.host))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AutoRouteTopologyAuthType {
    Password,
    Key,
    ManagedKey,
    Certificate,
    KeyboardInteractive,
    Agent,
}

impl AutoRouteTopologyAuthType {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Password => "sessionManager.auto_route.auth.password",
            Self::Key => "sessionManager.auto_route.auth.key",
            Self::ManagedKey => "sessionManager.auto_route.auth.managed_key",
            Self::Certificate => "sessionManager.auto_route.auth.certificate",
            Self::KeyboardInteractive => "sessionManager.auto_route.auth.keyboard_interactive",
            Self::Agent => "sessionManager.auto_route.auth.agent",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::Key => "key",
            Self::ManagedKey => "managed_key",
            Self::Certificate => "certificate",
            Self::KeyboardInteractive => "keyboard_interactive",
            Self::Agent => "agent",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
struct AutoRouteTopologyEdge {
    from: String,
    to: String,
    cost: i32,
}

#[derive(Clone, Debug)]
pub struct AutoRouteResult {
    pub path: Vec<String>,
    pub total_cost: i32,
}

#[derive(Clone, Debug)]
pub struct AutoRouteNetworkTopology {
    pub version: &'static str,
    nodes: HashMap<String, AutoRouteNodeConfig>,
    edges: Vec<AutoRouteTopologyEdge>,
}

#[derive(Clone, Debug)]
pub struct AutoRouteNodeInfo {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub display_name: Option<String>,
    pub auth_type: AutoRouteTopologyAuthType,
    pub is_local: bool,
    pub neighbors: Vec<String>,
    pub tags: Vec<String>,
    pub saved_connection_id: Option<String>,
}

impl AutoRouteNodeInfo {
    pub fn display_title(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| format!("{}@{}", self.username, self.host))
    }
}

#[derive(Eq, PartialEq)]
struct DijkstraState {
    cost: i32,
    node: String,
}

impl Ord for DijkstraState {
    fn cmp(&self, other: &Self) -> Ordering {
        // BinaryHeap is a max-heap; Tauri reverses the comparison to run Dijkstra as a min-heap.
        other.cost.cmp(&self.cost)
    }
}

impl PartialOrd for DijkstraState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl AutoRouteNetworkTopology {
    pub fn build_from_connections(connections: &[SavedConnection]) -> Self {
        let mut nodes = HashMap::new();
        let mut edges_set = HashSet::new();

        for conn in connections {
            let node_id = conn.id.clone();
            nodes.insert(
                node_id.clone(),
                AutoRouteNodeConfig {
                    id: node_id.clone(),
                    host: conn.host.clone(),
                    port: conn.port,
                    username: conn.username.clone(),
                    auth_type: topology_auth_type(&conn.auth),
                    key_path: topology_key_path(&conn.auth),
                    display_name: Some(conn.name.clone()),
                    is_local: false,
                    tags: conn.tags.clone(),
                    saved_connection_id: Some(conn.id.clone()),
                },
            );

            if conn.proxy_chain.is_empty() {
                edges_set.insert(AutoRouteTopologyEdge {
                    from: "local".to_string(),
                    to: node_id,
                    cost: 1,
                });
            } else {
                let mut previous = "local".to_string();
                for hop in &conn.proxy_chain {
                    let hop_id = Self::find_or_create_hop_node(&mut nodes, hop, connections);
                    edges_set.insert(AutoRouteTopologyEdge {
                        from: previous,
                        to: hop_id.clone(),
                        cost: 1,
                    });
                    previous = hop_id;
                }
                edges_set.insert(AutoRouteTopologyEdge {
                    from: previous,
                    to: node_id,
                    cost: 1,
                });
            }
        }

        Self {
            version: AUTO_ROUTE_ROUTE_VERSION,
            nodes,
            edges: edges_set.into_iter().collect(),
        }
    }

    fn find_or_create_hop_node(
        nodes: &mut HashMap<String, AutoRouteNodeConfig>,
        hop: &SavedProxyHop,
        connections: &[SavedConnection],
    ) -> String {
        for conn in connections {
            if conn.host == hop.host && conn.port == hop.port && conn.username == hop.username {
                return conn.id.clone();
            }
        }

        let hop_key = format!("{}:{}@{}", hop.username, hop.host, hop.port);
        if nodes.contains_key(&hop_key) {
            return hop_key;
        }

        nodes.insert(
            hop_key.clone(),
            AutoRouteNodeConfig {
                id: hop_key.clone(),
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth_type: topology_auth_type(&hop.auth),
                key_path: topology_key_path(&hop.auth),
                display_name: Some(format!("{}@{}", hop.username, hop.host)),
                is_local: false,
                tags: vec!["auto-generated".to_string()],
                saved_connection_id: None,
            },
        );
        hop_key
    }

    pub fn compute_route(&self, target_id: &str) -> Result<AutoRouteResult, String> {
        if !self.nodes.contains_key(target_id) {
            return Err(format!("Target node '{}' not found in topology", target_id));
        }
        for edge in &self.edges {
            if edge.cost <= 0 {
                return Err(format!(
                    "Invalid edge cost from '{}' to '{}': {}",
                    edge.from, edge.to, edge.cost
                ));
            }
            if edge.from != "local" && !self.nodes.contains_key(&edge.from) {
                return Err(format!("Invalid edge source '{}'", edge.from));
            }
            if !self.nodes.contains_key(&edge.to) {
                return Err(format!("Invalid edge target '{}'", edge.to));
            }
        }

        let mut adj: HashMap<String, Vec<(String, i32)>> = HashMap::new();
        adj.insert("local".to_string(), Vec::new());
        for node_id in self.nodes.keys() {
            adj.insert(node_id.clone(), Vec::new());
        }
        for edge in &self.edges {
            adj.entry(edge.from.clone())
                .or_default()
                .push((edge.to.clone(), edge.cost));
        }

        let mut dist = HashMap::new();
        let mut prev = HashMap::new();
        let mut heap = BinaryHeap::new();
        dist.insert("local".to_string(), 0);
        heap.push(DijkstraState {
            cost: 0,
            node: "local".to_string(),
        });

        while let Some(DijkstraState { cost, node }) = heap.pop() {
            if node == target_id {
                break;
            }
            if cost > *dist.get(&node).unwrap_or(&i32::MAX) {
                continue;
            }
            if let Some(neighbors) = adj.get(&node) {
                for (next, edge_cost) in neighbors {
                    let next_cost = cost.saturating_add(*edge_cost);
                    if next_cost < *dist.get(next).unwrap_or(&i32::MAX) {
                        dist.insert(next.clone(), next_cost);
                        prev.insert(next.clone(), node.clone());
                        heap.push(DijkstraState {
                            cost: next_cost,
                            node: next.clone(),
                        });
                    }
                }
            }
        }

        if !prev.contains_key(target_id) {
            return Err(format!("No route found to '{}'", target_id));
        }

        let mut path = Vec::new();
        let mut current = target_id.to_string();
        while let Some(parent) = prev.get(&current) {
            if parent == "local" {
                break;
            }
            path.push(parent.clone());
            current = parent.clone();
        }
        path.reverse();
        Ok(AutoRouteResult {
            path,
            total_cost: *dist.get(target_id).unwrap_or(&0),
        })
    }

    pub fn get_all_nodes(&self) -> Vec<AutoRouteNodeInfo> {
        let mut neighbors_map: HashMap<String, Vec<String>> = HashMap::new();
        for edge in &self.edges {
            neighbors_map
                .entry(edge.from.clone())
                .or_default()
                .push(edge.to.clone());
        }

        self.nodes
            .values()
            .filter(|node| !node.is_local)
            .map(|node| AutoRouteNodeInfo {
                id: node.id.clone(),
                host: node.host.clone(),
                port: node.port,
                username: node.username.clone(),
                display_name: node.display_name.clone(),
                auth_type: node.auth_type,
                is_local: node.is_local,
                neighbors: neighbors_map.get(&node.id).cloned().unwrap_or_default(),
                tags: node.tags.clone(),
                saved_connection_id: node.saved_connection_id.clone(),
            })
            .collect()
    }

    pub fn get_node(&self, node_id: &str) -> Option<&AutoRouteNodeConfig> {
        self.nodes.get(node_id)
    }

    pub fn find_node_for_runtime_config(
        &self,
        host: &str,
        port: u16,
        username: &str,
    ) -> Option<&AutoRouteNodeConfig> {
        self.nodes
            .values()
            .find(|node| node.host == host && node.port == port && node.username == username)
    }
}

fn topology_auth_type(auth: &SavedAuth) -> AutoRouteTopologyAuthType {
    match auth {
        SavedAuth::Password { .. } => AutoRouteTopologyAuthType::Password,
        SavedAuth::Key { .. } => AutoRouteTopologyAuthType::Key,
        SavedAuth::ManagedKey { .. } => AutoRouteTopologyAuthType::ManagedKey,
        SavedAuth::Certificate { .. } => AutoRouteTopologyAuthType::Certificate,
        SavedAuth::KeyboardInteractive => AutoRouteTopologyAuthType::KeyboardInteractive,
        SavedAuth::Agent => AutoRouteTopologyAuthType::Agent,
    }
}

fn topology_key_path(auth: &SavedAuth) -> Option<String> {
    match auth {
        SavedAuth::Key { key_path, .. } | SavedAuth::Certificate { key_path, .. } => {
            Some(key_path.clone())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use oxideterm_connections::ConnectionOptions;

    #[test]
    fn topology_routes_proxy_chain_like_tauri() {
        let jump = saved_connection("jump", "jump.internal", Vec::new());
        let mut target = saved_connection("target", "db.internal", Vec::new());
        target.proxy_chain.push(SavedProxyHop {
            host: "jump.internal".to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Agent,
            agent_forwarding: false,
            legacy_ssh_compatibility: false,
        });

        let topology = AutoRouteNetworkTopology::build_from_connections(&[jump, target]);
        let route = topology.compute_route("target").expect("route");

        assert_eq!(route.path, vec!["jump".to_string()]);
        assert_eq!(route.total_cost, 2);
    }

    #[test]
    fn topology_keeps_tauri_temp_hop_id_format() {
        let mut target = saved_connection("target", "db.internal", Vec::new());
        target.proxy_chain.push(SavedProxyHop {
            host: "jump.internal".to_string(),
            port: 2222,
            username: "alice".to_string(),
            auth: SavedAuth::Agent,
            agent_forwarding: false,
            legacy_ssh_compatibility: false,
        });

        let topology = AutoRouteNetworkTopology::build_from_connections(&[target]);

        assert!(topology.get_node("alice:jump.internal@2222").is_some());
    }

    fn saved_connection(id: &str, host: &str, proxy_chain: Vec<SavedProxyHop>) -> SavedConnection {
        SavedConnection {
            id: id.to_string(),
            version: oxideterm_connections::CONFIG_VERSION,
            name: id.to_string(),
            group: None,
            host: host.to_string(),
            port: 22,
            username: "root".to_string(),
            auth: SavedAuth::Agent,
            proxy_chain,
            upstream_proxy: oxideterm_connections::SavedUpstreamProxyPolicy::UseGlobal,
            options: ConnectionOptions::default(),
            created_at: Utc::now(),
            last_used_at: None,
            updated_at: None,
            color: None,
            icon: None,
            tags: Vec::new(),
            post_connect_command: None,
            privilege_credentials: Vec::new(),
        }
    }
}
