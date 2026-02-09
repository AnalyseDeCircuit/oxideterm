//! Known hosts management for SSH host key verification
//!
//! Wraps russh::keys known_hosts functionality with additional features.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use parking_lot::RwLock;
use russh::keys::{PublicKey, PublicKeyBase64};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use super::error::SshError;

/// Result of host key verification
#[derive(Debug, Clone, PartialEq)]
pub enum HostKeyVerification {
    /// Key matches known_hosts entry
    Verified,
    /// Host not in known_hosts (first connection)
    Unknown { fingerprint: String },
    /// Key changed from known_hosts entry (potential MITM)
    Changed {
        expected_fingerprint: String,
        actual_fingerprint: String,
    },
}

/// Entry in known_hosts: (key_type, base64_key)
#[derive(Clone, Debug)]
struct HostKeyEntry {
    key_type: String,
    key_data: String,
}

/// Global known hosts store
pub struct KnownHostsStore {
    /// Cache of host -> list of keys (supports multiple key types per host)
    hosts: RwLock<HashMap<String, Vec<HostKeyEntry>>>,
    /// Path to known_hosts file
    path: PathBuf,
}

impl Default for KnownHostsStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KnownHostsStore {
    /// Create a new known hosts store, loading from default location
    pub fn new() -> Self {
        let path = dirs::home_dir()
            .map(|h| h.join(".ssh").join("known_hosts"))
            .unwrap_or_else(|| PathBuf::from("~/.ssh/known_hosts"));

        let mut store = Self {
            hosts: RwLock::new(HashMap::new()),
            path,
        };

        if let Err(e) = store.load() {
            warn!("Failed to load known_hosts: {}", e);
        }

        store
    }

    /// Create with custom path (for testing)
    pub fn with_path(path: PathBuf) -> Self {
        let mut store = Self {
            hosts: RwLock::new(HashMap::new()),
            path,
        };

        if let Err(e) = store.load() {
            debug!("Known hosts file not found or empty: {}", e);
        }

        store
    }

    /// Load known_hosts file
    fn load(&mut self) -> Result<(), SshError> {
        if !self.path.exists() {
            return Ok(());
        }

        let file = fs::File::open(&self.path).map_err(SshError::IoError)?;

        let reader = BufReader::new(file);
        let mut hosts = self.hosts.write();
        let mut entry_count = 0;

        for line in reader.lines() {
            let line = line.map_err(SshError::IoError)?;
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse: hostname keytype base64key [comment]
            // Or: hostname,alias keytype base64key [comment]
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 3 {
                continue;
            }

            let hostnames = parts[0];
            let key_type = parts[1].to_string();
            let key_data = parts[2].to_string();

            let entry = HostKeyEntry { key_type, key_data };

            // Handle multiple hostnames (comma-separated)
            for hostname in hostnames.split(',') {
                // Handle hashed hostnames (|1|...) - skip for now
                if hostname.starts_with('|') {
                    continue;
                }

                // Normalize: remove [port] suffix if present
                let normalized = Self::normalize_hostname(hostname);
                hosts.entry(normalized).or_default().push(entry.clone());
                entry_count += 1;
            }
        }

        info!(
            "Loaded {} known host entries ({} unique hosts)",
            entry_count,
            hosts.len()
        );
        Ok(())
    }

    /// Normalize hostname for lookup
    fn normalize_hostname(host: &str) -> String {
        // Remove brackets and port: [host]:port -> host
        let host = host.trim_start_matches('[');
        if let Some(idx) = host.find("]:") {
            host[..idx].to_lowercase()
        } else {
            host.trim_end_matches(']').to_lowercase()
        }
    }

    /// Create lookup key for host:port
    fn make_key(host: &str, port: u16) -> String {
        let host = host.to_lowercase();
        if port == 22 {
            host
        } else {
            format!("[{}]:{}", host, port)
        }
    }

    /// Compute SHA256 fingerprint of public key
    pub fn fingerprint(key: &PublicKey) -> String {
        let key_bytes = key.public_key_bytes();
        let mut hasher = Sha256::new();
        hasher.update(&key_bytes);
        let hash = hasher.finalize();
        format!("SHA256:{}", BASE64.encode(hash).trim_end_matches('='))
    }

