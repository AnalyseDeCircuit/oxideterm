//! Network Topology Graph for Auto-Route Calculation
//!
//! Auto-generates topology from saved connections:
//! - Nodes: Each saved connection becomes a node
//! - Edges: Inferred from proxy_chain configuration
//!   - If connection has no proxy_chain → local can reach it directly
//!   - If proxy_chain exists → infer reachability from chain
//!
//! Users can also add custom edges via config file overlay.

use crate::config::types::SavedConnection;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;

// ============================================================================
// Data Structures
// ============================================================================

/// Network topology graph (auto-generated from saved connections)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NetworkTopology {
    /// Config version
    pub version: String,
    /// Node configs (node_id -> config)
    pub nodes: HashMap<String, TopologyNodeConfig>,
    /// Edges (reachability relations)
    pub edges: Vec<TopologyEdge>,
}

/// Node configuration in topology graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyNodeConfig {
    /// Unique node ID (same as saved connection ID)
    pub id: String,
    /// Host address
    pub host: String,
    /// SSH port
    #[serde(default = "default_port")]
    pub port: u16,
    /// Username
    pub username: String,
    /// Auth type: "password" | "key" | "agent"
    #[serde(default = "default_auth_type")]
    pub auth_type: String,
    /// Key path (when auth_type = "key")
    pub key_path: Option<String>,
    /// Display name
    pub display_name: Option<String>,
    /// Is this the local node (start point)
    #[serde(default)]
    pub is_local: bool,
    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,
    /// Reference to saved connection ID (for auto-generated nodes)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_connection_id: Option<String>,
}

fn default_port() -> u16 {
    22
}

fn default_auth_type() -> String {
    "agent".to_string()
}

/// Topology edge (reachability)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct TopologyEdge {
    /// Source node ID ("local" means local machine)
    pub from: String,
    /// Target node ID
    pub to: String,
    /// Cost (can be latency, hop count, etc.)
    #[serde(default = "default_cost")]
    pub cost: i32,
}

fn default_cost() -> i32 {
    1
}

/// Route calculation result
#[derive(Debug, Clone, Serialize)]
pub struct RouteResult {
    /// Intermediate nodes (excluding "local" and target)
    pub path: Vec<String>,
    /// Total cost
    pub total_cost: i32,
}

/// Node info for frontend display
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TopologyNodeInfo {
    pub id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub display_name: Option<String>,
    pub auth_type: String,
    pub is_local: bool,
    pub neighbors: Vec<String>,
    pub tags: Vec<String>,
    /// Reference to saved connection (for direct connect)
    pub saved_connection_id: Option<String>,
}

/// Custom edges overlay config (user-editable)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopologyEdgesConfig {
    /// User-defined custom edges
    #[serde(default)]
    pub custom_edges: Vec<TopologyEdge>,
    /// Edges to exclude from auto-generation
    #[serde(default)]
    pub excluded_edges: Vec<TopologyEdge>,
}

// ============================================================================
// Dijkstra Algorithm
// ============================================================================

/// Dijkstra priority queue state
#[derive(Eq, PartialEq)]
struct DijkstraState {
    cost: i32,
    node: String,
}

impl Ord for DijkstraState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Min-heap: reverse comparison
        other.cost.cmp(&self.cost)
    }
}

impl PartialOrd for DijkstraState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ============================================================================
// Implementation
// ============================================================================

