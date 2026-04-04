// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! Configuration Types
//!
//! Data structures for saved connections with version support for migrations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Current configuration version
pub const CONFIG_VERSION: u32 = 1;

/// Proxy hop configuration for multi-hop connections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyHopConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SavedAuth,
}

/// Authentication method for saved connections
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SavedAuth {
    /// Password stored in system keychain
    Password {
        /// Keychain entry ID (None if user chose not to save password)
        keychain_id: Option<String>,
    },
    /// SSH key file
    Key {
        /// Path to private key file
        key_path: String,
        /// Whether key requires passphrase
        has_passphrase: bool,
        /// Keychain entry ID for passphrase (if any)
        passphrase_keychain_id: Option<String>,
    },
    /// Use SSH agent
    Agent,
    /// SSH certificate authentication
    Certificate {
        /// Path to private key file
        key_path: String,
        /// Path to certificate file (*-cert.pub)
        cert_path: String,
        /// Whether key requires passphrase
        has_passphrase: bool,
        /// Keychain entry ID for passphrase (if any)
        passphrase_keychain_id: Option<String>,
    },
}

/// Connection options
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionOptions {
    /// Keep-alive interval in seconds (0 = disabled)
    #[serde(default)]
    pub keep_alive_interval: u32,

    /// Enable compression
    #[serde(default)]
    pub compression: bool,

    /// Jump host for ProxyJump (legacy - for backwards compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_host: Option<String>,

    /// Custom terminal type (default: xterm-256color)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub term_type: Option<String>,
}

/// A saved connection configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConnection {
    /// Unique identifier
    pub id: String,

    /// Configuration version
    pub version: u32,

    /// Display name
    pub name: String,

    /// Group name for organization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,

    /// SSH host
    pub host: String,

    /// SSH port (default 22)
    #[serde(default = "default_port")]
    pub port: u16,

    /// SSH username
    pub username: String,

    /// Authentication method
    pub auth: SavedAuth,

    /// Connection options
    #[serde(default)]
    pub options: ConnectionOptions,

    /// Creation timestamp
    pub created_at: DateTime<Utc>,

    /// Last used timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,

    /// Custom color for UI (hex format)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Tags for filtering
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Proxy chain for multi-hop connections (intermediate jump hosts only)
    /// Target server info is always in host/port/username fields
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub proxy_chain: Vec<ProxyHopConfig>,
}

fn default_port() -> u16 {
    22
}

impl SavedConnection {
    /// Create a new saved connection with password auth
    pub fn new_password(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        keychain_id: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            version: CONFIG_VERSION,
            name: name.into(),
            group: None,
            host: host.into(),
            port,
            username: username.into(),
            auth: SavedAuth::Password {
                keychain_id: Some(keychain_id.into()),
            },
            options: ConnectionOptions::default(),
            created_at: Utc::now(),
            last_used_at: None,
            color: None,
            tags: Vec::new(),
            proxy_chain: Vec::new(),
        }
    }

    /// Create a new saved connection with key auth
    pub fn new_key(
        name: impl Into<String>,
        host: impl Into<String>,
        port: u16,
        username: impl Into<String>,
        key_path: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            version: CONFIG_VERSION,
            name: name.into(),
            group: None,
            host: host.into(),
            port,
            username: username.into(),
            auth: SavedAuth::Key {
                key_path: key_path.into(),
                has_passphrase: false,
                passphrase_keychain_id: None,
            },
            options: ConnectionOptions::default(),
            created_at: Utc::now(),
            last_used_at: None,
            color: None,
            tags: Vec::new(),
            proxy_chain: Vec::new(),
        }
    }

    /// Update last used timestamp
    pub fn touch(&mut self) {
        self.last_used_at = Some(Utc::now());
    }

    /// Get display string (user@host:port)
    pub fn display_string(&self) -> String {
        if self.port == 22 {
            format!("{}@{}", self.username, self.host)
        } else {
            format!("{}@{}:{}", self.username, self.host, self.port)
        }
    }
}

/// Root configuration file structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Configuration version
    pub version: u32,

    /// Saved connections
    pub connections: Vec<SavedConnection>,

    /// Connection groups (for ordering)
    #[serde(default)]
    pub groups: Vec<String>,

    /// Recently used connection IDs (most recent first)
    #[serde(default)]
    pub recent: Vec<String>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            connections: Vec::new(),
            groups: Vec::new(),
            recent: Vec::new(),
        }
    }
}

impl ConfigFile {
    /// Add a connection
    pub fn add_connection(&mut self, connection: SavedConnection) {
        // Remove existing with same ID if any
        self.connections.retain(|c| c.id != connection.id);
        self.connections.push(connection);
    }

    /// Remove a connection by ID
    pub fn remove_connection(&mut self, id: &str) -> Option<SavedConnection> {
        if let Some(pos) = self.connections.iter().position(|c| c.id == id) {
            self.recent.retain(|r| r != id);
            Some(self.connections.remove(pos))
        } else {
            None
        }
    }

