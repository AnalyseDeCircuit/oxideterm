// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    net::ToSocketAddrs,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::{Duration, SystemTime},
};

use dashmap::DashMap;
use russh::{
    client,
    keys::{PublicKey, PublicKeyBase64, parse_public_key_base64, ssh_key::HashAlg},
};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::SshTransportError;

const CACHE_TTL_SECS: u64 = 3600;
const MAX_CACHE_ENTRIES: usize = 500;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum HostKeyStatus {
    Verified,
    Unknown {
        fingerprint: String,
        key_type: String,
    },
    Changed {
        expected_fingerprint: String,
        actual_fingerprint: String,
        key_type: String,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HostKeyVerification {
    Verified,
    Unknown {
        fingerprint: String,
        key_type: String,
    },
    Changed {
        expected_fingerprint: String,
        actual_fingerprint: String,
        key_type: String,
    },
}

pub fn public_key_fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

pub fn public_key_type(key: &PublicKey) -> String {
    match key.algorithm().as_str() {
        "ssh-ed25519" => "ssh-ed25519",
        "ssh-rsa" => "ssh-rsa",
        "ecdsa-sha2-nistp256" => "ecdsa-sha2-nistp256",
        "ecdsa-sha2-nistp384" => "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp521" => "ecdsa-sha2-nistp521",
        _ => "ssh-rsa",
    }
    .to_string()
}

#[derive(Clone)]
struct CacheEntry {
    fingerprint: String,
    verified_at: SystemTime,
}

struct HostKeyCache {
    cache: DashMap<String, CacheEntry>,
}

impl HostKeyCache {
    fn new() -> Self {
        Self {
            cache: DashMap::new(),
        }
    }

    fn get_verified(&self, host: &str, port: u16) -> Option<String> {
        let key = host_key_cache_key(host, port);
        let entry = self.cache.get(&key)?;
        let elapsed = entry.verified_at.elapsed().ok()?;
        if elapsed.as_secs() < CACHE_TTL_SECS {
            return Some(entry.fingerprint.clone());
        }
        drop(entry);
        self.cache.remove(&key);
        None
    }

    fn set_verified(&self, host: &str, port: u16, fingerprint: String) {
        if self.cache.len() >= MAX_CACHE_ENTRIES {
            self.evict_expired();
        }
        if self.cache.len() >= MAX_CACHE_ENTRIES {
            let mut entries = self
                .cache
                .iter()
                .map(|entry| (entry.key().clone(), entry.value().verified_at))
                .collect::<Vec<_>>();
            entries.sort_by_key(|(_, verified_at)| *verified_at);
            for (key, _) in entries.into_iter().take(MAX_CACHE_ENTRIES / 4) {
                self.cache.remove(&key);
            }
        }
        self.cache.insert(
            host_key_cache_key(host, port),
            CacheEntry {
                fingerprint,
                verified_at: SystemTime::now(),
            },
        );
    }

    fn invalidate(&self, host: &str, port: u16) {
        self.cache.remove(&host_key_cache_key(host, port));
    }

    fn evict_expired(&self) {
        let expired = self
            .cache
            .iter()
            .filter(|entry| {
                entry
                    .value()
                    .verified_at
                    .elapsed()
                    .map(|elapsed| elapsed.as_secs() >= CACHE_TTL_SECS)
                    .unwrap_or(true)
            })
            .map(|entry| entry.key().clone())
            .collect::<Vec<_>>();
        for key in expired {
            self.cache.remove(&key);
        }
    }

    #[cfg(test)]
    fn clear(&self) {
        self.cache.clear();
    }
}

static HOST_KEY_CACHE: LazyLock<HostKeyCache> = LazyLock::new(HostKeyCache::new);

fn host_key_cache_key(host: &str, port: u16) -> String {
    format!("{}:{}", host.to_lowercase(), port)
}

#[derive(Clone, Debug)]
struct HostKeyEntry {
    key_type: String,
    key_data: String,
}

#[derive(Debug)]
struct KnownHostsStore {
    hosts: HashMap<String, Vec<HostKeyEntry>>,
    path: PathBuf,
}

impl KnownHostsStore {
    fn new() -> Result<Self, SshTransportError> {
        Self::with_path(default_known_hosts_path()?)
    }

    fn with_path(path: PathBuf) -> Result<Self, SshTransportError> {
        let mut store = Self {
            hosts: HashMap::new(),
            path,
        };
        store.load()?;
        Ok(store)
    }

    fn load(&mut self) -> Result<(), SshTransportError> {
        if !self.path.exists() {
            return Ok(());
        }

        let file = fs::File::open(&self.path)
            .map_err(|error| SshTransportError::HostKeyCheckFailed(error.to_string()))?;
        for line in BufReader::new(file).lines() {
            let line =
                line.map_err(|error| SshTransportError::HostKeyCheckFailed(error.to_string()))?;
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 3 {
                continue;
            }

            let entry = HostKeyEntry {
                key_type: parts[1].to_string(),
                key_data: parts[2].to_string(),
            };
            for hostname in parts[0].split(',') {
                // Tauri's KnownHostsStore intentionally ignores hashed
                // entries, so native preflight must not verify hosts that
                // Tauri would still treat as unknown.
                if hostname.starts_with('|') {
                    continue;
                }
                let canonical = Self::canonical_host_entry(hostname);
                self.hosts.entry(canonical).or_default().push(entry.clone());
            }
        }
        Ok(())
    }

    fn verify(&self, host: &str, port: u16, key: &PublicKey) -> HostKeyVerification {
        let lookup_key = Self::make_key(host, port);
        let actual_key_b64 = key.public_key_base64();
        let actual_key_type = public_key_type(key);
        let fingerprint = public_key_fingerprint(key);

        if let Some(entries) = self.hosts.get(&lookup_key) {
            if let Some(result) =
                check_known_host_entries(entries, &actual_key_type, &actual_key_b64, &fingerprint)
            {
                return result;
            }
            return HostKeyVerification::Unknown {
                fingerprint,
                key_type: actual_key_type,
            };
        }

        let host_only = host.to_lowercase();
        if let Some(entries) = self.hosts.get(&host_only) {
            if let Some(result) =
                check_known_host_entries(entries, &actual_key_type, &actual_key_b64, &fingerprint)
            {
                return result;
            }
            return HostKeyVerification::Unknown {
                fingerprint,
                key_type: actual_key_type,
            };
        }

        HostKeyVerification::Unknown {
            fingerprint,
            key_type: actual_key_type,
        }
    }

    fn add_host(
        &mut self,
        host: &str,
        port: u16,
        key: &PublicKey,
    ) -> Result<(), SshTransportError> {
        let lookup_key = Self::make_key(host, port);
        let key_type = public_key_type(key);
        let key_data = key.public_key_base64();
        self.hosts
            .entry(lookup_key.clone())
            .or_default()
            .push(HostKeyEntry {
                key_type: key_type.clone(),
                key_data: key_data.clone(),
            });
        append_known_host_line(&self.path, &lookup_key, &key_type, &key_data)
    }

    fn remove_host_key(
        &mut self,
        host: &str,
        port: u16,
        key_type: &str,
        expected_fingerprint: &str,
    ) -> Result<(), SshTransportError> {
        let lookup_key = Self::make_key(host, port);
        let host_only_key = Self::normalize_hostname(host);
        let mut lookup_keys = vec![lookup_key.clone()];
        if host_only_key != lookup_key {
            lookup_keys.push(host_only_key);
        }
        let removed_any =
            rewrite_without_host_key(&self.path, &lookup_keys, key_type, expected_fingerprint)?;

        if !removed_any {
            return Err(SshTransportError::HostKeyCheckFailed(format!(
                "No saved host key matched {lookup_key} (type: {key_type}, fingerprint: {expected_fingerprint})"
            )));
        }

        self.hosts.clear();
        self.load()?;
        HOST_KEY_CACHE.invalidate(host, port);
        Ok(())
    }

    fn canonical_host_entry(hostname: &str) -> String {
        let hostname = hostname.trim();
        if let Some(stripped) = hostname.strip_prefix('[')
            && let Some((host, port_str)) = stripped.split_once("]:")
            && let Ok(port) = port_str.parse::<u16>()
        {
            return Self::make_key(host, port);
        }
        Self::normalize_hostname(hostname)
    }

    fn normalize_hostname(host: &str) -> String {
        let host = host.trim_start_matches('[');
        if let Some(index) = host.find("]:") {
            host[..index].to_lowercase()
        } else {
            host.trim_end_matches(']').to_lowercase()
        }
    }

    fn make_key(host: &str, port: u16) -> String {
        let host = host.to_lowercase();
        if port == 22 {
            host
        } else {
            format!("[{host}]:{port}")
        }
    }
}

fn check_known_host_entries(
    entries: &[HostKeyEntry],
    actual_key_type: &str,
    actual_key_b64: &str,
    actual_fingerprint: &str,
) -> Option<HostKeyVerification> {
    let mut expected_fingerprint = None;
    for entry in entries {
        if entry.key_type != actual_key_type {
            continue;
        }
        if entry.key_data == actual_key_b64 {
            return Some(HostKeyVerification::Verified);
        }
        if expected_fingerprint.is_none() {
            expected_fingerprint = Some(fingerprint_from_known_hosts_key_data(&entry.key_data));
        }
    }

    expected_fingerprint.map(|expected_fingerprint| HostKeyVerification::Changed {
        expected_fingerprint,
        actual_fingerprint: actual_fingerprint.to_string(),
        key_type: actual_key_type.to_string(),
    })
}

fn fingerprint_from_known_hosts_key_data(key_data: &str) -> String {
    parse_public_key_base64(key_data)
        .map(|key| public_key_fingerprint(&key))
        .unwrap_or_else(|_| "unknown".to_string())
}

fn append_known_host_line(
    path: &Path,
    host: &str,
    key_type: &str,
    key_data: &str,
) -> Result<(), SshTransportError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| SshTransportError::HostKeyCheckFailed(error.to_string()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| SshTransportError::HostKeyCheckFailed(error.to_string()))?;
    writeln!(file, "{host} {key_type} {key_data}")
        .map_err(|error| SshTransportError::HostKeyCheckFailed(error.to_string()))
}

fn rewrite_without_host_key(
    path: &Path,
    remove_hosts: &[String],
    remove_key_type: &str,
    remove_fingerprint: &str,
) -> Result<bool, SshTransportError> {
    if !path.exists() {
        return Ok(false);
    }

    let content = fs::read_to_string(path)
        .map_err(|error| SshTransportError::HostKeyCheckFailed(error.to_string()))?;
    let remove_hosts = remove_hosts
        .iter()
        .map(|host| host.to_lowercase())
        .collect::<Vec<_>>();
    let mut removed_any = false;

    let filtered = content
        .lines()
        .filter_map(|line| {
            let parts = line.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 3 {
                return Some(line.to_string());
            }

            let hostnames = parts[0];
            let key_type = parts[1];
            let key_data = parts[2];
            if key_type != remove_key_type
                || fingerprint_from_known_hosts_key_data(key_data) != remove_fingerprint
            {
                return Some(line.to_string());
            }

            let kept_hostnames = hostnames
                .split(',')
                .filter(|hostname| {
                    let canonical = KnownHostsStore::canonical_host_entry(hostname);
                    !remove_hosts
                        .iter()
                        .any(|remove_host| canonical == *remove_host)
                })
                .collect::<Vec<_>>();

            if kept_hostnames.len() == hostnames.split(',').count() {
                return Some(line.to_string());
            }

            removed_any = true;
            if kept_hostnames.is_empty() {
                return None;
            }

            let mut rebuilt = vec![
                kept_hostnames.join(","),
                key_type.to_string(),
                key_data.to_string(),
            ];
            rebuilt.extend(parts.iter().skip(3).map(|part| (*part).to_string()));
            Some(rebuilt.join(" "))
        })
        .collect::<Vec<_>>();

    fs::write(path, filtered.join("\n") + "\n")
        .map_err(|error| SshTransportError::HostKeyCheckFailed(error.to_string()))?;
    Ok(removed_any)
}

pub fn verify_host_key(
    host: &str,
    port: u16,
    server_public_key: &PublicKey,
) -> Result<HostKeyVerification, SshTransportError> {
    if HOST_KEY_CACHE.get_verified(host, port).is_some() {
        return Ok(HostKeyVerification::Verified);
    }
    Ok(KnownHostsStore::new()?.verify(host, port, server_public_key))
}

pub(crate) fn accept_host_key_for_session(host: &str, port: u16, fingerprint: String) {
    HOST_KEY_CACHE.set_verified(host, port, fingerprint);
}

pub fn learn_host_key(
    host: &str,
    port: u16,
    server_public_key: &PublicKey,
) -> Result<(), SshTransportError> {
    let fingerprint = public_key_fingerprint(server_public_key);
    let mut store = KnownHostsStore::new()?;
    store.add_host(host, port, server_public_key)?;
    accept_host_key_for_session(host, port, fingerprint);
    Ok(())
}

pub fn remove_host_key(
    host: &str,
    port: u16,
    key_type: &str,
    expected_fingerprint: &str,
) -> Result<(), SshTransportError> {
    KnownHostsStore::new()?.remove_host_key(host, port, key_type, expected_fingerprint)
}

fn default_known_hosts_path() -> Result<PathBuf, SshTransportError> {
    Ok(known_hosts_path_from_home(std::env::home_dir()))
}

fn known_hosts_path_from_home(home: Option<PathBuf>) -> PathBuf {
    home.map(|home| home.join(".ssh").join("known_hosts"))
        .unwrap_or_else(|| PathBuf::from("~/.ssh/known_hosts"))
}

#[cfg(test)]
fn retain_known_hosts_aliases(line: &str, host_pattern: &str) -> Option<String> {
    let split = line.find(char::is_whitespace)?;
    let hosts = &line[..split];
    let rest = &line[split..];
    let remaining = hosts
        .split(',')
        .filter(|pattern| *pattern != host_pattern)
        .collect::<Vec<_>>();

    if remaining.len() == hosts.split(',').count() || remaining.is_empty() {
        return None;
    }

    Some(format!("{}{}", remaining.join(","), rest))
}

struct PreflightHandler {
    host: String,
    port: u16,
    status: Arc<Mutex<Option<HostKeyStatus>>>,
}

impl PreflightHandler {
    fn new(host: String, port: u16) -> Self {
        Self {
            host,
            port,
            status: Arc::new(Mutex::new(None)),
        }
    }
}

impl client::Handler for PreflightHandler {
    type Error = SshTransportError;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKey,
    ) -> Result<bool, Self::Error> {
        let status = match verify_host_key(&self.host, self.port, server_public_key)? {
            HostKeyVerification::Verified => HostKeyStatus::Verified,
            HostKeyVerification::Unknown {
                fingerprint,
                key_type,
            } => HostKeyStatus::Unknown {
                fingerprint,
                key_type,
            },
            HostKeyVerification::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            } => HostKeyStatus::Changed {
                expected_fingerprint,
                actual_fingerprint,
                key_type,
            },
        };
        *self.status.lock().await = Some(status);
        Err(SshTransportError::PreflightComplete)
    }
}