impl NetworkTopology {
    /// Build topology from saved connections (auto-generation)
    ///
    /// # Logic:
    /// 1. Each saved connection becomes a node
    /// 2. Edges are inferred from proxy_chain:
    ///    - No proxy_chain → local can reach directly
    ///    - Has proxy_chain → each hop in chain implies reachability
    /// 3. Custom edges from config overlay are merged
    pub fn build_from_connections(connections: &[SavedConnection]) -> Self {
        let mut nodes = HashMap::new();
        let mut edges_set: HashSet<TopologyEdge> = HashSet::new();

        // Build nodes and infer edges from each connection
        for conn in connections {
            let node_id = conn.id.clone();

            // Create node from connection
            let auth_type = match &conn.auth {
                crate::config::types::SavedAuth::Password { .. } => "password",
                crate::config::types::SavedAuth::Key { .. } => "key",
                crate::config::types::SavedAuth::Certificate { .. } => "certificate",
                crate::config::types::SavedAuth::Agent => "agent",
            };

            let key_path = match &conn.auth {
                crate::config::types::SavedAuth::Key { key_path, .. } => Some(key_path.clone()),
                crate::config::types::SavedAuth::Certificate { key_path, .. } => {
                    Some(key_path.clone())
                }
                _ => None,
            };

            nodes.insert(
                node_id.clone(),
                TopologyNodeConfig {
                    id: node_id.clone(),
                    host: conn.host.clone(),
                    port: conn.port,
                    username: conn.username.clone(),
                    auth_type: auth_type.to_string(),
                    key_path,
                    display_name: Some(conn.name.clone()),
                    is_local: false,
                    tags: conn.tags.clone(),
                    saved_connection_id: Some(conn.id.clone()),
                },
            );

            // Infer edges from proxy_chain
            if conn.proxy_chain.is_empty() {
                // No proxy chain → local can reach this node directly
                edges_set.insert(TopologyEdge {
                    from: "local".to_string(),
                    to: node_id.clone(),
                    cost: 1,
                });
            } else {
                // Has proxy chain:
                // local → first_hop → second_hop → ... → target
                let mut prev = "local".to_string();

                for hop in &conn.proxy_chain {
                    // Find or create node for this hop
                    let hop_id = Self::find_or_create_hop_node(&mut nodes, hop, connections);

                    // Add edge: prev → hop
                    edges_set.insert(TopologyEdge {
                        from: prev.clone(),
                        to: hop_id.clone(),
                        cost: 1,
                    });

                    prev = hop_id;
                }

                // Final edge: last_hop → target
                edges_set.insert(TopologyEdge {
                    from: prev,
                    to: node_id,
                    cost: 1,
                });
            }
        }

        // Load and merge custom edges overlay
        if let Ok(overlay) = Self::load_edges_overlay() {
            // Add custom edges
            for edge in overlay.custom_edges {
                edges_set.insert(edge);
            }
            // Remove excluded edges
            for edge in overlay.excluded_edges {
                edges_set.remove(&edge);
            }
        }

        NetworkTopology {
            version: "2.0".to_string(),
            nodes,
            edges: edges_set.into_iter().collect(),
        }
    }

    /// Find existing node matching a proxy hop, or create a temporary one
    fn find_or_create_hop_node(
        nodes: &mut HashMap<String, TopologyNodeConfig>,
        hop: &crate::config::types::ProxyHopConfig,
        connections: &[SavedConnection],
    ) -> String {
        // Try to find existing connection with same host:port:username
        for conn in connections {
            if conn.host == hop.host && conn.port == hop.port && conn.username == hop.username {
                return conn.id.clone();
            }
        }

        // Check if already in nodes (from previous hop processing)
        let hop_key = format!("{}:{}@{}", hop.username, hop.host, hop.port);
        if nodes.contains_key(&hop_key) {
            return hop_key;
        }

        // Create temporary node for this hop
        let auth_type = match &hop.auth {
            crate::config::types::SavedAuth::Password { .. } => "password",
            crate::config::types::SavedAuth::Key { .. } => "key",
            crate::config::types::SavedAuth::Certificate { .. } => "certificate",
            crate::config::types::SavedAuth::Agent => "agent",
        };

        let key_path = match &hop.auth {
            crate::config::types::SavedAuth::Key { key_path, .. } => Some(key_path.clone()),
            crate::config::types::SavedAuth::Certificate { key_path, .. } => Some(key_path.clone()),
            _ => None,
        };

        nodes.insert(
            hop_key.clone(),
            TopologyNodeConfig {
                id: hop_key.clone(),
                host: hop.host.clone(),
                port: hop.port,
                username: hop.username.clone(),
                auth_type: auth_type.to_string(),
                key_path,
                display_name: Some(format!("{}@{}", hop.username, hop.host)),
                is_local: false,
                tags: vec!["auto-generated".to_string()],
                saved_connection_id: None,
            },
        );

        hop_key
    }