    /// Verify a host's public key
    pub fn verify(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyVerification {
        let lookup_key = Self::make_key(host, port);
        let actual_key_b64 = BASE64.encode(key.public_key_bytes());
        let actual_key_type = Self::key_type_name(key);
        let fingerprint = Self::fingerprint(key);

        let hosts = self.hosts.read();

        // Helper to check entries for a given hostname
        let check_entries = |entries: &Vec<HostKeyEntry>| -> Option<HostKeyVerification> {
            // Find entry with matching key type
            for entry in entries {
                if entry.key_type == actual_key_type {
                    if entry.key_data == actual_key_b64 {
                        debug!(
                            "Host key verified for {} (type: {})",
                            lookup_key, actual_key_type
                        );
                        return Some(HostKeyVerification::Verified);
                    } else {
                        let expected_fingerprint =
                            Self::compute_fingerprint_from_b64(&entry.key_data);
                        warn!(
                            "HOST KEY CHANGED for {} (type: {})! Expected {}, got {}",
                            lookup_key, actual_key_type, expected_fingerprint, fingerprint
                        );
                        return Some(HostKeyVerification::Changed {
                            expected_fingerprint,
                            actual_fingerprint: fingerprint.clone(),
                        });
                    }
                }
            }
            // No matching key type found - host is known but not for this key type
            None
        };

        // Try exact match first (with port)
        if let Some(entries) = hosts.get(&lookup_key) {
            if let Some(result) = check_entries(entries) {
                return result;
            }
            // Host known but not for this key type - treat as new key type (auto-accept)
            debug!(
                "Host {} known but no {} key stored, treating as new",
                lookup_key, actual_key_type
            );
            return HostKeyVerification::Unknown { fingerprint };
        }

        // Try hostname without port
        let host_only = host.to_lowercase();
        if let Some(entries) = hosts.get(&host_only) {
            if let Some(result) = check_entries(entries) {
                return result;
            }
            // Host known but not for this key type
            debug!(
                "Host {} known but no {} key stored, treating as new",
                host_only, actual_key_type
            );
            return HostKeyVerification::Unknown { fingerprint };
        }

        debug!("Unknown host: {}", lookup_key);
        HostKeyVerification::Unknown { fingerprint }
    }

    /// Compute fingerprint from stored base64 key
    fn compute_fingerprint_from_b64(stored_b64: &str) -> String {
        if let Ok(bytes) = BASE64.decode(stored_b64) {
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let hash = hasher.finalize();
            format!("SHA256:{}", BASE64.encode(hash).trim_end_matches('='))
        } else {
            "unknown".to_string()
        }
    }

    /// Add a new host key to known_hosts
    pub fn add_host(&self, host: &str, port: u16, key: &PublicKey) -> Result<(), SshError> {
        let lookup_key = Self::make_key(host, port);
        let key_b64 = BASE64.encode(key.public_key_bytes());
        let key_type = Self::key_type_name(key).to_string();

        // Update in-memory cache
        {
            let mut hosts = self.hosts.write();
            let entry = HostKeyEntry {
                key_type: key_type.clone(),
                key_data: key_b64.clone(),
            };
            hosts.entry(lookup_key.clone()).or_default().push(entry);
        }

        // Append to file
        self.append_to_file(&lookup_key, &key_type, &key_b64)?;

        info!(
            "Added host key for {} (type: {}) to known_hosts",
            lookup_key, key_type
        );
        Ok(())
    }

    /// Get key type name for known_hosts format
    fn key_type_name(key: &PublicKey) -> &'static str {
        match key.algorithm().as_str() {
            "ssh-ed25519" => "ssh-ed25519",
            "ssh-rsa" => "ssh-rsa",
            "ecdsa-sha2-nistp256" => "ecdsa-sha2-nistp256",
            "ecdsa-sha2-nistp384" => "ecdsa-sha2-nistp384",
            "ecdsa-sha2-nistp521" => "ecdsa-sha2-nistp521",
            _ => "ssh-rsa",
        }
    }

    /// Append entry to known_hosts file
    fn append_to_file(&self, host: &str, key_type: &str, key_b64: &str) -> Result<(), SshError> {
        // Ensure .ssh directory exists
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(SshError::IoError)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(SshError::IoError)?;

        writeln!(file, "{} {} {}", host, key_type, key_b64).map_err(SshError::IoError)?;

        Ok(())
    }

    /// Remove a host from known_hosts (for key rotation)
    pub fn remove_host(&self, host: &str, port: u16) -> Result<(), SshError> {
        let lookup_key = Self::make_key(host, port);

        // Update in-memory cache
        {
            let mut hosts = self.hosts.write();
            hosts.remove(&lookup_key);
        }

        // Rewrite file without this host
        self.rewrite_without_host(&lookup_key)?;

        info!("Removed host key for {} from known_hosts", lookup_key);
        Ok(())
    }

    /// Rewrite known_hosts file without specified host
    fn rewrite_without_host(&self, remove_host: &str) -> Result<(), SshError> {
        if !self.path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&self.path).map_err(SshError::IoError)?;

        let filtered: Vec<&str> = content
            .lines()
            .filter(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.is_empty() {
                    return true; // Keep empty lines
                }
                let hostnames = parts[0];
                !hostnames
                    .split(',')
                    .any(|h| Self::normalize_hostname(h) == remove_host)
            })
            .collect();

        fs::write(&self.path, filtered.join("\n") + "\n").map_err(SshError::IoError)?;

        Ok(())
    }
}

/// Global singleton for known hosts
static KNOWN_HOSTS: std::sync::OnceLock<KnownHostsStore> = std::sync::OnceLock::new();

/// Get the global known hosts store
pub fn get_known_hosts() -> &'static KnownHostsStore {
    KNOWN_HOSTS.get_or_init(KnownHostsStore::new)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_normalize_hostname() {
        assert_eq!(
            KnownHostsStore::normalize_hostname("github.com"),
            "github.com"
        );
        assert_eq!(
            KnownHostsStore::normalize_hostname("[github.com]:22"),
            "github.com"
        );
        assert_eq!(
            KnownHostsStore::normalize_hostname("[server.example.com]:2222"),
            "server.example.com"
        );
    }

    #[test]
    fn test_make_key() {
        assert_eq!(KnownHostsStore::make_key("github.com", 22), "github.com");
        assert_eq!(
            KnownHostsStore::make_key("server.com", 2222),
            "[server.com]:2222"
        );
    }
}