pub async fn check_host_key(host: &str, port: u16, timeout_secs: u64) -> HostKeyStatus {
    if HOST_KEY_CACHE.get_verified(host, port).is_some() {
        return HostKeyStatus::Verified;
    }

    let addr = format!("{host}:{port}");
    let socket_addr = match addr.to_socket_addrs() {
        Ok(mut addrs) => match addrs.next() {
            Some(addr) => addr,
            None => {
                return HostKeyStatus::Error {
                    message: format!("Could not resolve address: {addr}"),
                };
            }
        },
        Err(error) => {
            return HostKeyStatus::Error {
                message: format!("DNS resolution failed: {error}"),
            };
        }
    };

    let handler = PreflightHandler::new(host.to_string(), port);
    let status = Arc::clone(&handler.status);
    let config = client::Config {
        inactivity_timeout: Some(Duration::from_secs(timeout_secs)),
        ..client::Config::default()
    };

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client::connect(Arc::new(config), socket_addr, handler),
    )
    .await;

    if let Some(status) = status.lock().await.take() {
        return status;
    }

    match result {
        Ok(Ok(_)) => HostKeyStatus::Error {
            message: "Unexpectedly completed SSH preflight".to_string(),
        },
        Ok(Err(SshTransportError::PreflightComplete)) => HostKeyStatus::Error {
            message: "SSH preflight completed without a captured host key".to_string(),
        },
        Ok(Err(error)) => HostKeyStatus::Error {
            message: error.to_string(),
        },
        Err(_) => HostKeyStatus::Error {
            message: format!("Connection timeout after {timeout_secs}s"),
        },
    }
}