    /// Get edges overlay config path
    fn get_edges_config_path() -> Result<PathBuf, String> {
        let config_dir =
            dirs::config_dir().ok_or_else(|| "Failed to get config directory".to_string())?;
        Ok(config_dir.join("oxideterm").join("topology_edges.json"))
    }

    /// Load custom edges overlay
    fn load_edges_overlay() -> Result<TopologyEdgesConfig, String> {
        let path = Self::get_edges_config_path()?;
        if !path.exists() {
            return Ok(TopologyEdgesConfig::default());
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read edges config: {}", e))?;

        serde_json::from_str(&content).map_err(|e| format!("Failed to parse edges config: {}", e))
    }

    /// Save custom edges overlay
    pub fn save_edges_overlay(config: &TopologyEdgesConfig) -> Result<PathBuf, String> {
        let path = Self::get_edges_config_path()?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }

        let content = serde_json::to_string_pretty(config)
            .map_err(|e| format!("Failed to serialize edges config: {}", e))?;

        std::fs::write(&path, content)
            .map_err(|e| format!("Failed to write edges config: {}", e))?;

        Ok(path)
    }

    /// Add a custom edge
    pub fn add_custom_edge(from: String, to: String, cost: i32) -> Result<(), String> {
        let mut config = Self::load_edges_overlay().unwrap_or_default();

        let edge = TopologyEdge { from, to, cost };
        if !config.custom_edges.contains(&edge) {
            config.custom_edges.push(edge);
            Self::save_edges_overlay(&config)?;
        }

        Ok(())
    }

    /// Remove a custom edge
    pub fn remove_custom_edge(from: &str, to: &str) -> Result<(), String> {
        let mut config = Self::load_edges_overlay().unwrap_or_default();

        config
            .custom_edges
            .retain(|e| !(e.from == from && e.to == to));
        Self::save_edges_overlay(&config)?;

        Ok(())
    }

    /// Exclude an auto-generated edge
    pub fn exclude_edge(from: String, to: String) -> Result<(), String> {
        let mut config = Self::load_edges_overlay().unwrap_or_default();

        let edge = TopologyEdge { from, to, cost: 1 };
        if !config.excluded_edges.contains(&edge) {
            config.excluded_edges.push(edge);
            Self::save_edges_overlay(&config)?;
        }

        Ok(())
    }

    /// Get all edges (for UI display)
    pub fn get_all_edges(&self) -> Vec<TopologyEdge> {
        self.edges.clone()
    }

    /// Get custom edges overlay
    pub fn get_edges_overlay() -> TopologyEdgesConfig {
        Self::load_edges_overlay().unwrap_or_default()
    }

    /// Compute shortest path from local to target node (Dijkstra)
    pub fn compute_route(&self, target_id: &str) -> Result<RouteResult, String> {
        // Validate target exists
        if !self.nodes.contains_key(target_id) {
            return Err(format!("Target node '{}' not found in topology", target_id));
        }

        // Build adjacency list
        let mut adj: HashMap<String, Vec<(String, i32)>> = HashMap::new();
        adj.insert("local".to_string(), vec![]);
        for node_id in self.nodes.keys() {
            adj.insert(node_id.clone(), vec![]);
        }
        for edge in &self.edges {
            adj.entry(edge.from.clone())
                .or_default()
                .push((edge.to.clone(), edge.cost));
        }

        // Dijkstra algorithm
        let mut dist: HashMap<String, i32> = HashMap::new();
        let mut prev: HashMap<String, String> = HashMap::new();
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
                    let next_cost = cost + edge_cost;
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

        // Check reachability
        if !prev.contains_key(target_id) {
            return Err(format!("No route found to '{}'", target_id));
        }

        // Backtrack path (excluding local and target)
        let mut path = vec![];
        let mut current = target_id.to_string();
        while let Some(p) = prev.get(&current) {
            if p == "local" {
                break;
            }
            path.push(p.clone());
            current = p.clone();
        }
        path.reverse();

        let total_cost = *dist.get(target_id).unwrap_or(&0);
        Ok(RouteResult { path, total_cost })
    }

