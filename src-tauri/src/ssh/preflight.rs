//! SSH Host Key Preflight Check
//!
//! TOFU (Trust On First Use) implementation for SSH host key verification.
//! This module provides a preflight check that validates host keys before
//! establishing a full SSH connection.
//!
//! # Flow
//! 1. Frontend calls `ssh_preflight(host, port)` before connecting
//! 2. Backend initiates SSH handshake, captures host key in `check_server_key` callback
//! 3. Returns `HostKeyStatus` to frontend (Verified/Unknown/Changed)
//! 4. Frontend shows confirmation dialog if needed
//! 5. Frontend proceeds with `ssh_connect` with `trust_host_key` flag if user approves

use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use async_trait::async_trait;
use dashmap::DashMap;
use russh::client::{self, Config};
use russh_keys::PublicKey;
use serde::Serialize;
use tracing::{debug, info, warn};

use super::error::SshError;
use super::known_hosts::{get_known_hosts, HostKeyVerification, KnownHostsStore};

/// Cache TTL for verified hosts (1 hour)
const CACHE_TTL_SECS: u64 = 3600;

/// Result of host key preflight check
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HostKeyStatus {
    /// Host key matches known_hosts - safe to connect
    Verified,
    /// First time connecting to this host
    Unknown {
        /// SHA256 fingerprint (e.g., "SHA256:abc123...")
        fingerprint: String,
        /// Key type (e.g., "ssh-ed25519", "ssh-rsa")
        key_type: String,
    },
    /// Host key changed from known_hosts - possible MITM attack!
    Changed {
        /// Expected fingerprint from known_hosts
        expected_fingerprint: String,
        /// Actual fingerprint from server
        actual_fingerprint: String,
        /// Key type
        key_type: String,
    },
    /// Connection error during preflight
    Error {
        message: String,
    },
}

/// Cache entry for verified hosts
#[derive(Clone)]
struct CacheEntry {
    fingerprint: String,
    verified_at: SystemTime,
}

/// Global cache for verified host keys (memory only, not persisted)
/// Key: "host:port", Value: (fingerprint, timestamp)
pub struct HostKeyCache {
    cache: DashMap<String, CacheEntry>,
}

impl HostKeyCache {
    pub fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    /// Check if host was recently verified (within TTL)
    pub fn get_verified(&self, host: &str, port: u16) -> Option<String> {
        let key = format!("{}:{}", host.to_lowercase(), port);
        if let Some(entry) = self.cache.get(&key) {
            if let Ok(elapsed) = entry.verified_at.elapsed() {
                if elapsed.as_secs() < CACHE_TTL_SECS {
                    debug!("Host key cache hit for {}", key);
                    return Some(entry.fingerprint.clone());
                } else {
                    // Expired, remove from cache
                    drop(entry);
                    self.cache.remove(&key);
                }
            }
        }
        None
    }

    /// Mark host as verified
    pub fn set_verified(&self, host: &str, port: u16, fingerprint: String) {
        let key = format!("{}:{}", host.to_lowercase(), port);
        self.cache.insert(
            key,
            CacheEntry {
                fingerprint,
                verified_at: SystemTime::now(),
            },
        );
    }

    /// Clear a specific host from cache (e.g., when key changes)
    pub fn invalidate(&self, host: &str, port: u16) {
        let key = format!("{}:{}", host.to_lowercase(), port);
        self.cache.remove(&key);
    }

    /// Clear all cached entries
    pub fn clear(&self) {
        self.cache.clear();
    }
}

impl Default for HostKeyCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Global host key cache instance
static HOST_KEY_CACHE: std::sync::LazyLock<HostKeyCache> =
    std::sync::LazyLock::new(HostKeyCache::new);

/// Get the global host key cache
pub fn get_host_key_cache() -> &'static HostKeyCache {
    &HOST_KEY_CACHE
}

/// Preflight handler that captures the host key and aborts connection
struct PreflightHandler {
    host: String,
    port: u16,
    /// Captured result from check_server_key
    result: Arc<tokio::sync::Mutex<Option<HostKeyStatus>>>,
}

impl PreflightHandler {
    fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            result: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Get key type name from public key
    fn key_type_name(key: &PublicKey) -> &'static str {
        match key.algorithm().as_str() {
            "ssh-ed25519" => "ssh-ed25519",
            "ssh-rsa" => "ssh-rsa",
            "ecdsa-sha2-nistp256" => "ecdsa-sha2-nistp256",
            "ecdsa-sha2-nistp384" => "ecdsa-sha2-nistp384",
            "ecdsa-sha2-nistp521" => "ecdsa-sha2-nistp521",
            other => {
                warn!("Unknown key algorithm: {}", other);
                "unknown"
            }
        }
    }
}

#[async_trait]
impl client::Handler for PreflightHandler {
    type Error = SshError;