pub async fn check_host_key_via_stream(
    host: &str,
    port: u16,
    stream: russh::ChannelStream<client::Msg>,
    timeout_secs: u64,
) -> HostKeyStatus {
    if HOST_KEY_CACHE.get_verified(host, port).is_some() {
        return HostKeyStatus::Verified;
    }

    let handler = PreflightHandler::new(host.to_string(), port);
    let status = Arc::clone(&handler.status);
    let config = client::Config {
        inactivity_timeout: Some(Duration::from_secs(timeout_secs)),
        ..client::Config::default()
    };

    let result = tokio::time::timeout(
        Duration::from_secs(timeout_secs),
        client::connect_stream(Arc::new(config), stream, handler),
    )
    .await;

    if let Some(status) = status.lock().await.take() {
        return status;
    }

    match result {
        Ok(Ok(_)) => HostKeyStatus::Error {
            message: "Unexpectedly completed tunneled SSH preflight".to_string(),
        },
        Ok(Err(SshTransportError::PreflightComplete)) => HostKeyStatus::Error {
            message: "Tunneled SSH preflight completed without a captured host key".to_string(),
        },
        Ok(Err(error)) => HostKeyStatus::Error {
            message: error.to_string(),
        },
        Err(_) => HostKeyStatus::Error {
            message: format!("Connection timeout after {timeout_secs}s"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::{PublicKeyBase64, parse_public_key_base64};

    static CACHE_TEST_LOCK: LazyLock<std::sync::Mutex<()>> =
        LazyLock::new(|| std::sync::Mutex::new(()));

    fn sample_public_key() -> PublicKey {
        parse_public_key_base64(
            "AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ",
        )
        .unwrap()
    }

    fn alternate_public_key() -> PublicKey {
        parse_public_key_base64(
            "AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X",
        )
        .unwrap()
    }

    fn temp_known_hosts_path(test_name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "oxideterm-native-known-hosts-{test_name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ))
    }

    #[test]
    fn host_key_removal_preserves_other_aliases_on_same_line() {
        let line = "example.com,alias.example.com ssh-ed25519 AAAAC3Nza comment";

        assert_eq!(
            retain_known_hosts_aliases(line, "example.com").as_deref(),
            Some("alias.example.com ssh-ed25519 AAAAC3Nza comment")
        );
    }

    #[test]
    fn host_key_removal_drops_single_host_line() {
        let line = "[example.com]:2222 ssh-ed25519 AAAAC3Nza";

        assert_eq!(retain_known_hosts_aliases(line, "[example.com]:2222"), None);
    }

    #[test]
    fn known_hosts_store_skips_hashed_entries_like_tauri() {
        let path = temp_known_hosts_path("hashed");
        let key = sample_public_key();
        fs::write(
            &path,
            format!(
                "|1|salt|hash {} {}\n",
                public_key_type(&key),
                key.public_key_base64()
            ),
        )
        .unwrap();

        let store = KnownHostsStore::with_path(path.clone()).unwrap();

        assert!(matches!(
            store.verify("example.com", 22, &key),
            HostKeyVerification::Unknown { .. }
        ));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn known_hosts_store_verifies_plain_alias_entry() {
        let path = temp_known_hosts_path("alias");
        let key = sample_public_key();
        fs::write(
            &path,
            format!(
                "example.com,alias.example.com {} {}\n",
                public_key_type(&key),
                key.public_key_base64()
            ),
        )
        .unwrap();

        let store = KnownHostsStore::with_path(path.clone()).unwrap();

        assert_eq!(
            store.verify("alias.example.com", 22, &key),
            HostKeyVerification::Verified
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn known_hosts_store_reports_changed_for_same_key_type() {
        let path = temp_known_hosts_path("changed");
        let key = sample_public_key();
        let alternate = alternate_public_key();
        fs::write(
            &path,
            format!(
                "example.com {} {}\n",
                public_key_type(&key),
                key.public_key_base64()
            ),
        )
        .unwrap();

        let store = KnownHostsStore::with_path(path.clone()).unwrap();

        assert_eq!(
            store.verify("example.com", 22, &alternate),
            HostKeyVerification::Changed {
                expected_fingerprint: public_key_fingerprint(&key),
                actual_fingerprint: public_key_fingerprint(&alternate),
                key_type: public_key_type(&alternate),
            }
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn accepted_host_key_cache_makes_preflight_verified_for_session() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        let key = sample_public_key();
        let host = "accepted-cache-only.example.com";
        HOST_KEY_CACHE.clear();

        accept_host_key_for_session(host, 2222, public_key_fingerprint(&key));

        assert_eq!(
            verify_host_key(host, 2222, &key).unwrap(),
            HostKeyVerification::Verified
        );
        HOST_KEY_CACHE.clear();
    }

    #[test]
    fn preflight_returns_verified_from_session_cache_before_network() {
        let _guard = CACHE_TEST_LOCK.lock().unwrap();
        let key = sample_public_key();
        let host = "cached-before-dns.invalid";
        HOST_KEY_CACHE.clear();
        accept_host_key_for_session(host, 22, public_key_fingerprint(&key));

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let status = runtime.block_on(check_host_key(host, 22, 1));

        assert_eq!(status, HostKeyStatus::Verified);
        HOST_KEY_CACHE.clear();
    }

    #[test]
    fn default_known_hosts_path_falls_back_like_tauri_when_home_is_missing() {
        assert_eq!(
            known_hosts_path_from_home(None),
            PathBuf::from("~/.ssh/known_hosts")
        );
    }

    #[test]
    fn known_hosts_removal_preserves_unmatched_aliases() {
        let path = temp_known_hosts_path("remove");
        let key = sample_public_key();
        fs::write(
            &path,
            format!(
                "example.com,alias.example.com {} {} comment\n",
                public_key_type(&key),
                key.public_key_base64()
            ),
        )
        .unwrap();

        let mut store = KnownHostsStore::with_path(path.clone()).unwrap();
        store
            .remove_host_key(
                "example.com",
                22,
                &public_key_type(&key),
                &public_key_fingerprint(&key),
            )
            .unwrap();

        let rewritten = fs::read_to_string(&path).unwrap();
        assert_eq!(
            rewritten,
            format!(
                "alias.example.com {} {} comment\n",
                public_key_type(&key),
                key.public_key_base64()
            )
        );
        let _ = fs::remove_file(path);
    }
}