    /// Get all nodes info (for frontend display)
    pub fn get_all_nodes(&self) -> Vec<TopologyNodeInfo> {
        // Pre-compute neighbors for each node
        let neighbors_map: HashMap<String, Vec<String>> = {
            let mut map: HashMap<String, Vec<String>> = HashMap::new();
            for edge in &self.edges {
                map.entry(edge.from.clone())
                    .or_default()
                    .push(edge.to.clone());
            }
            map
        };

        self.nodes
            .values()
            .filter(|n| !n.is_local)
            .map(|n| TopologyNodeInfo {
                id: n.id.clone(),
                host: n.host.clone(),
                port: n.port,
                username: n.username.clone(),
                display_name: n.display_name.clone(),
                auth_type: n.auth_type.clone(),
                is_local: n.is_local,
                neighbors: neighbors_map.get(&n.id).cloned().unwrap_or_default(),
                tags: n.tags.clone(),
                saved_connection_id: n.saved_connection_id.clone(),
            })
            .collect()
    }

    /// Get a specific node config
    pub fn get_node(&self, node_id: &str) -> Option<&TopologyNodeConfig> {
        self.nodes.get(node_id)
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_topology() -> NetworkTopology {
        NetworkTopology {
            version: "2.0".to_string(),
            nodes: {
                let mut nodes = HashMap::new();
                nodes.insert(
                    "jump".to_string(),
                    TopologyNodeConfig {
                        id: "jump".to_string(),
                        host: "jump.test".to_string(),
                        port: 22,
                        username: "user".to_string(),
                        auth_type: "agent".to_string(),
                        key_path: None,
                        display_name: None,
                        is_local: false,
                        tags: vec![],
                        saved_connection_id: None,
                    },
                );
                nodes.insert(
                    "bastion".to_string(),
                    TopologyNodeConfig {
                        id: "bastion".to_string(),
                        host: "bastion.test".to_string(),
                        port: 22,
                        username: "admin".to_string(),
                        auth_type: "agent".to_string(),
                        key_path: None,
                        display_name: None,
                        is_local: false,
                        tags: vec![],
                        saved_connection_id: None,
                    },
                );
                nodes.insert(
                    "target".to_string(),
                    TopologyNodeConfig {
                        id: "target".to_string(),
                        host: "target.test".to_string(),
                        port: 22,
                        username: "root".to_string(),
                        auth_type: "agent".to_string(),
                        key_path: None,
                        display_name: None,
                        is_local: false,
                        tags: vec![],
                        saved_connection_id: None,
                    },
                );
                nodes
            },
            edges: vec![
                TopologyEdge {
                    from: "local".to_string(),
                    to: "jump".to_string(),
                    cost: 1,
                },
                TopologyEdge {
                    from: "jump".to_string(),
                    to: "bastion".to_string(),
                    cost: 1,
                },
                TopologyEdge {
                    from: "bastion".to_string(),
                    to: "target".to_string(),
                    cost: 1,
                },
            ],
        }
    }

    #[test]
    fn test_compute_route_simple() {
        let topology = create_test_topology();
        let result = topology.compute_route("target").unwrap();

        assert_eq!(result.path, vec!["jump", "bastion"]);
        assert_eq!(result.total_cost, 3);
    }

    #[test]
    fn test_compute_route_direct() {
        let mut topology = create_test_topology();
        topology.edges.push(TopologyEdge {
            from: "local".to_string(),
            to: "target".to_string(),
            cost: 10,
        });

        // Should choose shorter path
        let result = topology.compute_route("target").unwrap();
        assert_eq!(result.path, vec!["jump", "bastion"]);
        assert_eq!(result.total_cost, 3);
    }

    #[test]
    fn test_compute_route_not_found() {
        let topology = create_test_topology();
        let result = topology.compute_route("nonexistent");

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn test_compute_route_unreachable() {
        let mut topology = create_test_topology();
        topology.nodes.insert(
            "isolated".to_string(),
            TopologyNodeConfig {
                id: "isolated".to_string(),
                host: "isolated.test".to_string(),
                port: 22,
                username: "user".to_string(),
                auth_type: "agent".to_string(),
                key_path: None,
                display_name: None,
                is_local: false,
                tags: vec![],
                saved_connection_id: None,
            },
        );

        let result = topology.compute_route("isolated");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No route found"));
    }
}