    /// Get connection by ID
    pub fn get_connection(&self, id: &str) -> Option<&SavedConnection> {
        self.connections.iter().find(|c| c.id == id)
    }

    /// Get mutable connection by ID
    pub fn get_connection_mut(&mut self, id: &str) -> Option<&mut SavedConnection> {
        self.connections.iter_mut().find(|c| c.id == id)
    }

    /// Mark connection as recently used
    pub fn mark_used(&mut self, id: &str) {
        // Remove from recent list if exists
        self.recent.retain(|r| r != id);
        // Add to front
        self.recent.insert(0, id.to_string());
        // Keep only last 10
        self.recent.truncate(10);

        // Update last_used_at
        if let Some(conn) = self.get_connection_mut(id) {
            conn.touch();
        }
    }

    /// Get recent connections
    pub fn get_recent(&self, limit: usize) -> Vec<&SavedConnection> {
        self.recent
            .iter()
            .take(limit)
            .filter_map(|id| self.get_connection(id))
            .collect()
    }

    /// Get connections by group
    pub fn get_by_group(&self, group: Option<&str>) -> Vec<&SavedConnection> {
        self.connections
            .iter()
            .filter(|c| c.group.as_deref() == group)
            .collect()
    }

    /// Search connections by name or host
    pub fn search(&self, query: &str) -> Vec<&SavedConnection> {
        let query_lower = query.to_lowercase();
        self.connections
            .iter()
            .filter(|c| {
                c.name.to_lowercase().contains(&query_lower)
                    || c.host.to_lowercase().contains(&query_lower)
                    || c.username.to_lowercase().contains(&query_lower)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_display() {
        let conn = SavedConnection::new_password("Test", "example.com", 22, "user", "kc-123");
        assert_eq!(conn.display_string(), "user@example.com");

        let conn2 = SavedConnection::new_password("Test", "example.com", 2222, "user", "kc-123");
        assert_eq!(conn2.display_string(), "user@example.com:2222");
    }

    #[test]
    fn test_config_file_operations() {
        let mut config = ConfigFile::default();

        let conn = SavedConnection::new_password("Test", "example.com", 22, "user", "kc-123");
        let id = conn.id.clone();

        config.add_connection(conn);
        assert_eq!(config.connections.len(), 1);

        config.mark_used(&id);
        assert_eq!(config.recent.len(), 1);
        assert_eq!(config.recent[0], id);

        let removed = config.remove_connection(&id);
        assert!(removed.is_some());
        assert_eq!(config.connections.len(), 0);
        assert_eq!(config.recent.len(), 0);
    }

    #[test]
    fn test_new_key_connection() {
        let conn = SavedConnection::new_key("GPU Node", "gpu.hpc.edu", 22, "student", "/home/student/.ssh/id_ed25519");
        assert_eq!(conn.name, "GPU Node");
        assert_eq!(conn.host, "gpu.hpc.edu");
        assert_eq!(conn.port, 22);
        assert_eq!(conn.username, "student");
        assert!(matches!(conn.auth, SavedAuth::Key { ref key_path, .. } if key_path == "/home/student/.ssh/id_ed25519"));
        assert!(conn.last_used_at.is_none());
    }

    #[test]
    fn test_touch_sets_last_used() {
        let mut conn = SavedConnection::new_password("Test", "host", 22, "user", "kc-1");
        assert!(conn.last_used_at.is_none());
        conn.touch();
        assert!(conn.last_used_at.is_some());
    }

    #[test]
    fn test_search_by_name() {
        let mut config = ConfigFile::default();
        config.add_connection(SavedConnection::new_password("Production DB", "db.prod.com", 22, "admin", "kc-1"));
        config.add_connection(SavedConnection::new_password("Staging API", "api.staging.com", 22, "deploy", "kc-2"));

        let results = config.search("prod");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Production DB");
    }

    #[test]
    fn test_search_by_host() {
        let mut config = ConfigFile::default();
        config.add_connection(SavedConnection::new_password("Server", "192.168.1.100", 22, "root", "kc-1"));

        let results = config.search("192.168");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_by_username() {
        let mut config = ConfigFile::default();
        config.add_connection(SavedConnection::new_password("Server", "host.com", 22, "deploy_user", "kc-1"));

        let results = config.search("deploy");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_search_case_insensitive() {
        let mut config = ConfigFile::default();
        config.add_connection(SavedConnection::new_password("MyServer", "example.com", 22, "root", "kc-1"));

        assert_eq!(config.search("myserver").len(), 1);
        assert_eq!(config.search("MYSERVER").len(), 1);
    }

    #[test]
    fn test_search_no_match() {
        let mut config = ConfigFile::default();
        config.add_connection(SavedConnection::new_password("Server", "host.com", 22, "root", "kc-1"));

        assert!(config.search("nonexistent").is_empty());
    }
}