    async fn check_server_key(&mut self, server_public_key: &PublicKey) -> Result<bool, Self::Error> {
        let known_hosts = get_known_hosts();
        let verification = known_hosts.verify(&self.host, self.port, server_public_key);
        let key_type = Self::key_type_name(server_public_key).to_string();

        let status = match verification {
            HostKeyVerification::Verified => {
                info!("Preflight: Host key verified for {}:{}", self.host, self.port);
                // Update cache
                let fingerprint = KnownHostsStore::fingerprint(server_public_key);
                get_host_key_cache().set_verified(&self.host, self.port, fingerprint);
                HostKeyStatus::Verified
            }
            HostKeyVerification::Unknown { fingerprint } => {
                info!(
                    "Preflight: Unknown host {}:{} (fingerprint: {})",
                    self.host, self.port, fingerprint
                );
                HostKeyStatus::Unknown {
                    fingerprint,
                    key_type,
                }
            }
            HostKeyVerification::Changed {
                expected_fingerprint,
                actual_fingerprint,
            } => {
                warn!(
                    "Preflight: HOST KEY CHANGED for {}:{} - Expected: {}, Actual: {}",
                    self.host, self.port, expected_fingerprint, actual_fingerprint
                );
                // Invalidate cache for this host
                get_host_key_cache().invalidate(&self.host, self.port);
                HostKeyStatus::Changed {
                    expected_fingerprint,
                    actual_fingerprint,
                    key_type,
                }
            }
        };

        // Store the result
        *self.result.lock().await = Some(status.clone());

        // For Verified status, we still abort - preflight is just for checking
        // Return false to reject the connection (we don't want to complete it)
        Err(SshError::ConnectionFailed("Preflight check complete".to_string()))
    }
}

/// Perform a preflight check to verify host key
///
/// This initiates an SSH handshake but aborts after receiving the host key.
/// The result indicates whether the host is known and trusted.
pub async fn check_host_key(host: &str, port: u16, timeout_secs: u64) -> HostKeyStatus {
    // Check cache first
    if get_host_key_cache().get_verified(host, port).is_some() {
        debug!("Using cached verification for {}:{}", host, port);
        return HostKeyStatus::Verified;
    }

    let addr = format!("{}:{}", host, port);
    debug!("Starting preflight check for {}", addr);

    // Resolve address
    let socket_addr = match addr.to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) => addr,
            None => {
                return HostKeyStatus::Error {
                    message: format!("Could not resolve address: {}", addr),
                }
            }
        },
        Err(e) => {
            return HostKeyStatus::Error {
                message: format!("DNS resolution failed: {}", e),
            }
        }
    };

    // Create handler to capture host key
    let handler = PreflightHandler::new(host.to_string(), port);
    let result_ref = handler.result.clone();

    // SSH config with short timeout
    let ssh_config = Config {
        inactivity_timeout: Some(Duration::from_secs(timeout_secs)),
        ..Default::default()
    };

    // Attempt connection - we expect this to fail after capturing the key
    let connect_result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client::connect(Arc::new(ssh_config), socket_addr, handler),
    )
    .await;

    // Check if we captured a result before the connection failed/aborted
    if let Some(status) = result_ref.lock().await.take() {
        return status;
    }

    // If we got here without a result, something went wrong
    match connect_result {
        Ok(Ok(_)) => {
            // Connection succeeded unexpectedly (shouldn't happen with our handler)
            HostKeyStatus::Error {
                message: "Unexpected: connection completed during preflight".to_string(),
            }
        }
        Ok(Err(e)) => {
            // Connection error
            HostKeyStatus::Error {
                message: format!("Connection failed: {}", e),
            }
        }
        Err(_) => {
            // Timeout
            HostKeyStatus::Error {
                message: format!("Connection timeout after {}s", timeout_secs),
            }
        }
    }
}

/// Add a host key to known_hosts (called after user confirms Unknown status)
pub fn accept_host_key(host: &str, port: u16, fingerprint: &str) -> Result<(), String> {
    // Note: We can't directly add from fingerprint alone - we need the full public key.
    // This function is a placeholder for the flow where user trusts the host.
    // The actual key addition happens during the real connection with trust_host_key=true.
    
    // Update cache to mark as trusted for this session
    get_host_key_cache().set_verified(host, port, fingerprint.to_string());
    
    info!(
        "Host key accepted for {}:{} (fingerprint: {})",
        host, port, fingerprint
    );
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_operations() {
        let cache = HostKeyCache::new();
        
        // Initially empty
        assert!(cache.get_verified("test.example.com", 22).is_none());
        
        // Set and get
        cache.set_verified("test.example.com", 22, "SHA256:abc123".to_string());
        assert_eq!(
            cache.get_verified("test.example.com", 22),
            Some("SHA256:abc123".to_string())
        );
        
        // Case insensitive
        assert_eq!(
            cache.get_verified("TEST.EXAMPLE.COM", 22),
            Some("SHA256:abc123".to_string())
        );
        
        // Different port
        assert!(cache.get_verified("test.example.com", 2222).is_none());
        
        // Invalidate
        cache.invalidate("test.example.com", 22);
        assert!(cache.get_verified("test.example.com", 22).is_none());
    }
}
